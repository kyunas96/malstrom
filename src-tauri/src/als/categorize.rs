use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::livedb;

/// Coarse track grouping derived from a track's name or, when available,
/// its resolved Live Database tags. `Other` is a first-class outcome, not
/// an error state -- not every track is expected to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackCategory {
    Drums,
    Bass,
    Percussion,
    Vocals,
    Lead,
    Pads,
    Fx,
    Other,
}

/// Fixed priority order: earlier categories are checked first, so e.g. a
/// track named "Bass Drums" lands in Drums (a full drum bus is more likely
/// than a literal instrument named that) rather than flip-flopping on
/// keyword position. Shared by both name matching (substring) and Live
/// Database tag matching (exact) -- the two sources agree closely enough
/// that one table with the union of their keywords covers both.
const CATEGORY_KEYWORDS: &[(TrackCategory, &[&str])] = &[
    (
        TrackCategory::Drums,
        &["drum", "drums", "kit", "kick", "drum loop", "snare", "hihat", "hi-hat"],
    ),
    (TrackCategory::Bass, &["bass", "sub"]),
    (TrackCategory::Percussion, &["perc", "percussion"]),
    (TrackCategory::Vocals, &["vox", "vocal", "vocals", "solo voice"]),
    (TrackCategory::Lead, &["lead", "melody"]),
    (TrackCategory::Pads, &["pad", "pads", "atmos"]),
    (TrackCategory::Fx, &["fx", "sfx", "riser"]),
];

/// Fallback categorization: lowercases `track_name` once and checks each
/// category's keyword list for a substring match, in the fixed priority
/// order above. Pure function, no I/O.
pub fn categorize_by_name(track_name: &str) -> TrackCategory {
    let lower = track_name.to_lowercase();
    for (category, keywords) in CATEGORY_KEYWORDS {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return *category;
        }
    }
    TrackCategory::Other
}

/// Maps resolved Live Database tags to a category via the same keyword
/// table as `categorize_by_name`, matched by exact tag equality rather than
/// substring. `None` means no tag matched, so the caller should fall
/// through to `categorize_by_name`. Pure function, no I/O.
pub fn categorize_by_db_tags(tags: &[String]) -> Option<TrackCategory> {
    let lower: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
    for (category, keywords) in CATEGORY_KEYWORDS {
        if keywords.iter().any(|kw| lower.iter().any(|t| t == kw)) {
            return Some(*category);
        }
    }
    None
}

/// One track's name, kind ("Midi", "Audio", "Group", ...), and derived
/// category, as returned to the frontend. Return tracks are excluded by
/// `list_tracks` -- they're effects buses, not instrument/audio content to
/// categorize.
#[derive(Debug, Clone, Serialize)]
pub struct TrackSummary {
    pub name: String,
    pub kind: String,
    pub category: TrackCategory,
}

fn effective_name(track: &Node) -> String {
    track
        .children()
        .find(|c| c.tag_name().name() == "Name")
        .and_then(|name_node| {
            name_node
                .children()
                .find(|c| c.tag_name().name() == "EffectiveName")
        })
        .and_then(|n| n.attribute("Value"))
        .unwrap_or("")
        .to_string()
}

fn track_kind(track: &Node) -> String {
    track
        .tag_name()
        .name()
        .trim_end_matches("Track")
        .to_string()
}

/// Filenames of every sample this track references, resolved from the
/// track's own `SampleRef` > `FileRef` > `Path` nodes (the file Live
/// actually plays, not any `SourceContext`-nested original). Used for audio
/// tracks, where the clip's own sample *is* the content -- MIDI tracks go
/// through `instrument_tag_filename` instead, since a MIDI track's device
/// chain can include effects with their own unrelated `SampleRef`s that
/// would otherwise dilute the tag lookup.
fn sample_filenames(track: &Node) -> Vec<String> {
    track
        .descendants()
        .filter(|n| n.tag_name().name() == "SampleRef")
        .filter_map(|sample_ref| {
            sample_ref
                .children()
                .find(|c| c.tag_name().name() == "FileRef")
        })
        .filter_map(|file_ref| file_ref.children().find(|c| c.tag_name().name() == "Path"))
        .filter_map(|path_node| path_node.attribute("Value"))
        .filter_map(|path| Path::new(path).file_name())
        .filter_map(|name| name.to_str())
        .map(|s| s.to_string())
        .collect()
}

/// Live's built-in MIDI effect devices -- these can sit before the
/// instrument in a MIDI track's device chain (Live only allows MIDI
/// effects before the one instrument, audio effects after), so they must be
/// skipped when looking for "the instrument".
const MIDI_EFFECT_DEVICE_TAGS: &[&str] = &[
    "MidiArpeggiator",
    "MidiChord",
    "MidiScale",
    "MidiPitcher",
    "MidiRandom",
    "MidiVelocity",
    "MidiNoteLength",
    "MidiEffectGroupDevice",
];

/// Finds the one instrument device in a MIDI track's chain: the first
/// top-level device under `<Devices>` that isn't a known MIDI effect.
fn instrument_device<'a, 'input>(track: &Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    let devices = track
        .descendants()
        .find(|n| n.tag_name().name() == "Devices")?;
    devices
        .children()
        .filter(|n| n.is_element())
        .find(|n| !MIDI_EFFECT_DEVICE_TAGS.contains(&n.tag_name().name()))
}

/// Resolves the filename that represents an instrument device's own
/// identity for a DB tag lookup -- not every sample nested inside it.
/// `Simpler` wraps exactly one sample, so that sample *is* its identity;
/// anything else (Drum Rack, full Sampler, Wavetable, ...) is identified by
/// its own saved preset file (`LastPresetRef` > `FilePresetRef`), not any
/// nested branch's samples. A factory-default device (`AbletonDefaultPresetRef`,
/// not a real saved/tagged file) resolves to `None`, so the caller falls
/// back to name matching.
fn instrument_tag_filename(device: &Node) -> Option<String> {
    let source = if device.tag_name().name() == "OriginalSimpler" {
        device.descendants().find(|n| n.tag_name().name() == "SampleRef")
    } else {
        device
            .children()
            .find(|n| n.tag_name().name() == "LastPresetRef")
            .and_then(|last_preset| {
                last_preset
                    .descendants()
                    .find(|n| n.tag_name().name() == "FilePresetRef")
            })
    }?;

    source
        .descendants()
        .find(|n| n.tag_name().name() == "FileRef")
        .and_then(|file_ref| file_ref.children().find(|c| c.tag_name().name() == "Path"))
        .and_then(|path_node| path_node.attribute("Value"))
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
}

/// Checks a per-track override (see docs/track-category-overrides-spec.md)
/// before falling through to DB-tag/name matching. `overrides` is keyed
/// `"<project_path>::<track_name>"`, the whole `trackCategoryOverrides`
/// overlay namespace loaded once by the caller.
fn categorize_track(
    track: &Node,
    live_db_paths: &[PathBuf],
    project_path: &str,
    overrides: &HashMap<String, TrackCategory>,
) -> TrackCategory {
    let name = effective_name(track);

    if let Some(category) = overrides.get(&format!("{project_path}::{name}")) {
        return *category;
    }

    if !live_db_paths.is_empty() {
        let filenames: Vec<String> = if track.tag_name().name() == "MidiTrack" {
            instrument_device(track)
                .and_then(|d| instrument_tag_filename(&d))
                .into_iter()
                .collect()
        } else {
            sample_filenames(track)
        };
        for filename in &filenames {
            for db_path in live_db_paths {
                if let Ok(tags) = livedb::lookup_tags(db_path, filename) {
                    if let Some(category) = categorize_by_db_tags(&tags) {
                        return category;
                    }
                }
            }
        }
    }

    categorize_by_name(&name)
}

/// Walks every top-level track in the document, resolving a category for
/// each. `live_db_paths` lists every `Live-files-*.db` to try, in order;
/// when empty (or when every lookup errors, e.g. an invalid path), every
/// track falls back to `categorize_by_name`. `overrides` short-circuits
/// both of those for any track with a matching entry -- see
/// `categorize_track`.
pub fn list_tracks(
    doc: &Document,
    live_db_paths: &[PathBuf],
    project_path: &str,
    overrides: &HashMap<String, TrackCategory>,
) -> Result<Vec<TrackSummary>> {
    let tracks_node = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .ok_or_else(|| anyhow!("no <Tracks> element found in project"))?;

    Ok(tracks_node
        .children()
        .filter(|n| n.is_element())
        .filter(|n| n.tag_name().name() != "ReturnTrack")
        .map(|track| TrackSummary {
            name: effective_name(&track),
            kind: track_kind(&track),
            category: categorize_track(&track, live_db_paths, project_path, overrides),
        })
        .collect())
}
