//! Differential test for the use case front end: for every source, this port's model and graph
//! must come out as Pele 1.13.0's do, byte for byte in the JSON of the oracle's `usecase_front`
//! op, and a source Pele rejects must be rejected here with Pele's code, message and position.
//!
//! `tests/fixtures/usecase/harvest.json` holds the use case sources of Pele's own test suite and
//! the inputs of its seeded mutation runs, written by `scripts/harvest-usecase.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/usecase/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("usecase_front\t{source}")
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
    /// Rejections by Pele's error code and the first words of its message.
    kinds: std::collections::BTreeMap<String, usize>,
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

/// The four kinds of error a use case source can end in, told apart by how Pele words them.
fn error_kind(code: &str, message: &str) -> String {
    let kind = if message.starts_with("PeleError: Error lexing") {
        "lexer"
    } else if message.starts_with("PeleError: Error parsing") {
        "parser"
    } else if message.starts_with("PeleError: Invalid JSON") {
        "json"
    } else if code == "semantic" {
        "model"
    } else {
        "before the parser"
    };
    format!("{code}, {kind}")
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
                    let kind = error_kind(code_name(diagnostic.code), ours["error"].as_str().expect("a message"));
                    *self.kinds.entry(kind).or_default() += 1;
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
            "{what}: {total} inputs, {} identical to Pele, {} rejected with Pele's error {:?}, {} of another type, {} different",
            self.identical,
            self.rejected,
            self.kinds,
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

    fn rejected_as(&self, kind: &str) -> usize {
        self.kinds.get(kind).copied().unwrap_or(0)
    }
}

fn run(name: &str, sources: &[String]) -> Tally {
    let requests: Vec<String> = sources.iter().map(|source| request(source)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tally = Tally::default();
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let ours = pele_core::usecase_front_json(source, ParseOptions::default());
        tally.compare(&format!("{name} {index}"), source, ours, theirs);
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested use case sources");
    assert!(tally.identical > 1000, "the harvest holds use case diagrams Pele parses");
    for kind in ["syntax, lexer", "syntax, parser", "syntax, json", "semantic, model"] {
        assert!(tally.rejected_as(kind) > 10, "the harvest holds sources Pele rejects as {kind}: {:?}", tally.kinds);
    }
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated use case sources");
    assert!(tally.identical > 500, "a fair share of the mutated inputs gets past the parser");
}

/// Hand-written sources for the paths the harvest reaches least: deep and repeated JSON, error
/// positions behind entity codes, front matter and line breaks, and ids that collide.
fn cases() -> Vec<String> {
    let mut cases: Vec<String> = [
        "usecase-beta\njson P@{\"a\":1,\"a\":{\"b\":2},\"0\":[],\"k/~\":{\"x\":[{\"y\":{}}]}}\n",
        "usecase-beta\njson P@{\"a\":[1,{\"0\":1}],\"a\":{\"0\":{\"y\":1}}}\n",
        "usecase-beta\njson P@{\"n\":[1e21,-0,1.50,1E-7,123456789012345678901234567890,0.1e1],\"s\":\"\\u00e9\\ud83d\\ude00\\n\"}\n",
        "usecase-beta\njson P@{\"position 3\": x}\n",
        "usecase-beta\njson P@{\"at line 2 column 4\": ?}\n",
        "usecase-beta\n  json P@{\"Unexpected END\": ?}\n",
        "usecase-beta\njson P@{\"a\": \"#35;\", \"b\": tru }\n",
        "usecase-beta\njson P@{\"a\":\n\n  \"b\\x\"}\n",
        "usecase-beta\njson P@{\"a\":\r\n [1,\r 2,]}\n",
        "usecase-beta\njson P@{\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\": é}\n",
        "usecase-beta\njson P@{\"a\": 😀, \"bbbbbbbbbbbbbbbbbbbbbbbbbb\": 1}\n",
        "usecase-beta\njson P@{\"a\": 01}\njson Q@{\"a\": -}\njson R@{\"a\": 1.}\n",
        "usecase-beta\njson P@{\"a\": 1e}\n",
        "usecase-beta\njson P@{\"a\": \"\\u12\"}\n",
        "usecase-beta\njson P@{\"a\": \"\t\"}\n",
        "usecase-beta\njson P@{\"a\" 1}\n",
        "usecase-beta\njson P@{,}\n",
        "usecase-beta\njson P@{\"a\":1,}\n",
        "usecase-beta\njson P@{\"a\":[1 2]}\n",
        "usecase-beta\njson P@{\"a\":nul}\n",
        "usecase-beta\njson P@{\"a\":\"}\"}\njson Q@{\"a\":\"\\\"}\"}\n",
        "---\ntitle: T #35; t\n---\nusecase-beta\nactor A\nA --> B\nB@{ nope: 1 }\n",
        "---\ntitle: \"\"\n---\nusecase-beta\nA\n",
        "%%{init: {\"usecase\": {\"nodeSpacing\": 10}}}%%\nusecase-beta\n%% c\nA --> B\nclass B\n",
        "usecase-beta\r\nactor A\r\nA --> B\rB --> [x]\n",
        "usecase-beta\nactor A#35;B\nA --> \"x\n",
        "usecase-beta\nA <<\n",
        "usecase-beta\nA <<s>> --> B\nA <<t>>\n",
        "usecase-beta\n\"a b\" --> C\n\"a-b\"\n",
        "usecase-beta\nsystemBoundary \"\"\n  A\nend\nsystemBoundary S\n  A\nend\nsystemBoundary T\n  A\nend\n",
        // A boundary whose title makes an empty id is no parent, but it takes the place of one.
        "usecase-beta\nA\nsystemBoundary \"\"\n  A\nend\nsystemBoundary S\n  A\nend\n",
        "usecase-beta\nsystemBoundary S\n  A\nend\nsystemBoundary \"\"\n  A\nend\nsystemBoundary S\n  A\n  B\nend\n",
        "usecase-beta\nactor A\nsystemBoundary \" \"\n  actor A\nend\nsystemBoundary \"!\"\n  actor A\nend\nsystemBoundary T\n  actor A\nend\n",
        "usecase-beta\nactor A\nA e@--> B\nA edge-0@--> B\nB --> A\nstyle e stroke:red\nclass e,A c\ne@{ animation: slow }\n",
        "usecase-beta\nactor A@{ icon: \"\" }\nA@{ type: hollow, business: true }\n",
        "usecase-beta\nactor A@{ icon: \"x\" }\nA@{ type: awesome }\n",
        "usecase-beta\nA[A]@{ business: true }\n",
        "usecase-beta\nnote for A \"n\"\n",
        "usecase-beta\nsystemBoundary S\nend\nnote for S \"n\"\n",
        "usecase-beta\nactor A\nactor B\nA ..> : include B\n",
        "usecase-beta\nactor A\nA --|> B\n",
        "usecase-beta\njson P@{}\nA --o P\n",
        "usecase-beta\nA --> B:::c\n",
        "usecase-beta\nX@{ type: package }\n",
        "usecase-beta\nX@{ icon: \"i\", business: true }\n",
        "usecase-beta\nclassDef default fill:#eee, stroke : #333 ,x\nclassDef c stroke:red,fill:blue\nA:::c\nactor B\n",
        "usecase-beta\naccDescr {\n  one\n     two\n\n  three\n}\naccTitle:   T  \nA\n",
        "usecase-beta x\n",
        "usecase-beta\nrectangle R\n",
        "usecase-beta\nPackage(x)\n",
        "usecase-beta",
    ]
    .iter()
    .map(|&case| case.to_owned())
    .collect();
    // The deepest nesting the source limit of 50,000 characters leaves room for.
    for depth in [3000, 7000] {
        cases.push(format!("usecase-beta\njson P@{}1{}\n", "{\"a\":".repeat(depth), "}".repeat(depth)));
        cases.push(format!("usecase-beta\njson P@{}x{}\n", "{\"a\":".repeat(depth), "}".repeat(depth)));
    }
    for depth in [3000, 24_000] {
        cases.push(format!("usecase-beta\njson P@{{\"a\":{}{}}}\n", "[".repeat(depth), "]".repeat(depth)));
        cases.push(format!("usecase-beta\njson P@{{\"a\":{},{}}}\n", "[".repeat(depth), "]".repeat(depth)));
    }
    cases
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("hand-written use case sources");
    assert_eq!(tally.rejected_as("limit, before the parser"), 0, "every case reaches the parser: {:?}", tally.kinds);
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

    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

/// Whole statements over a handful of ids, which mostly get through the grammar and then agree
/// or clash in every way the model builder has to resolve. The plan is that of Pele's own
/// 'agrees on random diagrams built from statements'.
fn random_diagram(rng: &mut Rng) -> String {
    fn id(rng: &mut Rng) -> &'static str {
        rng.pick(&["A", "B", "C", "D", "e", "f", "S", "P", "A_b", "__proto__"])
    }
    fn text(rng: &mut Rng) -> &'static str {
        rng.pick(&["A", "A b", "B", "x y", "A-b", "", "é #35; <b>"])
    }
    fn arrow(rng: &mut Rng) -> &'static str {
        rng.pick(&[
            "-->",
            "<--",
            "--",
            "--o",
            "o--",
            "--x",
            "x--",
            "--->",
            "<---",
            "---",
            "--|>",
            "..> : include",
            "..> : extend",
            "-- l -->",
            "-- \"l\" --",
            "<-- l --",
            "o-- l --",
            "x-- l --",
            "-- l --o",
            "-- l --x",
            "-- \"`l`\" ---->",
            "-- \"\" -->",
            "<--- l --",
            "--- l -->",
        ])
    }
    fn meta(rng: &mut Rng) -> &'static str {
        rng.pick(&[
            "type: hollow",
            "type: normal",
            "type: awesome",
            "type: package",
            "type: rect",
            "icon: \"i\"",
            "business: true",
            "business: false",
            "animate: true",
            "animate: false",
            "animation: fast",
            "animation: slow",
            "animation: x",
            "k: v",
            "\"type\": \"hollow\"",
        ])
    }
    fn suffix(rng: &mut Rng) -> String {
        match rng.below(10) {
            0..=2 => String::new(),
            3 => format!("@{{ {} }}", meta(rng)),
            4 => format!("@{{ {}, {} }}", meta(rng), meta(rng)),
            5 => " <<S>>".to_owned(),
            6 => " <<T>>".to_owned(),
            7 => ":::c".to_owned(),
            8 => ":::c,d".to_owned(),
            _ => format!("@{{ {} }} <<S>>:::c", meta(rng)),
        }
    }
    // A label is often the id itself, which is what lets two declarations of one element agree.
    fn named(rng: &mut Rng, open: &str, close: &str) -> String {
        let name = id(rng);
        let label = match rng.below(4) {
            0 | 1 => name.to_owned(),
            2 => match text(rng) {
                "" => "l".to_owned(),
                text => text.to_owned(),
            },
            _ => format!("\"{name}\""),
        };
        format!("{name}{open}{label}{close}")
    }
    fn element(rng: &mut Rng) -> String {
        let name = match rng.below(6) {
            0 | 1 => id(rng).to_owned(),
            2 => named(rng, "(", ")"),
            3 => named(rng, "[", "]"),
            4 => format!("\"{}\"", text(rng)),
            _ => format!("\"`{}`\"", text(rng)),
        };
        name + &suffix(rng)
    }
    fn actor(rng: &mut Rng) -> String {
        let name = match rng.below(4) {
            0 => id(rng).to_owned(),
            1 => named(rng, "(", ")"),
            2 => named(rng, "[", "]"),
            _ => format!("\"{}\"", text(rng)),
        };
        name + &suffix(rng)
    }
    fn statement(rng: &mut Rng) -> String {
        match rng.below(19) {
            0 => format!("actor {}", actor(rng)),
            1 => format!("actor {}, {}", actor(rng), actor(rng)),
            2 => format!("actor {} {} {}", actor(rng), arrow(rng), element(rng)),
            3 => element(rng),
            4 => format!("{} {} {}", element(rng), arrow(rng), element(rng)),
            5 => format!("{} {}@{} {}", element(rng), id(rng), arrow(rng), element(rng)),
            6 => {
                let name = match rng.below(3) {
                    0 => id(rng).to_owned(),
                    1 => named(rng, "[", "]"),
                    _ => format!("\"{}\"", text(rng)),
                };
                let tail = match rng.below(4) {
                    0 | 1 => String::new(),
                    2 => format!("@{{ {} }}", meta(rng)),
                    _ => ":::c".to_owned(),
                };
                format!("systemBoundary {name}{tail}\n  {}\n  actor {}\nend", element(rng), actor(rng))
            }
            7 if rng.below(3) == 0 => format!("systemBoundary \"\"\n  {}\n  actor {}\nend", id(rng), id(rng)),
            7 => format!("systemBoundary {}\n  {}\nend", id(rng), element(rng)),
            8 => {
                let target = if rng.below(2) == 0 { id(rng).to_owned() } else { format!("\"{}\"", text(rng)) };
                format!("{target}@{{ {} }}", meta(rng))
            }
            9 => format!("{}@{{ {}, {} }}", id(rng), meta(rng), meta(rng)),
            10 => {
                // A metadata block with every mix of commas and line breaks between and after its properties.
                const GAPS: [&str; 13] =
                    ["", " ", ",", "\n", ",\n", "\n,", "\n\n", ",\n\n", "\n,\n", "\n\n,", "\n\n\n", ", ", ",,"];
                let opener = match rng.below(3) {
                    0 => id(rng).to_owned(),
                    1 => format!("actor {}", id(rng)),
                    _ => format!("systemBoundary {}", id(rng)),
                };
                let mut block = format!("{opener}@{{{}", rng.pick(&["", " ", "\n", "\n\n"]));
                for _ in 0..rng.below(4) {
                    block += meta(rng);
                    block += rng.pick(&GAPS);
                }
                if rng.below(5) == 0 {
                    block += rng.pick(&GAPS);
                }
                block + "}" + if opener.starts_with("system") { "\nend" } else { "" }
            }
            11 => format!("note for {} {}", id(rng), rng.pick(&["\"n\"", "n o", "\"`n`\""])),
            12 => format!(
                "json {}@{}{}",
                id(rng),
                rng.pick(&[
                    "{}",
                    "{\"a\":1}",
                    "{\"b\":{\"c\":[1,2]},\"a\":null}",
                    "{\"a\":}",
                    "[1]",
                    "{\"a\":1,\"a\":{\"b\":2}}",
                    "{\"2\":[true,[],{}],\"1\":-1.5e3,\"\":\"#35;\"}",
                    "{\"a\":[1,]}",
                    "{\"a\":\"\\q\"}",
                ]),
                rng.pick(&["", ":::c"])
            ),
            13 => format!("class {}{} c{}", id(rng), ["", ",B"][rng.below(2)], rng.pick(&["", ",d"])),
            14 => format!(
                "style {} {}",
                id(rng),
                rng.pick(&["fill:red,stroke-width:2px", "stroke-dasharray:5\\,5,border:1px solid red"])
            ),
            15 => format!("classDef {} fill:#eee,stroke:#333", rng.pick(&["c", "d", "default", "c,d"])),
            16 => format!("direction {}", rng.pick(&["TB", "TD", "BT", "LR", "RL"])),
            17 => rng.pick(&["accTitle: T", "accDescr: D", "accDescr {\n a\n  b\n}", "%% c", ""]).to_owned(),
            _ => format!("{} {} {}", id(rng), arrow(rng), id(rng)),
        }
    }
    let mut source = "usecase-beta\n".to_owned();
    for _ in 0..1 + rng.below(7) {
        source += &statement(rng);
        source.push('\n');
    }
    source
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x5eed_0fca_5e11);
    let sources: Vec<String> = (0..6000).map(|_| random_diagram(&mut rng)).collect();
    let tally = run("random", &sources).finish("random use case diagrams");
    assert!(tally.identical > 300, "a fair share of the random diagrams is accepted");
    assert!(tally.rejected_as("semantic, model") > 1000, "the random diagrams clash in the model: {:?}", tally.kinds);
    assert!(tally.rejected_as("syntax, json") > 50, "the random diagrams hold invalid JSON: {:?}", tally.kinds);
}
