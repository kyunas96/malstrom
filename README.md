<p align="center">
  <img src=".github/assets/wordmark.svg" height="128" alt="M.alstrom">
</p>

<p align="center">
  A maelstrom of <code>.als</code> files, tamed. Scan a folder of Ableton
  Live Sets, see what musical scale every project and clip is compatible
  with, and filter your whole library by key.
</p>

<p align="center">
  <a href="https://github.com/kyunas96/malstrom/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/kyunas96/malstrom/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="status" src="https://img.shields.io/badge/status-v0.1-blue">
  <img alt="platform" src="https://img.shields.io/badge/platform-macOS-lightgrey">
  <img alt="built with" src="https://img.shields.io/badge/built%20with-Tauri%20%2B%20React-24C8DB">
</p>

<p align="center">
  <img src=".github/assets/main-view.png" alt="malstrom main window">
</p>

## What it does

Point it at a folder of `.als` files and it will:

- **Scan** the directory (top-level only, not subfolders) and list every
  project in a filterable, virtualized table.
- **Read** each project's explicit `ScaleInformation` key/scale tags where
  present.
- **Infer** compatible scales directly from MIDI clip note content when a
  project has no explicit scale set.
- **Filter** the whole list by scale, so you can answer "which of my
  projects are in D Dorian?" in one click.
- **Apply** a chosen scale back to a project, tagging every MIDI clip that's
  compatible with it.

## Status

This is early — v0.1, unreleased. What's solid vs. what's still missing:

**Works today**
- Directory scanning + virtualized/filterable project table
- Explicit `ScaleInformation` parsing
- Scale inference from MIDI clip content
- Filter-by-scale UI
- Applying a scale to a project, writing it onto every compatible MIDI clip

**Not yet**
- No packaged release — run from source (below)
- Near-matches (a clip one or two notes off from a scale) aren't detected;
  only exact pitch-class matches count

### Experimental

Working on the backend, not yet in the UI. Checked by opening the output in
Ableton Live, but only on a small sample of files from the current Live
version. Projects saved in older Live versions haven't been tested.

- **Mashup** (`src-tauri/src/als/mashup.rs`): splice selected tracks from
  two `.als` projects into one new, valid Live Set. Both projects keep
  their return tracks, and every track's sends are lined up so each one
  still feeds its own project's returns. Exposed as the `mashup_create`
  Tauri command.
- **Dev CLIs** (`src-tauri/src/bin/`):
  - `mashup <A.als> <B.als> -o <out.als>` — mash up every track of one
    pair into a new file, checked for known structural problems before
    writing. Doesn't touch Live.

  These two drive the real Live app via macOS accessibility (`osascript`)
  to prove its output loads:
  - `mashup_batch_test <dir> [report.jsonl]` — mash up every pair of
    projects in a folder, open each result in Live, and log pass/fail to a
    resumable JSONL report
  - `als_upgrade <file-or-dir>` — have Live re-save older-version sets as
    current-schema copies (unverified; expect rough edges)

  Run with `cargo run --bin <name> -- <args>` from `src-tauri/`. The two
  Live-driving ones open and force-quit Live, so don't run them with
  unsaved work open.

## Technical highlights

- `.als` files are gzipped XML; the Rust backend decompresses (`flate2`) and
  parses (`roxmltree`) them, and writes changes back with a backup of the
  original first.
- Scale inference reduces every MIDI clip to its pitch-class set and scores
  it against all scale/root combinations.
- Scanning runs off the UI thread in parallel (`rayon`) so the app never
  freezes on large libraries.
- The table is virtualized (`@tanstack/react-virtual`) to stay smooth with
  thousands of rows.
- Tests at three layers: Rust integration tests, Vitest unit tests,
  Playwright end-to-end tests.
- Built with [Claude Code](https://claude.com/claude-code); `.claude/skills/`
  holds the project-specific agent skills used during development.

## Getting started

Developed and tested on macOS only. Windows/Linux aren't supported — PRs
welcome if you want to make it work there.

### Prerequisites

- [Rust toolchain](https://www.rust-lang.org/tools/install) (stable)
- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- Xcode Command Line Tools — see the
  [Tauri macOS prerequisites](https://v2.tauri.app/start/prerequisites/#macos)

### Run it

```sh
pnpm install
pnpm tauri dev
```

### Build it

```sh
pnpm tauri build
```

### Test it

```sh
cargo test       # from src-tauri/ — Rust unit tests
pnpm test        # vitest — frontend unit tests
pnpm test:e2e    # playwright — end-to-end UI tests
```

## How it works

The `.als` parsing and scale-scoring logic lives in `src-tauri/src/als/` —
see its [README](src-tauri/src/als/README.md) for how scale information is
extracted from a project and how scale candidates are scored.

## Roadmap

- [x] Scan MIDI clips in a project and determine every scale they're
      compatible with
- [x] Surface scale compatibility in the UI, filterable/sortable per project
- [x] Apply a scale to a project's compatible MIDI clips
- [ ] Detect near-matches (a clip that fits a scale except for one or two
      notes)
- [ ] Packaged builds / releases
- [x] Categorize a project's tracks (DRUMS, BASS, PERCUSSION, VOCALS, ...)
      from their names — not yet surfaced in the UI
- [ ] **Mashup**: filter projects down to a shared scale, select several of
      them, and get a picker of elements to pull from each into one new
      project — backend works for 2 projects (see Experimental above), UI
      in progress

## License

[MIT](LICENSE.md)

Malstrom is an independent project, not affiliated with or endorsed by
Ableton. Ableton and Live are trademarks of Ableton AG.
