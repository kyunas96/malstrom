# Automatic Backup for In-Place `.als` Writes — Spec

malstrom already offers writing changes back into a project's `.als` file,
behind a confirmation modal. That's a high-risk operation — a `.als` is a
user's actual project, not a derived artifact — so every in-place write
must be preceded by an automatic backup, no opt-out.

## Backup format

Match Ableton Live's own backup convention exactly, so a user recovering
manually sees something familiar and can open it directly in Live:

- **Location**: `Backup/` inside the project folder (an Ableton "Project"
  folder — Live designates these itself, `.als` is always at the project
  root with `Backup/`, `Samples/`, etc. alongside it).
- **Filename**: `{original basename, no extension} [{YYYY-MM-DD HHMMSS}].als`
  — space before the bracket, hyphens in the date, no colons in the time
  (colons aren't filename-safe).
- **Content**: byte-for-byte copy of the pre-write file. `.als` is already
  a gzip stream — the backup is just that stream copied, not re-encoded.

## Preconditions — stop, don't guess

Before any in-place write:

1. `Backup/` must already exist inside the project folder. If it doesn't,
   this isn't a Live-managed project folder (or an unusual layout) —
   refuse the write rather than creating the folder or picking somewhere
   else to put a backup.
2. `Backup/` must be writable. Check this alongside existence, not only
   after attempting the copy — a synced/read-only `Backup/` should hit the
   same stop path as a missing one, not fail mid-write.

On either failure: refuse the write, surface the specific reason (missing
vs. unwritable) in the confirmation modal/error — never a silent no-op.

## Write sequence

1. Validate preconditions above.
2. Copy the current `.als` bytes to the timestamped backup path in
   `Backup/`.
3. Only then perform the in-place write to the original `.als`.

If step 2 fails partway (disk full, permissions change mid-copy), abort
before step 3 — never write to the original without a confirmed-complete
backup.

## Out of scope

- Pruning old backups (Live's own `Backup/` already accumulates over
  time; this feature adds to that pattern rather than managing it).
- Any backup mechanism for non-Live-managed folders — those are refused
  per the preconditions above, not handled some other way.
