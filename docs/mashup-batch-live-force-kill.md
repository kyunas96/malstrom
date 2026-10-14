# Getting rid of Live between batch pairs: force-kill, not quit

Part of the [batch mashup test](mashup-batch-test-plan.md). After each
pair is checked in real Live, that Live instance has to go so the next pair
can open in a fresh one.

## Why not quit gracefully

My earlier single-pair tool tried the graceful route first: tell Live to
quit, then dismiss its "Save changes?" sheet. I took it out again because
the quit would sometimes simply hang, indefinitely, not just slowly.

That fits a known AppleScript trap. A plain `tell application "Live" to
quit` waits for Live to reply. If Live's quit handler opens the "Save
changes?" sheet and won't reply until it's dismissed, the script is stuck
inside the `quit` line and never reaches the code that would dismiss the
sheet.

There are ways around that, but the batch tool didn't need them. Every
merged file is throwaway, so there's nothing worth risking a hang to save.
Instead the tool force-kills Live between pairs and accepts the one
downside: on the next launch, Live shows its "didn't shut down properly,
recover your work?" dialog. That becomes one more dialog type the tool
recognises and dismisses.

That trade only makes sense for throwaway files. It would be wrong for a
tool that might have a real project open.

## How it works

- After each pair's Live result is recorded, force-kill Live
  (`pkill -9 -x Live`) and poll until the process is gone. `SIGKILL` can't
  be caught, so this is near-instant. The tool never sends `quit`, so the
  hang above can't happen.
- At startup, do one force-kill in case an interrupted earlier run left
  Live open.
- On each launch, Live's recovery dialog appears before anything else. The
  tool has to tell it apart from a real "this file is corrupt" result,
  dismiss it, and keep waiting for the actual outcome.

## What I confirmed by hand (2026-09-08)

- **The recovery dialog** reads: *"Live unexpectedly quit while you were
  working on the Live Set "<filename>". Would you like to recover your
  work?"*, with **No** and **Yes** buttons (Yes is the default). The file
  name changes and doesn't matter: every kill is deliberate and every file
  is throwaway, so the tool always clicks **No**. It matches on the stable
  part, `"unexpectedly quit"`.
- **The real failure dialog** reads: *"The document "<path>" is corrupt and
  cannot be loaded. (<reason>)"*, with a single **OK** button. For example,
  the reason `GroovePool corrupted` came up in practice. The tool matches
  on `"is corrupt and cannot be loaded"`. The two dialogs can also be told
  apart by their buttons alone: one button versus two.
- **No macOS crash-reporter dialog** appears on top. Only Live's own
  dialogs showed up.

## Not yet confirmed

Whether dismissing the recovery dialog can ever be mistaken for the next
pair's real result when pairs run back to back. Early unattended runs
looked promising, but I haven't checked this case specifically.
