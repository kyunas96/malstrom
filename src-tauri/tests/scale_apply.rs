use malstrom_lib::als::output_path::{derive_output_path, resolve_output_path};
use malstrom_lib::als::{AlsInspector, ApplyScaleOutcome};
use anyhow::Result;
use roxmltree::Document;
use std::path::Path;

/// Test-only shorthand: wrap a raw (un-gzipped) XML string and apply a
/// scale in one step, mirroring the fixture-backed tests elsewhere that go
/// through `AlsInspector::open`.
fn apply_scale(xml: &str, root_note: i32, scale_index: i32) -> Result<(String, ApplyScaleOutcome)> {
    AlsInspector::from_xml(xml.to_string()).apply_scale(root_note, scale_index)
}

fn clip_xml(inner: &str) -> String {
    format!(
        r#"<Ableton><LiveSet><Tracks><MidiTrack><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value><MidiClip Id="1" Time="0">
					{inner}
				</MidiClip></Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack></Tracks></LiveSet></Ableton>"#
    )
}

fn clip_xml_with_version(inner: &str, major_version: i32) -> String {
    format!(
        r#"<Ableton MajorVersion="{major_version}"><LiveSet><Tracks><MidiTrack><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value><MidiClip Id="1" Time="0">
						{inner}
					</MidiClip></Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack></Tracks></LiveSet></Ableton>"#
    )
}

fn notes_with_keys(midi_keys: &[i32]) -> String {
    let key_tracks: String = midi_keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            format!(
                r#"<KeyTrack Id="{i}"><Notes><MidiNoteEvent Time="0" Duration="1" Velocity="100" OffVelocity="0" NoteId="{i}" /></Notes><MidiKey Value="{key}" /></KeyTrack>"#
            )
        })
        .collect();
    format!(r#"<Notes><KeyTracks>{key_tracks}</KeyTracks></Notes>"#)
}

/// Wraps multiple already-built `<MidiClip>...</MidiClip>` blocks into one
/// document, so a test can exercise several clips (insert + rewrite +
/// leave-alone) getting edited in the same pass.
fn multi_clip_xml(clips: &[String]) -> String {
    let clip_slots: String = clips
        .iter()
        .enumerate()
        .map(|(i, clip)| {
            format!(
                r#"<ClipSlot Id="{i}"><ClipSlot><Value>{clip}</Value></ClipSlot></ClipSlot>"#
            )
        })
        .collect();
    format!(
        r#"<Ableton><LiveSet><Tracks><MidiTrack><DeviceChain><MainSequencer><ClipSlotList>{clip_slots}</ClipSlotList></MainSequencer></DeviceChain></MidiTrack></Tracks></LiveSet></Ableton>"#
    )
}

fn midi_clip(id: i32, inner: &str) -> String {
    format!(r#"<MidiClip Id="{id}" Time="0">{inner}</MidiClip>"#)
}

// Regression test for the well-formedness guard added to
// `apply_scale_to_xml`: it re-parses the fully-edited document (after every
// clip's edit has been spliced in) and rejects the write if that fails,
// since a wrong byte range from a misdetected Ableton schema must never
// reach disk as corrupted XML. This exercises several clips getting
// different edit kinds (insert, rewrite, and none) in one pass -- the
// scenario where a bad range would actually show up, because a single-clip
// edit landing wrong is indistinguishable from just not editing.
#[test]
fn multi_clip_apply_produces_well_formed_output() {
    let clips = vec![
        // No ScaleInformation yet -> inserted.
        midi_clip(1, &notes_with_keys(&[2, 5, 9])),
        // Default ScaleInformation -> rewritten.
        midi_clip(
            2,
            &format!(
                r#"<ScaleInformation><Root Value="0" /><Name Value="0" /></ScaleInformation>{}"#,
                notes_with_keys(&[2, 5, 9])
            ),
        ),
        // Notes don't fit the target scale -> left untouched.
        midi_clip(3, &notes_with_keys(&[1, 6, 10])),
    ];
    let xml = multi_clip_xml(&clips);

    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();

    assert_eq!(outcome.clips_created, 1);
    assert_eq!(outcome.clips_changed, 1);
    assert_eq!(outcome.clips_incompatible, 1);
    Document::parse(&new_xml).expect("multi-clip output must still be valid XML");
}

#[test]
fn rewrites_default_scale_information_when_compatible() {
    // C major triad pitch classes (0, 4, 7) fit D Minor? No -- use pitches
    // that fit D Minor (root=2, Minor intervals): D F A -> 2, 5, 9.
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><Root Value="0" /><Name Value="0" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_changed, 1);
    assert_eq!(outcome.clips_created, 0);
    assert_eq!(outcome.clips_already_set, 0);
    assert_eq!(outcome.clips_incompatible, 0);
    assert!(new_xml.contains(r#"<Root Value="2" />"#));
    assert!(new_xml.contains(r#"<Name Value="1" />"#));
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn flips_disabled_scale_awareness_to_enabled_when_correcting() {
    // Real Ableton files put `IsInKey` as a sibling immediately before
    // `ScaleInformation`, never as a child of it -- applying a scale must
    // still turn it on there.
    let xml = clip_xml(&format!(
        r#"<IsInKey Value="false" /><ScaleInformation><Root Value="0" /><Name Value="0" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_changed, 1);
    assert!(new_xml.contains(r#"<IsInKey Value="true" />"#));
    assert!(!new_xml.contains(r#"Value="false""#));
    assert_eq!(new_xml.matches("IsInKey").count(), 1);
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn does_not_duplicate_already_enabled_is_in_key() {
    let xml = clip_xml(&format!(
        r#"<IsInKey Value="true" /><ScaleInformation><Root Value="0" /><Name Value="0" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_changed, 1);
    assert_eq!(new_xml.matches("IsInKey").count(), 1);
    assert!(new_xml.contains(r#"<IsInKey Value="true" />"#));
}

#[test]
fn leaves_non_default_scale_information_untouched() {
    // A Minor (root=9, scale=1) genuinely fits notes D F A (2, 5, 9),
    // so it's a valid, deliberate label and must be left alone.
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><Root Value="9" /><Name Value="1" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_already_set, 1);
    assert_eq!(outcome.clips_changed, 0);
    assert_eq!(outcome.clips_corrected, 0);
    assert!(new_xml.contains(r#"<Root Value="9" />"#));
}

#[test]
fn leaves_is_in_key_untouched_for_already_valid_label() {
    // "Already set" clips are a deliberate, valid choice and must be left
    // completely alone -- including whatever they already have IsInKey
    // set to, even if that's "false".
    let xml = clip_xml(&format!(
        r#"<IsInKey Value="false" /><ScaleInformation><Root Value="9" /><Name Value="1" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_already_set, 1);
    assert!(new_xml.contains(r#"<IsInKey Value="false" />"#));
}

#[test]
fn corrects_stale_scale_label_that_does_not_fit_its_own_notes() {
    // Clip is explicitly labeled G Major (root=7, scale=0), but its
    // notes (2, 5, 9 = D F A) don't fit G Major at all -- a stale label.
    // D Minor (root=2, scale=1) does fit, so it should be corrected.
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><Root Value="7" /><Name Value="0" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_corrected, 1);
    assert_eq!(outcome.clips_already_set, 0);
    assert_eq!(outcome.clips_changed, 0);
    assert!(new_xml.contains(r#"<Root Value="2" />"#));
    assert!(new_xml.contains(r#"<Name Value="1" />"#));
}

#[test]
fn leaves_valid_non_default_label_untouched_even_if_different_from_target() {
    // Clip is labeled D Dorian (root=2, scale=2), and its notes (2, 5,
    // 9) DO fit D Dorian -- a deliberate, valid choice, so pulling a
    // different (but also compatible) scale must leave it alone.
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><Root Value="2" /><Name Value="2" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_already_set, 1);
    assert_eq!(outcome.clips_corrected, 0);
    assert!(new_xml.contains(r#"<Name Value="2" />"#));
}

#[test]
fn fills_in_scale_information_missing_root_and_name() {
    // Some real projects have a ScaleInformation node with only
    // IsScaleEnabled and no Root/Name at all -- must not crash, and the
    // other child should be preserved.
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><IsScaleEnabled Value="true" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_corrected, 1);
    assert!(new_xml.contains(r#"<Root Value="2" />"#));
    assert!(new_xml.contains(r#"<Name Value="1" />"#));
    assert!(new_xml.contains(r#"<IsScaleEnabled Value="true" />"#));
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn fills_in_self_closing_scale_information_missing_root_and_name() {
    let xml = clip_xml(&format!(
        r#"<ScaleInformation />{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_corrected, 1);
    assert!(new_xml.contains(r#"<Root Value="2" />"#));
    assert!(new_xml.contains(r#"<Name Value="1" />"#));
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn rewrites_live11_style_root_note_and_named_scale_without_duplicating() {
    // Live 11.x shape: root tag is `RootNote`, and `Name`'s value is
    // the literal scale name string, not a numeric index.
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><RootNote Value="0" /><Name Value="Major" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_changed, 1);
    // Must be rewritten in the SAME schema, not duplicated alongside a
    // new Root/Name numeric pair.
    assert!(new_xml.contains(r#"<RootNote Value="2" />"#));
    assert!(new_xml.contains(r#"<Name Value="Minor" />"#));
    assert!(!new_xml.contains("<Root "), "must not leave a stray <Root> tag");
    assert_eq!(new_xml.matches("RootNote").count(), 1);
    assert_eq!(new_xml.matches("<Name").count(), 1);
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn leaves_valid_live11_style_label_untouched() {
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><RootNote Value="9" /><Name Value="Minor" /></ScaleInformation>{}"#,
        notes_with_keys(&[2, 5, 9])
    ));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_already_set, 1);
    assert_eq!(outcome.clips_changed, 0);
    assert!(new_xml.contains(r#"<RootNote Value="9" />"#));
    assert!(new_xml.contains(r#"<Name Value="Minor" />"#));
}

#[test]
fn created_block_matches_sniffed_document_schema() {
    // One clip already uses the Live 11.x shape; a second clip has no
    // ScaleInformation at all and should get a new block created in the
    // SAME (sniffed) shape, not the hardcoded numeric default.
    let clip_a = format!(
        r#"<MidiClip Id="1" Time="0"><ScaleInformation><RootNote Value="0" /><Name Value="Major" /></ScaleInformation>{}</MidiClip>"#,
        notes_with_keys(&[0, 4, 7])
    );
    let clip_b = format!(
        r#"<MidiClip Id="2" Time="0">{}</MidiClip>"#,
        notes_with_keys(&[2, 5, 9])
    );
    let xml = format!(
        r#"<Ableton><LiveSet><Tracks><MidiTrack><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value>{clip_a}{clip_b}</Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack></Tracks></LiveSet></Ableton>"#
    );
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_created, 1);
    assert!(new_xml.contains(r#"<RootNote Value="2" />"#));
    assert!(new_xml.contains(r#"<Name Value="Minor" />"#));
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn leaves_incompatible_clip_untouched() {
    let xml = clip_xml(&format!(
        r#"<ScaleInformation><Root Value="0" /><Name Value="0" /></ScaleInformation>{}"#,
        // C# doesn't fit D Minor.
        notes_with_keys(&[1])
    ));
    let (_, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_incompatible, 1);
    assert_eq!(outcome.clips_changed, 0);
}

#[test]
fn creates_scale_information_when_missing_and_compatible() {
    let xml = clip_xml(&notes_with_keys(&[2, 5, 9]));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_created, 1);
    assert_eq!(outcome.clips_changed, 0);
    assert!(new_xml.contains(r#"<ScaleInformation>"#));
    assert!(new_xml.contains(r#"<Root Value="2" />"#));
    assert!(new_xml.contains(r#"<Name Value="1" />"#));
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn creates_is_in_key_alongside_new_scale_information() {
    // A clip with neither ScaleInformation nor IsInKey should get both --
    // IsInKey is what actually turns scale awareness on, and Ableton
    // always places it immediately before ScaleInformation.
    let xml = clip_xml(&notes_with_keys(&[2, 5, 9]));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_created, 1);
    assert!(new_xml.contains(r#"<IsInKey Value="true" />"#));
    assert_eq!(new_xml.matches("IsInKey").count(), 1);
    assert!(
        new_xml.find("IsInKey").unwrap() < new_xml.find("ScaleInformation").unwrap(),
        "IsInKey must precede ScaleInformation, matching Ableton's own ordering"
    );
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn creates_nothing_when_missing_and_incompatible() {
    let xml = clip_xml(&notes_with_keys(&[1]));
    let (new_xml, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert_eq!(outcome.clips_created, 0);
    assert_eq!(outcome.clips_incompatible, 1);
    assert!(!new_xml.contains("ScaleInformation"));
}

#[test]
fn flags_schema_predating_clip_scale() {
    let xml = clip_xml_with_version(&notes_with_keys(&[2, 5, 9]), 4);
    let (_, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert!(outcome.schema_predates_clip_scale);
    assert_eq!(outcome.clips_created, 1);
}

#[test]
fn does_not_flag_current_schema() {
    let xml = clip_xml_with_version(&notes_with_keys(&[2, 5, 9]), 5);
    let (_, outcome) = apply_scale(&xml, 2, 1).unwrap();
    assert!(!outcome.schema_predates_clip_scale);
}

#[test]
fn cleans_up_ambiguous_root_tags_from_earlier_corrupted_apply_without_duplicating() {
    // Reproduces real corruption seen in a Live-11.x-schema project
    // (`<RootNote Value="N"/><Name Value="ScaleName"/>`): an earlier version
    // of apply_scale_to_xml only recognized the Live-12.x shape (`<Root
    // Value="N"/><Name Value="N"/>`) and wrote its numeric block alongside
    // the original `RootNote` tag instead of replacing it, leaving both
    // present at once. Applying a scale again must clean that up, not
    // perpetuate or add to the duplication.
    let corrupted_clip = |id: &str| {
        format!(
            r#"<MidiClip Id="{id}" Time="0"><ScaleInformation><RootNote Value="0" /><Root Value="0" /><Name Value="1" /></ScaleInformation>{}</MidiClip>"#,
            notes_with_keys(&[2, 5, 9]) // D F A -- fits D Dorian (root=2, scale=2)
        )
    };
    let xml = format!(
        r#"<Ableton MajorVersion="5"><LiveSet><Tracks><MidiTrack><DeviceChain><MainSequencer><ClipSlotList><ClipSlot><ClipSlot><Value>{}{}</Value></ClipSlot></ClipSlot></ClipSlotList></MainSequencer></DeviceChain></MidiTrack></Tracks></LiveSet></Ableton>"#,
        corrupted_clip("1"),
        corrupted_clip("2"),
    );

    let (new_xml, outcome) = apply_scale(&xml, 2, 2).unwrap(); // D Dorian

    assert_eq!(outcome.clips_corrected, 2);
    assert_eq!(outcome.clips_already_set, 0);

    let mut idx = 0;
    let mut blocks_checked = 0;
    while let Some(start) = new_xml[idx..].find("<ScaleInformation") {
        let abs_start = idx + start;
        let end = new_xml[abs_start..]
            .find("</ScaleInformation>")
            .map(|e| abs_start + e + "</ScaleInformation>".len())
            .unwrap_or(new_xml.len());
        let block = &new_xml[abs_start..end];
        let has_root = block.contains("<Root ");
        let has_root_note = block.contains("<RootNote ");
        assert!(
            !(has_root && has_root_note),
            "ScaleInformation block has both Root and RootNote: {block}"
        );
        blocks_checked += 1;
        idx = end;
    }
    assert_eq!(blocks_checked, 2);
    Document::parse(&new_xml).expect("output must still be valid XML");
}

#[test]
fn resolve_output_path_is_none_when_nothing_touched() {
    let result =
        resolve_output_path(Path::new("/tmp/Project.als"), "D", "Minor", false, 0, None).unwrap();
    assert!(result.is_none());

    let result_overwrite =
        resolve_output_path(Path::new("/tmp/Project.als"), "D", "Minor", true, 0, None).unwrap();
    assert!(result_overwrite.is_none());
}

#[test]
fn resolve_output_path_overwrite_targets_original_path() {
    let src = Path::new("/tmp/Project.als");
    let result = resolve_output_path(src, "D", "Minor", true, 3, None).unwrap();
    assert_eq!(result, Some(src.to_path_buf()));
}

#[test]
fn resolve_output_path_duplicate_derives_new_path() {
    let dir = std::env::temp_dir().join(format!(
        "malstrom-test-resolve-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("Project.als");
    std::fs::write(&src, b"x").unwrap();

    let result = resolve_output_path(&src, "D", "Minor", false, 3, None).unwrap();
    assert_eq!(result, Some(dir.join("Project (D Minor).als")));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_output_path_uses_custom_file_name_when_given() {
    let dir = std::env::temp_dir().join(format!(
        "malstrom-test-resolve-named-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("Project.als");
    std::fs::write(&src, b"x").unwrap();

    let result =
        resolve_output_path(&src, "D", "Minor", false, 3, Some("My Custom Name.als")).unwrap();
    assert_eq!(result, Some(dir.join("My Custom Name.als")));

    // Extension is appended when the user's name omits it.
    let result_no_ext = resolve_output_path(&src, "D", "Minor", false, 3, Some("No Ext")).unwrap();
    assert_eq!(result_no_ext, Some(dir.join("No Ext.als")));

    // Unsafe path characters are sanitized rather than escaping the directory.
    let result_unsafe =
        resolve_output_path(&src, "D", "Minor", false, 3, Some("a/b\\c.als")).unwrap();
    assert_eq!(result_unsafe, Some(dir.join("a-b-c.als")));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn derive_output_path_avoids_collisions() {
    let dir = std::env::temp_dir().join(format!("malstrom-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let original = dir.join("MyProject.als");
    std::fs::write(&original, b"x").unwrap();

    let first = derive_output_path(&original, "D", "Minor").unwrap();
    assert_eq!(first, dir.join("MyProject (D Minor).als"));
    std::fs::write(&first, b"x").unwrap();

    let second = derive_output_path(&original, "D", "Minor").unwrap();
    assert_eq!(second, dir.join("MyProject (D Minor) (2).als"));

    std::fs::remove_dir_all(&dir).ok();
}

/// Mirrors the round trip `apply_scale_to_project` uses to populate
/// `AppliedScaleResult::updated_scales`: mutate the XML in memory via
/// `apply_scale`, then re-wrap and re-extract candidates from the *mutated*
/// string, without writing to disk. The returned candidates must reflect
/// the just-applied scale, not the pre-edit clip content.
#[test]
fn candidates_from_mutated_xml_reflect_the_applied_scale() {
    let xml = clip_xml(&notes_with_keys(&[60, 64, 67])); // C, E, G -- fits C Major
    let (new_xml, outcome) = apply_scale(&xml, 0, 0).unwrap(); // root C, scale Major
    assert_eq!(outcome.clips_created, 1);

    let candidates = AlsInspector::from_xml(new_xml)
        .extract_scale_candidates()
        .unwrap();

    assert!(
        candidates
            .scales
            .iter()
            .any(|c| c.root_name == "C" && c.scale_name == "Major"),
        "expected C Major among candidates, got {:?}",
        candidates.scales
    );
}
