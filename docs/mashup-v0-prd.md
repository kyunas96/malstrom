# Mashup v0 — PRD

## Outcome

The merge backend shipped in
[`5aede61`](https://github.com/kyunas96/malstrom/commit/5aede61): it
merges chosen tracks from two projects into a new set that opens in Live.
The picker UI described below was built and then taken back out before
merging. Every bug so far was in the XML merge, none in the UI, so
testing through a checkbox form was pure overhead. The backend is now
driven by the `mashup` and `mashup_batch_test` command-line tools instead.

As for this PRD's real question, whether I'd actually use it: I used it
through the CLI on real projects, and that convinced me it was worth doing
properly.

Building it showed that "combine two projects" has constraints that can't
just be copied over (one tempo, return tracks that have to line up), so
the next step became a staged plan rather than this PRD's UI. See
[Mashup: constraints and stages](mashup-constraints-and-stages.md). The
rest of this doc is the plan as written before building.

## Purpose

Not a smaller version of the full Mashup design — a deliberately crude
build meant to answer one question: does pulling tracks from
scale-compatible projects into a new file get used, at all, once it
exists? Everything here is chosen to make that answer available as
cheaply as possible, not to be a good long-term UX.

If v0 gets used: invest in the full category-first cart, provenance
badges, preview, tempo reconciliation, etc.
If it doesn't: the full design never needed to be built, and the
question got answered in a fraction of the time.

## Why this doesn’t need to wait for the rest of the roadmap

My roadmap at the time blocked Mashup behind App Overlay Storage, Track
Category Overrides, Scan Cache, and Recursive Detection. Every one of
those dependencies traces back to a feature v0 explicitly cuts:

- **Overrides / Overlay Storage** are only needed because the full
  design groups by category and lets you fix wrong categorization
  inline. v0 doesn't group by category at all (see Scope) — so a
  track sitting in `Other`, or miscategorized, is cosmetically
  unfortunate but not blocking.
- **Scan Cache / Recursive Detection** are only needed for
  cross-folder, multi-year matching. v0 stays scoped to whatever's
  already in the current scanned/filtered folder — same reach as
  every other feature in the app today.

Track Categorization itself (already merged) is the only real
prerequisite, because it's what supplies `category` in `list_tracks`
now. v0 doesn't need it as a *grouping* mechanism, just as an existing
field it's fine to ignore.

**Net effect: v0 can be built now, independent of those four features, and
doesn’t need a slot in the roadmap at all.**

## Problem / Goal

Unchanged from the original Mashup design: once a library is filtered by scale,
there's no way to pull pieces from several compatible projects into
one new project.

## Scope

**In:**
- Scale filter → select **exactly 2** projects (not "two or more" —
  caps combinatorics and test surface for a throwaway build)
- Whole tracks only (matches the standing decision in both specs)
- One section per selected project, plain checklist of track names
  (the original design's flow, without category grouping)
- Filename field + Create
- Overwrite-refusal on the output filename (already specced
  elsewhere in the app — reuse it)

**Explicitly cut (would be the first things added back if v0 earns
its keep):**
- Category-first grouping, or grouping by category at all
- Inline recategorize
- Any preview (audio playback or MIDI thumbnail)
- Tempo/time-signature reconciliation UI
- Provenance/confidence badges
- Pre-Create warnings/review screen
- >2 project selection

## Known limitations — surfaced, not hidden

Two things v0 does not solve but must not silently break either:

- **Tempo/time signature:** first selected project's values win,
  same as the original design's stated limitation. Show it as a plain
  one-line fact next to the filename field ("Using tempo/time
  signature from [Project A]") — cheap, and prevents mismatched
  output from reading as a bug.
- **Groove pool:** a clip's groove points at an entry in its own
  project's `<GroovePool>`, a document-level resource v0 doesn't
  merge. Groove assignments on the non-base project's tracks are
  dropped (set to "none") rather than left dangling or pointed at an
  unrelated groove in the base project's pool. Show a static line
  under the track list naming the affected project.
- **External sample references:** v0 does not copy or rewrite sample
  paths. Show a static line under the track list: "Tracks referencing
  audio files outside this project's folder may not play back
  correctly in the output." This is a known-broken edge case, not a
  silent one — the difference matters for whether v0's results are
  trustworthy enough to judge the workflow by.

## Success criteria

- A user can go from "these 2 projects are in D Dorian" to a new
  `.als` with chosen tracks from both, without leaving Malstrom.
- Output opens cleanly in Ableton Live for the case where no track
  references external samples.
- Source project files are never modified (verify via mtime/hash
  test, same as the full spec's checkpoint).
- Informal usage check after a couple weeks: did you reach for it
  unprompted, or did it sit unused? That answer is the actual
  deliverable of v0 — decide what to build next based on it, not on
  how the build went.