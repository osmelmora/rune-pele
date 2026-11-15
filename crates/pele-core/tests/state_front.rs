//! Differential test for the state diagram front end: for every source, this port's statement
//! tree, db and graph must come out as Pele 1.13.0's do, byte for byte in the JSON of the
//! oracle's `state_front` op, and a source Pele rejects must be rejected here with Pele's message,
//! code and position.
//!
//! `tests/fixtures/state/harvest.json` holds the state sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-state.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/state/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("state_front\t{source}")
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

fn run(name: &str, sources: &[String]) -> Tally {
    let requests: Vec<String> = sources.iter().map(|source| request(source)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tally = Tally::default();
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let ours = pele_core::state_front_json(source, ParseOptions::default());
        tally.compare(&format!("{name} {index}"), source, ours, theirs);
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested state sources");
    assert!(tally.identical > 1000, "the harvest holds state diagrams Pele parses");
    assert!(tally.rejected > 100, "the harvest holds state diagrams Pele rejects");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &harvest("mutated")).finish("mutated state sources");
}

/// Hand-written sources for the paths the harvest reaches least: regions, translated `[*]` ids,
/// id merges between notes and states, colour slots, class and style lookups, and error positions.
const CASES: &[&str] = &[
    "stateDiagram-v2\nstate A {\n a --> b\n --\n c --> d\n --\n e\n}\n",
    "stateDiagram-v2\nstate A {\n a\n --\n}\n",
    "stateDiagram-v2\nstate A {\n --\n --\n x\n}\n",
    "stateDiagram-v2\n[*] --> A\nstate A {\n [*] --> B\n B --> [*]\n}\nA --> [*]\n[*]\n",
    "stateDiagram-v2\nstate root {\n a --> b\n}\nroot --> root\n",
    "stateDiagram-v2\nstate A {\n state B {\n  x\n }\n}\nstate B {\n y\n}\n",
    "stateDiagram-v2\nstate a0 {\n state a1 {\n }\n}\nstate a1 {\n state a0 {\n }\n}\n",
    "stateDiagram-v2\nnote right of a : n1\nnote left of a : n2\na --> b\n",
    "stateDiagram-v2\na----note-0\nnote right of a : n\na----note-0 --> a\n",
    "stateDiagram-v2\nstate \"Long name\" as L {\n x\n}\nL : more\n",
    "stateDiagram-v2\nstate \"Desc\" as s:extra:more\ns : again\n",
    "stateDiagram-v2\na : one\na : two\na : three\nb : x\nb\n",
    "stateDiagram-v2\na\na : \nb : :x\n",
    "stateDiagram-v2\nclassDef hot fill:#f00,color:white;font-weight:bold\nclassDef hot stroke:blue\nclass a,b , c hot\na --> b\nclass a hot\n",
    "stateDiagram-v2\nstyle a,b fill:red;;,stroke:blue\na --> b:::x\nc:::y\n[*]:::z --> c\n",
    "stateDiagram-v2\nclick a \"https://example.com\" \"tip\"\nclick a href \"/x\"\nclick [*] \"u\" \"t\"\n",
    "stateDiagram-v2\ndirection LR\nstate A {\n direction BT\n direction RL\n x\n}\ndirection TB\n",
    "stateDiagram-v2\naccTitle:   Title  \naccDescr {\n  one\n    two\n\n three\n}\na\n",
    "stateDiagram-v2\nstate f <<fork>>\nstate j <<join>>\nstate c <<choice>>\nstate F [[fork]]\nf --> j\nj --> c\n",
    "stateDiagram-v2\nstate x <<fork>> y <<choice>>\n",
    "stateDiagram-v2\nnote \"floating\" as N\nnote left of a\n  line one\n  line two\nend note\n",
    "stateDiagram-v2\nhide empty description\nscale 350 width\nstate nl\naccTitle: nl\n",
    "stateDiagram-v2\nstate g {\n a\n}\ng : description\n",
    "stateDiagram-v2\nstate g {\n a\n}\nstate \"T\" as g:extra\n",
    "stateDiagram-v2\nstate two words {\n a\n}\n",
    "stateDiagram-v2\nstate A {\n a --> b\n}}\n",
    "stateDiagram-v2\nstate A {\n a --> b\n",
    "stateDiagram-v2\na -> b\n",
    "stateDiagram-v2\nstate A {\n a - b\n}\n",
    "stateDiagram-v2\nnote over a : x\n",
    "stateDiagram-v2\nnote right of a\n never ends\n",
    "stateDiagram-v2\nscale x width\n",
    "stateDiagram-v2\nclassDef default fill:red\n",
    "stateDiagram-v2\nclick a\n",
    "stateDiagram\n  [*] --> Still\n  Still --> [*]\n",
    "stateDiagram-v2\n  a%%b --> c\n  state \"x\" as %%y\n",
    "---\ntitle: Front #35; matter\n---\nstateDiagram-v2\na\n",
];

#[test]
fn cases_match_pele() {
    let cases: Vec<String> = CASES.iter().map(|&case| case.to_owned()).collect();
    run("case", &cases).finish("hand-written state diagrams");
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

const IDS: &[&str] = &["A", "B", "C", "S1", "S2", "x", "y", "Idle", "é", "a_b", "n1", "[*]", "[*]", "root", "g"];
const TEXTS: &[&str] =
    &["Moving", "a b", "**bold**", "#35; x", "&amp; y", "<b>tag</b>", "  spaced  ", "x:y", "multi word text", "1"];

fn id(rng: &mut Rng) -> String {
    let mut out = rng.pick(IDS).to_owned();
    if rng.chance(8) {
        out += &format!(":::{}", rng.pick(&["hot", "cold", "missing"]));
    }
    out
}

fn statement(rng: &mut Rng, depth: usize) -> String {
    match rng.below(18) {
        0..=4 => {
            let mut out = format!("{} --> {}", id(rng), id(rng));
            if rng.chance(3) {
                out += &format!(" : {}", rng.pick(TEXTS));
            }
            out
        }
        5 => format!("{} : {}", rng.pick(IDS), rng.pick(TEXTS)),
        6 if depth < 3 => {
            let header = match rng.below(3) {
                0 => format!("state {}", rng.pick(&["G1", "G2", "A", "x", "root"])),
                1 => format!("state \"{}\" as {}", rng.pick(TEXTS), rng.pick(&["G1", "G3", "A"])),
                _ => format!("state {}", rng.pick(&["G2", "B"])),
            };
            let mut out = format!("{header} {{\n");
            if rng.chance(3) {
                out += &format!("direction {}\n", rng.pick(&["TB", "LR", "RL", "BT"]));
            }
            for _ in 0..1 + rng.below(4) {
                out += &statement(rng, depth + 1);
                out += "\n";
                if rng.chance(5) {
                    out += "--\n";
                }
            }
            out + "}"
        }
        7 => format!(
            "state {} {}",
            rng.pick(&["f1", "j1", "c1", "A"]),
            rng.pick(&["<<fork>>", "<<join>>", "<<choice>>", "[[fork]]", "[[choice]]"])
        ),
        8 => match rng.below(3) {
            0 => format!("note {} of {} : {}", rng.pick(&["left", "right"]), rng.pick(IDS), rng.pick(TEXTS)),
            1 => format!(
                "note {} of {}\n  {}\n  {}\nend note",
                rng.pick(&["left", "right"]),
                rng.pick(IDS),
                rng.pick(TEXTS),
                rng.pick(TEXTS)
            ),
            _ => format!("note \"{}\" as N{}", rng.pick(TEXTS), rng.below(3)),
        },
        9 => format!(
            "classDef {} {}",
            rng.pick(&["hot", "cold"]),
            rng.pick(&["fill:#f00,color:white", "stroke:blue,stroke-width:2px", "font-weight:bold;", "fill:yellow"])
        ),
        10 => format!("class {} {}", rng.pick(&["A", "A,B", "x , y", "G1"]), rng.pick(&["hot", "cold"])),
        11 => format!(
            "style {} {}",
            rng.pick(&["A", "A,B", "G1"]),
            rng.pick(&["fill:red", "stroke:blue;,color:green", "font-size:20px"])
        ),
        12 => match rng.below(2) {
            0 => format!(
                "click {} \"{}\" \"{}\"",
                rng.pick(IDS),
                rng.pick(&["https://example.com", "javascript:x", "/a"]),
                rng.pick(TEXTS)
            ),
            _ => format!("click {} href \"{}\"", rng.pick(IDS), rng.pick(&["https://example.com", "#x"])),
        },
        13 => format!("state \"{}\" as {}", rng.pick(TEXTS), rng.pick(&["A", "s:extra", "B"])),
        14 => {
            rng.pick(&["accTitle: The title", "accDescr: A description", "accDescr {\n  many\n  lines\n}"]).to_owned()
        }
        15 => format!("direction {}", rng.pick(&["TB", "LR", "RL", "BT", "td"])),
        16 => rng
            .pick(&["%% a comment", "", "hide empty description", "scale 300 width", "state Lone", "# hash"])
            .to_owned(),
        _ => rng
            .pick(&["A -> B", "state two words {", "}", "note over A : x", "classDef default fill:red", "--"])
            .to_owned(),
    }
}

fn random_state(rng: &mut Rng) -> String {
    let mut out = String::new();
    if rng.chance(8) {
        out += &format!("---\ntitle: {}\n---\n", rng.pick(&["Front", "A #35; b"]));
    }
    out += rng.pick(&["stateDiagram-v2\n", "stateDiagram\n", "stateDiagram-v2\n\n"]);
    for _ in 0..1 + rng.below(10) {
        out += rng.pick(&["", "  ", "\t"]);
        out += &statement(rng, 0);
        out += rng.pick(&["\n", "\n", "\n\n"]);
    }
    out
}

#[test]
fn random_state_diagrams_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<String> = (0..3000).map(|_| random_state(&mut rng)).collect();
    let tally = run("random", &cases).finish("random state diagrams");
    assert!(tally.identical > 500, "many random state diagrams parse");
    assert!(tally.rejected > 300, "many random state diagrams are rejected");
}
