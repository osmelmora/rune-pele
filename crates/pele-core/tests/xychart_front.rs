//! Differential test for the XY chart front end: for every source, this port's db must come out as
//! Pele 1.13.0's does, byte for byte in the JSON of the oracle's `xychart_front` op, and a source
//! Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/xychart/harvest.json` holds the XY chart sources of Pele's own test suite, the
//! corpus of its XY chart parser test, its hostile templates, its worst cases and the inputs of its
//! seeded mutation runs, written by `scripts/harvest-xychart.mjs`.

mod xychart_random;

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use xychart_random::Rng;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/xychart/harvest.json");
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
    format!("xychart_front\t{source}")
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
    /// Rejections by what rejected the source: the lexer, the parser, the model, a limit, or the detection of no diagram. Pele's
    /// XY chart parser rejects nothing that lexes and parses, so the model's count stays 0.
    kinds: [usize; 5],
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
                        ErrorCode::Limit => 3,
                        _ => 4,
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
            "{what}: {total} inputs, {} identical to Pele, {} rejected with Pele's error ({} lexer, {} parser, {} model, {} limit, {} undetected), {} of another type, {} different",
            self.identical,
            self.rejected,
            self.kinds[0],
            self.kinds[1],
            self.kinds[2],
            self.kinds[3],
            self.kinds[4],
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
        match std::panic::catch_unwind(|| pele_core::xychart_front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("sources")).finish("harvested XY chart sources");
    assert!(tally.identical > 800, "the harvest holds XY charts Pele parses");
    assert!(tally.kinds[0] > 3 && tally.kinds[1] > 3, "the harvest is rejected by the lexer and the parser");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("mutated")).finish("mutated XY chart sources");
    assert!(tally.rejected > 1000, "the mutations hold XY charts Pele rejects");
}

/// Pele's worst cases, which its stress test parses with no limit on a source's length. The oracle
/// parses with the default limit, so the longest are rejected by both with the limit's message.
#[test]
fn worst_cases_match_pele() {
    run("worst", &harvest("worst")).finish("worst XY chart sources");
}

/// Hand-written sources for the lexer's states and the db's order of statements.
fn cases() -> Vec<String> {
    [
        "xychart",
        "xychart-beta horizontal",
        "xychart HORIZONTAL\nline [1]",
        "xychart vertical\nxychart horizontal\nline [1]",
        "xychart\n\n;;\nxychart-beta\nline [1]",
        "xychart\nline [1, 2, 3]\nx-axis [a, b]\nbar [4, 5, 6]",
        "xychart\nx-axis [a, b]\nline [1, 2, 3, 4]\nline [1]",
        "xychart\ny-axis 10 --> 0\nline [5, 50]\ny-axis title only\nbar [1]",
        "xychart\nline [5]\nx-axis 3 --> 3\nline [1, 2, 3]",
        "xychart\nx-axis \"t\" -1.5 --> +.5\ny-axis -.25 --> 4.",
        "xychart\nx-axis\nline [1]",
        "xychart\nx-axis 1 -> 2\nline [1]",
        "xychart\nline a b c [1 \"x\", 2 \"\", 3 \" spaced \"]\nbar d#e [1 \"ignored\"]",
        "xychart\nline [1 \"open",
        "xychart\ntitle \"never closed",
        "xychart\naccDescr {\n  never closed",
        "xychart\naccTitle\nline [1]",
        "xychart\naccTitle :  spaced  \naccDescr:x\naccDescr { a\n   b\n  c }\nline [1]",
        "xychart\ntitle a-b_c+d=e*f.g&h#i\nline [1]",
        "xychart\ntitle\nline [1]",
        "xychart\nline [1]  \n  bar [2]",
        "xychart\nx-axis [a, b] \n line [1, 2]",
        "xychart\nx-axis a\r\nline [1]\r\n",
        "xychart\nline [1]\rbar [2]",
        "xychart\nx-axis [`)]\nline [1]",
        "xychart\nline [1, 2] %% comment\n%% another\nbar [3]",
        "xychart\nline [1,]",
        "xychart\nline []",
        "xychart\nline [1 2]",
        "xychart\nbar [-0]",
        "xychart\ny-axis 99999999999999999999999 --> -99999999999999999999999\nline [1]",
        "xychart\nx-axis [é]\nline [1]",
        "xychart\n?",
        "xychart\nline [1] title [2]",
        "---\ntitle: Front\n---\nxychart\nline [1]",
        "---\ntitle: Front\n---\nxychart\ntitle Own\nline [1]",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("hand-written XY chart sources");
}

#[test]
fn random_xy_charts_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let sources: Vec<String> = (0..4000).map(|_| xychart_random::source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random XY charts");
    assert!(tally.identical > 3000, "most random charts parse");
    assert!(tally.rejected > 50, "some random charts are rejected");
}
