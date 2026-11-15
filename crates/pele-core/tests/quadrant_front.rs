//! Differential test for the quadrant chart front end: for every source, this port's model must
//! come out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `quadrant_front` op,
//! and a source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/quadrant/harvest.json` holds the quadrant sources of Pele's own test suite,
//! the corpus of its quadrant parser test, and the inputs of its mutated runs, written by
//! `scripts/harvest-quadrant.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/quadrant/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("quadrant_front\t{source}")
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
    lexical: usize,
    parse: usize,
    semantic: usize,
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
                if theirs.starts_with("{\"type\":\"quadrantChart\",") {
                    self.identical += 1;
                } else {
                    self.other_types += 1;
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
                    match diagnostic.code {
                        ErrorCode::Semantic => self.semantic += 1,
                        _ if diagnostic.message.starts_with("Lexical error") => self.lexical += 1,
                        _ => self.parse += 1,
                    }
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
        let rejected = self.lexical + self.parse + self.semantic;
        let total = self.identical + rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {rejected} rejected with Pele's error, {} of another type, {} different",
            self.identical,
            self.other_types,
            self.failures.len()
        );
        println!(
            "    rejected by kind: syntax, lexer {}; syntax, parser {}; semantic, model {}",
            self.lexical, self.parse, self.semantic
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
        match std::panic::catch_unwind(|| pele_core::quadrant_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested quadrant sources");
    assert!(tally.identical > 500, "the harvest holds quadrant charts Pele parses");
    assert!(tally.lexical + tally.parse > 100, "the harvest holds quadrant charts Pele cannot read");
    assert!(tally.semantic > 10, "the harvest holds quadrant charts Pele's db rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated quadrant sources");
    assert!(
        tally.lexical > 10 && tally.parse > 10 && tally.semantic > 10,
        "the mutations reach every kind of rejection"
    );
}

/// Hand-written sources for the paths the harvest reaches least: the end of input in each lexer
/// state, keywords run into what follows them, coordinates as `Number` reads them, each style
/// and each way a style is wrong, and the accessible title and description.
const CASES: &[&str] = &[
    "quadrantChart",
    "quadrantChart\n",
    "  \n ;quadrantChart",
    "QuadrantChart\nA: [0.1, 0.2]",
    "quadrantChartx\nA: [0.1, 0.2]",
    "quadrantChart A: [0.1, 0.2]",
    "quadrantChart\ntitle",
    "quadrantChart\ntitle ",
    "quadrantChart\ntitle  spaced  title  ",
    "quadrantChart\ntitlex",
    "quadrantChart\nTITLE shout",
    "quadrantChart\naccTitle",
    "quadrantChart\naccTitle:",
    "quadrantChart\naccTitle   :   spaced   ",
    "quadrantChart\naccDescr: one",
    "quadrantChart\naccDescr {",
    "quadrantChart\naccDescr {\n  many\n    lines\n}",
    "quadrantChart\naccDescr{x}\nA: [0.5, 0.5]",
    "quadrantChart\naccdescr: lower",
    "quadrantChart\nx-axis",
    "quadrantChart\nx-axis Low",
    "quadrantChart\nx-axis Low -->",
    "quadrantChart\nx-axis Low --> ",
    "quadrantChart\nx-axis Low ---> High",
    "quadrantChart\nx-axis Low -> High",
    "quadrantChart\nx-axis Low-->High",
    "quadrantChart\nx-axis \"Low\" --> \"High\"",
    "quadrantChart\nx-axis \"`**Low**`\" --> \"`_High_`\"",
    "quadrantChart\nx-axis \"`Low\" --> High",
    "quadrantChart\nx-axis \"`Low`",
    "quadrantChart\nx-axis \"Low",
    "quadrantChart\ny-axis a-b --> c d\ny-axis \"only\"",
    "quadrantChart\nX-AXIS shout\nY-Axis mixed",
    "quadrantChart\nx-axisLow --> High",
    "quadrantChart\nquadrant-1",
    "quadrantChart\nquadrant-1 one\nquadrant-2 two\nquadrant-3 three\nquadrant-4 four",
    "quadrantChart\nquadrant-5 five",
    "quadrantChart\nquadrant-1one",
    "quadrantChart\nquadrant-1 a_b.c&d+e=f*g#h!i$j%k'l`m?n\\o/p",
    "quadrantChart\nquadrant-1 \u{e9}t\u{e9} \u{9700}",
    "quadrantChart\nquadrant-1 a;quadrant-2 b;A: [0.5, 0.5];",
    "quadrantChart\nA",
    "quadrantChart\nA:",
    "quadrantChart\nA: [",
    "quadrantChart\nA: [0",
    "quadrantChart\nA: [0.5",
    "quadrantChart\nA: [0.5,",
    "quadrantChart\nA: [0.5, 0.5",
    "quadrantChart\nA: [0.5, 0.5]",
    "quadrantChart\nA: []",
    "quadrantChart\nA: [ ]",
    "quadrantChart\nA: [0.5, 0.5, 0.5]",
    "quadrantChart\nA: [0.5, 0.5][0.1, 0.1]",
    "quadrantChart\nA: [0x5, 0e9]\nB: [0,5, 0 5]\nC: [055, 0b1]\nD: [0o7, 0b2]\nE: [0X1, 0B1]\nF: [0.0, 1]",
    "quadrantChart\nA: [1.5, 0.5]",
    "quadrantChart\nA: [2, 0.5]",
    "quadrantChart\nA:[0.5,0.5]",
    "quadrantChart\nA \t :\n [\n0.5\n,\n0.5\n]",
    "quadrantChart\nA: [0.5, 0.5]   ",
    "quadrantChart\nA:::c: [0.5, 0.5]",
    "quadrantChart\nA:::: [0.5, 0.5]",
    "quadrantChart\nA::: c: [0.5, 0.5]",
    "quadrantChart\nA:::c_1-x: [0.5, 0.5]",
    "quadrantChart\nA: [0.5, 0.5] radius: 10",
    "quadrantChart\nA: [0.5, 0.5] radius:10,color:#ff0000",
    "quadrantChart\nA: [0.5, 0.5] radius : 10 , color : #abc",
    "quadrantChart\nA: [0.5, 0.5] radius: big",
    "quadrantChart\nA: [0.5, 0.5] radius: -1",
    "quadrantChart\nA: [0.5, 0.5] radius",
    "quadrantChart\nA: [0.5, 0.5] radius: 1: 2",
    "quadrantChart\nA: [0.5, 0.5] color: red",
    "quadrantChart\nA: [0.5, 0.5] color: abc",
    "quadrantChart\nA: [0.5, 0.5] color: #abcd",
    "quadrantChart\nA: [0.5, 0.5] stroke-color: #f0f",
    "quadrantChart\nA: [0.5, 0.5] stroke-width: 5px",
    "quadrantChart\nA: [0.5, 0.5] stroke-width: 5",
    "quadrantChart\nA: [0.5, 0.5] stroke-width: 5 px",
    "quadrantChart\nA: [0.5, 0.5] fill: #fff",
    "quadrantChart\nA: [0.5, 0.5] radius: 5,",
    "quadrantChart\nA: [0.5, 0.5] radius: 5 radius: 6",
    "quadrantChart\nA: [0.5, 0.5] radius: 99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999",
    "quadrantChart\nclassDef",
    "quadrantChart\nclassDef c",
    "quadrantChart\nclassDef c ",
    "quadrantChart\nclassDef c radius: 5",
    "quadrantChart\nclassDef c-1,2:3&4#5*6\u{e9} radius: 5",
    "quadrantChart\nclassDef  c radius: 5",
    "quadrantChart\nclassDefs c radius: 5",
    "quadrantChart\nclassDef c radius: 5\nclassDef c color: #fff",
    "quadrantChart\n\"quoted point\": [0.1, 0.1]",
    "quadrantChart\n\"\": [0.1, 0.1]",
    "quadrantChart\n\" \": [0.1, 0.1]",
    "quadrantChart\n\"`**md** point`\": [0.1, 0.1]",
    "quadrantChart\n%% comment\nA: [0.3, 0.6] %% trailing",
    "quadrantChart\na%%b\nA: [0.3, 0.6]",
    "quadrantChart\r\nA: [0.3, 0.6]\r\n",
    "quadrantChart\n\u{b}\u{c}A: [0.3, 0.6]",
    "quadrantChart\n(A): [0.3, 0.6]",
    "quadrantChart\nA @ B: [0.3, 0.6]",
    "---\ntitle: Front\n---\nquadrantChart\nA: [0.3, 0.6]",
    "---\ntitle: Front\n---\nquadrantChart\ntitle Own",
    "---\ntitle: \"\"\n---\nquadrantChart",
];

#[test]
fn hand_written_sources_match_pele() {
    let sources: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    run("case", &sources).finish("hand-written quadrant sources");
}

/// A small seeded generator, so a failure names an input that can be made again.
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

    fn pick<'a>(&mut self, list: &[&'a str]) -> &'a str {
        list[self.below(list.len())]
    }
}

/// A random quadrant chart from whole statements, and pieces the grammar or the db rejects in a
/// noisy one.
fn random_chart(rng: &mut Rng) -> String {
    const TEXTS: &[&str] = &[
        "Low",
        "High reach",
        "\"quoted text\"",
        "\"`**bold** md`\"",
        "a-b c",
        "x.y_z",
        "\u{e9}t\u{e9}",
        "7 days",
        "A&B",
        "50%",
    ];
    const COORDS: &[&str] = &["0", "1", "0.5", "0.25", "0.999", "0.0001", "0x5", "0e3", "05", "0.1234567"];
    const STYLES: &[&str] =
        &["radius: 10", "color: #ff0000", "color: abc", "stroke-color: #f0f", "stroke-width: 5px", "radius:3"];
    const NOISE: &[&str] = &[
        "radius: big",
        "color: red",
        "fill: #fff",
        "stroke-width: 5",
        "[0.5",
        "x-axis",
        ":::",
        "quadrant-5 x",
        "\"open",
        "\"`open",
        "accDescr {",
        "@",
        "A: [1.5, 0.5]",
    ];
    let noisy = rng.below(4) == 0;
    let mut src = rng.pick(&["quadrantChart", "  quadrantChart", "\nquadrantChart", "quadrantChart "]).to_owned();
    src.push_str(rng.pick(&["\n", "\n", "\r\n", ";"]));
    let classes = ["c1", "c2", "k"];
    for _ in 0..1 + rng.below(10) {
        src.push_str(rng.pick(&["", "", "  ", "\t"]));
        let statement = match rng.below(if noisy { 12 } else { 11 }) {
            0 => format!("title {}", rng.pick(TEXTS)),
            1 => format!("x-axis {}{}", rng.pick(TEXTS), rng.pick(&["", " --> ", " --> High", " ---> \"far\""])),
            2 => format!("y-axis {}{}", rng.pick(TEXTS), rng.pick(&["", " --> ", " --> Up"])),
            3 | 4 => format!("quadrant-{} {}", 1 + rng.below(4), rng.pick(TEXTS)),
            5..=7 => {
                let class = if rng.below(3) == 0 { format!(":::{}", classes[rng.below(3)]) } else { String::new() };
                let styles: Vec<&str> = (0..rng.below(3)).map(|_| rng.pick(STYLES)).collect();
                format!(
                    "{}{class}: [{}, {}]{}{}",
                    rng.pick(TEXTS),
                    rng.pick(COORDS),
                    rng.pick(COORDS),
                    if styles.is_empty() { "" } else { " " },
                    styles.join(", ")
                )
            }
            8 => format!("classDef {} {}", classes[rng.below(3)], rng.pick(STYLES)),
            9 => rng
                .pick(&["accTitle: a title", "accDescr: a description", "accDescr {\n  two\n  lines\n}", "%% note"])
                .to_owned(),
            10 => String::new(),
            _ => rng.pick(NOISE).to_owned(),
        };
        src.push_str(&statement);
        src.push_str(rng.pick(&["\n", "\n", "\r\n", ";", "\n\n"]));
    }
    src
}

#[test]
fn random_charts_match_pele() {
    let mut rng = Rng(0x0a0d_5eed_7a11_9a7d);
    let sources: Vec<String> = (0..4000).map(|_| random_chart(&mut rng)).collect();
    let tally = run("random", &sources).finish("random quadrant charts");
    assert!(tally.identical > 1500, "most random charts parse");
    assert!(tally.semantic > 100, "random charts reach the db's errors");
}
