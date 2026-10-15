use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};
use std::path::PathBuf;

use super::child_value;
use crate::als::categorize::{list_tracks, TrackCategory};
use crate::als::scale_candidates::{extract_from_document, ScaleCandidates};
use crate::als::scale_constants::{NOTE_NAMES, SCALE_INTERVALS, SCALE_NAMES};
use crate::als::scale_info_schema::ROOT_TAGS;
use crate::als::xml_edit::apply_edits;

/// Bitmask of the pitch classes in `candidates`' top scale, or `None` when
/// the project has no pitched clips.
fn top_pitch_classes(candidates: &ScaleCandidates) -> Option<u16> {
    let top = candidates.scales.first()?;
    let root = NOTE_NAMES.iter().position(|&n| n == top.root_name)?;
    let scale = SCALE_NAMES.iter().position(|&n| n == top.scale_name)?;
    Some(
        SCALE_INTERVALS[scale]
            .iter()
            .fold(0, |mask, &i| mask | 1 << ((root + i as usize) % 12)),
    )
}

/// Semitones to move B by so its top scale overlaps A's the most, from −6 to
/// +5. Ties go to the smallest move, then to moving down, so pairs whose
/// scales share every note don't move. 0 when either has no pitched clips.
pub fn key_shift(a: &ScaleCandidates, b: &ScaleCandidates) -> i32 {
    let (Some(a), Some(b)) = (top_pitch_classes(a), top_pitch_classes(b)) else {
        return 0;
    };
    let overlap = |s: i32| {
        (0..12)
            .filter(|&p| b & 1 << p != 0 && a & 1 << (p + s).rem_euclid(12) != 0)
            .count()
    };
    // 0, −1, +1, −2, +2, …, −6: the first best one wins.
    (0..12)
        .map(|i| if i % 2 == 1 { -(i + 1) / 2 } else { i / 2 })
        .fold((0, overlap(0)), |best, s| {
            let o = overlap(s);
            if o > best.1 {
                (s, o)
            } else {
                best
            }
        })
        .0
}

/// The category of each of `doc`'s `non_return_tracks`, as `list_tracks`
/// finds it: from the Live DB's tags when `live_db_paths` is non-empty,
/// else from track names.
pub fn track_categories(doc: &Document, live_db_paths: &[PathBuf]) -> Result<Vec<TrackCategory>> {
    Ok(list_tracks(doc, live_db_paths)?
        .iter()
        .map(|t| t.category)
        .collect())
}

/// Whether a track of `category` has a pitch to move: anything but drums
/// and percussion.
pub fn is_pitched(category: TrackCategory) -> bool {
    !matches!(category, TrackCategory::Drums | TrackCategory::Percussion)
}

/// Semitones to move each of `b`'s `non_return_tracks` by to bring it into
/// `a`'s key: `key_shift` for pitched tracks (see `is_pitched`), 0 for
/// drums and percussion.
pub fn track_semitones(a: &Document, b: &Document, live_db_paths: &[PathBuf]) -> Result<Vec<i32>> {
    let shift = key_shift(&extract_from_document(a)?, &extract_from_document(b)?);
    Ok(track_categories(b, live_db_paths)?
        .into_iter()
        .map(|c| if is_pitched(c) { shift } else { 0 })
        .collect())
}

fn int_value<'a, 'input>(
    node: Node<'a, 'input>,
    tag: &str,
) -> Option<(std::ops::Range<usize>, i32)> {
    let attr = child_value(node, tag)?;
    Some((attr.range_value(), attr.value().parse().ok()?))
}

/// Moves one track's block by `semitones`: every MIDI note and each MIDI
/// clip's scale root, and the pitch of every warped audio clip (clamped to
/// Live's ±48). If a note would leave 0–127, the whole track moves an
/// octave the other way instead. Unwarped audio is left alone, since
/// repitching it also changes its speed. Devices, automation and MPE aren't
/// touched.
pub(super) fn transpose_track(block: &str, semitones: i32) -> Result<String> {
    if semitones == 0 {
        return Ok(block.to_string());
    }
    let doc = Document::parse(block)
        .map_err(|e| anyhow!("XML parse error while transposing a track: {e}"))?;
    let clips = |tag: &'static str| {
        doc.descendants()
            .filter(move |n| n.tag_name().name() == tag)
    };

    let keys: Vec<_> = clips("MidiClip")
        .flat_map(|c| {
            c.descendants()
                .filter(|n| n.tag_name().name() == "KeyTrack")
        })
        .filter_map(|k| int_value(k, "MidiKey"))
        .collect();
    let (lo, hi) = keys
        .iter()
        .fold((127, 0), |(lo, hi), &(_, v)| (lo.min(v), hi.max(v)));
    let s = if hi + semitones > 127 {
        semitones - 12
    } else if lo + semitones < 0 {
        semitones + 12
    } else {
        semitones
    };

    let mut edits: Vec<_> = keys
        .into_iter()
        .map(|(range, v)| (range, (v + s).clamp(0, 127).to_string()))
        .collect();
    edits.extend(
        clips("MidiClip")
            .flat_map(|c| {
                c.descendants()
                    .filter(|n| n.tag_name().name() == "ScaleInformation")
            })
            .filter_map(|info| ROOT_TAGS.iter().find_map(|tag| int_value(info, tag)))
            .map(|(range, root)| (range, (root + s).rem_euclid(12).to_string())),
    );
    edits.extend(
        clips("AudioClip")
            .filter(|c| child_value(*c, "IsWarped").is_some_and(|w| w.value() == "true"))
            .filter_map(|c| int_value(c, "PitchCoarse"))
            .map(|(range, v)| (range, (v + s).clamp(-48, 48).to_string())),
    );
    Ok(apply_edits(block, edits))
}
