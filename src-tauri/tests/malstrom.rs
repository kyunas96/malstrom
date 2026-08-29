use malstrom_lib::als::AlsInspector;
use std::path::Path;

#[path = "support/mod.rs"]
mod support;

#[test]
fn open_reads_fixture_file() {
    // Live 12.x-shaped fixture: MajorVersion="5", numeric Root/Name.
    let dir = support::temp_dir("open-reads-fixture");
    let path = dir.join("10-30-25.als");
    support::write_als_fixture(&path, support::LIVE_12_STYLE_XML);

    let inspector = AlsInspector::open(&path).unwrap();
    let candidates = inspector.extract_scale_candidates().unwrap();

    assert!(!candidates.scales.is_empty());
}

#[test]
fn open_reads_pre_scale_schema_fixture_file() {
    // Live 9.x-shaped fixture: MajorVersion="4", no ScaleInformation at all
    // (the feature didn't exist yet in that schema). Candidates still come
    // purely from note content.
    let dir = support::temp_dir("open-reads-pre-scale-schema-fixture");
    let path = dir.join("5-27-20.als");
    support::write_als_fixture(&path, support::LIVE_9_STYLE_XML);

    let inspector = AlsInspector::open(&path).unwrap();
    let candidates = inspector.extract_scale_candidates().unwrap();

    assert!(!candidates.scales.is_empty());
}

#[test]
fn open_missing_file_returns_err() {
    let result = AlsInspector::open(Path::new("tests/fixtures/does-not-exist.als"));
    assert!(result.is_err());
}
