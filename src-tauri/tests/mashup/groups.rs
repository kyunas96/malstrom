use super::*;

fn two_sources_with_a_return_each() -> Vec<MashupSource> {
    let a_xml = project_xml_with_sends_pre(
        &format!(
            "{}{}",
            track_with_sends("MidiTrack", 1, "Drums A", &["0.5"], 100),
            track_with_sends("ReturnTrack", 2, "A-Rev", &[OFF], 110),
        ),
        1,
    );
    let b_xml = project_xml_with_sends_pre(
        &format!(
            "{}{}",
            track_with_sends("MidiTrack", 1, "Lead B", &["0.7"], 100),
            track_with_sends("ReturnTrack", 2, "B-Rev", &[OFF], 110),
        ),
        1,
    );
    vec![
        MashupSource {
            xml: a_xml,
            semitones: vec![],
        },
        MashupSource {
            xml: b_xml,
            semitones: vec![],
        },
    ]
}

fn variation(name: &str, picks: Vec<Vec<usize>>) -> Variation {
    Variation {
        name: name.to_string(),
        picks,
        ..Default::default()
    }
}

/// `(tag, Id, TrackGroupId, name, output target, send count)` per track in
/// `<Tracks>`, in order.
type TrackRow = (String, String, String, String, String, usize);
fn find<'a, 'input>(
    n: roxmltree::Node<'a, 'input>,
    path: &[&str],
) -> Option<roxmltree::Node<'a, 'input>> {
    path.iter().try_fold(n, |n, tag| {
        n.children().find(|c| c.tag_name().name() == *tag)
    })
}
fn track_rows(merged: &str) -> Vec<TrackRow> {
    let doc = roxmltree::Document::parse(merged).unwrap();
    let tracks = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .unwrap();
    tracks
        .children()
        .filter(|n| n.is_element())
        .map(|t| {
            let value = |path: &[&str]| {
                find(t, path)
                    .and_then(|n| n.attribute("Value"))
                    .unwrap_or("")
                    .to_string()
            };
            (
                t.tag_name().name().to_string(),
                t.attribute("Id").unwrap().to_string(),
                value(&["TrackGroupId"]),
                value(&["Name", "EffectiveName"]),
                value(&["DeviceChain", "AudioOutputRouting", "Target"]),
                find(t, &["DeviceChain", "Mixer", "Sends"])
                    .map_or(0, |s| s.children().filter(|n| n.is_element()).count()),
            )
        })
        .collect()
}

#[test]
fn a_track_picked_by_every_variation_is_in_the_set_once() {
    let merged = build_mashup(
        &two_sources_with_a_return_each(),
        &[
            variation("Var 1", vec![vec![0], vec![0]]),
            variation("Var 2", vec![vec![0], vec![0]]),
            variation("Var 3", vec![vec![0], vec![0]]),
        ],
    )
    .unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    let rows = track_rows(&merged);
    let returns: Vec<&str> = rows
        .iter()
        .filter(|r| r.0 == "ReturnTrack")
        .map(|r| r.3.as_str())
        .collect();
    assert_eq!(returns, ["A-Rev", "B-Rev"]);

    // Every copy still feeds its own project's return.
    let doc = roxmltree::Document::parse(&merged).unwrap();
    let sends_of = |name: &str| -> Vec<Vec<String>> {
        doc.descendants()
            .filter(|n| {
                n.children().any(|c| {
                    c.tag_name().name() == "Name"
                        && c.first_element_child().and_then(|e| e.attribute("Value")) == Some(name)
                })
            })
            .map(|t| {
                child(child(child(t, "DeviceChain"), "Mixer"), "Sends")
                    .children()
                    .filter(|n| n.is_element())
                    .map(|h| {
                        child(child(h, "Send"), "Manual")
                            .attribute("Value")
                            .unwrap()
                            .to_string()
                    })
                    .collect()
            })
            .collect()
    };
    assert_eq!(sends_of("Drums A"), vec![vec!["0.5", OFF]]);
    assert_eq!(sends_of("Lead B"), vec![vec![OFF, "0.7"]]);
    let names: Vec<&str> = rows.iter().map(|r| r.3.as_str()).collect();
    assert_eq!(names, ["Drums A", "Lead B", "A-Rev", "B-Rev"]);

    let mut target_ids: Vec<&str> = doc
        .descendants()
        .filter(|n| matches!(n.tag_name().name(), "AutomationTarget" | "ModulationTarget"))
        .filter_map(|n| n.attribute("Id"))
        .collect();
    let total = target_ids.len();
    target_ids.sort();
    target_ids.dedup();
    assert_eq!(target_ids.len(), total, "two tracks share an automation id");
}

#[test]
fn only_tracks_some_variation_picks_are_in_the_set_and_no_groups_are_added() {
    let merged = build_mashup(
        &two_sources_with_a_return_each(),
        &[
            variation("Var 1", vec![vec![0]]),
            variation("Var 2", vec![vec![0]]),
        ],
    )
    .unwrap();
    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());

    let rows = track_rows(&merged);
    let names: Vec<&str> = rows.iter().map(|r| r.3.as_str()).collect();
    assert_eq!(names, ["Drums A", "A-Rev", "B-Rev"]);
    assert_eq!(rows[0].2, "-1");
    assert_eq!(rows[0].4, "AudioOut/Main");
}

#[test]
fn a_nested_group_is_kept_whole_with_its_own_grouping() {
    let merged = build_mashup(
        &[MashupSource {
            xml: source_with_nested_groups(),
            semitones: vec![],
        }],
        &[
            variation("Var 1", vec![vec![0, 1, 2, 3]]),
            variation("Var 2", vec![vec![3, 2, 1, 0, 4]]),
        ],
    )
    .unwrap();
    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());

    let rows = track_rows(&merged);
    let id_of = |name: &str| -> &str { &rows.iter().find(|r| r.3 == name).unwrap().1 };
    let group_of = |name: &str| -> &str { &rows.iter().find(|r| r.3 == name).unwrap().2 };
    assert_eq!(group_of("Outer"), "-1");
    assert_eq!(group_of("In Outer"), id_of("Outer"));
    assert_eq!(group_of("Inner"), id_of("Outer"));
    assert_eq!(group_of("In Inner"), id_of("Inner"));
    let names: Vec<&str> = rows
        .iter()
        .take_while(|r| r.0 != "ReturnTrack")
        .map(|r| r.3.as_str())
        .collect();
    assert_eq!(
        names,
        ["Outer", "In Outer", "Inner", "In Inner", "Alone"],
        "every track once, in its original order"
    );
}

#[test]
fn half_a_group_is_rejected() {
    for picks in [vec![0, 1], vec![1], vec![0, 1, 2]] {
        let err = build_mashup(
            &[MashupSource {
                xml: source_with_nested_groups(),
                semitones: vec![],
            }],
            &[variation("Var 1", vec![picks.clone()])],
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("groups are picked whole"),
            "{picks:?}: {err}"
        );
    }
}
