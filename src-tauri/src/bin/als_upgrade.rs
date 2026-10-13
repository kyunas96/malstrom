//! Schema-upgrade tool: open older-Live-version .als file(s) in real
//! Ableton Live and drive Save -- which an old-schema file routes to a
//! Save As panel rather than saving in place, letting Live itself produce
//! a current-schema copy -- then confirm a valid new file landed on disk.
//! Never overwrites or touches an original.
//!
//! Usage: cargo run --bin als_upgrade -- <file-or-dir>
//!
//! A single file is upgraded on its own; a directory is recursively
//! scanned (via the same `scan_als_files` used by `mashup_batch_test`,
//! skipping `Backup/` folders) and every file without an upgraded copy
//! already sitting in its `LIVE 12/` subfolder is upgraded in turn -- that
//! existence check doubles as resume support for free, so re-running the
//! same command after an interruption just picks up where it left off.
//!
//! Plays the same "prove it on one real file first" role for the
//! schema-upgrade idea that mashup_smoke_test played for the mashup
//! feature. The Save As panel's
//! accessibility structure below is unverified guesswork (no GUI in this
//! environment); expect at least one round of real-Live feedback before
//! it's solid, same as that tool needed.

use malstrom_lib::als::output_path::{scan_als_files, LIVE_12_UPGRADE_DIR};
use malstrom_lib::als::AlsInspector;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        eprintln!("usage: als_upgrade <file-or-dir>");
        return ExitCode::FAILURE;
    };

    match run(Path::new(path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(path: &Path) -> anyhow::Result<()> {
    if path.is_dir() {
        run_dir(path)
    } else {
        upgrade_one(path)
    }
}

fn run_dir(root: &Path) -> anyhow::Result<()> {
    let mut paths = scan_als_files(root)?;
    paths.sort();
    println!("Found {} .als file(s) under {}.", paths.len(), root.display());

    let (mut upgraded, mut skipped, mut failed) = (0, 0, 0);
    for path in &paths {
        // An already-upgraded copy is itself a discovered .als file (it
        // lives inside a LIVE_12_UPGRADE_DIR folder); it has nothing
        // further to upgrade.
        let is_already_upgraded = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == LIVE_12_UPGRADE_DIR);
        if is_already_upgraded {
            continue;
        }
        if upgraded_path(path)?.exists() {
            skipped += 1;
            continue;
        }
        println!("Upgrading {}...", path.display());
        match upgrade_one(path) {
            Ok(()) => upgraded += 1,
            Err(e) => {
                println!("  FAILED: {e}");
                failed += 1;
            }
        }
    }
    println!("Done: {upgraded} upgraded, {skipped} already upgraded, {failed} failed.");
    Ok(())
}

fn upgraded_path(original: &Path) -> anyhow::Result<PathBuf> {
    let dir = original.parent().unwrap_or_else(|| Path::new("."));
    let name = original.file_name().ok_or_else(|| anyhow::anyhow!("path has no file name"))?;
    Ok(dir.join(LIVE_12_UPGRADE_DIR).join(name))
}

fn upgrade_one(path: &Path) -> anyhow::Result<()> {
    let target = upgraded_path(path)?;
    if target.exists() {
        anyhow::bail!(
            "upgraded copy already exists at {} -- not touching it, remove it first to redo this",
            target.display()
        );
    }
    // Pre-created so the Save panel can navigate straight to it (via "Go to
    // Folder") rather than needing to click a "New Folder" button through
    // GUI automation.
    std::fs::create_dir_all(target.parent().unwrap_or_else(|| Path::new(".")))?;

    println!("Opening {} in Live and saving as {}...", path.display(), target.display());
    save_as_via_live(path, &target)?;

    println!("Waiting for the file to land on disk...");
    wait_for_file(&target, Duration::from_secs(60))?;

    println!("Verifying it loads cleanly...");
    AlsInspector::open(&target)?;
    println!("PASS: {} is a valid, current-schema copy.", target.display());
    println!("(Live is left open -- quit it yourself once you're done looking.)");
    Ok(())
}

fn wait_for_file(path: &Path, timeout: Duration) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!("timed out waiting for {} to appear", path.display())
}

/// Drives real Ableton Live via `osascript`/System Events: opens `path`,
/// waits for it to finish loading, then invokes File > "Save Live Set
/// As..." and types `target`'s filename into the resulting panel.
///
/// Confirmed by hand against real Live 12 opening a genuine Live 9.7.6
/// file, 2026-09-08:
/// - Cmd+S is **not** usable here -- it maps to "Save Live Set", which
///   Live disables outright for an old-schema file (only "Save Live Set
///   As..." is enabled), so it must be invoked directly as a menu item.
/// - The resulting panel is a top-level window titled "Save" (`AXDialog`
///   subrole) -- not a sheet of the document window, the original guess.
/// - Its filename field is `text field 2 of splitter group 1` (the 1st
///   text field is the sidebar's search box); the confirm button is
///   `button "Save" of splitter group 1`, addressable by title.
/// - The field displays the name without its extension (confirmed
///   visually: a real "MALSTROM" set showed a bare `MALSTROM`), so the
///   value written here is `target`'s file stem, not its full file name.
///
/// UNVERIFIED (added for the `LIVE 12/` subfolder scheme, not yet
/// exercised against real Live): navigates the panel into `target`'s
/// parent directory first via the standard macOS "Go to Folder" shortcut
/// (Cmd+Shift+G, a one-field sheet accepting a path, confirmed by Return)
/// rather than clicking a "New Folder" button and typing a name through
/// GUI automation -- `upgrade_one` already creates that directory before
/// calling this, so there's nothing for the panel to create, only
/// navigate to.
///
/// Live's launch can also show its own "unexpectedly quit ... recover your
/// work?" dialog first if it didn't exit cleanly last time (also confirmed
/// by hand -- a leftover dirty-quit flag from earlier testing, not caused
/// by this tool itself, still triggered it on the next launch). That's
/// noise to dismiss ("No", nothing here is worth recovering) before
/// continuing to wait for the file to actually load, same handling
/// `mashup_batch_test`'s Phase B already has for the same dialog.
///
/// Deliberately doesn't try to wait out or confirm the save completing in
/// AppleScript at all -- the caller polls the filesystem for the resulting
/// file instead, far more reliable than guessing at more UI state. Also
/// deliberately doesn't handle a same-name "replace?" confirmation: the
/// caller already checked `target` doesn't exist, so seeing that
/// confirmation here would mean something unexpected is going on, and
/// silently confirming an overwrite is exactly the risk this tool exists
/// to avoid -- better to time out and surface it.
fn save_as_via_live(path: &Path, target: &Path) -> anyhow::Result<()> {
    let target_dir = target.parent().ok_or_else(|| anyhow::anyhow!("target path has no parent directory"))?;
    let target_stem = target
        .file_stem()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("target path has no file stem"))?;

    let script = format!(
        r#"
        on run
            set targetPath to "{path}"
            do shell script "open " & quoted form of targetPath

            set loaded to false
            repeat 60 times
                delay 2
                tell application "System Events"
                    set liveProcs to (every process whose name is "Live")
                    if (count of liveProcs) > 0 then
                        set liveProc to item 1 of liveProcs
                        set frontmost of liveProc to true
                        tell liveProc
                            set dialogWindows to (every window whose subrole is "AXDialog" or subrole is "AXSystemDialog")
                            if (count of dialogWindows) > 0 then
                                set dialogWindow to item 1 of dialogWindows
                                set messageText to ""
                                try
                                    set messageText to value of (static text 1 of group 1 of dialogWindow)
                                end try
                                if messageText contains "unexpectedly quit" then
                                    try
                                        click button "No" of dialogWindow
                                    on error
                                        try
                                            click button 1 of group 1 of dialogWindow
                                        end try
                                    end try
                                else
                                    return "DIALOG:" & messageText
                                end if
                            else if (count of windows) > 0 then
                                set loaded to true
                            end if
                        end tell
                    end if
                end tell
                if loaded then exit repeat
            end repeat
            if not loaded then return "TIMEOUT:old file never finished loading"

            tell application "System Events"
                set liveProc to item 1 of (every process whose name is "Live")
                set frontmost of liveProc to true
                tell liveProc
                    click menu item "Save Live Set As..." of menu "File" of menu bar 1

                    set panelFound to false
                    repeat 15 times
                        delay 1
                        if exists window "Save" then
                            set panelFound to true
                            exit repeat
                        end if
                    end repeat
                    if not panelFound then return "TIMEOUT:no save panel appeared"

                    set saveWin to window "Save"
                    keystroke "g" using {{command down, shift down}}
                    delay 1
                    if not (exists sheet 1 of saveWin) then return "TIMEOUT:no Go to Folder sheet appeared"
                    set value of text field 1 of sheet 1 of saveWin to "{target_dir}"
                    keystroke return
                    delay 1

                    set sg to splitter group 1 of saveWin
                    set value of text field 2 of sg to "{target_stem}"
                    click button "Save" of sg
                end tell
            end tell
            return "OK"
        end run
        "#,
        path = path.display(),
        target_dir = target_dir.display(),
    );

    let output = std::process::Command::new("osascript").arg("-e").arg(&script).output()?;
    if !output.status.success() {
        anyhow::bail!(
            "osascript failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if let Some(message) = result.strip_prefix("DIALOG:") {
        anyhow::bail!("Live raised an unexpected dialog while loading the old file: {message}");
    } else if let Some(reason) = result.strip_prefix("TIMEOUT:") {
        anyhow::bail!("{reason}");
    } else if result != "OK" {
        anyhow::bail!("unexpected osascript output: {result:?}");
    }
    Ok(())
}
