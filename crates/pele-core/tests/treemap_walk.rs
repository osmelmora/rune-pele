//! A treemap's Scene read the way an interpreter with no SVG reads it: each drawing has a key of
//! its own, a tooltip is text with its entity codes read once, and a small box's capped corner is
//! a radius the walk gives.

#[path = "differential/harness.rs"]
mod harness;

use harness::{drawn_keys, harvest, scene};
use pele_core::LayoutOptions;
use pele_core::scene::{Geometry, PaintVisit};

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let narrow = LayoutOptions { max_width: Some(320.0), ..LayoutOptions::default() };
    let mut drawn = 0;
    for source in harvest("treemap", "sources") {
        for options in [&LayoutOptions::default(), &narrow] {
            if let Some(scene) = scene(&source, options) {
                drawn_keys(&scene, &source);
                drawn += 1;
            }
        }
    }
    assert!(drawn > 1500, "the harvest draws");
}

#[test]
fn a_box_under_a_headerless_section_names_it_in_its_tooltip() {
    let source =
        "---\nconfig:\n  treemap:\n    nodeHeight: 3\n---\ntreemap\n\"A &amp; B\"\n  \"Inner\"\n    \"Leaf\": 1";
    let scene = scene(source, &LayoutOptions::default()).expect("it draws");
    let tooltips: Vec<String> = scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Begin(group) => group.tooltip,
            _ => None,
        })
        .collect();
    assert!(tooltips.contains(&"A & B \u{203a} Inner \u{203a} Leaf".to_owned()), "{tooltips:?}");
    assert!(tooltips.contains(&"A & B \u{203a} Inner".to_owned()), "{tooltips:?}");
}

#[test]
fn a_small_box_has_its_corners_capped() {
    let scene = scene("treemap\n\"big\": 1000\n\"small\": 4", &LayoutOptions::default()).expect("it draws");
    let radii: Vec<f64> = scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => match draw.geometry {
                Geometry::Rect { radius, .. } => Some(radius),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert!(radii.contains(&4.0), "a large box keeps the theme's radius: {radii:?}");
    assert!(radii.iter().any(|&radius| radius > 0.0 && radius < 4.0), "a small box caps it: {radii:?}");
    assert!(scene.unread_styles().is_empty(), "the walk reads the capped radius: {:?}", scene.unread_styles());
}
