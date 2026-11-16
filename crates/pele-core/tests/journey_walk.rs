//! A journey's Scene read the way an interpreter with no SVG reads it: each drawing has a key of
//! its own, across and as steps running down, and an actor's dot is a group whose tooltip is the
//! actor's name, drawn by the same entity as the actor's legend row.

#[path = "differential/harness.rs"]
mod harness;

use harness::{drawn_keys, harvest, scene};
use pele_core::LayoutOptions;
use pele_core::scene::{EntityKind, Facet, Geometry, PaintVisit};

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let narrow = LayoutOptions { max_width: Some(320.0), ..LayoutOptions::default() };
    let mut drawn = 0;
    for source in harvest("journey", "sources") {
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
fn an_actors_dot_shows_its_name() {
    let source = "journey\n  section S\n  Task: 5: Me #38; you, Cat\n  Other: 3: Cat";
    let scene = scene(source, &LayoutOptions::default()).expect("it draws");
    let visits: Vec<PaintVisit> = scene.paint().collect();
    let dots: Vec<(String, &Geometry)> = visits
        .windows(3)
        .filter_map(|run| match run {
            [PaintVisit::Begin(group), PaintVisit::Draw(draw), PaintVisit::End] => {
                Some((group.tooltip.clone()?, &draw.geometry))
            }
            _ => None,
        })
        .collect();
    let names: Vec<&str> = dots.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["Me & you", "Cat", "Cat"]);
    assert!(dots.iter().all(|(_, geometry)| matches!(geometry, Geometry::Circle { .. })));

    let cat: Vec<Facet> = visits
        .iter()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw)
                if draw.key.part.entity.kind() == EntityKind::Section
                    && draw.key.part.entity.identity() == "Cat".encode_utf16().collect::<Vec<_>>() =>
            {
                Some(draw.key.part.facet)
            }
            _ => None,
        })
        .collect();
    assert!(cat.contains(&Facet::Shape) && cat.contains(&Facet::Legend), "{cat:?}");
    let actor = scene.entities().find(|entity| entity.kind() == EntityKind::Section).expect("an actor");
    assert_eq!(scene.tooltip(actor).as_deref(), Some("Cat"));
}
