mod support;

use malstrom_lib::als::AlsInspector;
use rusqlite::Connection;

fn project_xml(tracks: &str) -> String {
    format!(r#"<Ableton><LiveSet><Tracks>{tracks}</Tracks></LiveSet></Ableton>"#)
}

fn midi_track(name: &str) -> String {
    format!(r#"<MidiTrack Id="1"><Name><EffectiveName Value="{name}" /></Name></MidiTrack>"#)
}

fn audio_track_with_sample(name: &str, sample_path: &str) -> String {
    format!(
        r#"<AudioTrack Id="2">
            <Name><EffectiveName Value="{name}" /></Name>
            <DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value>
                <AudioClip Id="1"><SampleRef><FileRef><Path Value="{sample_path}" /></FileRef></SampleRef></AudioClip>
            </Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain>
        </AudioTrack>"#
    )
}

fn return_track(name: &str) -> String {
    format!(r#"<ReturnTrack Id="3"><Name><EffectiveName Value="{name}" /></Name></ReturnTrack>"#)
}

fn midi_track_with_devices(name: &str, devices: &str) -> String {
    format!(
        r#"<MidiTrack Id="4">
            <Name><EffectiveName Value="{name}" /></Name>
            <DeviceChain><Devices>{devices}</Devices></DeviceChain>
        </MidiTrack>"#
    )
}

/// A rack-style instrument device (Drum Rack, Sampler, ...) identified by
/// its own saved preset file, not any sample nested inside it.
fn rack_device_with_preset(tag: &str, preset_path: &str) -> String {
    format!(
        r#"<{tag} Id="1">
            <LastPresetRef><Value><FilePresetRef Id="1"><FileRef><Path Value="{preset_path}" /></FileRef></FilePresetRef></Value></LastPresetRef>
        </{tag}>"#
    )
}

/// A `Simpler` device identified by the one sample it wraps.
fn simpler_device_with_sample(sample_path: &str) -> String {
    format!(
        r#"<OriginalSimpler Id="1">
            <SampleRef><FileRef><Path Value="{sample_path}" /></FileRef></SampleRef>
        </OriginalSimpler>"#
    )
}

fn db_with_tags(dir: &std::path::Path, filename: &str, tag: &str) -> std::path::PathBuf {
    let db_path = dir.join("Live-files-test.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE files (file_id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE keywords (file_id INTEGER, keyw_id INTEGER, is_auto BOOL);
         INSERT INTO files (file_id, name) VALUES (1, '{filename}'), (100, '{tag}');
         INSERT INTO keywords (file_id, keyw_id, is_auto) VALUES (1, 100, 1);"
    ))
    .unwrap();
    db_path
}

#[test]
fn excludes_return_tracks() {
    let xml = project_xml(&format!(
        "{}{}",
        midi_track("Lead Vocal"),
        return_track("A-Reverb")
    ));
    let inspector = AlsInspector::from_xml(xml);
    let tracks = inspector.extract_tracks(None).unwrap();

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].name, "Lead Vocal");
}

#[test]
fn falls_back_to_name_when_no_db_path_given() {
    let xml = project_xml(&midi_track("Lead Vocal"));
    let inspector = AlsInspector::from_xml(xml);
    let tracks = inspector.extract_tracks(None).unwrap();

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].name, "Lead Vocal");
    assert_eq!(tracks[0].kind, "Midi");
    assert!(matches!(
        tracks[0].category,
        malstrom_lib::als::TrackCategory::Vocals
    ));
}

#[test]
fn resolves_sample_filename_from_audio_track() {
    let xml = project_xml(&audio_track_with_sample(
        "Untitled",
        "/Users/me/Samples/Kick.wav",
    ));
    let inspector = AlsInspector::from_xml(xml);
    // No DB path given, so this exercises name fallback only -- confirms
    // the sample-resolution path doesn't panic/error even when unused, and
    // that a track with no name keyword lands in Other rather than
    // crashing or guessing.
    let tracks = inspector.extract_tracks(None).unwrap();

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].kind, "Audio");
    assert!(matches!(
        tracks[0].category,
        malstrom_lib::als::TrackCategory::Other
    ));
}

#[test]
fn db_tag_wins_over_an_unrelated_track_name() {
    let dir = support::temp_dir("track-list-db");
    let db_path = dir.join("Live-files-test.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE files (file_id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE keywords (file_id INTEGER, keyw_id INTEGER, is_auto BOOL);
         INSERT INTO files (file_id, name) VALUES (1, 'Kick.wav'), (100, 'Kick');
         INSERT INTO keywords (file_id, keyw_id, is_auto) VALUES (1, 100, 1);",
    )
    .unwrap();

    let sample_path = dir.join("Kick.wav");
    let xml = project_xml(&audio_track_with_sample(
        "Untitled",
        sample_path.to_str().unwrap(),
    ));
    let inspector = AlsInspector::from_xml(xml);
    let tracks = inspector.extract_tracks(Some(&db_path)).unwrap();

    assert_eq!(tracks.len(), 1);
    assert!(matches!(
        tracks[0].category,
        malstrom_lib::als::TrackCategory::Drums
    ));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn categorizes_rack_instrument_by_its_own_preset_not_a_nested_sample() {
    let dir = support::temp_dir("track-list-rack-preset");
    let db_path = db_with_tags(&dir, "Abyss Kit.adg", "drum loop");

    let xml = project_xml(&midi_track_with_devices(
        "Untitled",
        &rack_device_with_preset("DrumGroupDevice", "Drums/Abyss Kit.adg"),
    ));
    let inspector = AlsInspector::from_xml(xml);
    let tracks = inspector.extract_tracks(Some(&db_path)).unwrap();

    assert_eq!(tracks.len(), 1);
    assert!(matches!(
        tracks[0].category,
        malstrom_lib::als::TrackCategory::Drums
    ));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn categorizes_simpler_by_its_wrapped_sample() {
    let dir = support::temp_dir("track-list-simpler-sample");
    let sample_path = dir.join("Kick.wav");
    let db_path = db_with_tags(&dir, "Kick.wav", "kick");

    let xml = project_xml(&midi_track_with_devices(
        "Untitled",
        &simpler_device_with_sample(sample_path.to_str().unwrap()),
    ));
    let inspector = AlsInspector::from_xml(xml);
    let tracks = inspector.extract_tracks(Some(&db_path)).unwrap();

    assert_eq!(tracks.len(), 1);
    assert!(matches!(
        tracks[0].category,
        malstrom_lib::als::TrackCategory::Drums
    ));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn skips_leading_midi_effect_to_find_the_instrument() {
    let dir = support::temp_dir("track-list-midi-fx");
    let db_path = db_with_tags(&dir, "Abyss Kit.adg", "drum loop");

    let devices = format!(
        r#"<MidiArpeggiator Id="0" />{}"#,
        rack_device_with_preset("DrumGroupDevice", "Drums/Abyss Kit.adg")
    );
    let xml = project_xml(&midi_track_with_devices("Untitled", &devices));
    let inspector = AlsInspector::from_xml(xml);
    let tracks = inspector.extract_tracks(Some(&db_path)).unwrap();

    assert_eq!(tracks.len(), 1);
    assert!(matches!(
        tracks[0].category,
        malstrom_lib::als::TrackCategory::Drums
    ));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn factory_default_preset_falls_back_to_name() {
    let xml = project_xml(&midi_track_with_devices(
        "Lead Synth",
        r#"<InstrumentVector Id="1">
            <LastPresetRef><Value><AbletonDefaultPresetRef Id="1"><FileRef><Path Value="/Applications/Ableton Live 12 Suite.app/.../Wavetable" /></FileRef></AbletonDefaultPresetRef></Value></LastPresetRef>
        </InstrumentVector>"#,
    ));
    let inspector = AlsInspector::from_xml(xml);
    // No matching DB row for the factory path, so this exercises "no
    // FilePresetRef found" -> falls back to name -- which does resolve here
    // via "lead".
    let dir = support::temp_dir("track-list-default-preset");
    let db_path = db_with_tags(&dir, "Abyss Kit.adg", "drum loop");
    let tracks = inspector.extract_tracks(Some(&db_path)).unwrap();

    assert_eq!(tracks.len(), 1);
    assert!(matches!(
        tracks[0].category,
        malstrom_lib::als::TrackCategory::Lead
    ));

    std::fs::remove_dir_all(&dir).unwrap();
}
