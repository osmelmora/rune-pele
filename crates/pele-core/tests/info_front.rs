//! Differential test for the info front end: for every source, this port's info model must come
//! out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `info_front` op, and a
//! source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/info/harvest.json` holds the corpus of Pele's info parser test and its
//! mutations, written by `scripts/harvest-info.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/info/harvest.json");
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

/// Puts every source through both front ends, and answers how many models were identical and
/// how many sources were rejected alike.
fn run(what: &str, sources: &[String]) -> (usize, usize) {
    let requests: Vec<String> = sources
        .iter()
        .map(|source| {
            let source = serde_json::to_string(source).expect("a string encodes");
            format!("info_front\t{}", source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"))
        })
        .collect();
    let answers = pele_oracle::ask(&requests);
    let (mut identical, mut rejected, mut other, mut failures) = (0, 0, 0, Vec::new());
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let ours = std::panic::catch_unwind(|| pele_core::info_front_json(source, ParseOptions::default()));
        let problem = match ours {
            Err(_) => Some("this port panics".to_owned()),
            Ok(Ok(ours)) if ours == *theirs => {
                if theirs.starts_with("{\"type\":\"info\",") {
                    identical += 1;
                } else {
                    other += 1;
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
                    rejected += 1;
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Ok(Err(error)) => Some(format!("ours fails with {error}, pele answers {theirs}")),
        };
        if let Some(problem) = problem {
            failures.push(format!("{what} {index}: {problem}\n    source: {:?}", &source[..source.len().min(300)]));
        }
    }
    println!(
        "{what}: {} inputs, {identical} models identical to Pele's, {rejected} rejected with Pele's error, {other} of another type, {} different",
        sources.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} {what} differ from Pele:\n{}",
        failures.len(),
        failures[..failures.len().min(8)].join("\n")
    );
    (identical, rejected)
}

#[test]
fn harvested_sources_match_pele() {
    let (identical, rejected) = run("harvested info sources", &harvest("sources"));
    assert!(identical > 15 && rejected > 10, "the harvest holds info diagrams Pele reads and rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let (identical, rejected) = run("mutated info sources", &harvest("mutated"));
    assert!(identical > 100 && rejected > 1000, "the mutations are read and rejected alike");
}

/// Pele's worst cases for parsing info (`tests/info/stress.test.ts`) at a tenth of their size,
/// and what the harvest reaches least.
fn cases() -> Vec<String> {
    let n = 5000;
    let mut out: Vec<String> = [
        "info",
        "info\n",
        "  info  ",
        "info showInfo",
        "info\tshowInfo\t\n\n",
        "info\r\nshowInfo\r\ntitle t\r\n",
        "info\naccTitle:   spaced   out   \naccDescr:   one   line   %% c\ntitle   a    title   %% c\n",
        "info\naccDescr {\n   first  \n\n\n   second   line\t\tthird  \n \r\n  }\ntitle t",
        "info\naccDescr{}\naccTitle:\ntitle",
        "info\ntitle #35; and #quot;quoted#quot;",
        "info\ntitle <b>bold</b> and *emphasis*",
        "info\ntitle first\ntitle second",
        "info\ntitle a\naccTitle: b\n",
        "info title t",
        "info\n\ntitle x showInfo",
        "info\ntitle\tx",
        "info\ntitlex",
        "info\naccDescr",
        "info\naccTitle",
        "info\n---\nyaml: here\n---\ntitle t",
        "info\n%%{ directive }%%\ntitle t",
        "---\ntitle: front\n---\ninfo",
        "---\ntitle: front\n---\ninfo\ntitle own",
        "---\ntitle: front\n---\ninfo\ntitle",
        "%%{init: {\"theme\":\"dark\"}}%%\ninfo showInfo %% c\n",
        "info\n\u{a0}title x",
        "info\ntitle \u{1f600} emoji",
        "info showInfo\nshowInfo",
        "info\n}",
        "Info",
        "INFO\ntitle x",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    out.push(format!("info{}showInfo", " ".repeat(n)));
    out.push(format!("info\n{}", "%% c\n".repeat(n / 5)));
    out.push(format!("info\n{}", "%%{\n".repeat(n / 4)));
    out.push(format!("info\n{}", "---\n".repeat(n / 4)));
    out.push(format!("info\naccDescr {{{}", " a\n".repeat(n / 3)));
    out.push(format!("info\naccDescr {{{}}}", " a\n".repeat(n / 3)));
    out.push("info ".repeat(n / 5));
    out.push(format!("info {}", "showInfo".repeat(n / 8)));
    out.push(format!("info\n{}", "title t\n".repeat(n / 8)));
    out
}

#[test]
fn cases_match_pele() {
    run("info cases", &cases());
}
