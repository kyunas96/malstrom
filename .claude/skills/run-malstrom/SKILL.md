---
name: run-malstrom
description: Build, run, and drive the malstrom frontend (a Tauri + React + Vite desktop app). Use when asked to start malstrom, run its dev server, build it, take a screenshot of its UI, or interact with the running table/filter UI.
---

malstrom is a Tauri desktop app whose UI is a Vite/React frontend.
The frontend renders against dummy fixture data (`src/fixtures/alsProjects.ts`)
until a user picks a real folder, so it can be built and driven headlessly
with **no Rust/Tauri backend running at all** — just the Vite dev server on
port 1420, driven via the REPL at
`.claude/skills/run-malstrom/driver.mjs` (a chromium-cli-style wrapper
around the `@playwright/test` package already in `devDependencies`; plain
`chromium-cli` is not installed in every environment this skill runs in).

All paths below are relative to the repo root.

## Prerequisites

Chromium for Playwright (one-time; already present if `pnpm install` has
been run before and the browser cache wasn't cleared):

```bash
pnpm exec playwright install chromium
```

## Setup

```bash
pnpm install
```

## Build

No separate build step is needed to drive the UI — `pnpm dev` serves it
directly. A production build (only needed if you're checking `tsc`/bundling,
not for driving the UI) is:

```bash
pnpm build   # tsc && vite build
```

## Run (agent path)

Start the dev server in the background, wait for it to serve, then pipe
commands to the driver:

```bash
(pnpm dev > /tmp/malstrom-vite.log 2>&1 &)
timeout 30 bash -c 'until curl -sf http://localhost:1420 >/dev/null; do sleep 1; done'

node .claude/skills/run-malstrom/driver.mjs <<'EOF'
nav http://localhost:1420
wait-for text=Sunset Drive
screenshot screenshots/initial.png
quit
EOF
```

Stop the dev server when done: `lsof -ti:1420 -sTCP:LISTEN | xargs -r kill`.

Screenshots land wherever you pass as the `screenshot` argument, relative to
CWD (default `screenshots/<n>.png` if omitted); create the dir first if it
doesn't exist (the driver does this for you).

Driver commands (one per stdin line):

| command | what it does |
|---|---|
| `nav <url>` | navigate |
| `wait-for text=<substring>` | wait for text to appear (or pass a raw selector) |
| `click <selector>` | Playwright locator syntax, e.g. `role=button[name=/Major/]` |
| `click-text <substring>` | shorthand for `getByText(substring).click()` |
| `fill <selector> <text...>` | fill an input |
| `screenshot [path]` | full-page screenshot |
| `eval <js expr>` | runs in page context, prints the JSON result |
| `console` | print collected `console.error`/`pageerror` output so far |
| `quit` | close the browser and exit |

The driver must be run with CWD at the repo root (`node
.claude/skills/run-malstrom/driver.mjs`, not `cd` into the skill dir
first) — it resolves `@playwright/test` against the repo's `node_modules`.

## Run (human path)

```bash
pnpm tauri dev   # opens the real desktop window; requires a Rust toolchain
```

Useless headless — only for a human with a display.

## Test

```bash
pnpm test        # vitest: pure filter/matching logic (src/lib/*.test.ts)
pnpm test:e2e    # playwright: full UI flows against the dev server (src/tests/e2e/)
```

`test:e2e` boots `pnpm dev` itself via `playwright.config.ts`'s `webServer`
block (`reuseExistingServer: true`, so it's safe to run alongside an
already-running `pnpm dev`).

---

## Gotchas

- **Import `@playwright/test`, not `playwright`.** The repo has
  `@playwright/test` in `devDependencies`, not the bare `playwright`
  package — `import { chromium } from 'playwright'` throws
  `ERR_MODULE_NOT_FOUND` even though the browser binaries are installed.
- **Port 1420 is hardcoded and strict.** `vite.config.ts` sets
  `server: { port: 1420, strictPort: true }` for Tauri's benefit — if
  something else is already listening there, `pnpm dev` exits instead of
  picking a fallback port. Free it first: `lsof -ti:1420 -sTCP:LISTEN |
  xargs -r kill`.
- **No Tauri/Rust backend needed for UI work.** `AlsProjectList` seeds its
  state from `dummyAlsProjects` on load; the `invoke('list_projects', ...)`
  Tauri call only fires after a user clicks "Choose Root Folder", which
  the driver never needs to touch to exercise the table/filter UI.

## Troubleshooting

- **`Error [ERR_MODULE_NOT_FOUND]: Cannot find package 'playwright'`**:
  either fix the import to `@playwright/test` (see Gotchas), or you ran
  `node driver.mjs` from inside the skill directory instead of the repo
  root — Node resolves `node_modules` relative to CWD.
- **`curl: (7) Failed to connect`** while polling for the dev server: check
  `/tmp/malstrom-vite.log` — usually port 1420 is already bound by a
  stale `pnpm dev` process (see Gotchas).
