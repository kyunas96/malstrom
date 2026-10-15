use super::project_xml;
use malstrom_lib::als::categorize::TrackCategory::*;
use malstrom_lib::als::mashup::{
    build_mashup, diagnostics, plan_variations, MashupSource, Speed, Twist, Variation,
};
use roxmltree::Document;

/// A clip slot holding `clip`, or nothing when it's empty.
pub(crate) fn slot(clip: &str) -> String {
    let value = if clip.is_empty() {
        "<Value />".to_string()
    } else {
        format!("<Value>{clip}</Value>")
    };
    format!(r#"<ClipSlot Id="0"><ClipSlot>{value}</ClipSlot><HasStop Value="true" /></ClipSlot>"#)
}

/// A MIDI clip named `name`: 4 beats long, one note at beat 1 lasting half
/// a beat, and an envelope pointing at automation target 7.
pub(crate) fn midi_clip(name: &str) -> String {
    format!(
        r#"<MidiClip Id="1" Time="0"><CurrentStart Value="0" /><CurrentEnd Value="4" /><Loop><LoopStart Value="0" /><LoopEnd Value="4" /><LoopOn Value="true" /></Loop><Name Value="{name}" /><Color Value="5" /><Envelopes><Envelopes><ClipEnvelope Id="0"><EnvelopeTarget><PointeeId Value="7" /></EnvelopeTarget></ClipEnvelope></Envelopes></Envelopes><Notes><KeyTracks><KeyTrack Id="0"><Notes><MidiNoteEvent Time="1" Duration="0.5" Velocity="100" /></Notes><MidiKey Value="60" /></KeyTrack></KeyTracks></Notes></MidiClip>"#
    )
}

/// A track whose Session slots hold `clips`, one per scene row.
pub(crate) fn track(tag: &str, id: usize, name: &str, clips: &[&str]) -> String {
    let slots: String = clips.iter().map(|c| slot(c)).collect();
    format!(
        r#"<{tag} Id="{id}"><Name><EffectiveName Value="{name}" /></Name><TrackGroupId Value="-1" /><DeviceChain><MainSequencer><ClipSlotList>{slots}</ClipSlotList></MainSequencer></DeviceChain></{tag}>"#
    )
}

/// A project holding `tracks`, with one scene for the layout to copy.
pub(crate) fn project(tracks: &str) -> String {
    project_xml(tracks).replace(
        "</LiveSet>",
        r#"<Scenes><Scene Id="0"><Name Value="" /><Color Value="-1" /><Tempo Value="120" /></Scene></Scenes></LiveSet>"#,
    )
}

fn var(n: usize, picks: Vec<Vec<usize>>, twist: Twist) -> Variation {
    Variation {
        name: format!("Var {n}"),
        picks,
        twist,
    }
}

fn mash(a: &[String], b: &[String], variations: &[Variation]) -> String {
    let merged = build_mashup(
        &[
            MashupSource {
                xml: project(&a.concat()),
                semitones: vec![],
            },
            MashupSource {
                xml: project(&b.concat()),
                semitones: vec![],
            },
        ],
        variations,
    )
    .unwrap();
    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    merged
}

/// The clip names in each Session slot of the merged track named `name`
/// ("" for an empty slot).
pub(crate) fn rows(xml: &str, name: &str) -> Vec<String> {
    let doc = Document::parse(xml).unwrap();
    let track = doc
        .descendants()
        .find(|n| {
            n.children().any(|c| {
                c.tag_name().name() == "Name"
                    && c.first_child().and_then(|e| e.attribute("Value")) == Some(name)
            })
        })
        .unwrap();
    track
        .descendants()
        .filter(|n| n.tag_name().name() == "ClipSlotList")
        .flat_map(|l| l.children().filter(|c| c.tag_name().name() == "ClipSlot"))
        .map(|s| {
            s.descendants()
                .find(|n| n.tag_name().name() == "Name" && n.attribute("Value").is_some())
                .map_or(String::new(), |n| n.attribute("Value").unwrap().to_string())
        })
        .collect()
}

pub(crate) fn attrs(xml: &str, tag: &str, attr: &str) -> Vec<String> {
    Document::parse(xml)
        .unwrap()
        .descendants()
        .filter(|n| n.tag_name().name() == tag)
        .filter_map(|n| n.attribute(attr).map(str::to_string))
        .collect()
}

/// `(name, colour)` of every scene.
pub(crate) fn scenes(xml: &str) -> Vec<(String, String)> {
    let doc = Document::parse(xml).unwrap();
    doc.descendants()
        .filter(|n| n.tag_name().name() == "Scene")
        .map(|s| {
            let v = |tag: &str| {
                s.children()
                    .find(|c| c.tag_name().name() == tag)
                    .and_then(|c| c.attribute("Value"))
                    .unwrap()
                    .to_string()
            };
            (v("Name"), v("Color"))
        })
        .collect()
}

#[test]
fn each_variation_is_a_block_of_whole_scenes() {
    // A's sections are rows 0 and 2, B's rows 1 and 3. "Pad" only has a
    // clip in row 0, so it holds it through every section.
    let a = [
        track(
            "MidiTrack",
            1,
            "Keys",
            &[&midi_clip("a0"), "", &midi_clip("a2"), ""],
        ),
        track("MidiTrack", 2, "Pad", &[&midi_clip("pad"), "", "", ""]),
    ];
    let b = [track(
        "MidiTrack",
        1,
        "Lead",
        &["", &midi_clip("b1"), "", &midi_clip("b3")],
    )];
    let shifted = Twist {
        scene_shift: 1,
        ..Default::default()
    };

    let merged = mash(
        &a,
        &b,
        &[
            var(1, vec![vec![0, 1], vec![0]], Twist::default()),
            var(2, vec![vec![0], vec![0]], shifted),
        ],
    );

    let names: Vec<String> = scenes(&merged).into_iter().map(|s| s.0).collect();
    assert_eq!(
        names,
        [
            "━━ Var 1 ━━",
            "Var 1 · A1 × B2",
            "Var 1 · A3 × B4",
            "",
            "━━ Var 2 ━━",
            "Var 2 · A1 × B4",
            "Var 2 · A3 × B2",
            ""
        ]
    );
    assert_eq!(
        rows(&merged, "Keys"),
        ["", "a0", "a2", "", "", "a0", "a2", ""]
    );
    assert_eq!(
        rows(&merged, "Pad"),
        ["", "pad", "pad", "", "", "", "", ""],
        "Var 2 leaves Pad out"
    );
    assert_eq!(
        rows(&merged, "Lead"),
        ["", "b1", "b3", "", "", "b3", "b1", ""]
    );
}

#[test]
fn a_variation_s_scenes_and_clips_share_its_colour() {
    let a = [track("MidiTrack", 1, "Keys", &[&midi_clip("a0")])];
    let b = [track("MidiTrack", 1, "Lead", &[&midi_clip("b0")])];
    let merged = mash(
        &a,
        &b,
        &[
            var(1, vec![vec![0], vec![0]], Twist::default()),
            var(2, vec![vec![0], vec![0]], Twist::default()),
        ],
    );

    let colors: Vec<String> = scenes(&merged).into_iter().map(|s| s.1).collect();
    let (one, two) = (&colors[0], &colors[3]);
    assert_ne!(one, two);
    assert_eq!(colors, [one, one, "-1", two, two, "-1"]);
    assert_eq!(
        attrs(&merged, "Color", "Value")
            .iter()
            .filter(|c| *c == one)
            .count(),
        2 + 2,
        "two scenes, two clips"
    );
}

#[test]
fn half_time_stretches_b_s_clips_in_its_variation() {
    let a = [track("MidiTrack", 1, "Keys", &[&midi_clip("a0"), ""])];
    let audio = |warped: bool| {
        format!(
            r#"<AudioClip Id="0" Time="0"><CurrentEnd Value="8" /><IsWarped Value="{warped}" /><WarpMarkers><WarpMarker Id="0" SecTime="0" BeatTime="0" /><WarpMarker Id="1" SecTime="4" BeatTime="8" /></WarpMarkers></AudioClip>"#
        )
    };
    let b = [
        track("MidiTrack", 1, "Lead", &[&midi_clip("b0"), ""]),
        track("AudioTrack", 2, "Loop", &[&audio(true), &audio(false)]),
    ];
    let half = Twist {
        speed: Speed::Half,
        ..Default::default()
    };

    let merged = mash(&a, &b, &[var(2, vec![vec![0], vec![0, 1]], half)]);

    // Two scenes (B has two sections): A's clip twice, then B's, stretched.
    assert_eq!(attrs(&merged, "LoopEnd", "Value"), ["4", "4", "8", "8"]);
    assert_eq!(
        attrs(&merged, "MidiNoteEvent", "Time"),
        ["1", "1", "2", "2"]
    );
    assert_eq!(
        attrs(&merged, "MidiNoteEvent", "Duration"),
        ["0.5", "0.5", "1", "1"]
    );
    assert_eq!(
        attrs(&merged, "WarpMarker", "BeatTime"),
        ["0", "16", "0", "8"],
        "warped audio stretches, unwarped doesn't"
    );
}

#[test]
fn swap_plays_a_s_clip_on_b_s_track_without_a_s_envelopes() {
    let a = [track("MidiTrack", 1, "Keys", &[&midi_clip("a riff")])];
    let b = [track("MidiTrack", 1, "Lead", &[&midi_clip("b riff")])];
    let twist = Twist {
        speed: Speed::Double,
        swap: Some((0, 0)),
        ..Default::default()
    };

    let merged = mash(&a, &b, &[var(2, vec![vec![0], vec![0]], twist)]);

    assert_eq!(rows(&merged, "Lead"), ["", "a riff", ""]);
    // Only A's own clip still points at its envelope target.
    assert_eq!(attrs(&merged, "PointeeId", "Value"), ["7"]);
    assert_eq!(
        attrs(&merged, "LoopEnd", "Value"),
        ["4", "4"],
        "swapped clips keep A's speed"
    );
}

#[test]
fn heat_adds_strategies_and_leaves_var_1_alone() {
    let a = project_xml(&track("MidiTrack", 1, "Keys", &[&midi_clip("a")]));
    let b = project_xml(
        &[
            track("MidiTrack", 1, "Pad", &[&midi_clip("b"), &midi_clip("b2")]),
            track("MidiTrack", 2, "Drums", &[&midi_clip("d"), ""]),
        ]
        .concat(),
    );
    let docs = [Document::parse(&a).unwrap(), Document::parse(&b).unwrap()];
    let categories = [vec![Lead], vec![Pads, Drums]];
    let plan = |heat| plan_variations(&docs, &categories, 4, heat, 9).unwrap();

    let mild = plan(1);
    assert!(mild.iter().all(|v| v.twist == Twist::default()));
    assert_eq!(mild[1].name, "Var 2");

    for v in &plan(5)[1..] {
        let t = &v.twist;
        assert_eq!(
            t.scene_shift, 1,
            "B has two sections, so the only real shift is 1"
        );
        assert!([12, -12, 7, -5].contains(&t.semitones[0]), "{v:?}");
        assert_eq!(t.semitones[1], 0, "drums never move");
        assert_ne!(t.speed, Speed::Same);
        if v.picks[1].contains(&0) {
            assert_eq!(t.swap, Some((0, 0)), "{v:?}");
        } else {
            assert_eq!(t.swap, None, "only a picked, pitched B track swaps");
        }
        assert!(v.name.contains(" · scenes+"), "{}", v.name);
    }
    let hot = plan(5);
    assert_eq!(hot[0].twist, Twist::default());
    assert_eq!(hot[0].picks, mild[0].picks);
}

#[test]
fn later_variations_never_stack_two_drum_parts_or_two_basslines() {
    let tracks = |names: [&str; 3]| {
        project_xml(
            &names
                .iter()
                .enumerate()
                .map(|(i, n)| track("MidiTrack", i + 1, n, &[]))
                .collect::<String>(),
        )
    };
    let (a, b) = (
        tracks(["Kit", "Sub", "Keys"]),
        tracks(["Break", "Reese", "Pad"]),
    );
    let docs = [Document::parse(&a).unwrap(), Document::parse(&b).unwrap()];
    let categories = [vec![Drums, Bass, Lead], vec![Percussion, Bass, Pads]];

    let mut drum_sides = [false; 2];
    for seed in 0..50 {
        let plan = plan_variations(&docs, &categories, 6, 1, seed).unwrap();
        assert_eq!(
            plan[0].picks,
            [vec![0, 1, 2], vec![0, 1, 2]],
            "Var 1 is everything"
        );
        for v in &plan[1..] {
            let [a, b] = [&v.picks[0], &v.picks[1]];
            assert!(!a.is_empty() && !b.is_empty(), "seed {seed}: {v:?}");
            assert!(
                !(a.contains(&0) && b.contains(&0)),
                "seed {seed}: two drum parts in {v:?}"
            );
            assert!(
                !(a.contains(&1) && b.contains(&1)),
                "seed {seed}: two basslines in {v:?}"
            );
            drum_sides[0] |= a.contains(&0);
            drum_sides[1] |= b.contains(&0);
        }
    }
    assert_eq!(drum_sides, [true, true], "either side's drums can play");
}
