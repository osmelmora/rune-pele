//! The Scene's paint walk read the way an interpreter reads it: paint order, inherited style,
//! keys and the numbers the SVG spells.

use pele_core::scene::{
    AnchorKind, EntityKind, Facet, FragmentDetail, Geometry, LineJoin, Paint, PaintVisit, Palette, PathCommand, Point,
    Rgba, Role, SeriesSlot,
};
use pele_core::{Font, LayoutOptions, NativeInputs, ParseOptions, TextMeasure, TextMeasurer, TextRef, prepare};
use std::f64::consts::TAU;

const PETS: &str = include_str!("../../../tests/fixtures/pie-pets.mmd");
const SHARES: [(&str, f64); 3] = [("Dogs", 386.0), ("Cats", 85.0), ("Rats", 15.0)];

fn visits(source: &str) -> Vec<PaintVisit> {
    pele_core::parity_scene(source).expect("a pie draws").scene().paint().collect()
}

fn keys(visits: &[PaintVisit]) -> Vec<String> {
    visits
        .iter()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => Some(draw.key.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_walk_keeps_pele_paint_order_and_keys_each_drawing_by_what_it_draws() {
    let keys = keys(&visits(PETS));
    let want = [
        "diagram:/title/0.0",
        "section:Dogs/shape",
        "section:Cats/shape",
        "section:Rats/shape",
        "section:Dogs/label/0.0",
        "section:Cats/label/0.0",
        "section:Rats/label/0.0",
        "section:Dogs/legend",
        "section:Dogs/legend/0.0",
        "section:Cats/legend",
        "section:Cats/legend/0.0",
        "section:Rats/legend",
        "section:Rats/legend/0.0",
    ];
    assert_eq!(keys, want);
}

#[test]
fn groups_open_and_close_around_what_they_hold() {
    let mut depth = 0i32;
    let mut deepest = 0;
    for visit in visits(PETS) {
        match visit {
            PaintVisit::Begin(group) => {
                assert_eq!(group.opacity, 1.0, "pie has no group opacity");
                depth += 1;
                deepest = deepest.max(depth);
            }
            PaintVisit::End => depth -= 1,
            PaintVisit::Draw(_) => {}
        }
        assert!(depth >= 0, "no End before its Begin");
    }
    assert_eq!((depth, deepest), (0, 2), "balanced, with legend items inside the legend");
}

#[test]
fn slices_inherit_the_seam_and_end_where_their_share_of_a_turn_ends() {
    let total: f64 = SHARES.iter().map(|(_, value)| value).sum();
    let mut end_share = 0.0;
    let slices: Vec<_> = visits(PETS)
        .into_iter()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) if matches!(draw.geometry, Geometry::Path(_)) => Some(draw),
            _ => None,
        })
        .collect();
    assert_eq!(slices.len(), 3);
    for (index, (draw, (label, value))) in slices.iter().zip(SHARES).enumerate() {
        assert_eq!(draw.key.part.entity.kind(), EntityKind::Section);
        assert_eq!(String::from_utf16_lossy(draw.key.part.entity.identity()), label);
        assert_eq!(draw.style.fill, Paint::Series(SeriesSlot::new(index)));
        let stroke = draw.style.stroke.expect("the slices layer strokes every slice");
        assert_eq!((stroke.paint, stroke.width, stroke.join), (Paint::Role(Role::Background), 1.0, LineJoin::Round));
        assert!(draw.origin.x > 120.0 && draw.origin.y > 120.0, "origin is the plot's centre: {:?}", draw.origin);
        let Geometry::Path(commands) = &draw.geometry else { unreachable!() };
        let Some(&PathCommand::Arc { rx, ry, end, .. }) =
            commands.iter().find(|c| matches!(c, PathCommand::Arc { .. }))
        else {
            panic!("a slice has an arc: {commands:?}")
        };
        assert_eq!((rx, ry), (120.0, 120.0));
        end_share += value / total;
        let want = Point { x: (end_share * TAU).sin() * 120.0, y: -(end_share * TAU).cos() * 120.0 };
        assert!(
            (end.x - want.x).abs() <= 0.005 && (end.y - want.y).abs() <= 0.005,
            "{label} ends at {end:?}, its share's end is {want:?}"
        );
    }
}

#[test]
fn text_inherits_its_size_anchor_and_ink_from_its_groups() {
    let runs: Vec<_> = visits(PETS)
        .into_iter()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => match draw.geometry {
                Geometry::Text(run) => Some((draw.key, draw.style.fill, run)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let (_, fill, title) = &runs[0];
    assert_eq!(
        (title.text.as_str(), title.font.size, title.font.weight, *fill),
        ("Pets adopted by volunteers", 16.0, pele_core::FontWeight::Bold, Paint::Role(Role::Foreground))
    );
    let width = pele_core::parity_scene(PETS).expect("draws").scene().size().width;
    assert_eq!(title.anchor, AnchorKind::Start, "a bold title is rich text, placed run by run");
    assert_eq!(title.baseline.x + title.advance / 2.0, width / 2.0, "the title is centred on the diagram");
    let percent: Vec<_> = runs[1..4].iter().map(|(_, fill, run)| (run.text.as_str(), *fill, run.font.size)).collect();
    assert_eq!(
        percent,
        [
            ("79%", Paint::OnSeries(SeriesSlot::new(0)), 14.0),
            ("17%", Paint::OnSeries(SeriesSlot::new(1)), 14.0),
            ("3%", Paint::Role(Role::Foreground), 14.0),
        ],
        "inside labels are ink on their series; the one outside the rim inherits the diagram's ink"
    );
    for (key, fill, run) in &runs[4..] {
        assert_eq!(key.detail, FragmentDetail::TextRun { line: 0, run: 0 });
        assert_eq!((*fill, run.anchor, run.font.size), (Paint::Role(Role::Foreground), AnchorKind::Middle, 14.0));
        assert!(run.advance > 0.0);
    }
}

#[test]
fn rich_runs_start_where_the_runs_before_them_end() {
    let source = "pie\n\"a <b>bold</b> c\" : 1";
    let runs: Vec<_> = visits(source)
        .into_iter()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) if draw.key.part.facet == Facet::Legend => match draw.geometry {
                Geometry::Text(run) => Some(run),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let texts: Vec<&str> = runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(texts, ["a ", "bold", " c"]);
    for pair in runs.windows(2) {
        assert_eq!(pair[1].baseline.x, pair[0].baseline.x + pair[0].advance);
        assert_eq!(pair[1].baseline.y, pair[0].baseline.y);
        assert_eq!(pair[1].anchor, AnchorKind::Start);
    }
    assert_eq!(runs[1].font.weight, pele_core::FontWeight::Bold);
    assert_eq!(runs[0].font.weight, pele_core::FontWeight::Normal);
}

/// Half an em per code unit: any measurer will do where only keys are read.
struct HalfEm;

impl TextMeasurer for HalfEm {
    fn measure(&self, text: TextRef<'_>, font: &Font) -> TextMeasure {
        TextMeasure { width: text.utf16_units().len() as f64 * font.size / 2.0 }
    }
}

#[test]
fn an_id_prefix_changes_no_key() {
    let draw = |prefix: Option<&str>| {
        let options = LayoutOptions { id_prefix: prefix.map(str::to_owned), ..LayoutOptions::default() };
        let prepared = prepare(PETS, ParseOptions::default()).expect("parses");
        let measure = HalfEm;
        keys(
            &prepared
                .scene(NativeInputs { measure: &measure, options: &options })
                .expect("draws")
                .paint()
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(draw(None), draw(Some("other")));
}

#[test]
fn a_label_that_spells_a_separator_keys_apart_from_one_that_is_split_by_it() {
    let keys = keys(&visits("pie\n\"a/legend\" : 1\n\"a\\\\\" : 1"));
    assert!(keys.contains(&"section:a\\/legend/shape".to_owned()), "{keys:?}");
    assert!(keys.contains(&"section:a\\\\/shape".to_owned()), "{keys:?}");
}

#[test]
fn a_palette_change_resolves_again_and_text_on_a_series_contrasts_with_it() {
    let scene = pele_core::parity_scene(PETS).expect("draws");
    let scene = scene.scene();
    let pele = scene.resolve_paints(&Palette::pele(), false);
    let srgb = |paint| pele.get(paint).map(Rgba::to_srgb8);
    assert_eq!(srgb(Paint::Series(SeriesSlot::new(0))), Some([0x4c, 0x78, 0xa8, 255]));
    assert_eq!(srgb(Paint::OnSeries(SeriesSlot::new(0))), Some([255, 255, 255, 255]), "white on the dark blue");
    assert_eq!(srgb(Paint::OnSeries(SeriesSlot::new(5))), Some([0x1f, 0x1f, 0x1f, 255]), "ink on the yellow");
    assert_eq!(pele.get(Paint::None), None);

    let yellow = Rgba::from_srgb([1.0, 0.9, 0.2], 1.0);
    let changed = scene.resolve_paints(&Palette::pele().with_series(0, yellow), false);
    assert_eq!(changed.get(Paint::Series(SeriesSlot::new(0))), Some(yellow));
    assert_eq!(changed.get(Paint::OnSeries(SeriesSlot::new(0))).map(Rgba::to_srgb8), Some([0x1f, 0x1f, 0x1f, 255]));
}

#[test]
fn a_series_colour_the_source_sets_wins_over_the_palette() {
    let source = "%%{init: {\"themeVariables\": {\"pie1\": \"light-dark(red, navy)\"}}}%%\npie\n\"a\" : 1";
    let scene = pele_core::parity_scene(source).expect("draws");
    let slot = Paint::Series(SeriesSlot::new(0));
    let light = scene.scene().resolve_paints(&Palette::pele(), false);
    let dark = scene.scene().resolve_paints(&Palette::pele(), true);
    assert_eq!(light.get(slot).map(Rgba::to_srgb8), Some([255, 0, 0, 255]));
    assert_eq!(dark.get(slot).map(Rgba::to_srgb8), Some([0, 0, 128, 255]));
}
