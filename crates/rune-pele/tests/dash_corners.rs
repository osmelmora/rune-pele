//! A thick dashed stroke round a rounded box, a circle and an ellipse, drawn natively beside
//! resvg's drawing of Pele's SVG: held to the limits the corpus is held to, and its dashes to
//! where resvg puts them.

mod common;

use common::{Apart, as_a_page_reads, compare, in_a_row, magnified, marked, over, rasterise, text_boxes};
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels};
use rune_pele::geometry::{Arc, Outline, Point, Polyline, Segment};
use rune_pele::stroke::{Cap, Join, StrokeLook, stroke};
use rune_pele::{Diagram, FontCatalog, Palette, RenderStatus, ViewOptions};

const RUN: &str = "dash-corners";

/// The source the follow-ups round measured at 12.9 unexplained pixels in 10,000.
const ROUNDED: &str = "flowchart LR\n  A(Rounded) --> B\n  style A stroke-width:6px,stroke-dasharray:5 5";

struct Page {
    diagram: Diagram,
    source: String,
    size: (f32, f32),
}

fn page_view(s: &Page, cx: &mut Cx<Page>) -> El {
    let options = ViewOptions::natural().pele_metrics().palette(Palette::pele());
    let drawn = s.diagram.view(cx, &s.source, options).expect("the source draws");
    el().size(s.size.0, s.size.1).fill(Color::WHITE).child(drawn)
}

/// One source drawn natively and by resvg from Pele's SVG, and the boxes of its text.
struct Drawn {
    ours: Pixels,
    theirs: Pixels,
    text: Vec<[f64; 4]>,
}

fn drawn(capturer: &Capturer, source: &str, scale: f32) -> Drawn {
    let pele = pele_oracle::render_all(&[source.to_owned()]);
    let pele_oracle::PeleRender::Svg { svg, width, height, .. } = &pele[0] else { panic!("Pele draws {source:?}") };
    let size = (width.ceil() as f32, height.ceil() as f32);
    let page = Page { diagram: Diagram::new("diagram", FontCatalog::rune()), source: source.to_owned(), size };
    let mut window = capturer.window(page, page_view, size, Env::new(false, scale));
    let ours = window.capture();
    let diagram = &window.state().diagram;
    assert_eq!(diagram.status(), RenderStatus::Pele, "{source:?} is drawn natively");
    let text = text_boxes(&diagram.scene().expect("a scene"));
    Drawn { ours, theirs: rasterise(&as_a_page_reads(svg), scale), text }
}

/// How far apart the two drawings are, and a row of them and of where they differ.
fn apart(drawn: &Drawn, scale: f32) -> (Apart, Pixels) {
    let (apart, differ) = compare(&drawn.ours, &drawn.theirs, &drawn.text, f64::from(scale));
    (apart, in_a_row(&[&drawn.ours, &drawn.theirs, &marked(&drawn.ours, &differ)], 8))
}

fn summary(apart: &Apart) -> String {
    format!(
        "patch {} px, {:.2} unexplained in 10,000, flat {} levels",
        apart.largest_blob,
        apart.share(),
        apart.flat_error
    )
}

/// The limits `apart` is over at `scale`: all of them at 2x, and the 1x rules at 1x.
fn over_limits(apart: &Apart, scale: f32) -> Vec<String> {
    let (sharp, other) = over(apart, f64::from(scale));
    if scale < 2.0 { other } else { sharp.into_iter().chain(other).collect() }
}

#[test]
fn a_thick_dashed_stroke_on_a_rounded_box_draws_as_resvg_draws_it() {
    let capturer = Capturer::new();
    for scale in [1.0f32, 2.0] {
        let (apart, row) = apart(&drawn(&capturer, ROUNDED, scale), scale);
        common::save_in(RUN, &row, &format!("rounded@{scale}x.png"));
        common::save_in(RUN, &magnified(&row, 4), &format!("rounded@{scale}x.magnified.png"));
        println!("rounded@{scale}x: {}", summary(&apart));
        let over = over_limits(&apart, scale);
        assert!(over.is_empty(), "rounded@{scale}x: {over:?}");
    }
}

/// Where the stroke along one straight side of a path shows and stops showing, as distances
/// along the path: where the coverage of the device pixels just inside the side, over the
/// fill, crosses a half. The side runs `length` from `from` in the unit direction `step`, and
/// starts `s0` along the path.
fn dash_ends(pixels: &Pixels, scale: f64, from: Point, step: Point, length: f64, s0: f64) -> Vec<f64> {
    let (fill, ink) = (243.0, 138.0);
    let coverage = |s: f64| {
        let p = [from[0] + step[0] * s, from[1] + step[1] * s];
        (fill - f64::from(pixels.at((p[0] * scale) as u32, (p[1] * scale) as u32)[0])) / (fill - ink)
    };
    let samples: Vec<(f64, f64)> =
        (0..(length * scale) as usize).map(|k| (k as f64 + 0.5) / scale).map(|s| (s, coverage(s))).collect();
    samples
        .windows(2)
        .filter(|pair| (pair[0].1 < 0.5) != (pair[1].1 < 0.5))
        .map(|pair| {
            let ((s, a), (t, b)) = (pair[0], pair[1]);
            s0 + s + (t - s) * (0.5 - a) / (b - a)
        })
        .collect()
}

#[test]
fn the_dashes_along_each_side_of_a_rounded_box_lie_where_resvg_s_do() {
    let capturer = Capturer::new();
    let scale = 2.0;
    let drawn = drawn(&capturer, ROUNDED, scale as f32);
    // Node A's box in Pele's SVG, and its corner radius.
    let ([x0, y0], [x1, y1], r) = ([8.0, 8.0], [108.93, 56.0], 12.0);
    let (across, down, corner) = (x1 - x0 - 2.0 * r, y1 - y0 - 2.0 * r, std::f64::consts::FRAC_PI_2 * r);
    let inset = 0.25;
    // SVG starts a rect's path at (x + rx, y), and runs clockwise.
    let sides: [(&str, Point, Point, f64, f64); 4] = [
        ("top", [x0 + r, y0 + inset], [1.0, 0.0], across, 0.0),
        ("right", [x1 - inset, y0 + r], [0.0, 1.0], down, across + corner),
        ("bottom", [x1 - r, y1 - inset], [-1.0, 0.0], across, across + down + 2.0 * corner),
        ("left", [x0 + inset, y1 - r], [0.0, -1.0], down, 2.0 * across + down + 3.0 * corner),
    ];
    for (side, from, step, length, s0) in sides {
        let ours = dash_ends(&drawn.ours, scale, from, step, length, s0);
        let theirs = dash_ends(&drawn.theirs, scale, from, step, length, s0);
        let show = |ends: &[f64]| ends.iter().map(|s| format!("{s:.2}")).collect::<Vec<_>>().join(" ");
        println!("{side:<6} here  {}\n{side:<6} resvg {}", show(&ours), show(&theirs));
        assert!(ours.len() > 2, "{side}: dashes show along it");
        assert_eq!(ours.len(), theirs.len(), "{side}: as many dash ends here as in resvg");
        for (here, there) in ours.iter().zip(&theirs) {
            assert!((here - there).abs() <= 0.25, "{side}: a dash ends at {here:.2} here and at {there:.2} in resvg");
        }
    }
}

/// A node of `shape` stroked `width` wide in `dash`, and with `radius` a corner radius.
fn dashed(shape: &str, width: u32, dash: &str, radius: Option<u32>) -> String {
    let rx = radius.map_or(String::new(), |radius| format!(",rx:{radius}px"));
    format!("flowchart LR\n  {shape}\n  style A stroke-width:{width}px,stroke-dasharray:{dash}{rx}")
}

/// The pixels of a dash round a corner tighter than half the stroke are listed and not held:
/// there the curve's normals cross at its centre inside the stroke, and a box along each chord
/// of the flattened curve reaches past where they meet, by up to half a chord.
fn tighter_than_the_stroke(radius: Option<u32>, width: u32) -> bool {
    radius.is_some_and(|radius| radius > 0 && 2 * radius < width)
}

#[test]
fn dashed_boxes_circles_and_ellipses_draw_as_resvg_draws_them() {
    let capturer = Capturer::new();
    // A rounded box is 48 points high, so a radius of 30 is over half its height.
    let shapes: [(&str, &str, &[Option<u32>]); 3] = [
        ("box", "A(Rounded)", &[Some(0), Some(5), Some(30)]),
        ("circle", "A((Circle))", &[None]),
        ("ellipse", "A(-Ellipse-)", &[None]),
    ];
    let mut report = String::from("shape   radius width dash        share@1x patch@1x  share@2x patch@2x flat  over\n");
    let (mut failed, mut listed) = (Vec::new(), 0);
    for (name, shape, radii) in shapes {
        for &radius in radii {
            for width in [1, 2, 6, 12] {
                for dash in ["5 5", "2 8", "10 3 2 3"] {
                    let source = dashed(shape, width, dash, radius);
                    let tight = tighter_than_the_stroke(radius, width);
                    let (mut cells, mut held, mut flat) = (Vec::new(), Vec::new(), 0);
                    for scale in [1.0f32, 2.0] {
                        let (apart, row) = apart(&drawn(&capturer, &source, scale), scale);
                        let file =
                            format!("{name}-r{}-w{width}-{}@{scale}x.png", radius.unwrap_or(0), dash.replace(' ', "_"));
                        common::save_in(RUN, &row, &format!("set/{file}"));
                        cells.push(format!("{:>8.2} {:>8}", apart.share(), apart.largest_blob));
                        flat = flat.max(apart.flat_error);
                        let (sharp, other) = over(&apart, f64::from(scale));
                        let sharp = if scale < 2.0 || tight { Vec::new() } else { sharp };
                        held.extend(sharp.into_iter().chain(other).map(|over| format!("{scale}x: {over}")));
                    }
                    let radius = radius.map_or("-".to_owned(), |radius| radius.to_string());
                    let note = if tight {
                        "listed: a corner tighter than half the stroke".to_owned()
                    } else {
                        held.join(", ")
                    };
                    let line =
                        format!("{name:<7} {radius:>6} {width:>5} {dash:<10} {}  {flat:>4}  {note}", cells.join("  "));
                    println!("{line}");
                    report.push_str(&line);
                    report.push('\n');
                    listed += usize::from(tight);
                    if !held.is_empty() {
                        failed.push(format!("{name} radius {radius} width {width} dash {dash}: {}", held.join(", ")));
                    }
                }
            }
        }
    }
    let path = common::out_dir_for(RUN).join("set.txt");
    std::fs::write(&path, &report).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(listed, 3, "only the box of radius 5 stroked 12 wide is tighter than its stroke");
    assert!(failed.is_empty(), "over the limits:\n{}", failed.join("\n"));
}

#[test]
fn a_box_with_a_radius_over_half_its_height_has_elliptical_corners_as_svg_clamps_each_radius() {
    let capturer = Capturer::new();
    for width in [1, 2, 6] {
        let source = format!("flowchart LR\n  A(Rounded)\n  style A stroke-width:{width}px,rx:30px");
        let (apart, row) = apart(&drawn(&capturer, &source, 2.0), 2.0);
        common::save_in(RUN, &row, &format!("solid-r30-w{width}@2x.png"));
        println!("solid radius 30 width {width}@2x: {}", summary(&apart));
        let over = over_limits(&apart, 2.0);
        assert!(over.is_empty(), "solid radius 30 width {width}@2x: {over:?}");
    }
}

/// Whether `p` lies inside `polygon`, and how far it is from the polygon's nearest side.
fn inside(polygon: &[Point], p: Point) -> (bool, f64) {
    let n = polygon.len();
    let (mut crossings, mut nearest) = (0, f64::MAX);
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            crossings += 1;
        }
        let (d, e) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
        let t = ((e[0] * d[0] + e[1] * d[1]) / (d[0] * d[0] + d[1] * d[1])).clamp(0.0, 1.0);
        nearest = nearest.min((e[0] - d[0] * t).hypot(e[1] - d[1] * t));
    }
    (crossings % 2 == 1, nearest)
}

/// A stroke `width` wide with SVG's default ends and corners.
fn look(width: f64) -> StrokeLook {
    StrokeLook { width, cap: Cap::Butt, join: Join::Miter, dash: Vec::new() }
}

#[test]
fn a_dash_round_a_curve_has_a_round_join_wherever_the_curve_was_cut_and_the_gap_would_show() {
    // A corner of radius 12, flattened for a window of `1 / pixel` device pixels a point.
    let corner = [
        Segment::Move([0.0, 0.0]),
        Segment::Line([20.0, 0.0]),
        Segment::Arc(Arc { radii: [12.0; 2], rotation: 0.0, large: false, sweep: true, to: [32.0, 12.0] }),
        Segment::Line([32.0, 40.0]),
    ];
    let dashed = |width: f64, pixel: f64| {
        let [line] = &Outline::lower(&corner).flatten(0.25 * pixel)[..] else { panic!("one subpath") };
        stroke(std::slice::from_ref(line), &StrokeLook { dash: vec![5.0, 5.0], ..look(width) }, pixel)
    };
    let (pixel, thick) = (0.5f64, dashed(6.0, 0.5));
    // Each point a dash bends at, and how far the dash runs on past it the shorter way.
    let mut joints: Vec<(Point, f64)> = Vec::new();
    for piece in &thick.butt {
        let lengths: Vec<f64> =
            piece.windows(2).map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1])).collect();
        let total: f64 = lengths.iter().sum();
        for (i, joint) in piece.iter().enumerate().take(piece.len() - 1).skip(1) {
            let before: f64 = lengths[..i].iter().sum();
            joints.push((*joint, before.min(total - before)));
        }
    }
    assert!(joints.len() >= 4, "the dashes round the corner bend at the points it was cut at: {:?}", thick.butt);
    for (joint, room) in &joints {
        let deepest = thick.patches.iter().map(|patch| inside(patch, *joint)).filter(|(is, _)| *is).map(|(_, d)| d);
        let margin = deepest.fold(0.0, f64::max);
        let want = pixel.min(*room) * 0.99;
        assert!(margin >= want, "{joint:?}, where two boxes meet, lies {want} inside a patch: {margin}");
    }
    // A stroke a point wide and shorter than that, which bends by a fifth of a radian where a
    // curve was cut: its notch takes 0.025 square points.
    let (bend, step) = (0.2f64, 0.4);
    let points = vec![[0.0, 0.0], [step, 0.0], [step + step * bend.cos(), step * bend.sin()]];
    let short = Polyline { corners: vec![None; points.len()], points, closed: false };
    let hairline = |pixel: f64| stroke(std::slice::from_ref(&short), &look(1.0), pixel);
    let look_at_1x = hairline(1.0);
    assert_eq!(look_at_1x.butt.len(), 1, "drawn as boxes: {look_at_1x:?}");
    assert!(look_at_1x.patches.is_empty(), "at 1x the notch takes under a twentieth of a pixel and is left open");
    assert_eq!(hairline(0.5).patches.len(), 1, "at 2x it takes a tenth of one, and is filled");
}
