//! Differential test for the kanban front end: for every source, this port's board must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `kanban_front` op, and a source
//! Pele rejects must be rejected here with Pele's message, code and position.
//!
//! `tests/fixtures/kanban/harvest.json` holds the kanban sources of Pele's own test suite and the
//! inputs of its mutated runs, written by `scripts/harvest-kanban.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/kanban/harvest.json");
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
    format!("kanban_front\t{source}")
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
        match std::panic::catch_unwind(|| pele_core::kanban_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested kanban sources");
    assert!(tally.identical > 800, "the harvest holds kanban boards Pele parses");
    assert!(tally.rejected("syntax") > 100, "the harvest holds sources Pele cannot parse");
    assert!(tally.rejected("semantic") > 10, "the harvest holds boards Pele's db rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated kanban sources");
    assert!(tally.identical > 2000 && tally.rejected("syntax") > 2000);
}

#[test]
fn worst_sources_match_pele() {
    let mut sources = harvest("worst");
    sources.extend(harvest("configured"));
    run("worst", &sources).finish("worst and configured kanban sources");
}

/// Hand-written sources for each rule of the db and each place the metadata can fail.
fn cases() -> Vec<String> {
    [
        "kanban",
        "kanban\n",
        "kanban\n\n",
        "kanban\ntodo",
        "kanban\n  todo\n    a\n  done\n    b",
        "kanban\n    a\n  shallow",
        "kanban\n    a\n  shallow\n    b",
        "kanban\n    a\n  shallow\n  shallower",
        "kanban\n  todo\n    a\n      deep\n    b",
        "kanban\n  todo[To do]\n  :::wide\n  ::icon(inbox)\n    a[Task]@{ icon: star }\n    :::urgent large",
        "kanban\n::icon(star)\n:::big\ntodo",
        "kanban\n  todo\n    a[Task]@{ ticket: MC-2037, assigned: 'knsv', priority: 'High', icon: star }",
        "kanban\n  todo\n    a@{\n      assigned: knsv\n      label: \"A new\n        label\"\n      ticket: 12\n    }",
        "kanban\n  todo\n    a@{ label: \"x }",
        "kanban\n  todo\n    a@{ a: [ }",
        "kanban\n  todo\n    a@{\n}",
        "kanban\n  todo\n    a@{\n - x\n}",
        "kanban\n  todo\n    a@{}",
        "kanban\n  todo\n    a@{ shape: Rect }",
        "kanban\n  todo\n    a@{ shape: kanban_item }",
        "kanban\n  todo\n    a@{ shape: rect }",
        "kanban\n  todo\n    a@{ shape: \u{c9}t\u{e9} }",
        "kanban\n  todo\n    a@{ label: 0, ticket: 0, assigned: false, priority: null, icon: '' }",
        "kanban\n  todo\n    a@{ label: [1, [2, 3], null], ticket: {a: 1}, assigned: 1.50, priority: 1e21 }",
        "kanban\n  todo\n    a@{ label: true, ticket: .inf, assigned: -0 }",
        "kanban\n  todo@{ label: Column, ticket: T-1 }\n    a",
        "kanban\n  a@{ label: \"first\" }\n    b\n  ab@{ label: \"two\" } \n",
        "kanban\n  todo\n    a@{ label: \"x\" }@{ ticket: y }",
        "kanban\n  todo\n    a@{ label: \"multi\n   line\" }",
        "kanban\n  todo\n    \"quoted\"\n    [bracketed]\n    (rounded)\n    ((circle))\n    {{hex}}\n    ))bang((\n    )cloud(",
        "kanban\n  todo\n    a[\"`**bold** _it_`\"]",
        "kanban\n  todo %% comment\n    a %% x\n  %% only\n",
        "kanban\r\n  todo\r\n    a\r\n",
        "---\ntitle: Sprint 12\n---\nkanban\n  todo\n    a",
        "kanban\n  __proto__\n    constructor@{ __proto__: [polluted], constructor: 1, label: x }\n    :::__proto__",
        "kanban\n  todo\n    #35; &amp; <b>x</b>",
        "kanban\n  todo\n    a@b",
        "kanban\n  todo\n    a@{ ^ }",
        "kanban\n  todo\n    a@{ \"open",
        "kanban\n  todo\n    @{ label: x }",
        "kanbanx\n  todo",
        "kanban todo",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn hand_written_sources_match_pele() {
    let tally = run("case", &cases()).finish("hand-written kanban sources");
    assert!(tally.rejected("syntax") >= 5 && tally.rejected("semantic") >= 3 && tally.identical >= 25);
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
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const IDS: &[&str] = &["todo", "a", "id1", "card", "\u{e9}t\u{e9}", "x_y", "a b"];
const SHAPES: &[(&str, &str)] =
    &[("[", "]"), ("(", ")"), ("((", "))"), ("{{", "}}"), ("))", "(("), (")", "("), ("[\"", "\"]"), ("[\"`", "`\"]")];
const TEXTS: &[&str] = &["Task", "Two words", "**bold**", "a<br>b", "#35; &amp;", "\u{1f913}", "x y z"];
const KEYS: &[&str] = &["ticket", "assigned", "priority", "label", "icon", "shape", "other"];
const VALUES: &[&str] =
    &["MC-1", "'knsv'", "\"High\"", "very low", "12", "\"a\n  b\"", "rect", "Rect", "[1, 2]", "", "\"x"];
const NOISE: &[&str] = &["@{", "}", "(", "]", ":::", "::icon(", "\"", "%%", "@", "kanban", "^"];

/// A board of columns and cards at a few indentations, with metadata, decorations and comments,
/// most of it valid, some spoiled by an edit.
fn random_source(rng: &mut Rng) -> String {
    let mut src = String::from(rng.pick(&["kanban\n", "kanban\n", "kanban\n\n", "%% c\nkanban\n", "kanban"]));
    let indents = ["", "  ", "    ", "      ", "\t"];
    for _ in 0..rng.below(10) {
        let mut line = indents[rng.below(indents.len())].to_owned();
        match rng.below(10) {
            0 => line += &format!("::icon({})", rng.pick(IDS)),
            1 => line += &format!(":::{} {}", rng.pick(IDS), rng.pick(IDS)),
            2 => line += "%% a comment",
            _ => {
                if rng.chance(70) {
                    line += rng.pick(IDS);
                }
                if rng.chance(50) || line.trim().is_empty() {
                    let (open, close) = SHAPES[rng.below(SHAPES.len())];
                    line += &format!("{open}{}{close}", rng.pick(TEXTS));
                }
                if rng.chance(35) {
                    let pairs: Vec<String> =
                        (0..=rng.below(3)).map(|_| format!("{}: {}", rng.pick(KEYS), rng.pick(VALUES))).collect();
                    line += &format!("@{{ {} }}", pairs.join(", "));
                }
            }
        }
        if rng.chance(6) {
            let at = rng.below(line.len() + 1);
            if line.is_char_boundary(at) {
                line.insert_str(at, rng.pick(NOISE));
            }
        }
        src += &line;
        src += rng.pick(&["\n", "\n", "\n", "\r\n", "\n\n", " %% c\n"]);
    }
    src
}

#[test]
fn random_sources_match_pele() {
    let mut rng = Rng(0x6a2b_a2d0_0001_0001);
    let sources: Vec<String> = (0..4000).map(|_| random_source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random kanban sources");
    assert!(tally.identical > 1500 && tally.rejected("syntax") > 300 && tally.rejected("semantic") > 50);
}
