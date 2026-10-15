use malstrom_lib::als::mashup::{
    build_mashup, diagnostics, plan_variations, MashupSource, Variation, GROUP_TRACK_TEMPLATE,
};

/// One variation picking `indices` from each source.
fn mash(sources: Vec<(String, Vec<usize>)>) -> anyhow::Result<String> {
    let (xmls, picks): (Vec<_>, Vec<_>) = sources.into_iter().unzip();
    let sources: Vec<MashupSource> = xmls.into_iter().map(|xml| MashupSource { xml }).collect();
    build_mashup(
        &sources,
        &[Variation {
            name: "Var 1".to_string(),
            picks,
        }],
    )
}

fn project_xml(tracks: &str) -> String {
    project_xml_with_next_pointee_id(tracks, 100)
}

fn project_xml_with_next_pointee_id(tracks: &str, next_pointee_id: u64) -> String {
    format!(
        r#"<Ableton MajorVersion="5"><LiveSet><Tempo>120</Tempo><NextPointeeId Value="{next_pointee_id}" /><Tracks>{tracks}</Tracks></LiveSet></Ableton>"#
    )
}

fn midi_track(name: &str) -> String {
    format!(
        r#"<MidiTrack Id="1"><Name><EffectiveName Value="{name}" /></Name><TrackGroupId Value="-1" /></MidiTrack>"#
    )
}

/// A return track with its one send (to itself) turned down, as Live
/// writes it in a set with a single return.
fn return_track(name: &str) -> String {
    track_with_sends("ReturnTrack", 9, name, &[OFF], 900)
}

#[test]
fn merges_selected_tracks_from_both_sources_and_keeps_base_metadata() {
    let a_xml = project_xml(&format!(
        "{}{}{}",
        midi_track("Drums A"),
        midi_track("Bass A"),
        return_track("A-Reverb"),
    ));
    let b_xml = project_xml(&format!("{}{}", midi_track("Lead B"), midi_track("Pads B")));

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![1])]).unwrap();

    assert!(merged.contains("Drums A"));
    assert!(
        !merged.contains("Bass A"),
        "unselected track from source A leaked in"
    );
    assert!(merged.contains("Pads B"));
    assert!(
        !merged.contains("Lead B"),
        "unselected track from source B leaked in"
    );
    assert!(
        merged.contains("A-Reverb"),
        "base project's return track should carry over"
    );
    assert!(
        merged.contains("<Tempo>120</Tempo>"),
        "base project's tempo should carry over"
    );

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
}

#[test]
fn shifts_every_id_in_a_donor_track_s_subtree_not_just_its_own() {
    // Source A (the base) is never touched. Source B's track carries a
    // nested device Id that collides with source A's own -- the exact shape
    // of the real "non-unique Pointee IDs" failure, one level deeper than
    // the top-level track Id.
    let a_xml = project_xml(&midi_track("Drums A"));
    let b_xml = project_xml(
        r#"<MidiTrack Id="1"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><Devices><PluginDevice Id="7" /></Devices></DeviceChain></MidiTrack>"#,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    // Source A's own Ids are the base document's -- never rewritten.
    assert!(
        merged.contains(r#"<MidiTrack Id="1">"#),
        "base source's track Id should be untouched"
    );
    assert!(
        !merged.contains(r#"<PluginDevice Id="7" />"#),
        "source B's nested device Id should have been shifted, not left colliding with source A's id space"
    );
}

#[test]
fn shifted_pointee_references_still_resolve_to_their_shifted_target() {
    // A PointeeId pointing at a device Id within the same (donor) track
    // must move by the same offset as the device it targets, or the
    // reference dangles once the ids are remapped.
    let a_xml = project_xml(&midi_track("Drums A"));
    let b_xml = project_xml(
        r#"<MidiTrack Id="1"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><Devices><PluginDevice Id="7" /></Devices></DeviceChain><AutomationEnvelopes><AutomationEnvelope Id="8"><EnvelopeTarget><PointeeId Value="7" /></EnvelopeTarget></AutomationEnvelope></AutomationEnvelopes></MidiTrack>"#,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    let doc = roxmltree::Document::parse(&merged).unwrap();
    let device_id = doc
        .descendants()
        .find(|n| n.tag_name().name() == "PluginDevice")
        .and_then(|n| n.attribute("Id"))
        .unwrap();
    let pointee_value = doc
        .descendants()
        .find(|n| n.tag_name().name() == "PointeeId")
        .and_then(|n| n.attribute("Value"))
        .unwrap();
    assert_eq!(
        device_id, pointee_value,
        "PointeeId must still point at its shifted target"
    );
}

#[test]
fn raises_next_pointee_id_above_every_id_brought_in_from_either_source() {
    // Base's own counter (100) easily covers its own low Ids, but source B
    // carries a device Id far above it -- exactly what triggered Live's
    // real "NextPointeeId is too low: X must be bigger than Y" error.
    let a_xml = project_xml_with_next_pointee_id(&midi_track("Drums A"), 100);
    let b_xml = project_xml(
        r#"<MidiTrack Id="2"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><Devices><PluginDevice Id="26192" /></Devices></DeviceChain></MidiTrack>"#,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    let doc = roxmltree::Document::parse(&merged).unwrap();
    let next_pointee_id: u64 = doc
        .descendants()
        .find(|n| n.tag_name().name() == "NextPointeeId")
        .and_then(|n| n.attribute("Value"))
        .and_then(|v| v.parse().ok())
        .unwrap();
    assert!(
        next_pointee_id > 26192,
        "NextPointeeId ({next_pointee_id}) must clear every id merged into the document"
    );
}

#[test]
fn drops_donor_groove_references_instead_of_leaving_them_dangling() {
    // Source B's clip points at groove Id 3 in *its own* document's
    // GroovePool -- a resource this merge never copies over. Left as-is,
    // that reference would dangle (or get id-shifted into pointing at some
    // unrelated groove in the base project's pool); Live called the
    // dangling case "GroovePool corrupted". Fixture shape (GrooveSettings/
    // GrooveId Value=...) confirmed against a real project's decompressed
    // XML -- a clip references a groove this way, not via `<Groove Id=..>`.
    let a_xml = project_xml(&midi_track("Drums A"));
    let b_xml = project_xml(
        r#"<MidiTrack Id="1"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value><MidiClip Id="2"><GrooveSettings><GrooveId Value="3" /></GrooveSettings></MidiClip></Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack>"#,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    let doc = roxmltree::Document::parse(&merged).unwrap();
    let groove_id = doc
        .descendants()
        .find(|n| n.tag_name().name() == "GrooveId")
        .and_then(|n| n.attribute("Value"))
        .unwrap();
    assert_eq!(
        groove_id, "-1",
        "donor track's groove reference should be cleared, not shifted"
    );
}

/// Base-project tracks keep whatever groove they had -- it's valid against
/// the base document's own retained GroovePool, never touched by this merge.
#[test]
fn base_project_s_own_groove_references_are_left_alone() {
    let a_xml = project_xml(
        r#"<MidiTrack Id="1"><Name><EffectiveName Value="Drums A" /></Name><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value><MidiClip Id="2"><GrooveSettings><GrooveId Value="3" /></GrooveSettings></MidiClip></Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack>"#,
    );

    let merged = mash(vec![(a_xml, vec![0])]).unwrap();

    assert!(
        merged.contains(r#"<GrooveId Value="3" />"#),
        "base project's own groove reference should survive as-is"
    );
}

#[test]
fn next_pointee_id_does_not_need_to_exceed_its_own_value() {
    // NextPointeeId's tag ends in "Id" like PointeeId does, but it isn't a
    // reference to anything -- it's a forward-looking counter. Treating its
    // own Value as something that must be exceeded is self-referential and
    // can never be satisfied; diagnostics::check must not flag a correctly
    // updated NextPointeeId as still "too low" against itself.
    let a_xml = project_xml_with_next_pointee_id(&midi_track("Drums A"), 100);
    let b_xml = project_xml(
        r#"<MidiTrack Id="2"><Name><EffectiveName Value="Lead B" /></Name></MidiTrack>"#,
    );

    let merged = mash(vec![(a_xml, vec![0]), (b_xml, vec![0])]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
}

#[test]
fn diagnostics_does_not_flag_legitimate_reuse_of_id_outside_the_tracks_list() {
    // Real, valid, untouched Ableton projects reuse "Id" as a local key in
    // many unrelated scopes -- ClipSlot, KeyTrack, AutomationLane, MidiClip
    // and more all commonly repeat small Id values across sibling lists
    // elsewhere in the document. Only the top-level <Tracks> list is known
    // to require unique Ids (Live's own "Non-unique list ids"); a blanket
    // whole-document uniqueness check would flag pristine files as corrupt.
    let a_xml = project_xml(&format!(
        "{}{}",
        midi_track("Drums A"),
        return_track("A-Rev").replace(
            "<Mixer>",
            r#"<ClipSlot Id="42" /><Automation><ClipSlot Id="42" /></Automation><Mixer>"#,
        ),
    ));

    let merged = mash(vec![(a_xml, vec![0])]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
}

#[test]
fn out_of_range_index_errors_instead_of_panicking() {
    let a_xml = project_xml(&midi_track("Only Track"));
    let err = mash(vec![(a_xml, vec![5])]).unwrap_err();
    assert!(err.to_string().contains("out of range"));
}

/// Live 12's value for a send turned all the way down (-70 dB).
const OFF: &str = "0.0003162277571";

fn send_holder(id: usize, value: &str, target_id: usize) -> String {
    format!(
        r#"<TrackSendHolder Id="{id}"><Send><LomId Value="0" /><Manual Value="{value}" /><MidiControllerRange><Min Value="{OFF}" /><Max Value="1" /></MidiControllerRange><AutomationTarget Id="{target_id}"><LockEnvelope Value="0" /></AutomationTarget><ModulationTarget Id="{}"><LockEnvelope Value="0" /></ModulationTarget></Send><EnabledByUser Value="true" /></TrackSendHolder>"#,
        target_id + 1
    )
}

/// A track whose mixer sends hold `values`, one per return in its project.
/// `first_target_id` keeps the automation-target ids distinct per track.
fn track_with_sends(
    tag: &str,
    id: usize,
    name: &str,
    values: &[&str],
    first_target_id: usize,
) -> String {
    let holders: String = values
        .iter()
        .enumerate()
        .map(|(i, v)| send_holder(i, v, first_target_id + i * 2))
        .collect();
    let sends = if holders.is_empty() {
        "<Sends />".to_string()
    } else {
        format!("<Sends>{holders}</Sends>")
    };
    format!(
        r#"<{tag} Id="{id}"><Name><EffectiveName Value="{name}" /></Name><TrackGroupId Value="-1" /><DeviceChain><AudioOutputRouting><Target Value="AudioOut/Main" /><UpperDisplayString Value="Main" /></AudioOutputRouting><Mixer>{sends}</Mixer></DeviceChain></{tag}>"#
    )
}

fn project_xml_with_sends_pre(tracks: &str, return_count: usize) -> String {
    let flags: String = (0..return_count)
        .map(|i| format!(r#"<SendPreBool Id="{i}" Value="true" />"#))
        .collect();
    format!(
        r#"<Ableton MajorVersion="5"><LiveSet><Tempo>120</Tempo><NextPointeeId Value="1000" /><Tracks>{tracks}</Tracks><SendsPre>{flags}</SendsPre></LiveSet></Ableton>"#
    )
}

fn child<'a, 'input>(n: roxmltree::Node<'a, 'input>, tag: &str) -> roxmltree::Node<'a, 'input> {
    n.children().find(|c| c.tag_name().name() == tag).unwrap()
}

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

#[test]
fn diagnostics_flags_a_track_whose_send_count_does_not_match_the_returns() {
    let xml = project_xml_with_sends_pre(
        &format!(
            "{}{}{}",
            track_with_sends("MidiTrack", 1, "Drums A", &["0.5"], 100),
            track_with_sends("ReturnTrack", 2, "A-Rev", &[OFF, OFF], 110),
            track_with_sends("ReturnTrack", 3, "A-Dly", &[OFF, OFF], 120),
        ),
        2,
    );

    let problems = diagnostics::check(&xml);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("has 1 sends but the set has 2 return tracks"));
}

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

/// `track` moved under the group with Id `group`.
fn in_group(track: String, group: usize) -> String {
    track.replacen(
        r#"<TrackGroupId Value="-1" />"#,
        &format!(r#"<TrackGroupId Value="{group}" />"#),
        1,
    )
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

/// Group 5 holds a track and group 7, which holds another track; track 9
/// stands alone.
fn source_with_nested_groups() -> String {
    project_xml_with_sends_pre(
        &[
            track_with_sends("GroupTrack", 5, "Outer", &[OFF], 100),
            in_group(track_with_sends("MidiTrack", 6, "In Outer", &[OFF], 110), 5),
            in_group(track_with_sends("GroupTrack", 7, "Inner", &[OFF], 120), 5),
            in_group(track_with_sends("MidiTrack", 8, "In Inner", &[OFF], 130), 7),
            track_with_sends("MidiTrack", 9, "Alone", &[OFF], 140),
            track_with_sends("ReturnTrack", 2, "Rev", &[OFF], 150),
        ]
        .concat(),
        1,
    )
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

#[test]
fn diagnostics_flags_a_track_grouped_under_a_missing_or_later_group() {
    let later = project_xml(&format!(
        "{}{}",
        in_group(midi_track("Child"), 5),
        midi_track("Group")
            .replace("MidiTrack Id=\"1\"", "GroupTrack Id=\"5\"")
            .replace("</MidiTrack>", "</GroupTrack>"),
    ));
    let missing = project_xml(&in_group(midi_track("Child"), 77));
    for xml in [later, missing] {
        let problems = diagnostics::check(&xml);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("not an earlier GroupTrack"));
    }
}

/// Eight standalone tracks: enough units for seeds to differ.
fn source_with_eight_tracks() -> String {
    project_xml_with_sends_pre(
        &(0..8)
            .map(|i| track_with_sends("MidiTrack", i + 1, &format!("T{i}"), &[], 100 + i * 10))
            .collect::<String>(),
        0,
    )
}

fn plan(n: usize, seed: u64) -> Vec<Variation> {
    let a = source_with_nested_groups();
    let b = source_with_eight_tracks();
    let docs = [
        roxmltree::Document::parse(&a).unwrap(),
        roxmltree::Document::parse(&b).unwrap(),
    ];
    plan_variations(&docs, n, seed).unwrap()
}

#[test]
fn the_same_seed_gives_the_same_plan_and_var_1_is_everything() {
    let first = plan(6, 42);
    assert_eq!(first, plan(6, 42));
    assert_eq!(first.len(), 6);
    let names: Vec<&str> = first.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        names,
        ["Var 1", "Var 2", "Var 3", "Var 4", "Var 5", "Var 6"]
    );
    assert_eq!(first[0].picks, [vec![0, 1, 2, 3, 4], (0..8).collect()]);
}

#[test]
fn later_variations_take_whole_units_from_both_sides() {
    let sources = [
        MashupSource {
            xml: source_with_nested_groups(),
        },
        MashupSource {
            xml: source_with_eight_tracks(),
        },
    ];
    for seed in 0..50 {
        let variations = plan(6, seed);
        for v in &variations[1..] {
            assert!(
                v.picks.iter().all(|p| !p.is_empty()),
                "seed {seed}: {} misses a side",
                v.name
            );
            // The outer group's unit is tracks 0-3: all of them or none.
            let a_group = v.picks[0].iter().filter(|&&i| i < 4).count();
            assert!(
                a_group == 0 || a_group == 4,
                "seed {seed}: {v:?} splits a unit"
            );
        }
        let merged = build_mashup(&sources, &variations).unwrap();
        assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    }
}

#[test]
fn different_seeds_give_different_plans() {
    assert_ne!(plan(6, 1), plan(6, 2));
}

#[test]
fn variations_outside_one_to_six_are_rejected() {
    let a = source_with_nested_groups();
    let docs = [roxmltree::Document::parse(&a).unwrap()];
    for n in [0, 7] {
        assert!(plan_variations(&docs, n, 1).is_err());
    }
}
