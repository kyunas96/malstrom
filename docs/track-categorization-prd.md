# Track Categorization — PRD

## Problem

malstrom can tell you a project's scale, but not what's _in_ it. Before
[Mashup](mashup-v0-prd.md) can show "here's what you can pull from each
project," malstrom needs some way to make sense of a project's tracks —
otherwise the Mashup form is just a flat, meaningless list of whatever
names the producer happened to type. This is a prerequisite for Mashup, not
part of it: it's useful on its own (e.g. surfacing categories in the main
table) and Mashup is only the first consumer.

## Goal

Derive a coarse category (DRUMS, BASS, PERCUSSION, VOCALS, etc.) for each
track in a project, based on how the track is named in Live, so malstrom
can group and present tracks coherently instead of as a raw name list.

## Non-goals

- Audio content analysis (detecting a drum track by its waveform/MIDI
  content). This is naming-convention detection only.
- Renaming or modifying tracks in source files.
- A fully exhaustive category taxonomy — a fixed, extensible keyword list
  is enough; not everything will match.

## Approach

Primary signal: look up a track's clips/devices against Ableton's own Live
Database, a `Live-files-*.db` file under
`~/Library/Application Support/Ableton/Live Database/`. The user supplies
the path to this file via a native file picker — malstrom never scans or
walks the filesystem to find it (Live keeps one per major version, and the
folder layout isn't guaranteed stable across installs/versions, so guessing
isn't reliable, and searching the user's file tree isn't something this app
should do). It auto-tags every sample Live's browser has ever
touched (see
Investigation findings below) with coarse content tags (`Kick`, `Bass`,
`Loop`, `Solo Voice`, etc.). Map those tags to our category list.

Fallback: for anything the DB doesn't resolve (samples imported outside
Live's browser, MIDI tracks with no sample to look up), fall back to
matching the track's Live name, case-insensitively, against a fixed set of
category keywords.

Either path, no match falls back to an `Other` category — the PRD
explicitly accepts that not everything fits cleanly, so `Other` is a first-
class outcome, not an error state.

### Investigation findings (2026-08-31)

Spiked directly against the local Live Database to de-risk this before
committing to it as the primary signal:

- Keywords aren't a side table — they're rows in `files` itself, under a
  synthetic `<keywords>` root (e.g. `Sounds/Bass`, `Drums/Kick`). A content
  file's tags come via the `keywords` join table, resolved back through
  `files`.
- Coverage is high for the actual workflow (pulling samples via Live's
  browser): confirmed both factory-pack content and the user's own
  imported/third-party samples get auto-tagged (`is_auto=1`) the moment
  they're used, not just curated Ableton Packs.
- "Collect All and Save" copies (`Samples/Imported/...`) are indexed and
  tagged too, as their own `file_id`, distinct from the original library
  file — so lookup by filename inside the project's own folder works
  without needing to resolve the pre-collection path.
- Auto-tags are coarse (`Loop`, `One Shot`, `Drum Loop`, `Kick`) — a good
  match for this PRD's coarse category goal, not for finer detail.
- Real gap: content imported some way other than Live's browser (manual
  file copy, another DAW's export) has no `files` row at all — nothing to
  join against, hence the name-keyword fallback above.

## Success criteria

- Given a project with conventionally-named tracks ("Drums", "Bass",
  "Lead Vocal"), malstrom reports the right category for each.
- A track with an idiosyncratic name doesn't crash or get miscategorized
  into something misleading — it lands in `Other`.
- The category list is a single place to edit, not scattered logic, so
  adding a keyword later is a one-line change.

## Open questions (to resolve in the spec)

- How to resolve a track's clips/devices to sample file paths from the
  `.als` XML, and handle duplicate filenames across multiple DB `file_id`
  matches.
- Exact tag → category mapping, and exact keyword list/match rules for the
  name-based fallback.
- Where this lives in the data model returned to the frontend.
- Whether/how this surfaces in the main project table now, vs. only inside
  Mashup for now.

## Follow-on

Resolved into [track-categorization-spec.md](track-categorization-spec.md).
[Mashup](mashup-v0-prd.md) depends on this landing first.
