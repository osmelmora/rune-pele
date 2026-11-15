//! Differential test for the ER front end: for every source, this port's ER model and ER graph
//! must come out as Pele 1.13.0's do, byte for byte in the JSON of the oracle's `er_front` op,
//! and a source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/er/harvest.json` holds the ER sources of Pele's own test suite, the corpus and
//! seeds of its ER parser test, and the inputs of its seeded mutation runs, written by
//! `scripts/harvest-er.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/er/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("er_front\t{source}")
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
            self.failures[..self.failures.len().min(20)].join("\n")
        );
        self
    }
}

fn run(name: &str, sources: &[String]) -> Tally {
    let requests: Vec<String> = sources.iter().map(|source| request(source)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tally = Tally::default();
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let name = format!("{name} {index}");
        match std::panic::catch_unwind(|| pele_core::er_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested ER sources");
    assert!(tally.identical > 1000, "the harvest holds ER diagrams Pele parses");
    assert!(tally.rejected > 100, "the harvest holds ER diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &harvest("mutated")).finish("mutated ER sources");
}

/// Hand-written sources for the paths the harvest reaches least: direction lines, subgraphs that
/// claim and rename, entity names that are subgraph ids, generics, keys, error positions and
/// the end of input in each lexer state.
const CASES: &[&str] = &[
    "erDiagram\nA direction LR B\nC\n  direction\n\n  TB x",
    "erDiagram\ndirection RL direction TB\nA",
    "erDiagram\nsubgraph s\ndirection LR\nA\nsubgraph t[\"Two words\" more]\nB\nA\nend\nend\ns ||--|| t : x",
    "erDiagram\nA ||--|| s : before\nsubgraph s\nB\nend\nA ||--|| s : after\ns {\nint x\n}\nsubgraph \" s \"\nC\nend",
    "erDiagram\nsubgraph a\nX\nend\nsubgraph b\nX\n X\nY\nend\nsubgraph a\nZ\nend\nstyle a fill:red\nclass a,b k\nclassDef k color:red,fill:blue",
    "erDiagram\nA {\n  List~int~ items PK, FK \"many\"\n  string? name UK\n  `a b` `c d`\n  a~b~c~ d\n  *x y\n}",
    "erDiagram\nA {\n  string\n}",
    "erDiagram\nA {\n  string name PK,\n}",
    "erDiagram\nA {\n  string? \n}",
    "erDiagram\nA {\n  int a\n",
    "erDiagram\nA {\n  `open",
    "erDiagram\nA ||--o{ B",
    "erDiagram\nA ||--o{ B :",
    "erDiagram\nA ||--x B : r",
    "erDiagram\nA || B : r",
    "erDiagram\nA[\"alias\"]:::k,m {\n int id\n}\nA[other]\nB:::k\nclassDef k,m fill:#f9f;\nclass A,B m",
    "erDiagram\nstyle A fill:red\nA\nstyle A stroke:#333 , stroke-width: 4px\nstyle A fill:red;",
    "erDiagram\nstyle A fill!red",
    "erDiagram\nclassDef c fill:red",
    "erDiagram\nsubgraph s\nA",
    "erDiagram\nsubgraph s\nend",
    "erDiagram\nend",
    "erDiagram\naccTitle:\naccDescr:   \nA",
    "erDiagram\naccTitle",
    "erDiagram\naccDescr {\n  one\n    two\n\n three\n}\nA",
    "erDiagram\naccDescr {\n unclosed",
    "erDiagram\n1 to 1+ 2 : one\n1.5 ||--|| 0+ x : y\n1--1 b : c\n1 .. 2 : d\n12 one or more to only one u : e",
    "erDiagram\nu--u u.-u u|o u|u : x",
    "erDiagram\nA }|..|{ B : \"\"\nC |o--o| D : \"a%b\"\nE }o..o{ F : \"line\\\\\"",
    "erDiagram\n\"\" ||--|| B : x",
    "erDiagram\n\u{e9}\u{b5} ||--|| \u{a0}x : \u{2028}",
    "erDiagram\nA {\n \u{b5} \u{c0}\u{2028}x\n}",
    "erDiagram\r\nA ||--o{ B : has\r\nB {\r\n int x\r\n}\r\n",
    "erDiagram A B C\nD { a b c d } E",
    "erDiagram\nA:::",
    "erDiagram\nclass A",
    "erDiagram\n#35; A",
    "---\ntitle: The title\n---\nerDiagram\nA",
];

#[test]
fn cases_match_pele() {
    let cases: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    run("case", &cases).finish("hand-written ER diagrams");
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

const NAMES: &[&str] =
    &["A", "B", "CUSTOMER", "LINE-ITEM", "\"Two words\"", "1", "12", "1.5", "\u{e9}", "a.b", "s", "t"];
const LEFT: &[&str] = &["||", "|o", "o|", "}o", "}|", "|{", "o{", "u", "one", "only one", "zero or one", "1+", "many"];
const RIGHT: &[&str] =
    &["||", "|o", "o|", "o{", "|{", "}|", "}o", "u", "one or more", "zero or many", "many(0)", "0+", "1"];
const LINES: &[&str] = &["--", "..", "-.", ".-", " to ", " optionally to "];
const TYPES: &[&str] = &["string", "int", "List~int~", "Map~K,V~", "`a type`", "string?", "*x", "\u{e9}t"];
const KEYS: &[&str] = &["PK", "FK", "UK", "pk", "PK, FK", "PK,UK"];
const STYLES: &[&str] =
    &["fill:#f9f", "stroke:#333,stroke-width:4px", "color:red", "fill:#bbf,color:#fff", "font-size:12pt"];

fn attribute(rng: &mut Rng) -> String {
    let mut out = format!("  {} {}", rng.pick(TYPES), rng.pick(&["name", "id", "`a name`", "x1", "first_name"]));
    if rng.chance(3) {
        out += &format!(" {}", rng.pick(KEYS));
    }
    if rng.chance(3) {
        out += &format!(" \"{}\"", rng.pick(&["a comment", "", "has ~T~"]));
    }
    out
}

fn statement(rng: &mut Rng, depth: usize) -> String {
    match rng.below(12) {
        0..=3 => {
            let classes = |rng: &mut Rng| if rng.chance(5) { ":::k" } else { "" };
            format!(
                "{}{} {}{}{} {}{} : {}",
                rng.pick(NAMES),
                classes(rng),
                rng.pick(LEFT),
                rng.pick(LINES),
                rng.pick(RIGHT),
                rng.pick(NAMES),
                classes(rng),
                rng.pick(&["has", "\"a label\"", "\"\"", "places", "r"])
            )
        }
        4..=6 => {
            let mut out = rng.pick(NAMES).to_owned();
            if rng.chance(3) {
                out += rng.pick(&["[Alias]", "[\"Long alias\"]"]);
            }
            if rng.chance(4) {
                out += ":::k";
            }
            if rng.chance(2) {
                out += " {\n";
                for _ in 0..rng.below(4) {
                    out += &attribute(rng);
                    out += "\n";
                }
                out += "}";
            }
            out
        }
        7 if depth < 2 => {
            let mut out = format!(
                "subgraph {}{}\n",
                rng.pick(&["s", "t", "\"Group\"", "A"]),
                rng.pick(&["", "", " [Title]", "[\"A title\" more]"])
            );
            for _ in 0..1 + rng.below(3) {
                out += &statement(rng, depth + 1);
                out += "\n";
            }
            out + "end"
        }
        8 => match rng.below(3) {
            0 => format!("style {} {}", rng.pick(&["A", "B", "s", "A,B"]), rng.pick(STYLES)),
            1 => format!("classDef {} {}", rng.pick(&["k", "k,m", "default"]), rng.pick(STYLES)),
            _ => format!("class {} {}", rng.pick(&["A", "A,B", "s"]), rng.pick(&["k", "k,m"])),
        },
        9 => rng.pick(&["direction LR", "direction BT", "direction TB", "  direction RL"]).to_owned(),
        10 => rng.pick(&["accTitle: t", "accDescr: d", "accDescr {\n a\n  b\n}", "%% a comment"]).to_owned(),
        _ => rng.pick(&["", "A", "B ||--|| B : self", ";", "end"]).to_owned(),
    }
}

#[test]
fn random_er_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<String> = (0..3000)
        .map(|_| {
            let mut source = String::from("erDiagram\n");
            for _ in 0..1 + rng.below(10) {
                source += rng.pick(&["", "  ", "\t"]);
                source += &statement(&mut rng, 0);
                source += rng.pick(&["\n", "\n", "\n\n", " "]);
            }
            source
        })
        .collect();
    let tally = run("random", &cases).finish("random ER diagrams");
    assert!(tally.identical > 1000, "many random ER diagrams parse");
    assert!(tally.rejected > 100, "some random ER diagrams are rejected");
}
