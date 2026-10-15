use super::*;

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
