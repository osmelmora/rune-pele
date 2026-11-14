//! Lowered pie geometry, measured in pixels from Rune's own renderer, headless.

mod common;

use std::f64::consts::TAU;

use common::save;
use rune::color::hex;
use rune::{Color, El, canvas, el};
use rune_pele::capture::Capturer;
use rune_pele::lower::{Outline, Stroke, ring_wedge, wedge};

const BACKGROUND: Color = Color::WHITE;
const RADIUS: f64 = 120.0;
const SHARES: [f64; 2] = [0.79, 0.21];
const SERIES: [&str; 2] = ["#4e79a7", "#f28e2b"];

fn pie_view(centre: [f64; 2], inner: f64) -> El {
    let mut root = el().size(300.0, 300.0).fill(BACKGROUND);
    let mut start = 0.0;
    for (share, series) in SHARES.into_iter().zip(SERIES) {
        let end = start + share * TAU;
        let program = if inner > 0.0 { ring_wedge(start, end, RADIUS, inner) } else { wedge(start, end, RADIUS) };
        let fill = hex(series).expect("a series colour");
        let shape = Outline::lower(&program)
            .translated(centre)
            .paint(Some(fill), Some(Stroke { width: 1.0, color: BACKGROUND }))
            .expect("a slice is one simple contour");
        root = root.child(canvas(shape).absolute(0.0, 0.0).size(300.0, 300.0));
        start = end;
    }
    root
}

fn rgb(hex: &str) -> [u8; 3] {
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex");
    [byte(1), byte(3), byte(5)]
}

fn near(got: [u8; 4], want: [u8; 3]) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 3)
}

/// How much of a pixel `series` covers over white, counting antialiased edges by their share:
/// zero for a pixel that is not a blend of the two.
fn coverage(px: &[u8], series: [u8; 3]) -> f64 {
    let towards: Vec<f64> = series.iter().map(|&c| 255.0 - c as f64).collect();
    let got: Vec<f64> = px[..3].iter().map(|&c| 255.0 - c as f64).collect();
    let length: f64 = towards.iter().map(|t| t * t).sum();
    let f = (got.iter().zip(&towards).map(|(g, t)| g * t).sum::<f64>() / length).clamp(0.0, 1.0);
    let off: f64 = got.iter().zip(&towards).map(|(g, t)| (g - f * t).powi(2)).sum::<f64>().sqrt();
    if off <= 6.0 { f } else { 0.0 }
}

/// The device pixel at `angle` radians clockwise from twelve and `radius` points from `centre`.
fn polar_px(centre: [f64; 2], angle: f64, radius: f64, scale: f64) -> (u32, u32) {
    let x = (centre[0] + angle.sin() * radius) * scale;
    let y = (centre[1] - angle.cos() * radius) * scale;
    (x as u32, y as u32)
}

#[test]
fn the_two_slice_pie_fills_each_share_and_seams_them_in_the_background() {
    let (scale, centre) = (2.0, [150.0, 150.0]);
    let capturer = Capturer::new();
    let pixels = capturer.capture(pie_view(centre, 0.0), (300.0, 300.0), scale as f32);
    save(&pixels, "pie@2x.png");
    save(&capturer.capture(pie_view(centre, 0.0), (300.0, 300.0), 1.0), "pie@1x.png");
    let white = [255, 255, 255];
    let mut start = 0.0;
    for (share, series) in SHARES.into_iter().zip(SERIES) {
        let mid = start + share * TAU / 2.0;
        let (x, y) = polar_px(centre, mid, RADIUS / 2.0, scale);
        assert!(near(pixels.at(x, y), rgb(series)), "{series} at its slice's middle: {:?}", pixels.at(x, y));
        let (x, y) = polar_px(centre, start, RADIUS / 2.0, scale);
        assert!(near(pixels.at(x, y), white), "the seam at the slice's start is background: {:?}", pixels.at(x, y));
        let (x, y) = polar_px(centre, mid, RADIUS + 3.0, scale);
        assert!(near(pixels.at(x, y), white), "outside the rim is background: {:?}", pixels.at(x, y));
        let painted: f64 = pixels.rgba.chunks(4).map(|px| coverage(px, rgb(series))).sum();
        let (r, w) = (RADIUS * scale, scale);
        let perimeter = 2.0 * r + share * TAU * r;
        let expected = share * TAU / 2.0 * r * r - perimeter * w / 2.0;
        let error = painted / expected - 1.0;
        println!("{series} {share}: {painted:.0} px painted, {expected:.0} expected ({error:+.4})");
        assert!(
            error.abs() < 0.01,
            "{series} covers its share less the stroke: {painted} px of {expected:.0} ({error:+.4})"
        );
        start += share * TAU;
    }
}

#[test]
fn a_donut_keeps_its_hole_and_its_ring_slices() {
    let (scale, centre, inner) = (2.0, [150.0, 150.0], 60.0);
    let pixels = Capturer::new().capture(pie_view(centre, inner), (300.0, 300.0), scale as f32);
    save(&pixels, "donut@2x.png");
    let (x, y) = polar_px(centre, 1.0, inner / 2.0, scale);
    assert!(near(pixels.at(x, y), [255, 255, 255]), "the hole shows the background: {:?}", pixels.at(x, y));
    let mut start = 0.0;
    for (share, series) in SHARES.into_iter().zip(SERIES) {
        let (x, y) = polar_px(centre, start + share * TAU / 2.0, (RADIUS + inner) / 2.0, scale);
        assert!(near(pixels.at(x, y), rgb(series)), "{series} mid-ring: {:?}", pixels.at(x, y));
        start += share * TAU;
    }
}
