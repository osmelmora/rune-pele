//! Differential test for the sankey front end: for every source, this port's model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `sankey_front` op, and a
//! source Pele rejects must be rejected here with Pele's message, code and position, which is
//! placed in the text as written, before Mermaid's trimming of the rows.
//!
//! `tests/fixtures/sankey/harvest.json` holds the sankey sources of Pele's own test suite and
//! the inputs of its seeded mutation runs, written by `scripts/harvest-sankey.mjs`.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "sankey/random.rs"]
mod random;

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use random::{Rng, random_sankey};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/sankey/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source and its parse options as one oracle request on one line.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("sankey_front\t{source}\t{options}")
}

fn ours(source: &str, options: &Value) -> Result<String, RenderError> {
    let mut parse = ParseOptions::default();
    if let Some(config) = options.get("config") {
        parse.config = HostConfig::from_init_json(&config.to_string())?;
    }
    pele_core::sankey_front_json(source, parse)
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

fn nodes(model: &Value) -> impl Iterator<Item = &str> {
    model["model"]["nodes"].as_array().into_iter().flatten().filter_map(Value::as_str)
}

fn links(model: &Value) -> impl Iterator<Item = &Value> {
    model["model"]["links"].as_array().into_iter().flatten()
}

/// The value of a link, from the bits the JSON holds.
fn value(link: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(link[2].as_str().expect("bits"), 16).expect("hex"))
}

const FEATURES: &[(&str, Holds)] = &[
    ("a name with a quote", |m| nodes(m).any(|n| n.contains('"'))),
    ("a name with a comma", |m| nodes(m).any(|n| n.contains(','))),
    ("an empty name", |m| nodes(m).any(str::is_empty)),
    ("a name beyond ASCII", |m| nodes(m).any(|n| !n.is_ascii())),
    ("a name with an entity code", |m| nodes(m).any(|n| n.contains('&') || n.contains('\u{fb02}'))),
    ("a zero value", |m| links(m).any(|l| value(l) == 0.0)),
    ("a negative value", |m| links(m).any(|l| value(l) < 0.0)),
    ("a fractional value", |m| links(m).any(|l| value(l).fract() != 0.0)),
    ("a huge value", |m| links(m).any(|l| value(l) > 1e300)),
    ("a link from a node to itself", |m| links(m).any(|l| l[0] == l[1])),
    ("two links between one pair", |m| {
        let pairs: Vec<(&Value, &Value)> = links(m).map(|l| (&l[0], &l[1])).collect();
        pairs.iter().enumerate().any(|(i, pair)| pairs[i + 1..].contains(pair))
    }),
    ("a title", |m| !m["model"]["title"].is_null()),
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested sankey sources");
    assert!(tally.identical > 300, "the harvest holds sankey diagrams Pele parses");
    assert!(tally.rejected > 100, "the harvest holds sankey diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &plain(harvest("mutated"))).finish("mutated sankey sources");
    assert!(tally.kinds.get("syntax, parser").is_some_and(|&count| count > 100), "the mutations reach parse errors");
}

/// Hand-written sources for what the harvest reaches least: rows at the edges of the text, quotes
/// across lines, characters outside the CSV alphabet, and values that are not numbers.
fn cases() -> Vec<(String, Value)> {
    let sources = [
        "sankey",
        "sankey\n",
        "sankey\n\n\n",
        "  sankey  \n  a , b , 1  \n\n  c,d,2\t\n  ",
        "sankey-beta\r\na,b,1\r\n\r\nb,c,2\r\n",
        "sankey\r\ra,b,1",
        "sankey\n\u{feff}a,b,1\n\u{feff}\u{feff}b,c,2",
        "sankey\n a,b,1\n ,c,2\n  \t,d,3",
        "sankey\n\"a\nb\",c,1\n\"x\"\"y\",\"\",2",
        "sankey\n\"unclosed,b,1",
        "sankey\n\"a\" ,b,1",
        "sankey\na,b",
        "sankey\na,b,",
        "sankey\na,b,x",
        "sankey\na,b,1,2",
        "sankey\na,b,Infinity",
        "sankey\na,b,1e999",
        "sankey\na,b,-0",
        "sankey\na,b,  7kg",
        "sankey\na\tb,c,1",
        "sankey\na\u{7f},c,1",
        "sankey\n\u{1},c,1",
        "sankey\na&#1;,b,1",
        "sankey\na&#xFFFE;,b,1",
        "sankey\n\"a#35;\",b,1",
        "sankeyx\na,b,1",
        "sankey a,b,1",
        "SANKEY-BETA\na,b,1",
        "sanKey\na,b,1",
        "sankey\na,b,1\nsankey,x,2",
        "sankey\na,b,1\n\"",
        "sankey\na,b,1\n\"x",
        "---\ntitle: Front\n---\nsankey\na,b,1",
        "---\ntitle: \"\"\n---\nsankey\na,b,1",
        "%%{init: {\"sankey\": {\"width\": 10}}}%%\nsankey\na,b,1",
        "sankey\n%% c\na,b,1\n  %% d\n",
    ];
    sources.iter().map(|source| (source.to_string(), json!({}))).collect()
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("hand-written sankey sources");
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_sankey(&mut rng), json!({}))).collect();
    let tally = run("random", &cases).finish("random sankey diagrams");
    let missed: Vec<&str> =
        FEATURES.iter().map(|(name, _)| *name).filter(|name| !tally.holds.contains_key(name)).collect();
    println!("{} of {} model features held", FEATURES.len() - missed.len(), FEATURES.len());
    assert!(missed.is_empty(), "no random diagram's model holds {missed:?}");
}
