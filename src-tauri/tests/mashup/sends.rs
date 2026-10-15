use super::*;

/// The `Manual` value of each of a track's own mixer sends, in order.
fn send_values(merged: &str, track_name: &str) -> Vec<String> {
    let doc = roxmltree::Document::parse(merged).unwrap();
    let track = doc
        .descendants()
        .find(|n| {
            n.children().any(|c| {
                c.tag_name().name() == "Name"
                    && c.first_element_child().and_then(|e| e.attribute("Value"))
                        == Some(track_name)
            })
        })
        .unwrap_or_else(|| panic!("no track named {track_name}"));
    let sends = child(child(child(track, "DeviceChain"), "Mixer"), "Sends");
    sends
        .children()
        .filter(|n| n.is_element())
        .map(|holder| {
            child(child(holder, "Send"), "Manual")
                .attribute("Value")
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn donor_returns_are_appended_and_every_track_s_sends_still_reach_its_own_returns() {
    // A has 2 returns, B has 3 -> the merged set has 5: A's then B's. A's
    // sends must stay in slots 1-2 with slots 3-5 off; B's must move to
    // slots 3-5 with slots 1-2 off, so B still feeds its own reverb.
    let a_xml = project_xml_with_sends_pre(
        &format!(
            "{}{}{}",
            track_with_sends("MidiTrack", 1, "Drums A", &["0.5", "0.6"], 100),
            track_with_sends("ReturnTrack", 2, "A-Rev", &[OFF, "0.2"], 110),
            track_with_sends("ReturnTrack", 3, "A-Dly", &[OFF, OFF], 120),
        ),
        2,
    );
    let b_xml = project_xml_with_sends_pre(
        &format!(
            "{}{}{}{}",
            track_with_sends("MidiTrack", 1, "Lead B", &["0.7", "0.8", "0.9"], 100),
            track_with_sends("ReturnTrack", 2, "B-Rev", &[OFF, OFF, OFF], 110),
            track_with_sends("ReturnTrack", 3, "B-Dly", &["0.3", OFF, OFF], 120),
            track_with_sends("ReturnTrack", 4, "B-Echo", &[OFF, OFF, OFF], 130),
        ),
        3,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());

    let doc = roxmltree::Document::parse(&merged).unwrap();
    let return_names: Vec<&str> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "ReturnTrack")
        .filter_map(|n| {
            n.descendants()
                .find(|d| d.tag_name().name() == "EffectiveName")
        })
        .filter_map(|n| n.attribute("Value"))
        .collect();
    assert_eq!(return_names, ["A-Rev", "A-Dly", "B-Rev", "B-Dly", "B-Echo"]);

    assert_eq!(
        send_values(&merged, "Drums A"),
        ["0.5", "0.6", OFF, OFF, OFF]
    );
    assert_eq!(send_values(&merged, "A-Rev"), [OFF, "0.2", OFF, OFF, OFF]);
    assert_eq!(
        send_values(&merged, "Lead B"),
        [OFF, OFF, "0.7", "0.8", "0.9"]
    );
    assert_eq!(send_values(&merged, "B-Dly"), [OFF, OFF, "0.3", OFF, OFF]);

    // Padding holders bring new automation targets; none may reuse an id.
    let mut target_ids: Vec<&str> = doc
        .descendants()
        .filter(|n| matches!(n.tag_name().name(), "AutomationTarget" | "ModulationTarget"))
        .filter_map(|n| n.attribute("Id"))
        .collect();
    let total = target_ids.len();
    target_ids.sort();
    target_ids.dedup();
    assert_eq!(
        target_ids.len(),
        total,
        "padding sends reused an existing automation/modulation id"
    );
}

#[test]
fn base_with_no_returns_gets_its_empty_sends_list_filled_in() {
    // A project with no returns stores a self-closing `<Sends />`.
    let a_xml =
        project_xml_with_sends_pre(&track_with_sends("MidiTrack", 1, "Drums A", &[], 100), 0);
    let b_xml = project_xml_with_sends_pre(
        &format!(
            "{}{}",
            track_with_sends("MidiTrack", 1, "Lead B", &["0.7"], 100),
            track_with_sends("ReturnTrack", 2, "B-Rev", &[OFF], 110),
        ),
        1,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    assert_eq!(send_values(&merged, "Drums A"), [OFF]);
    assert_eq!(send_values(&merged, "Lead B"), ["0.7"]);
}

#[test]
fn sends_inside_rack_chains_are_not_padded() {
    // A rack chain has its own Mixer/Sends for the rack's internal return
    // chains -- unrelated to the set's return tracks, so it must be left
    // exactly as it was.
    let rack_sends = format!("<Sends>{}</Sends>", send_holder(0, "0.4", 500));
    let a_xml = project_xml_with_sends_pre(
        &format!(
            "{}{}",
            track_with_sends("MidiTrack", 1, "Drums A", &[OFF], 100),
            track_with_sends("ReturnTrack", 2, "A-Rev", &[OFF], 110),
        ),
        1,
    );
    let b_track = track_with_sends("MidiTrack", 1, "Lead B", &["0.7"], 100).replace(
        "<Mixer>",
        &format!(r#"<Devices><InstrumentGroupDevice Id="40"><Branches><Chain><DeviceChain><Mixer>{rack_sends}</Mixer></DeviceChain></Chain></Branches></InstrumentGroupDevice></Devices><Mixer>"#),
    );
    let b_xml = project_xml_with_sends_pre(
        &format!(
            "{}{}",
            b_track,
            track_with_sends("ReturnTrack", 2, "B-Rev", &[OFF], 110)
        ),
        1,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    assert_eq!(send_values(&merged, "Lead B"), [OFF, "0.7"]);
    let doc = roxmltree::Document::parse(&merged).unwrap();
    let rack_holders = doc
        .descendants()
        .find(|n| n.tag_name().name() == "InstrumentGroupDevice")
        .unwrap()
        .descendants()
        .filter(|n| n.tag_name().name() == "TrackSendHolder")
        .count();
    assert_eq!(
        rack_holders, 1,
        "rack chain's own sends should be untouched"
    );
}
