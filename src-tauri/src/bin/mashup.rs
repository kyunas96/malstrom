//! Mashes up one pair of projects: every track of A and B, A as the base.
//! For trying out one mashup feature at a time; `mashup_batch_test` is the
//! regression check across a whole library.
//!
//! Usage: cargo run --bin mashup -- A.als B.als -o out.als

use malstrom_lib::als::mashup::{build_mashup, diagnostics, non_return_tracks, MashupSource};
use malstrom_lib::als::output_path::write_als;
use malstrom_lib::als::AlsInspector;
use roxmltree::Document;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: mashup <A.als> <B.als> -o <out.als>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (a, b, out) = match args.as_slice() {
        [a, b, flag, out] if flag == "-o" => (a, b, out),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    match run(Path::new(a), Path::new(b), Path::new(out)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(a: &Path, b: &Path, out: &Path) -> anyhow::Result<()> {
    if out.exists() {
        anyhow::bail!("{} already exists -- choose a different name", out.display());
    }
    let sources = [a, b]
        .iter()
        .map(|path| {
            let xml = AlsInspector::open(path)?.raw_xml().to_string();
            let doc = Document::parse(&xml)?;
            let track_indices = (0..non_return_tracks(&doc)?.len()).collect();
            Ok(MashupSource { xml, track_indices })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let merged = build_mashup(&sources)?;
    let problems = diagnostics::check(&merged);
    if !problems.is_empty() {
        anyhow::bail!("diagnostics failed, nothing written:\n  {}", problems.join("\n  "));
    }
    write_als(&merged, out)?;
    println!("wrote {}", out.display());
    Ok(())
}
