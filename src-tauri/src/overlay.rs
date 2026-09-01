//! App-owned key/value storage that overlays a Live project (starting with
//! track category overrides) without touching the source `.als` file.
//! See docs/app-overlay-storage-spec.md.

use std::path::PathBuf;

use serde_json::{Map, Value};
use tauri::{async_runtime::Mutex, AppHandle, Manager};

/// Guards read-modify-write access to the overlay file so concurrent Tauri
/// command invocations serialize instead of racing on disk.
pub struct OverlayLock(pub Mutex<()>);

impl Default for OverlayLock {
    fn default() -> Self {
        Self(Mutex::new(()))
    }
}

pub fn overlay_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .expect("app_config_dir unavailable")
        .join("overlay.json")
}

/// Reads the overlay file. Missing or empty -> `{}`. Corrupt -> backs up the
/// bad file as `overlay.json.bak-<unix-timestamp>` and continues with `{}`;
/// never a hard failure.
pub fn read(app: &AppHandle) -> Map<String, Value> {
    read_from(&overlay_path(app))
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
            let backup_path = path.with_file_name(format!("overlay.json.bak-{timestamp}"));
            let _ = std::fs::rename(path, backup_path);
            Map::new()
        }
    }
}

pub fn write(app: &AppHandle, data: &Map<String, Value>) -> std::io::Result<()> {
    write_to(&overlay_path(app), data)
}

pub fn write_to(path: &std::path::Path, data: &Map<String, Value>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(data)?)
}
