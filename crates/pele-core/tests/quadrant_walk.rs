//! A quadrant chart's Scene read the way an interpreter with no SVG reads it: every drawing has a
//! key of its own, a point's circle and label are one part even when two points share a name,
//! and a y-axis label is a group that moves to its place and turns about it.

use pele_core::scene::{Geometry, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::HashSet;

fn scene(source: &str) -> Scene {
    pele_core::parity_scene_with(source, ParseOptions::default(), &LayoutOptions::default())
        .expect("a quadrant chart draws")
        .scene()
        .clone()
}

const CHART: &str = "quadrantChart\n  title Reach\n  x-axis Low --> High\n  y-axis Down --> Up\n  quadrant-1 Expand\n  quadrant-3 Re-evaluate\n  Campaign A: [0.25, 0.75]\n  Campaign A: [0.3, 0.7] radius: 9\n  Campaign B:::hot: [0.75, 0.25]\n  classDef hot color: #c00";

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let keys: Vec<String> = scene(CHART)
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => Some(draw.key.to_string()),
            _ => None,
        })
        .collect();
    let mut seen = HashSet::new();
    let repeated: Vec<&String> = keys.iter().filter(|key| !seen.insert(*key)).collect();
    assert!(repeated.is_empty(), "keys drawn twice: {repeated:?}");
    let keys: HashSet<&str> = keys.iter().map(String::as_str).collect();
    for want in [
        "diagram:/title/0.0",
        "group:1/shape",
        "group:1/label/0.0",
        "group:2/shape",
        "group:x-axis/start-label/0.0",
        "group:x-axis/end-label/0.0",
        "group:y-axis/start-label/0.0",
        "group:y-axis/end-label/0.0",
        "node:Campaign A/shape",
        "node:Campaign A/label/0.0",
        "node:Campaign A#1/shape",
        "node:Campaign A#1/label/0.0",
        "node:Campaign B/shape",
    ] {
        assert!(keys.contains(want), "no drawing has the key {want}; the keys are {keys:?}");
    }
}

#[test]
fn a_y_axis_label_moves_to_its_place_and_turns_about_it() {
    let visits: Vec<PaintVisit> = scene(CHART).paint().collect();
    let mut turned = 0;
    for (at, visit) in visits.iter().enumerate() {
        let PaintVisit::Begin(group) = visit else { continue };
        let Some(rotation) = group.rotate else { continue };
        turned += 1;
        assert_eq!(rotation.degrees, -90.0, "the label reads upwards");
        assert_eq!(rotation.centre, group.translate, "the turn is about the point the label moves to");
        let PaintVisit::Draw(draw) = &visits[at + 1] else { panic!("the group holds the label") };
        assert_eq!(draw.origin, group.translate, "the label is drawn from the point it moves to");
        let Geometry::Text(run) = &draw.geometry else { panic!("the label is text") };
        assert_eq!(run.baseline.x, 0.0, "the label is centred on the point before it turns");
    }
    assert_eq!(turned, 2, "both ends of the y axis are turned");
}
