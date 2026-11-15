//! Differential test for the gantt front end: for every source, this port's model and each
//! task's schedule must come out as Pele 1.13.0's do, byte for byte in the JSON of the oracle's
//! `gantt_front` op, and a source Pele rejects must be rejected here with Pele's code, message and
//! position. A schedule that fails must fail with Pele's message and code.
//!
//! `tests/fixtures/gantt/harvest.json` holds the gantt sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-gantt.mjs`.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "gantt/random.rs"]
mod random;

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use random::{Rng, random_gantt};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Pele's parity clock, `Date.UTC(2026, 0, 15, 12)`.
const NOW: f64 = 1_768_478_400_000.0;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/gantt/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("gantt_front\t{source}\t{options}")
}

fn ours(source: &str, options: &Value) -> Result<String, RenderError> {
    let mut parse = ParseOptions::default();
    let mut now = NOW;
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "limit" => parse.limits.source_utf16 = usize::try_from(value.as_u64().expect("a count")).expect("a count"),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string()).expect("a config object"),
            "now" => now = value.as_f64().expect("a time"),
            other => panic!("{other} is not an option this test maps"),
        }
    }
    pele_core::gantt_front_json(source, parse, now)
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
struct Tally {
    identical: usize,
    rejected: usize,
    other_types: usize,
    /// How many identical models hold each thing a gantt model can hold.
    holds: BTreeMap<&'static str, usize>,
    failures: Vec<String>,
}

/// Whether a model, as the oracle's JSON, holds one thing.
type Holds = fn(&Value) -> bool;

/// What an identical model can hold, each with the test that finds it in the oracle's JSON.
const FEATURES: &[(&str, Holds)] = &[
    ("a task", |m| m["tasks"].as_array().is_some_and(|t| !t.is_empty())),
    ("a section", |m| m["sections"].as_array().is_some_and(|s| !s.is_empty())),
    ("a milestone", |m| tasks(m).any(|t| t["milestone"] == true)),
    ("a vertical marker", |m| tasks(m).any(|t| t["vert"] == true)),
    ("a critical task", |m| tasks(m).any(|t| t["crit"] == true)),
    ("a done task", |m| tasks(m).any(|t| t["done"] == true)),
    ("an active task", |m| tasks(m).any(|t| t["active"] == true)),
    ("a task with its own id", |m| {
        tasks(m).any(|t| t["raw"]["data"].as_str().is_some_and(|d| d.matches(',').count() >= 2))
    }),
    ("a task with no start", |m| tasks(m).any(|t| t["raw"]["start"].is_null())),
    ("an after reference", |m| {
        tasks(m).any(|t| t["raw"]["start"].as_str().is_some_and(|s| s.trim().starts_with("after")))
    }),
    ("an until reference", |m| tasks(m).any(|t| t["raw"]["end"].as_str().is_some_and(|s| s.starts_with("until")))),
    ("a clickable task", |m| tasks(m).any(|t| t["classes"].as_array().is_some_and(|c| !c.is_empty()))),
    ("a link", |m| m["links"].as_array().is_some_and(|l| !l.is_empty())),
    ("excluded days", |m| m["excludes"].as_array().is_some_and(|e| !e.is_empty())),
    ("included days", |m| m["includes"].as_array().is_some_and(|e| !e.is_empty())),
    ("inclusive end dates", |m| m["inclusiveEndDates"] == true),
    ("a top axis", |m| m["topAxis"] == true),
    ("a tick interval", |m| !m["tickInterval"].is_null()),
    ("a today marker", |m| m["todayMarker"].as_str().is_some_and(|s| !s.is_empty())),
    ("a weekday", |m| !m["weekday"].is_null()),
    ("a Friday weekend", |m| m["weekend"] == "friday"),
    ("a title", |m| !m["title"].is_null()),
    ("an accessible title", |m| !m["accTitle"].is_null()),
    ("an accessible description", |m| !m["accDescr"].is_null()),
    ("compact display", |m| m["displayMode"] == "compact"),
    ("a scheduled task", |m| schedule(m).any(|s| s[4] == true)),
    ("a task that never resolves", |m| schedule(m).any(|s| s[4] == false)),
    ("an end pushed out by excluded days", |m| schedule(m).any(|s| !s[2].is_null())),
    ("an end given as a date", |m| schedule(m).any(|s| s[3] == true)),
    ("an invalid end date", |m| schedule(m).any(|s| s[1] == "NaN")),
    ("a schedule that fails", |m| m["schedule"]["error"].is_string()),
    ("a schedule over the work limit", |m| m["schedule"]["code"] == "limit"),
];

fn tasks(model: &Value) -> impl Iterator<Item = &Value> {
    model["tasks"].as_array().into_iter().flatten()
}

fn schedule(model: &Value) -> impl Iterator<Item = &Value> {
    model["schedule"].as_array().into_iter().flatten()
}

/// Where two texts first differ, with what each holds there.
fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(80).min(bytes.len())..(at + 80).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
}

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<String, RenderError>, theirs: &str) {
        let problem = match ours {
            Ok(ours) if ours == theirs => {
                if theirs.starts_with("{\"type\":") {
                    self.other_types += 1;
                } else {
                    self.identical += 1;
                    let model: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
                    for (feature, holds) in FEATURES {
                        if holds(&model) {
                            *self.holds.entry(feature).or_default() += 1;
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested gantt sources");
    assert!(tally.identical > 1000, "the harvest holds gantt charts Pele parses");
    assert!(tally.rejected > 50, "the harvest holds gantt charts Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated gantt sources");
}

/// Hand-written charts for what the harvest reaches least, each with the options that reach it.
fn cases() -> Vec<(String, Value)> {
    let head = "gantt\n  dateFormat YYYY-MM-DD\n";
    let chain = format!(
        "{head}  A :a, 2024-01-01, 1d\n{}",
        (0..60)
            .map(|i| format!("  T :t{i}, after {}, 1d\n", if i == 0 { "a".to_owned() } else { format!("t{}", i - 1) }))
            .collect::<String>()
    );
    let backwards = format!(
        "{head}{}  Z :t30, 2024-01-01, 1d\n",
        (0..30).map(|i| format!("  T :t{i}, after t{}, 1d\n", i + 1)).collect::<String>()
    );
    vec![
        (format!("{head}  A :a, 2024-01-01, 3d\n"), json!({ "now": 0.0 })),
        (format!("{head}  A :3d\n  B :2d\n"), json!({ "now": 1_700_000_000_123.0 })),
        ("gantt\n  A :3d\n".to_owned(), json!({ "now": -8.64e15 })),
        (format!("{head}  A :a, after b, 1d\n  B :b, after a, 1d\n"), json!({})),
        (chain, json!({})),
        (backwards, json!({})),
        (format!("{head}  excludes weekends\n  T :2024-01-01, 99999999d\n"), json!({})),
        (
            format!("{head}  excludes weekends monday tuesday wednesday thursday friday\n  T :2024-01-01, 7d\n"),
            json!({}),
        ),
        (format!("{head}  excludes weekends\n{}", "  T :2024-01-01, 10950d\n".repeat(40)), json!({})),
        (
            "gantt\n  dateFormat DD.MM.YYYY [x]\n  excludes 01.01.2024 [x]\n  T :01.01.2024 [x], 20d\n".to_owned(),
            json!({}),
        ),
        ("gantt\n  dateFormat x\n  A :99999999999999999, 1d\n".to_owned(), json!({})),
        ("gantt\n  dateFormat X\n  A :1700000000, 1d\n  B :8640000000000000, 1d\n".to_owned(), json!({})),
        ("gantt\n  dateFormat x\n  inclusiveEndDates\n  A :a, 0, 8640000000000000\n".to_owned(), json!({})),
        (
            format!("{head}  inclusiveEndDates\n  A :a, 2024-01-01, 2024-01-05\n  B :b, 2024-01-01, 275760-09-13\n"),
            json!({}),
        ),
        (format!("{head}  A :a, 0001-01-01, 1d\n  B :b, -000001-01-01, 1d\n"), json!({})),
        (format!("{head}  A :a, Jan 5 2024, 1d\n  B :b, task 1, 1d\n  C :c, 2024/02/30, 1d\n"), json!({})),
        (format!("{head}  A :a, 10001-01-01, 1d\n"), json!({})),
        (
            format!(
                "{head}  A :a, 2024-01-01, until b\n  B :b, 2024-02-01, until a\n  C :c, 2024-03-01, until nothing\n"
            ),
            json!({}),
        ),
        (
            format!(
                "{head}  A :a, 2024-01-01, 1.5w\n  B :b, 2024-01-31, 1M\n  C :c, 2024-02-29, 1y\n  D :d, 2024-01-31, 0.5M\n"
            ),
            json!({}),
        ),
        (
            format!(
                "{head}  A :a, 2024-01-01, 3d\n  click a href \"https://example.com\"\n  click a,a call f(1)\n  click nothing href \"x\"\n"
            ),
            json!({}),
        ),
        (format!("{head}  A :a, 2024-01-01, 3d\n"), json!({ "config": { "gantt": { "displayMode": "compact" } } })),
        (
            format!("---\ndisplayMode: compact\n---\n{head}  A :a, 2024-01-01, 3d\n"),
            json!({ "config": { "gantt": { "displayMode": 7 } } }),
        ),
        ("gantt\n  accTitle: \n  title\n  accDescr {\n   one\n     two\n  }\n  A :1d\n".to_owned(), json!({})),
        ("gantt\nweekday\nmonday\nweekend   friday\nA :1d\n".to_owned(), json!({})),
        ("gantt\n  click a call f(\n".to_owned(), json!({})),
        ("gantt\n  click a\n".to_owned(), json!({})),
        ("gantt\n  A :crit, done, active, milestone, vert\n".to_owned(), json!({})),
        ("gantt\n  A :a, b, c, d\n".to_owned(), json!({})),
        ("gantt\n%%{init: {}}%%\n".to_owned(), json!({})),
        ("gantt\n  accTitle".to_owned(), json!({})),
    ]
}

#[test]
fn hand_written_charts_match_pele() {
    run("case", &cases()).finish("hand-written gantt charts");
}

#[test]
fn random_charts_match_pele() {
    let mut rng = Rng(0xd1b5_4a32_d192_ed03);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_gantt(&mut rng), json!({}))).collect();
    let tally = run("random", &cases).finish("random gantt charts");
    let missed: Vec<&str> =
        FEATURES.iter().map(|(name, _)| *name).filter(|name| !tally.holds.contains_key(name)).collect();
    for (feature, count) in &tally.holds {
        println!("  {count:5} hold {feature}");
    }
    let wanted = ["a schedule over the work limit", "a schedule that fails", "an invalid end date"];
    assert!(missed.iter().all(|name| wanted.contains(name)), "no random chart holds {missed:?}");
}

#[test]
fn every_feature_is_held_somewhere() {
    let mut all = plain(harvest("sources"));
    all.extend(cases());
    let mut rng = Rng(0xd1b5_4a32_d192_ed03);
    all.extend((0..4000).map(|_| (random_gantt(&mut rng), json!({}))));
    let tally = run("all", &all);
    let missed: Vec<&str> =
        FEATURES.iter().map(|(name, _)| *name).filter(|name| !tally.holds.contains_key(name)).collect();
    println!("{} of {} model features held", FEATURES.len() - missed.len(), FEATURES.len());
    assert!(missed.is_empty(), "no input holds {missed:?}");
    tally.finish("harvested, hand-written and random gantt charts together");
}
