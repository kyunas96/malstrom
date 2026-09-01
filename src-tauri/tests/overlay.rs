use malstrom_lib::overlay::{read_from, write_to};
use serde_json::{Map, Value};
use std::fs;

fn temp_overlay_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "malstrom-overlay-test-{name}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir.join("overlay.json")
}

fn set(data: &mut Map<String, Value>, namespace: &str, key: &str, value: Value) {
    data.entry(namespace.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap()
        .insert(key.to_string(), value);
}

fn remove(data: &mut Map<String, Value>, namespace: &str, key: &str) {
    if let Some(ns) = data.get_mut(namespace).and_then(|ns| ns.as_object_mut()) {
        ns.remove(key);
        if ns.is_empty() {
            data.remove(namespace);
        }
    }
}

#[test]
fn missing_file_reads_as_empty() {
    let path = temp_overlay_path("missing");
    assert!(!path.exists());
    assert_eq!(read_from(&path), Map::new());
}

#[test]
fn corrupt_file_reads_as_empty_and_is_backed_up() {
    let path = temp_overlay_path("corrupt");
    fs::write(&path, b"not json").unwrap();

    assert_eq!(read_from(&path), Map::new());

    let dir = path.parent().unwrap();
    let backups: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("overlay.json.bak-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert!(!path.exists());
}

#[test]
fn round_trips_a_value_through_set_get_remove() {
    let path = temp_overlay_path("roundtrip");

    let mut data = read_from(&path);
    set(&mut data, "trackCategoryOverrides", "track-1", Value::String("Drums".into()));
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
