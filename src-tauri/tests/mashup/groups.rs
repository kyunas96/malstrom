use super::*;

#[test]
fn group_track_template_is_an_empty_top_level_group() {
    let doc = roxmltree::Document::parse(GROUP_TRACK_TEMPLATE).unwrap();
    let group = doc.root_element();
    assert_eq!(group.tag_name().name(), "GroupTrack");
    let child = |path: &[&str]| {
        path.iter().try_fold(group, |n, tag| {
            n.children().find(|c| c.tag_name().name() == *tag)
        })
    };
    assert_eq!(
        child(&["TrackGroupId"]).unwrap().attribute("Value"),
        Some("-1")
    );
    for path in [
        &["DeviceChain", "Mixer", "Sends"][..],
        &["DeviceChain", "DeviceChain", "Devices"],
    ] {
        assert_eq!(
            child(path)
                .unwrap()
                .children()
                .filter(|n| n.is_element())
                .count(),
            0
        );
    }
}

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
    vec![MashupSource { xml: a_xml }, MashupSource { xml: b_xml }]
}

fn variation(name: &str, picks: Vec<Vec<usize>>) -> Variation {
    Variation {
        name: name.to_string(),
        picks,
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
fn copying_a_source_three_times_keeps_ids_unique_and_returns_once() {
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
    assert_eq!(sends_of("Drums A"), vec![vec!["0.5", OFF]; 3]);
    assert_eq!(sends_of("Lead B"), vec![vec![OFF, "0.7"]; 3]);

    let mut target_ids: Vec<&str> = doc
        .descendants()
        .filter(|n| matches!(n.tag_name().name(), "AutomationTarget" | "ModulationTarget"))
        .filter_map(|n| n.attribute("Id"))
        .collect();
    let total = target_ids.len();
    target_ids.sort();
    target_ids.dedup();
    assert_eq!(target_ids.len(), total, "two copies share an automation id");
}

#[test]
fn each_variation_is_one_group_holding_its_picked_tracks() {
    let merged = build_mashup(
        &two_sources_with_a_return_each(),
        &[
            variation("Var 1", vec![vec![0], vec![0]]),
            variation("Var 2", vec![vec![0]]),
        ],
    )
    .unwrap();
    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());

    let rows = track_rows(&merged);
    let groups: Vec<&TrackRow> = rows.iter().filter(|r| r.0 == "GroupTrack").collect();
    assert_eq!(groups.len(), 2);
    for (group, name) in groups.iter().zip(["Var 1", "Var 2"]) {
        assert_eq!(group.2, "-1");
        assert_eq!(group.3, name);
        assert_eq!(group.4, "AudioOut/Main");
        assert_eq!(group.5, 2, "one send per return");
    }

    let mut current_group = "";
    let mut members: Vec<(String, &str)> = Vec::new();
    for row in rows.iter().filter(|r| r.0 != "ReturnTrack") {
        if row.0 == "GroupTrack" {
            current_group = &row.1;
            continue;
        }
        assert_eq!(
            row.2, current_group,
            "{} is not in its variation's group",
            row.3
        );
        assert_eq!(row.4, "AudioOut/GroupTrack");
        members.push((row.3.clone(), current_group));
    }
    let names: Vec<&str> = members.iter().map(|m| m.0.as_str()).collect();
    assert_eq!(names, ["Drums A", "Lead B", "Drums A"]);
}

#[test]
fn a_nested_group_is_copied_whole_and_keeps_its_own_grouping() {
    let merged = build_mashup(
        &[MashupSource {
            xml: source_with_nested_groups(),
        }],
        &[
            variation("Var 1", vec![vec![0, 1, 2, 3]]),
            variation("Var 2", vec![vec![3, 2, 1, 0, 4]]),
        ],
    )
    .unwrap();
    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());

    let rows = track_rows(&merged);
    let id_of = |name: &str, nth: usize| -> &str {
        &rows.iter().filter(|r| r.3 == name).nth(nth).unwrap().1
    };
    let group_of = |name: &str, nth: usize| -> &str {
        &rows.iter().filter(|r| r.3 == name).nth(nth).unwrap().2
    };
    for nth in 0..2 {
        assert_eq!(group_of("In Outer", nth), id_of("Outer", nth));
        assert_eq!(group_of("Inner", nth), id_of("Outer", nth));
        assert_eq!(group_of("In Inner", nth), id_of("Inner", nth));
        assert_eq!(group_of("Outer", nth), id_of(["Var 1", "Var 2"][nth], 0));
    }
    let var2: Vec<&str> = rows
        .iter()
        .skip_while(|r| r.3 != "Var 2")
        .skip(1)
        .take_while(|r| r.0 != "ReturnTrack")
        .map(|r| r.3.as_str())
        .collect();
    assert_eq!(
        var2,
        ["Outer", "In Outer", "Inner", "In Inner", "Alone"],
        "a unit's tracks keep their original order"
    );
}

#[test]
fn half_a_group_is_rejected() {
    for picks in [vec![0, 1], vec![1], vec![0, 1, 2]] {
        let err = build_mashup(
            &[MashupSource {
                xml: source_with_nested_groups(),
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
