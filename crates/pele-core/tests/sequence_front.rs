//! Differential test for the sequence front end: for every source, this port's sequence model
//! must come out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's
//! `sequence_front` op, and a source Pele rejects must be rejected here with Pele's code, message
//! and position.
//!
//! `tests/fixtures/sequence/harvest.json` holds the sequence sources of Pele's own test suite,
//! the corpus of its sequence parser test, the inputs of its seeded mutation and generation runs
//! and the shapes of its worst cases, written by `scripts/harvest-sequence.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/sequence/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("sequence_front\t{source}")
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
    /// The rejections by what rejected the source.
    kinds: BTreeMap<&'static str, usize>,
    other_types: usize,
    /// Pele's answers for the sources it parses.
    models: Vec<String>,
    failures: Vec<String>,
}

/// What rejected a source, from Pele's error.
fn rejection(error: &Value) -> &'static str {
    let message = error["error"].as_str().unwrap_or_default();
    match error["code"].as_str().unwrap_or_default() {
        "syntax" if message.contains("Lexical error") => "syntax, lexer",
        "syntax" if message.contains("Parse error") => "syntax, parser",
        "syntax" if message.contains("is not valid YAML") => "syntax, config",
        "semantic" => "semantic, model",
        "limit" => "limit",
        "unsupported-diagram" => "no type",
        _ => "other",
    }
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
                    self.models.push(theirs.to_owned());
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
                    *self.kinds.entry(rejection(&theirs)).or_insert(0) += 1;
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
        println!("  rejected by: {:?}", self.kinds);
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
        match std::panic::catch_unwind(|| pele_core::sequence_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested sequence sources");
    assert!(tally.identical > 1000, "the harvest holds sequence diagrams Pele parses");
    for kind in ["syntax, parser", "syntax, config", "semantic, model"] {
        assert!(tally.kinds.get(kind).is_some_and(|&count| count > 5), "the harvest holds sources rejected by: {kind}");
    }
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated sequence sources");
    assert!(tally.identical > 3000 && tally.rejected > 3000, "the mutations both parse and fail");
    for kind in ["syntax, lexer", "syntax, parser", "syntax, config", "semantic, model"] {
        assert!(tally.kinds.get(kind).is_some_and(|&count| count > 10), "the mutations are rejected by: {kind}");
    }
}

#[test]
fn worst_cases_match_pele() {
    run("worst case", &harvest("worst")).finish("worst sequence sources");
}

/// Hand-written sources for the paths the harvest reaches least: participant configs, box
/// colours, menus, what creation and destruction demand, wrap settings and the end of input in
/// each lexer state.
const CASES: &[&str] = &[
    "sequenceDiagram\nparticipant A@{ \"type\": \"database\", \"alias\": \"Store\" }\nparticipant B@{ type: queue } as Q\nA->>B: x",
    "sequenceDiagram\nparticipant A@{ \"type\": null, \"alias\": 0 }\nparticipant B@{ \"type\": [1, [2, null]], \"alias\": [\"x\"] }\nparticipant C@{ \"alias\": {\"a\": 1} } as C",
    "sequenceDiagram\nparticipant A@{\n  type: boundary\n  alias: Edge\n}\nactor B@{ alias: Other } as wrap: B\nB->>A: x",
    "sequenceDiagram\nparticipant A@{ [1, 2] }\nparticipant B@{ }\nparticipant C@{ \"type\": \"entity\"",
    "sequenceDiagram\nparticipant A@{ a: [ }",
    "sequenceDiagram\nparticipant A@{ \"type\": \"queue\" } as\nparticipant B@{} as  \nA->>B: x",
    "sequenceDiagram\nparticipant A as wrap: Alice\nparticipant A as nowrap: Again\nparticipant A\nA->>A: x",
    "sequenceDiagram\nparticipant A\nparticipant A as Second\nB->>A: x\nparticipant B as Late",
    "sequenceDiagram\nbox Aqua Group\nparticipant A\nend\nbox rgb(1, 2, 3) wrap: Other\nparticipant A\nend",
    "sequenceDiagram\nbox\nparticipant A\nend\nbox\nactor A\nend",
    "sequenceDiagram\nA->>B: x\nbox rgba(0,0,0,0.5)\nparticipant A\nparticipant C\nend\nbox hsl(120deg 50% 50% / 0.5) T\nparticipant B\nend",
    "sequenceDiagram\nbox rgb (1,2,3) x\nparticipant A\nend\nbox RGB(1,2,3) x\nparticipant B\nend\nbox rgb(1,2) x) y\nparticipant C\nend",
    "sequenceDiagram\nbox hsl(1turn, 50%, 50%) a\nparticipant A\nend\nbox hsl(none none none) b\nparticipant B\nend\nbox rgb(1e2 .5 +3 / none) c\nparticipant C\nend",
    "sequenceDiagram\nbox rgb(1%, 2, 3) a\nparticipant A\nend\nbox rgba(1 2 3 / 4 / 5) b\nparticipant B\nend\nbox hsla(10, 20, 30%) c\nparticipant C\nend",
    "sequenceDiagram\nbox Transparent\nparticipant A\nend\nbox notacolor title\nparticipant B\nend\nbox  wrap: red\nparticipant C\nend\nbox CanvasText:nowrap:x\nparticipant D\nend",
    "sequenceDiagram\nbox rgb(1,2,3)\u{2028}x\nparticipant A\nend\nbox a\rb\nparticipant B\nend",
    "sequenceDiagram\nparticipant A\nlinks A: {\"Repo\": \"https://x\", \"2\": [1, {\"a\": null}], \"1\": 1e999}\nlinks A: \"str\u{1f600}\"\nlinks A: [true, 1.50]\nlinks A: 5\nlinks A: {bad\nlinks B: {\"x\": 1}",
    "sequenceDiagram\nparticipant A\nlink A: Docs @ https://x @ y\nlink A: nosep\nlink A: @start\nlink A: \nlink A: a@\nlink B: x @ y",
    "sequenceDiagram\nparticipant A\nproperties A: {\"class\": \"one two\", \"class\": \"three\"}\nproperties A: null\ndetails A: some-id\ndetails A: other",
    "sequenceDiagram\nlink->>links: x\nlink link: a @ b\nlinks A: {}\nproperties: x\ndetails->>A: y",
    "sequenceDiagram\ncreate participant C\nA->>C: x\ndestroy C\nC-xA: y\ncreate actor D as Dee\nA->>D: z",
    "sequenceDiagram\ncreate participant C\nA->>B: x",
    "sequenceDiagram\nA->>B: x\ncreate participant A",
    "sequenceDiagram\ndestroy A\nB->>C: x",
    "sequenceDiagram\ndestroy A\nNote over A: n\nA->>B: x\ncreate destroy E\nA->>E: y",
    "sequenceDiagram\ncreate participant C\ndestroy C\nA->>C: x\nA->>B: y",
    "sequenceDiagram\nA->>+B: x\nB-->>-A: y\nB-->>-A: z",
    "sequenceDiagram\nactivate A\nactivate A\ndeactivate A\ndeactivate A\ndeactivate A",
    "sequenceDiagram\nA()->>()B: both\nA()->>B: from\nA->>()B: to\nA()->>+B: plus\nA-->>-B: minus",
    "sequenceDiagram\nautonumber\nA->>B: x\nautonumber 10 5\nB->>A: y\nautonumber off\nA->>B: z\nautonumber 0 0\nA->>B: w\nautonumber .5 1.25\nA->>A: v",
    "sequenceDiagram\nautonumber 1.234\nA->>B: x",
    "sequenceDiagram\nautonumber off 1\nA->>B: x",
    "sequenceDiagram\nloop wrap: every minute\nA->>B: x\nend\nalt :nowrap:a\nA->>B: y\nelse\nB->>A: z\nelse wrap:\nB->>A: w\nend\npar_over p\nA->>B: v\nand q\nA->>B: u\nend\ncritical c\noption o\nend\nbreak b\nend\nopt\nend\nrect rgb(0, 0, 0)\nend",
    "sequenceDiagram\nloop a\nelse b\nend",
    "sequenceDiagram\nalt a\nand b\nend",
    "sequenceDiagram\nloop a\nA->>B: x",
    "sequenceDiagram\nend",
    "sequenceDiagram\nbox a\nA->>B: x\nend",
    "sequenceDiagram\nbox a\ntitle t\nend",
    "sequenceDiagram\nNote over A,B: wrap: spans\nNote left of A: :wrap:left\nNote right of B: right\nNote over A: one\nnote Over A , B : x",
    "sequenceDiagram\nNote A: x",
    "sequenceDiagram\nNote over A,B,C: x",
    "sequenceDiagram\ntitle The title\ntitle: Legacy\ntitle\ntitle:   \ntitle\tTabbed\naccTitle: Acc\naccDescr: Descr\naccDescr {\n  multi\n   line\n\n  text\n}\nA->>B: x",
    "sequenceDiagram\naccDescr { unclosed",
    "sequenceDiagram\naccTitle",
    "sequenceDiagram\naccTitle:",
    "sequenceDiagram\ntitle",
    "sequenceDiagram\nparticipant",
    "sequenceDiagram\nparticipant ",
    "sequenceDiagram\nparticipant A as",
    "sequenceDiagram\nparticipant A as ",
    "sequenceDiagram\nparticipant A@{",
    "sequenceDiagram\nparticipant <x>\nparticipant a:b\nparticipant a #c:d\nactor  spaced name  # comment\nparticipant x, y",
    "sequenceDiagram\nloop",
    "sequenceDiagram\nA->>B",
    "sequenceDiagram\nA->>B:",
    "sequenceDiagram\nA -->> B : spaced\nA-B->>C-D: dashes\nA - B: x\nend-user->>endpoint: keywords in names\nalternate->>parser: more",
    "sequenceDiagram\n1->>2: digits\n1 ->>2: split\n.5->>x: dot",
    "sequenceDiagram\nA->>B: semi; B->>A: colon # not a comment\n# a comment\n%% another\n% one\nA->>B: x %% kept",
    "sequenceDiagram\r\nparticipant A\r\nA->>B: x\r\nNote over A: n\r\n",
    "sequencediagram\nA->>B: lower",
    "SEQUENCEDIAGRAM\nA->>B: upper",
    "sequenceDiagram A->>B: same line",
    "sequenceDiagram\n\u{e9}->>\u{65e5}\u{672c}: \u{a0}x\u{2028}y\nparticipant \u{a0}pad\u{a0} as \u{3000}wide",
    "sequenceDiagram\nA->>B: #35;hash #quot;q#quot;\nparticipant C as #9829;",
    "%%{init: {\"wrap\": true}}%%\nsequenceDiagram\nA->>B: nowrap: x\nNote over A: n\nbox b\nparticipant C\nend",
    "%%{init: {\"sequence\": {\"wrap\": true}}}%%\nsequenceDiagram\nA->>B: x\nparticipant C as nowrap: c",
    "%%{init: {\"wrap\": false, \"sequence\": {\"wrap\": true}}}%%\nsequenceDiagram\nA->>B: x",
    "%%{wrap}%%\nsequenceDiagram\nA->>B: x",
    "---\ntitle: Front title\nconfig:\n  sequence:\n    wrap: true\n---\nsequenceDiagram\nA->>B: x",
    "---\ntitle: Front title\n---\nsequenceDiagram\ntitle Statement title\nA->>B: x",
];

#[test]
fn cases_match_pele() {
    let cases: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    let tally = run("case", &cases).finish("hand-written sequence diagrams");
    assert!(tally.identical > 30 && tally.rejected > 15, "the cases both parse and fail");
}

/// A menu's JSON nested deeper than this port reads is a limit error here, where Pele parses it:
/// the one known difference of the front end.
#[test]
fn json_past_the_nesting_limit_is_a_limit_error() {
    let source = format!("sequenceDiagram\nparticipant A\nlinks A: {}{}", "[".repeat(600), "]".repeat(600));
    let theirs = &pele_oracle::ask(&[request(&source)])[0];
    assert!(theirs.starts_with("{\"model\":"), "Pele parses it: {}", &theirs[..theirs.len().min(120)]);
    match pele_core::sequence_front_json(&source, ParseOptions::default()) {
        Err(RenderError::Diagnostic(diagnostic)) => assert_eq!(diagnostic.code, ErrorCode::Limit),
        other => panic!("expected a limit error, got {other:?}"),
    }
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

const ACTORS: &[&str] = &["A", "B", "C", "Alice", "Bob", "the-user", "a b", "\u{e9}", "A1", "link", "D"];
const ARROWS: &[&str] = &[
    "->>", "-->>", "->", "-->", "<<->>", "<<-->>", "-x", "--x", "-)", "--)", "-|\\", "-|/", "-\\\\", "-//", "--|\\",
    "--|/", "--\\\\", "--//", "/|-", "\\|-", "//-", "\\\\-", "/|--", "\\|--", "//--", "\\\\--",
];
const TEXTS: &[&str] =
    &["hi", "", "wrap: a long text", ":nowrap:short", "a # b", "<br/>two", "x, y", "#35;", "  padded  ", "Title Case"];
const CONFIGS: &[&str] = &[
    "@{ \"type\": \"queue\" }",
    "@{type: actor}",
    "@{ \"alias\": \"x\" }",
    "@{}",
    "@{ \"type\": \"database\", \"alias\": \"Store\" }",
    "@{ a: [ }",
    "@{ \"type\": 5 }",
    " @{ type: collections }",
];
const COLOURS: &[&str] = &[
    "Aqua",
    "rgb(0, 255, 0)",
    "rgba(0,0,0,0.1)",
    "transparent",
    "hsl(120, 50%, 50%)",
    "notacolour",
    "",
    "rgb(1 2 3 / 50%)",
    "rgb(1,2",
    "GREEN",
];

fn message(rng: &mut Rng) -> String {
    let (from, to) = (rng.pick(ACTORS), rng.pick(ACTORS));
    let (before, after) = match rng.below(8) {
        0 => ("", "+"),
        1 => ("", "-"),
        2 => ("()", "()"),
        3 => ("()", ""),
        4 => ("", "()"),
        _ => ("", ""),
    };
    let gap = rng.pick(&["", "", " "]);
    format!("{from}{gap}{before}{}{after}{gap}{to}: {}", rng.pick(ARROWS), rng.pick(TEXTS))
}

fn statement(rng: &mut Rng, depth: usize) -> String {
    match rng.below(24) {
        0..=6 => message(rng),
        7 | 8 => {
            let mut out = format!("{} {}", rng.pick(&["participant", "actor", "Participant"]), rng.pick(ACTORS));
            if rng.chance(4) {
                out += rng.pick(CONFIGS);
            }
            if rng.chance(3) {
                out += &format!(" as {}", rng.pick(TEXTS));
            }
            out
        }
        9 => {
            let actor = rng.pick(ACTORS);
            let kind = rng.pick(&["participant", "actor"]);
            let creating = if rng.chance(12) { message(rng) } else { format!("A->>{actor}: make") };
            format!("create {kind} {actor}\n{creating}")
        }
        10 => {
            let actor = rng.pick(ACTORS);
            let ending = if rng.chance(12) { message(rng) } else { format!("{actor}-xA: done") };
            format!("destroy {actor}\n{ending}")
        }
        11 | 12 => match rng.below(4) {
            0 => format!("Note over {},{}: {}", rng.pick(ACTORS), rng.pick(ACTORS), rng.pick(TEXTS)),
            1 => format!("Note over {}: {}", rng.pick(ACTORS), rng.pick(TEXTS)),
            2 => format!("note left of {}: {}", rng.pick(ACTORS), rng.pick(TEXTS)),
            _ => format!("Note right of {}: {}", rng.pick(ACTORS), rng.pick(TEXTS)),
        },
        13 | 14 if depth < 3 => {
            let (open, divider) = match rng.below(8) {
                0 => ("loop", ""),
                1 => ("opt", ""),
                2 => ("break", ""),
                3 => ("rect", ""),
                4 => ("alt", "else"),
                5 => ("par", "and"),
                6 => ("par_over", "and"),
                _ => ("critical", "option"),
            };
            let title = if open == "rect" { rng.pick(COLOURS) } else { rng.pick(TEXTS) };
            let mut out = format!("{open} {title}\n");
            for section in 0..1 + rng.below(3) {
                if section > 0 {
                    let divider = if rng.chance(8) { rng.pick(&["else", "and", "option"]) } else { divider };
                    if divider.is_empty() {
                        break;
                    }
                    out += &format!("{divider} {}\n", rng.pick(TEXTS));
                }
                for _ in 0..rng.below(3) {
                    out += &statement(rng, depth + 1);
                    out += "\n";
                }
            }
            out + if rng.chance(40) { "" } else { "end" }
        }
        15 => {
            let mut out =
                format!("box {} {}\n", rng.pick(COLOURS), rng.pick(&["", "Group", "wrap: A long group", "x y"]));
            for _ in 0..rng.below(3) {
                out += &format!("{} {}\n", rng.pick(&["participant", "actor"]), rng.pick(ACTORS));
            }
            out + "end"
        }
        16 => rng
            .pick(&[
                "autonumber",
                "autonumber 10",
                "autonumber 2 .5",
                "autonumber off",
                "autonumber 1 0",
                "autonumber 0",
            ])
            .to_owned(),
        17 => format!("{} {}", rng.pick(&["activate", "deactivate"]), rng.pick(ACTORS)),
        18 => match rng.below(4) {
            0 => {
                format!("links {}: {}", rng.pick(ACTORS), rng.pick(&["{\"a\": \"b\"}", "[1]", "\"s\"", "{bad", "null"]))
            }
            1 => format!("link {}: {}", rng.pick(ACTORS), rng.pick(&["Docs @ https://x", "none", "@", "a @ b @ c"])),
            2 => format!(
                "properties {}: {}",
                rng.pick(ACTORS),
                rng.pick(&["{\"class\": \"k m\"}", "{\"class\": 5}", "{", "{\"x\": {\"y\": [1]}}"])
            ),
            _ => format!("details {}: {}", rng.pick(ACTORS), rng.pick(TEXTS)),
        },
        19 => rng
            .pick(&[
                "title T",
                "title: Legacy",
                "accTitle: t",
                "accDescr: d",
                "accDescr {\n a\n  b\n}",
                "%% c",
                "# c",
                "title",
            ])
            .to_owned(),
        20 if rng.chance(4) => rng.pick(&["end", "else x", "and", "", ";", "A->>B", ">", "A B"]).to_owned(),
        _ => message(rng),
    }
}

/// What a model can hold, by a mark its JSON carries when it does.
const HELD: &[(&str, &str)] = &[
    ("a participant in a box", "{\"box\":"),
    ("a described participant", "\"description\":\"Title Case\""),
    ("a kind from a config", "\"type\":\"database\""),
    ("an alias from a config", "\"description\":\"Store\""),
    ("an actor", "\"type\":\"actor\"}"),
    ("links", "\"links\":[["),
    ("properties", "\"properties\":[["),
    ("details", "\"details\":"),
    ("a created participant", "\"createdActors\":[["),
    ("a destroyed participant", "\"destroyedActors\":[["),
    ("a box with a name", "\"boxes\":[{\"name\":"),
    ("a box with a colour", "\"fill\":\"rgb(0, 255, 0)\""),
    ("a wrapped message", "\"wrap\":true,\"type\":0,"),
    ("an activating message", "\"activate\":true,\"centralConnection\":0"),
    ("a central connection at the receiver", "\"centralConnection\":59"),
    ("a central connection at the sender", "\"centralConnection\":60"),
    ("central connections at both ends", "\"centralConnection\":61"),
    ("a deactivation", "\"type\":18,"),
    ("a note to one side", "\"placement\":0"),
    ("a note over a span", "\"placement\":2"),
    ("a numbering with a step", "\"step\":\"0.5\""),
    ("numbers switched off", "\"visible\":false"),
    ("a loop", "\"type\":10,"),
    ("an alt with an else", "\"type\":13,"),
    ("a par over", "\"type\":32,"),
    ("a critical option", "\"type\":28,"),
    ("a break", "\"type\":30,"),
    ("a rect", "\"type\":22,"),
    ("a half arrow", "\"type\":41,"),
    ("a reverse half arrow", "\"type\":58,"),
    ("a bidirectional arrow", "\"type\":34,"),
    ("a title", "\"title\":"),
    ("an accessible description", "\"accDescr\":"),
];

#[test]
fn random_sequence_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<String> = (0..4000)
        .map(|_| {
            let mut source = String::from("sequenceDiagram\n");
            for _ in 0..1 + rng.below(9) {
                source += rng.pick(&["", "  ", "\t"]);
                source += &statement(&mut rng, 0);
                source += rng.pick(&["\n", "\n", "\n\n", ";", " \n"]);
            }
            source
        })
        .collect();
    let tally = run("random", &cases).finish("random sequence diagrams");
    assert!(tally.identical > 1000, "many random sequence diagrams parse");
    assert!(tally.rejected > 500, "many random sequence diagrams are rejected");
    for kind in ["syntax, parser", "syntax, config", "semantic, model"] {
        assert!(tally.kinds.get(kind).is_some_and(|&count| count > 10), "random sources are rejected by: {kind}");
    }
    let mut missing = Vec::new();
    for (what, mark) in HELD {
        let count = tally.models.iter().filter(|model| model.contains(mark)).count();
        println!("  {count:5} models hold {what}");
        if count == 0 {
            missing.push(*what);
        }
    }
    assert!(missing.is_empty(), "no random model holds: {missing:?}");
    println!("  the random models hold {} of {} things a model can hold", HELD.len(), HELD.len());
}
