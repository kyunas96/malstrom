use super::project_xml;
use malstrom_lib::als::mashup::{
    build_mashup, key_shift, track_semitones, MashupSource, Variation,
};
use malstrom_lib::als::{ScaleCandidate, ScaleCandidates};
use roxmltree::Document;

fn top(root: &str, scale: &str) -> ScaleCandidates {
    ScaleCandidates {
        scales: vec![ScaleCandidate {
            root_name: root.to_string(),
            scale_name: scale.to_string(),
            common: true,
            score: 1,
            clip_count: 1,
            coverage_percent: 100,
            alternates: vec![],
        }],
    }
}

#[test]
fn key_shift_moves_b_onto_a_s_notes() {
    let c_major = top("C", "Major");
    assert_eq!(key_shift(&c_major, &top("A", "Minor")), 0, "same notes");
    assert_eq!(key_shift(&c_major, &top("D", "Major")), -2);
    assert_eq!(key_shift(&c_major, &top("F#", "Major")), -6, "tie-break");
    assert_eq!(key_shift(&c_major, &ScaleCandidates::default()), 0);
    assert_eq!(key_shift(&ScaleCandidates::default(), &c_major), 0);
}

/// A MIDI track with one clip playing `keys`, labelled with `scale_info`.
fn midi_track(name: &str, keys: &[i32], scale_info: &str) -> String {
    let key_tracks: String = keys
        .iter()
        .map(|k| format!(r#"<KeyTrack Id="0"><Notes><MidiNoteEvent Time="0" Duration="1" Velocity="100" /></Notes><MidiKey Value="{k}" /></KeyTrack>"#))
        .collect();
    format!(
        r#"<MidiTrack Id="1"><Name><EffectiveName Value="{name}" /></Name><TrackGroupId Value="-1" /><MidiClip Id="0"><Notes><KeyTracks>{key_tracks}</KeyTracks></Notes>{scale_info}</MidiClip></MidiTrack>"#
    )
}

fn audio_track(clips: &[(bool, i32)]) -> String {
    let clips: String = clips
        .iter()
        .map(|(warped, pitch)| format!(r#"<AudioClip Id="0"><IsWarped Value="{warped}" /><PitchCoarse Value="{pitch}" /></AudioClip>"#))
        .collect();
    format!(
        r#"<AudioTrack Id="2"><Name><EffectiveName Value="Loop" /></Name><TrackGroupId Value="-1" />{clips}</AudioTrack>"#
    )
}

fn values(xml: &str, tag: &str) -> Vec<String> {
    Document::parse(xml)
        .unwrap()
        .descendants()
        .filter(|n| n.tag_name().name() == tag)
        .map(|n| n.attribute("Value").unwrap().to_string())
        .collect()
}

fn moved(tracks: &str, semitones: Vec<i32>) -> String {
    build_mashup(
        &[MashupSource {
            xml: project_xml(tracks),
            semitones,
        }],
        &[Variation {
            name: "Var 1".to_string(),
            picks: vec![vec![0]],
            ..Default::default()
        }],
    )
    .unwrap()
}

#[test]
fn midi_notes_and_both_scale_label_schemas_move() {
    let live_12 = midi_track(
        "Keys",
        &[60, 64],
        r#"<ScaleInformation><Root Value="11" /><Name Value="0" /></ScaleInformation>"#,
    );
    let merged = moved(&live_12, vec![3]);
    assert_eq!(values(&merged, "MidiKey"), ["63", "67"]);
    assert_eq!(
        values(&merged, "Root"),
        ["2"],
        "root wraps round the octave"
    );

    let live_11 = midi_track(
        "Keys",
        &[60],
        r#"<ScaleInformation><RootNote Value="0" /><Name Value="Major" /></ScaleInformation>"#,
    );
    assert_eq!(values(&moved(&live_11, vec![-2]), "RootNote"), ["10"]);
}

#[test]
fn a_track_near_the_top_falls_back_an_octave() {
    let merged = moved(&midi_track("Keys", &[60, 126], ""), vec![3]);
    assert_eq!(values(&merged, "MidiKey"), ["51", "117"]);
    let merged = moved(&midi_track("Keys", &[1, 60], ""), vec![-3]);
    assert_eq!(values(&merged, "MidiKey"), ["10", "69"]);
}

#[test]
fn only_warped_audio_moves_and_stays_in_range() {
    let merged = moved(&audio_track(&[(true, 0), (false, 0), (true, 47)]), vec![5]);
    assert_eq!(values(&merged, "PitchCoarse"), ["5", "0", "48"]);
}

#[test]
fn a_track_set_to_zero_is_unchanged() {
    let track = format!(
        "{}{}",
        midi_track(
            "Keys",
            &[60],
            r#"<ScaleInformation><Root Value="0" /><Name Value="0" /></ScaleInformation>"#
        ),
        audio_track(&[(true, 2)])
    );
    let picks_both = |semitones| {
        build_mashup(
            &[MashupSource {
                xml: project_xml(&track),
                semitones,
            }],
            &[Variation {
                name: "Var 1".to_string(),
                picks: vec![vec![0, 1]],
                ..Default::default()
            }],
        )
        .unwrap()
    };
    assert_eq!(picks_both(vec![0, 0]), picks_both(vec![]));
}

#[test]
fn drums_and_percussion_stay_put() {
    let c_major = project_xml(&midi_track("Keys", &[60, 62, 64, 65, 67, 69, 71], ""));
    let d_major = project_xml(&format!(
        "{}{}{}",
        midi_track("Drums", &[38], ""),
        midi_track("Perc", &[42], ""),
        midi_track("Keys", &[62, 64, 66, 67, 69, 71, 73], ""),
    ));
    let semitones = track_semitones(
        &Document::parse(&c_major).unwrap(),
        &Document::parse(&d_major).unwrap(),
        &[],
    )
    .unwrap();
    assert_eq!(semitones, [0, 0, -2]);
}
