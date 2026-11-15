//! A class diagram's Scene read the way an interpreter with no SVG reads it: every drawing has a
//! key of its own, and each member is set in the font its marks and the host ask for.

use pele_core::scene::{Geometry, PaintVisit, Scene};
use pele_core::{FontFamily, FontWeight, LayoutOptions, MonospacePart, ParseOptions};
use std::collections::HashSet;

fn scene(source: &str, layout: &LayoutOptions) -> Scene {
    pele_core::parity_scene_with(source, ParseOptions::default(), layout)
        .expect("a class diagram draws")
        .scene()
        .clone()
}

/// Each drawing's key with its text, where it is text.
fn drawings(scene: &Scene) -> Vec<(String, Option<pele_core::scene::BaselineRun>)> {
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

const SHAPES: &str = "classDiagram\nclass Shape {\n  <<interface>>\n  +String name\n  +area()* double\n  +count()$ int\n  -List~int~ sides\n}\nclass note0\nnote \"A note named as a class\"\nnote0 ()-- Shape\nclass interface0\nShape \"1\" --> \"many\" interface0 : has\nShape \"1\" --> \"many\" interface0 : again\nShape --> Shape";

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let drawn = drawings(&scene(SHAPES, &LayoutOptions::default()));
    let mut seen = HashSet::new();
    let repeated: Vec<&String> = drawn.iter().map(|(key, _)| key).filter(|key| !seen.insert(*key)).collect();
    assert!(repeated.is_empty(), "keys drawn twice: {repeated:?}");
    let keys: HashSet<&str> = drawn.iter().map(|(key, _)| key.as_str()).collect();
    for want in [
        "node:Shape/annotation/0.0",
        "node:Shape/attribute/0.0",
        "node:Shape/attribute/1.0",
        "node:Shape/method/0.0",
        "node:Shape/method/1.0",
        "node:Shape/shape/1",
        "note:note0/shape",
        "node:note0/shape",
        "node:interface0/shape",
        "node:interface0#1/label/0.0",
        "edge:Shape~interface0~none dependency solid/start-label/0.0",
        "edge:Shape~interface0~none dependency solid#1/end-label/0.0",
        "edge:Shape~Shape~none dependency solid/end",
    ] {
        assert!(keys.contains(want), "no drawing has the key {want}; the keys are {keys:?}");
    }
}

#[test]
fn members_are_set_as_their_marks_say() {
    let run = |drawn: &[(String, Option<pele_core::scene::BaselineRun>)], key: &str| {
        drawn.iter().find(|(found, _)| found == key).and_then(|(_, run)| run.clone()).expect(key)
    };
    let drawn = drawings(&scene(SHAPES, &LayoutOptions::default()));
    let name = run(&drawn, "node:Shape/label/0.0");
    assert_eq!(name.font.weight, FontWeight::Bold, "a class's name is a heading");
    let abstract_member = run(&drawn, "node:Shape/method/0.0");
    assert!(abstract_member.font.italic && !abstract_member.underline, "an abstract member is italic");
    let static_member = run(&drawn, "node:Shape/method/1.0");
    assert!(static_member.underline && !static_member.font.italic, "a static member is underlined");
    assert_eq!(static_member.text, "+count() : int");
    let plain = run(&drawn, "node:Shape/attribute/1.0");
    assert_eq!(plain.text, "-List<int> sides");
    assert_eq!(plain.font.family, FontFamily::Sans);
    assert_eq!(plain.font.size, 14.0, "members are set at seven eighths of the size");

    let mono = LayoutOptions { monospace: vec![MonospacePart::Members], ..LayoutOptions::default() };
    let drawn = drawings(&scene(SHAPES, &mono));
    assert_eq!(run(&drawn, "node:Shape/attribute/1.0").font.family, FontFamily::Mono, "the host sets members in mono");
    assert_eq!(run(&drawn, "node:Shape/label/0.0").font.family, FontFamily::Sans, "a class's name stays in its font");
}
