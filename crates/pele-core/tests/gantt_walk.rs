//! A gantt chart's Scene read the way an interpreter with no SVG reads it: what each task,
//! section and marker draws and in which layer, how tasks are keyed across edits, and what a
//! Rune lowering of gantt will have to draw.

use pele_core::LayoutOptions;
use pele_core::scene::{Geometry, LineJoin, Paint, PaintVisit, Role, Scene};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

fn scene(source: &str) -> Scene {
    pele_core::parity_scene(source).expect("a gantt chart draws").scene().clone()
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

const CHART: &str = "gantt\n  dateFormat YYYY-MM-DD\n  title Plan\n  section Build\n  Design :done, a1, 2026-01-01, 10d\n  Code :active, crit, after a1, 12d\n  section Ship\n  Launch :milestone, m1, 2026-01-25, 0d\n  Freeze :vert, v1, 2026-01-20, 0d\n  click a1 href \"https://example.com\"\n";

#[test]
fn each_task_section_and_marker_draws_under_its_own_entity() {
    let keys = keys(&scene(CHART));
    let want = [
        "diagram:/title/0.0",
        "group:Build/shape",
        "group:Ship/shape",
        "diagram:/shape",
        "diagram:/shape/1",
        "node:v1/shape",
        "diagram:/shape/2",
        "diagram:/shape/3",
        "node:a1/shape",
        "node:a1/label/0.0",
        "node:Code/shape",
        "node:Code/label/0.0",
        "node:m1/shape",
        "node:m1/label/0.0",
        "group:Build/label/0.0",
        "group:Ship/label/0.0",
        "diagram:/label/0.0",
        "diagram:/label/1/0.0",
        "diagram:/label/2/0.0",
        "diagram:/label/3/0.0",
        "diagram:/label/4/0.0",
        "diagram:/label/5/0.0",
        "diagram:/label/6/0.0",
        "node:v1/label/0.0",
    ];
    assert_eq!(keys, want);
}

#[test]
fn a_task_keeps_its_key_when_a_task_is_added_before_it() {
    let before = scene("gantt\n  A :a1, 2026-01-01, 3d\n  Unnamed :2026-01-02, 2d\n");
    let after = scene("gantt\n  New :2026-01-01, 1d\n  A :a1, 2026-01-01, 3d\n  Unnamed :2026-01-02, 2d\n");
    let entities = |scene: &Scene| scene.entities().cloned().collect::<HashSet<_>>();
    let (before, after) = (entities(&before), entities(&after));
    assert!(before.is_subset(&after), "every task keeps its key: {before:?} in {after:?}");
}

#[test]
fn tasks_with_one_name_rank_apart() {
    let keys = keys(&scene("gantt\n  A :a, 2026-01-01, 3d\n  A :a, 2026-01-02, 2d\n  a :2026-01-03, 1d\n"));
    let shapes: Vec<&String> = keys.iter().filter(|key| key.starts_with("node:") && key.ends_with("/shape")).collect();
    assert_eq!(
        shapes,
        ["node:a/shape", "node:a#1/shape", "node:a#2/shape"],
        "the unnamed `a` ranks after the two named ones"
    );
}

#[test]
fn a_task_is_drawn_in_its_state_and_a_link_is_read_without_svg() {
    let scene = scene(CHART);
    let mut seen = 0;
    let mut links = Vec::new();
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => links.extend(group.link),
            PaintVisit::Draw(draw) => match draw.key.to_string().as_str() {
                "node:a1/shape" => {
                    let Geometry::Rect { radius, .. } = draw.geometry else { panic!("a task is a bar") };
                    assert_eq!(radius, 4.0, "the theme's radius");
                    assert_eq!(draw.style.fill, Paint::Role(Role::SurfaceAlt), "done work keeps the plain fill");
                    assert_eq!(draw.style.stroke.expect("a border").paint, Paint::Role(Role::Border));
                    seen += 1;
                }
                "node:Code/shape" => {
                    assert_eq!(draw.style.fill, Paint::Role(Role::Critical));
                    assert_eq!(draw.style.fill_opacity, 0.25, "active work is a wash");
                    seen += 1;
                }
                "node:m1/shape" => {
                    let Geometry::Path(commands) = &draw.geometry else { panic!("a milestone is a diamond") };
                    assert_eq!(commands.len(), 5);
                    assert_eq!(draw.style.stroke.as_ref().expect("a border").join, LineJoin::Round);
                    seen += 1;
                }
                "node:v1/shape" => {
                    let stroke = draw.style.stroke.expect("a marker is a line");
                    assert_eq!(
                        (stroke.paint, stroke.width, stroke.dash.as_slice()),
                        (Paint::Role(Role::Line), 1.5, &[4.0, 3.0][..])
                    );
                    seen += 1;
                }
                _ => {}
            },
            PaintVisit::End => {}
        }
    }
    assert_eq!(seen, 4);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].href, "https://example.com/", "as Pele's safeUrl normalizes it");
}

fn harvest() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/gantt/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).expect("the harvest is JSON");
    harvest["sources"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().expect("a string").to_owned())
        .collect()
}

/// Every drawing of every gantt chart the corpus and the harvest hold has a key of its own, and
/// what they draw is counted: what a Rune lowering of gantt has to draw.
#[test]
fn every_drawing_has_a_key_of_its_own() {
    let mut sources: Vec<String> = pele_oracle::corpus()
        .into_iter()
        .filter(|(kind, _)| kind == "gantt")
        .flat_map(|(_, sources)| sources)
        .collect();
    sources.extend(harvest());
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut drawn = 0;
    for source in &sources {
        let options = LayoutOptions::default();
        let Ok(parity) = pele_core::parity_scene_with(source, Default::default(), &options) else { continue };
        if parity.scene().entities().next().is_none() {
            continue;
        }
        drawn += 1;
        let mut seen = HashSet::new();
        for visit in parity.scene().paint() {
            let mut count = |what: &'static str, yes: bool| *counts.entry(what).or_default() += usize::from(yes);
            match visit {
                PaintVisit::Begin(group) => {
                    count("groups that are links", group.link.is_some());
                    count("groups with opacity", group.opacity < 1.0);
                }
                PaintVisit::Draw(draw) => {
                    let key = draw.key.to_string();
                    assert!(seen.insert(key.clone()), "{key} is drawn twice in {source:?}");
                    let stroke = draw.style.stroke.as_ref();
                    count("dashed strokes", stroke.is_some_and(|stroke| !stroke.dash.is_empty()));
                    count("round joins", stroke.is_some_and(|stroke| stroke.join == LineJoin::Round));
                    count("translucent fills", draw.style.fill_opacity < 1.0);
                    count("faded drawings", draw.style.opacity < 1.0);
                    match &draw.geometry {
                        Geometry::Rect { radius, .. } => {
                            count("rectangles", true);
                            count("rounded rectangles", *radius > 0.0);
                        }
                        Geometry::Path(_) => count("paths", true),
                        Geometry::Text(run) => {
                            count("text runs", true);
                            count(
                                "bold or italic runs",
                                run.font.weight != pele_core::FontWeight::Normal || run.font.italic,
                            );
                        }
                        Geometry::Circle { .. } | Geometry::Ellipse { .. } | Geometry::Polygon(_) => {
                            count("circles, ellipses or polygons", true)
                        }
                        Geometry::Image { .. } | Geometry::Icon { .. } => count("images or icons", true),
                    }
                    count("critical paint", draw.style.fill == Paint::Role(Role::Critical));
                }
                PaintVisit::End => {}
            }
        }
    }
    println!("{drawn} gantt charts walked");
    for (what, count) in &counts {
        println!("  {count:6} {what}");
    }
    assert!(drawn > 1000, "the corpus and the harvest hold gantt charts");
}
