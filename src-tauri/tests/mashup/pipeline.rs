use super::twist::{midi_clip, project, track};
use malstrom_lib::als::mashup::pipeline::parse_pipeline;
use malstrom_lib::als::mashup::{build_mashup_with, MashupSource, Variation};

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
