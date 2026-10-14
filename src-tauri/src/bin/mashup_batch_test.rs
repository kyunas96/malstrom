//! Batch mashup smoke test across a whole .als library: recursively scans a
//! directory, mashes up every possible pair of loadable projects, and
//! reports both the fast structural check and real-Live verification
//! results to a resumable JSONL report.
//!
//! Usage: cargo run --bin mashup_batch_test -- <root-dir> [report-path.jsonl]

use malstrom_lib::als::mashup::{build_mashup, diagnostics, non_return_tracks, MashupSource};
use malstrom_lib::als::output_path::{scan_als_files, write_als, LIVE_12_UPGRADE_DIR};
use malstrom_lib::als::AlsInspector;
use roxmltree::Document;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(root) = args.first() else {
        eprintln!("usage: mashup_batch_test <root-dir> [report-path.jsonl]");
        return ExitCode::FAILURE;
    };
    let root = PathBuf::from(root);
    let report_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("mashup-batch-report.jsonl"));

    match run(&root, &report_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path, report_path: &Path) -> anyhow::Result<()> {
    // In case a previous run was killed mid-batch with Live left open.
    force_kill_live();

    let mut paths = scan_als_files(root)?;
    paths.sort();
    let found = paths.len();
    paths = drop_superseded_originals(paths);
    println!(
        "Found {found} .als file(s) under {} ({} superseded by an upgraded \"{LIVE_12_UPGRADE_DIR}/\" copy, skipped).",
        root.display(),
        found - paths.len()
    );

    let done_pairs = load_done_pairs(report_path)?;
    let mut report = OpenOptions::new()
        .create(true)
        .append(true)
        .open(report_path)?;

    let mut loaded: Vec<(String, String)> = Vec::new();
    for path in &paths {
        let key = path.display().to_string();
        match AlsInspector::open(path) {
            Ok(inspector) => loaded.push((key, inspector.raw_xml().to_string())),
            Err(e) => append_line(&mut report, &json!({"file": key, "error": e.to_string()}))?,
        }
    }
    println!(
        "{} file(s) loaded cleanly; {} unusable.",
        loaded.len(),
        paths.len() - loaded.len()
    );

    let total_pairs = loaded.len() * loaded.len().saturating_sub(1) / 2;
    let mut done = 0usize;
    for i in 0..loaded.len() {
        for j in (i + 1)..loaded.len() {
            let (a_key, a_xml) = &loaded[i];
            let (b_key, b_xml) = &loaded[j];
            let pair_key = format!("{a_key}|{b_key}");
            done += 1;
            if done_pairs.contains(&pair_key) {
                continue;
            }
            print!("[{done}/{total_pairs}] {a_key} + {b_key} ... ");
            let _ = std::io::stdout().flush();
            process_pair(&mut report, a_key, a_xml, b_key, b_xml)?;
        }
    }

    println!("Done. Report at {}.", report_path.display());
    Ok(())
}

/// Runs Phase A (always) and, for a structurally clean merge, Phase B
/// against real Live -- appending exactly one report line for the pair
/// either way.
fn process_pair(
    report: &mut File,
    a_key: &str,
    a_xml: &str,
    b_key: &str,
    b_xml: &str,
) -> anyhow::Result<()> {
    let (merged, problems) = match phase_a(a_xml, b_xml) {
        Err(panic_payload) => {
            let msg = panic_message(&panic_payload);
            println!("PANIC: {msg}");
            return append_line(
                report,
                &json!({"a": a_key, "b": b_key, "error": format!("panicked: {msg}")}),
            );
        }
        Ok(Err(e)) => {
            println!("ERROR: {e}");
            return append_line(
                report,
                &json!({"a": a_key, "b": b_key, "error": e.to_string()}),
            );
        }
        Ok(Ok(pair)) => pair,
    };

    if !problems.is_empty() {
        println!("structurally unsound ({} problem(s))", problems.len());
        return append_line(
            report,
            &json!({"a": a_key, "b": b_key, "structural_problems": problems}),
        );
    }

    let live_result = phase_b(&merged)?;
    println!("{live_result}");
    append_line(
        report,
        &json!({"a": a_key, "b": b_key, "structural_problems": Vec::<String>::new(), "live_result": live_result}),
    )
}

fn all_track_indices(xml: &str) -> anyhow::Result<Vec<usize>> {
    let doc = Document::parse(xml)?;
    Ok((0..non_return_tracks(&doc)?.len()).collect())
}

/// The structural half of one pair's check, isolated in `catch_unwind`: the
/// merge/parse code is being pointed at arbitrary real-world files
/// (including old/malformed ones this batch expects), and an unhardened
/// edge case (e.g. a byte-slice boundary landing off a UTF-8 char boundary)
/// panics rather than returning `Err`. Since the report is the resume
/// checkpoint, an uncaught panic here would leave the pair unrecorded and
/// resume would walk straight back into it -- an infinite restart loop
/// instead of a recoverable skip.
fn phase_a(a_xml: &str, b_xml: &str) -> std::thread::Result<anyhow::Result<(String, Vec<String>)>> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let a_indices = all_track_indices(a_xml)?;
        let b_indices = all_track_indices(b_xml)?;
        let merged = build_mashup(&[
            MashupSource {
                xml: a_xml.to_string(),
                track_indices: a_indices,
            },
            MashupSource {
                xml: b_xml.to_string(),
                track_indices: b_indices,
            },
        ])?;
        let problems = diagnostics::check(&merged);
        Ok((merged, problems))
    }))
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Writes `merged` to a fresh temp file, verifies it against real Live, then
/// force-kills Live regardless of outcome. Returns `"pass"`,
/// `"dialog: <message>"`, or `"error: <message>"`. The temp file is deleted
/// unless the result is a dialog or error, which are kept for inspecting
/// by hand.
fn phase_b(merged: &str) -> anyhow::Result<String> {
    // A fresh name every pair: a fixed name risks `open` refocusing a stale
    // window instead of loading this pair's actual content.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temp_path = std::env::temp_dir().join(format!("mashup-batch-test-{unique}.als"));

    let live_result = match write_als(merged, &temp_path) {
        Ok(()) => match open_in_live_patient(&temp_path) {
            Ok(None) => "pass".to_string(),
            Ok(Some(message)) => format!("dialog: {message}"),
            Err(e) => format!("error: could not verify with Live: {e}"),
        },
        Err(e) => format!("error: failed to write temp file: {e}"),
    };
    force_kill_live();

    if !(live_result.starts_with("dialog") || live_result.starts_with("error")) {
        let _ = fs::remove_file(&temp_path);
    }
    Ok(live_result)
}

/// Drives real Ableton Live via `osascript`/System Events -- there is no
/// headless mode, so this is the only way to get a genuine "did Live accept
/// this file" answer. Returns `Ok(None)` on a clean open, `Ok(Some(message))`
/// if Live raised a dialog, and `Err` if the check itself couldn't run (e.g.
/// no Accessibility permission).
///
/// Tuned against one real Live 12 Suite install (verified by hand: the
/// process is literally named "Live"; a corrupt-file alert's subrole is
/// "AXDialog" with its message at `static text 1 of group 1` and an unnamed
/// OK button at `button 1 of group 1`; System Events enumerates none of
/// Live's windows until it's frontmost). Requires Accessibility (not just
/// Automation) permission for whatever process runs this.
///
/// Tuned for unattended batch use: a patient timeout (Live's Plugin Scanner
/// overhead is 15-90s per launch), and recognizing Live's own
/// "unexpectedly quit ... recover your work?" dialog -- produced by the
/// previous pair's force-kill, confirmed by hand -- as noise to dismiss
/// ("No", since every merged file here is throwaway) rather than this
/// pair's result.
fn open_in_live_patient(path: &Path) -> anyhow::Result<Option<String>> {
    let script = format!(
        r#"
        on run
            set targetPath to "{path}"
            do shell script "open " & quoted form of targetPath

            set timeoutTicks to 60
            repeat timeoutTicks times
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
                                if messageText is "" then
                                    try
                                        set messageText to value of (first static text of dialogWindow)
                                    end try
                                end if
                                if messageText contains "unexpectedly quit" then
                                    try
                                        click button "No" of dialogWindow
                                    on error
                                        try
                                            click button 1 of group 1 of dialogWindow
                                        end try
                                    end try
                                else
                                    try
                                        click button 1 of group 1 of dialogWindow
                                    end try
                                    return "DIALOG:" & messageText
                                end if
                            else if (count of windows) > 0 then
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

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()?;
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

/// SIGKILL is uncatchable, so unlike a graceful quit this is near-instant
/// and never risks the deadlock a `tell application "Live" to quit` can hit
/// against its own "Save changes?" sheet.
fn force_kill_live() {
    let _ = std::process::Command::new("pkill")
        .args(["-9", "-x", "Live"])
        .status();
    for _ in 0..20 {
        let still_running = std::process::Command::new("pgrep")
            .args(["-x", "Live"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !still_running {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Drops any original file that has an upgraded copy of itself sitting in
/// its own `LIVE 12/` subfolder -- otherwise the batch would test the stale
/// original too (reintroducing the schema-mismatch noise it was upgraded to
/// avoid) and pair it against everything, including its own upgraded twin, a
/// meaningless self-pair.
fn drop_superseded_originals(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let superseded: HashSet<PathBuf> = paths
        .iter()
        .filter_map(|p| {
            let parent = p.parent()?;
            let is_upgraded_copy =
                parent.file_name().and_then(|n| n.to_str()) == Some(LIVE_12_UPGRADE_DIR);
            if !is_upgraded_copy {
                return None;
            }
            let original_dir = parent.parent().unwrap_or_else(|| Path::new("."));
            Some(original_dir.join(p.file_name()?))
        })
        .collect();

    paths
        .into_iter()
        .filter(|p| !superseded.contains(p))
        .collect()
}

fn append_line(file: &mut File, value: &Value) -> anyhow::Result<()> {
    writeln!(file, "{value}")?;
    file.flush()?;
    Ok(())
}

/// Reads an existing report and returns the `"a"|"b"` keys of every pair
/// already recorded (structural failure, error, or full pass/dialog result
/// alike -- each is one complete, terminal line for that pair), so a
/// re-run of the same command skips work already done. A trailing line that
/// fails to parse (a hard kill mid-write) is dropped with a warning rather
/// than refusing to resume.
fn load_done_pairs(report_path: &Path) -> anyhow::Result<HashSet<String>> {
    let mut done = HashSet::new();
    let Ok(file) = File::open(report_path) else {
        return Ok(done);
    };
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => {
                if let (Some(a), Some(b)) = (
                    value.get("a").and_then(Value::as_str),
                    value.get("b").and_then(Value::as_str),
                ) {
                    done.insert(format!("{a}|{b}"));
                }
            }
            Err(e) => eprintln!("warning: dropping unparseable report line {}: {e}", i + 1),
        }
    }
    Ok(done)
}
