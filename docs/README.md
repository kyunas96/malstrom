# Design docs

These are the working docs behind Malstrom: what each feature was meant to
do, what I found out while building it, and what changed along the way.
They were written as I went, so they show the thinking at the time rather
than a polished spec. Plans for things that haven't been built yet live in
the [issues](https://github.com/kyunas96/malstrom/issues) instead.

Listed in the order they happened.

## 1. Filtering the project list

Malstrom started as a table of Ableton projects with their inferred scales,
so you could find projects that would sit well together. The first addition
was a plain filter by file name.

- [Filename filter: spec](filename-filter-spec.md). Shipped in
  [`a1a2658`](https://github.com/kyunas96/malstrom/commit/a1a2658).

## 2. Never damaging a real project

Applying a scale can write back into a project's `.als` file, which is
someone's actual work, not a derived file. Before going further, every
in-place write got a backup in Live's own `Backup/` format, and the app
refuses to write if it can't make one.

- [Backup before overwrite: spec](als-backup-spec.md). Shipped in
  [`b62144e`](https://github.com/kyunas96/malstrom/commit/b62144e).

## 3. Knowing what's inside a project

To combine projects you need to know what their tracks are (drums, bass,
vocals), not just their names. Before committing to an approach I probed
Ableton's own Live Database and found it auto-tags samples heavily enough
to be the main signal, with track names as the fallback.

- [Track categorization: PRD](track-categorization-prd.md). See
  "Investigation findings" for that probe.
- [Track categorization: spec](track-categorization-spec.md). Shipped in
  [`1f3065d`](https://github.com/kyunas96/malstrom/commit/1f3065d). The
  database lookup then moved to a folder setting in
  [`70e2d42`](https://github.com/kyunas96/malstrom/commit/70e2d42).

## 4. Mashup v0: the crude version first

The idea behind Malstrom is mashing up compatible projects. The full design
kept getting blocked on four other features, so I cut it down to the
smallest version that could answer one question: would I actually use it?
Two projects, whole tracks, no grouping.

- [Mashup v0: PRD](mashup-v0-prd.md). The merge backend shipped in
  [`5aede61`](https://github.com/kyunas96/malstrom/commit/5aede61); the
  picker UI didn't make it in yet.

## 5. Proving the output really opens in Live

A merged file can be well-formed XML and still be refused by Live. The only
real test is opening it in Live, so I built a tool that mashes up every pair
of projects in my library and opens each result in Live unattended. Each
Live error it surfaced became a fix and a structural check.

- [Batch test across a whole library: plan](mashup-batch-test-plan.md)
- [Getting rid of Live between pairs: force-kill, not quit](mashup-batch-live-force-kill.md)

## 6. Mashup Stage 1: both songs, combined correctly

Some things in a Live set can only exist once (tempo, the master track),
and some have to line up across every track (return tracks and sends,
scenes). This doc lists those constraints and splits the feature into
stages. The Stage 1 plan is the current work, checked off as it lands.

- [Mashup: constraints and stages](mashup-constraints-and-stages.md)
- [Mashup Stage 1: implementation plan](mashup-stage1-plan.md). Return
  tracks and sends shipped in
  [#2](https://github.com/kyunas96/malstrom/pull/2), verified in Live.
