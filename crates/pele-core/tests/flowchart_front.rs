//! Differential test for the flowchart front end: for every source, this port's flowchart model
//! and flow graph must come out as Pele 1.13.0's do, byte for byte in the JSON of the oracle's
//! `flow_front` op, and a source Pele rejects must be rejected here with Pele's message and
//! position.
//!
//! `tests/fixtures/flowchart/harvest.json` holds the flowchart sources of Pele's own test suite
//! and the inputs of its seeded mutation runs, written by `scripts/harvest-flowchart.mjs`.

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/flowchart/harvest.json");
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
        let ours = pele_core::flowchart_front_json(source, parse_options(options));
        tally.compare(&format!("{name} {index} with {options}"), source, ours, theirs);
    }
    tally
}

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested flowchart sources");
    assert!(tally.identical > 1000, "the harvest holds flowcharts Pele parses");
    assert!(tally.rejected > 100, "the harvest holds flowcharts Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated flowchart sources");
}

#[test]
fn edge_limits_and_host_config_match_pele() {
    let mut cases = Vec::new();
    for max in [0, 1, 2, 3, 5] {
        for source in
            ["graph TD\nA-->B-->C-->D", "graph TD\nA & B --> C & D", "graph TD\nA", "graph TD\nA-->B\nlinkStyle 2 x:y"]
        {
            cases.push((source.to_owned(), json!({ "maxEdges": max })));
        }
    }
    for config in [
        json!({"flowchart": {"inheritDir": true}}),
        json!({"flowchart": {"inheritDir": "yes", "curve": "basis"}}),
        json!({"flowchart": {"curve": 5}}),
        json!({"flowchart": "curve"}),
        json!({"flowchart": null}),
    ] {
        for source in [
            "graph LR\nsubgraph S\nA-->B\nend",
            "graph LR\nsubgraph S\ndirection TB\nA-->B\nend\nlinkStyle default interpolate linear",
            "---\nconfig:\n  flowchart:\n    curve: step\n---\ngraph LR\nA-->B",
        ] {
            cases.push((source.to_owned(), json!({ "config": config })));
        }
    }
    run("option case", &cases).finish("flowcharts with parse options");
}

/// Hand-written sources for the paths the harvest reaches least: collapsed and cyclic
/// subgraphs, id collisions, key order, metadata coercion, invalid links and error positions.
const CASES: &[&str] = &[
    "graph TD\nsubgraph O\n  subgraph I\n    a --> b\n  end\n  c\nend\nx --> a\nb --> x\na --> c\nO@{ view: collapsed }",
    "graph TD\nsubgraph O\n  subgraph I\n    a --> b\n  end\n  c\nend\nx --> a\nI@{ view: collapsed }\nb --> c",
    "graph TD\nsubgraph O\n  subgraph I\n    a --> b\n  end\nend\nI@{ view: collapsed }\nO@{ view: collapsed }\na --> b\nO --> I",
    "graph TD\nsubgraph O\nA --> B\nend\nO@{ view: \"collapsed\", label: x }\nO@{ other: 1 }\nA --> A",
    "graph TD\nsubgraph A\n subgraph B\n  A\n end\nend\nA --> B",
    "graph TD\nsubgraph A\n subgraph B\n  A\n end\nend\nA@{ view: collapsed }\nB@{ view: collapsed }\nA --> B",
    "graph LR\nsubgraph S\nx\nend\nsubgraph S\ny\nend\nsubgraph S [Again]\nx\nz\nend",
    "graph TD\nsubgraph\na --> b\nend\nsubgraph \" \"\nc\nend\nsubgraph \" \"[t]\nd\nend\nsubgraph a b [t]\ne\nend\nsubgraph two words\nf\nend",
    "graph TD\nsubgraph S\n;\naccTitle: inside\naccDescr: also\nA --> B & C --> D\nend",
    "graph TD\nS --> A\nsubgraph S\nA\nend\nstyle S fill:red\nclass S c1\nclassDef c1 fill:blue\nS:::c1",
    "graph RL\nsubgraph S\ndirection TD\nA\nend\nsubgraph T\nB\nend\nsubgraph U\ndirection LR\ndirection BT\nC\nend",
    "graph TD\nA e1@--> B\nA e1@--> C\nA & A e2@--> B\na a@b@--> c\ne1@{ animate: true }\ne2@{ curve: null, animation: [a, 1] }",
    "graph TD\nA-->B\nA-->B\nA-->B\nA e1@-->B\nL_A_B_0@{ animate: 5 }",
    "graph TD\nA-->B\nB-->C\nlinkStyle default stroke:red\nlinkStyle 0,1 stroke:blue,fill:green\nlinkStyle 1 interpolate basis\nlinkStyle default interpolate linear\nC-->D\nlinkStyle 2 interpolate step stroke-width:2px",
    "graph TD\nA-->B\nlinkStyle 0 interpolate basis color:red\nlinkStyle 0 fill:none",
    "graph TD\nclassDef a,b fill:#f00\\,x,color:red,stroke:green\nclassDef a stroke:green\nA:::a --> B:::b\nclass A,B,missing a",
    "graph TD\nA-->B\nclick A href \"https://example.com\" \"tip\" _blank\nclick B call cb(1, 2) \"tip2\"\nclick A,B \"  \"\nclick C callback \"t\"\nclick B \"/local#35;x\" _top",
    "graph TD\nstyle A,B fill:red\nstyle Q stroke:blue\nA --> B",
    "graph TD\nA[|__proto__:x|t] --> B[|k2:x|t]\nA[|__proto__:y|u]\nB[|k1:y|t]\nC[|a:b|x]\nC[|c:d|y]\nD\nD[|__proto__:z|w]",
    "graph TD\nA@{ shape: cyl }\nB@{ shape: CYL }\n",
    "graph TD\nA@{ label: \"\" }\nB@{ icon: \"fa:x\", label: \"  \" }\nC[text]@{ img: \"a.png\" }\nD@{ img: \"a.png\", label: L }\nE@{}\nF@{\n  shape: hex\n  label: multi\n}",
    "graph TD\nA@{ w: \"0x10\", h: \"  12  \" }\nB@{ w: \"1e3\", h: Infinity }\nC@{ w: -5, h: true }\nD@{ w: [7], h: [1, 2] }\nE@{ w: \"1_0\", h: \"0b101\" }",
    "graph TD\nA@{ __proto__: x, 2: b, 1: a, constructor: c }\nsubgraph S\nB\nend\nS@{ 2: x }\nS@{ 1: y, 2: z }",
    "graph TD\nA@{ shape: cyl } & B --> C\nA@{ shape: cyl } B",
    "graph TD\nA-- \"quoted\" -->B\nC--\"`md`\"-->D\nE-->|\"q\"|F\nG-->|`m`|H",
    "graph TD\nA-------->B\nC-......->D\nE=========>F\nG~~~~~~H\nI---J",
    "graph TD\nA <-- x --x B\nC <== y ==> D\nE x-- z --o F\nG o-. w .-o H\nI <-. v .-> J",
    "graph TD\nA -- x ==> B",
    "graph >\nA-->B",
    "graph BR\nA-->B",
    "graph TD;A-->B;B-->C",
    "graph TD   \nA-->B",
    "flowchart-elk LR\nA-->B",
    "graph TD\nA{x}   \n  B --> [oops]",
    "graph TD\nA@{ shape: cyl }  \n B -->\n",
    "graph TD\nA{x}  \n  B --> C\n  C --> ^",
    "graph TD\nA#35;B --> [x]",
    "graph TD\nA -->",
    "graph TD\nsubgraph S\nA",
    "graph TD\r\nA-->B\r\nB-->C",
    "graph TD\nA[\"a\u{2028}b\"] --> B",
    "graph TD\nA direction LR\nB",
    "graph TD\nsubgraph S\nA direction RL\nend",
    "graph TD\nA:::c1:::c2\nA --> B:::c3",
    "graph TD\nA & B --> C & D\nsubgraph S\nE & F --> G\nend",
    "graph TD\n  accTitle:   Title  \n  accDescr {\n    one\n      two\n  }\nA",
    "---\ntitle: Front #35; matter\n---\ngraph TD\nA",
    "graph TD\nend",
    "graph TD\nA --> B --> C\nlinkStyle 5 stroke:red",
    "graph TD\nA-->B\nlinkStyle 99999999999999999999 stroke:red",
];

#[test]
fn cases_match_pele() {
    run("case", &plain(CASES.iter().map(|&case| case.to_owned()).collect())).finish("hand-written flowcharts");
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

const IDS: &[&str] =
    &["A", "B", "C", "D", "E", "n1", "n2", "x", "o", "v", "12", "é", "end2", "S", "S2", "e1", "a-b", "A.B"];
const LABELS: &[&str] = &[
    "Hi",
    "a b",
    "\"quoted\"",
    "\"`**md** text`\"",
    "#35; x",
    "&amp; y",
    "fa:fa-car Car",
    "<b>bold</b>",
    " spaced ",
    "x/y",
    "Ünïcödé",
    "\"a\nb\"",
    "1",
    "multi word label",
];
const SHAPES: &[(&str, &str)] = &[
    ("[", "]"),
    ("(", ")"),
    ("((", "))"),
    ("(((", ")))"),
    ("([", "])"),
    ("[[", "]]"),
    ("[(", ")]"),
    ("{", "}"),
    ("{{", "}}"),
    ("[/", "/]"),
    ("[\\", "\\]"),
    ("[/", "\\]"),
    ("[\\", "/]"),
    (">", "]"),
    ("(-", "-)"),
    ("[|f:val|", "]"),
];
const LINKS: &[&str] = &[
    "-->",
    "---",
    "-.->",
    "-.-",
    "==>",
    "===",
    "~~~",
    "--x",
    "--o",
    "<-->",
    "x--x",
    "o--o",
    "---->",
    "-..->",
    "====>",
    "-- text -->",
    "-. dotted .->",
    "== thick ==>",
    "-->|label|",
    "---|a b|",
    "<-- both -->",
    "x-- cross --x",
    "--> ",
    " --> ",
    "-.-x",
    "<==>",
];
const METADATA: &[&str] = &[
    "{ shape: cyl, label: \"Store\" }",
    "{ shape: diamond }",
    "{ shape: bow-rect }",
    "{ label: \"from data\", labelType: string }",
    "{ icon: \"fa:user\", form: circle, pos: t }",
    "{ icon: \"fa:user\", label: \"\" }",
    "{ img: \"https://example.com/a.png\", w: 60, h: \"40\", constraint: on }",
    "{ img: \"x.png\", w: 0x10, h: [5] }",
    "{ animate: true, curve: linear }",
    "{ animation: fast }",
    "{ view: collapsed }",
    "{ label: [1, 2] }",
    "{ shape: rect, label: 3 }",
    "{ __proto__: x, 2: b, 1: a }",
];
/// Statements Pele rejects, one of which goes into some of the random flowcharts.
const MISTAKES: &[&str] = &[
    "A@{ shape: Rect }",
    "A@{ shape: no-such }",
    "A@{ shape: lean_right }",
    "A@{ bad: [ }",
    "A[\"\"]",
    "A -- mixed ==> B",
    "click A \"https://example.com\" ",
    "linkStyle 99 stroke:red",
    "A@{ label: x } e1@--> B",
    "end",
    "subgraph S\nA",
    "A --> B --> ",
];
const STYLES: &[&str] =
    &["fill:#f9f", "stroke:#333,stroke-width:4px", "color:red", "fill:#bbf,color:#fff", "stroke-dasharray: 5 5"];
const URLS: &[&str] =
    &["https://example.com", "https://Example.COM/a/../b", "javascript:alert(1)", "#35;", "/local", "mailto:a@b.c", ""];

fn node(rng: &mut Rng) -> String {
    let mut out = rng.pick(IDS).to_owned();
    if rng.chance(2) {
        let (open, close) = SHAPES[rng.below(SHAPES.len())];
        out += &format!("{open}{}{close}", rng.pick(LABELS));
    }
    if rng.chance(6) {
        out += &format!(":::{}", rng.pick(&["c1", "c2", "missing"]));
    }
    if rng.chance(6) {
        out += &format!("@{}", rng.pick(METADATA));
    }
    out
}

fn nodes(rng: &mut Rng) -> String {
    let mut out = node(rng);
    while rng.chance(5) {
        out += &format!(" & {}", node(rng));
    }
    out
}

/// One statement. `edges` counts the links written so far, so that most `linkStyle` indexes exist.
fn statement(rng: &mut Rng, depth: usize, edges: &mut usize) -> String {
    match rng.below(14) {
        0..=4 => {
            let mut out = nodes(rng);
            for _ in 0..rng.below(4) {
                // Shape data may not be followed by a space and a link id.
                let id = if rng.chance(5) { format!("{}@", rng.pick(&["e1", "e2", "x"])) } else { String::new() };
                let gap = if !id.is_empty() && out.ends_with('}') { "" } else { " " };
                out += &format!("{gap}{id}{} {}", rng.pick(LINKS), nodes(rng));
                *edges += 1;
            }
            out
        }
        5 if depth < 3 => {
            let header = match rng.below(4) {
                0 => String::new(),
                1 => format!(" {}", rng.pick(IDS)),
                2 => format!(" {} [{}]", rng.pick(IDS), rng.pick(LABELS)),
                _ => format!(" {}", rng.pick(&["\"a title\"", "two words", "S3"])),
            };
            let mut out = format!("subgraph{header}\n");
            if rng.chance(3) {
                out += &format!("direction {}\n", rng.pick(&["TB", "LR", "RL", "BT", "TD"]));
            }
            for _ in 0..1 + rng.below(4) {
                out += &statement(rng, depth + 1, edges);
                out += "\n";
            }
            out + "end"
        }
        6 => format!("classDef {} {}", rng.pick(&["c1", "c2", "c1,c2", "default"]), rng.pick(STYLES)),
        7 => format!("class {} {}", rng.pick(&["A", "A,B", "S", "e1"]), rng.pick(&["c1", "c2"])),
        8 => format!("style {} {}", rng.pick(IDS), rng.pick(STYLES)),
        9 => {
            let positions = match *edges {
                0 => "default".to_owned(),
                1 => rng.pick(&["0", "00", "default"]).to_owned(),
                count => match rng.below(3) {
                    0 => "0,1".to_owned(),
                    1 => "default".to_owned(),
                    _ => rng.below(count).to_string(),
                },
            };
            match rng.below(3) {
                0 => format!("linkStyle {positions} {}", rng.pick(STYLES)),
                1 => format!("linkStyle {positions} interpolate {}", rng.pick(&["basis", "linear", "stepBefore"])),
                _ => format!("linkStyle {positions} interpolate basis {}", rng.pick(STYLES)),
            }
        }
        10 => {
            let id = rng.pick(&["A", "B", "A,B"]);
            match rng.below(4) {
                0 => format!("click {id} \"{}\"", rng.pick(URLS)),
                1 => format!(
                    "click {id} href \"{}\" \"tip\" {}",
                    rng.pick(URLS),
                    rng.pick(&["_blank", "_self", "_top", "_parent"])
                ),
                2 => format!("click {id} call cb({})", rng.pick(&["", "1, 2"])),
                _ => format!("click {id} callback \"tooltip\""),
            }
        }
        11 => format!("{}@{}", rng.pick(&["e1", "e2", "S", "A"]), rng.pick(METADATA)),
        12 => {
            rng.pick(&["accTitle: The title", "accDescr: A description", "accDescr {\n  many\n  lines\n}"]).to_owned()
        }
        _ => rng.pick(&["%% a comment", "", ";", "direction LR"]).to_owned(),
    }
}

fn random_flowchart(rng: &mut Rng) -> String {
    let mut out = String::new();
    if rng.chance(6) {
        out += &format!(
            "---\ntitle: {}\nconfig:\n  flowchart:\n    inheritDir: {}\n---\n",
            rng.pick(&["Front", "A #35; b"]),
            rng.pick(&["true", "false"])
        );
    }
    out += rng.pick(&["graph", "flowchart", "flowchart-elk"]);
    out += rng.pick(&[" TD", " LR", " RL", " BT", " TB", " >", " <", " ^", " v", "", " BR", " TD;"]);
    out += "\n";
    let count = 1 + rng.below(12);
    let mistake = rng.chance(4).then(|| rng.below(count));
    let mut edges = 0;
    for index in 0..count {
        out += rng.pick(&["", "  ", "\t"]);
        if mistake == Some(index) {
            out += rng.pick(MISTAKES);
        } else {
            out += &statement(rng, 0, &mut edges);
        }
        out += rng.pick(&["\n", "\n", ";\n", "\n\n"]);
    }
    out
}

#[test]
fn random_flowcharts_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..3000).map(|_| (random_flowchart(&mut rng), json!({}))).collect();
    let tally = run("random", &cases).finish("random flowcharts");
    assert!(tally.identical > 500, "many random flowcharts parse");
    assert!(tally.rejected > 500, "many random flowcharts are rejected");
}
