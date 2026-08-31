use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

const UNSAFE_FILENAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

pub fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| if UNSAFE_FILENAME_CHARS.contains(&c) { '-' } else { c })
        .collect()
}

/// Derives a new, non-colliding output path alongside `original`, named
/// after the applied scale, e.g. `MyProject (D Minor).als`.
pub fn derive_output_path(original: &Path, root_name: &str, scale_name: &str) -> Result<PathBuf> {
    let dir = original.parent().unwrap_or_else(|| Path::new("."));
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let ext = original
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("als");
    let label = sanitize_for_filename(&format!("{root_name} {scale_name}"));

    for suffix in 0..1000 {
        let candidate_name = if suffix == 0 {
            format!("{stem} ({label}).{ext}")
        } else {
            format!("{stem} ({label}) ({}).{ext}", suffix + 1)
        };
        let candidate = dir.join(candidate_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!("could not find an available filename"))
}

/// Builds an output path alongside `original` using a user-chosen file name,
/// sanitizing it and appending `original`'s extension when the name doesn't
/// already end with it. Unlike `derive_output_path`, this never auto-suffixes
/// to avoid a collision -- the name came from a confirmation dialog the user
/// already saw, so saving over an existing file there is expected "save as"
/// behavior, not a surprise.
fn named_output_path(original: &Path, file_name: &str) -> PathBuf {
    let dir = original.parent().unwrap_or_else(|| Path::new("."));
    let ext = original
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("als");
    let sanitized = sanitize_for_filename(file_name.trim());
    let named = if sanitized.to_lowercase().ends_with(&format!(".{}", ext.to_lowercase())) {
        sanitized
    } else {
        format!("{sanitized}.{ext}")
    };
    dir.join(named)
}

/// Decides where (if anywhere) an applied scale should be written:
/// `None` when no clip needed changing (nothing to write); `src_path` itself
/// when `overwrite` is true; otherwise a path alongside it -- named after
/// `custom_file_name` when the caller supplied one (from the "pull as new
/// file" dialog), or freshly derived from the scale otherwise.
pub fn resolve_output_path(
    src_path: &Path,
    root_name: &str,
    scale_name: &str,
    overwrite: bool,
    total_touched: u32,
    custom_file_name: Option<&str>,
) -> Result<Option<PathBuf>> {
    if total_touched == 0 {
        return Ok(None);
    }
    if overwrite {
        return Ok(Some(src_path.to_path_buf()));
    }
    match custom_file_name {
        Some(file_name) => Ok(Some(named_output_path(src_path, file_name))),
        None => derive_output_path(src_path, root_name, scale_name).map(Some),
    }
}

/// Backs up `src_path` into its project's `Backup/` folder before an
/// in-place overwrite, matching Ableton Live's own backup convention
/// (`{stem} [YYYY-MM-DD HHMMSS].{ext}`, byte-for-byte copy). Refuses rather
/// than guessing when `Backup/` is missing or unwritable, since that means
/// this isn't a Live-managed project folder.
pub fn backup_before_overwrite(src_path: &Path) -> Result<PathBuf> {
    let project_dir = src_path.parent().unwrap_or_else(|| Path::new("."));
    let backup_dir = project_dir.join("Backup");

    if !backup_dir.is_dir() {
        return Err(anyhow!(
            "No Backup/ folder found next to {} — this doesn't look like a Live-managed project folder, refusing to write.",
            src_path.display()
        ));
    }

    // Existence isn't enough (e.g. a synced/read-only folder) -- probe with
    // a real write so a permission problem surfaces here, not mid-copy.
    let probe = backup_dir.join(".malstrom-write-check");
    std::fs::write(&probe, b"").map_err(|e| {
        anyhow!("Backup/ folder at {} is not writable ({e}) — refusing to write.", backup_dir.display())
    })?;
    let _ = std::fs::remove_file(&probe);

    let stem = src_path.file_stem().and_then(|s| s.to_str()).unwrap_or("project");
    let ext = src_path.extension().and_then(|s| s.to_str()).unwrap_or("als");
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
    let backup_path = backup_dir.join(format!("{stem} [{timestamp}].{ext}"));

    std::fs::copy(src_path, &backup_path)
        .map_err(|e| anyhow!("Failed to back up {} to {}: {e}", src_path.display(), backup_path.display()))?;

    Ok(backup_path)
}

/// Gzip-compresses `xml` and writes it to `dest_path`, matching how `.als`
/// files are stored on disk.
pub fn write_als(xml: &str, dest_path: &Path) -> Result<()> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let file = std::fs::File::create(dest_path)?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(xml.as_bytes())?;
    encoder.finish()?;
    Ok(())
}
