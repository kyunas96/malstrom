use anyhow::Result;
use rayon::prelude::*;
use roxmltree::{Document, Node};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::scale_constants::{NOTE_NAMES, SCALE_INTERVALS};
use super::scale_names::{child_value, root_note_name, validate_scale_name};

/// Scales presented as "common" by default; everything else is "exotic" and
/// hidden by default in the UI. This is an explicit allowlist by name rather
/// than a position cutoff into SCALE_NAMES/SCALE_INTERVALS, because that
/// array's order is Ableton's own internal scale index -- it's parsed
/// directly from `.als` files (see `scale_names::validate_scale_name`)
/// and can't be reordered or resized without breaking that.
const COMMON_SCALE_NAMES: &[&str] = &[
    "Major",
    "Minor",
    "Dorian",
    "Mixolydian",
    "Lydian",
    "Phrygian",
    "Locrian",
    "Minor Blues",
    "Minor Pentatonic",
    "Major Pentatonic",
];

#[derive(Debug, Clone, Serialize)]
pub struct ScaleAlternate {
    pub root_name: String,
    pub scale_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScaleCandidate {
    pub root_name: String,
    pub scale_name: String,
    /// True for a common Western scale/mode, false for a less common "exotic" one.
    pub common: bool,
    pub score: u32,
    pub clip_count: u32,
    /// Percentage of pitched clips (clips with at least one note) in the
    /// project that this scale matches. More intuitive than the raw score
    /// for judging how strong a match is at a glance.
    pub coverage_percent: u32,
    /// Other (root, scale) pairs whose notes are identical to this one's --
    /// relative modes of the same pitch collection (e.g. C Major / A Minor /
    /// D Dorian) always tie exactly on pitch content alone, so pitch
    /// matching can't distinguish them. We pick the most likely tonic (the
    /// root whose pitch class is used most across the project) as the
    /// top-level candidate and list the rest here.
    pub alternates: Vec<ScaleAlternate>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScaleCandidates {
    pub scales: Vec<ScaleCandidate>,
}

/// Walks the parsed document and, for every `MidiClip`, determines which of
/// the known scales are exactly compatible with the pitch classes used in
/// that clip's notes. Each compatible scale tracks how many distinct clips
/// it matched (`clip_count`) and what percentage of the project's pitched
/// clips that represents (`coverage_percent`), plus a note-count-weighted
/// `score` for finer-grained tie-breaking. Returns candidates sorted by
/// descending coverage (then score), with the top entry being the scale
/// most compatible with the project as a whole -- coverage is the more
/// intuitive "how strong is this match" signal for a non-expert reader.
pub(super) fn extract_from_document(doc: &Document) -> Result<ScaleCandidates> {
    let midi_clip_nodes: Vec<Node> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "MidiClip")
        .collect();

    // Scale-matching is the most expensive step per clip (12 roots x 35
    // scales), so tally clips in parallel and merge each thread's partial
    // tallies together at the end. Also build a project-wide pitch-class
    // histogram (weighted by how "active" the clips using each pitch are)
    // so ties between relative modes can be broken by emphasis later, and
    // count the total number of pitched clips to compute coverage.
    type Tallies = HashMap<(i32, String, String, bool, u16), (u32, u32)>;

    let (tallies, pitch_histogram, total_pitched_clips): (Tallies, [u32; 12], u32) =
        midi_clip_nodes
            .into_par_iter()
            .try_fold(
                || (HashMap::new(), [0u32; 12], 0u32),
                |(mut tallies, mut histogram, mut total_pitched_clips), node| -> Result<_> {
                    let pitch_classes = clip_pitch_classes(&node);
                    if pitch_classes.is_empty() {
                        return Ok((tallies, histogram, total_pitched_clips));
                    }
                    total_pitched_clips += 1;

                    let note_count = clip_note_count(&node);

                    for &pitch_class in &pitch_classes {
                        histogram[pitch_class as usize] += note_count;
                    }

                    for key in matching_scales(&pitch_classes)? {
                        let tally = tallies.entry(key).or_insert((0, 0));
                        tally.0 += note_count;
                        tally.1 += 1;
                    }

                    Ok((tallies, histogram, total_pitched_clips))
                },
            )
            .try_reduce(
                || (HashMap::new(), [0u32; 12], 0u32),
                |(mut a_tallies, mut a_hist, a_total), (b_tallies, b_hist, b_total)| {
                    for (key, (score, clip_count)) in b_tallies {
                        let tally = a_tallies.entry(key).or_insert((0, 0));
                        tally.0 += score;
                        tally.1 += clip_count;
                    }
                    for i in 0..12 {
                        a_hist[i] += b_hist[i];
                    }
                    Ok((a_tallies, a_hist, a_total + b_total))
                },
            )?;

    // Group candidates that describe the exact same notes (relative modes of
    // the same pitch-class collection always tie), then pick the root whose
    // pitch class is most emphasized in the project as the primary label.
    let mut groups: HashMap<u16, Vec<(i32, String, String, bool, u32, u32)>> = HashMap::new();
    for ((root_note, root_name, scale_name, common, pitch_set), (score, clip_count)) in tallies {
        groups
            .entry(pitch_set)
            .or_default()
            .push((root_note, root_name, scale_name, common, score, clip_count));
    }

    let mut scales: Vec<ScaleCandidate> = groups
        .into_values()
        .map(|mut candidates| {
            candidates.sort_by(|a, b| {
                pitch_histogram[b.0 as usize]
                    .cmp(&pitch_histogram[a.0 as usize])
                    .then_with(|| a.1.cmp(&b.1))
            });

            let (_, root_name, scale_name, common, score, clip_count) = candidates[0].clone();
            let alternates = candidates[1..]
                .iter()
                .map(|(_, root_name, scale_name, ..)| ScaleAlternate {
                    root_name: root_name.clone(),
                    scale_name: scale_name.clone(),
                })
                .collect();
            let coverage_percent = if total_pitched_clips == 0 {
                0
            } else {
                ((clip_count as f64 / total_pitched_clips as f64) * 100.0).round() as u32
            };

            ScaleCandidate {
                root_name,
                scale_name,
                common,
                score,
                clip_count,
                coverage_percent,
                alternates,
            }
        })
        .collect();
    scales.sort_by(|a, b| {
        b.coverage_percent
            .cmp(&a.coverage_percent)
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| a.root_name.cmp(&b.root_name))
            .then_with(|| a.scale_name.cmp(&b.scale_name))
    });

    Ok(ScaleCandidates { scales })
}

/// Reads every `KeyTrack`'s `MidiKey` value under a `MidiClip` and reduces
/// them to the distinct pitch classes (0-11) used in that clip.
pub(super) fn clip_pitch_classes(clip_node: &Node) -> HashSet<i32> {
    clip_node
        .descendants()
        .filter(|n| n.tag_name().name() == "KeyTrack")
        .filter_map(|key_track| child_value(&key_track, "MidiKey"))
        .filter_map(|v| v.parse::<i32>().ok())
        .map(|pitch| pitch.rem_euclid(12))
        .collect()
}

/// Counts the total number of individual note events across every `KeyTrack`
/// under a `MidiClip`, used to weight how strongly a clip should influence
/// the project-wide scale recommendation.
fn clip_note_count(clip_node: &Node) -> u32 {
    clip_node
        .descendants()
        .filter(|n| n.tag_name().name() == "MidiNoteEvent")
        .count() as u32
}

/// Finds every (root, scale) pair whose interval set exactly contains the
/// given pitch classes. No ranking or partial-match scoring. Each match also
/// carries the root's pitch class and a bitmask of the scale's absolute
/// pitch classes at that root, so identical-sounding matches (relative
/// modes) can be grouped together later.
fn matching_scales(
    pitch_classes: &HashSet<i32>,
) -> Result<Vec<(i32, String, String, bool, u16)>> {
    let mut matches = Vec::new();

    for root_note in 0..NOTE_NAMES.len() as i32 {
        for (scale_index, intervals) in SCALE_INTERVALS.iter().enumerate() {
            let is_compatible = pitch_classes.iter().all(|&pitch_class| {
                let offset = (pitch_class - root_note).rem_euclid(12) as u8;
                intervals.contains(&offset)
            });

            if !is_compatible {
                continue;
            }

            let root_name = root_note_name(root_note)?;
            let scale_name = validate_scale_name(scale_index as i32)?;
            let common = COMMON_SCALE_NAMES.contains(&scale_name.as_str());
            let pitch_set: u16 = intervals.iter().fold(0u16, |mask, &offset| {
                mask | (1 << ((root_note + offset as i32).rem_euclid(12)))
            });
            matches.push((root_note, root_name, scale_name, common, pitch_set));
        }
    }

    Ok(matches)
}
