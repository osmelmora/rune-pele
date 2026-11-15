//! Differential test for the Ishikawa front end: for every source, this port's outline must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `ishikawa_front` op, and a source
//! Pele rejects must be rejected here with Pele's message, code and position.
//!
//! `tests/fixtures/ishikawa/harvest.json` holds the Ishikawa sources of Pele's own test suite and the
//! inputs of its mutated runs, written by `scripts/harvest-ishikawa.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/ishikawa/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list]
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().or_else(|| item["source"].as_str()).expect("a source").to_owned())
        .collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("ishikawa_front\t{source}")
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
    other_types: usize,
    /// Rejections with Pele's error, by its code.
    rejected: BTreeMap<String, usize>,
    failures: Vec<String>,
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
                    *self.rejected.entry(code_name(diagnostic.code).to_owned()).or_default() += 1;
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Err(error) => Some(format!("ours fails with {error}, pele answers {}", &theirs[..theirs.len().min(200)])),
        };
        if let Some(problem) = problem {
            let shown: String = source.chars().take(400).collect();
            self.failures.push(format!("{name}: {problem}\n    source: {shown:?}"));
        }
    }

    fn finish(self, what: &str) -> Self {
        let rejected: usize = self.rejected.values().sum();
        let total = self.identical + rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {rejected} rejected with Pele's error {:?}, {} of another type, {} different",
            self.identical,
            self.rejected,
            self.other_types,
            self.failures.len()
        );
        assert!(
            self.failures.is_empty(),
            "{} of {total} {what} differ from Pele:\n{}",
            self.failures.len(),
            self.failures[..self.failures.len().min(20)].join("\n")
        );
        self
    }

    fn rejected(&self, code: &str) -> usize {
        self.rejected.get(code).copied().unwrap_or(0)
    }
}

fn run(name: &str, sources: &[String]) -> Tally {
    let requests: Vec<String> = sources.iter().map(|source| request(source)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tally = Tally::default();
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let name = format!("{name} {index}");
        match std::panic::catch_unwind(|| pele_core::ishikawa_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested Ishikawa sources");
    assert!(tally.identical > 500, "the harvest holds Ishikawa diagrams Pele parses");
    assert!(tally.rejected("syntax") > 15, "the harvest holds sources Pele cannot parse");
}

#[test]
fn mutated_and_generated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated Ishikawa sources");
    assert!(tally.identical > 2000 && tally.rejected("syntax") > 100);
    let tally = run("generated", &harvest("generated")).finish("Pele's random Ishikawa token sequences and outlines");
    assert!(tally.identical > 1000 && tally.rejected("syntax") > 1000);
}

#[test]
fn worst_sources_match_pele() {
    run("worst", &harvest("worst")).finish("worst Ishikawa sources");
}

/// Hand-written sources for each place the grammar can stop and each rule of the db.
fn cases() -> Vec<String> {
    [
        "ishikawa-beta",
        "ishikawa-beta\n",
        "ishikawa-beta\n\n  Effect\n",
        "ishikawa-beta\n  Effect\n  ishikawa again\n",
        "ishikawa-beta Effect on one line",
        "ishikawa-beta%% nothing else",
        "ishikawa\nEffect\n\tOne\n\t\tTwo\n\t\t\tThree\n\t\t\t\tFour\n\tFive\n",
        "ishikawa-beta\n    Problem\nCause A\n  Subcause A1\nCause B\n",
        "ishikawa-beta\nEffect\n  A\n      B\n    C\n  D\n E\nF\n",
        "ISHIKAWA-BETA\nEffect\r\n  Cause\r\n    Sub\r\n",
        "Ishikawa-Beta\nEffect",
        "ishikawa-betax\nEffect",
        "ishikawa-bet\nEffect",
        "ishikawax\nEffect",
        "ishikawa_\nEffect",
        "ishikawa-\nEffect",
        "ishikawa beta\nEffect",
        "%% first\n\n%% second\nishikawa-beta\n%% inside\n  Effect\n\n\n  %% between\n  Cause\n    Sub %% not a comment\n\n",
        "ishikawa-beta\n  Effect\n   \n\n  Cause\n   ",
        "ishikawa-beta\n  Effect\n  \u{2028}x\n",
        "ishikawa-beta\n  Effect\n    a\u{feff}\n",
        "ishikawa-beta\n  Effect\n\u{b}\u{c}  Cause\n",
        "---\ntitle: Root causes\n---\nishikawa-beta\n  Effect\n    Cause\n",
        "ishikawa-beta\n  #35; &amp; <b>x</b>\n    \u{e9}\u{65e5}\u{672c}\n",
        "ishikawa-beta\n  __proto__\n    constructor\n      toString\n",
        "ishikawa-beta\n  E\n" ,
        "\n\nishikawa-beta\n  E\n",
        "  ishikawa-beta\n  E\n",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn hand_written_sources_match_pele() {
    let tally = run("case", &cases()).finish("hand-written Ishikawa sources");
    assert!(tally.rejected("syntax") >= 3 && tally.identical >= 15);
}

/// xorshift64, so the sources are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const LINES: &[&str] =
    &["Effect", "Cause", "A b c", "x", "%% note", "", "ishikawa", "<b>bold</b>", "Sub %% not", "\u{e9}"];
const INDENTS: &[&str] = &["", " ", "  ", "    ", "      ", "\t", "\t\t", " \t ", "\u{a0}"];
const BREAKS: &[&str] = &["\n", "\n", "\n", "\r\n", "\n\n", " \n", "\r", "\n  \n"];

/// An outline of lines at random depths, with blank lines and comments between them, under each
/// spelling of the keyword.
fn random_source(rng: &mut Rng) -> String {
    let mut src =
        String::from(rng.pick(&["ishikawa-beta", "ishikawa", "Ishikawa-BETA", "%% c\nishikawa-beta", "\nishikawa"]));
    src += rng.pick(&["\n", "\n", "\r\n", " ", "\n\n"]);
    for _ in 0..rng.below(14) {
        src += rng.pick(INDENTS);
        src += rng.pick(LINES);
        src += rng.pick(BREAKS);
    }
    src
}

#[test]
fn random_sources_match_pele() {
    let mut rng = Rng(0x15b1_a3a0_0001_0001);
    let sources: Vec<String> = (0..4000).map(|_| random_source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random Ishikawa sources");
    assert!(tally.identical > 1000 && tally.rejected("syntax") > 300);
}
