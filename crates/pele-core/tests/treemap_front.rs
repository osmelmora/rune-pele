//! Differential test for the treemap front end: for every source, this port's model must come out
//! as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `treemap_front` op, values as
//! the bits of their doubles, and a source Pele rejects must be rejected here with Pele's message
//! and position.
//!
//! `tests/fixtures/treemap/harvest.json` holds the treemap sources of Pele's own test suite and
//! the inputs of its mutated runs, written by `scripts/harvest-treemap.mjs`.

#[path = "differential/harness.rs"]
mod harness;

use harness::{Feature, FrontTally, Rng, harvest, run_front};
use serde_json::Value;

fn nodes(model: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    let mut stack: Vec<&Value> = model["roots"].as_array().map(|roots| roots.iter().collect()).unwrap_or_default();
    while let Some(node) = stack.pop() {
        out.push(node);
        if let Some(children) = node["children"].as_array() {
            stack.extend(children);
        }
    }
    out
}

fn depth(node: &Value) -> usize {
    node["children"].as_array().map_or(0, |children| 1 + children.iter().map(depth).max().unwrap_or(0))
}

const FEATURES: &[Feature] = &[
    ("a title", |m| m["title"].is_string()),
    ("an accessible title", |m| m["accTitle"].is_string()),
    ("an accessible description over lines", |m| m["accDescr"].as_str().is_some_and(|d| d.contains('\n'))),
    ("a section with no rows", |m| nodes(m).iter().any(|n| n["children"].as_array().is_some_and(Vec::is_empty))),
    ("sections three deep", |m| m["roots"].as_array().is_some_and(|roots| roots.iter().any(|n| depth(n) >= 3))),
    ("a leaf at the top", |m| m["roots"].as_array().is_some_and(|roots| roots.iter().any(|n| n["value"].is_string()))),
    ("a value that is no number", |m| nodes(m).iter().any(|n| n["value"] == "7ff8000000000000")),
    ("a fractional value", |m| {
        nodes(m).iter().any(|n| {
            n["value"]
                .as_str()
                .and_then(|v| u64::from_str_radix(v, 16).ok())
                .is_some_and(|v| f64::from_bits(v).fract() != 0.0)
        })
    }),
    ("a class selector with no class", |m| {
        nodes(m).iter().any(|n| n["classSelector"].is_string() && n["cssCompiledStyles"].is_null())
    }),
    ("a class with its styles", |m| nodes(m).iter().any(|n| n["cssCompiledStyles"].is_array())),
    ("a class with no styles", |m| {
        m["classes"].as_array().is_some_and(|classes| classes.iter().any(|c| c[1] == serde_json::json!([""])))
    }),
    ("a class defined twice", |m| {
        m["classes"]
            .as_array()
            .is_some_and(|classes| classes.iter().any(|c| c[1].as_array().is_some_and(|s| s.len() > 3)))
    }),
    ("a style with an escaped comma", |m| {
        m["classes"].as_array().is_some_and(|classes| {
            classes
                .iter()
                .any(|c| c[1].as_array().is_some_and(|s| s.iter().any(|s| s.as_str().is_some_and(|s| s.contains(',')))))
        })
    }),
];

fn run(name: &str, sources: &[String]) -> FrontTally {
    run_front("treemap_front", name, sources, None, pele_core::treemap_front_json, FEATURES)
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &harvest("treemap", "sources")).finish("harvested treemap sources");
    assert!(tally.identical > 700, "the harvest holds treemaps Pele parses");
    assert!(tally.rejected("syntax") > 50, "the harvest holds sources Pele cannot parse");
}

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &harvest("treemap", "mutated")).finish("mutated treemap sources");
    assert!(tally.identical > 1500 && tally.rejected("syntax") > 1500);
}

#[test]
fn worst_cases_match_pele() {
    let worst = harvest("treemap", "worst");
    let tally = run_front("treemap_front", "worst", &worst, Some(100_000_000), pele_core::treemap_front_json, &[])
        .finish("Pele's worst-case treemaps");
    assert!(tally.identical > 20, "the worst cases parse");
}

/// Hand-written sources for each place the grammar can stop and each rule of the model.
fn cases() -> Vec<String> {
    [
        "treemap",
        "treemap-beta",
        "treemap-betax\n\"a\": 1",
        "treemapx",
        "treemap\n\"A\"\n  \"B\": 1\n    \"C\": 2\n  \"D\"\n      \"E\": 3\n\"F\": 4",
        "treemap\n\t\"A\"\n  \"B\": 1\n \t\"C\": 2",
        "treemap\n\"A\" \"B\": 1",
        "treemap\n\"A\"   :   5",
        "treemap\n\"A\"   ,   5",
        "treemap\n\"A\": 1,2,3",
        "treemap\n\"A\": 1__2",
        "treemap\n\"A\": ._5",
        "treemap\n\"A\": 0.",
        "treemap\n\"A\": 0000.5000",
        "treemap\n\"A\": 123456789012345678901234567890",
        "treemap\n\"A\": 1:::cls\n\"B\":::cls\nclassDef cls fill:red\\,blue,stroke:green;",
        "treemap\nclassDef cls\nclassDef cls fill:red\nclassDef cls stroke:blue;\n\"A\": 1:::cls",
        "treemap\nclassDef cls §§§fill:red\n\"A\": 1:::cls",
        "treemap\nclassDef cls   \n\n  \"A\": 1",
        "treemap\nclassDef  c2 fill:red;stroke:blue\n\"A\": 1",
        "treemap\n  classDef c3 fill:red\n  \"A\": 1:::c3",
        "treemap\nclassDef c\n\"A\": 1",
        "treemap\nclassDef 2c fill:red",
        "treemap\n\"A\":::",
        "treemap\n\"A\":::1",
        "treemap\n\"A\": 1 :::c",
        "treemap\n'A': 1\n\"it's\": 2\n'say \"hi\"': 3",
        "treemap\n\"#35; &amp; <b>x</b>\": 1",
        "treemap\n\"\u{1f600}\u{e9}\": 1",
        "treemap\n\"unclosed: 1",
        "treemap\n\"A\": -1",
        "treemap\n\"A\": 1e3",
        "treemap title  Spaced   out  \n\"A\": 1",
        "treemap\ntitle\n\"A\": 1",
        "treemap\ntitles\n\"A\": 1",
        "treemap\naccTitle:   spaced  \naccDescr {\n  one\n\n   two  \n}\n\"A\": 1",
        "treemap\naccDescr: one %% c\n\"A\": 1",
        "treemap\naccDescr {never closed\n\"A\": 1",
        "---\ntitle: front\n---\ntreemap\n\"A\": 1",
        "---\ntitle: front\n---\ntreemap\ntitle own\n\"A\": 1",
        "---\ntitle: \"\"\n---\ntreemap\n\"A\": 1",
        "treemap\r\n\"A\"\r\n  \"B\": 1\r\n",
        "treemap\n%% comment\n\"A\": 1 %% after\n",
        "treemap\n%%{init: {}}%%\n\"A\": 1",
        "treemap\n\"A\"\n:",
        "treemap\n:",
        "treemap\n,",
        "treemap\n\"A\": x",
        "treemap\nx",
        "treemap\n\"\": 1",
        "treemap\n\"A\"\n  \"\"\n    \"B\": 1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn hand_written_sources_match_pele() {
    let tally = run("case", &cases()).finish("hand-written treemap sources");
    assert!(tally.rejected("syntax") >= 10 && tally.identical >= 30);
}

const NAMES: &[&str] = &[
    "\"a\"",
    "'b'",
    "\"Two words\"",
    "\"\"",
    "\"  padded  \"",
    "\"it's\"",
    "'say \"hi\"'",
    "\"#35; &amp;\"",
    "\"\u{e9}\u{91c7}\u{1f600}\"",
    "\"multi\nline\"",
    "\"a\"",
];
const NUMBERS: &[&str] =
    &["1", "2", "10", "1,000", "1.5", "0", "0.25", "_", ".", "1.2.3", "00.10", "5,", "99999999", "3__"];
const SEPARATORS: &[&str] = &[":", ": ", " : ", ",", " ,", ":\t", ":  "];
const CLASSES: &[&str] = &["c1", "c2", "big", "c", "titles", "title_2", "x1"];
const STYLES: &[&str] =
    &["fill:red", "fill:red,stroke:blue", "color:white;", "font-size:20px", "fill:#ff0\\,x", "", " "];
const LINES: &[&str] =
    &["title A title", "title", "accTitle: acc", "accDescr: descr", "accDescr {\n a\n b\n}", "%% c", ""];
const NOISE: &[&str] = &[":", ":::", ",", "\"x\"", "x", "1", "classDef", ";", "-", "treemap"];
const INDENTS: &[&str] = &["", "", "  ", "    ", "      ", "\t", " \t", "        "];
const BREAKS: &[&str] =
    &["\n", "\n", "\n", "\n", "\n", "\n", "\n", "\n\n", "\r\n", "%% c\n", "%% c\n", "\n%% c\n", " %% c\n"];

/// A source of rows, class definitions and title lines, most of them valid, a few spoiled by a
/// token edit.
fn random_source(rng: &mut Rng) -> String {
    let mut src = String::from(rng.pick(&["treemap", "treemap", "treemap", "treemap-beta", "treemap "]));
    src += "\n";
    for _ in 0..=rng.below(9) {
        let mut line = String::from(rng.pick(INDENTS));
        let roll = rng.below(100);
        if roll < 70 {
            line += rng.pick(NAMES);
            if rng.chance(60) {
                line += rng.pick(SEPARATORS);
                line += rng.pick(NUMBERS);
            }
            if rng.chance(15) {
                line += ":::";
                line += rng.pick(CLASSES);
            }
        } else if roll < 85 {
            line += "classDef ";
            line += rng.pick(CLASSES);
            line += " ";
            line += rng.pick(STYLES);
        } else {
            line += rng.pick(LINES);
        }
        if rng.chance(3) {
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
    let mut rng = Rng(0x7ee3_a9f1_0000_0001);
    let sources: Vec<String> = (0..4000).map(|_| random_source(&mut rng)).collect();
    let tally = run("random", &sources).finish("random treemap sources");
    assert!(tally.identical > 1500 && tally.rejected("syntax") > 300);
    tally.require(FEATURES);
}
