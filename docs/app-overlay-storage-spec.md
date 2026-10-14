# App Overlay Storage — Spec

Implementation spec for [app-overlay-storage-prd.md](app-overlay-storage-prd.md).

## File

- Location: `<app_config_dir>/store.json` (Tauri v2 `app.path().app_config_dir()`
  — same directory class the app would use for any other config, no new
  concept). Created on first write if missing.
- Shape: `{ "<namespace>": { "<key>": <any JSON value> } }` — two-level
  object, no envelope beyond that.
- Missing or empty file: treated as `{}`, no error surfaced to the caller.
- Corrupt (fails to parse): back up the bad file next to itself as
  `store.json.bak-<unix-timestamp>` (same rename-don't-delete instinct as
  the existing `.als` backup-before-write behavior) and continue with `{}`
  in memory. Next write replaces the file with valid JSON. Never a hard
  failure — a corrupt overlay file must not block the app from opening a
  project.
- No format version field. A namespace's shape is that feature's problem to
  evolve (e.g. read old shape defensively, or move to a new key name like
  `trackCategoryOverridesV2` if a change is truly incompatible) —
  versioning the whole file for a single-namespace shape change is
  speculative machinery for a problem that hasn't occurred yet.

## Write strategy

Whole-file read-modify-write on every command, guarded by a single
`tokio::sync::Mutex<()>` held in Tauri managed state so concurrent command
invocations serialize instead of racing on the file. The file is small
(a handful of small records) — there's no case here for granular/partial
writes.

## Tauri commands

Generic, not per-namespace, so a new feature never needs a new command:

- `store_get(namespace: String, key: String) -> Option<serde_json::Value>`
- `store_set(namespace: String, key: String, value: serde_json::Value)`
- `store_remove(namespace: String, key: String)`

All three take the mutex, read the file (or `{}` per above), operate on
`json[namespace][key]`, and (for set/remove) write the file back.
`store_get` doesn't need the file rewritten, only read under the lock so
it doesn't race a concurrent writer.

## Module

New `src-tauri/src/store.rs`:

- `fn store_path(app: &AppHandle) -> PathBuf`
- `fn read(app: &AppHandle) -> serde_json::Map<String, Value>` — handles
  missing/empty/corrupt per above.
- `fn write(app: &AppHandle, data: &Map<String, Value>) -> io::Result<()>`
- The three Tauri commands, each `async fn` taking `State<Mutex<()>>` (this
  project's threading convention: commands default to `async fn` even for
  quick file I/O).

> **Shipped differently** ([#10](https://github.com/kyunas96/malstrom/pull/10)):
> the three commands live in `commands.rs` with the other Tauri commands,
> so `store.rs` holds only storage logic (`read`/`write`/`set`/`remove`).
> Writes go to a temp file and are then renamed, so a crash can't leave a
> truncated store. `store_remove` also drops a namespace once its last key
> is gone.

Frontend never touches the file path or format directly — same boundary
`list_tracks`/`.als` parsing already keep.

## Out of scope

- Migrating `liveDbFolder` off `localStorage` (per PRD non-goals).
- Any specific namespace's data shape — first consumer is
  [track category overrides](https://github.com/kyunas96/malstrom/issues/8).

## Tests

- Rust unit tests on `read`/`write` against a temp dir: missing file → `{}`,
  corrupt file → `{}` plus a `.bak-*` file created, round-trip a value
  through set → get → remove.

> **Shipped differently:** these are integration tests in
> `src-tauri/tests/store.rs`, like `tests/backup.rs`, not inline unit tests.
> They also cover invalid UTF-8 (treated as corrupt) and a namespace that
> holds a non-object value.
