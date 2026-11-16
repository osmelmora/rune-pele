//! Each lowering on its own: what SVG draws and Rune has no word for, drawn through rune-pele's
//! figures and text runs, captured headless, and held to what the geometry says and to the same
//! SVG rasterised by resvg.

mod common;

use std::f64::consts::PI;

use common::{Apart, around_pixel, compare, in_a_row, marked, rasterise};
use rune::color::hex;
use rune::{Color, Cx, El, Env, TextStyle, el};
use rune_pele::capture::{Capturer, Pixels};
use rune_pele::geometry::{Affine, Arc, Cubic, Outline, Point, Segment};
use rune_pele::lower::{Figure, TextRun};
use rune_pele::stroke::{Cap, Join, StrokeLook};
use rune_pele::text::{Anchor, BaselinePoint, Baselines};
use rune_pele::{Diagram, FontCatalog, Palette, ViewOptions};

const RUN: &str = "rune-graph";
const BLUE: &str = "#4c78a8";
const ORANGE: &str = "#f58518";
const INK: &str = "#1f1f1f";

fn colour(spelling: &str) -> Color {
    hex(spelling).expect("a hex colour")
}

fn faded(spelling: &str, alpha: f32) -> Color {
    Color { a: alpha, ..colour(spelling) }
}

fn levels(spelling: &str) -> [u8; 3] {
    let byte = |i: usize| u8::from_str_radix(&spelling[i..i + 2], 16).expect("hex");
    [byte(1), byte(3), byte(5)]
}

/// `colour` at `alpha` over white, in levels, as sRGB blending leaves it.
fn over_white(spelling: &str, alpha: f64) -> [u8; 3] {
    levels(spelling).map(|c| (255.0 + (f64::from(c) - 255.0) * alpha).round() as u8)
}

fn near(got: [u8; 4], want: [u8; 3], within: u8) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= within)
}

fn look(width: f64, cap: Cap, join: Join, dash: &[f64]) -> StrokeLook {
    StrokeLook { width, cap, join, dash: dash.to_vec() }
}

fn figure(program: &[Segment], filled: bool, look: Option<&StrokeLook>, scale: f32) -> Figure {
    Figure::new(&Outline::lower(program), filled, look, scale).expect("no crossing").expect("something to draw")
}

fn polyline(points: &[Point], closed: bool) -> Vec<Segment> {
    let mut program: Vec<Segment> = points.iter().map(|&p| Segment::Line(p)).collect();
    program[0] = Segment::Move(points[0]);
    if closed {
        program.push(Segment::Close);
    }
    program
}

const SIZE: (f32, f32) = (200.0, 120.0);

/// The elements on a white page, drawn as a drawing's are: nothing snapped to the pixel grid.
fn draw(elements: Vec<El>, scale: f32) -> Pixels {
    let root = el().size(SIZE.0, SIZE.1).fill(Color::WHITE).pixel_snap(false).children(elements);
    Capturer::new().capture(root, SIZE, scale)
}

fn svg(body: &str) -> String {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">{body}</svg>", SIZE.0, SIZE.1)
}

/// Holds a drawing to the same thing drawn by resvg, by the limits the corpus is held to at 2x,
/// and saves the two with their differences marked.
fn as_resvg_draws(name: &str, ours: &Pixels, body: &str, scale: f32) -> Apart {
    let theirs = rasterise(&svg(body), scale);
    let (apart, differ) = compare(ours, &theirs, &[], f64::from(scale));
    common::save_in(
        RUN,
        &in_a_row(&[ours, &theirs, &marked(ours, &differ)], 8),
        &format!("lowering/{name}@{scale}x.png"),
    );
    println!(
        "{name}@{scale}x: patch {} px, {:.2} unexplained in 10,000, flat {} levels",
        apart.largest_blob,
        apart.share(),
        apart.flat_error
    );
    assert!(apart.largest_blob as f32 <= 3.0 * scale * scale, "{name}: a patch of {} px", apart.largest_blob);
    assert!(apart.share() <= 5.0, "{name}: {:.1} unexplained px in 10,000", apart.share());
    assert!(apart.flat_error <= 4, "{name}: flat colours {} levels apart", apart.flat_error);
    apart
}

/// The runs of ink along row `y` between `from` and `to`, as `(start, length)` in pixels. Ink
/// is anything more than half way from white to `colour`.
fn runs(pixels: &Pixels, y: u32, from: u32, to: u32, colour: [u8; 3]) -> Vec<(u32, u32)> {
    let inked = |x: u32| {
        let px = pixels.at(x, y);
        let most = (0..3).max_by_key(|&c| 255 - colour[c]).expect("three channels");
        2 * (255 - u32::from(px[most])) > 255 - u32::from(colour[most])
    };
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for x in from..to {
        match runs.last_mut() {
            Some((start, length)) if inked(x) && *start + *length == x => *length += 1,
            _ if inked(x) => runs.push((x, 1)),
            _ => {}
        }
    }
    runs
}

/// The least box around the pixels within `within` levels of `colour`: left, top, right, bottom.
fn ink_box(pixels: &Pixels, colour: [u8; 3], within: u8) -> Option<[u32; 4]> {
    let mut found: Option<[u32; 4]> = None;
    for y in 0..pixels.height {
        for x in 0..pixels.width {
            if near(pixels.at(x, y), colour, within) {
                let [left, top, right, bottom] = found.unwrap_or([x, y, x, y]);
                found = Some([left.min(x), top.min(y), right.max(x), bottom.max(y)]);
            }
        }
    }
    found
}

#[test]
fn dashes_keep_the_pattern_from_the_start_with_the_strokes_caps() {
    let scale = 2.0;
    let straight = polyline(&[[10.0, 10.0], [190.0, 10.0]], false);
    let s = Cubic { c1: [10.0, 80.0], c2: [190.0, 30.0], to: [190.0, 100.0] };
    let curve = [Segment::Move([10.0, 30.0]), Segment::Cubic(s)];
    let ring = polyline(&[[60.0, 40.0], [140.0, 40.0], [140.0, 90.0], [60.0, 90.0]], true);
    let elements = vec![
        figure(&straight, false, Some(&look(2.0, Cap::Butt, Join::Miter, &[6.0, 4.0])), scale)
            .el(None, Some(colour(INK))),
        figure(&curve, false, Some(&look(1.0, Cap::Round, Join::Miter, &[3.0, 4.0])), scale)
            .el(None, Some(colour(BLUE))),
        figure(&ring, false, Some(&look(2.0, Cap::Butt, Join::Miter, &[9.0, 5.0])), scale)
            .el(None, Some(colour(ORANGE))),
    ];
    let ours = draw(elements.into_iter().flatten().collect(), scale);

    // The straight line: 18 dashes of 6 points from its start, 4 points apart.
    let dashes = runs(&ours, 20, 0, 400, levels(INK));
    assert_eq!(dashes.len(), 18, "{dashes:?}");
    for (index, (start, length)) in dashes.iter().enumerate() {
        assert_eq!((*start, *length), (20 + 20 * index as u32, 12), "dash {index} of {dashes:?}");
    }
    // The ring's top side starts with a dash at its first corner: 9 on, 5 off.
    let top = runs(&ours, 80, 100, 290, levels(ORANGE));
    assert_eq!(top[1], (148, 18), "the second dash starts 14 points on: {top:?}");
    assert_eq!(top[0].0 + top[0].1, 138, "the first ends 9 points from the corner: {top:?}");

    let body = format!(
        "<path d=\"M10,10L190,10\" fill=\"none\" stroke=\"{INK}\" stroke-width=\"2\" stroke-dasharray=\"6 4\"/>\
         <path d=\"M10,30C10,80 190,30 190,100\" fill=\"none\" stroke=\"{BLUE}\" stroke-linecap=\"round\" stroke-dasharray=\"3 4\"/>\
         <path d=\"M60,40L140,40L140,90L60,90Z\" fill=\"none\" stroke=\"{ORANGE}\" stroke-width=\"2\" stroke-dasharray=\"9 5\"/>"
    );
    as_resvg_draws("dashes", &ours, &body, scale);
}

#[test]
fn arcs_go_round_the_way_their_flags_say() {
    let scale = 2.0;
    // An actor's head as usecase draws it: nearly a whole circle, the long way round.
    let head = [
        Segment::Move([44.0, 30.0]),
        Segment::Arc(Arc { radii: [14.0, 14.0], rotation: 0.0, large: true, sweep: true, to: [56.0, 30.0] }),
        Segment::Close,
    ];
    // A bow: a half ellipse out on the left and the same curve in on the right.
    let bow = [
        Segment::Move([100.0, 20.0]),
        Segment::Line([170.0, 20.0]),
        Segment::Arc(Arc { radii: [10.0, 30.0], rotation: 0.0, large: false, sweep: false, to: [170.0, 80.0] }),
        Segment::Line([100.0, 80.0]),
        Segment::Arc(Arc { radii: [10.0, 30.0], rotation: 0.0, large: false, sweep: true, to: [100.0, 20.0] }),
        Segment::Close,
    ];
    let border = look(1.0, Cap::Butt, Join::Miter, &[]);
    let elements = vec![
        figure(&head, true, Some(&border), scale).el(Some(colour(BLUE)), Some(colour(INK))),
        figure(&bow, true, Some(&border), scale).el(Some(colour(ORANGE)), Some(colour(INK))),
    ];
    let ours = draw(elements.into_iter().flatten().collect(), scale);

    // The head's circle is centred above the chord: its top is 14 above the centre.
    let centre = [50.0, 30.0 - (14f64 * 14.0 - 36.0).sqrt()];
    let px = |x: f64, y: f64| ours.at((x * 2.0) as u32, (y * 2.0) as u32);
    assert!(near(px(centre[0], centre[1]), levels(BLUE), 2), "the head is filled at its centre");
    assert!(near(px(centre[0], centre[1] - 12.0), levels(BLUE), 2), "and up to its top, the long way round");
    assert!(near(px(centre[0], centre[1] - 16.0), [255; 3], 2), "and not past it");
    assert!(near(px(50.0, 34.0), [255; 3], 2), "nor on the short side of the chord");
    assert!(near(px(94.0, 50.0), levels(ORANGE), 2), "the bow's left side bulges out");
    assert!(near(px(165.0, 50.0), [255; 3], 2), "and its right side curves in");

    let body = format!(
        "<path d=\"M44,30A14,14 0 1 1 56,30Z\" fill=\"{BLUE}\" stroke=\"{INK}\"/>\
         <path d=\"M100,20H170A10,30 0 0 0 170,80H100A10,30 0 0 1 100,20Z\" fill=\"{ORANGE}\" stroke=\"{INK}\"/>"
    );
    as_resvg_draws("arcs", &ours, &body, scale);
}

fn circle(centre: Point, radius: f64, sweep: bool) -> [Segment; 4] {
    let half = |to: Point| Segment::Arc(Arc { radii: [radius; 2], rotation: 0.0, large: true, sweep, to });
    let (top, bottom) = ([centre[0], centre[1] - radius], [centre[0], centre[1] + radius]);
    [Segment::Move(top), half(bottom), half(top), Segment::Close]
}

#[test]
fn a_hole_shows_what_is_behind_and_its_slit_does_not_show() {
    let scale = 2.0;
    let donut: Vec<Segment> =
        circle([50.0, 60.0], 40.0, true).into_iter().chain(circle([50.0, 60.0], 20.0, false)).collect();
    let mut plate = polyline(&[[110.0, 20.0], [190.0, 20.0], [190.0, 100.0], [110.0, 100.0]], true);
    plate.extend(polyline(&[[120.0, 30.0], [120.0, 50.0], [140.0, 50.0], [140.0, 30.0]], true));
    plate.extend(polyline(&[[155.0, 60.0], [155.0, 90.0], [180.0, 90.0], [180.0, 60.0]], true));
    for (name, alpha) in [("holes", 1.0f32), ("holes-translucent", 0.5)] {
        let elements = vec![
            figure(&donut, true, None, scale).el(Some(faded(BLUE, alpha)), None),
            figure(&plate, true, None, scale).el(Some(faded(ORANGE, alpha)), None),
        ];
        let ours = draw(elements.into_iter().flatten().collect(), scale);
        let px = |x: f64, y: f64| ours.at((x * 2.0) as u32, (y * 2.0) as u32);
        let (blue, orange) = (over_white(BLUE, f64::from(alpha)), over_white(ORANGE, f64::from(alpha)));
        for hole in [[50.0, 60.0], [130.0, 40.0], [167.0, 75.0]] {
            assert!(near(px(hole[0], hole[1]), [255; 3], 1), "{name}: the hole at {hole:?} shows the page");
        }
        // The slits run level from each hole's right side to the contour around it. Along them
        // the fill is the fill, drawn once: no lighter seam, and no darker one where the
        // sliver over a slit lies on a translucent fill.
        for x in [72.0, 80.0, 88.0] {
            for dy in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                assert!(
                    near(px(x, 60.0 + dy), blue, 2),
                    "{name}: the donut's slit at {x}, {dy}: {:?}",
                    px(x, 60.0 + dy)
                );
            }
        }
        for (x, y) in [(147.0, 40.0), (150.0, 40.0), (185.0, 75.0)] {
            for dy in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                assert!(
                    near(px(x, y + dy), orange, 2),
                    "{name}: the plate's slit at {x}, {}: {:?}",
                    y + dy,
                    px(x, y + dy)
                );
            }
        }
        let body = format!(
            "<path d=\"M50,20A40,40 0 1 1 50,100A40,40 0 1 1 50,20ZM50,40A20,20 0 1 0 50,80A20,20 0 1 0 50,40Z\" fill=\"{BLUE}\" fill-opacity=\"{alpha}\"/>\
             <path d=\"M110,20L190,20L190,100L110,100ZM120,30L120,50L140,50L140,30ZM155,60L155,90L180,90L180,60Z\" fill=\"{ORANGE}\" fill-opacity=\"{alpha}\"/>"
        );
        as_resvg_draws(name, &ours, &body, scale);
    }
}

#[test]
fn a_translucent_stroke_is_one_colour_through_its_joints() {
    let scale = 2.0;
    let zigzag = [[20.0, 30.0], [70.0, 90.0], [120.0, 30.0], [170.0, 90.0]];
    let wide = look(10.0, Cap::Round, Join::Round, &[]);
    let stroked = figure(&polyline(&zigzag, false), false, Some(&wide), scale).el(None, Some(faded(BLUE, 0.5)));
    let ours = draw(stroked.into_iter().collect(), scale);
    let px = |p: Point| ours.at((p[0] * 2.0) as u32, (p[1] * 2.0) as u32);
    let want = over_white(BLUE, 0.5);
    let middle = |a: Point, b: Point| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    for joint in &zigzag[1..3] {
        assert!(near(px(*joint), want, 2), "the joint at {joint:?} is blended once: {:?} for {want:?}", px(*joint));
    }
    for pair in zigzag.windows(2) {
        assert!(near(px(middle(pair[0], pair[1])), want, 2), "and so is the middle of each leg");
    }
    let body = format!(
        "<path d=\"M20,30L70,90L120,30L170,90\" fill=\"none\" stroke=\"{BLUE}\" stroke-opacity=\"0.5\" stroke-width=\"10\" \
         stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
    );
    as_resvg_draws("translucent-stroke", &ours, &body, scale);
}

#[test]
fn corners_are_mitred_cut_or_round_and_ends_are_square_as_the_stroke_says() {
    let scale = 2.0;
    let wide = |cap, join| look(8.0, cap, join, &[]);
    let corner = polyline(&[[20.0, 100.0], [20.0, 20.0], [80.0, 20.0]], false);
    let needle = polyline(&[[100.0, 100.0], [115.0, 20.0], [130.0, 100.0]], false);
    let round = polyline(&[[150.0, 100.0], [150.0, 20.0], [185.0, 20.0]], false);
    let elements = vec![
        figure(&corner, false, Some(&wide(Cap::Butt, Join::Miter)), scale).el(None, Some(colour(INK))),
        figure(&needle, false, Some(&wide(Cap::Butt, Join::Miter)), scale).el(None, Some(colour(BLUE))),
        figure(&round, false, Some(&wide(Cap::Round, Join::Round)), scale).el(None, Some(colour(ORANGE))),
    ];
    let ours = draw(elements.into_iter().flatten().collect(), scale);
    let px = |x: f64, y: f64| ours.at((x * 2.0) as u32, (y * 2.0) as u32);
    assert!(near(px(16.5, 16.5), levels(INK), 2), "a mitred corner is filled out to its tip");
    assert!(near(px(16.5, 103.0), [255; 3], 2), "a butt end stops where the line does");
    assert!(near(px(16.5, 99.0), levels(INK), 2), "and is square up to there");
    // 21 degrees between the needle's legs is past the miter limit: the corner is cut straight
    // across between the legs' outer corners, 0.74 above its point, where a mitre would reach 21.7.
    assert!(near(px(115.0, 12.0), [255; 3], 2), "a needle's mitre is cut off: {:?}", px(115.0, 12.0));
    assert!(near(px(115.0, 18.0), [255; 3], 2), "just above the cut: {:?}", px(115.0, 18.0));
    assert!(near(px(115.0, 20.0), levels(BLUE), 2), "and filled below it: {:?}", px(115.0, 20.0));
    assert!(near(px(112.0, 20.0), levels(BLUE), 2), "out to the legs' outer corners: {:?}", px(112.0, 20.0));
    assert!(near(px(146.5, 16.5), [255; 3], 2), "a round corner leaves its tip empty");
    assert!(near(px(150.0, 103.0), levels(ORANGE), 2), "and a round end reaches past the line's end");

    let body = format!(
        "<path d=\"M20,100L20,20L80,20\" fill=\"none\" stroke=\"{INK}\" stroke-width=\"8\"/>\
         <path d=\"M100,100L115,20L130,100\" fill=\"none\" stroke=\"{BLUE}\" stroke-width=\"8\"/>\
         <path d=\"M150,100L150,20L185,20\" fill=\"none\" stroke=\"{ORANGE}\" stroke-width=\"8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
    );
    as_resvg_draws("corners-and-ends", &ours, &body, scale);
}

#[test]
fn a_rectangle_and_a_circle_are_boxes_with_the_border_on_the_outline() {
    let border = |width, join| look(width, Cap::Butt, join, &[]);
    let boxes = |scale: f32| {
        let _ = scale;
        vec![
            Figure::boxed([10.5, 10.5], [70.0, 40.0], 4.0, true, Some(&border(1.0, Join::Miter)))
                .and_then(|figure| figure.el(Some(colour("#f3f3f3")), Some(colour("#8a8a8a")))),
            Figure::boxed([100.3, 12.7], [40.0, 40.0], 20.0, true, Some(&border(6.0, Join::Miter)))
                .and_then(|figure| figure.el(Some(colour(BLUE)), Some(faded(ORANGE, 0.5)))),
            Figure::boxed([20.0, 70.0], [60.0, 30.0], 0.0, true, Some(&border(6.0, Join::Miter)))
                .and_then(|figure| figure.el(Some(colour(ORANGE)), Some(colour(INK)))),
            Figure::boxed([110.0, 70.0], [60.0, 30.0], 0.0, false, Some(&border(6.0, Join::Round)))
                .and_then(|figure| figure.el(None, Some(colour(BLUE)))),
        ]
    };
    let body = format!(
        "<rect x=\"10.5\" y=\"10.5\" width=\"70\" height=\"40\" rx=\"4\" fill=\"#f3f3f3\" stroke=\"#8a8a8a\"/>\
         <circle cx=\"120.3\" cy=\"32.7\" r=\"20\" fill=\"{BLUE}\" stroke=\"{ORANGE}\" stroke-opacity=\"0.5\" stroke-width=\"6\"/>\
         <rect x=\"20\" y=\"70\" width=\"60\" height=\"30\" fill=\"{ORANGE}\" stroke=\"{INK}\" stroke-width=\"6\"/>\
         <rect x=\"110\" y=\"70\" width=\"60\" height=\"30\" fill=\"none\" stroke=\"{BLUE}\" stroke-width=\"6\" stroke-linejoin=\"round\"/>"
    );
    for scale in [1.0, 2.0] {
        let ours = draw(boxes(scale).into_iter().flatten().collect(), scale);
        let px = |x: f64, y: f64| ours.at((x * f64::from(scale)) as u32, (y * f64::from(scale)) as u32);
        // A translucent border shows the fill under its inner half and the page under its outer:
        // the circle's outline is at 12.7 at its top, and the border runs 3 either side of it.
        let (inner, outer) = (px(120.3, 14.2), px(120.3, 11.0));
        let over_fill = levels(BLUE)
            .map(f64::from)
            .iter()
            .zip(levels(ORANGE))
            .map(|(f, o)| ((f + f64::from(o)) / 2.0).round() as u8)
            .collect::<Vec<_>>();
        assert!(
            near(inner, [over_fill[0], over_fill[1], over_fill[2]], 3),
            "the inner half is over the fill: {inner:?}"
        );
        assert!(near(outer, over_white(ORANGE, 0.5), 3), "the outer half is over the page: {outer:?}");
        assert!(near(px(17.5, 67.5), levels(INK), 2), "a mitred box is square at its outer corner");
        assert!(near(px(107.4, 67.4), [255; 3], 3), "a round-joined one is not: {:?}", px(107.4, 67.4));
        as_resvg_draws("boxes", &ours, &body, scale);
    }
    assert!(
        Figure::boxed([0.0; 2], [10.0; 2], 0.0, true, Some(&look(1.0, Cap::Butt, Join::Miter, &[3.0, 4.0]))).is_none(),
        "a dashed border is no box"
    );
}

#[test]
fn a_turned_shape_is_turned_point_by_point_with_its_dashes_and_arcs() {
    let scale = 2.0;
    let turn = Affine::rotate_about(30.0, [100.0, 60.0]);
    let pill = [
        Segment::Move([60.0, 45.0]),
        Segment::Line([140.0, 45.0]),
        Segment::Arc(Arc { radii: [15.0, 15.0], rotation: 0.0, large: false, sweep: true, to: [140.0, 75.0] }),
        Segment::Line([60.0, 75.0]),
        Segment::Arc(Arc { radii: [15.0, 15.0], rotation: 0.0, large: false, sweep: true, to: [60.0, 45.0] }),
        Segment::Close,
    ];
    let dashed = look(2.0, Cap::Butt, Join::Miter, &[6.0, 4.0]);
    let outline = Outline::lower(&pill).transformed(&turn);
    let turned = Figure::new(&outline, true, Some(&dashed), scale).expect("no crossing").expect("a pill");
    let ours = draw(turned.el(Some(colour("#f3f3f3")), Some(colour(INK))).into_iter().collect(), scale);
    let px = |p: Point| ours.at((p[0] * 2.0) as u32, (p[1] * 2.0) as u32);
    assert!(near(px(turn.apply([150.0, 60.0])), [0xf3; 3], 2), "the pill's right end is where the turn takes it");
    assert!(near(px([150.0, 60.0]), [255; 3], 2), "and not where it was");
    let body = format!(
        "<g transform=\"rotate(30 100 60)\"><path d=\"M60,45L140,45A15,15 0 0 1 140,75L60,75A15,15 0 0 1 60,45Z\" \
         fill=\"#f3f3f3\" stroke=\"{INK}\" stroke-width=\"2\" stroke-dasharray=\"6 4\"/></g>"
    );
    as_resvg_draws("turned-shape", &ours, &body, scale);
}

fn baselines(scale: f32) -> Baselines {
    Baselines::new(&[], "Inter", "Inter", scale)
}

fn run(content: &str, style: TextStyle, at: BaselinePoint, turn: f32, halo: Option<f32>, scale: f32) -> TextRun {
    TextRun::new(content.to_owned(), style, at, turn, halo, false, &mut baselines(scale))
}

#[test]
fn text_sits_on_its_baseline_from_its_anchor() {
    let scale = 2.0;
    let style = TextStyle::new(16.0, 400.0);
    for (anchor, name) in [(Anchor::Start, "start"), (Anchor::Middle, "middle"), (Anchor::End, "end")] {
        let at = BaselinePoint { x: 100.0, y: 60.0, anchor };
        let ours =
            draw(run("HHHH", style, at, 0.0, None, scale).el(Some(colour(INK)), None).into_iter().collect(), scale);
        let [left, top, right, bottom] = ink_box(&ours, levels(INK), 40).expect("ink");
        let (left, right) = (f64::from(left) / 2.0, f64::from(right + 1) / 2.0);
        let width = right - left;
        let want = match anchor {
            Anchor::Start => left,
            Anchor::Middle => (left + right) / 2.0,
            Anchor::End => right,
        };
        // An H has a side bearing of under a point at this size.
        assert!((want - 100.0).abs() <= 1.5, "anchored at its {name}: ink from {left} to {right}");
        assert!((40.0..60.0).contains(&width), "four capitals at 16 points: {width}");
        assert_eq!(bottom + 1, 120, "the capitals stand on the baseline at 60 points: bottom row {bottom}");
        assert!((f64::from(top) / 2.0 - (60.0 - 0.73 * 16.0)).abs() <= 1.0, "and rise a cap height above it: {top}");
    }
}

#[test]
fn weight_slant_and_underline_change_the_ink() {
    let scale = 2.0;
    let at = BaselinePoint { x: 20.0, y: 60.0, anchor: Anchor::Start };
    let ink = |style: TextStyle, underline: bool| {
        let run = TextRun::new("Hillside".to_owned(), style, at, 0.0, None, underline, &mut baselines(scale));
        let pixels = draw(run.el(Some(colour(INK)), None).into_iter().collect(), scale);
        let dark = pixels.rgba.chunks(4).map(|px| 255 - usize::from(px[1])).sum::<usize>();
        (pixels, dark)
    };
    let (regular, plain) = ink(TextStyle::new(16.0, 400.0), false);
    let (_, bold) = ink(TextStyle::new(16.0, 700.0), false);
    let (italic, slanted) = ink(TextStyle::new(16.0, 400.0).italic(), false);
    let (underlined, lined) = ink(TextStyle::new(16.0, 400.0), true);
    assert!(bold as f64 > 1.12 * plain as f64, "bold is heavier: {bold} against {plain}");
    assert!(slanted.abs_diff(plain) * 10 < plain, "italic is about as heavy: {slanted} against {plain}");
    let differ = regular.rgba.iter().zip(&italic.rgba).filter(|(a, b)| a != b).count();
    assert!(differ > 200, "and not the same glyphs: {differ} bytes differ");
    assert!(lined > plain, "an underline adds ink");
    // The underline is a row of ink below the baseline, as long as the run.
    let below: Vec<(u32, u32)> = (121..128).flat_map(|y| runs(&underlined, y, 0, 400, levels(INK))).collect();
    let longest = below.iter().map(|run| run.1).max().unwrap_or(0);
    let [left, _, right, _] = ink_box(&regular, levels(INK), 40).expect("ink");
    assert!(longest + 8 >= right - left, "a line {longest} px long under a run {} px long", right - left);
}

#[test]
fn turned_text_turns_about_its_anchor_on_the_baseline() {
    let scale = 2.0;
    let style = TextStyle::new(16.0, 400.0);
    let at = BaselinePoint { x: 100.0, y: 60.0, anchor: Anchor::Middle };
    let upright =
        draw(run("HHHH", style, at, 0.0, None, scale).el(Some(colour(INK)), None).into_iter().collect(), scale);
    let turned = run("HHHH", style, at, -(PI as f32) / 2.0, None, scale);
    let ours = draw(turned.el(Some(colour(INK)), None).into_iter().collect(), scale);
    common::save_in(RUN, &in_a_row(&[&upright, &ours], 8), "lowering/turned-text@2x.png");
    let [left, top, right, bottom] = ink_box(&upright, levels(INK), 60).expect("ink");
    let [t_left, t_top, t_right, t_bottom] = ink_box(&ours, levels(INK), 60).expect("turned ink");
    let (width, height) = (right - left, bottom - top);
    let (t_width, t_height) = (t_right - t_left, t_bottom - t_top);
    assert!(
        t_width.abs_diff(height) <= 2 && t_height.abs_diff(width) <= 2,
        "{width}x{height} turns to {t_width}x{t_height}"
    );
    // A quarter turn anticlockwise about (100, 60): the run reads upwards, centred on y = 60,
    // with its baseline on x = 100 and its capitals to the left of it.
    let middle = f64::from(t_top + t_bottom + 1) / 4.0;
    assert!((middle - 60.0).abs() <= 1.0, "centred on the anchor's y: {middle}");
    assert!((f64::from(t_right + 1) / 2.0 - 100.0).abs() <= 1.0, "the baseline is on the anchor's x: {t_right}");
    assert!((f64::from(t_left) / 2.0 - (100.0 - 0.73 * 16.0)).abs() <= 1.5, "the capitals rise to its left: {t_left}");

    // The same against resvg, in Inter on both sides, so that the glyphs are the same.
    let inter = std::env::var_os("RUNE_PELE_RESVG_FONT");
    if inter.is_some() {
        let body = format!(
            "<g transform=\"rotate(-90 100 60)\"><text x=\"100\" y=\"60\" text-anchor=\"middle\" font-size=\"16\" fill=\"{INK}\">HHHH</text></g>"
        );
        let theirs = rasterise(&svg(&body), scale);
        let [r_left, r_top, r_right, r_bottom] = ink_box(&theirs, levels(INK), 60).expect("resvg ink");
        for (ours, theirs) in [(t_left, r_left), (t_top, r_top), (t_right, r_right), (t_bottom, r_bottom)] {
            assert!(
                ours.abs_diff(theirs) <= 2,
                "the turned run's box {:?} against resvg's {:?}",
                [t_left, t_top, t_right, t_bottom],
                [r_left, r_top, r_right, r_bottom]
            );
        }
    }
}

#[test]
fn a_halo_is_an_outline_of_the_text_under_it() {
    let scale = 2.0;
    let style = TextStyle::new(16.0, 600.0);
    let at = BaselinePoint { x: 100.0, y: 60.0, anchor: Anchor::Middle };
    let plain =
        draw(run("Title", style, at, 0.0, None, scale).el(Some(colour(INK)), None).into_iter().collect(), scale);
    let haloed = run("Title", style, at, 0.0, Some(2.5), scale);
    let ours = draw(haloed.el(Some(colour(INK)), Some(colour(ORANGE))).into_iter().collect(), scale);
    common::save_in(RUN, &in_a_row(&[&plain, &ours], 8), "lowering/halo@2x.png");
    let [left, top, right, bottom] = ink_box(&plain, levels(INK), 60).expect("ink");
    let [h_left, h_top, h_right, h_bottom] = ink_box(&ours, levels(ORANGE), 30).expect("a halo");
    // 2.5 points is 5 pixels: the outline reaches that far past the glyphs on every side.
    for (inner, outer) in [(left, h_left), (top, h_top)] {
        assert!((4..=6).contains(&(inner - outer)), "the halo reaches {} px past the ink", inner - outer);
    }
    for (inner, outer) in [(right, h_right), (bottom, h_bottom)] {
        assert!((4..=6).contains(&(outer - inner)), "the halo reaches {} px past the ink", outer - inner);
    }
    let same_ink = |a: &Pixels, b: &Pixels| {
        a.rgba
            .chunks(4)
            .zip(b.rgba.chunks(4))
            .filter(|(p, q)| {
                near([p[0], p[1], p[2], 255], levels(INK), 30) != near([q[0], q[1], q[2], 255], levels(INK), 30)
            })
            .count()
    };
    assert!(same_ink(&plain, &ours) < 40, "the text itself is drawn as before, over the halo");
    let only_halo = run("Title", style, at, 0.0, Some(2.5), scale).el(None, Some(colour(ORANGE)));
    assert!(only_halo.is_some(), "a halo with no ink of its own is still drawn");
}

struct Viewer {
    diagram: Diagram,
    source: String,
}

const ER: &str = "erDiagram\n    CAR {\n        string plate\n        string make\n        string model\n    }\n    style CAR fill:#f96";

fn er_view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let options = ViewOptions::natural().pele_metrics().palette(Palette::pele());
    el().size(SIZE.0, 200.0).fill(Color::WHITE).child(s.diagram.view(cx, &s.source, options).expect("an ER diagram"))
}

#[test]
fn a_group_fades_as_a_whole_where_its_drawings_overlap() {
    use pele_core::scene::{Geometry, PaintVisit};
    let scale = 2.0;
    let capturer = Capturer::new();
    let viewer = Viewer { diagram: Diagram::new("er", FontCatalog::rune()), source: ER.to_owned() };
    let mut window = capturer.window(viewer, er_view, (SIZE.0, 200.0), Env::new(false, scale));
    let ours = window.capture();
    let scene = window.state().diagram.scene().expect("a native scene");
    // The stripes of the entity's rows are a group at 0.6. Its last row is two rectangles, one
    // over the top half of the other.
    let mut faded: Vec<[f64; 4]> = Vec::new();
    let mut depth_of_group = None;
    let mut depth = 0;
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => {
                depth += 1;
                if group.opacity == 0.6 {
                    depth_of_group = Some(depth);
                }
            }
            PaintVisit::End => {
                if depth_of_group == Some(depth) {
                    depth_of_group = None;
                }
                depth -= 1;
            }
            PaintVisit::Draw(draw) => {
                if let (Some(_), Geometry::Rect { origin, size, .. }) = (depth_of_group, &draw.geometry) {
                    faded.push([draw.origin.x + origin.x, draw.origin.y + origin.y, size.width, size.height]);
                }
            }
        }
    }
    let [.., whole, half] = faded[..] else { panic!("the last row's two rectangles: {faded:?}") };
    assert_eq!((whole[0], whole[1], whole[2]), (half[0], half[1], half[2]), "one lies over the top half of the other");
    assert!(half[3] < whole[3]);
    let px = |x: f64, y: f64| ours.at((x * 2.0) as u32, (y * 2.0) as u32);
    let x = whole[0] + whole[2] - 12.0;
    let (twice, once) = (px(x, whole[1] + half[3] / 2.0), px(x, whole[1] + (half[3] + whole[3]) / 2.0));
    // The shade over the fill at 0.6, once: white over #f96.
    let want = levels("#ff9966").map(|c| (f64::from(c) + (255.0 - f64::from(c)) * 0.6).round() as u8);
    assert!(near(once, want, 2), "the stripe is the shade at 0.6 over the fill: {once:?} for {want:?}");
    assert!(near(twice, want, 2), "and no lighter where two of its rectangles overlap: {twice:?} for {want:?}");

    let pele = pele_oracle::render_all(&[ER.to_owned()]);
    let pele_oracle::PeleRender::Svg { svg, .. } = &pele[0] else { panic!("Pele draws it: {pele:?}") };
    let theirs = rasterise(&common::as_a_page_reads(svg), scale);
    let (apart, differ) = compare(&ours, &theirs, &common::text_boxes(&scene), f64::from(scale));
    common::save_in(RUN, &in_a_row(&[&ours, &theirs, &marked(&ours, &differ)], 8), "lowering/group-opacity@2x.png");
    assert!(apart.largest_blob <= 12 && apart.share() <= 5.0 && apart.flat_error <= 4, "against Pele's own: {apart:?}");
}

/// Every glyph Pele draws itself: architecture's five icons, 24 points across, and treeview's
/// folder and file, 16 points across.
const GLYPHS: [(&str, &str); 2] = [
    (
        "architecture",
        "architecture-beta\n    service db(database)[DB]\n    service web(server)[Web]\n    service disk(disk)[Disk]\n    \
         service net(internet)[Net]\n    service sky(cloud)[Sky]",
    ),
    ("treeview", "---\nconfig:\n  treeView:\n    showIcons: true\n---\ntreeView-beta\n    src/\n        index.js"),
];

struct Glyphs {
    diagram: Diagram,
    source: &'static str,
    size: (f32, f32),
}

fn glyphs_view(s: &Glyphs, cx: &mut Cx<Glyphs>) -> El {
    let options = ViewOptions::natural().pele_metrics().palette(Palette::pele());
    let drawn = s.diagram.view(cx, s.source, options).expect("the glyphs draw");
    el().size(s.size.0, s.size.1).fill(Color::WHITE).child(drawn)
}

/// Each icon box of `scene` that holds a glyph: the glyph, and the box's corner and side in the
/// drawing.
fn glyph_boxes(scene: &pele_core::Scene) -> Vec<(pele_core::scene::IconGlyph, Point, f64)> {
    use pele_core::scene::{Geometry, PaintVisit};
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => match draw.geometry {
                Geometry::Icon { origin, size, glyph: Some(glyph), .. } => {
                    Some((glyph, [draw.origin.x + origin.x, draw.origin.y + origin.y], size.width))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn a_built_in_glyph_is_stroked_in_its_box_as_pele_draws_it() {
    use pele_core::scene::IconGlyph;
    let scale = 2.0;
    let capturer = Capturer::new();
    let mut drawn = Vec::new();
    for (kind, source) in GLYPHS {
        let pele = pele_core::render(source).expect("Pele draws it");
        let size = (pele.width.ceil() as f32, pele.height.ceil() as f32);
        let viewer = Glyphs { diagram: Diagram::new(kind, FontCatalog::rune()), source, size };
        let mut window = capturer.window(viewer, glyphs_view, size, Env::new(false, scale));
        let ours = window.capture();
        let theirs = rasterise(&common::as_a_page_reads(&pele.svg), scale);
        let scene = window.state().diagram.scene().expect("drawn natively");
        for (glyph, [x, y], side) in glyph_boxes(&scene) {
            let name = format!("glyph-{glyph:?}").to_lowercase();
            // A point of the grid, 24 across, as the glyph's box shows it, in device pixels.
            let k = side / 24.0;
            let at = |[gx, gy]: [f64; 2]| [(x + gx * k) * f64::from(scale), (y + gy * k) * f64::from(scale)];
            let px = |grid: [f64; 2]| {
                let [px, py] = at(grid);
                ours.at(px as u32, py as u32)
            };
            // A point on the outline, one inside it where nothing is drawn, and one on a side
            // that stands upright, where a row of pixels crosses the stroke square on.
            let (ink, on, off, upright) = match glyph {
                IconGlyph::Database => (INK, [12.0, 3.0], [12.0, 6.0], Some([4.0, 9.0])),
                IconGlyph::Server => (INK, [7.0, 7.5], [5.0, 9.5], Some([3.0, 9.5])),
                IconGlyph::Disk => (INK, [12.0, 10.5], [12.0, 8.5], Some([5.0, 12.0])),
                IconGlyph::Internet => (INK, [12.0, 12.0], [7.0, 8.0], None),
                IconGlyph::Cloud => (INK, [12.0, 18.0], [12.0, 13.0], None),
                IconGlyph::Folder => ("#6e6e6e", [3.0, 12.0], [12.0, 13.0], Some([3.0, 12.0])),
                IconGlyph::File => ("#6e6e6e", [5.0, 12.0], [12.0, 14.0], Some([5.0, 12.0])),
            };
            assert!(near(px(on), levels(ink), 3), "{name}: its outline at {on:?} is {ink}: {:?}", px(on));
            let behind = px(off);
            assert!(!near(behind, levels(ink), 40), "{name}: and nothing is drawn at {off:?}");
            if let Some([gx, gy]) = upright {
                // How much of each pixel along the row the stroke covers, summed: its width.
                let ([from, row], [to, _]) = (at([gx - 2.0, gy]), at([gx + 2.0, gy]));
                let shade = |x: u32| {
                    let level = f64::from(ours.at(x, row as u32)[0]);
                    ((f64::from(behind[0]) - level) / (f64::from(behind[0]) - f64::from(levels(ink)[0])))
                        .clamp(0.0, 1.0)
                };
                let width: f64 = (from as u32..to as u32).map(shade).sum();
                let want = 1.5 * k * f64::from(scale);
                println!("{name}@{scale}x: stroked {width:.2} px wide, for {want} px");
                assert!((width - want).abs() < 0.25, "{name}: stroked {want} px wide, 1.5 of 24: {width:.2} px");
            }
            let centre = ((x + side / 2.0) * f64::from(scale)) as u32;
            let middle = ((y + side / 2.0) * f64::from(scale)) as u32;
            let reach = ((side / 2.0 + 1.0) * f64::from(scale)) as u32;
            let (ours, theirs) =
                (around_pixel(&ours, (centre, middle), reach), around_pixel(&theirs, (centre, middle), reach));
            let (apart, differ) = compare(&ours, &theirs, &[], f64::from(scale));
            common::save_in(
                RUN,
                &in_a_row(&[&ours, &theirs, &marked(&ours, &differ)], 8),
                &format!("lowering/{name}@{scale}x.png"),
            );
            println!(
                "{name}@{scale}x: patch {} px, {:.2} unexplained in 10,000, flat {} levels",
                apart.largest_blob,
                apart.share(),
                apart.flat_error
            );
            assert!(apart.largest_blob as f32 <= 3.0 * scale * scale, "{name}: a patch of {} px", apart.largest_blob);
            assert!(apart.share() <= 5.0, "{name}: {:.1} unexplained px in 10,000", apart.share());
            assert!(apart.flat_error <= 4, "{name}: flat colours {} levels apart", apart.flat_error);
            drawn.push(glyph);
        }
    }
    let every = [IconGlyph::Database, IconGlyph::Server, IconGlyph::Disk, IconGlyph::Internet, IconGlyph::Cloud];
    assert!(every.iter().chain(&[IconGlyph::Folder, IconGlyph::File]).all(|glyph| drawn.contains(glyph)), "{drawn:?}");
}
