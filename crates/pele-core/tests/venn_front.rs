//! Differential test for the venn front end: for every source, this port's venn model must come
//! out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `venn_front` op, and a
//! source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/venn/harvest.json` holds the venn sources of Pele's own test suite, the
//! corpus of its venn parser test, and the inputs of its mutated and generated runs, written by
//! `scripts/harvest-venn.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/venn/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("venn_front\t{source}")
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
                if theirs.starts_with("{\"type\":\"venn\",") {
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
        match std::panic::catch_unwind(|| pele_core::venn_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested venn sources");
    assert!(tally.identical > 500, "the harvest holds venn diagrams Pele parses");
    assert!(tally.lexical + tally.parse > 100, "the harvest holds venn diagrams Pele cannot read");
    assert!(tally.semantic > 10, "the harvest holds venn diagrams Pele's db rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated venn sources");
    assert!(
        tally.lexical > 10 && tally.parse > 10 && tally.semantic > 10,
        "the mutations reach every kind of rejection"
    );
}

/// Hand-written sources for the paths the harvest reaches least: the end of input in each lexer
/// state, a title in each place, indented text before and after a set, numbers as `parseFloat`
/// reads them, colours that do not close, and quotes the db takes off.
const CASES: &[&str] = &[
    "venn-beta",
    "venn-beta\n",
    "  venn-beta\n\n\nset A",
    "venn-beta set A",
    "venn-betaset A",
    "venn-beta\nset",
    "venn-beta\nset A[",
    "venn-beta\nset A[\"",
    "venn-beta\nset A[\"x\"",
    "venn-beta\nset A[\"x\" ]",
    "venn-beta\nset A[x\"y]",
    "venn-beta\nset A[]",
    "venn-beta\nset A[  ]",
    "venn-beta\nset A[\"  \"]",
    "venn-beta\nset A[\" \\\"q\\\" \"]",
    "venn-beta\nset A:",
    "venn-beta\nset A:+",
    "venn-beta\nset A:-.",
    "venn-beta\nset A:1.",
    "venn-beta\nset A:1.5.5",
    "venn-beta\nset A:-0",
    "venn-beta\nset A:0000.000100",
    "venn-beta\nset A:1e5",
    "venn-beta\nset A:99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999",
    "venn-beta\nset A:.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
    "venn-beta\nset \"\"",
    "venn-beta\nset \"\"\"",
    "venn-beta\nset \"a\nb\"",
    "venn-beta\nset \" spaced \"",
    "venn-beta\nset A\nset B\nunion A",
    "venn-beta\nset A\nunion A,B,C",
    "venn-beta\nset A\nunion A,A",
    "venn-beta\nset A\nunion A,A,A[\"x\"]:3",
    "venn-beta\nset A\nunion B",
    "venn-beta\nunion A,B:5",
    "venn-beta\nset A\nunion A,\"A\"",
    "venn-beta\nset A\nunion A,,A",
    "venn-beta\nset A\nunion A,",
    "venn-beta\ntitle",
    "venn-beta\ntitle ",
    "venn-beta\ntitle \t ",
    "venn-beta\ntitle\nset A\n  text a",
    "venn-beta\nset A\ntitle\n  text a",
    "venn-beta\nset A\n  title\n  text a",
    "venn-beta\nset A\n\ttitle  \n\n\n  text a",
    "venn-beta\ntitle a",
    "venn-beta\ntitle\ta",
    "venn-beta\nTITLE a # b",
    "venn-beta\ntitle a;b\nset A",
    "venn-beta\ntitle #\nset A",
    "venn-beta\ntitle  \"quoted\"  ",
    "venn-beta\ntitlex\nset A",
    "venn-beta\ntitle-x\nset A",
    "venn-beta title x",
    "title x\nvenn-beta",
    "venn-beta\nset A\n  text",
    "venn-beta\nset A\n  text ",
    "venn-beta\nset A\n  text\n",
    "venn-beta\nset A\n  texts",
    "venn-beta\nset A\n  text-x",
    "venn-beta\nset A\n  text a[b]",
    "venn-beta\nset A\n  text 5[b]",
    "venn-beta\nset A\n  text \"q\"[\"l\"]",
    "venn-beta\nset A\n  text a\n  text b\nset B\n    text c\nunion A,B\n\ttext d",
    "venn-beta\n  text a",
    "venn-beta\nset A\n\n  text a",
    "venn-beta\nset A\n%% comment\n  text a",
    "venn-beta\nset A %% trailing\n  text a",
    "venn-beta\nset A\n  %% indented comment\n  text a",
    "venn-beta\nset A\n  %%{ not a comment\n  text a",
    "venn-beta\nset A\nx%%c\n  text a",
    "venn-beta\nset A\r\n  text a\r\n",
    "venn-beta\nset A\r  text a",
    "venn-beta\nset A\ntext A",
    "venn-beta\nset A\ntext A,B",
    "venn-beta\nset A\ntext A 5 [x]",
    "venn-beta\nset A\ntext A t[x] extra",
    "venn-beta\nset A\nstyle A",
    "venn-beta\nset A\nstyle A fill",
    "venn-beta\nset A\nstyle A fill:",
    "venn-beta\nset A\nstyle A fill:,",
    "venn-beta\nset A\nstyle A fill:red,",
    "venn-beta\nset A\nstyle A fill:red color:blue",
    "venn-beta\nset A\nstyle A fill:\"red\" \"blue\"",
    "venn-beta\nset A\nstyle A fill:red blue 5 #fff rgb(1,2,3) rgba(1,2,3,.5)",
    "venn-beta\nset A\nstyle A fill:rgb(1,2",
    "venn-beta\nset A\nstyle A fill:rgb (1,2,3)",
    "venn-beta\nset A\nstyle A fill:rgba(1,2,3)",
    "venn-beta\nset A\nstyle A fill:rgb(1,2,3,4)",
    "venn-beta\nset A\nstyle A fill:RgBa( 1 , 2 , 3 , 4 )",
    "venn-beta\nset A\nstyle A fill:rgbx",
    "venn-beta\nset A\nstyle A fill:#12",
    "venn-beta\nset A\nstyle A fill:#123",
    "venn-beta\nset A\nstyle A fill:#123456789",
    "venn-beta\nset A\nstyle A fill:#",
    "venn-beta\nset A\nstyle A 5:red",
    "venn-beta\nset A\nstyle A fill:red, fill:blue",
    "venn-beta\nset A\nstyle A fill:\" red \"",
    "venn-beta\nset A\nstyle \"A\",B fill:red",
    "venn-beta\nset A\nstyle A fill:red\nstyle A fill:blue, color:red",
    "venn-beta\nset A\nset set-a\nset _x\nset a-\nset -a",
    "venn-beta\nset A\nset 5",
    "venn-beta\nset A\n;",
    "venn-beta\nset A\n#",
    "venn-beta\nset A\n\u{e9}",
    "venn-beta\nSET A\nUnIoN A,A\n  TeXt x\nSTYLE A FILL:Red",
    "venn-beta\nvenn-beta",
    "venn-beta\nset A venn-beta",
    "venn-beta\nset A set B union A,B",
    "venn-beta\nset A\nset B\nunion B,A[\"ba\"]\nunion A,B[\"ab\"]:1",
    "---\ntitle: Front\n---\nvenn-beta\nset A",
    "---\ntitle: Front\n---\nvenn-beta\ntitle Own\nset A",
    "---\ntitle: \"\"\n---\nvenn-beta\nset A",
];

#[test]
fn hand_written_sources_match_pele() {
    let sources: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    run("case", &sources).finish("hand-written venn sources");
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

/// A random venn diagram from whole statements and the pieces between them, over a few ids,
/// so that unions find their sets, texts find their regions and styles find their targets. A
/// noisy one also takes pieces the grammar or the db rejects.
fn random_diagram(rng: &mut Rng) -> String {
    const IDS: &[&str] = &["A", "B", "C", "\"D e\"", "x-1", "_y", "\"A\"", "Ab", "\"\"", "\" s \""];
    const LABELS: &[&str] = &["", "", "", "[\"L\"]", "[L m]", "[\"\"]", "[ x ]", "[\"#35; &amp;\"]", "[\"a,b:c\"]"];
    const SIZES: &[&str] = &["", "", "", ":5", ": 2.5", ":-1", ":.5", ":0", ":+3", ":12.25", ":-0", ":1000.5"];
    const INDENTS: &[&str] = &["", "", "", "  ", "    ", "\t", " ", " \t "];
    const TITLES: &[&str] = &["T", "A b c", "\"quoted\"", "  padded  ", "\u{e9}t\u{e9}"];
    const NOISE: &[&str] = &[
        "set",
        "union",
        "style A",
        "[x]",
        ":5",
        "\"open",
        "#",
        ",",
        "rgb(1,2",
        ":x",
        "[\"a\"b\"]",
        "title x # y",
        "text set",
        "x%% eaten",
        ":1.",
        "Text 9",
        "text t1",
    ];
    let noisy = rng.below(4) == 0;
    let ends: &[&str] =
        if noisy { &["\n", "\n", " ", "\r\n", "\n\n", "\r", ";"] } else { &["\n", "\n", "\r\n", "\n\n", "\n  \n"] };
    let mut src = rng.pick(&["venn-beta", "  venn-beta", "\nvenn-beta", "venn-beta "]).to_owned();
    src.push_str(rng.pick(&["\n", "\n", "\r\n", "\n\n"]));
    let mut declared: Vec<&str> = Vec::new();
    for _ in 0..rng.below(6) {
        let id = rng.pick(IDS);
        declared.push(id);
        src.push_str(&format!("{}set {id}{}{}\n", rng.pick(INDENTS), rng.pick(LABELS), rng.pick(SIZES)));
    }
    let ids = |rng: &mut Rng, declared: &[&str], count: usize| {
        let from = if declared.is_empty() || (noisy && rng.below(3) == 0) { IDS } else { declared };
        (0..count).map(|_| rng.pick(from)).collect::<Vec<_>>().join(",")
    };
    for _ in 0..1 + rng.below(10) {
        src.push_str(rng.pick(INDENTS));
        let statement = match rng.below(if noisy { 13 } else { 12 }) {
            0 | 1 => {
                let id = rng.pick(IDS);
                declared.push(id);
                format!("set {id}{}{}", rng.pick(LABELS), rng.pick(SIZES))
            }
            2..=4 => {
                let count = 2 + rng.below(3);
                format!("union {}{}{}", ids(rng, &declared, count), rng.pick(LABELS), rng.pick(SIZES))
            }
            5 => {
                let id = rng.pick(IDS);
                declared.push(id);
                let text = rng.pick(&["t1", "\"q r\"", "7", "t2", "\"q\"", "A"]);
                let indent = rng.pick(&["  ", "\t", " ", "    "]);
                format!("set {id}\n{indent}text {text}{}\n{indent}Text t9", rng.pick(LABELS))
            }
            6 => {
                let count = 1 + rng.below(2);
                format!(
                    "text {} {}{}",
                    ids(rng, &declared, count),
                    rng.pick(&["t1", "\"q r\"", "7", "t3"]),
                    rng.pick(LABELS)
                )
            }
            7 | 8 => {
                let count = 1 + rng.below(2);
                let value = rng.pick(&["red", "#ff6b6b", "rgb(1, 2, 3)", "\"x y\"", "a b 5", "rgba(1,2,3,.5)", "#fff"]);
                let more =
                    rng.pick(&["", ", color:#333", ", stroke-width:2px", ", fill-opacity:.4", ", font-weight:bold"]);
                format!("style {} fill:{value}{more}", ids(rng, &declared, count))
            }
            9 => format!("title {}", rng.pick(TITLES)),
            10 => rng.pick(&["", "   ", "%% comment", "%%{ directive }%%", "title"]).to_owned(),
            11 => {
                let count = 2 + rng.below(2);
                format!("union {}\n  text {}", ids(rng, &declared, count), rng.pick(&["u1[\"U\"]", "\"u q\"", "8"]))
            }
            _ => rng.pick(NOISE).to_owned(),
        };
        src.push_str(&statement);
        src.push_str(rng.pick(ends));
    }
    src
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x5eed_0f7e_119a_7d00);
    let sources: Vec<String> = (0..4000).map(|_| random_diagram(&mut rng)).collect();
    let tally = run("random", &sources).finish("random venn diagrams");
    assert!(tally.identical > 1000, "most random diagrams parse");
    assert!(tally.semantic > 50, "random diagrams reach the db's errors");
}
