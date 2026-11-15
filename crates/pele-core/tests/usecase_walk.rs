//! A use case diagram's Scene read the way an interpreter with no SVG reads it: what each entity
//! draws, what joins what, and what the diagrams ask an interpreter to be able to draw.

use pele_core::scene::{
    EntityKey, EntityKind, FragmentDetail, Geometry, LineCap, Paint, PaintVisit, Palette, PathCommand, Role, Scene,
};
use pele_core::{FontWeight, Script};
use std::collections::{BTreeMap, HashSet};

fn scene(source: &str) -> Scene {
    pele_core::parity_scene(source).expect("a use case diagram draws").scene().clone()
}

fn named(scene: &Scene, kind: EntityKind, name: &str) -> EntityKey {
    let name: Vec<u16> = name.encode_utf16().collect();
    scene
        .entities()
        .find(|entity| entity.kind() == kind && entity.identity() == name)
        .unwrap_or_else(|| panic!("no {kind:?} named {}", String::from_utf16_lossy(&name)))
        .clone()
}

const SHOP: &str = "usecase-beta\ndirection TB\nclassDef dark fill:#123\nactor Customer\nactor Clerk@{ type: hollow, business: true } <<staff>>\nsystemBoundary Shop[The shop]:::dark\n  Browse(Browse)\n  Order(Order)@{ business: true }\n  Pay[Pay]\nend\nCustomer --> Browse\nCustomer pays@--> Pay\nOrder --|> Browse\nOrder ..> : include Pay\nClerk -- handles --> Clerk\nnote for Pay \"Cards only\"\njson Card@{\"number\": \"4111\", \"tags\": [\"a\", \"b\"]}\nPay --> Card";

#[test]
fn every_kind_of_element_is_an_entity() {
    let scene = scene(SHOP);
    for node in ["Customer", "Clerk", "Browse", "Order", "Pay", "Card", "note-0"] {
        named(&scene, EntityKind::Node, node);
    }
    named(&scene, EntityKind::Group, "Shop");
    let pays = named(&scene, EntityKind::Edge, "pays");
    let (customer, pay) = (named(&scene, EntityKind::Node, "Customer"), named(&scene, EntityKind::Node, "Pay"));
    assert_eq!(scene.related(&pays), [&customer, &pay]);
    assert_eq!(scene.related(&pay).len(), 4, "Pay has two relationships, an include and a note's connector");
}

#[test]
fn a_hollow_marker_on_a_fill_shows_the_fill() {
    let scene = scene(SHOP);
    let paints = scene.resolve_paints(&Palette::pele(), false);
    let colour = |paint: Paint| paints.get(paint).map(|rgba| rgba.to_srgb8());
    let marker = scene
        .paint()
        .find_map(|visit| match visit {
            PaintVisit::Draw(draw) if draw.key.to_string().starts_with("edge:Order~Browse~generalization/end") => {
                Some(draw)
            }
            _ => None,
        })
        .expect("the generalization has a marker at its end");
    assert_eq!(colour(marker.style.fill), Some([0x11, 0x22, 0x33, 255]), "the boundary's fill shows inside it");
    let stroke = marker.style.stroke.expect("a hollow marker is an outline");
    assert_eq!(stroke.paint, Paint::Role(Role::OnDark), "in the ink the edge takes on the fill");
}

/// The keys of everything `entity` draws, in paint order.
fn keys_of(scene: &Scene, entity: &str) -> Vec<String> {
    let keys = scene.paint().filter_map(|visit| match visit {
        PaintVisit::Draw(draw) => Some(draw.key.to_string()),
        _ => None,
    });
    keys.filter(|key| key.starts_with(entity)).collect()
}

#[test]
fn a_stereotype_and_the_cells_of_a_table_have_keys_apart_from_the_label() {
    let scene = scene(SHOP);
    let clerk = [
        "node:Clerk/shape",
        "node:Clerk/shape/1",
        "node:Clerk/shape/2",
        "node:Clerk/stereotype/0.0",
        "node:Clerk/label/0.0",
    ];
    assert_eq!(keys_of(&scene, "node:Clerk/"), clerk, "a head, a body, a slash, then the two texts");
    let card = [
        "node:Card/shape",
        "node:Card/shape/1",
        "node:Card/label/0.0",
        "node:Card/row/0.0",
        "node:Card/row/1/0.0",
        "node:Card/row/2/0.0",
        "node:Card/row/3/0.0",
        "node:Card/row/4/0.0",
    ];
    assert_eq!(keys_of(&scene, "node:Card/"), card, "a box, its lines, its name, then a key and a value per row");
}

fn usecase_sources() -> Vec<String> {
    let corpus =
        pele_oracle::corpus().into_iter().filter(|(kind, _)| kind == "usecase").flat_map(|(_, sources)| sources);
    corpus.chain([SHOP.to_owned()]).collect()
}

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let sources = usecase_sources();
    assert!(sources.len() > 20);
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

/// What use case diagrams ask of an interpreter, counted from the walk. Run with `--nocapture`
/// to read the counts.
#[test]
fn the_sources_count_what_an_interpreter_must_draw() {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut count = |name: &'static str, by: usize| *counts.entry(name).or_default() += by;
    for source in &usecase_sources() {
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
            count("fills a source colours", usize::from(matches!(draw.style.fill, Paint::Source(_))));
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
                    count("text runs not at the root size", usize::from(run.font.size != 16.0));
                }
            }
            if draw.key.detail != FragmentDetail::Whole {
                count("further drawings of one facet", 1);
            }
        }
    }
    for (name, count) in &counts {
        println!("{count:>6}  {name}");
    }
    for needed in ["paths with cubics", "ellipses", "circles", "strokes dashed", "fills with opacity", "text runs bold"]
    {
        assert!(counts.get(needed).copied().unwrap_or(0) > 0, "the sources have {needed}");
    }
}
