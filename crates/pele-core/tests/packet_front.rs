//! Differential test for the packet front end: for every source, this port's model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `packet_front` op, rows split
//! at their ends included, and a source Pele rejects must be rejected here with Pele's message,
//! code and position.
//!
//! `tests/fixtures/packet/harvest.json` holds the packet sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-packet.mjs`.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "packet/random.rs"]
mod random;

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use random::{Rng, random_packet};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/packet/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source and its parse options as one oracle request on one line.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("packet_front\t{source}\t{options}")
}

fn ours(source: &str, options: &Value) -> Result<String, RenderError> {
    let mut parse = ParseOptions::default();
    if let Some(config) = options.get("config") {
        parse.config = HostConfig::from_init_json(&config.to_string())?;
    }
    pele_core::packet_front_json(source, parse)
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

/// What a model can hold, each with how its JSON shows it.
type Holds = fn(&Value) -> bool;

fn blocks(model: &Value) -> impl Iterator<Item = &Value> {
    model["model"]["rows"].as_array().into_iter().flatten().filter_map(Value::as_array).flatten()
}

fn number(value: &Value) -> f64 {
    value.as_str().and_then(|text| text.parse().ok()).unwrap_or(f64::NAN)
}

const FEATURES: &[(&str, Holds)] = &[
    ("a one-bit field", |m| blocks(m).any(|b| b[0] == b[1])),
    ("a field over several bits", |m| blocks(m).any(|b| number(&b[1]) > number(&b[0]))),
    ("a field split at a row's end", |m| blocks(m).any(|b| number(&b[2]) == number(&b[1]) - number(&b[0]))),
    ("two rows or more", |m| m["model"]["rows"].as_array().is_some_and(|rows| rows.len() > 1)),
    ("a row that ends inside a field", |m| {
        m["model"]["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row.as_array().is_some_and(|row| row.len() == 1)))
    }),
    ("an empty label", |m| blocks(m).any(|b| b[3] == "")),
    ("a row width of its own", |m| m["model"]["bitsPerRow"] != "32"),
    ("no fields", |m| m["model"]["rows"].as_array().is_some_and(Vec::is_empty)),
    ("a title", |m| !m["model"]["title"].is_null()),
    ("an accessible title", |m| !m["model"]["accTitle"].is_null()),
    ("an accessible description", |m| !m["model"]["accDescr"].is_null()),
];

/// Where two texts first differ, with what each holds there.
fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(80).min(bytes.len())..(at + 80).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
}

#[derive(Default)]
struct Tally {
    identical: usize,
    rejected: usize,
    other_types: usize,
    failures: Vec<String>,
    holds: BTreeMap<&'static str, usize>,
    /// Rejections by Pele's code, and for syntax errors whether the lexer or the parser gave up.
    kinds: BTreeMap<String, usize>,
}

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<String, RenderError>, theirs: &str) {
        let problem = match ours {
            Ok(ours) if ours == theirs => {
                if theirs.starts_with("{\"type\":") {
                    self.other_types += 1;
                } else {
                    self.identical += 1;
                    // serde_json reads no lone surrogate, which a model can hold; such a model counts no feature.
                    if let Ok(model) = serde_json::from_str::<Value>(theirs) {
                        for (feature, holds) in FEATURES {
                            if holds(&model) {
                                *self.holds.entry(feature).or_default() += 1;
                            }
                        }
                    }
                }
                None
            }
            Ok(ours) => Some(first_difference(&ours, theirs)),
            Err(RenderError::Diagnostic(diagnostic)) => {
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
                    self.rejected += 1;
                    let kind = match diagnostic.code {
                        ErrorCode::Syntax if diagnostic.message.starts_with("Lexical") => "syntax, lexer",
                        ErrorCode::Syntax => "syntax, parser",
                        code => code_name(code),
                    };
                    *self.kinds.entry(kind.to_owned()).or_default() += 1;
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Err(error) => Some(format!("ours fails with {error}, pele answers {}", &theirs[..theirs.len().min(200)])),
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {source:?}"));
        }
    }

    fn finish(self, what: &str) -> Self {
        let total = self.identical + self.rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {} rejected with Pele's error, {} of another type, {} different",
            self.identical,
            self.rejected,
            self.other_types,
            self.failures.len()
        );
        for (kind, count) in &self.kinds {
            println!("    rejected by {kind}: {count}");
        }
        assert!(
            self.failures.is_empty(),
            "{} of {total} {what} differ from Pele:\n{}",
            self.failures.len(),
            self.failures[..self.failures.len().min(12)].join("\n")
        );
        self
    }
}

fn run(name: &str, cases: &[(String, Value)]) -> Tally {
    let requests: Vec<String> = cases.iter().map(|(source, options)| request(source, options)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tally = Tally::default();
    for (index, ((source, options), theirs)) in cases.iter().zip(&answers).enumerate() {
        let name = format!("{name} {index} with {options}");
        match std::panic::catch_unwind(|| ours(source, options)) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested packet sources");
    assert!(tally.identical > 300, "the harvest holds packet diagrams Pele parses");
    assert!(tally.rejected > 100, "the harvest holds packet diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &plain(harvest("mutated"))).finish("mutated packet sources");
    for kind in ["syntax, lexer", "syntax, parser", "semantic"] {
        assert!(tally.kinds.get(kind).is_some_and(|&count| count > 10), "the mutations reach {kind} errors");
    }
}

/// Hand-written sources for what the harvest reaches least: numbers too large for a double, rows
/// cut off at Pele's limit, and the forms the grammar rejects.
fn cases() -> Vec<(String, Value)> {
    let digits = "9".repeat(400);
    let sources = [
        "packet".to_owned(),
        "packet-beta".to_owned(),
        "packet\n0-15: \"a\"\n16: \"b\"\n+15: \"c\"".to_owned(),
        format!("packet\n+{digits}: \"x\"\n+1: \"y\""),
        format!("packet\n0-{digits}: \"x\""),
        "packet\n+1000000000: \"wide\"\n+1: \"after\"".to_owned(),
        "packet\n0-16: \"a\"\n18-20: \"b\"".to_owned(),
        "packet\n0-16: \"a\"\n25-20: \"b\"".to_owned(),
        "packet\n+0: \"a\"".to_owned(),
        "packet\n0-0: \"a\"\n1-0: \"b\"".to_owned(),
        "packet\n08: \"a\"".to_owned(),
        "packet\n0 - 15 : \"a\"".to_owned(),
        "packet\n0---: \"b\"".to_owned(),
        "packet\n0-1: \"a\"\n---\nfoo\n---\n2: \"b\"".to_owned(),
        "packet\ntitle T 0: \"a\"".to_owned(),
        "packet\n0: \"a\\nb\"".to_owned(),
    ];
    let mut out: Vec<(String, Value)> = sources.iter().map(|source| (source.clone(), json!({}))).collect();
    let body = "packet\n+4: \"a\"\n+8: \"b\"\n+20: \"c\"";
    for config in [
        json!({ "packet": { "bitsPerRow": 8 } }),
        json!({ "packet": { "bitsPerRow": 1 } }),
        json!({ "packet": { "bitsPerRow": 0.5 } }),
        json!({ "packet": { "bitsPerRow": 5000 } }),
        json!({ "packet": "not a record" }),
    ] {
        out.push((body.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("hand-written packet sources");
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_packet(&mut rng), json!({}))).collect();
    let tally = run("random", &cases).finish("random packet diagrams");
    let missed: Vec<&str> =
        FEATURES.iter().map(|(name, _)| *name).filter(|name| !tally.holds.contains_key(name)).collect();
    println!("{} of {} model features held", FEATURES.len() - missed.len(), FEATURES.len());
    assert!(missed.is_empty(), "no random diagram's model holds {missed:?}");
}
