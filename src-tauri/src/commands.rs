use rayon::prelude::*;
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::Emitter;

use crate::als::inspector::AlsInspector;
use crate::als::output_path;
use crate::als::scale_candidates::ScaleCandidate;
use crate::als::scale_constants::{NOTE_NAMES, SCALE_NAMES};

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
    /// The written file's fresh scale candidates, so the frontend can patch
    /// its cached project list instead of rescanning the whole folder. Only
    /// set when a file was actually written (`new_path.is_some()`).
    pub updated_scales: Option<Vec<ScaleCandidate>>,
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
        let mut updated_scales = None;
        let new_path = match &dest_path {
            Some(dest_path) => {
                output_path::write_als(&new_xml, dest_path).map_err(|e| e.to_string())?;
                updated_scales = Some(
                    AlsInspector::from_xml(new_xml)
                        .extract_scale_candidates()
                        .map_err(|e| e.to_string())?
                        .scales,
                );
                Some(dest_path.to_string_lossy().to_string())
            }
            None => None,
        };

        Ok(AppliedScaleResult {
            new_path,
            updated_scales,
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
