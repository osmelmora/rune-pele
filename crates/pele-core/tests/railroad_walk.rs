//! A railroad diagram's Scene read the way an interpreter with no SVG reads it: every drawing
//! has a key of its own, a special sequence's question marks are drawn in the muted ink around
//! text in the diagram's own, and terminals, headings and specials are set in the fonts Pele
//! writes them in.

use pele_core::scene::{BaselineRun, Geometry, Paint, PaintVisit, Role, Scene};
use pele_core::{FontFamily, FontWeight, ParseOptions};
use serde_json::Value;
use std::collections::HashSet;

fn scene(source: &str) -> Option<Scene> {
    pele_core::parity_scene_with(source, ParseOptions::default(), &pele_core::LayoutOptions::default())
        .ok()
        .map(|parity| parity.scene().clone())
}

/// Each drawing's key with its fill and its text, where it is text.
fn drawings(scene: &Scene) -> Vec<(String, Paint, Option<BaselineRun>)> {
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => {
                let text = match draw.geometry {
                    Geometry::Text(run) => Some(run),
                    _ => None,
                };
                Some((draw.key.to_string(), draw.style.fill, text))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn every_drawing_of_the_harvest_has_a_key_of_its_own() {
    let path = pele_oracle::root().join("tests/fixtures/railroad/harvest.json");
    let harvest: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("the harvest")).expect("JSON");
    let mut drawn = 0;
    for source in harvest["sources"].as_array().expect("a list").iter().filter_map(Value::as_str) {
        let Some(scene) = scene(source) else { continue };
        let mut seen = HashSet::new();
        let keys = drawings(&scene);
        let repeated: Vec<&String> = keys.iter().map(|(key, _, _)| key).filter(|key| !seen.insert(*key)).collect();
        assert!(repeated.is_empty(), "keys drawn twice in {source:?}: {repeated:?}");
        drawn += 1;
    }
    assert!(drawn > 1500, "the harvest holds railroad diagrams that draw");
}

const GRAMMAR: &str = "railroad-ebnf-beta\ntitle Grammar\nr = \"lit\" name ? any ? ( a | b )+ ;\nr = name ;";

#[test]
fn each_part_of_a_rule_has_a_key_that_names_it() {
    let drawn = drawings(&scene(GRAMMAR).expect("the grammar draws"));
    let keys: HashSet<&str> = drawn.iter().map(|(key, _, _)| key.as_str()).collect();
    for want in [
        "diagram:/title/0.0",
        "group:r/title/0.0",
        "group:r/shape",
        "group:r/shape/1",
        "group:r/start",
        "group:r/end",
        "group:r/label/0.0",
        "node:name/shape",
        "node:name/label/0.0",
        "node:a/label/0.0",
        "group:r#1/title/0.0",
        "group:r#1/shape",
        "node:name/shape/1",
        "node:name/label/1/0.0",
    ] {
        assert!(keys.contains(want), "no drawing has the key {want}; the keys are {keys:?}");
    }
}

#[test]
fn a_special_sequence_marks_its_text_in_the_muted_ink() {
    let drawn = drawings(&scene(GRAMMAR).expect("the grammar draws"));
    let runs: Vec<(&Paint, &BaselineRun)> = drawn
        .iter()
        .filter_map(|(_, fill, run)| run.as_ref().map(|run| (fill, run)))
        .skip_while(|(_, run)| run.text != "? ")
        .take(3)
        .collect();
    let texts: Vec<&str> = runs.iter().map(|(_, run)| run.text.as_str()).collect();
    assert_eq!(texts, ["? ", "any", " ?"], "a special is drawn as its marks around its text");
    let fills: Vec<Paint> = runs.iter().map(|(fill, _)| **fill).collect();
    assert_eq!(fills, [Paint::Role(Role::Muted), Paint::Role(Role::Foreground), Paint::Role(Role::Muted)]);
    for pair in runs.windows(2) {
        let (left, right) = (pair[0].1, pair[1].1);
        assert!(
            (left.baseline.x + left.advance - right.baseline.x).abs() < 1e-9,
            "each run starts where the last ends"
        );
        assert_eq!(left.baseline.y, right.baseline.y, "the runs share one baseline");
    }
    assert!(runs.iter().all(|(_, run)| run.font.italic), "a special is set in italics");
}

#[test]
fn terminals_headings_and_names_are_set_in_their_fonts() {
    let drawn = drawings(&scene(GRAMMAR).expect("the grammar draws"));
    let run = |text: &str| {
        drawn
            .iter()
            .find_map(|(_, _, run)| run.as_ref().filter(|run| run.text == text))
            .unwrap_or_else(|| panic!("no run {text:?}"))
    };
    assert_eq!(run("lit").font.family, FontFamily::Mono, "a terminal is set in the monospace font");
    assert_eq!(run("name").font.family, FontFamily::Sans, "a non-terminal is set in the diagram's font");
    assert_eq!(run("r").font.weight, FontWeight::Bold, "a rule's heading is set in the heading weight");
    assert_eq!(run("r").font.size, 16.0, "a heading is set at the diagram's size");
    assert_eq!(run("lit").font.size, 14.0, "boxes are set at seven eighths of it");
}
