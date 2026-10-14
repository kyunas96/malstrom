use malstrom_lib::store::{read_from, remove, set, write_to};
use serde_json::{Map, Value};
use std::fs;

fn temp_store_path(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("malstrom-store-test-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join("store.json")
}

#[test]
fn missing_file_reads_as_empty() {
    let path = temp_store_path("missing");
    assert!(!path.exists());
    assert_eq!(read_from(&path), Map::new());
}

#[test]
fn corrupt_file_reads_as_empty_and_is_backed_up() {
    let path = temp_store_path("corrupt");
    fs::write(&path, b"not json").unwrap();

    assert_eq!(read_from(&path), Map::new());

    let dir = path.parent().unwrap();
    let backups: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("store.json.bak-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert!(!path.exists());
}

#[test]
fn round_trips_a_value_through_set_get_remove() {
    let path = temp_store_path("roundtrip");

    let mut data = read_from(&path);
    set(
        &mut data,
        "trackCategoryOverrides".into(),
        "track-1".into(),
        Value::String("Drums".into()),
    )
    .unwrap();
    write_to(&path, &data).unwrap();

    let data = read_from(&path);
    assert_eq!(
        data["trackCategoryOverrides"]["track-1"],
        Value::String("Drums".into())
    );

    let mut data = data;
    remove(&mut data, "trackCategoryOverrides", "track-1");
    write_to(&path, &data).unwrap();

    // Emptied namespace is pruned, not left as a dangling {}.
    let data = read_from(&path);
    assert!(!data.contains_key("trackCategoryOverrides"));
}

#[test]
fn set_into_non_object_namespace_errors_instead_of_panicking() {
    let mut data = Map::new();
    data.insert("trackCategoryOverrides".into(), Value::from(5));

    let result = set(
        &mut data,
        "trackCategoryOverrides".into(),
        "track-1".into(),
        Value::String("Drums".into()),
    );

    assert!(result.is_err());
    assert_eq!(data["trackCategoryOverrides"], Value::from(5));
}

#[test]
fn invalid_utf8_file_is_backed_up_not_clobbered() {
    let path = temp_store_path("utf8");
    fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();

    assert_eq!(read_from(&path), Map::new());

    let backed_up = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("store.json.bak-")
        });
    assert!(backed_up);
    assert!(!path.exists());
}
