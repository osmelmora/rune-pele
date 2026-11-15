//! An agentflow's Scene read the way an interpreter with no SVG reads it: every drawing has a
//! key of its own, a node is drawn in the series colour of its kind, and the counts say what an
//! interpreter must be able to draw.

use pele_core::scene::{
    EntityKind, FragmentDetail, Geometry, LineCap, Paint, PaintVisit, PathCommand, Scene, SeriesSlot,
};
use pele_core::{FontWeight, RenderError};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

fn scene(source: &str) -> Result<Scene, RenderError> {
    pele_core::parity_scene(source).map(|parity| parity.scene().clone())
}

/// The documentation corpus, and every source of Pele's tests that draws.
fn sources() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/agentflow/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvested = harvest["sources"].as_array().expect("a list").iter();
    harvested.map(|item| item.as_str().expect("a string").to_owned()).collect()
}

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let mut drawn = 0;
    for source in &sources() {
        let Ok(scene) = scene(source) else {
            continue;
        };
        drawn += 1;
        let (mut depth, mut seen) = (0i32, HashSet::new());
        for visit in scene.paint() {
            match visit {
                PaintVisit::Begin(_) => depth += 1,
                PaintVisit::End => depth -= 1,
                PaintVisit::Draw(draw) => {
                    let key = draw.key.to_string();
                    assert!(seen.insert(key.clone()), "{key} is drawn twice in {source:?}");
                }
            }
            assert!(depth >= 0);
        }
        assert_eq!(depth, 0, "every group closes");
    }
    assert!(drawn > 2000, "the harvest holds agentflows that draw");
}

#[test]
fn a_node_is_drawn_in_the_colour_of_its_kind() {
    let source = "agentflow-beta TB\n  flow f[\"F\"]\n    t@{ shape: tool } --> d@{ shape: decision }\n  end\n  flow g@{ view: collapsed }\n    x\n  end\n  connector k\n  d --> k\n  k --> g\n";
    let scene = scene(source).expect("the agentflow draws");
    let mut fills: BTreeMap<String, (Paint, f64)> = BTreeMap::new();
    for visit in scene.paint() {
        if let PaintVisit::Draw(draw) = visit
            && draw.key.detail == FragmentDetail::Whole
            && matches!(draw.geometry, Geometry::Rect { .. } | Geometry::Polygon(_))
            && matches!(draw.key.part.entity.kind(), EntityKind::Node | EntityKind::Group)
        {
            fills.insert(draw.key.to_string(), (draw.style.fill, draw.style.fill_opacity));
        }
    }
    let series = |slot: usize| Paint::Series(SeriesSlot::new(slot));
    for (key, slot, wash) in [
        ("node:t/shape", 0, 0.14),
        ("node:d/shape", 2, 0.14),
        ("node:k/shape", 5, 0.14),
        ("group:f/shape", 7, 0.06),
        ("node:g/shape", 7, 0.06),
    ] {
        assert_eq!(fills.get(key), Some(&(series(slot), wash)), "{key} among {fills:?}");
    }
    let edges: Vec<_> = scene.entities().filter(|entity| entity.kind() == EntityKind::Edge).collect();
    assert_eq!(edges.len(), 3);
    assert_eq!(scene.related(edges[0]).len(), 2, "an edge joins two nodes");
}

/// What agentflows ask of an interpreter, counted from the walk. Run with `--nocapture` to read
/// the counts.
#[test]
fn the_sources_count_what_an_interpreter_must_draw() {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut count = |name: &'static str, by: usize| *counts.entry(name).or_default() += by;
    for source in &sources() {
        let Ok(scene) = scene(source) else {
            continue;
        };
        count("agentflows", 1);
        for visit in scene.paint() {
            let draw = match visit {
                PaintVisit::Begin(group) => {
                    count("groups", 1);
                    count("groups that are links", usize::from(group.link.is_some()));
                    count("groups the pointer passes through", usize::from(group.untouchable));
                    continue;
                }
                PaintVisit::End => continue,
                PaintVisit::Draw(draw) => draw,
            };
            count("drawings", 1);
            count("drawings with opacity", usize::from(draw.style.opacity < 1.0));
            count("fills in a series colour", usize::from(matches!(draw.style.fill, Paint::Series(_))));
            count("fills with opacity", usize::from(draw.style.fill != Paint::None && draw.style.fill_opacity < 1.0));
            count("fills a source colours", usize::from(matches!(draw.style.fill, Paint::Source(_))));
            if let Some(stroke) = &draw.style.stroke {
                count("strokes", 1);
                count("strokes in a series colour", usize::from(matches!(stroke.paint, Paint::Series(_))));
                count("strokes dashed", usize::from(!stroke.dash.is_empty()));
                count("strokes with round caps", usize::from(stroke.cap == LineCap::Round));
                count("strokes wider than 1", usize::from(stroke.width > 1.0));
                count("strokes a source colours", usize::from(matches!(stroke.paint, Paint::Source(_))));
            }
            match &draw.geometry {
                Geometry::Path(commands) => {
                    count("paths", 1);
                    let has = |find: fn(&PathCommand) -> bool| usize::from(commands.iter().any(find));
                    count("paths with cubics", has(|c| matches!(c, PathCommand::Cubic { .. })));
                    count("paths with quadratics", has(|c| matches!(c, PathCommand::Quad { .. })));
                    count("paths with arcs", has(|c| matches!(c, PathCommand::Arc { .. })));
                    let moves = commands.iter().filter(|c| matches!(c, PathCommand::Move(_))).count();
                    count("paths of several subpaths filled", usize::from(moves > 1 && draw.style.fill != Paint::None));
                }
                Geometry::Rect { radius, .. } => {
                    count("rects", 1);
                    count("rects with round corners", usize::from(*radius > 0.0));
                }
                Geometry::Polygon(_) => count("polygons", 1),
                Geometry::Circle { .. } | Geometry::Ellipse { .. } => count("circles and ellipses", 1),
                Geometry::Image { .. } | Geometry::Icon { .. } => count("images and icons", 1),
                Geometry::Text(run) => {
                    count("text runs", 1);
                    count("text runs bold", usize::from(run.font.weight == FontWeight::Bold));
                    count("text runs italic", usize::from(run.font.italic));
                    count("text runs not at the root size", usize::from(run.font.size != 16.0));
                }
            }
        }
    }
    for (name, count) in &counts {
        println!("{count:>7}  {name}");
    }
    let needed = [
        "fills in a series colour",
        "fills with opacity",
        "strokes in a series colour",
        "strokes dashed",
        "paths with cubics",
        "paths with arcs",
        "paths of several subpaths filled",
        "polygons",
        "rects with round corners",
        "groups that are links",
    ];
    for needed in needed {
        assert!(counts.get(needed).copied().unwrap_or(0) > 0, "the sources have {needed}");
    }
    for absent in ["circles and ellipses", "images and icons"] {
        assert_eq!(counts.get(absent), None, "an agentflow draws no {absent}");
    }
}
