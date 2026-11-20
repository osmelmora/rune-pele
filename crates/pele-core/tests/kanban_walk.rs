//! A kanban board's Scene read the way an interpreter with no SVG reads it: every drawing has a
//! key of its own, whether the board folds or not, and a card's ticket and assignee keep their
//! keys however its text wraps.

use pele_core::scene::{EntityKind, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::BTreeSet;

fn scene(source: &str, options: &LayoutOptions) -> Option<Scene> {
    let parity = pele_core::parity_scene_with(source, ParseOptions::default(), options).ok()?;
    Some(parity.scene().clone())
}

fn sources() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/kanban/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: serde_json::Value = serde_json::from_str(&text).expect("the harvest is JSON");
    let corpus = pele_oracle::corpus().into_iter().filter(|(kind, _)| kind == "kanban").flat_map(|(_, s)| s);
    let harvested = harvest["sources"].as_array().expect("a list").iter().filter_map(|s| s.as_str().map(str::to_owned));
    corpus.chain(harvested).collect()
}

/// Every drawing's key, failing on one that is drawn twice.
fn keys(scene: &Scene, source: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for visit in scene.paint() {
        if let PaintVisit::Draw(draw) = visit {
            let key = draw.key.to_string();
            assert!(seen.insert(key.clone()), "{key} is drawn twice in {source:?}");
        }
    }
    seen
}

#[test]
fn every_drawing_has_a_key_of_its_own_at_every_width() {
    let narrow = LayoutOptions::default().max_width(320.0);
    let mut drawn = 0;
    for source in sources() {
        for options in [&LayoutOptions::default(), &narrow] {
            let Some(scene) = scene(&source, options) else { continue };
            drawn += 1;
            keys(&scene, &source);
        }
    }
    assert!(drawn > 1500, "the harvest draws");
}

#[test]
fn a_cards_metadata_keeps_its_keys_however_its_text_wraps() {
    let card = |label: &str| {
        let source = format!("kanban\n  todo\n    a[{label}]@{{ ticket: T-1, assigned: knsv }}\n    a[Again]");
        keys(&scene(&source, &LayoutOptions::default()).expect("it draws"), &source)
    };
    let short = card("Short");
    let long = card("A card whose text is long enough to wrap over three lines of its column");
    for key in ["node:a/attribute/0.0", "node:a/annotation/0.0", "node:a#1/label/0.0"] {
        assert!(short.contains(key) && long.contains(key), "{key} is drawn for both cards");
    }
    assert!(long.contains("node:a/label/2/0.0") && !short.contains("node:a/label/1/0.0"));
}

#[test]
fn columns_are_groups_and_cards_named_alike_are_ranked() {
    let source = "kanban\n  todo\n    a\n    a\n  todo\n    a";
    let scene = scene(source, &LayoutOptions::default()).expect("it draws");
    let of = |kind: EntityKind| {
        scene.entities().filter(|entity| entity.kind() == kind).map(|e| e.duplicate()).collect::<Vec<_>>()
    };
    assert_eq!(of(EntityKind::Group), [0, 1], "two columns named todo");
    assert_eq!(of(EntityKind::Node), [0, 1, 2], "three cards named a");
}
