//! App-owned key/value storage that overlays a Live project (starting with
//! track category overrides) without touching the source `.als` file.
//! See docs/app-overlay-storage-spec.md.

use std::path::PathBuf;

use serde_json::{Map, Value};
use tauri::{async_runtime::Mutex, AppHandle, Manager, State};

/// Guards read-modify-write access to the overlay file so concurrent Tauri
/// command invocations serialize instead of racing on disk.
pub struct OverlayLock(pub Mutex<()>);

impl Default for OverlayLock {
    fn default() -> Self {
        Self(Mutex::new(()))
    }
}

fn overlay_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .expect("app_config_dir unavailable")
        .join("overlay.json")
}

/// Reads the overlay file. Missing or empty -> `{}`. Corrupt -> backs up the
/// bad file as `overlay.json.bak-<unix-timestamp>` and continues with `{}`;
/// never a hard failure.
fn read(app: &AppHandle) -> Map<String, Value> {
    read_from(&overlay_path(app))
}

fn read_from(path: &std::path::Path) -> Map<String, Value> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => return Map::new(),
    };
    if contents.trim().is_empty() {
        return Map::new();
    }
    match serde_json::from_str(&contents) {
        Ok(Value::Object(map)) => map,
        _ => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup_path = path.with_file_name(format!("overlay.json.bak-{timestamp}"));
            let _ = std::fs::rename(path, backup_path);
            Map::new()
        }
    }
}

fn write(app: &AppHandle, data: &Map<String, Value>) -> std::io::Result<()> {
    write_to(&overlay_path(app), data)
}

fn write_to(path: &std::path::Path, data: &Map<String, Value>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(data)?)
}

#[tauri::command]
pub async fn overlay_get(
    app: AppHandle,
    state: State<'_, OverlayLock>,
    namespace: String,
    key: String,
) -> Result<Option<Value>, String> {
    let _guard = state.0.lock().await;
    let data = read(&app);
    Ok(data
        .get(&namespace)
        .and_then(|ns| ns.get(&key))
        .cloned())
}

#[tauri::command]
pub async fn overlay_set(
    app: AppHandle,
    state: State<'_, OverlayLock>,
    namespace: String,
    key: String,
    value: Value,
) -> Result<(), String> {
    let _guard = state.0.lock().await;
    let mut data = read(&app);
    data.entry(namespace)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("namespace entries are always objects")
        .insert(key, value);
    write(&app, &data).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn overlay_remove(
    app: AppHandle,
    state: State<'_, OverlayLock>,
    namespace: String,
    key: String,
) -> Result<(), String> {
    let _guard = state.0.lock().await;
    let mut data = read(&app);
    if let Some(ns) = data.get_mut(&namespace).and_then(|ns| ns.as_object_mut()) {
        ns.remove(&key);
        if ns.is_empty() {
            data.remove(&namespace);
        }
    }
    write(&app, &data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_overlay_path() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("malstrom-overlay-test-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
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
        let path = temp_overlay_path();
        assert!(!path.exists());
        assert_eq!(read_from(&path), Map::new());
    }

    #[test]
    fn corrupt_file_reads_as_empty_and_is_backed_up() {
        let path = temp_overlay_path();
        std::fs::write(&path, b"not json").unwrap();

        assert_eq!(read_from(&path), Map::new());

        let dir = path.parent().unwrap();
        let backups: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("overlay.json.bak-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(!path.exists());
    }

    #[test]
    fn round_trips_a_value_through_set_get_remove() {
        let path = temp_overlay_path();

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
}
