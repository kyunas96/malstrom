# Track Categorization — Spec

Implementation spec for [track-categorization-prd.md](track-categorization-prd.md).

## Scope for v1

- **Primary: Live Database tag lookup.** For each track, resolve its
  clip/device sample file paths from the `.als`, look them up by filename
  in the local `Live-files-*.db` (`files` → `keywords` → `files` under
  `<keywords>`, see PRD investigation findings), and map the resolved tags
  to a category via a fixed tag → category table (e.g. `Kick`, `Drum Loop`
  → `Drums`; `Synth Bass`, `Sub` → `Bass`). First matching tag wins, same
  fixed priority order as the fallback below.
- **Fallback: case-insensitive substring match** of a track's Live name
  against each category's keyword list, used when no sample resolves in
  the DB (MIDI tracks, samples never touched via Live's browser). First
  matching category wins, in a fixed priority order (so "Bass Drums"
  doesn't ambiguously flip-flop — DRUMS checked before BASS, since a full
  drum bus is more likely than a literal instrument named that).
- **Category list (v1, extend as real libraries surface gaps):**
  - `Drums`: drum, drums, kit, kick
  - `Bass`: bass, sub
  - `Percussion`: perc, percussion
  - `Vocals`: vox, vocal, vocals
  - `Lead`: lead, melody
  - `Pads`: pad, pads, atmos
  - `Fx`: fx, sfx, riser
  - `Other`: fallback, no match
- **Surfaced now only where consumed.** No new main-table column in v1 —
  categorization ships as a backend capability + Mashup's grouped UI is the
  first consumer, per the PRD's open question. Revisit surfacing it in the
  main table once Mashup validates the taxonomy against real libraries.

## Backend (`src-tauri/src/als/`)

New module `categorize.rs`:

- `enum TrackCategory { Drums, Bass, Percussion, Vocals, Lead, Pads, Fx, Other }`
  — serializes to its variant name as a plain string for the frontend.
- `fn categorize_by_name(track_name: &str) -> TrackCategory` — the fallback:
  lowercases the name once, checks each category's keyword list in the
  fixed priority order above, returns the first match or `Other`. Pure
  function, no I/O — trivial to unit test directly.
- `fn categorize_by_db_tags(tags: &[String]) -> Option<TrackCategory>` — maps
  resolved Live Database tags to a category via the fixed tag table, same
  priority order. Pure function; `None` means no tag matched, caller falls
  through to `categorize_by_name`.
- New submodule `livedb.rs`: opens a `Live-files-*.db` at a caller-supplied
  path, and for a given sample filename returns its resolved keyword tags
  (`files` → `keywords` → `files` under `<keywords>`). Opens the connection
  in SQLite read-only mode (not just "no write calls" by convention),
  fresh per call — no long-lived connection or caching in v1. Does not
  search or guess the path — an invalid/missing path is an error the
  caller surfaces, falling through to `categorize_by_name` for every track.
  Filename isn't a unique key in `files` (e.g. a "Collect All and Save"
  copy gets its own `file_id` distinct from the original, and common
  sample-pack names collide across libraries) — a lookup can return
  multiple `file_id` rows. Union the keyword tags across all matching
  rows before handing them to `categorize_by_db_tags`, rather than picking
  one arbitrarily.
- `list_tracks` (introduced here, reused by
  [Mashup](mashup-v0-prd.md)) takes the project path plus an
  `Option<String>` Live DB path (frontend-supplied, e.g. from a settings
  field), walks the project's `<Track>` elements the same way
  `inspector.rs` already walks the document, resolves each track's sample
  filenames, tries `livedb` first when a DB path is given and falls back to
  `categorize_by_name`, and returns `Vec<TrackSummary>` where
  `TrackSummary { name, kind, category }`.

## Tauri command

- `list_tracks(path: String, live_db_path: Option<String>) -> Vec<TrackSummary>`
  — `live_db_path` is whatever the user has configured (see below); when
  `None`, categorization is name-only. Mashup's original design
  called this `mashup_list_tracks`; it's named plainly here since it's a
  general capability, not Mashup-specific.

## Live DB path setting

> **Shipped differently:** the setting became a Live Database *folder*, not
> a single file
> ([`70e2d42`](https://github.com/kyunas96/malstrom/commit/70e2d42)). Live
> keeps one `Live-files-*.db` per major version, so `list_tracks` tries
> every one in the folder and the first tag match wins.

- No filesystem search of any kind — malstrom never globs or walks the
  user's file tree looking for the DB. The user picks the file once via a
  native file-open dialog (Tauri's `dialog` plugin) in Settings, and the
  chosen path is persisted (same settings store malstrom already uses for
  other prefs) and passed to `list_tracks` on every call.
- Unset or invalid path: `livedb.rs` returns an error for that lookup,
  `list_tracks` catches it and falls through to `categorize_by_name` for
  every track — never a hard failure.
- No network calls anywhere in this feature — the project `.als` and the
  Live DB are both local file reads, nothing is sent anywhere.

## Tests

- Rust unit tests on `categorize_by_name` and `categorize_by_db_tags`
  directly — no fixtures needed since both are pure functions: assert
  "Drums 2" → `Drums`, "drum_bounce" → `Drums`, "Lead Vocal" → `Vocals`
  (checked before `Lead` in priority order — add to the fixed list above
  if this surprises you), "Synth 3" → `Other`; tags `["Kick"]` → `Drums`,
  `["Loop", "Drum Loop"]` → `Drums`, `[]`/no match → `None`.
- One integration test for `livedb` against a small fixture `.db` (not the
  real system DB — not portable to CI) covering the `files` →
  `keywords` → `files` join.

## Out of scope for v1

- Audio-content analysis (waveform/MIDI content) — the Live Database
  lookup is metadata, not signal analysis, so it stays in scope.
- Handling multiple installed Live versions' DBs — the user picks one file
  directly, so there's no "newest" to resolve.
- Main project table UI surfacing (deferred per PRD open question).
- Per-track manual correction — that's
  Track Category Overrides (not built yet), a separate
  dependency for Mashup, not part of this spec.
- User-editable/customizable keyword lists or tag mappings (the fixed rules
  themselves, as opposed to a per-track override — still out of scope).
