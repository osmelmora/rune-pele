//! Differential test for the cynefin front end: for every source, this port's model must come
//! out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `cynefin_front` op, and a
//! source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/cynefin/harvest.json` holds the cynefin sources of Pele's own test suite and
//! the inputs of its mutated runs, written by `scripts/harvest-cynefin.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/cynefin/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("cynefin_front\t{source}")
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
            self.failures.push(format!("{name}: {problem}\n    source: {source:?}"));
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
        match std::panic::catch_unwind(|| pele_core::cynefin_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested cynefin sources");
    assert!(tally.identical > 300, "the harvest holds cynefin diagrams Pele parses");
    assert!(tally.rejected("syntax") > 100, "the harvest holds sources Pele cannot parse");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated cynefin sources");
    assert!(tally.identical > 500 && tally.rejected("syntax") > 1000);
}

/// Hand-written sources for each place the grammar can stop and each rule of the db.
fn cases() -> Vec<String> {
    [
        "cynefin-beta",
        "cynefin-beta:",
        "cynefin-beta:complex",
        "cynefin-beta: complex",
        "cynefin-betax",
        "cynefin-beta x",
        "\n\n  cynefin-beta  \n",
        "cynefin-beta title T\ncomplex",
        "cynefin-beta\ntitle\ncomplex",
        "---\ntitle: front\n---\ncynefin-beta\ncomplex",
        "---\ntitle: front\n---\ncynefin-beta\ntitle own",
        "---\ntitle: \"\"\n---\ncynefin-beta",
        "cynefin-beta\naccTitle:   spaced  out  \naccDescr {\n  one\n\n   two  \n}\ncomplex",
        "cynefin-beta\naccDescr: one line %% c\nclear",
        "cynefin-beta\naccDescr {never closed\nclear",
        "cynefin-beta\naccTitle: a title clear",
        "cynefin-beta\ncomplex \"a\" \"b\" clear\n\"c\"",
        "cynefin-beta\ncomplex\n\"first\"\ncomplex\n\"second\"\nclear\ncomplex\n",
        "cynefin-beta\nclear\nchaotic\nclear\n\"x\"",
        "cynefin-beta\ncomplex --> clear\nclear --> clear : \"self\"\nchaotic --> confusion : \"\"\nconfusion --> complex : 'single'",
        "cynefin-beta\ncomplex --> clear :",
        "cynefin-beta\ncomplex --> clear : x",
        "cynefin-beta\ncomplex --> clear \"x\"",
        "cynefin-beta\ncomplex --> \"x\"",
        "cynefin-beta\ncomplex -->",
        "cynefin-beta\ncomplex --> clear clear",
        "cynefin-beta\ncomplex --> nowhere",
        "cynefin-beta\n\"orphan\"",
        "cynefin-beta\ncomplexity",
        "cynefin-beta\ncomplicated\n\"a\"",
        "cynefin-beta\nComplex",
        "cynefin-beta\ncomplex\n\"unclosed",
        "cynefin-beta\ncomplex\n\"bad \\\nescape\"",
        "cynefin-beta\ncomplex\n\"esc \\\" \\n \\t \\\\ \\x \\0 end\"",
        "cynefin-beta\ncomplex\n\"multi\nline\"",
        "cynefin-beta\ncomplex\n\"#35; &amp; <b>x</b>\"",
        "cynefin-beta\ncomplex\n\"\u{1f600} \u{e9} \u{91c7}\"",
        "cynefin-beta\r\ncomplex\r\n\"a\"\r\n",
        "cynefin-beta\n---\nskipped: yes\n---\ncomplex",
        "cynefin-beta\n%%{init: {}}%%\ncomplex %%{ x }%%\n",
        "cynefin-beta\ncomplex %% c\n\"a\" %% d\n%% only\n",
        "cynefin-beta\n:",
        "cynefin-beta\n-->",
        "cynefin-beta\ncomplex : \"a\"",
        "cynefin-beta complex",
        "cynefin-beta\ttitle tabbed",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn hand_written_sources_match_pele() {
    let tally = run("case", &cases()).finish("hand-written cynefin sources");
    assert!(tally.rejected("syntax") >= 15 && tally.identical >= 20);
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
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const DOMAINS: &[&str] = &["complex", "complicated", "clear", "chaotic", "confusion"];
const ITEMS: &[&str] = &["\"a\"", "'b'", "\"two words\"", "\"\"", "\"esc \\\" q\"", "\"\u{e9}\u{91c7}\""];
const LINES: &[&str] = &["title A title", "title", "accTitle: acc", "accDescr: descr", "accDescr {\n a\n b\n}", "%% c"];
const NOISE: &[&str] = &["-->", ":", "\"x\"", "complexity", "Clear", "--", "->", "title", "cynefin-beta"];
const SEPARATORS: &[&str] = &[" ", " ", " ", " ", " ", "  ", "\t"];
const BREAKS: &[&str] = &["\n", "\n", "\n", "\n\n", "\r\n", " %% c\n", "\n  "];

/// A source of domain blocks, transitions and title lines, most of them valid, a few spoiled by
/// a token edit.
fn random_source(rng: &mut Rng) -> String {
    let mut src = String::from(if rng.chance(90) { "cynefin-beta" } else { "cynefin-beta:" });
    src += rng.pick(BREAKS);
    for _ in 0..=rng.below(8) {
        let mut tokens: Vec<String> = Vec::new();
        let roll = rng.below(100);
        if roll < 45 {
            tokens.push(rng.pick(DOMAINS).to_owned());
            for _ in 0..rng.below(4) {
                tokens.push(rng.pick(BREAKS).to_owned());
                tokens.push(rng.pick(ITEMS).to_owned());
            }
        } else if roll < 85 {
            tokens.extend([rng.pick(DOMAINS).to_owned(), "-->".to_owned(), rng.pick(DOMAINS).to_owned()]);
            if rng.chance(50) {
                tokens.extend([":".to_owned(), rng.pick(ITEMS).to_owned()]);
            }
        } else {
            tokens.push(rng.pick(LINES).to_owned());
        }
        if rng.chance(6) {
            let at = rng.below(tokens.len() + 1);
            match rng.below(3) {
                0 if at < tokens.len() => {
                    tokens.remove(at);
                }
                1 => tokens.insert(at, rng.pick(NOISE).to_owned()),
                _ if at < tokens.len() => tokens[at] = rng.pick(NOISE).to_owned(),
                _ => {}
            }
        }
        for token in tokens {
            src += &token;
            src += rng.pick(SEPARATORS);
        }
        src += rng.pick(BREAKS);
    }
    src
}

#[test]
fn random_sources_match_pele() {
    let mut rng = Rng(0xc7ef_1a5e_ed00_0001);
    let sources: Vec<String> = (0..4000).map(|_| random_source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random cynefin sources");
    assert!(tally.identical > 1500 && tally.rejected("syntax") > 500);
}
