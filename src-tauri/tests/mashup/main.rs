use malstrom_lib::als::mashup::{
    build_mashup, diagnostics, plan_variations, MashupSource, Variation,
};

mod checks;
mod groups;
mod ids;
mod plan;
mod sends;
mod transpose;
mod twist;

/// One variation picking `indices` from each source.
fn mash(sources: Vec<(String, Vec<usize>)>) -> anyhow::Result<String> {
    let (xmls, picks): (Vec<_>, Vec<_>) = sources.into_iter().unzip();
    let sources: Vec<MashupSource> = xmls
        .into_iter()
        .map(|xml| MashupSource {
            xml,
            semitones: vec![],
        })
        .collect();
    build_mashup(
        &sources,
        &[Variation {
            name: "Var 1".to_string(),
            picks,
            ..Default::default()
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

/// `track` moved under the group with Id `group`.
fn in_group(track: String, group: usize) -> String {
    track.replacen(
        r#"<TrackGroupId Value="-1" />"#,
        &format!(r#"<TrackGroupId Value="{group}" />"#),
        1,
    )
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
fn sets_from_different_live_versions_are_refused() {
    let set = |version: &str| {
        project_xml(&midi_track("T")).replacen(
            r#"MajorVersion="5""#,
            &format!(r#"MajorVersion="5" MinorVersion="{version}""#),
            1,
        )
    };
    let err = mash(vec![
        (set("11.0_11300"), vec![0]),
        (set("12.0_12203"), vec![0]),
    ])
    .unwrap_err();
    assert!(
        err.to_string().contains("Live 11 set with a Live 12"),
        "{err}"
    );
    assert!(mash(vec![
        (set("12.0_12203"), vec![0]),
        (set("12.0_12100"), vec![0])
    ])
    .is_ok());
}
