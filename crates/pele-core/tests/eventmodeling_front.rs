//! Differential test for the event modeling front end: for every source, this port's model must
//! come out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's
//! `eventmodeling_front` op, and a source Pele rejects must be rejected here with Pele's message,
//! code and position.
//!
//! `tests/fixtures/eventmodeling/harvest.json` holds the event modeling sources of Pele's test
//! suite and the inputs of its mutation runs, written by `scripts/harvest-eventmodeling.mjs`.

#[path = "eventmodeling/random.rs"]
mod random;

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/eventmodeling/harvest.json");
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
            format!("eventmodeling_front\t{}", source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"))
        })
        .collect();
    let answers = pele_oracle::ask(&requests);
    let (mut counts, mut failures) = (Counts::default(), Vec::new());
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let ours = std::panic::catch_unwind(|| pele_core::eventmodeling_front_json(source, ParseOptions::default()));
        let problem = match ours {
            Err(_) => Some("this port panics".to_owned()),
            Ok(Ok(ours)) if ours == *theirs => {
                if theirs.starts_with("{\"type\":\"eventmodeling\",") {
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
        "{what}: {} inputs, {} models identical to Pele's, {} rejected with Pele's syntax error, {} with its duplicate frame error, {} of another type, {} different",
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
    let counts = run("harvested event modeling sources", &harvest("sources"));
    assert!(counts.identical > 500 && counts.syntax > 50, "the harvest holds models Pele reads and rejects");
    assert!(counts.semantic > 0, "the harvest names a frame twice");
}

#[test]
fn mutated_sources_match_pele() {
    let counts = run("mutated event modeling sources", &harvest("mutated"));
    assert!(counts.identical > 500 && counts.syntax > 1000, "the mutations are read and rejected alike");
}

/// Pele's worst cases for event modeling (`tests/eventmodeling/stress.test.ts`) at a tenth of
/// their size, and what the harvest reaches least.
fn cases() -> Vec<String> {
    let n = 5000;
    let head = "eventmodeling\n";
    let id = |i: usize| format!("{:03}", i % 1000);
    let kinds = ["ui", "cmd", "evt", "rmo", "pcr"];
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let mut out: Vec<String> = [
        "eventmodeling",
        "eventmodeling ",
        "eventmodeling\ntf 01 evt A ->> 01\n",
        "eventmodeling\ntf 1 evt A\ntf 01 evt B\ntf 001 evt C\n",
        "eventmodeling\ntf 01 evt A\ntf 01 evt B\n",
        "eventmodeling\n\n\n%% c\ntf 01 evt A\n// c\ntf 02 evt B\ntf 01 evt C\n",
        "eventmodeling\ntf 01 evt A {}\ntf 02 evt B \"\"\ntf 03 evt C ''\ntf 04 evt D \"a{b}c\"\ntf 05 evt E \"{\"\n",
        "eventmodeling\ntf 01 evt A [[D]]\ndata D {\n\n  \n   a\n     b\n\n}\ndata D {\n second\n}\n",
        "eventmodeling\ntf 01 evt A [[D]]\ndata D {\r\n  a\r\n}\r\n",
        "eventmodeling\ntf 01 evt A [[D]]\ndata D {\n}x\n}\n",
        "eventmodeling\ntf 01 evt A [[D]]\ndata D {\n}",
        "eventmodeling\ndata D {\n}\ndata E {\n x\n}\ntf 01 evt A [[D]]\ntf 02 evt B [[E]]\n",
        "eventmodeling\nnote 01 {\n}\nnote 02 {\n y\n}\n",
        "eventmodeling\ntf 01 evt A [[D]]\ndata D {\n x\n} ",
        "eventmodeling\ntf 01 evt A [[D]]\ndata D {\u{a0}\n x\n}\n",
        "eventmodeling\ntf 01 evt A { x }\u{2028}}\n",
        "eventmodeling\ntf 01 evt A { x \u{2028} y }\n",
        "eventmodeling\ntf 01 evt a.b.c\ntf 02 evt a.b\ntf 03 cmd a.b\ntf 04 ui a.b\ntf 05 pcr b.a\n",
        "eventmodeling accTitle:   spaced   out   \ntf 01 ui A accDescr {\n   first  \n\n   second\n}\n",
        "eventmodeling title\ntf 01 ui A\n",
        "eventmodeling title  \ntf 01 ui A\n",
        "---\ntitle: Front\n---\neventmodeling title\ntf 01 ui A\n",
        "---\ntitle: Front\n---\neventmodeling title Own\ntf 01 ui A\n",
        "eventmodeling\ttitle T\ntf 01 ui A\n",
        "eventmodeling\ntf 01 ui A title B\n",
        "eventmodeling\ntf 01 ui A titled\n",
        "eventmodeling\ngwt 01 given evt A when then evt B\n",
        "eventmodeling\ngwt 01 given evt A\n",
        "eventmodeling\nnote 01 `json` {\n a\n}\nnote 01 {\n b\n}\n",
        "eventmodeling\ntf 01 evt A `` {x}\n",
        "eventmodeling\ntf 01 evt A `jsn` {x}\n",
        "eventmodeling\ntf 01 evt A ``\n",
        "eventmodeling\ntf 01 evt A `json`\n",
        "eventmodeling\nentity\n",
        "eventmodeling\nentity a.\n",
        "eventmodeling\ntf 01 evt \u{e9}\n",
        "eventmodeling\ntf 01 evt A /* inline */ ->> 01\n",
        "eventmodeling\ntf 01 evt A /* unclosed\n",
        "eventmodeling\n%%{init: {}}%%\ntf 01 evt A\n",
        "eventmodeling\n---\ntf 01 evt A\n",
        "eventmodeling\n---\nx: 1\n---\ntf 01 evt A\n",
        "EVENTMODELING\ntf 01 evt A\n",
        "eventmodeling\u{a0}\ntf 01 evt A\n",
        "eventmodeling\u{feff}tf 01 evt A\n",
        "eventmodeling\u{b}tf 01 evt A\n",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    out.push(head.to_owned() + &repeat(100, &|i| format!("tf {} {} E{i}\n", id(i), kinds[i % 5])));
    out.push(head.to_owned() + &repeat(100, &|i| format!("tf {} evt N{i}.E\n", id(i))));
    out.push(
        head.to_owned()
            + &repeat(100, &|i| {
                format!("tf {} evt E{i}{}\n", id(i), repeat(i.min(9), &|k| format!(" ->> {}", id(i - k - 1))))
            }),
    );
    out.push(format!("{head}tf 001 cmd A\ntf 002 evt B{}\n", " ->> 001".repeat(900)));
    out.push(head.to_owned() + &"tf 01 evt A\n".repeat(500));
    out.push(format!("{head}tf 01 evt A [[D]]\ndata D {{\n{}}}\n", "a: 1\n".repeat(1000)));
    out.push(format!("{head}tf 01 evt A {{ {} }}\n", "x".repeat(n)));
    out.push(format!("{head}tf 01 evt A {}{}\n", "{".repeat(n / 2), "}".repeat(n / 2)));
    out.push(format!("{head}tf 01 evt {}z\n", "a.".repeat(1000)));
    out.push(format!("{head}tf 01 evt {}\n", "a".repeat(n)));
    out.push(head.to_owned() + &"tf 01 evt A {\n".repeat(n / 14));
    out.push(format!("{head}data D {{\n{}", "}x\n".repeat(n / 3)));
    out.push(head.to_owned() + &format!("{}\n", "{".repeat(99)).repeat(n / 100));
    out.push(format!("{head}tf 01 evt A {}", "\"".repeat(n)));
    out.push(head.to_owned() + &"tf 01 evt A '\n".repeat(n / 14));
    out.push(format!("{head}/*{}", " x\n".repeat(n / 3)));
    out.push(head.to_owned() + &"/* ".repeat(n / 3));
    out.push(head.to_owned() + &"tf 01 evt A accDescr {\n".repeat(n / 23));
    out.push(head.to_owned() + &format!("x{}accDescr ", " ".repeat(90)).repeat(n / 100));
    out.push(head.to_owned() + &"a title b\n".repeat(n / 10));
    out.push(head.to_owned() + &"x %%{\n".repeat(n / 6));
    out.push(head.to_owned() + &"x\n---\n".repeat(n / 6));
    out.push(format!("{head}tf {}", "1".repeat(n)));
    out.push(head.to_owned() + &"tfevtuidatanote".repeat(n / 15));
    out
}

#[test]
fn cases_match_pele() {
    run("event modeling cases", &cases());
}

#[test]
fn random_models_match_pele() {
    let mut rng = random::Rng(0x0e7e_47a0_de11);
    let sources: Vec<String> = (0..3000).map(|_| random::source(&mut rng)).collect();
    let counts = run("random event models", &sources);
    assert!(counts.identical > 2000, "most random models are read");
    assert!(counts.syntax > 50 && counts.semantic > 20, "some random models are rejected, each way");
}
