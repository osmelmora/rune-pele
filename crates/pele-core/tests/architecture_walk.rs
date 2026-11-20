//! An architecture diagram's Scene read the way an interpreter with no SVG reads it: each drawing
//! has a key of its own, keys do not move with another id prefix, a tight drawing draws the same
//! parts, an edge joins its two nodes, and a built-in icon reaches the interpreter as its glyph.

use pele_core::scene::{EntityKind, Geometry, IconGlyph, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::BTreeSet;

fn scene(source: &str, options: &LayoutOptions) -> Option<Scene> {
    let parity = pele_core::parity_scene_with(source, ParseOptions::default(), options).ok()?;
    Some(parity.scene().clone())
}

fn sources() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/architecture/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: serde_json::Value = serde_json::from_str(&text).expect("the harvest is JSON");
    let corpus = pele_oracle::corpus().into_iter().filter(|(kind, _)| kind == "architecture").flat_map(|(_, s)| s);
    let harvested = harvest["sources"].as_array().expect("a list").iter().filter_map(|s| s.as_str().map(str::to_owned));
    corpus.chain(harvested).collect()
}

/// The part each drawing is of: its entity and facet.
fn parts(scene: &Scene) -> BTreeSet<String> {
    let parts = scene.paint().filter_map(|visit| match visit {
        PaintVisit::Draw(draw) => Some(format!("{:?}", draw.key.part)),
        _ => None,
    });
    parts.collect()
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
    let plain = LayoutOptions::default();
    let other = LayoutOptions::default().id_prefix("other");
    let tight = LayoutOptions::default().max_width(320.0);
    let mut drawn = 0;
    for source in sources() {
        let Some(base) = scene(&source, &plain) else { continue };
        drawn += 1;
        let again = scene(&source, &other).expect("drawn above");
        assert_eq!(keys(&again, &source), keys(&base, &source), "keys move with the id prefix in {source:?}");
        // A tight drawing may wrap a label onto more lines, so only its parts are the same.
        let narrow = scene(&source, &tight).expect("drawn above");
        keys(&narrow, &source);
        assert_eq!(parts(&narrow), parts(&base), "a tight drawing draws other parts in {source:?}");
    }
    assert!(drawn > 1000, "the harvest draws");
}

#[test]
fn edges_join_their_nodes_and_icons_carry_their_glyph() {
    let source = "architecture-beta\ngroup api(cloud)[API]\nservice db(database)[DB] in api\nservice x(logos:aws)[X] in api\nservice web\ndb:L <-[reads]- R:web\ndb:L -- R:web";
    let scene = scene(source, &LayoutOptions::default()).expect("it draws");
    let edges: Vec<_> = scene.entities().filter(|entity| entity.kind() == EntityKind::Edge).collect();
    assert_eq!(edges.len(), 2, "two edges with the same ends are two entities");
    assert_ne!(edges[0], edges[1]);
    let named = |kind: EntityKind, name: &str| {
        let name: Vec<u16> = name.encode_utf16().collect();
        scene.entities().find(|entity| entity.kind() == kind && entity.identity() == name).expect("drawn").clone()
    };
    let (db, web) = (named(EntityKind::Node, "db"), named(EntityKind::Node, "web"));
    assert_eq!(scene.related(edges[0]), [&db, &web]);
    let glyphs: Vec<(String, Option<IconGlyph>)> = scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => match draw.geometry {
                Geometry::Icon { name, glyph, .. } => Some((name, glyph)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let expected = [
        ("cloud".to_owned(), Some(IconGlyph::Cloud)),
        ("database".to_owned(), Some(IconGlyph::Database)),
        ("logos:aws".to_owned(), None),
    ];
    assert_eq!(glyphs, expected, "a name Pele knows draws its glyph, any other leaves the box to the host");
}
