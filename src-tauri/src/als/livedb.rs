use anyhow::{anyhow, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Finds every `Live-files-*.db` directly inside `folder` -- Ableton keeps
/// one such file per installed Live major version, so a user's projects can
/// span several. Sorted for deterministic lookup order. A missing or
/// unreadable folder yields an empty list rather than an error, so the
/// caller naturally falls back to name-based categorization.
pub fn find_db_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("Live-files-") && name.ends_with(".db"))
        })
        .collect();
    paths.sort();
    paths
}

/// Looks up `filename`'s auto-tags in a Live Database (`Live-files-*.db`),
/// opened read-only at `db_path`. Tags are rows in `files` themselves
/// (e.g. "Kick", "One Shot"), joined to a content file via the `keywords`
/// table (`keywords.file_id` -> content, `keywords.keyw_id` -> tag row).
///
/// Filename isn't a unique key in `files` -- a "Collect All and Save" copy
/// gets its own `file_id` distinct from the original, and common
/// sample-pack names collide across libraries -- so a lookup can match
/// multiple content rows; `SELECT DISTINCT` unions their tags rather than
/// picking one content row arbitrarily.
pub fn lookup_tags(db_path: &Path, filename: &str) -> Result<Vec<String>> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| anyhow!("failed to open Live Database at {}: {e}", db_path.display()))?;

    let mut stmt = conn.prepare(
        "SELECT DISTINCT tag.name
         FROM files content
         JOIN keywords k ON k.file_id = content.file_id
         JOIN files tag ON tag.file_id = k.keyw_id
         WHERE content.name = ?1",
    )?;

    let tags = stmt
        .query_map([filename], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tags)
}
