# App Overlay Storage — PRD

## Problem

malstrom is starting to accumulate data that isn't part of a Live project
and doesn't belong in it: [track category overrides](https://github.com/kyunas96/malstrom/issues/8)
are the first case, but they won't be the last — grouping/layout choices
for Mashup's picker are a predictable next one. Today the
app has exactly one piece of persisted app state (`liveDbFolder`), and it
lives in browser `localStorage`, keyed globally, invisible to the Rust side
where `.als` parsing and categorization actually run. That's fine for a
single global setting; it's the wrong shape for data keyed to a specific
project or track, and every feature that needs this kind of storage
shouldn't have to re-decide where it lives and how it's keyed from
scratch.

## Goal

Give malstrom one place to persist small amounts of app-owned data that
overlays on top of a Live project — corrections, preferences, anything
derived that the user has explicitly set — without touching the source
`.als` file, and expose it through a single storage boundary that any
feature (starting with track category overrides) can read and write
through.

## Non-goals

- A general-purpose database, query engine, or ORM. This is key→value
  storage for a handful of small, independently-keyed records, not
  relational data — see [Approach](#approach) for why.
- Caching or storing anything recomputable/derived-on-demand (e.g. scan
  results, categorization output). This is for data with no other source
  of truth — a user's explicit correction — not a performance layer. A
  scan/parse cache is a separate, sibling concern with a different
  lifetime (invalidated by file mtime, safe to discard and rebuild) and
  likely a different backend (SQLite, for path+mtime lookups); it should
  get its own PRD rather than share this one's file or format.
- Migrating the existing `liveDbFolder` `localStorage` setting. It's a
  single global value with no per-project/per-track keying; there's
  nothing broken about it and no reason to touch it here.
- Sync, multi-device, or multi-user concerns. This is local storage for a
  local desktop app.
- Any specific feature's data shape (what a track override record looks
  like). That's [track category overrides](https://github.com/kyunas96/malstrom/issues/8)'s
  concern; this PRD only defines the storage mechanism it sits on.

## Approach

A single JSON file, written and read from the Rust side, living in the
app's platform config directory (`tauri::api::path::app_config_dir` or
equivalent) — not browser `localStorage`, so it's reachable from Rust
where the data will actually be consumed (categorization, `.als`
processing), and not SQLite, because the data doesn't need relational
queries: every consumer so far is a flat key→value lookup ("does this
track have an override," "what's this element's saved position"), and a
JSON file needs no schema migrations, no bundled query engine, nothing to
initialize.

Structure: one file, top-level keyed by a namespace per feature (e.g.
`trackCategoryOverrides`, later `mashupLayouts`), so unrelated features
don't collide on keys and each feature owns its own sub-shape without
needing its own file. Exposed to the frontend via Tauri commands — the
frontend never reads/writes the file directly, the same way it doesn't
parse `.als` XML itself.

If a future feature's overlay data (user-authored, non-recomputable)
outgrows flat key→value, that's the signal to introduce `rusqlite` for
overlay storage too — it's already a dependency for reading Live's own DB.
Not needed for what's known today. Note this is separate from whether a
scan/parse cache uses SQLite (a likely-yes for that PRD, for different
reasons — see Non-goals); the two would not necessarily share a database
even if both end up on `rusqlite`.

## Success criteria

- Track category overrides (and any future overlay feature) read and
  write through one shared storage module instead of inventing their own
  file/format.
- The storage file survives app restarts and is not affected by
  re-scanning or re-opening a project.
- Adding a second feature's overlay data doesn't require a new file, a new
  settings mechanism, or touching the first feature's code.
- No new external dependency (no bundled DB server, no new crate) for this
  PRD's scope.

## Open questions (to resolve in the spec)

- Exact file location and name, and what happens on read if the file is
  missing, empty, or corrupt (fresh install vs. a bad write).
- Write strategy: read-modify-write whole file per change vs. something
  more granular, and whether concurrent writes from multiple frontend
  calls need any guarding (single-user desktop app, but Tauri commands can
  run concurrently).
- Exact Tauri command surface (get/set/remove per namespace, or a more
  generic interface).
- Versioning the file format itself, in case a namespace's shape changes
  later and old data needs to still load.

## Follow-on

Resolved in [app-overlay-storage-spec.md](app-overlay-storage-spec.md).
[Track category overrides](https://github.com/kyunas96/malstrom/issues/8) should
depend on this landing first, rather than inventing its own storage.
