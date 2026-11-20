//! A cynefin diagram's Scene read the way an interpreter with no SVG reads it: each drawing has a
//! key of its own, and the keys do not move when another id prefix seeds other boundaries.

use pele_core::scene::{EntityKind, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::BTreeSet;

fn scene(source: &str, options: &LayoutOptions) -> Option<Scene> {
    let parity = pele_core::parity_scene_with(source, ParseOptions::default(), options).ok()?;
    Some(parity.scene().clone())
}

fn sources() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/cynefin/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: serde_json::Value = serde_json::from_str(&text).expect("the harvest is JSON");
    let corpus = pele_oracle::corpus().into_iter().filter(|(kind, _)| kind == "cynefin").flat_map(|(_, s)| s);
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
fn every_drawing_has_a_key_that_stays_put() {
    let other = LayoutOptions::default().id_prefix("other");
    let mut drawn = 0;
    for source in sources() {
        let Some(base) = scene(&source, &LayoutOptions::default()) else { continue };
        drawn += 1;
        let again = scene(&source, &other).expect("drawn above");
        assert_eq!(keys(&again, &source), keys(&base, &source), "keys move with the id prefix in {source:?}");
    }
    assert!(drawn > 300, "the harvest draws");
}

#[test]
fn a_transition_joins_its_domains_and_items_are_entities() {
    let source = "cynefin-beta\ncomplex\n\"a\"\n\"a\"\nclear\n\"a\"\ncomplex --> clear : \"x\"\ncomplex --> clear";
    let scene = scene(source, &LayoutOptions::default()).expect("it draws");
    let of = |kind: EntityKind| scene.entities().filter(|entity| entity.kind() == kind).cloned().collect::<Vec<_>>();
    let items = of(EntityKind::Node);
    assert_eq!(items.len(), 3, "an item named like another is an entity of its own");
    assert_eq!(items.iter().map(|item| item.duplicate()).collect::<Vec<_>>(), [0, 1, 2]);
    let edges = of(EntityKind::Edge);
    assert_eq!(edges.len(), 2);
    let domains = of(EntityKind::Group);
    let named = |name: &str| {
        domains.iter().find(|d| d.identity() == name.encode_utf16().collect::<Vec<_>>()).expect("a domain")
    };
    assert_eq!(scene.related(&edges[1]), [named("complex"), named("clear")]);
}
