//! An ER diagram's Scene read the way an interpreter with no SVG reads it: every drawing has a
//! key of its own, the cells of an entity's table start where its columns do, and the host's
//! monospace setting reaches the attribute cells but not the comments.

use pele_core::scene::{AnchorKind, BaselineRun, Geometry, PaintVisit, Scene};
use pele_core::{FontFamily, FontWeight, LayoutOptions, MonospacePart, ParseOptions};
use std::collections::HashSet;

fn scene(source: &str, layout: &LayoutOptions) -> Scene {
    pele_core::parity_scene_with(source, ParseOptions::default(), layout).expect("an ER diagram draws").scene().clone()
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

fn run<'a>(drawn: &'a [(String, Option<BaselineRun>)], text: &str) -> &'a BaselineRun {
    drawn
        .iter()
        .filter_map(|(_, run)| run.as_ref())
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("no run {text:?}"))
}

const SHOP: &str = "erDiagram\nCUSTOMER ||--o{ ORDER : places\nORDER ||--|{ ITEM : contains\nCUSTOMER |o--o| CUSTOMER : refers\nsubgraph s [Sales]\nORDER\nend\nCUSTOMER {\n  string name PK \"The name\"\n  int age\n}\nstyle CUSTOMER font-weight:bold";

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let drawn = drawings(&scene(SHOP, &LayoutOptions::default()));
    let mut seen = HashSet::new();
    let repeated: Vec<&String> = drawn.iter().map(|(key, _)| key).filter(|key| !seen.insert(*key)).collect();
    assert!(repeated.is_empty(), "keys drawn twice: {repeated:?}");
    let keys: HashSet<&str> = drawn.iter().map(|(key, _)| key.as_str()).collect();
    for want in [
        "group:s/shape",
        "group:s/title/0.0",
        "node:CUSTOMER/shape",
        "node:CUSTOMER/shape/3",
        "node:CUSTOMER/label/0.0",
        "node:ITEM/label/0.0",
        "edge:CUSTOMER~ORDER~only_one identifying zero_or_more/shape",
        "edge:CUSTOMER~ORDER~only_one identifying zero_or_more/start",
        "edge:CUSTOMER~ORDER~only_one identifying zero_or_more/end/1",
        "edge:CUSTOMER~ORDER~only_one identifying zero_or_more/label/0.0",
        "edge:CUSTOMER~CUSTOMER~zero_or_one identifying zero_or_one/end/1",
    ] {
        assert!(keys.contains(want), "no drawing has the key {want}; the keys are {keys:?}");
    }
}

#[test]
fn cells_start_at_their_columns_in_the_fonts_asked_for() {
    let drawn = drawings(&scene(SHOP, &LayoutOptions::default()));
    let (kind, name, keys, comment) =
        (run(&drawn, "string"), run(&drawn, "name"), run(&drawn, "PK"), run(&drawn, "The name"));
    for cell in [kind, name, keys, comment] {
        assert_eq!(cell.anchor, AnchorKind::Start, "{:?} starts at its column", cell.text);
        assert_eq!(cell.baseline.y, kind.baseline.y, "{:?} shares its row's baseline", cell.text);
        assert_eq!(cell.font.weight, FontWeight::Bold, "the entity's style makes {:?} bold", cell.text);
    }
    assert!(kind.baseline.x < name.baseline.x && name.baseline.x < keys.baseline.x, "columns run left to right");
    assert!(keys.baseline.x < comment.baseline.x, "the comment is the last column");
    assert_eq!(kind.font.size, 14.0, "cells are set at seven eighths of the size");
    assert_eq!(keys.font.size, 12.0, "keys are set at three quarters of the size");
    assert_eq!(run(&drawn, "int").baseline.x, kind.baseline.x, "the rows of one column line up");

    let mono = LayoutOptions { monospace: vec![MonospacePart::Attributes], ..LayoutOptions::default() };
    let drawn = drawings(&scene(SHOP, &mono));
    for text in ["string", "name", "PK", "int", "age"] {
        assert_eq!(run(&drawn, text).font.family, FontFamily::Mono, "the host sets {text:?} in mono");
    }
    assert_eq!(run(&drawn, "The name").font.family, FontFamily::Sans, "a comment stays in its font");
    assert_eq!(run(&drawn, "CUSTOMER").font.family, FontFamily::Sans, "an entity's name stays in its font");
}
