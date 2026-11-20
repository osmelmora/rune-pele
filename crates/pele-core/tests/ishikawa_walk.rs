//! An Ishikawa diagram's Scene read the way an interpreter with no SVG reads it: every drawing has
//! a key of its own, across and as an indented list, and the categories are groups across both.

use pele_core::scene::{EntityKind, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::BTreeSet;

fn scene(source: &str, options: &LayoutOptions) -> Option<Scene> {
    let parity = pele_core::parity_scene_with(source, ParseOptions::default(), options).ok()?;
    Some(parity.scene().clone())
}

fn sources() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/ishikawa/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: serde_json::Value = serde_json::from_str(&text).expect("the harvest is JSON");
    let corpus = pele_oracle::corpus().into_iter().filter(|(kind, _)| kind == "ishikawa").flat_map(|(_, s)| s);
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
    assert!(drawn > 1000, "the harvest draws");
}

#[test]
fn the_effect_and_causes_are_nodes_and_the_categories_groups_either_way() {
    let source = "ishikawa-beta\n  E\n    C\n      x\n        x\n    D\n";
    let narrow = LayoutOptions::default().max_width(120.0);
    for options in [&LayoutOptions::default(), &narrow] {
        let scene = scene(source, options).expect("it draws");
        let names = |kind: EntityKind| {
            let named = scene.entities().filter(|entity| entity.kind() == kind);
            named.map(|e| (String::from_utf16_lossy(e.identity()), e.duplicate())).collect::<Vec<_>>()
        };
        assert_eq!(names(EntityKind::Group), [("C".to_owned(), 0), ("D".to_owned(), 0)]);
        assert_eq!(names(EntityKind::Node), [("E".to_owned(), 0), ("x".to_owned(), 0), ("x".to_owned(), 1)]);
    }
}
