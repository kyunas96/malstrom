# Batch-testing mashups across a whole `.als` library

## Why

Mashup v0 merges tracks from two Live sets by splicing their XML. A merged
file can be perfectly well-formed and still be refused by Live, and the
only real test is opening it in Live. My first tool did that one pair at a
time: mash up two files, run a quick structural check, open the result in
Live and read whatever dialog came up. That loop found and fixed four real
bugs in `build_mashup`:

- track Ids colliding between the two projects
- a `NextPointeeId` counter left too low for the merged Ids
- `PointeeId` references left pointing at Ids that had been shifted
- groove references pointing into a groove pool that wasn't copied over

Testing one pair at a time was too slow to find the rest, so I wanted to
point a tool at my whole project library, try every possible pair, and get
back a report of every failure to fix in one batch.

## Decisions

- **Scan recursively** from a root folder, skipping Live's own `Backup/`
  folders so old revisions of a project aren't tested as separate files.
- **Test every pair**, with no scale-compatibility filtering. This is a
  stress test of the merge code, not a simulation of the app's feature.
- **Two phases per pair:** a fast structural check for every pair, then a
  real Live check for every pair that passes it.
- **Run unattended for hours** on a spare Mac.
- **Expect old and broken files.** A library built up over years has files
  from older Live versions. Those should be reported and skipped, not crash
  the run.

## How it works

`cargo run --bin mashup_batch_test -- <root-dir> [report-path.jsonl]`
([source](../src-tauri/src/bin/mashup_batch_test.rs)). It calls the same
library code the app does: `build_mashup`, `diagnostics::check` and
`write_als`.

### Load once

Each file is decompressed and parsed once up front and kept in memory, not
once per pair. A file that won't open is written to the report once as
`{"file": ..., "error": ...}` and left out of pairing. Files that have an
upgraded copy in a `LIVE 12/` subfolder (made by the `als_upgrade` tool)
are skipped in favour of that copy.

### Phase A: structural check (every pair)

Merge all tracks of both files and run `diagnostics::check`. There are two
ways this can fail, and both have to be recorded:

- **An error** returned by the merge code, which is easy to log.
- **A panic.** The parser and the byte-range slicing are being pointed at
  arbitrary real files, and an unhandled edge case panics instead of
  returning an error. Because the report doubles as the resume checkpoint,
  an unrecorded crash would send a resumed run straight back into the same
  pair forever. Each pair runs inside `catch_unwind`, so a panic is logged
  as that pair's result and the run moves on.

### Phase B: real Live check (every structurally clean pair)

Write the merged set to a uniquely named temp file (a fixed name risked
Live refocusing a stale window instead of loading the new file), open it
in Live, and poll through macOS accessibility (`osascript`) until either a
normal window or a dialog appears. A dialog's text is recorded as that
pair's result.

Then Live has to go before the next pair. Quitting it gracefully turned
out to hang, so the tool force-kills it instead; see
[force-kill, not quit](mashup-batch-live-force-kill.md).

Temp files for passing pairs are deleted straight away. Files that produced
a dialog or error are kept so I can open them by hand. Without that, a run
over thousands of pairs would fill the temp folder with multi-MB files.

### Resume

The report is JSON Lines, one line per pair, flushed as it's written:

```json
{"a": "...", "b": "...", "structural_problems": [], "live_result": "pass"}
{"a": "...", "b": "...", "structural_problems": ["duplicate Id among top-level tracks (4 tracks, 3 unique Ids)"]}
{"file": "...", "error": "XML does not parse: ..."}
```

Re-running the same command skips every pair already in the report, so an
interrupted run picks up where it stopped. If the machine died mid-write,
the truncated last line is dropped with a warning instead of blocking the
resume.

## Limits

- **It's slow, and that can't be fixed.** Live's plugin scan adds roughly
  15–90 seconds to every launch, and every Live-checked pair needs a fresh
  launch. With N files that's N·(N−1)/2 pairs, so anything beyond a couple
  of dozen files takes hours. Live has no headless mode and can't run two
  instances, so there's nothing to parallelise.
- **Very old files fail early** and show up as `error` lines. The tool
  reports them; it doesn't try to convert them.
