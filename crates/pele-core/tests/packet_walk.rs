//! A packet's Scene read the way an interpreter with no SVG reads it: each field is an entity
//! named by its range of bits and the further pieces of a field that runs across rows rank after
//! it, every drawing has a key of its own across and down, and a key stays put when the source
//! changes elsewhere or the rows get shorter.

#[path = "differential/harness.rs"]
mod harness;

use harness::{drawn_entities, drawn_entity_texts, drawn_keys, edit, keys_survive, rune_fixture, scene, walk_sources};
use pele_core::scene::{EntityKind, EntityName, Facet, FragmentDetail, PaintVisit};
use pele_core::{LayoutOptions, ParseOptions, Scene};
use serde_json::Value;

/// The fields a Scene names, in the order it draws them, each as the bits it runs over and the
/// number of pieces it is drawn in.
fn fields(scene: &Scene, source: &str) -> Vec<([f64; 2], u32)> {
    let mut out: Vec<([f64; 2], u32)> = Vec::new();
    for entity in drawn_entities(scene).iter().filter(|entity| entity.kind() == EntityKind::Node) {
        let EntityName::Named(name) = entity.name() else { panic!("a field is named in {source:?}") };
        let name = String::from_utf16_lossy(name);
        let bits = name.split_once('-').map(|(start, end)| [start.parse::<f64>(), end.parse::<f64>()]);
        let Some([Ok(start), Ok(end)]) = bits else { panic!("{name} is not a range of bits in {source:?}") };
        match out.last_mut() {
            Some((last, pieces)) if *last == [start, end] && entity.duplicate() == *pieces => *pieces += 1,
            _ => {
                assert_eq!(entity.duplicate(), 0, "{name} is first drawn as a further piece in {source:?}");
                out.push(([start, end], 1));
            }
        }
    }
    out
}

/// Each piece of a field draws one box, keyed whole.
fn one_box_each(scene: &Scene, source: &str) {
    let mut boxes = Vec::new();
    for visit in scene.paint() {
        if let PaintVisit::Draw(draw) = visit
            && draw.key.part.entity.kind() == EntityKind::Node
            && draw.key.part.facet == Facet::Shape
        {
            assert_eq!(draw.key.detail, FragmentDetail::Whole, "{} draws a second box in {source:?}", draw.key);
            boxes.push(draw.key.part.entity);
        }
    }
    let named = scene.entities().filter(|entity| entity.kind() == EntityKind::Node).count();
    assert_eq!(boxes.len(), named, "a piece draws no box in {source:?}");
}

#[test]
fn every_element_is_drawn_under_a_key_of_its_own() {
    let down = LayoutOptions::default().max_width(320.0);
    let (mut drawn, mut turned, mut split) = (0, 0, 0);
    for source in walk_sources("packet") {
        let Ok(model) = pele_core::packet_front_json(&source, ParseOptions::default()) else { continue };
        let model: Value = serde_json::from_str(&model).expect("the model is JSON");
        let Some(rows) = model["model"]["rows"].as_array() else { continue };
        let pieces: Vec<&Value> = rows.iter().flat_map(|row| row.as_array().expect("a row")).collect();
        let Some(across) = scene(&source, &LayoutOptions::default()) else { continue };
        let narrow = scene(&source, &down).expect("what draws across draws narrow");
        for scene in [&across, &narrow] {
            drawn_keys(scene, &source);
            one_box_each(scene, &source);
        }
        let named = fields(&across, &source);
        let mut next = 0.0;
        for ([start, end], _) in &named {
            assert_eq!(*start, next, "the fields run on from each other in {source:?}");
            next = end + 1.0;
        }
        let last = pieces.last().map(|piece| piece[1].as_str().expect("a number").parse::<f64>().expect("a number"));
        assert_eq!(named.last().map(|([_, end], _)| *end), last, "the fields cover every bit in {source:?}");
        assert_eq!(named.iter().map(|(_, pieces)| *pieces as usize).sum::<usize>(), pieces.len(), "{source:?}");
        let names = |fields: Vec<([f64; 2], u32)>| fields.into_iter().map(|(bits, _)| bits).collect::<Vec<_>>();
        let across_names = names(named.clone());
        assert_eq!(names(fields(&narrow, &source)), across_names, "the fields are named alike at every width");
        drawn += 1;
        turned += usize::from(narrow.size().width < across.size().width);
        split += usize::from(named.iter().any(|(_, pieces)| *pieces > 1));
    }
    println!(
        "packets walked across and narrow: {drawn}. Drawn down when narrow: {turned}. With a field split: {split}."
    );
    assert!(drawn > 300, "the corpus, the fixture and the harvest draw");
    assert!(turned > 100 && split > 50, "the walk reaches packets drawn down and fields split across rows");
}

#[test]
fn keys_come_from_field_ranges() {
    let down = LayoutOptions::default().max_width(320.0);
    let fixture = rune_fixture("packet");
    let across = scene(&fixture, &LayoutOptions::default()).expect("the fixture draws");
    let narrow = scene(&fixture, &down).expect("the fixture draws");
    let want = ["node:0-15", "node:16-31", "node:32-47", "node:48-63", "node:64-95", "diagram:"];
    assert_eq!(drawn_entity_texts(&across), want);
    let want = ["node:0-15", "node:16-31", "node:32-47", "node:48-63", "node:64-95", "node:64-95#1", "diagram:"];
    assert_eq!(drawn_entity_texts(&narrow), want, "the data field runs over two rows of 16 bits");
    let split = "packet\n0-47: \"Header\"\n48-63: \"Sum\"";
    let across = scene(split, &LayoutOptions::default()).expect("it draws");
    assert_eq!(drawn_entity_texts(&across), ["node:0-47", "node:0-47#1", "node:48-63"]);
    let narrow = scene(split, &down).expect("it draws");
    assert_eq!(drawn_entity_texts(&narrow), ["node:0-47", "node:0-47#1", "node:0-47#2", "node:48-63"]);
}

#[test]
fn a_key_survives_an_edit_elsewhere() {
    let before = rune_fixture("packet");
    let after = edit(&before, "\"Length\"", "\"Total length\"");
    let after = edit(&after, "title: \"UDP Packet\"", "title: \"UDP Datagram\"");
    let after = after + "96-127: \"Options\"\n";
    keys_survive(&before, &after, &["node:32-47", "diagram:"]);
}
