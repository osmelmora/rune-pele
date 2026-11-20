//! A mindmap's Scene read the way an interpreter with no SVG reads it: every drawing has a key of
//! its own, across and as an outline, and each link joins a node to its parent.

use pele_core::scene::{EntityKind, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::BTreeSet;

fn scene(source: &str, options: &LayoutOptions) -> Option<Scene> {
    let parity = pele_core::parity_scene_with(source, ParseOptions::default(), options).ok()?;
    Some(parity.scene().clone())
}

fn sources() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/mindmap/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: serde_json::Value = serde_json::from_str(&text).expect("the harvest is JSON");
    let corpus = pele_oracle::corpus().into_iter().filter(|(kind, _)| kind == "mindmap").flat_map(|(_, s)| s);
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
fn a_link_joins_a_node_to_its_parent_and_nodes_named_alike_are_ranked() {
    let source = "mindmap\n  root((R))\n    A\n      B\n    A";
    let scene = scene(source, &LayoutOptions::default()).expect("it draws");
    let of = |kind: EntityKind| scene.entities().filter(|entity| entity.kind() == kind).cloned().collect::<Vec<_>>();
    let nodes = of(EntityKind::Node);
    let named = |name: &str, rank: u32| {
        nodes
            .iter()
            .find(|n| n.identity() == name.encode_utf16().collect::<Vec<_>>() && n.duplicate() == rank)
            .expect("a node")
    };
    let edges = of(EntityKind::Edge);
    assert_eq!(edges.len(), 3);
    assert_eq!(scene.related(&edges[1]), [named("A", 0), named("B", 0)]);
    assert_eq!(scene.related(&edges[2]), [named("root", 0), named("A", 1)]);
}
