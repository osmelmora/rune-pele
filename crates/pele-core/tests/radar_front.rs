//! Differential test for the radar front end: for every source, this port's model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `radar_front` op, and a source
//! Pele rejects must be rejected here with Pele's message, code and position.
//!
//! `tests/fixtures/radar/harvest.json` holds the radar sources of Pele's test suite and the inputs
//! of its mutation runs, written by `scripts/harvest-radar.mjs`.

#[path = "radar/random.rs"]
mod random;

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/radar/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

fn code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Unsupported => "unsupported-diagram",
        ErrorCode::Syntax => "syntax",
        ErrorCode::Semantic => "semantic",
        ErrorCode::Limit => "limit",
        _ => "unknown",
    }
}

#[derive(Default)]
struct Counts {
    identical: usize,
    syntax: usize,
    semantic: usize,
    other: usize,
}

/// Puts every source through both front ends.
fn run(what: &str, sources: &[String]) -> Counts {
    let requests: Vec<String> = sources
        .iter()
        .map(|source| {
            let source = serde_json::to_string(source).expect("a string encodes");
            format!("radar_front\t{}", source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"))
        })
        .collect();
    let answers = pele_oracle::ask(&requests);
    let (mut counts, mut failures) = (Counts::default(), Vec::new());
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let ours = std::panic::catch_unwind(|| pele_core::radar_front_json(source, ParseOptions::default()));
        let problem = match ours {
            Err(_) => Some("this port panics".to_owned()),
            Ok(Ok(ours)) if ours == *theirs => {
                if theirs.starts_with("{\"type\":\"radar\",") {
                    counts.identical += 1;
                } else {
                    counts.other += 1;
                }
                None
            }
            Ok(Ok(ours)) => Some(format!("models differ\n    ours: {ours}\n    pele: {theirs}")),
            Ok(Err(RenderError::Diagnostic(diagnostic))) => {
                let location = diagnostic.location.as_ref();
                let ours = json!({
                    "error": format!("PeleError: {}", diagnostic.message),
                    "code": code_name(diagnostic.code),
                    "type": diagnostic.kind.map(|kind| kind.name()),
                    "mermaidType": diagnostic.mermaid_type.map(|mermaid| mermaid.name()),
                    "line": location.map_or(0, |location| location.line),
                    "column": location.map_or(0, |location| location.column_utf16),
                    "snippet": location.map_or("", |location| location.snippet.as_str()),
                });
                let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
                if ours == theirs {
                    match diagnostic.code {
                        ErrorCode::Semantic => counts.semantic += 1,
                        _ => counts.syntax += 1,
                    }
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Ok(Err(error)) => Some(format!("ours fails with {error}, pele answers {theirs}")),
        };
        if let Some(problem) = problem {
            failures.push(format!(
                "{what} {index}: {problem}\n    source: {:?}",
                source.chars().take(300).collect::<String>()
            ));
        }
    }
    println!(
        "{what}: {} inputs, {} models identical to Pele's, {} rejected with Pele's syntax error, {} with its semantic error, {} of another type, {} different",
        sources.len(),
        counts.identical,
        counts.syntax,
        counts.semantic,
        counts.other,
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} {what} differ from Pele:\n{}",
        failures.len(),
        failures[..failures.len().min(8)].join("\n")
    );
    counts
}

#[test]
fn harvested_sources_match_pele() {
    let counts = run("harvested radar sources", &harvest("sources"));
    assert!(counts.identical > 500 && counts.syntax > 100, "the harvest holds charts Pele reads and rejects");
    assert!(counts.semantic > 0, "the harvest holds curves Pele cannot place on the axes");
}

#[test]
fn mutated_sources_match_pele() {
    let counts = run("mutated radar sources", &harvest("mutated"));
    assert!(counts.identical > 500 && counts.syntax > 1000, "the mutations are read and rejected alike");
}

/// Pele's worst cases for radar (`tests/radar/stress.test.ts`) at a tenth of their size, and what
/// the harvest reaches least.
fn cases() -> Vec<String> {
    let n = 5000;
    let list = |count: usize, item: &dyn Fn(usize) -> String| (0..count).map(item).collect::<Vec<_>>().join(", ");
    let mut out: Vec<String> = [
        "radar-beta\naxis a\ncurve c{1e3}",
        "radar-beta\naxis a\ncurve c{.5}",
        "radar-beta\naxis a\ncurve c{-1}",
        "radar-beta\naxis a, b\ncurve c{a 1}",
        "radar-beta\naxis a, b\ncurve c{b: 1, a: 2, b: 3}",
        "radar-beta\ncurve c{a 1}\naxis a",
        "radar-beta\naxis a\ncurve c{1}\ncurve d{a 1}, e{b 2}",
        "radar-beta\nshowLegend false, ticks 2.5, max 3, min 1, graticule polygon",
        "radar-beta\nticks 99",
        "radar-beta\nticks 33.5",
        "radar-beta\nmin 5\nmax 1",
        "radar-beta\ngraticule\ncircle",
        "radar-beta\nshowLegend\ntrue",
        "radar-beta\ngraticule polygon-x",
        "radar-beta\ngraticule circle-",
        "radar-beta\naxis polygon, true-ish, false",
        "radar-beta\naccDescr { a }\naccDescr {\naxis a",
        "radar-beta\naxis a accDescr: d",
        "radar-beta\naxis a\naccTitle: t %% c\ntitle  spaced  out  ",
        "radar-beta\ntitle\naccTitle:\naxis a",
        "---\ntitle: Front\n---\nradar-beta\ntitle\naxis a",
        "radar-beta\naxis a[\"\"]\ncurve c[\"\"]{1}",
        "radar-beta\naxis a['x\\'y'], b[\"\\u0041\"]",
        "radar-beta\ncurve c{1}, d{2}, e{3}",
        "radar-beta\ncurve c{1}\n\n\ncurve d{2}\n",
        "radar-beta:\n",
        "radar-beta:axis a",
        "  radar-beta\n",
        "RADAR-BETA\naxis a",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    out.push(format!(
        "radar-beta\naxis {}\ncurve c{{{}}}",
        list(500, &|i| format!("a{i}")),
        list(500, &|i| i.to_string())
    ));
    out.push(format!(
        "radar-beta\naxis {}\ncurve c{{{}}}",
        list(200, &|i| format!("a{i}")),
        list(200, &|i| format!("a{i}: {i}"))
    ));
    out.push(format!(
        "radar-beta\naxis a,b,c\n{}",
        (0..400).map(|i| format!("curve c{i}{{1,2,3}}\n")).collect::<String>()
    ));
    out.push(format!("radar-beta\n{}", "ticks 3\nmax 9\n".repeat(250)));
    out.push(format!("radar-beta\ncurve c{{{}}}", "0".repeat(n)));
    out.push(format!("radar-beta\nmax {}.5", "0".repeat(n)));
    out.push(format!("radar-beta\naxis a\ncurve c{{1}}\nmax {}", "9".repeat(n)));
    out.push(format!("radar-beta\ncurve c{{{}}}", "1.2".repeat(n / 3)));
    out.push(format!("radar-beta\naxis {}", "a".repeat(n)));
    out.push(format!("radar-beta\naxis a{}", "-".repeat(n)));
    out.push(format!("radar-beta\n{}", "axis".repeat(n / 4)));
    out.push(format!("radar-beta\n{}", "accDescr {".repeat(n / 10)));
    out.push(format!("radar-beta\naccDescr {{{}", " a\n".repeat(n / 3)));
    out.push(format!("radar-beta\naxis a[\"{}", "x".repeat(n)));
    out.push(format!("radar-beta\naxis a{}", "[".repeat(n / 2)));
    out.push(format!("radar-beta\naxis a\ncurve c{{{}1{}}}", "\n".repeat(2000), "\n".repeat(2000)));
    out.push(format!("radar-beta\n{}", "%%{\n".repeat(n / 4)));
    out.push(format!("radar-beta\n{}", "---\n".repeat(n / 4)));
    out.push(format!("radar-beta\n{}axis a", " ".repeat(n)));
    out
}

#[test]
fn cases_match_pele() {
    run("radar cases", &cases());
}

#[test]
fn random_charts_match_pele() {
    let mut rng = random::Rng(0x7ada_7c0f_fee5);
    let sources: Vec<String> = (0..3000).map(|_| random::source(&mut rng)).collect();
    let counts = run("random radar charts", &sources);
    assert!(counts.identical > 2000, "most random charts are read");
    assert!(counts.syntax > 20 && counts.semantic > 20, "some random charts are rejected, each way");
}
