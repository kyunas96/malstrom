use malstrom_lib::als::mashup::{build_mashup, diagnostics, MashupSource};

fn project_xml(tracks: &str) -> String {
    project_xml_with_next_pointee_id(tracks, 100)
}

fn project_xml_with_next_pointee_id(tracks: &str, next_pointee_id: u64) -> String {
    format!(
        r#"<Ableton MajorVersion="5"><LiveSet><Tempo>120</Tempo><NextPointeeId Value="{next_pointee_id}" /><Tracks>{tracks}</Tracks></LiveSet></Ableton>"#
    )
}

fn midi_track(name: &str) -> String {
    format!(r#"<MidiTrack Id="1"><Name><EffectiveName Value="{name}" /></Name></MidiTrack>"#)
}

fn return_track(name: &str) -> String {
    format!(r#"<ReturnTrack Id="9"><Name><EffectiveName Value="{name}" /></Name></ReturnTrack>"#)
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

    let merged = build_mashup(&[
        MashupSource { xml: a_xml, track_indices: vec![0] },
        MashupSource { xml: b_xml, track_indices: vec![1] },
    ])
    .unwrap();

    assert!(merged.contains("Drums A"));
    assert!(!merged.contains("Bass A"), "unselected track from source A leaked in");
    assert!(merged.contains("Pads B"));
    assert!(!merged.contains("Lead B"), "unselected track from source B leaked in");
    assert!(merged.contains("A-Reverb"), "base project's return track should carry over");
    assert!(merged.contains("<Tempo>120</Tempo>"), "base project's tempo should carry over");

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
}

#[test]
fn shifts_every_id_in_a_donor_track_s_subtree_not_just_its_own() {
    // Source A (the base) is never touched. Source B's track carries a
    // nested device Id that collides with source A's own -- the exact shape
    // of the real "non-unique Pointee IDs" failure, one level deeper than
    // the top-level track Id.
    let a_xml = project_xml(&midi_track("Drums A"));
    let b_xml = project_xml(r#"<MidiTrack Id="1"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><Devices><PluginDevice Id="7" /></Devices></DeviceChain></MidiTrack>"#);

    let merged = build_mashup(&[
        MashupSource { xml: a_xml, track_indices: vec![0] },
        MashupSource { xml: b_xml, track_indices: vec![0] },
    ])
    .unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    // Source A's own Ids are the base document's -- never rewritten.
    assert!(merged.contains(r#"<MidiTrack Id="1">"#), "base source's track Id should be untouched");
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
    let b_xml = project_xml(r#"<MidiTrack Id="1"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><Devices><PluginDevice Id="7" /></Devices></DeviceChain><AutomationEnvelopes><AutomationEnvelope Id="8"><EnvelopeTarget><PointeeId Value="7" /></EnvelopeTarget></AutomationEnvelope></AutomationEnvelopes></MidiTrack>"#);

    let merged = build_mashup(&[
        MashupSource { xml: a_xml, track_indices: vec![0] },
        MashupSource { xml: b_xml, track_indices: vec![0] },
    ])
    .unwrap();

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
    assert_eq!(device_id, pointee_value, "PointeeId must still point at its shifted target");
}

#[test]
fn raises_next_pointee_id_above_every_id_brought_in_from_either_source() {
    // Base's own counter (100) easily covers its own low Ids, but source B
    // carries a device Id far above it -- exactly what triggered Live's
    // real "NextPointeeId is too low: X must be bigger than Y" error.
    let a_xml = project_xml_with_next_pointee_id(&midi_track("Drums A"), 100);
    let b_xml = project_xml(r#"<MidiTrack Id="2"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><Devices><PluginDevice Id="26192" /></Devices></DeviceChain></MidiTrack>"#);

    let merged = build_mashup(&[
        MashupSource { xml: a_xml, track_indices: vec![0] },
        MashupSource { xml: b_xml, track_indices: vec![0] },
    ])
    .unwrap();

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
    let b_xml = project_xml(r#"<MidiTrack Id="1"><Name><EffectiveName Value="Lead B" /></Name><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value><MidiClip Id="2"><GrooveSettings><GrooveId Value="3" /></GrooveSettings></MidiClip></Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack>"#);

    let merged = build_mashup(&[
        MashupSource { xml: a_xml, track_indices: vec![0] },
        MashupSource { xml: b_xml, track_indices: vec![0] },
    ])
    .unwrap();

    let doc = roxmltree::Document::parse(&merged).unwrap();
    let groove_id = doc
        .descendants()
        .find(|n| n.tag_name().name() == "GrooveId")
        .and_then(|n| n.attribute("Value"))
        .unwrap();
    assert_eq!(groove_id, "-1", "donor track's groove reference should be cleared, not shifted");
}

/// Base-project tracks keep whatever groove they had -- it's valid against
/// the base document's own retained GroovePool, never touched by this merge.
#[test]
fn base_project_s_own_groove_references_are_left_alone() {
    let a_xml = project_xml(r#"<MidiTrack Id="1"><Name><EffectiveName Value="Drums A" /></Name><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value><MidiClip Id="2"><GrooveSettings><GrooveId Value="3" /></GrooveSettings></MidiClip></Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack>"#);

    let merged = build_mashup(&[MashupSource { xml: a_xml, track_indices: vec![0] }]).unwrap();

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
    let b_xml = project_xml(r#"<MidiTrack Id="2"><Name><EffectiveName Value="Lead B" /></Name></MidiTrack>"#);

    let merged = build_mashup(&[
        MashupSource { xml: a_xml, track_indices: vec![0] },
        MashupSource { xml: b_xml, track_indices: vec![0] },
    ])
    .unwrap();

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
        r#"<ReturnTrack Id="9"><DeviceChain><ClipSlot Id="42" /></DeviceChain><Automation><ClipSlot Id="42" /></Automation></ReturnTrack>"#,
    ));

    let merged = build_mashup(&[MashupSource { xml: a_xml, track_indices: vec![0] }]).unwrap();

    assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
}

#[test]
fn out_of_range_index_errors_instead_of_panicking() {
    let a_xml = project_xml(&midi_track("Only Track"));
    let err = build_mashup(&[MashupSource { xml: a_xml, track_indices: vec![5] }]).unwrap_err();
    assert!(err.to_string().contains("out of range"));
}
