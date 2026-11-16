//! Differential test for the timeline front end: for every source, this port's model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `timeline_front` op, and a
//! source Pele rejects must be rejected here with Pele's message and position.
//!
//! `tests/fixtures/timeline/harvest.json` holds the timeline sources of Pele's own test suite and
//! the inputs of its mutated runs, written by `scripts/harvest-timeline.mjs`.

#[path = "differential/harness.rs"]
mod harness;

use harness::{Feature, FrontTally, Rng, harvest, run_front};
use serde_json::Value;

fn periods(model: &Value) -> &[Value] {
    model["periods"].as_array().map_or(&[], Vec::as_slice)
}

const FEATURES: &[Feature] = &[
    ("a title", |m| m["title"].is_string()),
    ("an accessible title", |m| m["accTitle"].is_string()),
    ("an accessible description over lines", |m| m["accDescr"].as_str().is_some_and(|d| d.contains('\n'))),
    ("running down", |m| m["direction"] == "TD"),
    ("sections", |m| m["sections"].as_array().is_some_and(|s| s.len() > 1)),
    ("a period before the first section", |m| {
        m["sections"].as_array().is_some_and(|s| !s.is_empty()) && periods(m).iter().any(|p| p["sectionIndex"] == -1)
    }),
    ("a section with no periods", |m| {
        let count = m["sections"].as_array().map_or(0, Vec::len);
        (0..count).any(|i| !periods(m).iter().any(|p| p["sectionIndex"] == i))
    }),
    ("several events of one period", |m| {
        periods(m).iter().any(|p| p["events"].as_array().is_some_and(|e| e.len() > 2))
    }),
    ("an event with a colon in it", |m| {
        periods(m).iter().any(|p| {
            p["events"].as_array().is_some_and(|e| e.iter().any(|e| e.as_str().is_some_and(|e| e.contains(':'))))
        })
    }),
    ("a period with no events", |m| periods(m).iter().any(|p| p["events"].as_array().is_some_and(Vec::is_empty))),
];

fn run(name: &str, sources: &[String]) -> FrontTally {
    run_front("timeline_front", name, sources, None, pele_core::timeline_front_json, FEATURES)
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("timeline", "sources")).finish("harvested timeline sources");
    assert!(tally.identical > 700, "the harvest holds timelines Pele parses");
    assert!(tally.rejected("syntax") > 20, "the harvest holds sources Pele cannot parse");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("timeline", "mutated")).finish("mutated timeline sources");
    assert!(tally.identical > 2000 && tally.rejected("syntax") > 500);
}

#[test]
fn worst_cases_match_pele() {
    let worst = harvest("timeline", "worst");
    let tally = run_front("timeline_front", "worst", &worst, Some(100_000_000), pele_core::timeline_front_json, &[])
        .finish("Pele's worst-case timelines");
    assert!(tally.identical > 25, "the worst cases parse");
}

/// Hand-written sources for each place the grammar can stop and each rule of the model.
fn cases() -> Vec<String> {
    [
        "timeline",
        "timeline LR",
        "timeline TD",
        "timeline  \t td\n2001 : a",
        "timeline LRx\n2001 : a",
        "timelineLR\n2001 : a",
        "timeline lr td",
        "TIMELINE Lr\n2001",
        "timelines",
        "timeline\n: a",
        "timeline\n2001 : a : b : c\n     : d\n     : e : f",
        "timeline\n2001 : a:b : c::d : e:\n2002 :b\n2003 : \n",
        "timeline\n2001 :\nnext line\n",
        "timeline\n2001 # a comment\n2002 %% another\nx%%y\n",
        "timeline\n  % comment\n  %{ not one\n",
        "timeline\nsection One\n2001 : a\nsection Two\nsection Three\n2002 : b",
        "timeline\n2000 : before\nsection One\n2001 : a",
        "timeline\nsection\nNext line : x\n",
        "timeline\nsection A : B\n",
        "timeline\nsectionx\n",
        "timeline\nSECTION Caps\n2001",
        "timeline\ntitle History of things\n2001 : a",
        "timeline\ntitle\n2001 : a",
        "timeline\ntitle   \n2001 : a",
        "timeline\ntitlex : a",
        "timeline\ntitle\tTabbed\ntitle Second\n",
        "timeline title T section S p : e",
        "---\ntitle: Front\n---\ntimeline\n2001 : a",
        "---\ntitle: Front\n---\ntimeline\ntitle Own\n2001 : a",
        "timeline\naccTitle: Acc  title \naccDescr: Acc description\n2001 : a",
        "timeline\naccDescr {\n  Several\n    lines\n}\n2001 : a",
        "timeline\naccDescr {}title x\n2001 : a",
        "timeline\naccTitle:",
        "timeline\naccDescr {\n never closed",
        "timeline\naccTitle\n\n:\nx",
        "timeline\r\ntitle Windows\r\n2001 : a\r\n",
        "timeline\n2001 : a ; b\n; : c",
        "timeline\n2001 : \u{1f600} \u{e9} <br> #35; &amp;",
        "timeline\n:: a",
        "timeline\n2001 : a\ntimeline\n",
        "  \n  timeline\n\n\n  2001\n\n  : late event\n",
        "timeline\n\u{a0}2001 : a",
        "timeline\n2001\u{2028}: a",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn hand_written_sources_match_pele() {
    let tally = run("case", &cases()).finish("hand-written timeline sources");
    assert!(tally.rejected("syntax") >= 3 && tally.rejected("semantic") >= 1 && tally.identical >= 30);
}

const PERIODS: &[&str] =
    &["2001", "2002 ", "Q1", "1990s", "A long period name", "x", "\u{e9}\u{91c7}", "#35;", "<b>b</b>"];
const EVENTS: &[&str] = &["a", "LinkedIn", "two words", "x:y", "with a: colon", "<br>", "\u{1f600}", "&amp;", "", "e "];
const LINES: &[&str] = &[
    "title History",
    "title",
    "accTitle: acc",
    "accDescr: descr",
    "accDescr {\n a\n b\n}",
    "%% c",
    "# c",
    "section One",
    "section Two words",
    "section",
    "",
];
const NOISE: &[&str] = &[":", " : ", "::", "timeline", "title", "section", "accTitle", ";", "%", "{", "}"];
const INDENTS: &[&str] = &["", "", "  ", "    ", "\t", "         "];
const BREAKS: &[&str] = &["\n", "\n", "\n", "\n", "\n\n", "\r\n", " %% c\n", " # c\n"];

/// A source of periods, events, sections and title lines, most of them valid, a few spoiled by a
/// token edit.
fn random_source(rng: &mut Rng) -> String {
    let mut src =
        String::from(rng.pick(&["timeline", "timeline", "timeline LR", "timeline TD", "Timeline td", "timeline "]));
    src += "\n";
    for _ in 0..=rng.below(10) {
        let mut line = String::from(rng.pick(INDENTS));
        let roll = rng.below(100);
        if roll < 55 {
            line += rng.pick(PERIODS);
            for _ in 0..rng.below(4) {
                line += rng.pick(&[" : ", " :\t", " :  "]);
                line += rng.pick(EVENTS);
            }
        } else if roll < 70 {
            line += ": ";
            line += rng.pick(EVENTS);
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
    let mut rng = Rng(0x71e1_17e5_0000_0002);
    let sources: Vec<String> = (0..4000).map(|_| random_source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random timeline sources");
    assert!(tally.identical > 2000 && tally.rejected("syntax") > 100);
    tally.require(FEATURES);
}
