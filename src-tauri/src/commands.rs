use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{AppHandle, Emitter, State};

use crate::als::categorize::TrackSummary;
use crate::als::inspector::AlsInspector;
use crate::als::mashup;
use crate::als::output_path;
use crate::als::scale_candidates::ScaleCandidate;
use crate::als::scale_constants::{NOTE_NAMES, SCALE_NAMES};
use crate::store::{self, StoreLock};

#[derive(Debug, Clone, Serialize)]
pub struct AlsProjectSummary {
    pub path: String,
    pub name: String,
    pub scales: Vec<ScaleCandidate>,
}

fn is_als_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("als")
}

/// Emitted to the frontend as each project file finishes processing so a
/// long scan can show progress instead of an indefinite spinner.
#[derive(Clone, Serialize)]
pub struct ListProjectsProgress {
    pub completed: usize,
    pub total: usize,
}

/// Lists the .als projects directly inside `root_path` (no recursion into
/// subfolders) along with each project's compatible scale candidates.
/// Emits a `list-projects-progress` event after each file finishes.
// Plain `fn` commands run on the main thread; only `async fn` commands are
// handed off by Tauri to its async runtime. So the CPU-bound work below is
// wrapped in `spawn_blocking` and awaited, keeping the main thread free to
// keep pumping the event loop (window redraws, IPC) while it runs.
#[tauri::command]
pub async fn list_projects(
    window: tauri::Window,
    root_path: String,
) -> Result<Vec<AlsProjectSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        list_projects_in_with_progress(root_path, |completed, total| {
            let _ =
                window.emit("list-projects-progress", ListProjectsProgress { completed, total });
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Result of applying a scale to a project, returned to the frontend.
#[derive(Serialize)]
pub struct AppliedScaleResult {
    /// `None` when no clip needed changing, in which case no file was written.
    pub new_path: Option<String>,
    pub clips_changed: u32,
    pub clips_created: u32,
    pub clips_corrected: u32,
    pub clips_already_set: u32,
    pub clips_incompatible: u32,
    /// True when the project predates Ableton's per-clip Scale feature, so
    /// any newly created `ScaleInformation` blocks may not actually be
    /// honored by Ableton until the project is resaved there once.
    pub schema_predates_clip_scale: bool,
    /// The written file's fresh summary (path/name/scales), so the frontend
    /// can patch or insert its cached project list instead of rescanning the
    /// whole folder. Only set when a file was actually written
    /// (`new_path.is_some()`); covers both an in-place overwrite and a
    /// "save as new file" copy.
    pub updated_project: Option<AlsProjectSummary>,
}

/// Writes `root_name`/`scale_name` onto every eligible MIDI clip in `path`.
/// When `overwrite` is false (the default the frontend uses), the result is
/// saved as a new file alongside the original and the original is never
/// touched; when true, `path` itself is overwritten in place. `new_file_name`
/// (ignored when `overwrite` is true) names that new file, from the "pull as
/// new file" confirmation dialog; `None` or blank falls back to a name
/// derived from the scale.
#[tauri::command]
pub async fn apply_scale_to_project(
    path: String,
    root_name: String,
    scale_name: String,
    overwrite: bool,
    new_file_name: Option<String>,
) -> Result<AppliedScaleResult, String> {
    let new_file_name = new_file_name.filter(|s| !s.trim().is_empty());
    tauri::async_runtime::spawn_blocking(move || {
        let root_note = NOTE_NAMES
            .iter()
            .position(|&n| n == root_name)
            .ok_or_else(|| format!("Unknown root note {root_name}"))? as i32;
        let scale_index = SCALE_NAMES
            .iter()
            .position(|&n| n == scale_name)
            .ok_or_else(|| format!("Unknown scale {scale_name}"))? as i32;

        let src_path = std::path::Path::new(&path);
        let inspector = AlsInspector::open(src_path).map_err(|e| e.to_string())?;
        let (new_xml, outcome) = inspector
            .apply_scale(root_note, scale_index)
            .map_err(|e| e.to_string())?;

        let total_touched = outcome.clips_changed + outcome.clips_created + outcome.clips_corrected;
        let dest_path = output_path::resolve_output_path(
            src_path,
            &root_name,
            &scale_name,
            overwrite,
            total_touched,
            new_file_name.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        // A custom name that happens to match the original file would
        // silently overwrite it despite `overwrite: false` -- refuse rather
        // than clobber the source.
        if !overwrite && dest_path.as_deref() == Some(src_path) {
            return Err("That name matches the original file — choose a different name to save as a new file.".to_string());
        }
        let mut updated_project = None;
        let new_path = match &dest_path {
            Some(dest_path) => {
                if overwrite {
                    // Overwriting the source is high-risk -- back it up first,
                    // and never touch the original if the backup didn't
                    // fully succeed.
                    output_path::backup_before_overwrite(src_path).map_err(|e| e.to_string())?;
                }
                output_path::write_als(&new_xml, dest_path).map_err(|e| e.to_string())?;
                let scales = AlsInspector::from_xml(new_xml)
                    .extract_scale_candidates()
                    .map_err(|e| e.to_string())?
                    .scales;
                let name = dest_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                updated_project = Some(AlsProjectSummary {
                    path: dest_path.to_string_lossy().to_string(),
                    name,
                    scales,
                });
                Some(dest_path.to_string_lossy().to_string())
            }
            None => None,
        };

        Ok(AppliedScaleResult {
            new_path,
            updated_project,
            clips_changed: outcome.clips_changed,
            clips_created: outcome.clips_created,
            clips_corrected: outcome.clips_corrected,
            clips_already_set: outcome.clips_already_set,
            clips_incompatible: outcome.clips_incompatible,
            schema_predates_clip_scale: outcome.schema_predates_clip_scale,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Lists `path`'s tracks with a derived category each. `live_db_folder` is
/// whatever the user has configured in Settings; every `Live-files-*.db`
/// inside it is tried in turn, and when unset (or empty) categorization is
/// name-only.
#[tauri::command]
pub async fn list_tracks(
    path: String,
    live_db_folder: Option<String>,
) -> Result<Vec<TrackSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let candidates = live_db_folder
            .map(|folder| crate::als::livedb::find_db_files(std::path::Path::new(&folder)))
            .unwrap_or_default();
        let inspector = AlsInspector::open(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
        inspector.extract_tracks(&candidates).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One source project's path plus the positions of the tracks to pull from
/// it, as picked in the Mashup v0 form's checklist -- indices into the same
/// order `list_tracks` returns for that project.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MashupSourceInput {
    pub path: String,
    pub track_indices: Vec<usize>,
}

/// Builds a Mashup v0 output file: merges the checked tracks from exactly 2
/// source projects into a new `.als` alongside `sources[0]`, which also
/// donates tempo/time signature and the master track (documented
/// limitation, not solved in v0). Never touches either source file.
/// `file_name` is refused if it would collide with an existing file --
/// same "ask for a different name" behavior as `apply_scale_to_project`.
#[tauri::command]
pub async fn mashup_create(sources: Vec<MashupSourceInput>, file_name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if sources.len() != 2 {
            return Err("Mashup v0 supports exactly 2 source projects".to_string());
        }

        let mashup_sources = sources
            .iter()
            .map(|s| {
                let xml = AlsInspector::open(std::path::Path::new(&s.path))
                    .map_err(|e| e.to_string())?
                    .raw_xml()
                    .to_string();
                Ok(mashup::MashupSource {
                    xml,
                    track_indices: s.track_indices.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let merged_xml = mashup::build_mashup(&mashup_sources).map_err(|e| e.to_string())?;

        let base_path = std::path::Path::new(&sources[0].path);
        let dest_path = output_path::named_output_path(base_path, &file_name);
        if dest_path.exists() {
            return Err(format!(
                "{} already exists — choose a different name.",
                dest_path.display()
            ));
        }
        output_path::write_als(&merged_xml, &dest_path).map_err(|e| e.to_string())?;
        Ok(dest_path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reads `store.json[namespace][key]`, or `None` if either is absent.
#[tauri::command]
pub async fn store_get(
    app: AppHandle,
    state: State<'_, StoreLock>,
    namespace: String,
    key: String,
) -> Result<Option<Value>, String> {
    let _guard = state.0.lock().await;
    let data = store::read(&app);
    Ok(data.get(&namespace).and_then(|ns| ns.get(&key)).cloned())
}

/// Writes `store.json[namespace][key] = value`, creating the namespace if
/// it doesn't exist yet.
#[tauri::command]
pub async fn store_set(
    app: AppHandle,
    state: State<'_, StoreLock>,
    namespace: String,
    key: String,
    value: Value,
) -> Result<(), String> {
    let _guard = state.0.lock().await;
    let mut data = store::read(&app);
    store::set(&mut data, namespace, key, value)?;
    store::write(&app, &data).map_err(|e| e.to_string())
}

/// Removes `store.json[namespace][key]`. Prunes `namespace` too if that
/// was its last entry, rather than leaving a dangling empty object.
#[tauri::command]
pub async fn store_remove(
    app: AppHandle,
    state: State<'_, StoreLock>,
    namespace: String,
    key: String,
) -> Result<(), String> {
    let _guard = state.0.lock().await;
    let mut data = store::read(&app);
    store::remove(&mut data, &namespace, &key);
    store::write(&app, &data).map_err(|e| e.to_string())
}

pub fn list_projects_in(root_path: String) -> Result<Vec<AlsProjectSummary>, String> {
    list_projects_in_with_progress(root_path, |_, _| {})
}

fn list_projects_in_with_progress(
    root_path: String,
    on_progress: impl Fn(usize, usize) + Sync,
) -> Result<Vec<AlsProjectSummary>, String> {
    let root = std::path::Path::new(&root_path);
    let entries = std::fs::read_dir(root).map_err(|e| e.to_string())?;

    let mut als_paths: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_als_file(path))
        .collect();
    als_paths.sort();

    let total = als_paths.len();
    let completed = AtomicUsize::new(0);

    als_paths
        .into_par_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let scales = AlsInspector::open(&path)
                .and_then(|inspector| inspector.extract_scale_candidates())
                .map_err(|e| e.to_string())?
                .scales;

            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            on_progress(done, total);

            Ok(AlsProjectSummary {
                path: path.to_string_lossy().to_string(),
                name,
                scales,
            })
        })
        .collect()
}
