//! Mashes up one pair of projects, A as the base: Var 1 is every track of
//! both, and each later variation a random pick of units from each.
//! For trying out one mashup feature at a time; `mashup_batch_test` is the
//! regression check across a whole library.
//!
//! B is moved into A's key unless `--no-transpose` is given. `--live-db`
//! points at the folder holding Live's `Live-files-*.db`, so drum tracks are
//! found the way `list_tracks` finds them; without it, by name only.
//!
//! Usage: cargo run --bin mashup -- A.als B.als -o out.als [--variations n] [--seed s]
//!        [--no-transpose] [--live-db DIR]

use malstrom_lib::als::livedb::find_db_files;
use malstrom_lib::als::mashup::{
    build_mashup, diagnostics, plan_variations, track_semitones, MashupSource,
};
use malstrom_lib::als::output_path::write_als;
use malstrom_lib::als::AlsInspector;
use roxmltree::Document;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: mashup <A.als> <B.als> -o <out.als> [--variations n (1 to 6)] [--seed s] [--no-transpose] [--live-db DIR]";

struct Args<'a> {
    a: &'a str,
    b: &'a str,
    out: &'a str,
    variations: usize,
    seed: Option<u64>,
    transpose: bool,
    live_db: Option<&'a str>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(args) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let seed = args.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
    });

    match run(&args, seed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String]) -> Option<Args<'_>> {
    let [a, b, flag, out, rest @ ..] = args else {
        return None;
    };
    if flag != "-o" {
        return None;
    }
    let mut args = Args {
        a,
        b,
        out,
        variations: 1,
        seed: None,
        transpose: true,
        live_db: None,
    };
    let mut rest = rest.iter();
    while let Some(f) = rest.next() {
        match f.as_str() {
            "--variations" => args.variations = rest.next()?.parse().ok()?,
            "--seed" => args.seed = Some(rest.next()?.parse().ok()?),
            "--no-transpose" => args.transpose = false,
            "--live-db" => args.live_db = Some(rest.next()?),
            _ => return None,
        }
    }
    Some(args)
}

fn run(args: &Args, seed: u64) -> anyhow::Result<()> {
    let out = Path::new(args.out);
    if out.exists() {
        anyhow::bail!(
            "{} already exists -- choose a different name",
            out.display()
        );
    }
    let xmls = [args.a, args.b]
        .iter()
        .map(|path| Ok(AlsInspector::open(Path::new(path))?.raw_xml().to_string()))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let docs = xmls
        .iter()
        .map(|xml| Document::parse(xml))
        .collect::<Result<Vec<_>, _>>()?;
    let variations = plan_variations(&docs, args.variations, seed)?;
    let b_semitones = if args.transpose {
        let live_db = args
            .live_db
            .map(|dir| find_db_files(Path::new(dir)))
            .unwrap_or_default();
        track_semitones(&docs[0], &docs[1], &live_db)?
    } else {
        vec![]
    };
    let shift = b_semitones.iter().copied().find(|&s| s != 0).unwrap_or(0);
    let sources = [
        MashupSource {
            xml: xmls[0].clone(),
            semitones: vec![],
        },
        MashupSource {
            xml: xmls[1].clone(),
            semitones: b_semitones,
        },
    ];

    let merged = build_mashup(&sources, &variations)?;
    let problems = diagnostics::check(&merged);
    if !problems.is_empty() {
        anyhow::bail!(
            "diagnostics failed, nothing written:\n  {}",
            problems.join("\n  ")
        );
    }
    write_als(&merged, out)?;
    println!(
        "wrote {} (seed {seed}, B moved {shift:+} semitones)",
        out.display()
    );
    Ok(())
}
