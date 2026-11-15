//! Differential test for the architecture front end: for every source, this port's model must come
//! out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `architecture_front` op,
//! and a source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/architecture/harvest.json` holds the architecture sources of Pele's own test
//! suite and the inputs of its mutated and generated runs, written by
//! `scripts/harvest-architecture.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/architecture/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("architecture_front\t{source}")
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
        match std::panic::catch_unwind(|| pele_core::architecture_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested architecture sources");
    assert!(tally.identical > 500, "the harvest holds architecture diagrams Pele parses");
    assert!(tally.rejected("syntax") > 50, "the harvest holds sources Pele cannot parse");
    assert!(tally.rejected("semantic") > 10, "the harvest holds sources Pele's db rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated architecture sources");
    assert!(tally.rejected("syntax") > 1000 && tally.rejected("semantic") > 100);
}

/// Hand-written sources for each check of the db and each place the grammar can stop.
fn cases() -> Vec<String> {
    [
        "architecture-beta",
        "architecture-beta\n",
        "\n\n  architecture-beta  \n",
        "architecture-beta title T\nservice a",
        "architecture-beta\ntitle\nservice a",
        "---\ntitle: front\n---\narchitecture-beta\nservice a",
        "---\ntitle: front\n---\narchitecture-beta\ntitle own\nservice a",
        "architecture-beta\naccTitle:   spaced  out  \naccDescr {\n  one\n\n   two  \n}\nservice a",
        "architecture-beta\naccDescr: one line %% c\nservice a",
        "architecture-beta\naccDescr {never closed\nservice a",
        "architecture-beta\ngroup g(cloud)[G]\ngroup h[\"q \\\" q\"] in g\nservice a(server)[A] in h\nservice b 'txt'['s \\' s'] in g\njunction j in h",
        "architecture-beta\nservice a\nservice a",
        "architecture-beta\ngroup a\nservice a",
        "architecture-beta\nservice a\ngroup a",
        "architecture-beta\ngroup g in g",
        "architecture-beta\nservice a in a",
        "architecture-beta\nservice a in g",
        "architecture-beta\ngroup g in h\ngroup h",
        "architecture-beta\nservice s\nservice a in s",
        "architecture-beta\nservice s\njunction j in s",
        "architecture-beta\nservice a\na:R -- L:b",
        "architecture-beta\nservice b\na:R -- L:b",
        "architecture-beta\ngroup g\nservice a in g\na:R -- L:g",
        "architecture-beta\ngroup g\nservice a in g\ng:R -- L:a",
        "architecture-beta\ngroup g\nservice a in g\nservice b in g\na{group}:R -- L:b",
        "architecture-beta\ngroup g\nservice a in g\nservice b in g\na:R -- L:b{group}",
        "architecture-beta\ngroup g\ngroup h\nservice a in g\nservice b in h\na{group}:R <--> L:b{group}",
        "architecture-beta\nservice a\nservice b\nalign row a b a",
        "architecture-beta\ngroup g\nservice a\nalign column a g",
        "architecture-beta\nservice a\nalign row a c",
        "architecture-beta\nservice a\nservice b\nalign diagonal a b",
        "architecture-beta\nservice a\nalign row a",
        "architecture-beta\nservice a\nservice b\na:R - L:b",
        "architecture-beta\nservice a\nservice b\na:R -[x] L:b",
        "architecture-beta\nservice a\nservice b\na:R -x- L:b",
        "architecture-beta\nservice a\nservice b\na:X -- L:b",
        "architecture-beta\nservice a\nservice b\na R -- L:b",
        "architecture-beta\nservice a\nservice b\na:R -- L b",
        "architecture-beta\nservice a\nservice b\na:R -- L:b service c",
        "architecture-beta\nservice Load\nservice Rx",
        "architecture-beta\nservice a\nservice b\na:R --> L:b\na:L <-- R:b\na:T <--> B:b\na:B >--< T:b",
        "architecture-beta\nservice a(server)\"text\"",
        "architecture-beta\nservice a \"text\"(server)",
        "architecture-beta\nservice a(a:b-c)[]",
        "architecture-beta\nservice a[]",
        "architecture-beta\nservice a[\"]",
        "architecture-beta\nservice a[\"a]\"]",
        "architecture-beta\nservice a[ \"  padded  \" ]",
        "architecture-beta\nservice a['\\'\\\"']",
        "architecture-beta\nservice a[\"two\nlines\"]",
        "architecture-beta\nservice a[two\nlines]",
        "architecture-beta\nservice in-x\nservice inner\nservice titles\nservice title-y",
        "architecture-beta\nservice a\n  title\n  title-\n",
        "architecture-beta\n---\nskipped: yes\n---\nservice a",
        "architecture-beta\nservice a\n---\n",
        "architecture-beta\n%%{init: {}}%%\nservice a %%{ x }%%\n",
        "architecture-beta\nservice a\nservice b\na:R -- L:b %% c\n%% only\n",
        "architecture\nservice a",
        "architecture-betax\nservice a",
        "architecture-beta service a",
        "architecture-beta\n\u{e9}",
        "architecture-beta\nservice \u{e9}",
        "architecture-beta\nservice a[\u{1f600} \u{e9} \u{91c7}]",
        "architecture-beta\r\nservice a\r\nservice b\r\na:R -- L:b\r\n",
        "architecture-beta\nservice a\nservice b\na:R -[#35; &amp; <b>x</b>]- L:b",
        "architecture-beta\nservice __proto__\ngroup constructor\nservice toString in constructor",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn hand_written_sources_match_pele() {
    let tally = run("case", &cases()).finish("hand-written architecture sources");
    assert!(tally.rejected("semantic") >= 18, "every check of the db is reached");
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

const ICONS: &[&str] = &["(cloud)", "(server)", "(a:b-c)", "(x-1)", "(_)", "(database)"];
const TITLES: &[&str] =
    &["[T]", "[\"q\"]", "['q']", "[ sp  aced ]", "[\"]", "[a\"b]", "[\u{e9} \u{91c7}]", "[*b* _i_]", "[a<br>b]"];
const STRINGS: &[&str] = &["\"s\"", "'s'", "\"a\\\"b\"", "\"\"", "\"two words\""];
const SIDES: &[&str] = &["L", "R", "T", "B"];
const LINES: &[&str] = &["title A title", "title", "accTitle: acc", "accDescr: descr", "accDescr {\n a\n b\n}", "%% c"];
const NOISE: &[&str] = &[
    "service", "group", "junction", "align", "row", "in", ":", "--", "-", "<", ">", "{group}", "[x]", "(y)", "\"z\"",
    "Lx",
];
const SEPARATORS: &[&str] = &[" ", " ", " ", " ", " ", " ", "  ", "\t"];
const BREAKS: &[&str] = &["\n", "\n", "\n", "\n", "\n\n", "\r\n", " %% c\n", "\n  "];

/// A source that declares groups, then services and junctions, then edges and alignments between
/// them, as the db wants them in the main, with now and then an id used twice, a name that is not
/// declared, or a token edit.
fn random_source(rng: &mut Rng) -> String {
    let mut src = String::from(if rng.chance(97) { "architecture-beta" } else { "architecture" });
    src += rng.pick(BREAKS);
    let groups = rng.below(4);
    let nodes = 1 + rng.below(7);
    let group = |rng: &mut Rng, i: usize| if rng.chance(3) { "x".to_owned() } else { format!("g{i}") };
    let node = |rng: &mut Rng, i: usize| if rng.chance(2) { format!("g{i}") } else { format!("n{i}") };
    let mut statements: Vec<Vec<String>> = Vec::new();
    for g in 0..groups {
        let mut tokens = vec!["group".to_owned(), group(rng, g)];
        if rng.chance(40) {
            tokens.push(rng.pick(ICONS).to_owned());
        }
        if rng.chance(50) {
            tokens.push(rng.pick(TITLES).to_owned());
        }
        if g > 0 && rng.chance(40) {
            tokens.extend(["in".to_owned(), {
                let i = rng.below(g);
                group(rng, i)
            }]);
        }
        statements.push(tokens);
    }
    for i in 0..nodes {
        let mut tokens = if rng.chance(80) {
            let mut tokens = vec!["service".to_owned(), node(rng, i)];
            if rng.chance(40) {
                tokens.push(rng.pick(ICONS).to_owned());
            } else if rng.chance(30) {
                tokens.push(rng.pick(STRINGS).to_owned());
            }
            if rng.chance(50) {
                tokens.push(rng.pick(TITLES).to_owned());
            }
            tokens
        } else {
            vec!["junction".to_owned(), node(rng, i)]
        };
        if groups > 0 && rng.chance(60) {
            tokens.extend(["in".to_owned(), {
                let i = rng.below(groups);
                group(rng, i)
            }]);
        }
        statements.push(tokens);
    }
    for _ in 0..rng.below(nodes * 2 + 1) {
        let mut tokens = vec![{
            let i = rng.below(nodes + 1);
            node(rng, i)
        }];
        if rng.chance(15) {
            tokens.push("{group}".to_owned());
        }
        tokens.extend([":".to_owned(), rng.pick(SIDES).to_owned()]);
        if rng.chance(30) {
            tokens.push(rng.pick(&["<", ">"]).to_owned());
        }
        if rng.chance(75) {
            tokens.push("--".to_owned());
        } else {
            tokens.extend(["-".to_owned(), rng.pick(TITLES).to_owned(), "-".to_owned()]);
        }
        if rng.chance(40) {
            tokens.push(rng.pick(&["<", ">"]).to_owned());
        }
        tokens.extend([rng.pick(SIDES).to_owned(), ":".to_owned(), {
            let i = rng.below(nodes);
            node(rng, i)
        }]);
        if rng.chance(15) {
            tokens.push("{group}".to_owned());
        }
        statements.push(tokens);
    }
    if nodes > 1 && rng.chance(30) {
        let mut tokens = vec!["align".to_owned(), rng.pick(&["row", "column"]).to_owned()];
        for _ in 0..2 + rng.below(2) {
            tokens.push({
                let i = rng.below(nodes);
                node(rng, i)
            });
        }
        statements.push(tokens);
    }
    if rng.chance(30) {
        let at = rng.below(statements.len() + 1);
        statements.insert(at, vec![rng.pick(LINES).to_owned()]);
    }
    for mut tokens in statements {
        if rng.chance(4) {
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
    let mut rng = Rng(0x5eed_a5c4_17ec_7a7e);
    let sources: Vec<String> = (0..4000).map(|_| random_source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random architecture sources");
    assert!(tally.identical > 800, "random sources reach the db");
    assert!(tally.rejected("syntax") > 500 && tally.rejected("semantic") > 500);
}
