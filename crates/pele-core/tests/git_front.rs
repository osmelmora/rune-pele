//! Differential test for the git graph front end: for every source, this port's model must come
//! out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `git_front` op, and a
//! source Pele rejects must be rejected here with Pele's message, code and position.
//!
//! `tests/fixtures/git/harvest.json` holds the git sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-git.mjs`.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "git/random.rs"]
mod random;

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use random::{Rng, random_git};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/git/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source and its parse options as one oracle request on one line.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("git_front\t{source}\t{options}")
}

fn ours(source: &str, options: &Value) -> Result<String, RenderError> {
    let mut parse = ParseOptions::default();
    if let Some(config) = options.get("config") {
        parse.config = HostConfig::from_init_json(&config.to_string())?;
    }
    pele_core::git_front_json(source, parse)
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

fn commits(model: &Value) -> impl Iterator<Item = &Value> {
    model["model"]["commits"].as_array().into_iter().flatten()
}

const FEATURES: &[(&str, Holds)] = &[
    ("a normal commit", |m| commits(m).any(|c| c["type"] == 0)),
    ("a reverse commit", |m| commits(m).any(|c| c["type"] == 1)),
    ("a highlight commit", |m| commits(m).any(|c| c["type"] == 2)),
    ("a merge", |m| commits(m).any(|c| c["type"] == 3)),
    ("a cherry-pick", |m| commits(m).any(|c| c["type"] == 4)),
    ("a merge with a type", |m| commits(m).any(|c| c["customType"].is_number())),
    ("a merge with its own id", |m| commits(m).any(|c| c["customId"] == true)),
    ("a merge with a made-up id", |m| commits(m).any(|c| c["customId"] == false)),
    ("a cherry-pick's own tag", |m| {
        commits(m).any(|c| {
            c["tags"]
                .as_array()
                .is_some_and(|t| t.iter().any(|t| t.as_str().is_some_and(|t| t.starts_with("cherry-pick:"))))
        })
    }),
    ("a cherry-picked merge", |m| {
        commits(m).any(|c| {
            c["tags"].as_array().is_some_and(|t| t.iter().any(|t| t.as_str().is_some_and(|t| t.contains("|parent:"))))
        })
    }),
    ("a commit with tags", |m| commits(m).any(|c| c["tags"].as_array().is_some_and(|t| !t.is_empty()))),
    ("a commit with a message", |m| commits(m).any(|c| c["type"].as_u64() < Some(3) && c["message"] != "")),
    ("a commit id taken twice", |m| m["model"]["warnings"].as_array().is_some_and(|w| !w.is_empty())),
    ("a branch with no commits", |m| {
        m["model"]["branches"].as_array().is_some_and(|b| b.iter().any(|pair| pair[1].is_null()))
    }),
    ("lanes out of creation order", |m| {
        let names: Vec<&Value> = m["model"]["branches"].as_array().into_iter().flatten().map(|pair| &pair[0]).collect();
        let lanes: Vec<&Value> = m["model"]["lanes"].as_array().into_iter().flatten().collect();
        names != lanes
    }),
    ("top to bottom", |m| m["model"]["direction"] == "TB"),
    ("bottom to top", |m| m["model"]["direction"] == "BT"),
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested git sources");
    assert!(tally.identical > 500, "the harvest holds git graphs Pele parses");
    assert!(tally.rejected > 100, "the harvest holds git graphs Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &plain(harvest("mutated"))).finish("mutated git sources");
    for kind in ["syntax, lexer", "syntax, parser", "semantic"] {
        assert!(tally.kinds.get(kind).is_some_and(|&count| count > 10), "the mutations reach {kind} errors");
    }
}

/// Hand-written graphs for what the harvest reaches least, each with the host config that reaches it.
fn cases() -> Vec<(String, Value)> {
    let digits = "9".repeat(400);
    let sources = [
        "gitGraph".to_owned(),
        "gitGraph:".to_owned(),
        "gitGraph LR:".to_owned(),
        "gitGraph TB :\ncommit".to_owned(),
        "gitGraph LR\ncommit".to_owned(),
        "gitGraph TB".to_owned(),
        "gitGraph\nbranch a order: 1\nbranch b order: 1\nbranch c\ncheckout main\nbranch d order: 0".to_owned(),
        format!("gitGraph\nbranch a order: {digits}\nbranch b order: {digits}\nbranch c order: 3"),
        "gitGraph\ncommit id:\"x\"\nbranch b\ncommit id:\"x\"\ncheckout main\ncommit".to_owned(),
        "gitGraph\ncommit id:\"x\"\nbranch b\ncommit id:\"x\"\ncheckout main\nmerge b".to_owned(),
        "gitGraph\ncommit\ncommit id:\"0-\"\ncommit id:\"1-f1f9cd2\"\ncommit".to_owned(),
        "gitGraph\ncommit id:\"a\"\nbranch b\ncherry-pick id:\"a\"".to_owned(),
        "gitGraph\ncommit id:\"a\"\nbranch b\ncommit id:\"c\"\ncheckout main\ncherry-pick id:\"c\" tag:\"\" tag:\"\"".to_owned(),
        "gitGraph\ncommit id:\"a\"\nbranch b\ncommit\ncheckout main\ncommit\nmerge b id:\"m\"\ncheckout b\ncherry-pick id:\"m\"".to_owned(),
        "gitGraph\ncommit id:\"a\"\nbranch b\ncommit\ncheckout main\ncommit\nmerge b id:\"m\"\ncheckout b\ncherry-pick id:\"m\" parent:\"zz\"".to_owned(),
        "gitGraph\ncommit\nmerge main".to_owned(),
        "gitGraph\ncommit\nbranch b\nmerge main".to_owned(),
        "gitGraph\nbranch b\ncommit\nmerge main".to_owned(),
        "gitGraph\ncommit\nmerge nowhere".to_owned(),
        "gitGraph\ncommit\nbranch b\ncheckout main\nmerge b".to_owned(),
        "gitGraph\ncommit\nbranch b\ncommit\ncheckout main\ncommit id:\"m\"\nmerge b id:\"m\"".to_owned(),
        "gitGraph\ncommit\nbranch b\ncommit\ncheckout main\nmerge b\nmerge b".to_owned(),
        "gitGraph\ncheckout nowhere".to_owned(),
        "gitGraph\nbranch main".to_owned(),
        "gitGraph\ncherry-pick id:\"\"".to_owned(),
        "gitGraph\ncommit id:\"a\"\ncherry-pick id:\"a\"".to_owned(),
        "gitGraph\ncommit id:\"a\"\nbranch b\ncommit id:\"c\"\nbranch e\ncherry-pick id:\"c\"".to_owned(),
        "gitGraph\naccDescr {\naccDescr: later\ncommit".to_owned(),
        "gitGraph\naccDescr {\ncommit\n}\naccDescr {\nnever".to_owned(),
        "gitGraph\ntitle\ncommit".to_owned(),
        "gitGraph\ncommit id: \"a\" id: \"b\" msg: \"m\" \"n\" type: NORMAL type: REVERSE".to_owned(),
        "gitGraph\ncommit type: OTHER".to_owned(),
        "gitGraph\nbranch b order: 1.5".to_owned(),
        "gitGraph\nbranch b order: -1".to_owned(),
        "gitGraph\ncommit\nbranch 12.5".to_owned(),
        "gitGraph\nswitch".to_owned(),
        "gitGraph\nmerge".to_owned(),
        "gitGraph\nmerge \"quoted\" tag: 'v'".to_owned(),
        "  \n\ngitGraph\n\n".to_owned(),
        "gitGraph\n  commit\n  %% a comment\n  commit".to_owned(),
    ];
    let mut out: Vec<(String, Value)> = sources.iter().map(|source| (source.clone(), json!({}))).collect();
    let hosted = "gitGraph\ncommit\nbranch b\ncommit\ncheckout main\ncommit\nmerge b";
    for config in [
        json!({ "gitGraph": { "mainBranchName": "trunk", "mainBranchOrder": 3 } }),
        json!({ "gitGraph": { "mainBranchName": 7, "mainBranchOrder": "1" } }),
        json!({ "gitGraph": { "mainBranchName": "b" } }),
        json!({ "gitGraph": "not a record" }),
    ] {
        out.push((hosted.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("hand-written git graphs");
}

#[test]
fn random_graphs_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_git(&mut rng), json!({}))).collect();
    let tally = run("random", &cases).finish("random git graphs");
    let missed: Vec<&str> =
        FEATURES.iter().map(|(name, _)| *name).filter(|name| !tally.holds.contains_key(name)).collect();
    println!("{} of {} model features held", FEATURES.len() - missed.len(), FEATURES.len());
    assert!(missed.is_empty(), "no random graph's model holds {missed:?}");
}
