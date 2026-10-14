//! App-owned key/value storage for data about Live projects (starting with
//! track category overrides) without touching the source `.als` file.
//! See notes/app-overlay-storage-spec.md.

use std::path::PathBuf;

use serde_json::{Map, Value};
use tauri::{async_runtime::Mutex, AppHandle, Manager};

/// Guards read-modify-write access to the store file so concurrent Tauri
/// command invocations serialize instead of racing on disk.
pub struct StoreLock(pub Mutex<()>);

impl Default for StoreLock {
    fn default() -> Self {
        Self(Mutex::new(()))
    }
}

fn store_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .expect("app_config_dir unavailable")
        .join("store.json")
}

/// Reads the store file. Missing or empty -> `{}`. Corrupt -> backs up the
/// bad file as `store.json.bak-<unix-timestamp>` and continues with `{}`;
/// never a hard failure.
pub fn read(app: &AppHandle) -> Map<String, Value> {
    read_from(&store_path(app))
}

pub fn read_from(path: &std::path::Path) -> Map<String, Value> {
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
            let backup_path = path.with_file_name(format!("store.json.bak-{timestamp}"));
            let _ = std::fs::rename(path, backup_path);
            Map::new()
        }
    }
}

pub fn write(app: &AppHandle, data: &Map<String, Value>) -> std::io::Result<()> {
    write_to(&store_path(app), data)
}

pub fn write_to(path: &std::path::Path, data: &Map<String, Value>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write-then-rename so a crash mid-write can't leave a truncated store.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(data)?)?;
    std::fs::rename(tmp, path)
}

/// Sets `data[namespace][key] = value`, creating the namespace if it doesn't
/// exist yet. Errors if the namespace holds a non-object (e.g. a hand edit).
pub fn set(
    data: &mut Map<String, Value>,
    namespace: String,
    key: String,
    value: Value,
) -> Result<(), String> {
    data.entry(namespace)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or("store namespace is not an object")?
        .insert(key, value);
    Ok(())
}

/// Removes `data[namespace][key]`. Prunes `namespace` too if that was its
/// last entry, rather than leaving a dangling empty object.
pub fn remove(data: &mut Map<String, Value>, namespace: &str, key: &str) {
    if let Some(ns) = data.get_mut(namespace).and_then(|ns| ns.as_object_mut()) {
        ns.remove(key);
        if ns.is_empty() {
            data.remove(namespace);
        }
    }
}
