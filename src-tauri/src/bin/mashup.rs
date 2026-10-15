//! Mashes up one pair of projects, A as the base: Var 1 is every track of
//! both, and each later variation a random pick of units from each.
//! For trying out one mashup feature at a time; `mashup_batch_test` is the
//! regression check across a whole library.
//!
//! Usage: cargo run --bin mashup -- A.als B.als -o out.als [--variations n] [--seed s]

use malstrom_lib::als::mashup::{build_mashup, diagnostics, plan_variations, MashupSource};
use malstrom_lib::als::output_path::write_als;
use malstrom_lib::als::AlsInspector;
use roxmltree::Document;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str =
    "usage: mashup <A.als> <B.als> -o <out.als> [--variations n (1 to 6)] [--seed s]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((a, b, out, variations, seed)) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let seed = seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
    });

    match run(Path::new(a), Path::new(b), Path::new(out), variations, seed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String]) -> Option<(&str, &str, &str, usize, Option<u64>)> {
    let [a, b, flag, out, rest @ ..] = args else {
        return None;
    };
    if flag != "-o" {
        return None;
    }
    let (mut variations, mut seed) = (1, None);
    for pair in rest.chunks(2) {
        match pair {
            [f, v] if f == "--variations" => variations = v.parse().ok()?,
            [f, v] if f == "--seed" => seed = Some(v.parse().ok()?),
            _ => return None,
        }
    }
    Some((a, b, out, variations, seed))
}

fn run(a: &Path, b: &Path, out: &Path, variations: usize, seed: u64) -> anyhow::Result<()> {
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
    let docs = sources
        .iter()
        .map(|s| Document::parse(&s.xml))
        .collect::<Result<Vec<_>, _>>()?;
    let variations = plan_variations(&docs, variations, seed)?;

    let merged = build_mashup(&sources, &variations)?;
    let problems = diagnostics::check(&merged);
    if !problems.is_empty() {
        anyhow::bail!(
            "diagnostics failed, nothing written:\n  {}",
            problems.join("\n  ")
        );
    }
    write_als(&merged, out)?;
    println!("wrote {} (seed {seed})", out.display());
    Ok(())
}
