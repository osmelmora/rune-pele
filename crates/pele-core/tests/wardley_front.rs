//! Differential test for the wardley front end: for every source, this port's model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `wardley_front` op, and a
//! source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/wardley/harvest.json` holds the wardley sources of Pele's own test suite, the
//! corpus of its wardley parser test, its hostile templates, its worst cases and the inputs of its
//! seeded mutation runs, written by `scripts/harvest-wardley.mjs`.

mod wardley_random;

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use wardley_random::Rng;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/wardley/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list]
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().or_else(|| item["source"].as_str()).expect("a source").to_owned())
        .collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("wardley_front\t{source}")
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
    /// Rejections by what rejected the source: the lexer, the parser, the model, or a limit.
    kinds: [usize; 4],
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
                    let kind = match diagnostic.code {
                        ErrorCode::Syntax if diagnostic.message.starts_with("Lexical") => 0,
                        ErrorCode::Syntax => 1,
                        ErrorCode::Semantic => 2,
                        _ => 3,
                    };
                    self.kinds[kind] += 1;
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Err(error) => Some(format!("ours fails with {error}, pele answers {}", &theirs[..theirs.len().min(200)])),
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {:?}", &source[..source.len().min(600)]));
        }
    }

    fn finish(self, what: &str) -> Self {
        let total = self.identical + self.rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {} rejected with Pele's error ({} lexer, {} parser, {} model, {} limit), {} of another type, {} different",
            self.identical,
            self.rejected,
            self.kinds[0],
            self.kinds[1],
            self.kinds[2],
            self.kinds[3],
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
        match std::panic::catch_unwind(|| pele_core::wardley_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested wardley sources");
    assert!(tally.identical > 500, "the harvest holds wardley maps Pele parses");
    assert!(tally.kinds[..3].iter().all(|&count| count > 3), "the harvest is rejected by the lexer, parser and model");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated wardley sources");
    assert!(tally.rejected > 1000, "the mutations hold wardley maps Pele rejects");
}

/// Pele's worst cases, which its stress test parses with no limit on a source's length. The oracle
/// parses with the default limit, so the longest are rejected by both with the limit's message.
#[test]
fn worst_cases_match_pele() {
    run("worst", &harvest("worst")).finish("worst wardley sources");
}

/// Hand-written sources for what the harvest reaches least.
fn cases() -> Vec<String> {
    [
        "wardley-beta",
        "wardley-beta\ncomponent A [0.5, 0.5]\ncomponent A [0.2, 0.3] label [-0, 4]\nanchor A [0.1, 0.9]",
        "wardley-beta\ncomponent A [0.5, 0.5] label [1, 2] (buy) inertia\ncomponent A [0.4, 0.4]\npipeline A {\n component A [0.5]\n}\ncomponent A_A [0.1, 0.1] label [3, 3]",
        "wardley-beta\ncomponent P [0.5, 0.5]\npipeline P {\n component X [0.2]\n component X [0.3]\n}\npipeline P {\n component Y [0.9]\n}\nX -> P\nY -> P_X",
        "wardley-beta\ncomponent P [0.5, 0.5]\ncomponent Q [0.2, 0.2]\npipeline P {\n component X [0.2]\n}\npipeline Q {\n component X [0.8]\n}\nX -> Q\nQ -> X\nX -> P_X",
        "wardley-beta\nanchor A [0.5, 0.5]\npipeline A {\n component B [0.5]\n}\nevolve A 0.9\nevolve A 0.8\nevolve Missing 0.5",
        "wardley-beta\ncomponent A [1.0, 1.0]\ncomponent B [100.0, 0.0]\ncomponent C [1.5, 99.9]\ncomponent D [0.0, 100.0]",
        "wardley-beta\ncomponent A [100.1, 0.5]",
        "wardley-beta\nanchor A [0.5, 100.5]",
        "wardley-beta\nnote \"n\" [-1, 0.5]",
        "wardley-beta\nnote \"n\" [0.5, 101.0]",
        "wardley-beta\npipeline Nope {\n component A [0.5]\n}",
        "wardley-beta\ncomponent A [0.5, 0.5]\npipeline A {\n component B [150.0]\n}",
        "wardley-beta\ncomponent A [0.5, 0.5]\nevolve A 1.5\nevolve A 200.0",
        "wardley-beta\nannotations [150, 0.5]",
        "wardley-beta\nannotations [0.5, 150]",
        "wardley-beta\nannotation 3,[150, 0.5] \"x\"",
        "wardley-beta\nannotation 99999999999999999999999,[0.5, 0.5] \"x\"",
        "wardley-beta\naccelerator A [150.0, 0.5]",
        "wardley-beta\ndeaccelerator \"B c\" [0.5, 150.0]",
        "wardley-beta\nevolution A -> B\nevolution C@0.5 / \"\" -> D@1.0 / E -> F",
        "wardley-beta\nevolution \"  spaced  \"@0.2 / \"  second  \" -> B",
        "wardley-beta\nA +'a<>b'> B\nA +'x'< B\nA +'.-.'> B\nA -.-> B +<\nA +<> -> B +>\nA +> B +<",
        "wardley-beta\ncomponent Tea Cup [0.5, 0.5]\nTea Cup -> Tea Cup\n\"Tea Cup\" -> Missing; label  \nA -> B;x",
        "wardley-beta\ntitle\naccTitle:\naccDescr:\n",
        "---\ntitle: Front\n---\nwardley-beta\ntitle\n",
        "---\ntitle: Front\n---\nwardley-beta\ntitle Own\naccTitle:   lead\naccDescr {\n  first\n    second\n}",
        "wardley-beta\nsize [0, 99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999]",
        "wardley-beta\ncomponent labelled thing [0.1, 0.2]",
        "wardley-beta\ncomponent A [0.5, 0.5] (inertia) (buy)",
        "wardley-beta\ncomponent A [0.5, 0.5] (build)(inertia)",
        "wardley-beta\ncomponent A [0.5, 0.5] label[3,4]",
        "wardley-beta\npipeline A {component X [0.5]}",
        "wardley-beta\ncomponent A [0.5, 0.5]\npipeline A {\n\n\n component X [0.5]\n\n}\n",
        "wardley-beta\nannotation 1,[1, 0] 'q'\nannotation 2,[0.5, 50] \"r\"\nannotations [1, 99]\nannotations [0.2, 0.2]",
        "wardley-beta\n12. 3",
        "wardley-beta\n0.5.5",
        "wardley-beta\n-->",
        "wardley-beta\nWARDLEY-BETA",
        "WARDLEY-BETA\ncomponent A [0.5, 0.5]",
        "wardley-beta\ncomponent A [0.5, 0.5]\ncomponents B [0.5, 0.5]",
        "wardley-beta\ntitles x",
        "wardley-beta\naccTitlex: y",
        "wardley-beta\r\ncomponent A [0.5, 0.5]\r\nA -> B\r\n",
        "wardley-beta\ncomponent \"#35; x &amp; y\" [0.5, 0.5]\n\"#35; x &amp; y\" -> B",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("hand-written wardley sources");
}

#[test]
fn random_wardley_maps_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let sources: Vec<String> = (0..4000).map(|_| wardley_random::source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random wardley maps");
    assert!(tally.identical > 2000, "most random maps parse");
    assert!(tally.rejected > 50, "some random maps are rejected");
}
