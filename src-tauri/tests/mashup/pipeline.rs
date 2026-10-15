use super::twist::{midi_clip, project, rows, scenes, track};
use malstrom_lib::als::mashup::pipeline::parse_pipeline;
use malstrom_lib::als::mashup::{build_mashup_with, diagnostics, MashupSource, Variation};

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
    let source = |tracks: &[String]| MashupSource {
        xml: project(&tracks.concat()),
        semitones: vec![],
    };
    let merged = build_mashup_with(
        &[source(&a), source(&b)],
        &[Variation {
            name: "Var 1".to_string(),
            picks: vec![vec![0], vec![0]],
            ..Default::default()
        }],
        &parse_pipeline("medley").unwrap(),
    )
    .unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
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
