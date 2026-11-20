use pele_oracle::PeleRender;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage:
  pele render <file.mmd> [--engine rune|pele] [-o <out.svg>]
  pele parity [--type <type>] [--out <dir>] [--corpus <dir>] [--expect-pele-errors <n>]

render  writes the SVG for one Mermaid file. `rune` (default) is this port, `pele` is Pele 1.13.0.
parity  renders Pele's documentation corpus through both engines and reports, per diagram type,
        how many outputs are byte-identical. Every <type>-docs.json of the corpus at the Pele pin
        must be there, read from <dir> of --corpus (default vendor/pele/tests/corpus). Files land
        in <dir> of --out (default out/parity), which each run empties first; it must be new,
        empty, or written by an earlier `pele parity`.
        Exits 1 when a source differs from Pele, when this port does not render one, when Pele
        rejects other than --expect-pele-errors sources (default 0), or when nothing is compared.";

/// The exit statuses the verify skill's contract names.
#[derive(Clone, Copy)]
enum Status {
    GateFailed = 1,
    NotPorted = 3,
    Usage = 64,
    Rejected = 65,
    NoInput = 66,
    Oracle = 70,
    CantWrite = 73,
}

/// Why the command stops, with the status it stops with.
struct Exit {
    status: Status,
    message: String,
}

impl Exit {
    fn new(status: Status, message: impl Into<String>) -> Self {
        Exit { status, message: message.into() }
    }

    fn usage(problem: impl std::fmt::Display) -> Self {
        Exit::new(Status::Usage, format!("{problem}\n{USAGE}"))
    }

    fn oracle(error: pele_oracle::OracleError) -> Self {
        Exit::new(Status::Oracle, format!("oracle: {error}"))
    }

    fn cant_write(path: &Path, error: std::io::Error) -> Self {
        Exit::new(Status::CantWrite, format!("{}: {error}", path.display()))
    }
}

enum Engine {
    Rune,
    Pele,
}

/// What the command line asks for, read once.
enum Command {
    Render { file: PathBuf, engine: Engine, out: Option<PathBuf> },
    Parity(Parity),
}

struct Parity {
    only: Option<String>,
    out: PathBuf,
    corpus: PathBuf,
    expect_pele_errors: usize,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let done = command(&args).and_then(|command| match command {
        Command::Render { file, engine, out } => render(&file, engine, out.as_deref()),
        Command::Parity(run) => parity(&run),
    });
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(exit) => {
            eprintln!("{}", exit.message);
            ExitCode::from(exit.status as u8)
        }
    }
}

/// One subcommand's positional arguments and `--option value` pairs. Every option takes a value.
/// An unknown or repeated option, or one with no value after it, is a usage error.
fn split(args: &[String], known: &[&str]) -> Result<(Vec<String>, BTreeMap<String, String>), Exit> {
    let mut positional = Vec::new();
    let mut options = BTreeMap::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if !arg.starts_with('-') {
            positional.push(arg.clone());
            continue;
        }
        if !known.contains(&arg.as_str()) {
            return Err(Exit::usage(format!("unknown option {arg}")));
        }
        let Some(value) = args.next().filter(|value| !value.starts_with('-')) else {
            return Err(Exit::usage(format!("{arg} needs a value")));
        };
        if options.insert(arg.clone(), value.clone()).is_some() {
            return Err(Exit::usage(format!("{arg} is given twice")));
        }
    }
    Ok((positional, options))
}

fn command(args: &[String]) -> Result<Command, Exit> {
    let Some((name, rest)) = args.split_first() else {
        return Err(Exit::usage("no command"));
    };
    match name.as_str() {
        "render" => {
            let (files, mut options) = split(rest, &["--engine", "-o"])?;
            let [file] = files.as_slice() else {
                return Err(Exit::usage("render takes one file"));
            };
            let engine = match options.remove("--engine").as_deref() {
                None | Some("rune") => Engine::Rune,
                Some("pele") => Engine::Pele,
                Some(other) => {
                    return Err(Exit::new(Status::Usage, format!("unknown engine {other:?}; use rune or pele")));
                }
            };
            Ok(Command::Render { file: PathBuf::from(file), engine, out: options.remove("-o").map(PathBuf::from) })
        }
        "parity" => {
            let (extra, mut options) = split(rest, &["--type", "--out", "--corpus", "--expect-pele-errors"])?;
            if let Some(arg) = extra.first() {
                return Err(Exit::usage(format!("parity takes no argument {arg:?}")));
            }
            let expect_pele_errors = match options.remove("--expect-pele-errors") {
                None => 0,
                Some(count) => count
                    .parse()
                    .map_err(|_| Exit::usage(format!("--expect-pele-errors takes a count, not {count:?}")))?,
            };
            Ok(Command::Parity(Parity {
                only: options.remove("--type"),
                out: options.remove("--out").map_or_else(|| PathBuf::from("out/parity"), PathBuf::from),
                corpus: options.remove("--corpus").map_or_else(pele_oracle::corpus_dir, PathBuf::from),
                expect_pele_errors,
            }))
        }
        other => Err(Exit::usage(format!("unknown command {other:?}"))),
    }
}

fn render(file: &Path, engine: Engine, out: Option<&Path>) -> Result<(), Exit> {
    let source =
        std::fs::read_to_string(file).map_err(|e| Exit::new(Status::NoInput, format!("{}: {e}", file.display())))?;
    let pele = |source: &String| {
        pele_oracle::try_render_all(std::slice::from_ref(source))
            .map(|mut answers| answers.remove(0))
            .map_err(Exit::oracle)
    };
    let svg = match engine {
        Engine::Rune => match pele_core::render(&source) {
            Ok(rendered) => rendered.svg,
            Err(pele_core::RenderError::NotPorted) => {
                let kind = pele(&source)?.diagram_type().unwrap_or("this source").to_owned();
                return Err(Exit::new(Status::NotPorted, format!("not ported: rune-pele cannot render {kind} yet")));
            }
            Err(error) => {
                return Err(Exit::new(Status::Rejected, format!("rune-pele: {error}")));
            }
        },
        Engine::Pele => match pele(&source)? {
            PeleRender::Svg { svg, .. } => svg,
            PeleRender::Error { error, .. } => return Err(Exit::new(Status::Rejected, format!("pele: {error}"))),
        },
    };
    match out {
        Some(path) => std::fs::write(path, svg).map_err(|e| Exit::cant_write(path, e)),
        None => {
            println!("{svg}");
            Ok(())
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Tally {
    sources: usize,
    identical: usize,
    different: usize,
    not_ported: usize,
    pele_errors: usize,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        self.sources += other.sources;
        self.identical += other.identical;
        self.different += other.different;
        self.not_ported += other.not_ported;
        self.pele_errors += other.pele_errors;
    }

    /// Why a finished run fails its gate: every source must be compared and identical, except
    /// the number of sources Pele itself is expected to reject.
    fn gate(&self, expect_pele_errors: usize) -> Vec<String> {
        let mut failures = Vec::new();
        if self.different > 0 {
            failures.push(format!("differ from Pele: {}", self.different));
        }
        if self.not_ported > 0 {
            failures.push(format!("not ported: {}", self.not_ported));
        }
        if self.pele_errors != expect_pele_errors {
            failures.push(format!("rejected by Pele: {}, expected {expect_pele_errors}", self.pele_errors));
        }
        if self.identical + self.different == 0 {
            failures.push("compared: 0".to_owned());
        }
        failures
    }
}

/// The file in an evidence folder that says `pele parity` wrote it, so a later run may empty it.
const MARK: &str = ".pele-parity";

/// Makes `dir` a folder that holds nothing but this command's mark. A folder that is already
/// there is emptied only when it is empty or an earlier run marked it; any other path is left
/// alone and refused.
fn fresh_evidence(dir: &Path) -> Result<(), Exit> {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                if !dir.join(MARK).is_file() {
                    return Err(Exit::new(
                        Status::CantWrite,
                        format!(
                            "{} is not empty and holds no {MARK}, so `pele parity` did not write it; give a new or empty folder",
                            dir.display()
                        ),
                    ));
                }
                std::fs::remove_dir_all(dir).map_err(|e| Exit::cant_write(dir, e))?;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Exit::cant_write(dir, e)),
    }
    std::fs::create_dir_all(dir).map_err(|e| Exit::cant_write(dir, e))?;
    let mark = dir.join(MARK);
    std::fs::write(&mark, "written by `pele parity`, which empties this folder on its next run\n")
        .map_err(|e| Exit::cant_write(&mark, e))
}

fn parity(run: &Parity) -> Result<(), Exit> {
    let no_input = |error: pele_oracle::CorpusError| Exit::new(Status::NoInput, error.to_string());
    if let Some(only) = &run.only
        && !pele_oracle::corpus_types().map_err(no_input)?.contains(only)
    {
        return Err(Exit::usage(format!(
            "no corpus matches {only:?}; types are the <type>-docs.json names in vendor/pele/tests/corpus at the Pele pin"
        )));
    }
    fresh_evidence(&run.out)?;
    let mut corpus = pele_oracle::load_corpus(&run.corpus).map_err(no_input)?;
    corpus.retain(|(name, _)| run.only.as_ref().is_none_or(|only| only == name));
    let sources: Vec<String> = corpus.iter().flat_map(|(_, s)| s.iter().cloned()).collect();
    let mut theirs = pele_oracle::try_render_all(&sources).map_err(Exit::oracle)?.into_iter();
    let write = |path: PathBuf, contents: &str| std::fs::write(&path, contents).map_err(|e| Exit::cant_write(&path, e));
    let mut report: BTreeMap<String, Tally> = BTreeMap::new();
    for (name, sources) in &corpus {
        let tally = report.entry(name.clone()).or_default();
        let folder = run.out.join(name);
        std::fs::create_dir_all(&folder).map_err(|e| Exit::cant_write(&folder, e))?;
        for (i, source) in sources.iter().enumerate() {
            tally.sources += 1;
            write(folder.join(format!("{i}.mmd")), source)?;
            let (want, width, height) = match theirs.next().expect("one oracle answer per source") {
                PeleRender::Svg { svg, width, height, .. } => (svg, width, height),
                PeleRender::Error { error, .. } => {
                    eprintln!("{name} {i}: Pele rejects the source: {error}");
                    tally.pele_errors += 1;
                    continue;
                }
            };
            write(folder.join(format!("{i}.pele.svg")), &want)?;
            match pele_core::render(source) {
                Err(pele_core::RenderError::NotPorted) => tally.not_ported += 1,
                Err(error) => {
                    eprintln!("{name} {i}: rune-pele rejects a source Pele renders: {error}");
                    tally.different += 1;
                }
                Ok(ours) => {
                    write(folder.join(format!("{i}.ours.svg")), &ours.svg)?;
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
        total.add(t);
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
    write(run.out.join("report.json"), &serde_json::to_string_pretty(&json).expect("json"))?;
    println!("corpus from {}", run.corpus.display());
    if let Ok(node) = pele_oracle::node() {
        println!("oracle on node {} at {}", node.version, node.path.display());
    }
    println!("files and report.json in {}", run.out.display());
    let failures = total.gate(run.expect_pele_errors);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Exit::new(Status::GateFailed, format!("parity: FAIL ({})", failures.join("; "))))
    }
}

#[cfg(test)]
mod tests {
    use super::Tally;

    /// Every type of Pele 1.13.0 is ported, so no corpus run reaches this rule.
    #[test]
    fn a_source_this_port_does_not_render_fails_the_gate() {
        let tally = Tally { sources: 2, identical: 1, not_ported: 1, ..Tally::default() };
        assert_eq!(tally.gate(0), ["not ported: 1"]);
        assert!(Tally { not_ported: 0, ..tally }.gate(0).is_empty());
    }
}
