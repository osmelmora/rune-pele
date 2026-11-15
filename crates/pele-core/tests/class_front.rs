//! Differential test for the class diagram front end: for every source, this port's class model
//! and class graph must come out as Pele 1.13.0's do, byte for byte in the JSON of the oracle's
//! `class_front` op, and a source Pele rejects must be rejected here with Pele's message and
//! position.
//!
//! `tests/fixtures/class/harvest.json` holds the class sources of Pele's own test suite, the
//! grammar tour of its class parser test, and the inputs of its seeded mutation runs, written by
//! `scripts/harvest-class.mjs`.

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/class/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("class_front\t{source}\t{options}")
}

fn parse_options(options: &Value) -> ParseOptions {
    let mut parse = ParseOptions::default();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "config" => parse.config = HostConfig::from_init_json(&value.to_string()).expect("a config object"),
            other => panic!("{other} is not a parse option this test maps"),
        }
    }
    parse
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

fn run(name: &str, cases: &[(String, Value)]) -> Tally {
    let requests: Vec<String> = cases.iter().map(|(source, options)| request(source, options)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tally = Tally::default();
    for (index, ((source, options), theirs)) in cases.iter().zip(&answers).enumerate() {
        let name = format!("{name} {index} with {options}");
        match std::panic::catch_unwind(|| pele_core::class_front_json(source, parse_options(options))) {
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested class sources");
    assert!(tally.identical > 3000, "the harvest holds class diagrams Pele parses");
    assert!(tally.rejected > 1000, "the harvest holds class diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated class sources");
}

/// Hand-written sources for the paths the harvest reaches least: dotted and nested namespace
/// names, flat namespaces, classDef order, lookups by digit, link targets and error positions.
const CASES: &[&str] = &[
    "classDiagram\nnamespace A.B.C {\nclass X\n}\nnamespace A.B {\nclass Y\n}\nnamespace A {\nnamespace B.D {\nclass Z\n}\n}",
    "classDiagram\nnamespace A[\"Alpha\"] {\nclass X\n}\nnamespace A[\"Again\"] {\nclass Y\n}\nnamespace A.B[\"\"] {\nnote \"n\"\n}",
    "---\nconfig:\n  class:\n    hierarchicalNamespaces: false\n---\nclassDiagram\nnamespace A.B.C {\nclass X\n}\nnamespace A {\nclass Y\nnote \"n\"\n}\nclass Z",
    "classDiagram\nclass A:::k\nclassDef k fill:#f00,color:red\nclass B:::k\nclassDef k,m stroke:blue\ncssClass \"B,C\" m",
    "classDiagram\nclass classId-1\ncssClass \"1\" k\nlink 1 \"https://example.com\"\ncallback 1 \"cb\" \"tip\"",
    "classDiagram\nclass A\nlink A \"  \" _top\nclick A href \"&#106;avascript:alert(1)\" \"tip\" _parent\nclick A call f(1, \"a\") \"tip2\"",
    "classDiagram\nA <|-- B : x\nA \"1\" *-- \"many\" C : has\nA o.. D\nA <.. E\nA ..|> F\nA --() G\nH ()-- A\nI ()--() J\nK <|--|> L",
    "classDiagram\nclass A~T~\nA~T~ : +List~int~ items$\nA : -run(a, b) bool*\nA : #f()$\nA : ~g() List~T~\nA : <<interface>>\nA : x)\nA : (a)",
    "classDiagram\nclass A {\n  +a\n  -b()\n  <<service>>\n  \"q\"\n  [*]\n}\nclass B[\"Label\"] {\n}\nclass C <<i>> {\n+x\n}",
    "classDiagram\nnote \"free\"\nnote for A \"for a class not yet declared\"\nclass A\nnote for `B c` \"backticks\"",
    "classDiagram\naccTitle:   Title  \naccDescr {\n  one\n    two\n  }\nclass A",
    "classDiagram\ndirection LR\nclass A\ndirection RL\ndirection\nTB",
    "classDiagram\nclass A\nstyle A fill:#f9f,stroke:#333 , stroke-width:4px\nstyle Missing fill:red",
    "classDiagram\nclass A {",
    "classDiagram\nclass A {\n+x\n",
    "classDiagram\nnamespace N {\nclass A\n",
    "classDiagram\nA -- ",
    "classDiagram\nA <| B",
    "classDiagram\nclass A[\"x\"",
    "classDiagram\n  \u{2028}class A",
    "classDiagram\n#35; A",
    "classDiagram-v2\nA --> B",
    "classDiagram\r\nclass A\r\nA : +x\r\n",
];

#[test]
fn cases_match_pele() {
    run("case", &plain(CASES.iter().map(|&case| case.to_owned()).collect())).finish("hand-written class diagrams");
}

#[test]
fn host_config_matches_pele() {
    let mut cases = Vec::new();
    for config in [
        json!({"class": {"hierarchicalNamespaces": false}}),
        json!({"class": {"hierarchicalNamespaces": "false"}}),
        json!({"class": "flat"}),
        json!({"class": null}),
    ] {
        for source in [
            "classDiagram\nnamespace A.B {\nclass X\n}\nnamespace A {\nclass Y\n}",
            "---\nconfig:\n  class:\n    hierarchicalNamespaces: true\n---\nclassDiagram\nnamespace A.B.C {\nclass X\n}",
        ] {
            cases.push((source.to_owned(), json!({ "config": config })));
        }
    }
    run("config case", &cases).finish("class diagrams with host config");
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

/// The pieces Pele's member test builds random members from, which reach every branch of the
/// method split.
const MEMBER_PIECES: &[&str] = &[
    "(", ")", "()", "+", "-", "#", "~", "$", "*", " ", "  ", ":", ",", "a", "foo", "int", "List", "\r", "\u{2028}",
    "\t", "é",
];

fn random_member(rng: &mut Rng) -> String {
    (0..1 + rng.below(9)).map(|_| rng.pick(MEMBER_PIECES)).collect()
}

/// Every random member as an attribute line of a body and as a `Class : member` statement.
#[test]
fn random_members_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut cases = Vec::new();
    for _ in 0..3000 {
        let member = random_member(&mut rng);
        cases.push((format!("classDiagram\nclass A {{\n{member}\n}}"), json!({})));
        cases.push((format!("classDiagram\nA : {member}"), json!({})));
    }
    let tally = run("random member", &cases).finish("random members");
    assert!(tally.identical > 3000, "most random members parse");
}

const NAMES: &[&str] = &["A", "B", "C", "Animal", "N.A", "`a b`", "é", "x-y", "C1", "note0", "interface0", "1"];
const ENDS: &[&str] = &["", "<|", "|>", "<", ">", "*", "o", "()"];
const LINES: &[&str] = &["--", ".."];
const MEMBERS: &[&str] =
    &["+name", "-age : int", "+run(a, b) bool", "#List~int~ items$", "~f()*", "<<interface>>", "x"];
const STYLES: &[&str] = &["fill:#f9f", "stroke:#333,stroke-width:4px", "color:red", "fill:#bbf,color:#fff"];

fn statement(rng: &mut Rng, depth: usize) -> String {
    let name = |rng: &mut Rng| {
        let mut name = rng.pick(NAMES).to_owned();
        if rng.chance(6) {
            name += rng.pick(&["~T~", "~List~int~~"]);
        }
        name
    };
    match rng.below(12) {
        0..=3 => {
            let card = |rng: &mut Rng| {
                if rng.chance(3) { format!("\"{}\" ", rng.pick(&["1", "0..*", "many"])) } else { String::new() }
            };
            let (a, b) = (name(rng), name(rng));
            let (left, right) = (card(rng), card(rng));
            let line = format!("{}{}{}", rng.pick(ENDS), rng.pick(LINES), rng.pick(ENDS));
            let label =
                if rng.chance(2) { format!(" : {}", rng.pick(&["uses", "has many", "x"])) } else { String::new() };
            format!("{a} {left}{line} {}{b}{label}", right.trim_start())
        }
        4 => {
            let mut out = format!("class {}", name(rng));
            if rng.chance(4) {
                out += "[\"A label\"]";
            }
            match rng.below(4) {
                0 => out += ":::k",
                1 => out += " <<service>>",
                _ => {}
            }
            if rng.chance(2) {
                out += " {\n";
                for _ in 0..rng.below(4) {
                    out += rng.pick(MEMBERS);
                    out += "\n";
                }
                out += "}";
            }
            out
        }
        5 => format!("{} : {}", name(rng), rng.pick(MEMBERS)),
        6 if depth < 3 => {
            let label = if rng.chance(3) { "[\"Group\"]" } else { "" };
            let mut out = format!("namespace {}{label} {{\n", rng.pick(&["N", "N.M", "Outer.Inner", "`ns`"]));
            for _ in 0..1 + rng.below(3) {
                if rng.chance(4) {
                    out += &statement(rng, depth + 1);
                } else if rng.chance(3) {
                    out += &format!("note \"{}\"", rng.pick(&["n", "a note"]));
                } else {
                    out += &format!("class {}", name(rng));
                }
                out += "\n";
            }
            out + "}"
        }
        7 => match rng.below(3) {
            0 => format!("note \"{}\"", rng.pick(&["free", "two\\nlines"])),
            _ => format!("note for {} \"{}\"", name(rng), rng.pick(&["n", "for it"])),
        },
        8 => match rng.below(4) {
            0 => format!("classDef {} {}", rng.pick(&["k", "k,m", "default"]), rng.pick(STYLES)),
            1 => format!("style {} {}", rng.pick(&["A", "B", "Missing"]), rng.pick(STYLES)),
            2 => format!("cssClass \"{}\" {}", rng.pick(&["A", "A,B", "1"]), rng.pick(&["k", "m"])),
            _ => format!("<<{}>> {}", rng.pick(&["interface", "enum"]), name(rng)),
        },
        9 => match rng.below(4) {
            0 => format!("link {} \"{}\"", name(rng), rng.pick(&["https://example.com", "javascript:x", "/a"])),
            1 => format!(
                "click {} href \"https://example.com\" \"tip\" {}",
                name(rng),
                rng.pick(&["_blank", "_self", ""])
            ),
            2 => format!("click {} call f({})", name(rng), rng.pick(&["", "1, 2"])),
            _ => format!("callback {} \"cb\" \"tip\"", name(rng)),
        },
        10 => rng
            .pick(&["direction LR", "direction BT", "accTitle: t", "accDescr: d", "accDescr {\n a\n b\n}"])
            .to_owned(),
        _ => rng.pick(&["%% comment", "", "A", "class"]).to_owned(),
    }
}

#[test]
fn random_class_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut cases = Vec::new();
    for _ in 0..3000 {
        let mut source = String::new();
        if rng.chance(6) {
            source += &format!(
                "---\ntitle: {}\nconfig:\n  class:\n    hierarchicalNamespaces: {}\n---\n",
                rng.pick(&["T", "A #35; b"]),
                rng.pick(&["true", "false"])
            );
        }
        source += rng.pick(&["classDiagram\n", "classDiagram\n", "classDiagram-v2\n"]);
        for _ in 0..1 + rng.below(10) {
            source += rng.pick(&["", "  ", "\t"]);
            source += &statement(&mut rng, 0);
            source += rng.pick(&["\n", "\n", "\n\n"]);
        }
        cases.push((source, json!({})));
    }
    let tally = run("random", &cases).finish("random class diagrams");
    assert!(tally.identical > 1000, "many random class diagrams parse");
    assert!(tally.rejected > 100, "some random class diagrams are rejected");
}
