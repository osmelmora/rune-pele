//! A flowchart's Scene read the way an interpreter with no SVG reads it: what each entity draws
//! and in which layer, what a style declaration means, and where links and tooltips are.

use pele_core::scene::{
    EntityKey, EntityKind, EntityName, Facet, FragmentDetail, Geometry, LineCap, Paint, PaintVisit, Palette,
    PathCommand, Role, Scene,
};
use pele_core::{FontWeight, Script};
use std::collections::{BTreeMap, HashSet};

fn scene(source: &str) -> Scene {
    pele_core::parity_scene(source).expect("a flowchart draws").scene().clone()
}

fn named(scene: &Scene, kind: EntityKind, name: &str) -> EntityKey {
    let name: Vec<u16> = name.encode_utf16().collect();
    scene
        .entities()
        .find(|entity| entity.kind() == kind && entity.identity() == name)
        .unwrap_or_else(|| panic!("no {kind:?} named {}", String::from_utf16_lossy(&name)))
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

const CROSSING: &str =
    "flowchart TD\nsubgraph G [Group]\n  A[Inside]\nend\nA e1@-->|yes| B((Out))\nB --> A\nB --> A\nstyle G fill:#036";

#[test]
fn an_edge_draws_in_several_layers_under_one_entity() {
    let keys = keys(&scene(CROSSING));
    let want = [
        "group:G/shape",
        "group:G/title/0.0",
        "edge:e1/casing",
        "edge:e1/casing/1",
        "edge:B~A~arrow_point normal/casing",
        "edge:B~A~arrow_point normal/casing/1",
        "edge:B~A~arrow_point normal#1/casing",
        "edge:B~A~arrow_point normal#1/casing/1",
        "edge:e1/shape",
        "edge:e1/end",
        "edge:B~A~arrow_point normal/shape",
        "edge:B~A~arrow_point normal/end",
        "edge:B~A~arrow_point normal#1/shape",
        "edge:B~A~arrow_point normal#1/end",
        "edge:e1/label",
        "edge:e1/label/0.0",
        "node:A/shape",
        "node:A/label/0.0",
        "node:B/shape",
        "node:B/label/0.0",
    ];
    assert_eq!(keys, want);
}

#[test]
fn an_edge_key_survives_an_edge_added_before_it() {
    let before = scene("flowchart TD\nA --> B\nA --> C");
    let after = scene("flowchart TD\nA --> B\nX --> A\nA --> C");
    let to_c = |scene: &Scene| {
        scene
            .entities()
            .find(|entity| matches!(entity.name(), EntityName::Between { to, .. } if *to == [u16::from(b'C')]))
            .expect("an edge to C")
            .clone()
    };
    assert_eq!(to_c(&before), to_c(&after));
}

#[test]
fn an_edge_is_related_to_its_two_ends() {
    let scene = scene(CROSSING);
    let (a, b) = (named(&scene, EntityKind::Node, "A"), named(&scene, EntityKind::Node, "B"));
    let edge = named(&scene, EntityKind::Edge, "e1");
    assert_eq!(scene.related(&edge), [&a, &b]);
    let of_a = scene.related(&a);
    assert_eq!(of_a.len(), 3, "A has three edges");
    assert!(of_a.contains(&&edge));
}

#[test]
fn a_style_declaration_wins_over_the_attribute_it_follows() {
    let source = "flowchart TD\nA[Styled] -.-> B\nstyle A fill:#f00,stroke:blue,stroke-width:4px,opacity:0.5\nlinkStyle 0 stroke:#0f0";
    let scene = scene(source);
    let paints = scene.resolve_paints(&Palette::pele(), false);
    let colour = |paint: Paint| paints.get(paint).map(|rgba| rgba.to_srgb8());
    let mut seen = 0;
    for visit in scene.paint() {
        let PaintVisit::Draw(draw) = visit else { continue };
        match draw.key.to_string().as_str() {
            "node:A/shape" => {
                let Geometry::Rect { radius, .. } = draw.geometry else { panic!("A is a rectangle") };
                assert_eq!(radius, 4.0, "the theme's radius, at Pele's default");
                assert_eq!(colour(draw.style.fill), Some([255, 0, 0, 255]));
                let stroke = draw.style.stroke.expect("A has a border");
                assert_eq!(colour(stroke.paint), Some([0, 0, 255, 255]));
                assert_eq!(stroke.width, 4.0);
                assert_eq!(draw.style.opacity, 0.5);
                seen += 1;
            }
            "node:B/shape" => {
                assert_eq!(draw.style.fill, Paint::Role(Role::Surface));
                assert_eq!(draw.style.opacity, 1.0);
                seen += 1;
            }
            "edge:A~B~arrow_point dotted/shape" => {
                let stroke = draw.style.stroke.expect("an edge is a stroke");
                assert_eq!(colour(stroke.paint), Some([0, 255, 0, 255]), "the edge's own stroke");
                assert_eq!(stroke.dash, [3.0, 4.0]);
                assert_eq!(stroke.cap, LineCap::Round, "from the edges layer");
                assert_eq!(draw.style.fill, Paint::None);
                seen += 1;
            }
            "edge:A~B~arrow_point dotted/end" => {
                assert_eq!(colour(draw.style.fill), Some([0, 255, 0, 255]), "a marker takes its edge's colour");
                assert!(draw.style.stroke.is_none(), "a marker's own stroke of none wins over its edge's");
                seen += 1;
            }
            "node:A/label/0.0" => {
                assert_eq!(draw.style.fill, Paint::Role(Role::OnLight), "ink that contrasts with the fill");
                seen += 1;
            }
            _ => {}
        }
    }
    assert_eq!(seen, 5);
}

#[test]
fn links_and_tooltips_are_read_without_svg() {
    let source = "flowchart TD\nA[Home] --> B[My Note]:::internal-link\nclick A \"https://example.com/x\" \"A tip #amp; more\" _blank";
    let scene = scene(source);
    let links: Vec<_> = scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Begin(group) => group.link,
            _ => None,
        })
        .collect();
    assert_eq!(links.len(), 2);
    assert_eq!(
        (links[0].href.as_str(), links[0].internal, links[0].target),
        ("https://example.com/x", false, Some("_blank"))
    );
    assert_eq!((links[1].href.as_str(), links[1].internal, links[1].target), ("My Note", true, None));
    assert_eq!(scene.tooltip(&named(&scene, EntityKind::Node, "A")).as_deref(), Some("A tip & more"));
    assert_eq!(scene.tooltip(&named(&scene, EntityKind::Node, "B")), None);
}

fn flowchart_corpus() -> Vec<String> {
    pele_oracle::corpus().into_iter().filter(|(kind, _)| kind == "flowchart").flat_map(|(_, sources)| sources).collect()
}

#[test]
fn every_drawing_of_the_corpus_has_a_key_of_its_own() {
    let sources = flowchart_corpus();
    assert!(sources.len() > 100);
    for source in &sources {
        let scene = scene(source);
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
}

/// What the flowchart corpus asks of an interpreter, counted from the walk. Run with
/// `--nocapture` to read the counts.
#[test]
fn the_corpus_counts_what_an_interpreter_must_draw() {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut count = |name: &'static str, by: usize| *counts.entry(name).or_default() += by;
    for source in &flowchart_corpus() {
        for visit in scene(source).paint() {
            let draw = match visit {
                PaintVisit::Begin(group) => {
                    count("groups", 1);
                    count("groups that are links", usize::from(group.link.is_some()));
                    count("groups with opacity", usize::from(group.opacity < 1.0));
                    count("groups the pointer passes through", usize::from(group.untouchable));
                    continue;
                }
                PaintVisit::End => continue,
                PaintVisit::Draw(draw) => draw,
            };
            count("drawings", 1);
            count("drawings with opacity", usize::from(draw.style.opacity < 1.0));
            count("fills with opacity", usize::from(draw.style.fill != Paint::None && draw.style.fill_opacity < 1.0));
            count("paints a source writes", usize::from(matches!(draw.style.fill, Paint::Source(_))));
            count(
                "paints mixed from two roles",
                usize::from(draw.style.stroke.as_ref().is_some_and(|s| matches!(s.paint, Paint::Mix { .. }))),
            );
            if let Some(stroke) = &draw.style.stroke {
                count("strokes", 1);
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
                    count("paths of several subpaths", usize::from(moves > 1));
                    count("paths filled", usize::from(draw.style.fill != Paint::None));
                    count("paths of several subpaths filled", usize::from(moves > 1 && draw.style.fill != Paint::None));
                }
                Geometry::Rect { radius, .. } => {
                    count("rects", 1);
                    count("rects with round corners", usize::from(*radius > 0.0));
                }
                Geometry::Circle { .. } => count("circles", 1),
                Geometry::Ellipse { .. } => count("ellipses", 1),
                Geometry::Polygon(_) => count("polygons", 1),
                Geometry::Image { .. } => count("images", 1),
                Geometry::Icon { .. } => count("icons", 1),
                Geometry::Text(run) => {
                    count("text runs", 1);
                    count("text runs bold", usize::from(run.font.weight == FontWeight::Bold));
                    count("text runs italic", usize::from(run.font.italic));
                    count("text runs underlined", usize::from(run.underline));
                    count("text runs shifted", usize::from(run.script != Script::Normal));
                    count("text runs stroked", usize::from(draw.style.stroke.is_some()));
                    count("text runs not at the root size", usize::from(run.font.size != 16.0));
                }
            }
            if draw.key.detail != FragmentDetail::Whole && draw.key.part.facet == Facet::Shape {
                count("further shapes of one node", 1);
            }
        }
    }
    for (name, count) in &counts {
        println!("{count:>6}  {name}");
    }
    for needed in ["paths with cubics", "polygons", "strokes dashed", "fills with opacity", "text runs bold"] {
        assert!(counts[needed] > 0, "the corpus has {needed}");
    }
}
