//! Differential test for the requirement front end: for every source, this port's requirement
//! model and the graph it lays out must come out as Pele 1.13.0's do, byte for byte in the JSON
//! of the oracle's `requirement_front` op, and a source Pele rejects must be rejected here with
//! Pele's code, message and position.
//!
//! `tests/fixtures/requirement/harvest.json` holds the requirement sources of Pele's own test
//! suite and the inputs of its seeded mutation runs, written by `scripts/harvest-requirement.mjs`.

use pele_core::{ErrorCode, HostConfig, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/requirement/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str, options: &Value) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("requirement_front\t{source}\t{options}")
}

fn parse_options(options: &Value) -> ParseOptions {
    let mut parse = ParseOptions::default();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "limit" => parse.limits.source_utf16 = usize::try_from(value.as_u64().expect("a count")).expect("a count"),
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
        match std::panic::catch_unwind(|| pele_core::requirement_front_json(source, parse_options(options))) {
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested requirement sources");
    assert!(tally.identical > 900, "the harvest holds requirement diagrams Pele parses");
    assert!(tally.rejected > 1000, "the harvest holds requirement diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &plain(harvest("mutated"))).finish("mutated requirement sources");
    assert!(tally.identical > 1000 && tally.rejected > 1000, "the mutations both parse and fail");
}

const HEAD: &str = "requirementDiagram\n";

/// Hand-written sources for the paths the harvest reaches least: Jison's whitespace and
/// direction rules, repeated fields, shared names, class copies and the copy limit.
fn cases() -> Vec<(String, Value)> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let mut out: Vec<(String, Value)> = [
        "requirementDiagram \nrequirement a {\n}\n",
        "requirementDiagram\nrequirement a {\nid: 1\nid: 2\ntext: x\ntext: y\nrisk: low\nrisk: high\nverifymethod: test\nverifyMethod: analysis\n}\n",
        "requirementDiagram\nelement e {\ntype: a\ntype: b\ndocref: c\ndocRef: d\n}\nelement e {\ntype: z\n}\n",
        "requirementDiagram\nrequirement x {\nid: 1\n}\nelement x {\ntype: t\n}\nx - contains -> x\nx:::c\nclassDef c fill:red\n",
        "requirementDiagram\nfoo direction LR bar\nelement a {\n}\n",
        "requirementDiagram\ndirection TB direction LR\nxdirection   rl\n",
        "requirementDiagram\nDIRECTION lr\nDirection\tBt\n",
        "requirementDiagram\ndirection\nTB\n",
        "requirementDiagram\ndirection XX\n",
        "requirementDiagram\ntitle My title\n",
        "requirementDiagram\nTitle\nx\n",
        "accTitle: t\naccDescr: d\nrequirementDiagram\naccDescr {\n multi\n   line\n\n  more\n}\n",
        "ACCTITLE:x\nrequirementDiagram\naccdescr:   y  \n",
        "requirementDiagram\naccTitle\n",
        "requirementDiagram\naccDescr {\n unclosed",
        "requirementDiagram\nrequirement \"a b\":::c1, c2 {\n}\nelement \"e\":::\"q r\" {\n}\n",
        "requirementDiagram\nstyle a,b fill:#f9f,stroke:#333;stroke-width:4px\nrequirement a {\n}\nstyle a,b fill:#f9f\nstyle zz,a fill:red\n",
        "requirementDiagram\nrequirement r {\n}\nclassDef c1,c2 color: red, font-weight:bold\nclass r c1,c2\nr:::c1\nclassDef c1 fill:blue\n",
        "requirementDiagram\nclassDef c fill:red\nrequirement r:::c {\n}\nclassDef c stroke:blue\nclass r,missing c\n",
        "requirementDiagram\nstyle a fill:50%\n",
        "requirementDiagram\nstyle a fill:red\tx\n",
        "requirementDiagram\nrequirement a {\nrisk: Low\nverifymethod: Test\n}\n",
        "requirementDiagram\nrequirement a {\nrisk: unknown\n}\n",
        "requirementDiagram\nrequirement a {\nfoo: 1\n}\n",
        "requirementDiagram\nelement a {\nid: 1\n}\n",
        "requirementDiagram\nrequirement a\n{\n}\n",
        "requirementDiagram\nrequirement a {\nid: \"unclosed\n}\n",
        "requirementDiagram\na <- traces - b\nb - traces - a\n",
        "requirementDiagram\na -> b\n",
        "requirementDiagram\na < b\n",
        "requirementDiagram\na.b - contains -> c\n",
        "requirementDiagram\n# comment\n% also\n%% and this\nrequirement a {\n} # trailing\n",
        "requirementDiagram\r\nrequirement a {\r\nid: 1\r\n}\r\n",
        "requirementDiagram\rrequirement a {\r}\r",
        "requirementDiagram\n\u{2028}requirement a {\n}\n",
        "requirementDiagram\nrequirement _ {\ntext: a: b\n}\n",
        "requirementDiagram\nrequirement a {\ntext: \"a: b, c\"\n}\n",
        "requirementDiagram\nrequirement 1x {\ntext: Ünïcödé\n}\n",
        "requirementDiagram\nrequirement a {\nid:1\n}",
        "requirementDiagram",
        "%%{init: {}}%%\nrequirementDiagram\n",
        "---\ntitle: Front #35; matter\n---\nrequirementDiagram\nrequirement a {\n}\n",
        "requirementDiagram\nrequirement __proto__ {\nid: constructor\n}\nelement constructor {\ntype: toString\n}\n__proto__ - contains -> constructor\nclassDef __proto__ fill:red\nclass __proto__,constructor __proto__,toString\n",
    ]
    .iter()
    .map(|&source| (source.to_owned(), json!({})))
    .collect();
    let big = json!({ "limit": 10_000_000 });
    out.push((
        format!(
            "{HEAD}requirement a {{\n}}\nclassDef c {}a:b\n{}",
            "a:b,".repeat(6000),
            repeat(2500, &|_| "class a c\n".to_owned())
        ),
        big.clone(),
    ));
    out.push((format!("{HEAD}requirement a {{\n}}\n{}", "classDef c fill:red\nclass a c\n".repeat(1600)), big.clone()));
    out.push((
        format!("{HEAD}{}classDef c a:b\n", repeat(2000, &|i| format!("element e{i}:::c {{\n}}\n"))),
        big.clone(),
    ));
    out.push((format!("{HEAD}style a {}\n", "f:#-;".repeat(10000)), big.clone()));
    out.push((format!("{HEAD}{}", "element a {\n}\n".repeat(5000)), json!({})));
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("hand-written requirement diagrams");
    assert!(tally.rejected > 10, "the cases reach the errors");
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

const NAMES: &[&str] = &["a", "b", "r1", "test_req", "e", "\"a b\"", "\"x:y\"", "\"Élément\"", "x1", "constructor"];
const KINDS: &[&str] = &[
    "requirement",
    "functionalRequirement",
    "interfaceRequirement",
    "performanceRequirement",
    "physicalRequirement",
    "designConstraint",
    "Requirement",
];
const RELATIONS: &[&str] = &["contains", "copies", "derives", "satisfies", "verifies", "refines", "traces"];
const TEXTS: &[&str] = &[
    "x",
    "the test text.",
    "\"quoted text\"",
    "a b c",
    "\"**bold** _it_\"",
    "\"#35; &amp;\"",
    "1.2",
    "a long value that wraps across more than one line of the field table",
];
const STYLES: &[&str] =
    &["fill:#f9f", "stroke:#333", "stroke-width:4px", "color: red", "font-weight:bold", "fill:none"];
const MISTAKES: &[&str] =
    &["requirement a {\nfoo: 1\n}", "element {\n}", "a - nothing -> b", "a <- copies -> b", "style", "@", "{", "}"];

fn body_field(rng: &mut Rng, requirement: bool) -> String {
    if requirement {
        match rng.below(4) {
            0 => format!("id: {}", rng.pick(TEXTS)),
            1 => format!("text: {}", rng.pick(TEXTS)),
            2 => format!("risk: {}", rng.pick(&["low", "medium", "high", "High"])),
            _ => format!("verifymethod: {}", rng.pick(&["analysis", "demonstration", "inspection", "test"])),
        }
    } else if rng.chance(2) {
        format!("type: {}", rng.pick(TEXTS))
    } else {
        format!("docref: {}", rng.pick(TEXTS))
    }
}

fn block(rng: &mut Rng, requirement: bool) -> String {
    let head = if requirement { rng.pick(KINDS) } else { "element" };
    let mut out = format!("{head} {}", rng.pick(NAMES));
    if rng.chance(4) {
        out += &format!(":::{}", rng.pick(&["c1", "c1,c2", "\"q\""]));
    }
    out += " {\n";
    for _ in 0..rng.below(5) {
        out += &body_field(rng, requirement);
        out += rng.pick(&["\n", "\n\n", "\n", " \n"]);
    }
    out + "}"
}

fn statement(rng: &mut Rng) -> String {
    match rng.below(12) {
        0..=2 => block(rng, true),
        3 | 4 => block(rng, false),
        5..=7 => {
            let (a, b, kind) = (rng.pick(NAMES), rng.pick(NAMES), rng.pick(RELATIONS));
            if rng.chance(2) { format!("{a} - {kind} -> {b}") } else { format!("{a} <- {kind} - {b}") }
        }
        8 => format!("classDef {} {}", rng.pick(&["c1", "c2", "c1,c2"]), rng.pick(STYLES)),
        9 => match rng.below(3) {
            0 => format!("class {} {}", rng.pick(&["a", "a,b", "e"]), rng.pick(&["c1", "c2", "c1,c2"])),
            1 => format!("{}:::{}", rng.pick(NAMES), rng.pick(&["c1", "c2"])),
            _ => format!("style {} {},{}", rng.pick(&["a", "a,b", "zz,a"]), rng.pick(STYLES), rng.pick(STYLES)),
        },
        10 => rng
            .pick(&["direction LR", "direction RL", "direction BT", "direction TB", "accTitle: T", "accDescr: D"])
            .to_owned(),
        _ => rng.pick(&["%% comment", "# comment", "", "accDescr {\n a\n b\n}"]).to_owned(),
    }
}

fn random_requirement(rng: &mut Rng) -> String {
    let mut out = String::new();
    if rng.chance(8) {
        out += "---\ntitle: A title\n---\n";
    }
    out +=
        rng.pick(&["requirementDiagram\n", "requirementDiagram\n\n", "requirementDiagram\n", "requirementDiagram \n"]);
    let count = 1 + rng.below(10);
    let mistake = rng.chance(6).then(|| rng.below(count));
    for index in 0..count {
        out += rng.pick(&["", "  ", "\t"]);
        if mistake == Some(index) {
            out += rng.pick(MISTAKES);
        } else {
            out += &statement(rng);
        }
        out += rng.pick(&["\n", "\n", "\n\n", "\r\n"]);
    }
    out
}

#[test]
fn random_requirements_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..3000).map(|_| (random_requirement(&mut rng), json!({}))).collect();
    let tally = run("random", &cases).finish("random requirement diagrams");
    assert!(tally.identical > 1000, "many random requirement diagrams parse");
    assert!(tally.rejected > 300, "many random requirement diagrams are rejected");
}
