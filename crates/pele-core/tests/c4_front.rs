//! Differential test for the C4 front end: for every source, this port's C4 model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `c4_front` op, and a source
//! Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/c4/harvest.json` holds the C4 sources of Pele's own test suite, the corpus of
//! its C4 parser test, and the inputs of its generated and mutated runs, written by
//! `scripts/harvest-c4.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/c4/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("c4_front\t{source}")
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
                if theirs.starts_with("{\"type\":") && !theirs.starts_with("{\"type\":\"c4\",") {
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
        match std::panic::catch_unwind(|| pele_core::c4_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested C4 sources");
    assert!(tally.identical > 1000, "the harvest holds C4 diagrams Pele parses");
    assert!(tally.rejected > 100, "the harvest holds C4 diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &harvest("mutated")).finish("mutated C4 sources");
}

/// Hand-written sources for the paths the harvest reaches least: the end of input in each lexer
/// state, direction lines, what a short boundary list does with its kind, named arguments in
/// every slot, numbers as `parseInt` reads them, and redefinitions.
const CASES: &[&str] = &[
    "C4Context",
    "C4Context\n",
    "C4Context\nPerson(",
    "C4Context\nPerson(a",
    "C4Context\nPerson(a, \"",
    "C4Context\nPerson(a, $",
    "C4Context\nPerson(a, $k",
    "C4Context\nPerson(a, $k=",
    "C4Context\nPerson(a, $k=  \"",
    "C4Context\nPerson(a, $k=\"v",
    "C4Context\nPerson(a, $k=\"v\"",
    "C4Context\nPerson(a, $k=x)",
    "C4Context\nPerson(a, $k \"v\")",
    "C4Context\nPerson",
    "C4Context\nPerson x",
    "C4Context\nPerson()",
    "C4Context\nPerson( , a)",
    "C4Context\nPerson(a,,b)",
    "C4Context\nPerson(a, \"\")\n",
    "C4Context\nPerson(a, \"A\")\n",
    "C4Context\nPerson(a, \"A\") \n",
    "C4Context\nPerson(a)\nPerson(b, \"B\")\n",
    "C4Context\naccTitle",
    "C4Context\naccTitle:",
    "C4Context\naccTitle: t",
    "C4Context\naccTitle: t\n",
    "C4Context\naccDescr: d\nPerson(a, \"A\")\n",
    "C4Context\naccDescr {",
    "C4Context\naccDescr { one\n   two\n\n  three }\nPerson(a, \"A\")\n",
    "C4Context\naccDescr {\n}\nPerson(a, \"A\")\n",
    "C4Context\naccDescription  spaced  \nPerson(a, \"A\")\n",
    "C4Context\naccDescription\nPerson(a, \"A\")\n",
    "C4Context\ntitle\nPerson(a, \"A\")\n",
    "C4Context\ntitle   \n\n\nPerson(a, \"A\")\n",
    "C4Context\ntitle  two  spaces \nPerson(a, \"A\")\n",
    "C4Context\ntitle a;b\nPerson(a, \"A\")\n",
    "C4Context\ntitle a#b\nPerson(a, \"A\")\n",
    "C4Context\ntitle x Person(a, \"A\")\n",
    "C4Context\ndirection TB\nPerson(a, \"A\")\n",
    "C4Context\ndirection  LR extra\nPerson(a, \"A\")\n",
    "C4Context\nPerson(a, \"direction TB\")\n",
    "C4Context\nPerson(a, \"A\") direction RL\nPerson(b, \"B\")\n",
    "C4Context\ndirection\nBT\nPerson(a, \"A\")\n",
    "C4Context\ndirection direction LR\nPerson(a, \"A\")\n",
    "C4Context\ndirection XX\nPerson(a, \"A\")\n",
    "C4Context\nxdirection TB\nPerson(a, \"A\")\n",
    "C4Context\n%% c\nPerson(a, \"A\")\n",
    "C4Context\n%% c\r\n\r\nPerson(a, \"A\")\n",
    "C4Context\nPerson(a, \"A\") %% c\n",
    "C4Context\n{\n}\n",
    "C4Context\n}\n",
    "C4Context\nEnterprise_Boundary(b) {\nPerson(a, \"A\")\n}\n",
    "C4Context\nEnterprise_Boundary(b, \"B\") {\nPerson(a, \"A\")\n}\n",
    "C4Context\nEnterprise_Boundary(b, \"B\", \"T\", \"tags\") {\nPerson(a, \"A\")\n}\n",
    "C4Context\nSystem_Boundary(b)\n{\nPerson(a, \"A\")\n}\n",
    "C4Container\nContainer_Boundary(b, \"B\")\n{\n\nContainer(a, \"A\")\n}\n",
    "C4Container\nContainer_Boundary(b, \"B\") {\n}\n",
    "C4Container\nContainer_Boundary(b, \"B\") {}\n",
    "C4Context\nBoundary(b, \"B\") x\n",
    "C4Context\nBoundary(b, \"B\")",
    "C4Context\nBoundary(b, \"B\") {\nPerson(a, \"A\")\n",
    "C4Context\nBoundary(b, \"B\") {\nBoundary(c, \"C\") {\nPerson(a, \"A\")\n}\n}\n",
    "C4Context\nBoundary(b, \"B\") {\nPerson(a, \"A\")\n}\nBoundary(b, \"B2\", \"t\") {\nPerson(c, \"C\")\n}\n",
    "C4Context\nBoundary($k=\"v\", \"B\") {\nPerson(a, \"A\")\n}\nBoundary($k=\"w\") {\nPerson(c, \"C\")\n}\n",
    "C4Context\nBoundary(global, \"G\") {\nPerson(a, \"A\")\n}\n",
    "C4Deployment\nDeployment_Node(n, \"N\", \"t\", \"d\", \"s\", \"tags\", \"link\") {\nNode_L(m, \"M\") {\nNode_R(o, \"O\", $descr=\"x\") {\nContainer(a, \"A\")\n}\n}\n}\n",
    "C4Deployment\nNode(n) {\nContainer(a, \"A\")\n}\n",
    "C4Context\nPerson($label=\"L\", \"A\")\n",
    "C4Context\nPerson(a, $label=\"L\")\n",
    "C4Context\nPerson(a, $descr=\"D\", \"x\", $techn=\"T\")\n",
    "C4Context\nPerson(a, \"A\", $type=\"T\", $sprite=\"s\", $tags=\"t\", $link=\"l\", $bgColor=\"red\")\n",
    "C4Context\nPerson(a, \"A\", $fontColor=\"red\", $borderColor=\"red\", $shadowing=\"x\", $shape=\"db\")\n",
    "C4Context\nPerson(a, \"A\", $legendText=\"l\", $legendSprite=\"s\", $other=\"o\", $other=\"p\", $more=\"q\")\n",
    "C4Context\nPerson(a, \"A\", \"d\", \"s\", \"t\", \"l\", \"extra\", \"more\")\n",
    "C4Context\nPerson(a, \"A\")\nPerson(a, \"B\")\n",
    "C4Context\nPerson(a, \"A\", \"d\")\nPerson_Ext(a, $label=\"B\")\n",
    "C4Context\nBoundary(b, \"B\") {\nPerson(a, \"A\")\n}\nPerson(a, \"again\")\n",
    "C4Container\nContainer(a, \"A\")\nContainerDb(b, \"B\", \"t\", \"d\")\nContainerQueue_Ext(c, \"C\", , \"d\")\n",
    "C4Component\nComponent(a, \"A\")\nComponentDb_Ext(b, \"B\")\nComponentQueue(c, \"C\", $techn=\"T\")\n",
    "C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nRel(a, b)\n",
    "C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nRel(a)\n",
    "C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nRel(a, b, \"x\")\nRel(a, b, \"y\", \"t\")\nRel_Back(a, b, \"z\")\n",
    "C4Context\nSystem(a, \"A\")\nRel($from=\"a\", b, \"x\")\nRel(a, $to=\"b\", \"y\")\nRel(a, b, $label=\"z\", $offsetX=\"3\")\n",
    "C4Context\nSystem(a, \"A\")\nRel(a, b, \"x\", $offsetY=\"-0\", $offsetX=\"0x1F\", $textColor=\"red\", $k=\"v\")\n",
    "C4Context\nSystem(a, \"A\")\nRel(a, b, \"x\", \"t\", \"d\", \"s\", \"tags\", \"link\", \"extra\")\n",
    "C4Context\nSystem(a, \"A\")\nBiRel(a, b, \"x\")\nRel_U(a, b, \"u\")\nRel_Up(b, a, \"u\")\nRel_D(a, a, \"d\")\nRel_Down(a, b, \"d\")\n",
    "C4Context\nRel_L(a, b, \"l\")\nRel_Left(a, b, \"l\")\nRel_R(a, b, \"r\")\nRel_Right(a, b, \"r\")\n",
    "C4Dynamic\nRelIndex(1, a, b, \"x\")\nRelIndex(a, b, \"y\")\nRelIndex(1, a, b)\n",
    "C4Context\nSystem(a, \"A\")\nRel(a, b, \"x\")\nUpdateRelStyle(a, b, \"red\", \"blue\", \"12px\", \" -7.5\")\n",
    "C4Context\nSystem(a, \"A\")\nRel(a, b, \"x\")\nUpdateRelStyle(a, b, $offsetX=\"x\", $offsetY=\"+3\", $lineColor=\"c\", $k=\"v\")\n",
    "C4Context\nSystem(a, \"A\")\nRel(a, b, \"x\")\nUpdateRelStyle(a, c, \"red\")\nUpdateRelStyle($from=\"a\", b, \"red\")\n",
    "C4Context\nSystem(a, \"A\")\nUpdateElementStyle(a, \"red\", \"blue\", \"green\", \"s\", \"db\", \"sp\", \"t\", \"lt\", \"ls\", \"more\")\n",
    "C4Context\nSystem(a, \"A\")\nUpdateElementStyle(a, $bgColor=\"red\", $label=\"L\", $nope=\"n\")\nUpdateElementStyle(x, \"red\")\n",
    "C4Context\nBoundary(b, \"B\") {\nSystem(a, \"A\")\n}\nUpdateElementStyle(b, \"red\")\nUpdateElementStyle($k=\"b\", \"red\")\n",
    "C4Context\nUpdateLayoutConfig(\"3\", \"1\")\nSystem(a, \"A\")\n",
    "C4Context\nUpdateLayoutConfig($c4BoundaryInRow=\"3\", $c4ShapeInRow=\"1\")\nSystem(a, \"A\")\n",
    "C4Context\nUpdateLayoutConfig(\"0\", \"-2\")\nUpdateLayoutConfig(\" 0x10 \", \"7.9\")\nUpdateLayoutConfig(\"\", \"x\")\n",
    "C4Context\nUpdateLayoutConfig(\"99999999999999999999999\", \"0x1fffffffffffffffffffffffffffffffffff\")\n",
    "C4Context\nSystem(__proto__, \"A\", $__proto__=\"polluted\", $constructor=\"x\")\nRel(__proto__, constructor, \"x\")\n",
    "C4Context\nSystem(a, \"A\", $link=\"https://x.test/#frag;x\")\nSystem(b, \"#35; &amp; <br>\")\n",
    "C4Context\nSystem(a, unquoted label, \"d\")\n",
    "C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nRel(a, b, \"x\")\n\n\n",
    "C4Context\r\nSystem(a, \"A\")\r\nRel(a, a, \"self\")\r\n",
    "C4Context\n  title T\n  System(a, \"A\")\n",
    "C4Context\ntitle T\nSystem(a, \"A\")\ntitle U\naccTitle: V\n",
    "C4Context\ntitle T System(a, \"A\")\n",
    "C4Context\naccTitle: T\nSystem(a, \"A\")\n",
    "---\ntitle: Front\n---\nC4Context\nSystem(a, \"A\")\n",
    "---\ntitle: Front\n---\nC4Context\ntitle Own\nSystem(a, \"A\")\n",
    "C4Context\nSystem(a, \"A\")\n}\n",
    "C4Context\nSystem(a, \"A\") System(b, \"B\")\n",
    "C4Context\n\tSystem(a, \"A\")\t\n",
    "C4Context\nSystem (a, \"A\")\n",
    "C4Context\nSystem(a, \"A\")x\n",
    "C4Context\nSystemDbX(a, \"A\")\n",
    "C4Context\nPersonPersonPersonPerson(a, \"A\")\n",
    "C4Context\n\u{e9}\n",
    "C4Context\nSystem(a, \"\u{e9}\u{2028}x\")\n",
    "C4Context \nSystem(a, \"A\")\n",
    "C4Context x\nSystem(a, \"A\")\n",
];

#[test]
fn hand_written_sources_match_pele() {
    let sources: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    run("case", &sources).finish("hand-written C4 sources");
}

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
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const HEADERS: &[&str] = &["C4Context", "C4Container", "C4Component", "C4Dynamic", "C4Deployment"];
const SHAPES: &[&str] = &[
    "Person",
    "Person_Ext",
    "System",
    "SystemDb",
    "SystemQueue",
    "System_Ext",
    "SystemDb_Ext",
    "SystemQueue_Ext",
    "Container",
    "ContainerDb",
    "ContainerQueue",
    "Container_Ext",
    "ContainerDb_Ext",
    "ContainerQueue_Ext",
    "Component",
    "ComponentDb",
    "ComponentQueue",
    "Component_Ext",
    "ComponentDb_Ext",
    "ComponentQueue_Ext",
];
const BOUNDARIES: &[&str] = &[
    "Boundary",
    "Enterprise_Boundary",
    "System_Boundary",
    "Container_Boundary",
    "Deployment_Node",
    "Node",
    "Node_L",
    "Node_R",
];
const RELS: &[&str] = &[
    "Rel",
    "BiRel",
    "Rel_Up",
    "Rel_U",
    "Rel_Down",
    "Rel_D",
    "Rel_Left",
    "Rel_L",
    "Rel_Right",
    "Rel_R",
    "Rel_Back",
    "RelIndex",
];
const KEYS: &[&str] = &[
    "label",
    "type",
    "descr",
    "techn",
    "sprite",
    "tags",
    "link",
    "bgColor",
    "fontColor",
    "borderColor",
    "shadowing",
    "shape",
    "legendText",
    "legendSprite",
    "textColor",
    "lineColor",
    "offsetX",
    "offsetY",
    "c4ShapeInRow",
    "c4BoundaryInRow",
    "other",
    "__proto__",
];
const VALUES: &[&str] = &["x", "A b", "red", "#fff", "12", "-3", "0x1f", "db", "queue,person", "", " pad ", "&amp;"];
/// What a named argument's value may be. An empty one is a syntax error.
const NAMED: &[&str] = &["x", "A b", "red", "#fff", "12", "-3", "0x1f", "db", "queue,person", " pad ", "&amp;", "2"];

/// Random C4 documents, built from statements with arguments of every form, so that the db's
/// rules for slots, names and redefinitions meet each other.
fn random_document(rng: &mut Rng) -> String {
    let aliases = ["a", "b", "c", "d", "g", "h"];
    let mut src = rng.pick(HEADERS).to_string();
    src.push_str(rng.pick(&["\n", "\n", "\n\n", " \n", "\r\n"]));
    let mut depth = 0;
    let argument = |rng: &mut Rng| -> String {
        match rng.below(10) {
            0 => String::new(),
            1 => format!("${}=\"{}\"", rng.pick(KEYS), rng.pick(NAMED)),
            2 => rng.pick(&aliases).to_string(),
            3 => " ".to_owned(),
            _ => format!("\"{}\"", rng.pick(VALUES)),
        }
    };
    // An unquoted last argument runs on to the next comma in the source, through the closing
    // parenthesis, so most lists end in a quoted or named one.
    let arguments = |rng: &mut Rng, first: String| -> String {
        let mut list = vec![first];
        for _ in 0..rng.below(9) {
            list.push(argument(rng));
        }
        if rng.below(8) > 0 {
            list.push(match rng.below(3) {
                0 => format!("${}=\"{}\"", rng.pick(KEYS), rng.pick(NAMED)),
                _ => format!("\"{}\"", rng.pick(VALUES)),
            });
        }
        list.join(rng.pick(&[", ", ",", " , "]))
    };
    for _ in 0..1 + rng.below(14) {
        let alias = if rng.below(12) == 0 {
            format!("${}=\"{}\"", rng.pick(KEYS), rng.pick(&aliases))
        } else {
            rng.pick(&aliases).to_string()
        };
        let line = match rng.below(14) {
            0..=4 => format!("{}({})", rng.pick(SHAPES), arguments(rng, alias)),
            5..=6 => {
                // The first statement in a boundary has to be a diagram statement.
                depth += 1;
                let open = rng.pick(&[" {\n", "{\n", "\n{\n", " {\n\n", "\n{"]);
                let kind = rng.pick(BOUNDARIES);
                let list = arguments(rng, alias);
                let inner = rng.pick(&aliases);
                let label = rng.pick(VALUES);
                format!("{kind}({list}){open}{}({inner}, \"{label}\")", rng.pick(SHAPES))
            }
            7..=9 => {
                let index = if rng.below(2) == 0 { "1, " } else { "" };
                let to = rng.pick(&aliases);
                let kind = rng.pick(RELS);
                let first = argument(rng);
                format!("{kind}({index}{alias}, {to}, {})", arguments(rng, first))
            }
            10 => format!("UpdateElementStyle({})", arguments(rng, alias)),
            11 => {
                let to = rng.pick(&aliases);
                let first = argument(rng);
                format!("UpdateRelStyle({alias}, {to}, {})", arguments(rng, first))
            }
            12 => {
                let first = argument(rng);
                format!("UpdateLayoutConfig({})", arguments(rng, first))
            }
            _ => match rng.below(5) {
                0 => format!("title {}", rng.pick(VALUES)),
                1 => format!("accTitle: {}", rng.pick(VALUES)),
                2 => format!("accDescr: {}", rng.pick(VALUES)),
                3 => format!("accDescr {{\n  {}\n   x\n}}", rng.pick(VALUES)),
                _ if depth > 0 => {
                    depth -= 1;
                    "}".to_owned()
                }
                _ => format!("accDescription {}", rng.pick(VALUES)),
            },
        };
        src.push_str(&line);
        // A comment takes the line breaks after it, so one at the end of a statement's line
        // leaves the statement unended.
        src.push_str(rng.pick(&["\n", "\n", "\n", "\n\n", " \n", "\n  ", "\r\n", "\n%% c\n", " %% c\n"]));
    }
    for _ in 0..depth {
        src.push_str("}\n");
    }
    src
}

#[test]
fn random_documents_match_pele() {
    let mut rng = Rng(0x0c4f_2026_1115);
    let sources: Vec<String> = (0..4000).map(|_| random_document(&mut rng)).collect();
    let tally = run("random", &sources).finish("random C4 documents");
    assert!(tally.identical > 1500, "most random documents parse, so the db's rules are what is compared");
}
