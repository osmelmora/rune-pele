//! A wardley map's Scene read the way an interpreter with no SVG reads it: every drawing has a
//! key of its own, the turned axis title is drawn inside a turned group, and text written from
//! its start lands where the SVG puts it.

mod wardley_random;

use pele_core::scene::{AnchorKind, BaselineRun, Geometry, PaintVisit, Rotation, Scene};
use pele_core::{LayoutOptions, ParseOptions};
use std::collections::HashSet;

fn scene(source: &str) -> Scene {
    pele_core::parity_scene_with(source, ParseOptions::default(), &LayoutOptions::default())
        .expect("a wardley map draws")
        .scene()
        .clone()
}

/// Each drawing's key with its text, where it is text, and the turn of the group around it.
fn drawings(scene: &Scene) -> Vec<(String, Option<BaselineRun>, Option<Rotation>)> {
    let mut turns: Vec<Option<Rotation>> = Vec::new();
    let mut out = Vec::new();
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => turns.push(group.rotate),
            PaintVisit::End => {
                turns.pop();
            }
            PaintVisit::Draw(draw) => {
                let text = match draw.geometry {
                    Geometry::Text(run) => Some(run),
                    _ => None,
                };
                out.push((draw.key.to_string(), text, turns.iter().rev().find_map(|turn| *turn)));
            }
        }
    }
    out
}

const TEA: &str = "wardley-beta\ntitle Tea Shop\nanchor Business [0.95, 0.63]\ncomponent Cup of Tea [0.79, 0.61]\ncomponent Tea [0.63, 0.81] (buy) inertia\ncomponent Kettle [0.43, 0.35]\npipeline Kettle {\n  component Campfire [0.35]\n  component Electric [0.53]\n}\nBusiness -> Cup of Tea\nCup of Tea +<> Tea; flows\nTea -.-> Kettle\nevolve Kettle 0.62\nnote \"Standardising<br>power\" [0.30, 0.49]\naccelerator Pressure [0.6, 0.2]\ndeaccelerator Pressure [0.2, 0.2]\nannotation 1,[0.48, 0.85] \"one\"\nannotation 1,[0.9, 0.3] \"again\"";

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let mut sources = vec![TEA.to_owned()];
    let mut rng = wardley_random::Rng(0x5eed);
    sources.extend((0..300).map(|_| wardley_random::source(&mut rng)));
    let mut drawn_maps = 0;
    for source in &sources {
        let Ok(parity) = pele_core::parity_scene(source) else { continue };
        drawn_maps += 1;
        let drawn = drawings(parity.scene());
        let mut seen = HashSet::new();
        let repeated: Vec<&String> = drawn.iter().map(|(key, ..)| key).filter(|key| !seen.insert(*key)).collect();
        assert!(repeated.is_empty(), "keys drawn twice in {source:?}: {repeated:?}");
    }
    assert!(drawn_maps > 200, "most random maps draw");
    let keys: HashSet<String> = drawings(&scene(TEA)).into_iter().map(|(key, ..)| key).collect();
    for want in [
        "diagram:/title/0.0",
        "node:Business/label/0.0",
        "node:Tea/shape",
        "node:Tea/shape/1",
        "edge:Cup of Tea~Tea~bidirectional/start",
        "edge:Cup of Tea~Tea~bidirectional/end",
        "edge:Cup of Tea~Tea~bidirectional/label/0.0",
        "edge:Tea~Kettle~plain dashed/shape",
        "edge:Kettle~Kettle~evolve/end",
        "group:Kettle/shape",
        "note:1/shape",
        "note:1#1/shape",
        "note:Pressure#1/label/0.0",
    ] {
        assert!(keys.contains(want), "no drawing keyed {want}: {keys:?}");
    }
}

#[test]
fn the_visibility_title_is_drawn_inside_a_group_turned_upright() {
    let svg = pele_core::render(TEA).expect("renders").svg;
    let tag = svg.split("<text class=\"pele-wardley-axis-label\"").nth(2).expect("a second axis title");
    let attribute = |name: &str| -> f64 {
        let (_, rest) = tag.split_once(&format!(" {name}=\"")).expect(name);
        rest[..rest.find('"').expect("closed")].parse().expect("a number")
    };
    let centre_y: f64 = {
        let (_, rest) = tag.split_once("rotate(-90 -16 ").expect("a turn");
        rest[..rest.find(')').expect("closed")].parse().expect("a number")
    };
    let drawn = drawings(&scene(TEA));
    let (_, run, turn) = drawn
        .iter()
        .find(|(_, run, _)| run.as_ref().is_some_and(|run| run.text == "Visibility"))
        .expect("the visibility title");
    let (run, turn) = (run.as_ref().expect("text"), turn.expect("a turn"));
    assert_eq!((turn.degrees, turn.centre.x, turn.centre.y), (-90.0, -16.0, centre_y));
    assert_eq!((run.baseline.x, run.baseline.y), (attribute("x"), attribute("y")));
    assert_eq!(run.anchor, AnchorKind::Middle);
    let level = drawn.iter().find(|(_, run, _)| run.as_ref().is_some_and(|run| run.text == "Evolution"));
    assert!(level.is_some_and(|(_, _, turn)| turn.is_none()), "the evolution title is level");
}

#[test]
fn text_written_from_its_start_lands_where_the_svg_puts_its_lines() {
    let svg = pele_core::render(TEA).expect("renders").svg;
    let note = svg.split("<text class=\"pele-wardley-note\"").nth(1).expect("a note");
    let note = &note[..note.find("</text>").expect("closed")];
    assert!(!note.contains("text-anchor"), "the note writes no anchor: {note}");
    let points: Vec<(f64, f64)> = note
        .split("<tspan x=\"")
        .skip(1)
        .map(|rest| {
            let (x, rest) = rest.split_once("\" y=\"").expect("a point");
            (x.parse().expect("x"), rest[..rest.find('"').expect("y")].parse().expect("y"))
        })
        .collect();
    assert_eq!(points.len(), 2, "two lines: {note}");
    let drawn = drawings(&scene(TEA));
    for (text, (x, y)) in ["Standardising", "power"].into_iter().zip(points) {
        let run = drawn
            .iter()
            .filter_map(|(_, run, _)| run.as_ref())
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("no run {text:?}"));
        assert_eq!((run.baseline.x, run.baseline.y, run.anchor), (x, y, AnchorKind::Start), "{text}");
    }
}

#[test]
fn a_component_keeps_its_key_when_text_before_it_changes() {
    let key = |source: &str| -> String {
        drawings(&scene(source))
            .into_iter()
            .find(|(_, run, _)| run.as_ref().is_some_and(|run| run.text == "Kettle"))
            .map(|(key, ..)| key)
            .expect("the kettle's label")
    };
    let edited = TEA.replace("title Tea Shop", "title A much longer title for the shop").replace("Cup of Tea", "Mug");
    assert_eq!(key(TEA), key(&edited));
}
