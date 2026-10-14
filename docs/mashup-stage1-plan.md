# Mashup Stage 1: implementation plan

Write the mashup logic as Rust library code that doesn't depend on Tauri.
Test it with `cargo test` and a CLI, then connect it to Tauri last. The UI is
a separate problem. See [mashup-constraints-and-stages.md](mashup-constraints-and-stages.md)
for what Stage 1 is.

Most of this setup already exists: `src/als/` doesn't depend on Tauri,
`tests/mashup.rs` tests `build_mashup` directly, `mashup_batch_test` calls it
as a library, and the `mashup_create` command is a thin wrapper.

## 1. Add one CLI: `src/bin/mashup.rs`

It runs a single pair of projects, which suits trying out one feature at a
time. `mashup_batch_test` keeps its job as the regression check across the
whole library.

```
cargo run --bin mashup -- A.als B.als -o out.als
    --tempo a|b|<bpm>        # default a
    --returns append|drop    # default append
    --no-groups              # v0 behaviour, for comparison
    --dry-run                # build + diagnostics::check, write nothing
    --open                   # open the result in Live to verify (reuses batch-test's osascript code)
```

Parse the flags by hand. Five flags doesn't justify adding `clap`.

**Started:** the minimal form, `mashup A.als B.als -o out.als`, merges every
track of both, refuses to overwrite, and writes nothing if
`diagnostics::check` fails. Add each flag with the feature it controls.

## 2. Give `build_mashup` an options struct

For example, `build_mashup(&sources, &MashupOptions { tempo, returns, groups })`.
The CLI, the tests and the Tauri command all fill in the same struct, so
whatever passes in the CLI is exactly what the app will run.

## 3. Build Stage 1 one constraint at a time, each with a test

1. **Answer the open questions first.** Write tests that build a mashup from
   two projects with different return counts, then different scene counts,
   and run `diagnostics::check`. Whatever breaks tells you where to start.
   The return-count question is answered (see item 3); the scene-count one
   is still open.
2. Container groups (Group A / Group B).
3. ~~Add B's return tracks after A's, and pad every track's sends to match.~~
   **Done** and confirmed in Live. `diagnostics::check` now flags any track
   whose send count doesn't match the return count.
4. Pad scenes to the larger project's count.
5. Choose the tempo.

Each one goes in as a function in `mashup.rs`, plus a check in `diagnostics`
if Live has rejected that problem before, plus a test. Then confirm it once
with `--open` in real Live.

## 4. Connect it to Tauri last

`mashup_create` gets an options argument. The UI work can start whenever
you're ready.

## Tradeoff: one crate vs. a workspace

The CLI is part of the same crate as the app, so building it also compiles
Tauri. That's slow the first time and cached after that. If it gets annoying,
move `src/als/` into its own crate (`malstrom-core`) inside a Cargo
workspace. That's a mechanical change; don't do it until the build time
actually bothers you.
