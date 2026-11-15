//! Differential test for the swimlane front end, which is the flowchart's: for every source,
//! this port's model and flow graph must come out as Pele 1.13.0's do, byte for byte in the JSON
//! of the oracle's `flow_front` op, and a source Pele rejects must be rejected here with Pele's
//! code, type, message and position. Pele's flowchart parser names the flowchart as the type of
//! its errors, and so must this port's.
//!
//! `tests/fixtures/swimlane/harvest.json` holds the swimlane sources of Pele's own test suite,
//! the flowchart harvest with swimlane headers, and seeded mutations, written by
//! `scripts/harvest-swimlane.mjs`.

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/swimlane/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("flow_front\t{source}\t{options}")
}

fn parse_options(options: &Value) -> ParseOptions {
    let mut parse = ParseOptions::default();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxEdges" => parse.limits.max_edges = usize::try_from(value.as_u64().expect("a count")).expect("a count"),
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
        match std::panic::catch_unwind(|| pele_core::swimlane_front_json(source, parse_options(options))) {
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested swimlane sources");
    assert!(tally.identical > 100, "the harvest holds swimlanes Pele parses");
    assert!(tally.rejected > 10, "the harvest holds swimlanes Pele rejects");
}

#[test]
fn converted_flowcharts_match_pele() {
    let tally = run("converted", &plain(harvest("converted"))).finish("flowcharts with swimlane headers");
    assert!(tally.identical > 1000, "most converted flowcharts parse");
    assert!(tally.rejected > 100, "the converted flowcharts reach the parser's errors");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated swimlane sources");
}

#[test]
fn limits_and_host_config_match_pele() {
    let mut cases = Vec::new();
    for max in [0, 1, 3] {
        cases.push(("swimlane-beta LR\nsubgraph A\na --> b --> c\nend".to_owned(), json!({ "maxEdges": max })));
    }
    for config in
        [json!({"swimlane": {"curve": "linear"}}), json!({"flowchart": {"inheritDir": true, "curve": "step"}})]
    {
        let source = "swimlane-beta TB\nsubgraph A\ndirection LR\na --> b\nend\nlinkStyle 5 stroke:red";
        cases.push((source.to_owned(), json!({ "config": config })));
    }
    cases.push(("swimlane-beta\nA@{ shape: Nope }".to_owned(), json!({})));
    cases.push(("swimlane-beta\nA@{ bad: [ }".to_owned(), json!({})));
    cases.push(("swimlane-beta\nsubgraph A\na".to_owned(), json!({})));
    cases.push(("swimlane-beta TD\na --> b".to_owned(), json!({})));
    let tally = run("option case", &cases).finish("swimlanes with parse options");
    assert!(tally.rejected >= 5, "the cases reach the flowchart parser's errors");
}

/// Statements Pele rejects.
const MISTAKES: &[&str] =
    &["A@{ shape: Rect }\n", "subgraph S\nA\n", "A -- mixed ==> B\n", "linkStyle 99 stroke:red\n", "end\n"];

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

const LABELS: &[&str] = &[
    "Start",
    "A longer label with several words in it",
    "\"`**Bold** and _italic_ words`\"",
    "one<br>two",
    "#35; and #amp;",
    "x",
    "Supercalifragilisticexpialidocious",
];
const BRACKETS: &[(&str, &str)] =
    &[("[", "]"), ("(", ")"), ("([", "])"), ("{", "}"), ("((", "))"), ("[[", "]]"), ("[(", ")]")];
const LINKS: &[&str] = &[
    "-->",
    "-->",
    "-->",
    "---",
    "-.->",
    "==>",
    "--x",
    "--o",
    "<-->",
    "---->",
    "-- yes -->",
    "-->|a label|",
    "-->|a much longer label that wraps onto more than one line of text|",
];
const DECLARATIONS: &[&str] = &[
    "fill:#f9f",
    "fill:#036",
    "fill:#036,color:#fff",
    "fill:#000,fill-opacity:0.3",
    "stroke:#333,stroke-width:4px",
    "color:red",
    "font-weight:bold",
    "fill:none",
];
const CURVES: &[&str] = &["basis", "linear", "step", "stepBefore", "stepAfter"];

fn node(rng: &mut Rng, most: usize) -> String {
    let id = format!("N{}", rng.below(most));
    if rng.chance(3) {
        let (open, close) = BRACKETS[rng.below(BRACKETS.len())];
        format!("{id}{open}{}{close}", rng.pick(LABELS))
    } else {
        id
    }
}

/// A swimlane, as `swimlane_parity.rs` makes them, with a statement Pele rejects in some.
fn random_swimlane(rng: &mut Rng) -> String {
    let mut source = String::new();
    let mut config = String::new();
    if rng.chance(4) {
        config += &format!("    curve: {}\n", rng.pick(CURVES));
    }
    if rng.chance(6) {
        config += &format!("    nodeSpacing: {}\n    rankSpacing: {}\n", 5 + rng.below(90), 5 + rng.below(90));
    }
    if rng.chance(6) {
        config += &format!("    wrappingWidth: {}\n", 40 + rng.below(300));
    }
    let title = rng.chance(6);
    if !config.is_empty() || title {
        source += "---\n";
        if title {
            source += &format!(
                "title: {}\n",
                rng.pick(&["A title", "A much longer title than the lanes are wide, most likely"])
            );
        }
        if !config.is_empty() {
            source += &format!("config:\n  {}:\n{config}", rng.pick(&["swimlane", "flowchart"]));
        }
        source += "---\n";
    }
    source += &format!("swimlane-beta {}\n", rng.pick(&["LR", "LR", "TB", "TD", "RL", "BT", ""]));
    let lanes = 1 + rng.below(5);
    let many = rng.chance(5);
    let most = 3 + rng.below(if many { 30 } else { 10 });
    let mut groups = Vec::new();
    for lane in 0..lanes {
        let title = match rng.below(3) {
            0 => String::new(),
            1 => format!(" [{}]", rng.pick(LABELS)),
            _ => " [\"A lane title that is rather long\"]".to_owned(),
        };
        source += &format!("subgraph Lane{lane}{title}\n");
        if rng.chance(4) {
            let name = format!("G{lane}");
            source += &format!("subgraph {name} [\"A group title that is rather long, to be crossed\"]\n");
            for _ in 0..1 + rng.below(3) {
                source += &format!("{}\n", node(rng, most));
            }
            source += "end\n";
            groups.push(name);
        }
        for _ in 0..1 + rng.below(4) {
            source += &format!("{}\n", node(rng, most));
        }
        source += "end\n";
    }
    let mut edges = 0;
    for _ in 0..rng.below(2 * most) {
        let from = if rng.chance(15) { "Lane0".to_owned() } else { node(rng, most + 2) };
        let to = if rng.chance(15) {
            groups.first().cloned().unwrap_or_else(|| "Lane0".to_owned())
        } else {
            node(rng, most + 2)
        };
        source += &format!("{from} {} {to}\n", rng.pick(LINKS));
        edges += 1;
    }
    if rng.chance(5) {
        source += "Self --> Self\nSelf -->|again| Self\n";
        edges += 2;
    }
    for _ in 0..rng.below(3) {
        source += &match rng.below(5) {
            0 => format!("style Lane{} {}\n", rng.below(lanes), rng.pick(DECLARATIONS)),
            4 if !groups.is_empty() => {
                format!("style {} {}\n", groups[rng.below(groups.len())], rng.pick(DECLARATIONS))
            }
            1 => format!("style N{} {}\n", rng.below(most), rng.pick(DECLARATIONS)),
            2 => format!("classDef c1 {}\nclass Lane{} c1\n", rng.pick(DECLARATIONS), rng.below(lanes)),
            _ if edges > 0 => format!("linkStyle {} {}\n", rng.below(edges), rng.pick(DECLARATIONS)),
            _ => "%% nothing\n".to_owned(),
        };
    }
    if rng.chance(8) {
        source += "accTitle: The title\naccDescr: A description\n";
    }
    if rng.chance(5) {
        source += rng.pick(MISTAKES);
    }
    source
}

#[test]
fn random_swimlanes_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..3000).map(|_| (random_swimlane(&mut rng), json!({}))).collect();
    let tally = run("random", &cases).finish("random swimlanes");
    assert!(tally.identical > 2000, "most random swimlanes parse");
    assert!(tally.rejected > 300, "some random swimlanes are rejected");
}
