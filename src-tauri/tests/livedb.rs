mod support;

use malstrom_lib::als::livedb::lookup_tags;

/// Builds a tiny fixture DB shaped like a real `Live-files-*.db` -- not a
/// real system DB, so this stays portable to CI.
fn fixture_db(path: &std::path::Path) {
    let conn = support::open_live_db(path);

    // Two distinct content rows share the filename "Kick.wav" (e.g. one
    // original, one a "Collect All and Save" copy), tagged differently.
    conn.execute_batch(
        "INSERT INTO files (file_id, name) VALUES
            (1, 'Kick.wav'),
            (2, 'Kick.wav'),
            (100, 'Kick'),
            (101, 'One Shot'),
            (102, 'Loop');
         INSERT INTO keywords (file_id, keyw_id, is_auto) VALUES
            (1, 100, 1),
            (1, 101, 1),
            (2, 102, 1);",
    )
    .unwrap();
}

#[test]
fn unions_tags_across_duplicate_filenames() {
    let dir = support::temp_dir("livedb-union");
    let db_path = dir.join("Live-files-test.db");
    fixture_db(&db_path);

    let mut tags = lookup_tags(&db_path, "Kick.wav").unwrap();
    tags.sort();
    assert_eq!(tags, vec!["Kick", "Loop", "One Shot"]);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn returns_empty_for_unmatched_filename() {
    let dir = support::temp_dir("livedb-empty");
    let db_path = dir.join("Live-files-test.db");
    fixture_db(&db_path);

    let tags = lookup_tags(&db_path, "Nonexistent.wav").unwrap();
    assert!(tags.is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
}
