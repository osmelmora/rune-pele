//! Pie drawn natively through rune-pele's public API, captured headless and measured in pixels
//! against the Scene's own geometry, and against Pele's SVG rasterised by resvg.

// What only the tests that need rune-mermaid use is unused without `fallback`.
#![cfg_attr(not(feature = "fallback"), allow(dead_code, unused_imports))]

mod common;

use std::cell::RefCell;
use std::f64::consts::TAU;
use std::fmt::Write as _;

use common::{arial, as_a_page_reads, out_dir_for, rasterise, save_in, side_by_side};
use pele_core::Scene;
use pele_core::scene::{AnchorKind, Facet, Geometry, Paint, PaintVisit, PathCommand, ResolvedPaints, Rgba};
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels, Window};
use rune_pele::{Diagram, FontCatalog, Palette, RenderStatus, ViewError, ViewOptions};

const RUN: &str = "pie-rune";
const PETS: &str = include_str!("../../../tests/fixtures/pie-pets.mmd");
/// A type pele-core draws and the Rune view does not lower yet.
const NOT_LOWERED: &str = include_str!("../../../tests/fixtures/rune/unlowered.mmd");
const VIEWPORT: (f32, f32) = (480.0, 400.0);

struct Viewer {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
    error: RefCell<Option<ViewError>>,
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let root = el().size(VIEWPORT.0, VIEWPORT.1).fill(Color::WHITE);
    match s.diagram.view(cx, &s.source, s.options.clone()) {
        Ok(drawn) => {
            s.error.replace(None);
            root.child(drawn)
        }
        Err(error) => {
            s.error.replace(Some(error));
            root
        }
    }
}

fn window<'a>(capturer: &'a Capturer, source: &str, palette: Palette, scale: f32) -> Window<'a, Viewer> {
    let viewer = Viewer {
        diagram: Diagram::new("pets", FontCatalog::rune()),
        source: source.to_owned(),
        options: ViewOptions::new(VIEWPORT.0).palette(palette),
        error: RefCell::new(None),
    };
    capturer.window(viewer, view, VIEWPORT, Env::new(false, scale))
}

/// A palette that differs from Pele's in its series only.
fn other_palette() -> Palette {
    let srgb = |r: f32, g: f32, b: f32| {
        let [r, g, b] = [r, g, b].map(rune::color::srgb_decode);
        Color { r, g, b, a: 1.0 }
    };
    Palette::pele()
        .with_series(0, srgb(0.85, 0.30, 0.35))
        .with_series(1, srgb(0.20, 0.55, 0.45))
        .with_series(2, srgb(0.45, 0.35, 0.75))
}

/// One wedge as the Scene draws it, in points.
struct Slice {
    paint: Paint,
    centre: [f64; 2],
    radius: f64,
    start: f64,
    sweep: f64,
    seam: f64,
}

/// Clockwise from twelve o'clock, as Pele measures a slice.
fn angle(x: f64, y: f64) -> f64 {
    x.atan2(-y).rem_euclid(TAU)
}

fn slices(scene: &Scene) -> Vec<Slice> {
    scene
        .paint()
        .filter_map(|visit| {
            let PaintVisit::Draw(draw) = visit else { return None };
            let Geometry::Path(commands) = &draw.geometry else { return None };
            assert_eq!(draw.key.part.facet, Facet::Shape);
            let [
                PathCommand::Move(apex),
                PathCommand::Line(from),
                PathCommand::Arc { rx, end, .. },
                PathCommand::Close,
            ] = commands[..]
            else {
                panic!("a pie slice without a hole is a wedge: {commands:?}")
            };
            assert_eq!((apex.x, apex.y), (0.0, 0.0), "a wedge starts at the centre");
            let start = angle(from.x, from.y);
            let stroke = draw.style.stroke.expect("slices have a seam");
            Some(Slice {
                paint: draw.style.fill,
                centre: [draw.origin.x, draw.origin.y],
                radius: rx,
                start,
                sweep: (angle(end.x, end.y) - start).rem_euclid(TAU),
                seam: stroke.width,
            })
        })
        .collect()
}

/// The area of a wedge its seam leaves showing: the points at least half the seam's width from
/// the wedge's edges, integrated over the angle. Within `h` of an edge's line, or of the centre
/// where a point lies behind the edge, the seam covers it.
fn shown_area(radius: f64, sweep: f64, seam: f64) -> f64 {
    let h = seam / 2.0;
    let outer = radius - h;
    let reach = |phi: f64| if phi.cos() >= 0.0 { phi.sin().abs() } else { 1.0 };
    let steps = 400_000;
    let step = sweep / steps as f64;
    (0..steps)
        .map(|i| {
            let phi = (i as f64 + 0.5) * step;
            let inner = h / reach(phi).min(reach(sweep - phi));
            0.5 * (outer * outer - inner * inner).max(0.0) * step
        })
        .sum()
}

/// A text run's box in points: its advance, padded by a point, from a font size above its
/// baseline to under it by what descenders need.
struct TextBox {
    key: String,
    fill: Paint,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

fn text_boxes(scene: &Scene) -> Vec<TextBox> {
    scene
        .paint()
        .filter_map(|visit| {
            let PaintVisit::Draw(draw) = visit else { return None };
            let Geometry::Text(run) = &draw.geometry else { return None };
            let lead = match run.anchor {
                AnchorKind::Start => 0.0,
                AnchorKind::Middle => run.advance / 2.0,
                AnchorKind::End => run.advance,
            };
            let x = draw.origin.x + run.baseline.x - lead;
            let y = draw.origin.y + run.baseline.y;
            Some(TextBox {
                key: draw.key.to_string(),
                fill: draw.style.fill,
                left: x - 1.0,
                top: y - run.font.size,
                right: x + run.advance + 1.0,
                bottom: y + 0.3 * run.font.size,
            })
        })
        .collect()
}

impl TextBox {
    /// The device pixels whose centres lie inside.
    fn pixels(&self, scale: f64) -> impl Iterator<Item = (u32, u32)> {
        let span = |a: f64, b: f64| ((a * scale - 0.5).ceil().max(0.0) as u32)..=((b * scale - 0.5).floor() as u32);
        let rows = span(self.top, self.bottom);
        span(self.left, self.right).flat_map(move |x| rows.clone().map(move |y| (x, y)))
    }

    fn corners(&self) -> [[f64; 2]; 4] {
        [[self.left, self.top], [self.right, self.top], [self.left, self.bottom], [self.right, self.bottom]]
    }
}

fn srgb(rgba: Rgba) -> [f64; 3] {
    let [r, g, b, _] = rgba.to_srgb8();
    [r, g, b].map(f64::from)
}

/// How much of the pixel `v` each of `a` and `b` covers over `bg`, where it is a blend of the
/// three: the least-squares split of `v - bg` along `a - bg` and `b - bg`.
fn split(v: [f64; 3], bg: [f64; 3], a: [f64; 3], b: [f64; 3]) -> Option<(f64, f64)> {
    let d = |p: [f64; 3]| [p[0] - bg[0], p[1] - bg[1], p[2] - bg[2]];
    let (v, a, b) = (d(v), d(a), d(b));
    let dot = |p: [f64; 3], q: [f64; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    let (aa, ab, bb, av, bv) = (dot(a, a), dot(a, b), dot(b, b), dot(a, v), dot(b, v));
    let det = aa * bb - ab * ab;
    let (fa, fb) =
        if det.abs() < 1e-9 { (av / aa, 0.0) } else { ((av * bb - bv * ab) / det, (bv * aa - av * ab) / det) };
    let (fa, fb) = (fa.clamp(0.0, 1.0), fb.clamp(0.0, 1.0));
    let off = (0..3).map(|i| (v[i] - fa * a[i] - fb * b[i]).powi(2)).sum::<f64>().sqrt();
    (off <= 10.0).then_some((fa, fb))
}

/// Each slice's area in device pixels as drawn: the coverage of its colour over the background,
/// plus the boxes of the labels drawn over it, counted whole. Also the pixels on the plot that
/// are no blend of a slice, its neighbour and the background.
fn drawn_areas(
    pixels: &Pixels,
    slices: &[Slice],
    boxes: &[TextBox],
    paints: &ResolvedPaints,
    scale: f64,
) -> (Vec<f64>, usize) {
    let bg = srgb(paints.get(Paint::Role(pele_core::scene::Role::Background)).expect("a background"));
    let colours: Vec<[f64; 3]> = slices.iter().map(|s| srgb(paints.get(s.paint).expect("a fill"))).collect();
    let centre = slices[0].centre;
    let radius = slices[0].radius;
    let containing = |phi: f64| {
        slices.iter().position(|s| (phi - s.start).rem_euclid(TAU) < s.sweep).expect("the slices make a whole turn")
    };
    let inside = |[x, y]: [f64; 2], slice: &Slice| {
        let (dx, dy) = (x - centre[0], y - centre[1]);
        let shown = |phi: f64| (phi - slice.start).rem_euclid(TAU) < slice.sweep;
        (dx * dx + dy * dy).sqrt() < radius - slice.seam && shown(angle(dx, dy))
    };
    let mut areas = vec![0.0; slices.len()];
    let mut covered = std::collections::HashSet::new();
    for label in boxes {
        let near = label.corners().iter().any(|&[x, y]| (x - centre[0]).hypot(y - centre[1]) < radius + 2.0);
        if !near {
            continue;
        }
        let owner = (0..slices.len()).find(|&k| label.corners().iter().all(|&c| inside(c, &slices[k])));
        let owner = owner.unwrap_or_else(|| panic!("{} lies inside one slice, clear of its seams", label.key));
        for pixel in label.pixels(scale) {
            if covered.insert(pixel) {
                areas[owner] += 1.0;
            }
        }
    }
    let mut unexplained = 0;
    let reach = ((radius + 2.0) * scale).ceil();
    let (cx, cy) = (centre[0] * scale, centre[1] * scale);
    for y in (cy - reach).max(0.0) as u32..(cy + reach) as u32 {
        for x in (cx - reach).max(0.0) as u32..(cx + reach) as u32 {
            let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
            if dx.hypot(dy) > reach || covered.contains(&(x, y)) {
                continue;
            }
            let px = pixels.at(x, y);
            let v = [px[0], px[1], px[2]].map(f64::from);
            let phi = angle(dx, dy);
            let k = containing(phi);
            let s = &slices[k];
            let into = (phi - s.start).rem_euclid(TAU);
            let j = if into < s.sweep / 2.0 { (k + slices.len() - 1) % slices.len() } else { (k + 1) % slices.len() };
            match split(v, bg, colours[k], colours[j]) {
                Some((fk, fj)) => {
                    areas[k] += fk;
                    if j != k {
                        areas[j] += fj;
                    }
                }
                None => unexplained += 1,
            }
        }
    }
    (areas, unexplained)
}

/// Device pixels inside each label's box that are within a few levels of its ink.
fn ink(pixels: &Pixels, boxes: &[TextBox], paints: &ResolvedPaints, scale: f64) -> Vec<(String, usize, usize)> {
    boxes
        .iter()
        .map(|label| {
            let want = srgb(paints.get(label.fill).expect("text has ink"));
            let all: Vec<_> = label.pixels(scale).collect();
            let inked = all
                .iter()
                .filter(|&&(x, y)| {
                    let px = pixels.at(x, y);
                    (0..3).all(|i| (f64::from(px[i]) - want[i]).abs() <= 40.0)
                })
                .count();
            (label.key.clone(), inked, all.len())
        })
        .collect()
}

fn element_keys(window: &Window<'_, Viewer>) -> Vec<String> {
    let mut keys: Vec<String> = window
        .runtime()
        .elements()
        .into_iter()
        .filter_map(|element| element.key)
        .filter(|key| key.starts_with("section:") || key.starts_with("diagram:"))
        .collect();
    keys.sort();
    keys
}

fn scene_keys(scene: &Scene) -> Vec<String> {
    let mut keys: Vec<String> = scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => Some(draw.key.to_string()),
            _ => None,
        })
        .collect();
    keys.sort();
    keys
}

fn changed_pixels(a: &Pixels, b: &Pixels) -> usize {
    a.rgba.chunks(4).zip(b.rgba.chunks(4)).filter(|(p, q)| p != q).count()
}

fn write_report(name: &str, text: &str) {
    std::fs::create_dir_all(out_dir_for(RUN)).unwrap();
    std::fs::write(out_dir_for(RUN).join(name), text).unwrap();
}

#[test]
fn each_slice_covers_its_share_and_every_label_is_drawn_at_1x_and_2x() {
    let capturer = Capturer::new();
    let mut report = String::from("scale slice  drawn_px  shown_px   error  (Rune, native view)\n");
    for scale in [1.0, 2.0] {
        let mut window = window(&capturer, PETS, Palette::pele(), scale as f32);
        let pixels = window.capture();
        save_in(RUN, &pixels, &format!("pie-pets@{scale}x.png"));
        let scene = window.state().diagram.scene().expect("rune-pele drew the pie itself");
        assert_eq!(window.state().diagram.status(), RenderStatus::Pele);
        let paints = scene.resolve_paints(Palette::pele().core(), false);
        let slices = slices(&scene);
        let boxes = text_boxes(&scene);
        assert_eq!(slices.len(), 3);
        assert!((slices.iter().map(|s| s.sweep).sum::<f64>() - TAU).abs() < 1e-3, "the slices make a whole turn");

        let (areas, unexplained) = drawn_areas(&pixels, &slices, &boxes, &paints, scale);
        for (index, (slice, drawn)) in slices.iter().zip(&areas).enumerate() {
            let shown = shown_area(slice.radius, slice.sweep, slice.seam) * scale * scale;
            let error = drawn / shown - 1.0;
            writeln!(report, "{scale:>5} {index:>5} {drawn:>9.1} {shown:>9.1} {error:>+7.4}").unwrap();
            assert!(
                error.abs() <= 0.005,
                "slice {index} at {scale}x: {drawn:.1} px drawn, {shown:.1} shown ({error:+.4})"
            );
        }
        writeln!(report, "{scale:>5} unexplained plot pixels: {unexplained}").unwrap();
        assert!(unexplained <= 4, "{unexplained} plot pixels are no blend of slices and background");

        for (key, inked, all) in ink(&pixels, &boxes, &paints, scale) {
            writeln!(report, "{scale:>5} {key}: {inked} of {all} px inked").unwrap();
            assert!(inked * 20 >= all && inked >= 3, "{key} at {scale}x has ink in its box: {inked} of {all} px");
        }
    }
    print!("{report}");
    write_report("areas-rune.txt", &report);
}

#[test]
fn a_palette_change_repaints_without_parsing_laying_out_or_measuring() {
    let capturer = Capturer::new();
    let mut window = window(&capturer, PETS, Palette::pele(), 2.0);
    let first = window.capture();
    let laid_out = window.state().diagram.stats();
    assert_eq!((laid_out.layouts, laid_out.paints), (1, 1));
    assert!(laid_out.measurements > 0, "layout measured its labels");

    let again = window.capture();
    assert_eq!(window.state().diagram.stats(), laid_out, "an unchanged view reuses its paint");
    assert_eq!(changed_pixels(&first, &again), 0);

    window.update(|viewer| viewer.options = viewer.options.clone().palette(other_palette()));
    let repainted = window.capture();
    save_in(RUN, &repainted, "pie-pets.other-palette@2x.png");
    let after = window.state().diagram.stats();
    let changed = changed_pixels(&first, &repainted);
    let report = format!(
        "before palette change: {laid_out:?}\nafter palette change:  {after:?}\npixels changed: {changed} of {}\n",
        first.rgba.len() / 4
    );
    print!("{report}");
    write_report("theme-change.txt", &report);
    let mut painted_once_more = laid_out;
    painted_once_more.paints += 1;
    assert_eq!(after, painted_once_more, "a palette change paints once more and lays out and measures nothing");
    assert!(changed > 10_000, "the slices changed colour: {changed} pixels");
}

#[test]
fn keys_follow_what_is_drawn_not_the_id_prefix_or_the_values() {
    let capturer = Capturer::new();
    let mut window = window(&capturer, PETS, Palette::pele(), 1.0);
    window.capture();
    let keys = element_keys(&window);
    let scene = window.state().diagram.scene().expect("a native scene");
    assert_eq!(keys, scene_keys(&scene), "every drawing is one element, keyed by its fragment");
    assert!(keys.contains(&"section:Dogs/shape".to_owned()), "{keys:?}");

    window.update(|viewer| viewer.options = viewer.options.clone().id_prefix("elsewhere"));
    window.capture();
    assert_eq!(element_keys(&window), keys, "an idPrefix names SVG ids, not elements");

    window.update(|viewer| viewer.source = PETS.replace("85", "120"));
    window.capture();
    assert_eq!(element_keys(&window), keys, "a new value keeps every slice's elements");
    assert_eq!(window.state().diagram.stats().layouts, 3, "each of the three inputs laid out once");
}

#[cfg(feature = "fallback")]
#[test]
fn only_a_source_the_view_does_not_lower_falls_back_to_rune_mermaid() {
    let capturer = Capturer::new();
    // Every type of Pele 1.13.0 is ported and lowered, so a fallback is for a Scene with something
    // the lowering refuses: here a line through text, which the walk does not read.
    let mut unlowered = window(&capturer, NOT_LOWERED, Palette::pele(), 1.0);
    unlowered.capture();
    assert_eq!(*unlowered.state().error.borrow(), None);
    assert_eq!(unlowered.state().diagram.status(), RenderStatus::RuneMermaid);
    let unread = rune_pele::Fallback::NotLowered(rune_pele::lower::Unsupported::Style(
        "text-decoration:line-through".to_owned(),
    ));
    assert_eq!(unlowered.state().diagram.fallback(), Some(unread));

    let mut bad = window(&capturer, "pie\n \"a\" 1", Palette::pele(), 1.0);
    bad.capture();
    let error = bad.state().error.borrow().clone();
    assert!(
        matches!(&error, Some(ViewError::Core(pele_core::RenderError::Diagnostic(d))) if d.message.starts_with("Parse error on line 2")),
        "a pie that does not parse is Pele's error, not another grammar: {error:?}"
    );
    assert_eq!(bad.state().diagram.status(), RenderStatus::Unrendered);
}

#[test]
fn pele_svg_through_resvg_draws_the_same_slice_areas() {
    let pele = pele_oracle::render_all(&[PETS.to_owned()]);
    let pele_oracle::PeleRender::Svg { svg, .. } = &pele[0] else { panic!("Pele renders the pets: {pele:?}") };
    let parity = pele_core::parity_scene(PETS).expect("a parity scene");
    let parity = parity.scene();
    let paints = parity.resolve_paints(Palette::pele().core(), false);
    let capturer = Capturer::new();
    let mut report = format!(
        "Pele's SVG through resvg 0.45 in {:?}, with CSS variables resolved as a page without CSS of its own does;\n\
         rune-pele through Rune in Inter. Palette: Pele's own.\n\
         scale slice  rune_px  resvg_px  shown_px  rune-resvg\n",
        arial()
    );
    for scale in [1.0f64, 2.0] {
        let theirs = rasterise(&as_a_page_reads(svg), scale as f32);
        save_in(RUN, &theirs, &format!("pie-pets.pele-resvg@{scale}x.png"));
        let mut window = window(&capturer, PETS, Palette::pele(), scale as f32);
        let ours = window.capture();
        save_in(
            RUN,
            &side_by_side(&ours, &theirs, (16.0 * scale) as u32),
            &format!("pie-pets.side-by-side@{scale}x.png"),
        );

        let scene = window.state().diagram.scene().expect("a native scene");
        let (rune, _) = drawn_areas(&ours, &slices(&scene), &text_boxes(&scene), &paints, scale);
        let (resvg, unexplained) = drawn_areas(&theirs, &slices(parity), &text_boxes(parity), &paints, scale);
        assert!(unexplained <= 4, "{unexplained} resvg plot pixels are no blend of slices and background");
        for (index, slice) in slices(parity).iter().enumerate() {
            let shown = shown_area(slice.radius, slice.sweep, slice.seam) * scale * scale;
            let apart = rune[index] / resvg[index] - 1.0;
            writeln!(
                report,
                "{scale:>5} {index:>5} {:>8.1} {:>9.1} {shown:>9.1} {apart:>+11.4}",
                rune[index], resvg[index]
            )
            .unwrap();
            assert!(
                apart.abs() <= 0.01,
                "slice {index} at {scale}x: Rune {:.1} px, resvg {:.1} px",
                rune[index],
                resvg[index]
            );
        }
    }
    print!("{report}");
    write_report("areas-side-by-side.txt", &report);
}

/// The origin and radius of the first shape the Scene draws, in points.
fn first_shape(scene: &Scene) -> ([f64; 2], f64) {
    scene
        .paint()
        .find_map(|visit| match visit {
            PaintVisit::Draw(draw) => match draw.geometry {
                Geometry::Circle { radius, .. } => Some(([draw.origin.x, draw.origin.y], radius)),
                Geometry::Path(commands) => commands.iter().find_map(|command| match command {
                    PathCommand::Arc { rx, .. } => Some(([draw.origin.x, draw.origin.y], *rx)),
                    _ => None,
                }),
                _ => None,
            },
            _ => None,
        })
        .expect("a shape")
}

fn near(got: [u8; 4], want: [u8; 4]) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 3)
}

#[test]
fn a_whole_donut_keeps_its_hole_and_an_empty_pie_draws_its_rim() {
    let capturer = Capturer::new();
    let scale = 2.0;
    let px = |[x, y]: [f64; 2]| ((x * scale) as u32, (y * scale) as u32);
    let white = [255, 255, 255, 255];

    let donut = "%%{init: {\"pie\": {\"donutHole\": 0.5}}}%%\npie\n\"Only\" : 1";
    let mut window = window(&capturer, donut, Palette::pele(), scale as f32);
    let pixels = window.capture();
    save_in(RUN, &pixels, "donut-whole@2x.png");
    assert_eq!(*window.state().error.borrow(), None, "a whole donut draws");
    let ([x, y], radius) = first_shape(&window.state().diagram.scene().expect("a native scene"));
    let blue = Palette::pele().core().series(0).to_srgb8();
    let (cx, cy) = px([x, y]);
    assert!(near(pixels.at(cx, cy), white), "the hole shows what is behind: {:?}", pixels.at(cx, cy));
    for (dx, dy) in [(0.0, -0.75), (-0.75, 0.0), (0.75, 0.0)] {
        let (mx, my) = px([x + dx * radius, y + dy * radius]);
        assert!(near(pixels.at(mx, my), blue), "the ring is the series colour at {dx},{dy}: {:?}", pixels.at(mx, my));
    }
    let (ox, oy) = px([x + radius + 3.0, y]);
    assert!(near(pixels.at(ox, oy), white), "nothing past the rim");

    let mut empty = window_with(&capturer, "pie title Nothing yet", scale as f32);
    let pixels = empty.capture();
    save_in(RUN, &pixels, "pie-empty@2x.png");
    let ([x, y], radius) = first_shape(&empty.state().diagram.scene().expect("a native scene"));
    let border = Palette::pele().core().role(pele_core::scene::Role::Border).to_srgb8();
    let (rx, ry) = px([x + radius, y]);
    let rim = (rx - 1..=rx).map(|x| pixels.at(x, ry)).collect::<Vec<_>>();
    assert!(rim.iter().any(|&p| near(p, border)), "the empty pie's rim is the border colour: {rim:?}");
    assert!(near(pixels.at(px([x, y]).0, px([x, y]).1), white), "and has no fill");
}

fn window_with<'a>(capturer: &'a Capturer, source: &str, scale: f32) -> Window<'a, Viewer> {
    window(capturer, source, Palette::pele(), scale)
}
