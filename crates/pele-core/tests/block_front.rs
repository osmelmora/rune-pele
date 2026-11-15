//! Differential test for the block diagram front end: for every source, this port's model must
//! come out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `block_front` op,
//! and a source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/block/harvest.json` holds the block sources of Pele's own test suite, the
//! corpus of its block parser test, its hostile templates, and the inputs of its seeded mutation
//! runs, written by `scripts/harvest-block.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/block/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("block_front\t{source}")
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
        match std::panic::catch_unwind(|| pele_core::block_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested block sources");
    assert!(tally.identical > 1000, "the harvest holds block diagrams Pele parses");
    assert!(tally.rejected > 100, "the harvest holds block diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated block sources");
    assert!(tally.rejected > 1000, "the mutations hold block diagrams Pele rejects");
}

/// Hand-written sources for the paths the harvest reaches least: ids that repeat, statements that
/// name a block before it exists, link statements as composite headers, the limit on spaces, and
/// the end of input in each lexer state.
const CASES: &[&str] = &[
    "block\n A\n A[\"two\"]\n A(\"three\")\n A",
    "block\n block:A\n  B\n end\n A[\"leaf\"]\n block:A\n  B\n  C\n end",
    "block\n A\n block:A\n  B\n end",
    "block\n class A,B hot\n style A fill:red\n A B\n classDef hot color:red;,fill:blue",
    "block\n style A,B fill:red,stroke:blue\n A B",
    "block\n class C x\n style C stroke:red\n D",
    "block\n block:A --> B --> C\n  D\n end\n E --> F",
    "block\n block:g:3\n  a\n end\n block:h[\"Title\"]:2\n  b\n end",
    "block\n A:3 --> B:2",
    "block\n A:3 B:0 C:00 D:999999999999999999999999",
    "block\n a space:100001 b",
    "block\n space:60000 space:40000 space:1",
    "block\n space:60000 space:40000",
    "block\n a space:0 b space c",
    "block\n columns 0\n a",
    "block\n columns auto\n a\n columns 3\n b",
    "block\n columns 99999999999999999999\n a b",
    "block\n A[\"x",
    "block\n A[\"`x",
    "block\n A<[\"x\"]>(",
    "block\n A<[\"x\"]>(right",
    "block\n A<[\"x\"]>( up , down,x ,y)",
    "block\n A<[\"x\"]>(sideways)",
    "block\n A -- \"x",
    "block\n A -- \"x\" --> B -. \"y\" .-> C == \"z\" ==> D",
    "block\n A x--x B o--o C <--> D <==> E <-.-> F --- G === H -.- I ~~~ J",
    "block\n classDef",
    "block\n classDef DEFAULT fill:red",
    "block\n classDef DEFAULTx fill:red\n classDef a",
    "block\n class a",
    "block\n style a",
    "block\n style",
    "block\n accTitle: t\n a",
    "block\n accDescr { d }\n a",
    "block\n accDescr {",
    "block\n default",
    "block\n end",
    "block\n block\n a",
    "block\n block\n end",
    "block-beta\n a",
    "block-betax\n a",
    "block:g\n a",
    "block\n a\nend",
    "block\n (\"x\") a",
    "block\n a[\"\"]",
    "block\n a(-\"x\"-) b[|\"x\"] c>\"x\"] d)\"x\") e{\"x\"}",
    "block\n a((\"x\") b(((\"x\") c[(\"x\"])",
    "block\n root\n root --> a",
    "block\n __proto__ constructor toString\n classDef __proto__ fill:red\n class constructor,toString __proto__",
    "---\ntitle: The title\n---\nblock\n a",
    "---\ntitle: \"\"\n---\nblock\n a",
    "block\r\n a b\r\n a --> b\r\n",
    "block\n \u{e9}\u{b5} \u{a0}x \u{2028}y",
    "block\n a #35; b",
];

#[test]
fn cases_match_pele() {
    let cases: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    run("case", &cases).finish("hand-written block diagrams");
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

const IDS: &[&str] = &["A", "B", "C", "D", "g", "h", "id1", "x", "\u{e9}"];
const SHAPES: &[&str] = &[
    "",
    "",
    "[\"a\"]",
    "(\"b\")",
    "((\"c\"))",
    "(((\"d\")))",
    ">\"e\"]",
    "{\"f\"}",
    "{{\"g\"}}",
    "([\"h\"])",
    "[[\"i\"]]",
    "[(\"j\")]",
    "[/\"k\"/]",
    "[\\\"l\"\\]",
    "[/\"m\"\\]",
    "[\\\"n\"/]",
    "<[\"o\"]>(right)",
    "<[\" \"]>(left, up)",
    "<[\"p\"]>(x)",
    "<[\"q\"]>(y, down)",
    "(-\"r\"-)",
];
const LINKS: &[&str] = &[
    "-->",
    "---",
    "--x",
    "--o",
    "<-->",
    "x--x",
    "==>",
    "===",
    "-.->",
    "-.-",
    " -- \"l\" --> ",
    " == \"t\" ==> ",
    " -. \"d\" .-> ",
    "->",
];

fn node(rng: &mut Rng) -> String {
    let mut out = format!("{}{}", rng.pick(IDS), rng.pick(SHAPES));
    if rng.chance(5) {
        out += rng.pick(&[":2", ":3", ":0", ":10"]);
    }
    out
}

fn statement(rng: &mut Rng, depth: usize) -> String {
    match rng.below(12) {
        0..=3 => (0..1 + rng.below(4)).map(|_| node(rng)).collect::<Vec<_>>().join(" "),
        4 | 5 => {
            let mut out = node(rng);
            for _ in 0..1 + rng.below(3) {
                out += rng.pick(LINKS);
                out += &node(rng);
            }
            out
        }
        6 if depth < 3 => {
            let header = match rng.below(4) {
                0 => "block".to_owned(),
                1 => format!("block:{}", node(rng)),
                2 => format!("block:{}{}", rng.pick(IDS), rng.pick(&[":2", "[\"T\"]", ""])),
                _ => format!("block:{} --> {}", rng.pick(IDS), rng.pick(IDS)),
            };
            let mut out = header + "\n";
            for _ in 0..rng.below(4) {
                out += &statement(rng, depth + 1);
                out += "\n";
            }
            out + "end"
        }
        7 => rng.pick(&["columns 1", "columns 2", "columns 3", "columns auto", "columns 0"]).to_owned(),
        8 => rng.pick(&["space", "space:2", "space:0", "space:3"]).to_owned(),
        9 => match rng.below(3) {
            0 => format!(
                "classDef {} {}",
                rng.pick(&["k", "m", "default"]),
                rng.pick(&["fill:#f96", "color:red", "fill:red;,stroke:blue"])
            ),
            1 => format!("class {} {}", rng.pick(&["A", "A,B", "A, C", "g"]), rng.pick(&["k", "m"])),
            _ => format!(
                "style {} {}",
                rng.pick(&["A", "B", "A,B", "g"]),
                rng.pick(&["fill:red", "stroke:#333,stroke-width:4px", ""])
            ),
        },
        10 => rng.pick(&["%% a comment", "", "end", "accTitle: t"]).to_owned(),
        _ => node(rng),
    }
}

#[test]
fn random_block_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<String> = (0..4000)
        .map(|_| {
            let mut source = String::from(if rng.chance(8) { "block-beta\n" } else { "block\n" });
            for _ in 0..1 + rng.below(10) {
                source += rng.pick(&["", "  ", "\t"]);
                source += &statement(&mut rng, 0);
                source += rng.pick(&["\n", "\n", "\n\n", " "]);
            }
            source
        })
        .collect();
    let tally = run("random", &cases).finish("random block diagrams");
    assert!(tally.identical > 1000, "many random block diagrams parse");
    assert!(tally.rejected > 100, "some random block diagrams are rejected");
}
