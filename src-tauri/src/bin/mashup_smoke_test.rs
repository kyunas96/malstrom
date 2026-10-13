//! Headless mashup smoke test: mash up everything from two real `.als`
//! files and report whether the result is structurally sound *and* whether
//! Ableton Live itself accepts it -- the same question a person testing
//! this feature by hand would ask, without opening the app manually.
//!
//! Usage: cargo run --bin mashup_smoke_test -- <ProjectA.als> <ProjectB.als>

use malstrom_lib::als::mashup::{build_mashup, diagnostics, non_return_tracks, MashupSource};
use malstrom_lib::als::output_path::write_als;
use malstrom_lib::als::AlsInspector;
use roxmltree::Document;
use std::path::PathBuf;
use std::process::ExitCode;

fn all_track_indices(xml: &str) -> anyhow::Result<Vec<usize>> {
    let doc = Document::parse(xml)?;
    Ok((0..non_return_tracks(&doc)?.len()).collect())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path_a, path_b] = args.as_slice() else {
        eprintln!("usage: mashup_smoke_test <ProjectA.als> <ProjectB.als>");
        return ExitCode::FAILURE;
    };

    match run(path_a.into(), path_b.into()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Returns `Ok(true)` only if the merge is structurally clean *and* Live
/// accepted the file with no dialog.
fn run(path_a: PathBuf, path_b: PathBuf) -> anyhow::Result<bool> {
    println!("Loading {} and {}...", path_a.display(), path_b.display());
    let a = AlsInspector::open(&path_a)?;
    let b = AlsInspector::open(&path_b)?;

    let a_indices = all_track_indices(a.raw_xml())?;
    let b_indices = all_track_indices(b.raw_xml())?;
    println!(
        "Mashing up everything: {} track(s) from A, {} track(s) from B.",
        a_indices.len(),
        b_indices.len()
    );

    let merged = build_mashup(&[
        MashupSource { xml: a.raw_xml().to_string(), track_indices: a_indices },
        MashupSource { xml: b.raw_xml().to_string(), track_indices: b_indices },
    ])?;

    let problems = diagnostics::check(&merged);
    if !problems.is_empty() {
        println!("FAIL: structural check found {} problem(s):", problems.len());
        for p in &problems {
            println!("  - {p}");
        }
        return Ok(false);
    }
    println!("Structural check: clean.");

    // A fresh name every run: since Live is deliberately left open
    // afterward (see open_in_live_and_check's doc comment), reusing one
    // fixed filename risks `open` just refocusing a stale already-open
    // window from a previous run instead of loading this run's actual
    // (possibly different) merged content.
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
    let temp_path = std::env::temp_dir().join(format!("mashup-smoke-test-{unique}.als"));
    write_als(&merged, &temp_path)?;
    println!("Wrote merged output to {}", temp_path.display());

    println!("Opening in Ableton Live...");
    let result = match open_in_live_and_check(&temp_path) {
        Ok(None) => {
            println!("PASS: Live opened the file with no error dialog.");
            Ok(true)
        }
        Ok(Some(message)) => {
            println!("FAIL: Live reported a problem:\n  {message}");
            Ok(false)
        }
        Err(e) => {
            println!("FAIL: could not verify with Live ({e}).");
            Ok(false)
        }
    };
    println!("(Live is left open -- quit it yourself once you're done looking.)");
    result
}

/// Drives real Ableton Live via `osascript`/System Events -- there is no
/// headless mode, so this is the only way to get a genuine "did Live
/// accept this file" answer. Returns `Ok(None)` on a clean open, `Ok(Some(message))`
/// if Live raised a dialog (corrupt file, repair prompt, etc.), and `Err`
/// if the check itself couldn't run (e.g. no Accessibility permission).
///
/// Best-effort and macOS-only, tuned against one real Live 12 Suite
/// installation (verified by hand: the process is literally named "Live";
/// a corrupt-file alert's subrole is "AXDialog" with its message at
/// `static text 1 of group 1` and an unnamed OK button at `button 1 of
/// group 1`; a loaded project window has subrole "AXUnknown"). Requires
/// Accessibility (not just Automation) permission for whatever process
/// runs this. May need adjusting for other Live versions/editions.
///
/// Deliberately does not try to quit Live afterward. An earlier version
/// did, but Live's own quit behavior here proved too inconsistent to
/// automate reliably: `quit` on a loaded project shows a "Save changes?"
/// sheet with unnamed buttons (only matchable by `description`), Live
/// spawns a Plugin Scanner helper on every launch that leaves it fully
/// unresponsive to any Apple Event for tens of seconds, and even after
/// that clears, `quit` has been observed to simply hang. The one thing
/// force-killing it definitely does is flag the next launch as crash
/// recovery, producing an unrelated prompt that looks like a real merge
/// failure. None of that is worth the risk for a step (cleanup) that
/// isn't the actual question this tool answers -- Live is left open;
/// quit it yourself once you've seen the result.
fn open_in_live_and_check(path: &std::path::Path) -> anyhow::Result<Option<String>> {
    let script = format!(
        r#"
        on run
            set targetPath to "{path}"
            do shell script "open " & quoted form of targetPath

            set timeoutTicks to 20
            repeat timeoutTicks times
                delay 1
                tell application "System Events"
                    set liveProcs to (every process whose name is "Live")
                    if (count of liveProcs) > 0 then
                        set liveProc to item 1 of liveProcs
                        -- Live's windows (including any alert) aren't
                        -- enumerated by System Events at all until the app
                        -- is frontmost -- confirmed by hand: `count windows`
                        -- stays 0 indefinitely on a background Live process
                        -- even with a project fully loaded. Setting
                        -- `frontmost` on the process itself (rather than
                        -- `tell application "Live" to activate`, which
                        -- resolves the app by name through a separate,
                        -- slower path and can fail with "Can't get
                        -- application" if it races Live's own launch) stays
                        -- entirely within the already-resolved liveProc.
                        set frontmost of liveProc to true
                        tell liveProc
                            set dialogWindows to (every window whose subrole is "AXDialog" or subrole is "AXSystemDialog")
                            if (count of dialogWindows) > 0 then
                                set dialogWindow to item 1 of dialogWindows
                                set messageText to ""
                                try
                                    set messageText to value of (static text 1 of group 1 of dialogWindow)
                                end try
                                if messageText is "" then
                                    try
                                        set messageText to value of (first static text of dialogWindow)
                                    end try
                                end if
                                try
                                    click button 1 of group 1 of dialogWindow
                                end try
                                return "DIALOG:" & messageText
                            end if
                            if (count of windows) > 0 then
                                return "OK"
                            end if
                        end tell
                    end if
                end tell
            end repeat
            return "TIMEOUT"
        end run
        "#,
        path = path.display()
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
        Ok(Some(message.to_string()))
    } else if result == "OK" {
        Ok(None)
    } else if result == "TIMEOUT" {
        anyhow::bail!("timed out waiting for Live to open the file or show a dialog")
    } else {
        anyhow::bail!("unexpected osascript output: {result:?}")
    }
}
