//! A venn diagram's Scene read the way an interpreter with no SVG reads it: every drawing has a
//! key of its own, a set's circle and its label are one part, an overlap is a part of its own
//! even when it writes the same `data-id` as a set, and a key survives a change of the text
//! written before it.

use pele_core::scene::{BaselineRun, Geometry, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::HashSet;

fn scene(source: &str) -> Scene {
    pele_core::parity_scene_with(source, ParseOptions::default(), &LayoutOptions::default())
        .expect("a venn diagram draws")
        .scene()
        .clone()
}

/// Each drawing's key with its text, where it is text.
fn drawings(scene: &Scene) -> Vec<(String, Option<BaselineRun>)> {
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => {
                let text = match draw.geometry {
                    Geometry::Text(run) => Some(run),
                    _ => None,
                };
                Some((draw.key.to_string(), text))
            }
            _ => None,
        })
        .collect()
}

fn key_of<'a>(drawn: &'a [(String, Option<BaselineRun>)], text: &str) -> &'a str {
    drawn
        .iter()
        .find(|(_, run)| run.as_ref().is_some_and(|run| run.text == text))
        .map(|(key, _)| key.as_str())
        .unwrap_or_else(|| panic!("no run {text:?}"))
}

const TEAMS: &str = "venn-beta\ntitle Teams\nset A[\"Frontend\"]:20\n  text a1[\"React\"]\n  text a1[\"again\"]\nset B:12\nset C\nunion A,B[\"Shared\"]:3\n  text ab[\"OpenAPI\"]\nunion A,A[\"Self\"]\nunion A,B,C[\"All\"]\nstyle A,B fill:gold\nstyle A,B,C fill:#333";

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let drawn = drawings(&scene(TEAMS));
    let mut seen = HashSet::new();
    let repeated: Vec<&String> = drawn.iter().map(|(key, _)| key).filter(|key| !seen.insert(*key)).collect();
    assert!(repeated.is_empty(), "keys drawn twice: {repeated:?}");
    let keys: HashSet<&str> = drawn.iter().map(|(key, _)| key.as_str()).collect();
    for want in [
        "diagram:/title/0.0",
        "node:A/shape",
        "node:A/label/0.0",
        "node:C/shape",
        "section:[\"A\",\"B\"]/shape",
        "section:[\"A\",\"B\"]/label/0.0",
        "section:[\"A\",\"B\",\"C\"]/shape",
        "section:[\"A\",\"A\"]/label/0.0",
        "note:a1/label/0.0",
        "note:a1#1/label/0.0",
        "note:ab/label/0.0",
    ] {
        assert!(keys.contains(want), "no drawing has the key {want}; the keys are {keys:?}");
    }
}

#[test]
fn a_key_survives_text_written_before_it() {
    let before = drawings(&scene(TEAMS));
    let longer = TEAMS
        .replace("set A[\"Frontend\"]", "set A[\"The frontend team that ships the app every week\"]")
        .replace("text a1[\"React\"]", "text a0\n  text a1[\"React\"]");
    let after = drawings(&scene(&longer));
    for text in ["OpenAPI", "Shared", "All", "again"] {
        assert_eq!(key_of(&before, text), key_of(&after, text), "the key of {text:?}");
    }
}
