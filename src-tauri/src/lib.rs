// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

pub mod als;
mod commands;
pub mod store;

use commands::{
    apply_scale_to_project, list_projects, list_tracks, mashup_create, store_get, store_remove,
    store_set,
};
use store::StoreLock;

pub use commands::list_projects_in;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Leave a core free for the UI thread: saturating every core with rayon
    // workers can starve the OS scheduler for the app's main thread and show
    // up as a beachball even though nothing is actually deadlocked.
    let workers = std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).max(1))
        .unwrap_or(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .expect("failed to configure rayon thread pool");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(StoreLock::default())
        .invoke_handler(tauri::generate_handler![
            list_projects,
            apply_scale_to_project,
            list_tracks,
            mashup_create,
            store_get,
            store_set,
            store_remove
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
