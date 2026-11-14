use pele_oracle::PeleRender;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage:
  pele render <file.mmd> [--engine rune|pele] [-o <out.svg>]
  pele parity [--type <type>] [--out <dir>]

render  writes the SVG for one Mermaid file. `rune` (default) is this port, `pele` is Pele 1.13.0.
parity  renders Pele's documentation corpus through both engines and reports, per diagram type,
        how many outputs are byte-identical. Files land in <dir> (default out/parity).
        Exits 1 when any ported source differs from Pele.";

#[derive(Default, Clone, Copy)]
struct Tally {
    sources: usize,
    identical: usize,
    different: usize,
    not_ported: usize,
    pele_errors: usize,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    match args.first().map(String::as_str) {
        Some("render") => {
            let Some(file) = args.get(1).filter(|a| !a.starts_with('-')) else {
                eprintln!("{USAGE}");
                return ExitCode::from(64);
            };
            render(PathBuf::from(file), flag("--engine").as_deref().unwrap_or("rune"), flag("-o").map(PathBuf::from))
        }
        Some("parity") => parity(flag("--type"), PathBuf::from(flag("--out").unwrap_or_else(|| "out/parity".into()))),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(64)
        }
    }
}

fn render(file: PathBuf, engine: &str, out: Option<PathBuf>) -> ExitCode {
    let source = match std::fs::read_to_string(&file) {
        Ok(source) => source,
        Err(e) => {
            eprintln!("{}: {e}", file.display());
            return ExitCode::from(66);
        }
    };
    let svg = match engine {
        "rune" => match pele_core::render(&source) {
            Ok(rendered) => rendered.svg,
            Err(pele_core::RenderError::NotPorted) => {
                let pele = pele_oracle::render_all(std::slice::from_ref(&source)).remove(0);
                eprintln!("not ported: rune-pele cannot render {} yet", pele.diagram_type().unwrap_or("this source"));
                return ExitCode::from(3);
            }
        },
        "pele" => match pele_oracle::render_all(std::slice::from_ref(&source)).remove(0) {
            PeleRender::Svg { svg, .. } => svg,
            PeleRender::Error { error, .. } => {
                eprintln!("pele: {error}");
                return ExitCode::from(65);
            }
        },
        other => {
            eprintln!("unknown engine {other:?}; use rune or pele");
            return ExitCode::from(64);
        }
    };
    match out {
        Some(path) => std::fs::write(&path, svg).map(|()| ExitCode::SUCCESS).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path.display());
            ExitCode::from(73)
        }),
        None => {
            println!("{svg}");
            ExitCode::SUCCESS
        }
    }
}

fn parity(only: Option<String>, dir: PathBuf) -> ExitCode {
    let corpus: Vec<(String, Vec<String>)> =
        pele_oracle::corpus().into_iter().filter(|(name, _)| only.as_ref().is_none_or(|o| o == name)).collect();
    if corpus.is_empty() {
        eprintln!("no corpus matches {only:?}; types are the <type>-docs.json names in vendor/pele/tests/corpus");
        return ExitCode::from(64);
    }
    let sources: Vec<String> = corpus.iter().flat_map(|(_, s)| s.iter().cloned()).collect();
    let mut theirs = pele_oracle::render_all(&sources).into_iter();
    let mut report: BTreeMap<String, Tally> = BTreeMap::new();
    for (name, sources) in &corpus {
        let tally = report.entry(name.clone()).or_default();
        let folder = dir.join(name);
        std::fs::create_dir_all(&folder).expect("parity dir is writable");
        for (i, source) in sources.iter().enumerate() {
            tally.sources += 1;
            std::fs::write(folder.join(format!("{i}.mmd")), source).expect("write source");
            let pele = theirs.next().expect("one oracle answer per source");
            let PeleRender::Svg { svg: want, width, height, .. } = pele else {
                tally.pele_errors += 1;
                continue;
            };
            std::fs::write(folder.join(format!("{i}.pele.svg")), &want).expect("write pele svg");
            match pele_core::render(source) {
                Err(pele_core::RenderError::NotPorted) => tally.not_ported += 1,
                Ok(ours) => {
                    std::fs::write(folder.join(format!("{i}.ours.svg")), &ours.svg).expect("write our svg");
                    if ours.svg == want && ours.width == width && ours.height == height {
                        tally.identical += 1;
                    } else {
                        tally.different += 1;
                    }
                }
            }
        }
    }
    println!(
        "{:<16} {:>7} {:>9} {:>9} {:>10} {:>11}",
        "type", "sources", "identical", "different", "not ported", "pele errors"
    );
    let mut total = Tally::default();
    for (name, t) in &report {
        println!(
            "{name:<16} {:>7} {:>9} {:>9} {:>10} {:>11}",
            t.sources, t.identical, t.different, t.not_ported, t.pele_errors
        );
        total.sources += t.sources;
        total.identical += t.identical;
        total.different += t.different;
        total.not_ported += t.not_ported;
        total.pele_errors += t.pele_errors;
    }
    let t = total;
    println!(
        "{:<16} {:>7} {:>9} {:>9} {:>10} {:>11}",
        "total", t.sources, t.identical, t.different, t.not_ported, t.pele_errors
    );
    let json: serde_json::Value = report
        .iter()
        .map(|(name, t)| {
            (name.clone(), serde_json::json!({"sources": t.sources, "identical": t.identical, "different": t.different, "not_ported": t.not_ported, "pele_errors": t.pele_errors}))
        })
        .collect::<serde_json::Map<_, _>>()
        .into();
    std::fs::write(dir.join("report.json"), serde_json::to_string_pretty(&json).expect("json")).expect("write report");
    println!("files and report.json in {}", dir.display());
    if total.different > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
