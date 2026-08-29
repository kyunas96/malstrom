// Mirrors src-tauri/src/als/output_path.rs::sanitize_for_filename /
// derive_output_path, so the dialog's default matches what a Pull would
// have named the file anyway.
const UNSAFE_FILENAME_CHARS = /[/\\:*?"<>|]/g;

export function sanitizeForFilename(s: string): string {
  return s.replace(UNSAFE_FILENAME_CHARS, '-');
}

export function deriveDefaultFileName(
  projectName: string,
  root_name: string,
  scale_name: string,
): string {
  const label = sanitizeForFilename(`${root_name} ${scale_name}`);
  return `${projectName} (${label}).als`;
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

// Mirrors src-tauri's named_output_path: appends the original extension when
// the typed name omits it, so "My File" and "My File.als" are recognized as
// the same collision.
function resolveNamedFileName(fileName: string, originalPath: string): string {
  const sanitized = sanitizeForFilename(fileName.trim());
  const ext = baseName(originalPath).split('.').pop() ?? 'als';
  return sanitized.toLowerCase().endsWith(`.${ext.toLowerCase()}`) ? sanitized : `${sanitized}.${ext}`;
}

/** True when `fileName` would resolve to the same file as `originalPath` — saving there would silently overwrite the original despite "save as new file". */
export function collidesWithOriginal(fileName: string, originalPath: string): boolean {
  return resolveNamedFileName(fileName, originalPath).toLowerCase() === baseName(originalPath).toLowerCase();
}
