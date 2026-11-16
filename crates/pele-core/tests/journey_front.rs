//! Differential test for the journey front end: for every source, this port's model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `journey_front` op, scores as
//! the bits of their doubles, and a source Pele rejects must be rejected here with Pele's message
//! and position.
//!
//! `tests/fixtures/journey/harvest.json` holds the journey sources of Pele's own test suite and
//! the inputs of its mutated runs, written by `scripts/harvest-journey.mjs`.

#[path = "differential/harness.rs"]
mod harness;

use harness::{Feature, FrontTally, Rng, harvest, run_front};
use serde_json::Value;

fn tasks(model: &Value) -> &[Value] {
    model["tasks"].as_array().map_or(&[], Vec::as_slice)
}

fn score(task: &Value) -> f64 {
    task["score"].as_str().and_then(|bits| u64::from_str_radix(bits, 16).ok()).map_or(f64::NAN, f64::from_bits)
}

const FEATURES: &[Feature] = &[
    ("a title", |m| m["title"].is_string()),
    ("an accessible title", |m| m["accTitle"].is_string()),
    ("an accessible description over lines", |m| m["accDescr"].as_str().is_some_and(|d| d.contains('\n'))),
    ("sections", |m| m["sections"].as_array().is_some_and(|s| s.len() > 1)),
    ("a task before the first section", |m| {
        m["sections"].as_array().is_some_and(|s| !s.is_empty()) && tasks(m).iter().any(|t| t["section"] == "")
    }),
    ("a score that is no number", |m| tasks(m).iter().any(|t| score(t).is_nan())),
    ("a score above five", |m| tasks(m).iter().any(|t| score(t) > 5.0)),
    ("a score below one", |m| tasks(m).iter().any(|t| score(t) < 1.0)),
    ("a fractional score", |m| tasks(m).iter().any(|t| score(t).fract() != 0.0 && score(t).is_finite())),
    ("a task with no actors", |m| tasks(m).iter().any(|t| t["people"].as_array().is_some_and(Vec::is_empty))),
    ("a task with several actors", |m| tasks(m).iter().any(|t| t["people"].as_array().is_some_and(|p| p.len() > 2))),
    ("an actor with no name", |m| {
        tasks(m).iter().any(|t| t["people"].as_array().is_some_and(|p| p.iter().any(|p| p == "")))
    }),
];

fn run(name: &str, sources: &[String]) -> FrontTally {
    run_front("journey_front", name, sources, None, pele_core::journey_front_json, FEATURES)
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("journey", "sources")).finish("harvested journey sources");
    assert!(tally.identical > 700, "the harvest holds journeys Pele parses");
    assert!(tally.rejected("syntax") > 100, "the harvest holds sources Pele cannot parse");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("journey", "mutated")).finish("mutated journey sources");
    assert!(tally.identical > 1500 && tally.rejected("syntax") > 1000);
}

#[test]
fn worst_cases_match_pele() {
    let worst = harvest("journey", "worst");
    let tally = run_front("journey_front", "worst", &worst, Some(100_000_000), pele_core::journey_front_json, &[])
        .finish("Pele's worst-case journeys");
    assert!(tally.identical > 20, "the worst cases parse");
}

/// Hand-written sources for each place the grammar can stop and each rule of the model.
fn cases() -> Vec<String> {
    [
        "journey",
        "journey\n",
        "journeys",
        "journey_",
        "JOURNEY\nTask: 5",
        "journey\nTask: 5",
        "journey\nTask:5:Me",
        "journey\nTask: 5: Me, Cat , ,Dog",
        "journey\nTask: 5: a: ignored: too",
        "journey\nTask:: x",
        "journey\nTask: : Me",
        "journey\nTask:",
        "journey\nTask: abc",
        "journey\nTask: 0x1F",
        "journey\nTask: 1e3",
        "journey\nTask: -3",
        "journey\nTask: 3.5",
        "journey\nTask:  5  ",
        "journey\nTask: Infinity",
        "journey\nTask: -Infinity",
        "journey\nTask: 5 # comment",
        "journey\nTask: 5 %% comment",
        "journey\nTask: 5; Next: 4",
        "journey\nTask",
        "journey\nTask\n: 5",
        "journey\n: 5",
        "journey\n:",
        "journey\n;",
        "journey\nsection A\nTask: 5\nsection B\nsection C\nTask: 1",
        "journey\nTask: 3\nsection Later\nTask: 4",
        "journey\nsection\nNext: line\nTask: 5",
        "journey\nsection A: 5",
        "journey\nsectionx: 5",
        "journey\ntitle My day\nTask: 5",
        "journey\ntitle\nTask: 5",
        "journey\ntitle a;b\nTask: 5",
        "journey\ntitle #x\nTask: 5",
        "journey\ntitlex: 5",
        "journey title T section S Task: 5: Me",
        "---\ntitle: Front\n---\njourney\nTask: 5",
        "---\ntitle: Front\n---\njourney\ntitle Own\nTask: 5",
        "journey\naccTitle: Acc  title \naccDescr: Descr\nTask: 5",
        "journey\naccDescr {\n  Several\n    lines\n}\nTask: 5",
        "journey\naccDescr {}title x\nTask: 5",
        "journey\naccTitle:",
        "journey\naccDescr {\n never closed",
        "journey\r\ntitle Windows\r\nTask: 5: Me\r\n",
        "journey\n50% done: 5\n% comment\n%{ not a comment: 1",
        "journey\n\u{1f600} \u{e9}: 5: \u{91c7}",
        "journey\n<br>a: 5: b<br>c",
        "  \n  journey\n\n\n  Task\n\n  : 5\n",
        "journey\nTask: 5\njourney\n",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn hand_written_sources_match_pele() {
    let tally = run("case", &cases()).finish("hand-written journey sources");
    assert!(tally.rejected("syntax") >= 8 && tally.identical >= 30);
}

const TASKS: &[&str] =
    &["Make tea", "Go upstairs", "Do work", "x", "A task with a long name", "\u{e9}\u{91c7}", "a<br>b", "50% done"];
const SCORES: &[&str] = &["5", "4", "3", "2", "1", "0", "6", "3.5", "x", "", " 2 ", "-1", "1e1"];
const ACTORS: &[&str] = &["Me", "Cat", "Dog", "", " Alice ", "Bob", "Me"];
const LINES: &[&str] = &[
    "title My working day",
    "title",
    "accTitle: acc",
    "accDescr: descr",
    "accDescr {\n a\n b\n}",
    "%% c",
    "# c",
    "section Go to work",
    "section Home",
    "section",
    "",
];
const NOISE: &[&str] = &[":", ";", "::", "journey", "title", "section", "accTitle", "#", "%", "{", "}", ","];
const INDENTS: &[&str] = &["", "", "  ", "    ", "\t", "      "];
const BREAKS: &[&str] = &["\n", "\n", "\n", "\n", "\n\n", "\r\n", " %% c\n", " # c\n"];

/// A source of tasks, sections and title lines, most of them valid, a few spoiled by a token edit.
fn random_source(rng: &mut Rng) -> String {
    let mut src = String::from(rng.pick(&["journey", "journey", "journey", "journey "]));
    src += "\n";
    for _ in 0..=rng.below(10) {
        let mut line = String::from(rng.pick(INDENTS));
        if rng.chance(65) {
            line += rng.pick(TASKS);
            line += rng.pick(&[": ", ":", " : "]);
            line += rng.pick(SCORES);
            if rng.chance(70) {
                line += rng.pick(&[": ", ":"]);
                let actors: Vec<&str> = (0..=rng.below(4)).map(|_| rng.pick(ACTORS)).collect();
                line += &actors.join(rng.pick(&[", ", ","]));
            }
        } else {
            line += rng.pick(LINES);
        }
        if rng.chance(4) {
            let at = rng.below(line.len() + 1);
            if line.is_char_boundary(at) {
                line.insert_str(at, rng.pick(NOISE));
            }
        }
        src += &line;
        src += rng.pick(BREAKS);
    }
    src
}

#[test]
fn random_sources_match_pele() {
    let mut rng = Rng(0x1a7e_0000_7ee3_000b);
    let sources: Vec<String> = (0..4000).map(|_| random_source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random journey sources");
    assert!(tally.identical > 2000 && tally.rejected("syntax") > 100);
    tally.require(FEATURES);
}
