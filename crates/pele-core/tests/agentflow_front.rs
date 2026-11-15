//! Differential test for the agentflow front end: for every source, this port's model, graph
//! and semantic model must come out as Pele 1.13.0's do, byte for byte in the JSON of the oracle's
//! `agentflow_front` op, and a source Pele rejects must be rejected here with Pele's message, code
//! and position.
//!
//! `tests/fixtures/agentflow/harvest.json` holds the agentflow sources of Pele's own test suite,
//! the sources of its reference corpus and the inputs of its seeded mutation runs, written by
//! `scripts/harvest-agentflow.mjs`.

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/agentflow/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("agentflow_front\t{source}\t{options}")
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
    /// How many accepted sources hold each thing the model can hold, by the name the JSON gives it.
    reached: std::collections::BTreeMap<&'static str, usize>,
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

/// What a model's JSON shows it holds, each with the text that marks it.
const MARKS: &[(&str, &str)] = &[
    ("a flow", "\"type\":\"flow\""),
    ("a collapsed flow", "\"shape\":\"collapsedGroup\""),
    ("a nested node", "\"parentId\":"),
    ("a connector", "\"isConnector\":true"),
    ("metadata", "\"metadata\":{"),
    ("an attachment mapping", "\"type\":\"attachment\""),
    ("a subgraph mapping", "\"type\":\"subgraph\""),
    ("a connector mapping", "\"type\":\"connector\""),
    ("an edge mapping", "\"type\":\"edge\""),
    ("a removed shape", "\"id\":\"SHAPE_REMOVED\""),
    ("an unsupported shape", "\"id\":\"SHAPE_UNSUPPORTED\""),
    ("a containment violation", "\"id\":\"CONTAINMENT_VIOLATION\""),
    ("a sequence edge", "\"edgeSemantic\":\"sequence\""),
    ("a reference edge", "\"edgeSemantic\":\"reference\""),
    ("a failure edge", "\"edgeSemantic\":\"failure\""),
    ("an edge id from the source", "\"isUserDefinedId\":true"),
    ("an edge curve", "\"curve\":"),
    ("an edge animation", "\"animate\":"),
    ("a class definition", "\"textStyles\":["),
    ("a link", "\"link\":\""),
    ("a tool", "\"vertexKind\":\"tool\""),
    ("a decision", "\"vertexKind\":\"decision\""),
    ("an input", "\"vertexKind\":\"input\""),
    ("a reference document", "\"vertexKind\":\"refdoc\""),
    ("an action", "\"vertexKind\":\"action\""),
    ("a flow direction", "\"dir\":\""),
    ("a title", "\"title\":\""),
    ("a number JSON cannot hold", "\"$number\":"),
];

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<String, RenderError>, theirs: &str) {
        let problem = match ours {
            Ok(ours) if ours == theirs => {
                if theirs.starts_with("{\"type\":") {
                    self.other_types += 1;
                } else {
                    self.identical += 1;
                    for &(what, mark) in MARKS {
                        *self.reached.entry(what).or_default() += usize::from(theirs.contains(mark));
                    }
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

    /// Fails unless the accepted sources held every thing in `MARKS`.
    fn reaches_everything(&self, what: &str) {
        let reached: Vec<String> =
            MARKS.iter().map(|(name, _)| format!("{name} {}", self.reached.get(name).copied().unwrap_or(0))).collect();
        println!("{what} hold: {}", reached.join(", "));
        let missing: Vec<&str> = MARKS
            .iter()
            .map(|&(name, _)| name)
            .filter(|name| self.reached.get(name).copied().unwrap_or(0) == 0)
            .collect();
        assert!(missing.is_empty(), "{what} never hold {missing:?}");
    }
}

fn run_with(name: &str, cases: &[(String, Value)]) -> Tally {
    let requests: Vec<String> = cases.iter().map(|(source, options)| request(source, options)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tally = Tally::default();
    for (index, ((source, options), theirs)) in cases.iter().zip(&answers).enumerate() {
        let ours = pele_core::agentflow_front_json(source, parse_options(options));
        tally.compare(&format!("{name} {index} with {options}"), source, ours, theirs);
    }
    tally
}

fn run(name: &str, sources: &[String]) -> Tally {
    let cases: Vec<(String, Value)> = sources.iter().map(|source| (source.clone(), json!({}))).collect();
    run_with(name, &cases)
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested agentflow sources");
    assert!(tally.identical > 1000, "the harvest holds agentflows Pele parses");
    assert!(tally.rejected > 100, "the harvest holds agentflows Pele rejects");
}

#[test]
fn reference_sources_match_pele() {
    let tally = run("reference", &harvest("reference")).finish("agentflow reference sources");
    assert!(tally.identical > 150, "the reference holds agentflows Pele parses");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated agentflow sources");
    assert!(tally.identical > 1000 && tally.rejected > 1000, "mutation leaves agentflows of both kinds");
}

/// Hand-written sources for the paths the harvest reaches least: what metadata may hold, which
/// thing a metadata block lands on, flows that name each other, collapsed flows, global blocks,
/// the positions of statements after front matter and comments, and error positions.
const CASES: &[&str] = &[
    "agentflow-beta TB\n  a@{ shape: tool, label: \"Run\" }\n  b@{ shape: decision }\n  c@{ shape: input }\n  d@{ shape: refdoc }\n  e@{ shape: action }\n  f@{ shape: task }\n  g@{ shape: round }\n",
    "agentflow-beta TB\n  a@{ shape: Tool }\n",
    "agentflow-beta TB\n  a@{ shape: lean_right }\n",
    "agentflow-beta TB\n  a@{ shape: nosuch }\n",
    "agentflow-beta TB\n  a@{ shape: 5 }\n",
    "agentflow-beta TB\n  a@{ shape: [x, 1] }\n",
    "agentflow-beta TB\n  a@{\n    shape:\n      k: .nan\n  }\n",
    "agentflow-beta TB\n  a@{ shape: true }\n",
    "agentflow-beta TB\n  a@{ shape: \"\" }\n  b@{ shape: constructor }\n",
    "agentflow-beta TB\n  c@{ shape: collapsedGroup }\n",
    "agentflow-beta TB\n  a@{ shape: stadium }\n  b@{ shape: cyl }\n  c@{ shape: hex }\n  d@{ shape: subproc }\n  e@{ shape: lined-document }\n",
    "agentflow-beta TB\n  a(round) --> b((circle)) --> c([stadium]) --> d[(db)] --> e>odd] --> f{{hex}} --> g(((dbl)))\n  h[/in/] --> i[\\out\\] --> j[/trap\\] --> k[\\inv/] --> l[[tool]] --> m{dec} --> n(-ell-)\n",
    "agentflow-beta TB\n  a@{ label: 5, labelType: string }\n  b@{ label: [1, 2], labelType: 7 }\n  c@{ label: \"\" }\n  d@{ label: \"x\", labelType: text }\n",
    "agentflow-beta TB\n  a@{\n    - one\n    - two\n  }\n  b@{\n    just text\n  }\n  c@{\n    5\n  }\n  d@{\n    false\n  }\n",
    "agentflow-beta TB\n  a@{\n    k: 1,\n    l: \"x, y\",\n    m: [1, 2],\n    n: |\n      text,\n    o: 'it''s',  # note,\n  }\n",
    "agentflow-beta TB\n  a@{\n    k: 1,\n    : bad,\n  }\n",
    "agentflow-beta TB\n  a@{ k: [1, }\n",
    "---\ntitle: T\n---\nagentflow-beta TB\n  %% note\n  a@{\n    k: \"unclosed\n  }\n",
    "---\ntitle: T\n---\n%% first\nagentflow-beta TB\n  a --> b\n  flow f[\"F\"]\n    c --> d\n  end\n  connector k[\"K\"]\n  a@{ x: 1 }\n",
    "agentflow-beta TB\n  a@{\n    __proto__: 1\n    constructor:\n      prototype: 2\n    ok:\n      __proto__: [3]\n      k: 4\n      list:\n        - prototype: 5\n          kept: 6\n  }\n",
    "agentflow-beta TB\n  a e1@--> b\n  e1@{ animate: true, animation: fast, curve: linear, note: x }\n  e1@{ curve: 0 }\n  a e2@--x b\n  e2@{ curve: null, animate: null }\n",
    "agentflow-beta TB\n  linkStyle default interpolate basis stroke:red\n  a --> b\n  b -.- c\n  linkStyle 1 interpolate step\n  linkStyle 0,1 stroke:blue\n",
    "agentflow-beta TB\n  a --> b\n  linkStyle 3 stroke:red\n",
    "agentflow-beta TB\n  flow f[\"F\"]@{ view: collapsed }\n    a --> b\n    flow g\n      c\n    end\n  end\n  a --> x\n  x --> c\n  f --> x\n",
    "agentflow-beta TB\n  flow f\n    a\n  end\n  f@{ view: collapsed }\n  flow g\n    f\n    b\n  end\n  g@{ view: collapsed }\n  a --> b\n  b --> z\n",
    "agentflow-beta TB\n  flow a\n    b\n  end\n  flow b\n    a\n  end\n",
    "agentflow-beta TB\n  flow a\n    b\n    c\n  end\n  flow b\n    c\n    d\n  end\n  flow a\n    e\n    direction LR\n  end\n  flow a[\"Again\"]\n  end\n",
    "agentflow-beta TB\n  global\n    a\n    b --> c\n  end\n  flow f\n    a --> d\n    c\n  end\n",
    "agentflow-beta TB\n  flow g\n    x\n    y\n  end\n  global\n    x\n  end\n  flow h\n    x\n    y\n  end\n",
    "agentflow-beta TB\n  flow\n    a\n  end\n  flow\n    b\n  end\n  flow  spaced id\n    c\n  end\n  flow  [\"only title\"]\n    d\n  end\n",
    "agentflow-beta TB\n  connector gh[\"GitHub\"]@{ protocol: \"mcp\" }\n  connector gh[\"Again\"]\n  connector s\n  s@{ k: 1 }\n  gh --> s\n  connectors --> gh\n  connector  \n",
    "agentflow-beta TB\n  a --> b\n  connector a\n  a[\"Later\"]\n  style a fill:#f00\n",
    "agentflow-beta TB\n  flow f\n    a\n  end\n  f[\"As a node\"] --> a\n  f:::hot\n  classDef hot fill:#f00,color:#fff\n  style f stroke:blue\n",
    "agentflow-beta TB\n  a & b --> c & d\n  a & b@{ k: 1 } --> c\n  a@{ k: 2 } & b --> d@{ k: 3 }\n",
    "agentflow-beta TB\n  a -- text --> b\n  a -- \"quoted\" --x b\n  a -->|pipe| b\n  a --- b\n  a -.- b\n  a -..- b\n  a ----> b\n  a -------------> b\n",
    "agentflow-beta LR\n  a --> b\n  click a href \"https://example.com\" \"tip\" _blank\n  click b call cb(\"x\") \"tip\"\n  click a \"javascript:alert(1)\"\n  click b cb\n",
    "agentflow-beta TB\n  accTitle: A title\n  accDescr: A description\n  accDescr {\n   many\n   lines\n  }\n  a\n",
    "agentflow-beta\n  a\n",
    "agentflow-beta >\n  a\n",
    "agentflow-beta BR\n  a --> b\n",
    "agentflow-beta TB %% trailing\n  a --> b %% four\n  b[one %% six\n two] --> c(-el %% seven\n lipse-)\n  d[/tr %% eight\n ap/]\n",
    "agentflow-beta TB\n  a[x]}   \n  b\n",
    "agentflow-beta TB\n  a@{ k: 1 }   \n  b --> }\n",
    "agentflow-beta TB\n  a --> b\n  flow f\n    c\n",
    "agentflow-beta TB\n  a --> b\n  end\n",
    "agentflow-beta TB\n  a ==> b\n",
    "agentflow-beta TB\n  a --> b %% comment\n\n\n   \n",
    "agentflow-beta TB\n  a -->\n",
    "agentflow-beta TB\n  flow f x\n  end\n",
    "agentflow-beta TB\n  global x\n  end\n",
    "agentflow-beta TB\n  a[|key:val|label] --> b[|k2:w2|x]\n  a[|k3:w3|y]\n  a[|__proto__:w4|z]\n",
    "agentflow-beta TB\n  a --> b\r\n  b --> c\r  c --> d\u{2028}x\n",
    "%%{init: {\"flowchart\": {\"curve\": \"step\", \"inheritDir\": true}}}%%\nagentflow-beta LR\n  flow f\n    a --> b\n  end\n",
];

#[test]
fn cases_match_pele() {
    let cases: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    run("case", &cases).finish("hand-written agentflows");
}

#[test]
fn parse_options_match_pele() {
    let source = "agentflow-beta LR\n  flow f\n    a & b --> c & d\n  end\n  x --> y\n";
    let cases: Vec<(String, Value)> = [
        json!({ "maxEdges": 3 }),
        json!({ "maxEdges": 5 }),
        json!({ "config": { "flowchart": { "inheritDir": true, "curve": "linear" } } }),
        json!({ "config": { "flowchart": { "inheritDir": "true", "curve": 5 } } }),
        json!({ "config": { "flowchart": { "curve": "" }, "direction": "RL" } }),
    ]
    .into_iter()
    .map(|options| (source.to_owned(), options))
    .collect();
    let tally = run_with("option case", &cases).finish("agentflows with parse options");
    assert_eq!(tally.rejected, 1, "the edge limit rejects one case");
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

const IDS: &[&str] = &["a", "b", "c", "d", "n1", "é", "x_y", "f", "g", "k", "connectors", "end1", "v", "A-B"];
const FLOWS: &[&str] = &["f", "g", "h", "a", "sub one", "f"];
const TEXTS: &[&str] = &["Run", "a b", "**bold**", "#35; x", "<b>t</b>", "x:y", "multi word text here", "1", "é ö"];
const SHAPES: &[&str] = &[
    "tool",
    "task",
    "input",
    "decision",
    "refdoc",
    "action",
    "round",
    "rect",
    "hex",
    "hexagon",
    "diamond",
    "subproc",
    "subprocess",
    "framed-rectangle",
    "lin-doc",
    "lined-document",
    "lean-right",
    "stadium",
    "circle",
    "cyl",
    "doc",
    "odd",
    "delay",
    "collapsedGroup",
    "constructor",
    "\"\"",
];
/// Shapes Pele rejects, each in its own way.
const BAD_SHAPES: &[&str] = &["nosuch", "Tool", "lean_right", "5", "[a]", "true", "{k: 1}"];
const VALUES: &[&str] =
    &["1", "x", "\"q, r\"", "[1, 2]", "{a: 1}", "true", "null", ".nan", ".inf", "-0", "'it''s'", "1e3", "0x1f", "~"];

/// One entry of a metadata block.
fn field(rng: &mut Rng) -> String {
    match rng.below(12) {
        0..=2 => format!("shape: {}", if rng.chance(25) { rng.pick(BAD_SHAPES) } else { rng.pick(SHAPES) }),
        3 => format!("label: {}", rng.pick(&["\"L\"", "5", "[1, 2]", "\"\"", "Plain text", "\"**md**\""])),
        4 => format!("labelType: {}", rng.pick(&["text", "string", "markdown", "other", "5"])),
        5 => format!("view: {}", rng.pick(&["collapsed", "expanded", "5"])),
        6 => format!("curve: {}", rng.pick(&["linear", "step", "basis", "0", "null", "stepAfter"])),
        7 => format!("{}: {}", rng.pick(&["animate", "animation"]), rng.pick(&["true", "fast", "null", "0"])),
        8 => format!("{}: {}", rng.pick(&["__proto__", "constructor", "prototype", "class", "icon"]), rng.pick(VALUES)),
        _ => format!("{}: {}", rng.pick(&["model", "k", "instruction", "w", "0", "10"]), rng.pick(VALUES)),
    }
}

fn metadata(rng: &mut Rng) -> String {
    let count = rng.below(4);
    let fields: Vec<String> = (0..count).map(|_| field(rng)).collect();
    match rng.below(40) {
        0..=27 => format!("@{{ {} }}", fields.join(", ")),
        28..=33 => {
            let comma = rng.pick(&[",", "", ", ", " ,"]);
            let lines: Vec<String> = fields.iter().map(|field| format!("    {field}{comma}")).collect();
            format!("@{{\n{}\n  }}", lines.join("\n"))
        }
        34..=38 => format!(
            "@{{\n    {}\n  }}",
            rng.pick(&[
                "- one\n    - two",
                "just text",
                "5",
                "k: |\n      block,\n    l: 2,",
                "k: [1,\n    2],",
                "false"
            ])
        ),
        _ => {
            format!("@{{ {} }}", rng.pick(&["k: [1,", ": x", "k: 'open", "a: 1, a: 2", "{a}", "", "\n    \"open\n  "]))
        }
    }
}

fn vertex(rng: &mut Rng) -> String {
    let id = rng.pick(IDS);
    let text = rng.pick(TEXTS);
    let mut out = match rng.below(16) {
        0..=5 => id.to_owned(),
        6 => format!("{id}[{text}]"),
        7 => format!("{id}[\"{text}\"]"),
        8 => format!("{id}({text})"),
        9 => format!("{id}{{{text}}}"),
        10 => format!("{id}[[{text}]]"),
        11 => format!("{id}{{{{{text}}}}}"),
        12 => format!("{id}[/{text}/]"),
        13 => format!("{id}[\"`{text}`\"]"),
        14 => {
            format!("{id}{}", rng.pick(&["([s])", "((c))", "[(db)]", ">odd]", "(-e-)", "[\\l\\]", "[/t\\]", "(((d)))"]))
        }
        _ => format!("{id}[|k:v|{text}]"),
    };
    if rng.chance(10) {
        out += &format!(":::{}", rng.pick(&["hot", "cold", "internal-link"]));
    }
    if rng.chance(4) {
        out += &metadata(rng);
    }
    out
}

fn node(rng: &mut Rng) -> String {
    let mut out = vertex(rng);
    while rng.chance(6) {
        out += " & ";
        out += &vertex(rng);
    }
    out
}

fn link(rng: &mut Rng) -> String {
    let id = if rng.chance(6) { rng.pick(&["e1@", "e2@", "a@"]) } else { "" };
    let arrow = match rng.below(60) {
        0..=24 => "-->".to_owned(),
        25..=31 => "--x".to_owned(),
        32..=38 => "-.-".to_owned(),
        39..=43 => rng.pick(&["--->", "---x", "-..-", "---", "----------------->"]).to_owned(),
        44..=48 => format!("-- {} -->", rng.pick(TEXTS)),
        49..=53 => format!("-- \"{}\" --x", rng.pick(TEXTS)),
        54..=58 => format!("-->|{}|", rng.pick(TEXTS)),
        _ => rng.pick(&["==>", "-.->", "--o", "<-->", "~~~"]).to_owned(),
    };
    format!("{id}{arrow}")
}

fn statement(rng: &mut Rng, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth + 1);
    match rng.below(120) {
        // Two flows that each name the other.
        110..=112 if depth == 0 => *out += "  flow p\n    q\n    a\n  end\n  flow q\n    p\n    b\n  end\n",
        113..=117 => {
            *out += &format!(
                "{indent}{}\n",
                rng.pick(&[
                    "a ==> b",
                    "end",
                    "a -->",
                    "flow f x",
                    "a[unclosed",
                    "global x",
                    "connector",
                    "a --> b }  ",
                    "a@{ k: 1 }  "
                ])
            );
        }
        roll => statement_of(rng, depth, out, roll % 23),
    }
}

fn statement_of(rng: &mut Rng, depth: usize, out: &mut String, roll: usize) {
    let indent = "  ".repeat(depth + 1);
    match roll {
        0..=7 => {
            let mut line = node(rng);
            for _ in 0..1 + rng.below(2) {
                line += &format!(" {} {}", link(rng), node(rng));
            }
            *out += &format!("{indent}{line}\n");
        }
        8 => *out += &format!("{indent}{}\n", node(rng)),
        9..=11 if depth < 3 => {
            let header = match rng.below(6) {
                0 => "flow".to_owned(),
                1 => format!("flow {}[\"{}\"]", rng.pick(FLOWS), rng.pick(TEXTS)),
                2 => format!("flow {}{}", rng.pick(FLOWS), metadata(rng)),
                3 => format!("flow {}[{}]@{{ view: collapsed }}", rng.pick(FLOWS), rng.pick(TEXTS)),
                4 => "global".to_owned(),
                _ => format!("flow {}", rng.pick(FLOWS)),
            };
            *out += &format!("{indent}{header}\n");
            if rng.chance(4) {
                *out += &format!("{indent}  direction {}\n", rng.pick(&["TB", "LR", "RL", "BT", "TD"]));
            }
            for _ in 0..rng.below(4) {
                statement(rng, depth + 1, out);
            }
            if !rng.chance(60) {
                *out += &format!("{indent}end\n");
            }
        }
        12 => {
            let title = if rng.chance(2) { format!("[\"{}\"]", rng.pick(TEXTS)) } else { String::new() };
            let data = if rng.chance(3) { metadata(rng) } else { String::new() };
            *out += &format!("{indent}connector {}{title}{data}\n", rng.pick(&["gh", "s", "a", "k", "two words"]));
        }
        13 => {
            *out +=
                &format!("{indent}{}{}\n", rng.pick(&["f", "g", "gh", "e1", "e2", "a", "connectors"]), metadata(rng))
        }
        14 => {
            let styles = rng.pick(&[
                "fill:#f00,color:#fff",
                "stroke:blue,stroke-width:2px",
                "font-weight:bold",
                "fill:yellow\\,x",
            ]);
            *out += &format!("{indent}classDef {} {styles}\n", rng.pick(&["hot", "cold", "hot,cold"]));
        }
        15 => {
            *out +=
                &format!("{indent}class {} {}\n", rng.pick(&["a", "a,b", "f", "e1", "gh"]), rng.pick(&["hot", "cold"]))
        }
        16 => {
            let styles = rng.pick(&["fill:#bbf", "stroke:red,stroke-dasharray:3 3", "color:green", "fill:none"]);
            *out += &format!("{indent}style {} {styles}\n", rng.pick(&["a", "b", "f", "gh"]));
        }
        17 => {
            let positions = rng.pick(&["0", "1", "0,1", "default", "7", "00"]);
            let rest =
                rng.pick(&["stroke:red", "interpolate basis", "interpolate step stroke:blue,fill:red", "fill:none"]);
            *out += &format!("{indent}linkStyle {positions} {rest}\n");
        }
        18 => {
            let action = rng.pick(&[
                "href \"https://example.com\"",
                "\"https://example.com\" \"tip\" _blank",
                "call cb()",
                "call cb(\"x\") \"tip\"",
                "\"javascript:alert(1)\"",
                "href \"\" _self",
            ]);
            *out += &format!("{indent}click {} {action}\n", rng.pick(&["a", "b", "a,b", "gh"]));
        }
        19 => {
            *out += &format!(
                "{indent}{}\n",
                rng.pick(&["accTitle: The title", "accDescr: A description", "accDescr {\n  many\n  lines\n}"])
            )
        }
        20 => *out += &format!("{indent}direction {}\n", rng.pick(&["TB", "LR", "RL", "BT", "TD"])),
        _ => *out += &format!("{indent}{}\n", rng.pick(&["%% a comment", "", "a --> b %% trailing", "a;b;c", "  "])),
    }
}

fn random_agentflow(rng: &mut Rng) -> String {
    let mut out = String::new();
    if rng.chance(8) {
        out += &format!("---\ntitle: {}\n---\n", rng.pick(&["Front", "A #35; b"]));
    }
    if rng.chance(8) {
        out += "%% leading comment\n";
    }
    out += rng.pick(&[
        "agentflow-beta TB\n",
        "agentflow-beta LR\n",
        "agentflow-beta\n",
        "agentflow-beta RL\n",
        "agentflow-beta BT;",
        "agentflow-beta TD %% note\n",
        "agentflow-beta >\n",
    ]);
    for _ in 0..1 + rng.below(8) {
        statement(rng, 0, &mut out);
    }
    out
}

#[test]
fn random_agentflows_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<String> = (0..4000).map(|_| random_agentflow(&mut rng)).collect();
    let tally = run("random", &cases).finish("random agentflows");
    assert!(tally.identical > 800, "many random agentflows parse");
    assert!(tally.rejected > 800, "many random agentflows are rejected");
    tally.reaches_everything("random agentflows");
}
