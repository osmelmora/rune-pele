//! An XY chart's Scene read the way an interpreter with no SVG reads it: every drawing has a key
//! of its own, a series keeps its key and its colour by its place, and the left axis title is drawn
//! inside a group that moves it and turns it upright.

mod xychart_random;

use pele_core::scene::{AnchorKind, Geometry, Group, Paint, PaintVisit, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::HashSet;

fn scene(source: &str) -> Scene {
    pele_core::parity_scene_with(source, ParseOptions::default(), &LayoutOptions::default())
        .expect("an XY chart draws")
        .scene()
        .clone()
}

fn keys(scene: &Scene) -> Vec<String> {
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => Some(draw.key.to_string()),
            _ => None,
        })
        .collect()
}

const SALES: &str = "xychart\n  title \"Sales\"\n  x-axis Months [jan, feb, mar]\n  y-axis \"Revenue\" 0 --> 100\n  bar Sales [50, 60, 75]\n  line Trend [40 \"low\", 65, 90]\n  line Trend [10, 20, 30]\n  bar [5, 5, 5]";

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let mut sources = vec![SALES.to_owned()];
    let mut rng = xychart_random::Rng(0x5eed);
    sources.extend((0..300).map(|_| xychart_random::source(&mut rng)));
    let mut drawn = 0;
    for source in &sources {
        let Ok(parity) = pele_core::parity_scene(source) else { continue };
        drawn += 1;
        let keys = keys(parity.scene());
        let mut seen = HashSet::new();
        let repeated: Vec<&String> = keys.iter().filter(|key| !seen.insert(*key)).collect();
        assert!(repeated.is_empty(), "keys drawn twice in {source:?}: {repeated:?}");
    }
    assert!(drawn > 250, "most random charts draw");
    let keys: HashSet<String> = keys(&scene(SALES)).into_iter().collect();
    for want in [
        "diagram:/title/0.0",
        "section:Sales/shape",
        "section:Sales/shape/2",
        "section:Sales/legend/0.0",
        "section:Trend/shape",
        "section:Trend/label/0.0",
        "section:Trend#1/shape",
        "section:/shape",
        "diagram:/label/0.0",
        "diagram:/legend/0.0",
    ] {
        assert!(keys.contains(want), "no drawing keyed {want}: {keys:?}");
    }
}

#[test]
fn a_series_keeps_the_colour_of_its_place() {
    let fills: Vec<Paint> = scene(SALES)
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) if draw.key.to_string().starts_with("section:/shape") => Some(draw.style.fill),
            _ => None,
        })
        .collect();
    assert!(!fills.is_empty(), "the untitled bars draw");
    for fill in fills {
        assert!(matches!(fill, Paint::Series(slot) if slot.index() == 3), "the fourth series' colour: {fill:?}");
    }
}

#[test]
fn the_left_axis_title_is_moved_then_turned_upright() {
    let svg = pele_core::render(SALES).expect("renders").svg;
    let (_, rest) = svg.split_once("<g transform=\"translate(").expect("a turned group");
    let (point, rest) = rest.split_once(") rotate(-90)\">").expect("a turn after the move");
    let (x, y) = point.split_once(',').expect("a point");
    let (x, y): (f64, f64) = (x.parse().expect("x"), y.parse().expect("y"));
    assert!(rest.starts_with("<text x=\"0\""), "the title is centred on the group's origin: {}", &rest[..60]);

    let mut groups: Vec<Group> = Vec::new();
    let mut found = None;
    for visit in scene(SALES).paint() {
        match visit {
            PaintVisit::Begin(group) => groups.push(group),
            PaintVisit::End => {
                groups.pop();
            }
            PaintVisit::Draw(draw) => {
                if let Geometry::Text(run) = &draw.geometry
                    && run.text == "Revenue"
                {
                    found = Some((groups.last().cloned().expect("inside a group"), draw.origin, run.clone()));
                }
            }
        }
    }
    let (group, origin, run) = found.expect("the left axis title");
    let turn = group.rotate.expect("the group turns");
    assert_eq!((group.translate.x, group.translate.y), (x, y), "the group moves to the SVG's point");
    assert_eq!((turn.degrees, turn.centre.x, turn.centre.y), (-90.0, x, y), "and turns about it");
    assert_eq!((origin.x, origin.y), (x, y), "the title's origin is the point it moved to");
    assert_eq!((run.baseline.x, run.anchor), (0.0, AnchorKind::Middle));
}
