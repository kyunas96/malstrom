use super::project_xml;
use super::twist::{attrs, midi_clip, project, rows, scenes, track};
use malstrom_lib::als::categorize::TrackCategory::*;
use malstrom_lib::als::mashup::pipeline::parse_pipeline;
use malstrom_lib::als::mashup::{
    build_mashup_with, diagnostics, plan_variations, MashupSource, Variation,
};
use roxmltree::Document;

/// A MIDI clip named `name`, `length` beats long, holding `notes` as
/// `(time, key)`, each a quarter beat long.
fn notes_clip(name: &str, length: u32, notes: &[(f64, i32)]) -> String {
    let tracks: String = notes
        .iter()
        .enumerate()
        .map(|(i, (time, key))| {
            format!(
                r#"<KeyTrack Id="{i}"><Notes><MidiNoteEvent Time="{time}" Duration="0.25" Velocity="100" NoteId="{}" /></Notes><MidiKey Value="{key}" /></KeyTrack>"#,
                i + 1
            )
        })
        .collect();
    format!(
        r#"<MidiClip Id="1" Time="0"><CurrentStart Value="0" /><CurrentEnd Value="{length}" /><Loop><LoopStart Value="0" /><LoopEnd Value="{length}" /><LoopOn Value="true" /></Loop><Name Value="{name}" /><Color Value="5" /><Notes><KeyTracks>{tracks}</KeyTracks><PerNoteEventStore><EventLists /></PerNoteEventStore><NoteIdGenerator><NextId Value="{}" /></NoteIdGenerator></Notes></MidiClip>"#,
        notes.len() + 1
    )
}

/// One variation of A's `a` and B's `b` through `pipeline`, checked clean.
fn mash(a: &[String], b: &[String], variation: Variation, pipeline: &str) -> String {
    let source = |tracks: &[String]| MashupSource {
        xml: project(&tracks.concat()),
        semitones: vec![],
    };
    let merged = build_mashup_with(
        &[source(a), source(b)],
        &[variation],
        &parse_pipeline(pipeline).unwrap(),
    )
    .unwrap();
    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    merged
}

/// Only B's track 0, paired with A's track 0.
fn partnered() -> Variation {
    Variation {
        name: "Var 2".to_string(),
        picks: vec![vec![], vec![0]],
        partners: vec![(0, 0)],
        ..Default::default()
    }
}

#[test]
fn medley_takes_turns_by_section() {
    let a = [track(
        "MidiTrack",
        1,
        "Keys",
        &[&midi_clip("a1"), &midi_clip("a2")],
    )];
    let b = [track(
        "MidiTrack",
        1,
        "Lead",
        &[&midi_clip("b1"), &midi_clip("b2")],
    )];
    let both = Variation {
        name: "Var 1".to_string(),
        picks: vec![vec![0], vec![0]],
        ..Default::default()
    };

    let merged = mash(&a, &b, both, "medley");

    let names: Vec<String> = scenes(&merged).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        [
            "━━ Var 1 ━━",
            "Var 1 · A1",
            "Var 1 · B1",
            "Var 1 · A2",
            "Var 1 · B2",
            ""
        ]
    );
    assert_eq!(rows(&merged, "Keys"), ["", "a1", "", "a2", "", ""]);
    assert_eq!(rows(&merged, "Lead"), ["", "", "b1", "", "b2", ""]);
}

#[test]
fn cast_plays_the_partner_s_clip_without_its_envelopes() {
    let a = [track("MidiTrack", 1, "Keys", &[&midi_clip("a riff")])];
    let b = [track("MidiTrack", 1, "Lead", &[&midi_clip("b riff")])];

    let merged = mash(&a, &b, partnered(), "layering,cast");

    assert_eq!(rows(&merged, "Lead"), ["", "a riff", ""]);
    assert!(attrs(&merged, "PointeeId", "Value").is_empty());
}

#[test]
fn cutup_alternates_bars_of_b_and_its_partner() {
    let a = [track(
        "MidiTrack",
        1,
        "Keys",
        &[&notes_clip("a", 4, &[(1.0, 48)])],
    )];
    let b = [track(
        "MidiTrack",
        1,
        "Lead",
        &[&notes_clip("b", 4, &[(0.0, 72)])],
    )];

    let merged = mash(&a, &b, partnered(), "layering,cutup");

    // Bar 1 is B's, bar 2 A's; one KeyTrack per key, lowest first.
    assert_eq!(attrs(&merged, "MidiKey", "Value"), ["48", "72"]);
    assert_eq!(attrs(&merged, "MidiNoteEvent", "Time"), ["5", "0"]);
    assert_eq!(attrs(&merged, "MidiNoteEvent", "NoteId"), ["1", "2"]);
    assert_eq!(attrs(&merged, "LoopEnd", "Value"), ["8"]);
    assert_eq!(rows(&merged, "Lead"), ["", "b", ""], "B's clip, recut");
}

#[test]
fn fuse_plays_the_partner_s_rhythm_with_b_s_pitches() {
    let a = [track(
        "MidiTrack",
        1,
        "Keys",
        &[&notes_clip("a", 4, &[(0.0, 48), (2.0, 50)])],
    )];
    let b = [track(
        "MidiTrack",
        1,
        "Lead",
        &[&notes_clip("b", 4, &[(3.0, 76), (1.0, 72)])],
    )];

    let merged = mash(&a, &b, partnered(), "layering,fuse");

    assert_eq!(rows(&merged, "Lead"), ["", "a", ""], "A's clip, re-keyed");
    assert_eq!(attrs(&merged, "MidiKey", "Value"), ["72", "76"]);
    assert_eq!(attrs(&merged, "MidiNoteEvent", "Time"), ["0", "2"]);
}

#[test]
fn a_pipeline_names_known_stages_and_lays_out_first() {
    let err = parse_pipeline("layering,remix").err().unwrap();
    assert!(
        err.to_string().contains("no stage called \"remix\""),
        "{err}"
    );

    let a = [track("MidiTrack", 1, "Keys", &[&midi_clip("a")])];
    let err = build_mashup_with(
        &[MashupSource {
            xml: project(&a.concat()),
            semitones: vec![],
        }],
        &[Variation {
            name: "Var 1".to_string(),
            picks: vec![vec![0]],
            ..Default::default()
        }],
        &parse_pipeline("twist").unwrap(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("layout stage"), "{err}");
}

#[test]
fn later_variations_pair_picked_pitched_b_midi_tracks() {
    let a = project_xml(&track("MidiTrack", 1, "Keys", &[&midi_clip("a")]));
    let b = project_xml(
        &[
            track("MidiTrack", 1, "Pad", &[&midi_clip("b")]),
            track("MidiTrack", 2, "Drums", &[&midi_clip("d")]),
        ]
        .concat(),
    );
    let docs = [Document::parse(&a).unwrap(), Document::parse(&b).unwrap()];
    let plan = plan_variations(&docs, &[vec![Lead], vec![Pads, Drums]], 4, 1, 9).unwrap();

    assert!(plan[0].partners.is_empty(), "Var 1 stays the reference");
    for v in &plan[1..] {
        let expected: Vec<(usize, usize)> = if v.picks[1].contains(&0) {
            vec![(0, 0)]
        } else {
            vec![]
        };
        assert_eq!(v.partners, expected, "drums never pair: {v:?}");
    }
}
