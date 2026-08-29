use malstrom_lib::als::scale_constants::{SCALE_INTERVALS, SCALE_NAMES};
use malstrom_lib::als::{AlsInspector, ScaleCandidate, ScaleCandidates};
use anyhow::Result;

#[path = "support/mod.rs"]
mod support;

/// Test-only shorthand: wrap a raw (un-gzipped) XML string and extract its
/// scale candidates in one step, mirroring how the fixture-backed test below
/// goes through `AlsInspector::open`.
fn extract_scale_candidates(xml: &str) -> Result<ScaleCandidates> {
    AlsInspector::from_xml(xml.to_string()).extract_scale_candidates()
}

fn midi_clip_xml_with_notes(clips: &[(&str, &[(i32, u32)])]) -> String {
    let clips_xml: String = clips
        .iter()
        .map(|(id, keys)| {
            let key_tracks: String = keys
                .iter()
                .enumerate()
                .map(|(i, (key, note_count))| {
                    let notes: String = (0..*note_count)
                        .map(|n| {
                            format!(
                                r#"<MidiNoteEvent Time="{n}" Duration="1" Velocity="100" OffVelocity="0" NoteId="{n}" />"#
                            )
                        })
                        .collect();

                    format!(
                        r#"<KeyTrack Id="{i}">
                            <Notes>{notes}</Notes>
                            <MidiKey Value="{key}" />
                        </KeyTrack>"#
                    )
                })
                .collect();

            format!(
                r#"<MidiClip Id="{id}" Time="0">
                    <Notes>
                        <KeyTracks>{key_tracks}</KeyTracks>
                    </Notes>
                </MidiClip>"#
            )
        })
        .collect();

    format!(r#"<Ableton>{clips_xml}</Ableton>"#)
}

fn midi_clip_xml(id: &str, midi_keys: &[i32]) -> String {
    let keys: Vec<(i32, u32)> = midi_keys.iter().map(|&k| (k, 1)).collect();
    midi_clip_xml_with_notes(&[(id, &keys)])
}

/// Splits a "{root} {scale name}" test label, e.g. "C Major", into its parts.
fn split_label(label: &str) -> (&str, &str) {
    let (root, scale_name) = label.split_once(' ').expect("label must be \"root name\"");
    (root, scale_name)
}

/// Finds a "{root} {scale name}" match either as a top-level candidate or as
/// one of its alternates -- relative modes of the same pitch collection tie
/// exactly, so only one becomes the primary label, and the rest are still
/// "detected", just folded into that primary's `alternates`.
fn find<'a>(candidates: &'a [ScaleCandidate], label: &str) -> Option<&'a ScaleCandidate> {
    let (root, scale_name) = split_label(label);
    candidates.iter().find(|c| {
        (c.root_name == root && c.scale_name == scale_name)
            || c.alternates
                .iter()
                .any(|a| a.root_name == root && a.scale_name == scale_name)
    })
}

fn has_scale(candidates: &[ScaleCandidate], label: &str) -> bool {
    find(candidates, label).is_some()
}

fn score_of(candidates: &[ScaleCandidate], label: &str) -> Option<u32> {
    find(candidates, label).map(|c| c.score)
}

fn clip_count_of(candidates: &[ScaleCandidate], label: &str) -> Option<u32> {
    find(candidates, label).map(|c| c.clip_count)
}

#[test]
fn partial_note_subset_matches_scale() {
    let xml = midi_clip_xml("1", &[60, 64, 67]); // C, E, G — subset of C Major
    let candidates = extract_scale_candidates(&xml).unwrap();

    assert!(has_scale(&candidates.scales, "C Major"));
}

#[test]
fn out_of_scale_note_removes_match() {
    let xml = midi_clip_xml("1", &[60, 64, 67, 61]); // C, E, G, C#
    let candidates = extract_scale_candidates(&xml).unwrap();

    assert!(!has_scale(&candidates.scales, "C Major"));
}

#[test]
fn clip_with_no_notes_is_skipped() {
    let xml = midi_clip_xml("1", &[]);
    let candidates = extract_scale_candidates(&xml).unwrap();

    assert!(candidates.scales.is_empty());
}

#[test]
fn every_scale_in_scale_names_is_detected() {
    for (name, intervals) in SCALE_NAMES.iter().zip(SCALE_INTERVALS.iter()) {
        let keys: Vec<i32> = intervals.iter().map(|&offset| 60 + offset as i32).collect();
        let xml = midi_clip_xml("1", &keys);
        let candidates = extract_scale_candidates(&xml).unwrap();

        let label = format!("C {name}");
        assert!(
            has_scale(&candidates.scales, &label),
            "expected scale {name} to be detected from its own intervals"
        );
    }
}

#[test]
fn score_reflects_note_count_and_ranks_higher_first() {
    // Clip "1": 3 notes, only compatible with C Major (and supersets like chromatic-ish scales
    // that also contain C/E/G, but we only assert relative ordering against clip "2" here).
    let sparse_clip: Vec<(i32, u32)> = vec![(60, 1), (64, 1), (67, 1)]; // C, E, G — 3 notes total
    // Clip "2": same pitch classes, but many more notes, so it should dominate the score.
    let dense_clip: Vec<(i32, u32)> = vec![(60, 20), (64, 20), (67, 20)]; // C, E, G — 60 notes total

    let xml = midi_clip_xml_with_notes(&[("1", &sparse_clip)]);
    let sparse_candidates = extract_scale_candidates(&xml).unwrap();
    let sparse_score = score_of(&sparse_candidates.scales, "C Major").unwrap();

    let xml = midi_clip_xml_with_notes(&[("2", &dense_clip)]);
    let dense_candidates = extract_scale_candidates(&xml).unwrap();
    let dense_score = score_of(&dense_candidates.scales, "C Major").unwrap();

    assert_eq!(sparse_score, 3);
    assert_eq!(dense_score, 60);
    assert!(dense_score > sparse_score);
}

#[test]
fn scores_accumulate_across_clips_matching_the_same_scale() {
    let clip_a: Vec<(i32, u32)> = vec![(60, 4), (64, 5)]; // C, E — 9 notes
    let clip_b: Vec<(i32, u32)> = vec![(67, 3)]; // G — 3 notes

    let xml = midi_clip_xml_with_notes(&[("1", &clip_a), ("2", &clip_b)]);
    let candidates = extract_scale_candidates(&xml).unwrap();

    assert_eq!(score_of(&candidates.scales, "C Major"), Some(12));
}

#[test]
fn clip_count_reflects_number_of_distinct_matching_clips() {
    let clip_a: Vec<(i32, u32)> = vec![(60, 4), (64, 5)]; // C, E
    let clip_b: Vec<(i32, u32)> = vec![(67, 3)]; // G
    let clip_c: Vec<(i32, u32)> = vec![(61, 2)]; // C# — not compatible with C Major

    let xml = midi_clip_xml_with_notes(&[("1", &clip_a), ("2", &clip_b), ("3", &clip_c)]);
    let candidates = extract_scale_candidates(&xml).unwrap();

    assert_eq!(clip_count_of(&candidates.scales, "C Major"), Some(2));
}

#[test]
fn candidates_are_sorted_descending_by_score() {
    let clip_a: Vec<(i32, u32)> = vec![(60, 1)]; // C only — matches many scales, 1 note
    let clip_b: Vec<(i32, u32)> = vec![(60, 1), (64, 1), (67, 1)]; // C Major triad, 3 notes

    let xml = midi_clip_xml_with_notes(&[("1", &clip_a), ("2", &clip_b)]);
    let candidates = extract_scale_candidates(&xml).unwrap();

    assert!(!candidates.scales.is_empty());
    for window in candidates.scales.windows(2) {
        assert!(window[0].score >= window[1].score);
    }
}

#[test]
fn extract_scale_candidates_from_als_reads_fixture_file() {
    let dir = support::temp_dir("extract-scale-candidates-fixture");
    let path = dir.join("10-30-25.als");
    support::write_als_fixture(&path, support::LIVE_12_STYLE_XML);

    let candidates = AlsInspector::open(&path)
        .unwrap()
        .extract_scale_candidates()
        .unwrap();

    assert!(!candidates.scales.is_empty());
}
