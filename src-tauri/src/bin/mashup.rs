//! Mashes up one pair of projects: every track of A and B, A as the base.
//! For trying out one mashup feature at a time; `mashup_batch_test` is the
//! regression check across a whole library.
//!
//! Usage: cargo run --bin mashup -- A.als B.als -o out.als [--variations n]
//!
//! Every variation is the full stack of both projects for now.

use malstrom_lib::als::mashup::{
    build_mashup, diagnostics, non_return_tracks, MashupSource, Variation,
};
use malstrom_lib::als::output_path::write_als;
use malstrom_lib::als::AlsInspector;
use roxmltree::Document;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: mashup <A.als> <B.als> -o <out.als> [--variations n]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match args.as_slice() {
        [a, b, flag, out] if flag == "-o" => Some((a, b, out, Some(1))),
        [a, b, flag, out, v, n] if flag == "-o" && v == "--variations" => {
            Some((a, b, out, n.parse().ok().filter(|n| (1..=6).contains(n))))
        }
        _ => None,
    };
    let Some((a, b, out, Some(variations))) = parsed else {
        eprintln!("{USAGE}  (n is 1 to 6)");
        return ExitCode::FAILURE;
    };

    match run(Path::new(a), Path::new(b), Path::new(out), variations) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(a: &Path, b: &Path, out: &Path, variations: usize) -> anyhow::Result<()> {
    if out.exists() {
        anyhow::bail!(
            "{} already exists -- choose a different name",
            out.display()
        );
    }
    let sources = [a, b]
        .iter()
        .map(|path| {
            Ok(MashupSource {
                xml: AlsInspector::open(path)?.raw_xml().to_string(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let everything = sources
        .iter()
        .map(|s| Ok((0..non_return_tracks(&Document::parse(&s.xml)?)?.len()).collect()))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let variations: Vec<Variation> = (1..=variations)
        .map(|i| Variation {
            name: format!("Var {i}"),
            picks: everything.clone(),
        })
        .collect();

    let merged = build_mashup(&sources, &variations)?;
    let problems = diagnostics::check(&merged);
    if !problems.is_empty() {
        anyhow::bail!(
            "diagnostics failed, nothing written:\n  {}",
            problems.join("\n  ")
        );
    }
    write_als(&merged, out)?;
    println!("wrote {}", out.display());
    Ok(())
}
