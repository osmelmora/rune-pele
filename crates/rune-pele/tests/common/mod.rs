//! What the view tests share: where their PNGs land, Pele's SVG rasterised by resvg, and the
//! comparison of two drawings of one diagram.
//!
//! PNGs land in `$RUNE_PELE_OUT`, or in `out/verify/<run>` at the workspace root when it is unset.

#![allow(dead_code, reason = "each test binary uses its own part of this module")]

use std::path::PathBuf;

use pele_core::Scene;
use pele_core::scene::{AnchorKind, Geometry, PaintVisit};
use rune_pele::capture::Pixels;
use rune_pele::geometry::Affine;
use rune_pele::lower::group_map;

pub fn out_dir_for(run: &str) -> PathBuf {
    std::env::var_os("RUNE_PELE_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/verify").join(run))
}

pub fn out_dir() -> PathBuf {
    out_dir_for("rune-spike")
}

pub fn save_in(run: &str, pixels: &Pixels, name: &str) {
    let path = out_dir_for(run).join(name);
    pixels.write_png(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

pub fn save(pixels: &Pixels, name: &str) {
    save_in("rune-spike", pixels, name);
}

/// The value of a root style declaration, such as one of Pele's private aliases.
fn root_declaration(svg: &str, name: &str) -> Option<String> {
    let style_at = svg.find("style=\"").expect("the root has a style") + 7;
    let style = &svg[style_at..style_at + svg[style_at..].find('"').expect("a closed style")];
    let mut declarations = style.split(';').filter_map(|declaration| declaration.split_once(':'));
    declarations.find(|(declared, _)| *declared == name).map(|(_, value)| value.to_owned())
}

/// Pele's SVG as a page without CSS of its own reads it, in the attributes resvg reads. resvg
/// has no CSS variables, so each `var()` becomes the root's own declaration of the variable,
/// which every element inherits, or else the variable's fallback. The root declares Pele's
/// private aliases, and the series colours a source sets in its theme. resvg does not read the
/// root's `font` shorthand, so that size becomes the root's `font-size`, nor a corner radius in
/// a `style`, so that becomes the element's `rx`.
pub fn as_a_page_reads(svg: &str) -> String {
    let font = root_declaration(svg, "font").expect("the root sets a font");
    let size = font.split("px").next().expect("a size in px");
    let mut out = svg.replacen("<svg ", &format!("<svg font-size=\"{size}\" "), 1);
    while let Some(at) = out.find("var(") {
        let end = at + out[at..].find(')').expect("a closed var()");
        let inner = &out[at + 4..end];
        let (name, fallback) = inner.split_once(',').unwrap_or((inner, ""));
        let value = root_declaration(svg, name).unwrap_or_else(|| fallback.to_owned());
        out.replace_range(at..=end, &value);
    }
    let mut tags: Vec<String> = out.split('<').map(str::to_owned).collect();
    for tag in tags.iter_mut().skip(2) {
        let Some(style_at) = tag.find(" style=\"") else { continue };
        let style = &tag[style_at + 8..];
        let style = &style[..style.find('"').expect("a closed style")];
        let radius = style.split(';').filter_map(|declaration| declaration.strip_prefix("rx:")).next_back();
        if let Some(radius) = radius {
            let radius = radius.trim_end_matches("px").to_owned();
            // A declaration wins over the attribute of the same name.
            if let Some(own) = tag.find(" rx=\"") {
                let end = own + 5 + tag[own + 5..].find('"').expect("a closed rx") + 1;
                tag.replace_range(own..end, "");
            }
            let style_at = tag.find(" style=\"").expect("found above");
            tag.insert_str(style_at, &format!(" rx=\"{radius}\""));
        }
    }
    tags.join("<")
}

/// Arial, whose metrics Pele lays out with, or a font with the same metrics: the regular and
/// bold faces. `RUNE_PELE_RESVG_FONT` names another regular face, which then draws bold too.
pub fn arial() -> Vec<String> {
    if let Some(path) = std::env::var_os("RUNE_PELE_RESVG_FONT") {
        return vec![path.to_string_lossy().into_owned()];
    }
    let candidates = [
        ["/System/Library/Fonts/Supplemental/Arial.ttf", "/System/Library/Fonts/Supplemental/Arial Bold.ttf"],
        ["C:\\Windows\\Fonts\\arial.ttf", "C:\\Windows\\Fonts\\arialbd.ttf"],
        [
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        ],
    ];
    let found = candidates.iter().find(|faces| faces.iter().all(|face| std::path::Path::new(face).exists()));
    let faces = found.expect("an Arial-compatible font, or RUNE_PELE_RESVG_FONT naming one, such as Rune's Inter");
    faces.iter().map(|face| (*face).to_owned()).collect()
}

/// `svg` drawn by resvg on white, in sRGB, at `scale` device pixels a point.
pub fn rasterise(svg: &str, scale: f32) -> Pixels {
    let mut options = resvg::usvg::Options::default();
    let fonts = options.fontdb_mut();
    for face in arial() {
        fonts.load_font_data(std::fs::read(&face).unwrap_or_else(|e| panic!("{face}: {e}")));
    }
    let family = fonts.faces().next().expect("a face").families[0].0.clone();
    fonts.set_sans_serif_family(&family);
    fonts.set_monospace_family(&family);
    options.font_family = family;
    let tree = resvg::usvg::Tree::from_str(svg, &options).expect("Pele's SVG parses");
    let size = tree.size();
    let (width, height) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).expect("a pixmap");
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Pixels { width, height, rgba: pixmap.take() }
}

/// [`rasterise`] with each pixel the mean of `times` by `times` of resvg's pixels at `times` the
/// scale. resvg takes a pixel's coverage in quarters of it across, so that at a scale that puts
/// an upright edge between quarters it is up to an eighth of a pixel off; the mean takes it to
/// a `4 * times`-th. resvg blends in sRGB as it is stored, so the mean is still a blend by the
/// area covered.
pub fn rasterise_fine(svg: &str, scale: f32, times: u32) -> Pixels {
    let fine = rasterise(svg, scale * times as f32);
    let (width, height) = (fine.width.div_ceil(times), fine.height.div_ceil(times));
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            let mut count = 0;
            for fy in y * times..((y + 1) * times).min(fine.height) {
                for fx in x * times..((x + 1) * times).min(fine.width) {
                    let px = fine.at(fx, fy);
                    for channel in 0..4 {
                        sum[channel] += u32::from(px[channel]);
                    }
                    count += 1;
                }
            }
            rgba.extend(sum.map(|total| ((total + count / 2) / count) as u8));
        }
    }
    Pixels { width, height, rgba }
}

/// The images in a row, `gap` pixels apart, on white.
pub fn in_a_row(images: &[&Pixels], gap: u32) -> Pixels {
    let width = images.iter().map(|image| image.width).sum::<u32>() + gap * (images.len() as u32 - 1);
    let height = images.iter().map(|image| image.height).max().expect("an image");
    let mut rgba = vec![255; (width * height * 4) as usize];
    let mut offset = 0;
    for image in images {
        for y in 0..image.height {
            for x in 0..image.width {
                let at = ((y * width + x + offset) * 4) as usize;
                rgba[at..at + 4].copy_from_slice(&image.at(x, y));
            }
        }
        offset += image.width + gap;
    }
    Pixels { width, height, rgba }
}

/// `image` with each pixel drawn `times` across and down, so that single pixels can be told apart.
pub fn magnified(image: &Pixels, times: u32) -> Pixels {
    let (width, height) = (image.width * times, image.height * times);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend(image.at(x / times, y / times));
        }
    }
    Pixels { width, height, rgba }
}

/// The part of `image` within `reach` pixels of `(x, y)`.
pub fn around_pixel(image: &Pixels, (x, y): (u32, u32), reach: u32) -> Pixels {
    let (left, top) = (x.saturating_sub(reach), y.saturating_sub(reach));
    let (right, bottom) = ((x + reach).min(image.width), (y + reach).min(image.height));
    let mut rgba = Vec::new();
    for y in top..bottom {
        for x in left..right {
            rgba.extend(image.at(x, y));
        }
    }
    Pixels { width: right - left, height: bottom - top, rgba }
}

pub fn side_by_side(left: &Pixels, right: &Pixels, gap: u32) -> Pixels {
    in_a_row(&[left, right], gap)
}

/// The box of every text run of `scene`, in points as `[left, top, right, bottom]`, with room
/// for the same run set in a wider font and for its outline: 15% of its advance and 3 points at
/// each end, from a font size and 3 points above its baseline to 0.4 of one and 3 points below.
/// A run in a turned group is the box around its turned box.
pub fn text_boxes(scene: &Scene) -> Vec<[f64; 4]> {
    let mut maps = vec![Affine::IDENTITY];
    let mut boxes = Vec::new();
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => {
                maps.push(maps.last().expect("the root").then(&group_map(&group)));
            }
            PaintVisit::End => {
                maps.pop();
            }
            PaintVisit::Draw(draw) => {
                let Geometry::Text(run) = &draw.geometry else { continue };
                let lead = match run.anchor {
                    AnchorKind::Start => 0.0,
                    AnchorKind::Middle => run.advance / 2.0,
                    AnchorKind::End => run.advance,
                };
                let room = 0.15 * run.advance + 3.0;
                let (left, right) = (run.baseline.x - lead - room, run.baseline.x - lead + run.advance + room);
                let (top, bottom) = (run.baseline.y - run.font.size - 3.0, run.baseline.y + 0.4 * run.font.size + 3.0);
                let map = maps.last().expect("the root");
                let corners = [[left, top], [right, top], [right, bottom], [left, bottom]].map(|p| map.apply(p));
                let least = |axis: usize| corners.iter().map(|c| c[axis]).fold(f64::MAX, f64::min);
                let most = |axis: usize| corners.iter().map(|c| c[axis]).fold(f64::MIN, f64::max);
                boxes.push([least(0), least(1), most(0), most(1)]);
            }
        }
    }
    boxes
}

// What counts as drawing a source as Pele does. Text is left out: it is Inter here and Arial
// in resvg. Every other pixel is compared with the pixels within one of it in the other
// drawing, so that an edge half a pixel apart is no difference. The three limits were set
// before any source was compared, from what Pele draws rather than from what the comparison
// found.

/// The largest patch of pixels one drawing has and the other has nothing like, in square points.
/// The smallest things Pele draws are a 1 point stroke and an arrowhead of about 30 square
/// points. A missing or misplaced one leaves a patch of 5 or more, and a wrong dash gap one of 4.
pub const LARGEST_PATCH: f64 = 3.0;

/// Such pixels in every 10,000 compared. Patches too small to count one by one still add up:
/// a dashed edge drawn solid, or the other way round, is some 50 pixels of a 100,000 pixel
/// drawing.
pub const MOST_UNEXPLAINED: f64 = 5.0;

/// The error in levels of 255 at the 99.9th percentile of the pixels that are flat in both
/// drawings: fills and the page. The closest two fills of Pele's are 12 levels apart, a fill at
/// half opacity is 6 from the page, and two renderers round one colour to within a level or two.
pub const FLAT_ERROR: u8 = 4;

// At one device pixel a point, a 1 point stroke is one pixel wide, and what a renderer does
// with a pixel the stroke half covers is most of what it draws. Rune takes a stroke's coverage
// from the distance to its centre line and resvg from the area covered, which on a slanted
// hairline differ by up to a tenth of full coverage: 20 levels of a dark edge, all along it.
// So the first two limits are held at 2x, where they pass, and at 1x the sources over them
// are counted and listed. This limit, set on finding that, is what a 1x drawing is held to
// instead, with the flat colours.

/// The largest patch a 1x drawing may have, in pixels: under the smallest arrowhead, so that
/// nothing Pele draws can be missing or out of place.
pub const LARGEST_PATCH_1X: usize = 30;

/// The limits `apart` is over, each as a few words, for drawings at `scale` device pixels a
/// point. `sharp` are the first two limits, of the drawings pixel for pixel. Under 2 pixels a
/// point the 1x rules apply: the sharp limits are listed and not held, and the patch limit is
/// [`LARGEST_PATCH_1X`].
pub fn over(apart: &Apart, scale: f64) -> (Vec<String>, Vec<String>) {
    let (mut sharp, mut other) = (Vec::new(), Vec::new());
    if apart.largest_blob as f64 > LARGEST_PATCH * scale * scale {
        sharp.push(format!("a patch of {} px", apart.largest_blob));
    }
    if apart.share() > MOST_UNEXPLAINED {
        sharp.push(format!("{:.1} unexplained px in 10,000", apart.share()));
    }
    if apart.flat_error > FLAT_ERROR {
        other.push(format!("flat colours {} levels apart", apart.flat_error));
    }
    if scale < 2.0 && apart.largest_blob > LARGEST_PATCH_1X {
        other.push(format!("a patch of {} px at 1x", apart.largest_blob));
    }
    (sharp, other)
}

/// How far two drawings of one diagram are apart, over the pixels that are not text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Apart {
    /// Pixels compared: those outside every text box.
    pub compared: usize,
    /// Pixels whose colour in one drawing the other has nowhere within a pixel of them.
    pub unexplained: usize,
    /// The most such pixels that touch one another.
    pub largest_blob: usize,
    /// A pixel of that patch.
    pub blob_at: (u32, u32),
    /// The error at the 99.9th percentile of the flat pixels, in levels of 255: those where
    /// both drawings are one colour for a pixel around.
    pub flat_error: u8,
}

impl Apart {
    /// Unexplained pixels in every 10,000 compared.
    pub fn share(&self) -> f64 {
        self.unexplained as f64 * 10_000.0 / self.compared.max(1) as f64
    }
}

/// A colour within this many levels of the other drawing's colours near it is taken for the same.
const NEAR: i16 = 16;

/// A pixel whose neighbours span no more levels than this is flat.
const FLAT: i16 = 2;

/// The least and greatest of each channel over the three by three pixels around `(x, y)`.
fn around(image: &Pixels, x: u32, y: u32) -> ([i16; 3], [i16; 3]) {
    let (mut low, mut high) = ([255i16; 3], [0i16; 3]);
    for ny in y.saturating_sub(1)..=(y + 1).min(image.height - 1) {
        for nx in x.saturating_sub(1)..=(x + 1).min(image.width - 1) {
            let px = image.at(nx, ny);
            for channel in 0..3 {
                low[channel] = low[channel].min(i16::from(px[channel]));
                high[channel] = high[channel].max(i16::from(px[channel]));
            }
        }
    }
    (low, high)
}

/// Compares `ours` with `theirs` outside `text` (boxes in points, at `scale` pixels a point).
/// Also answers the unexplained pixels, for a picture of where the drawings differ.
pub fn compare(ours: &Pixels, theirs: &Pixels, text: &[[f64; 4]], scale: f64) -> (Apart, Vec<(u32, u32)>) {
    compare_with_any(ours, &[theirs], text, scale)
}

/// [`compare`] with several drawings of Pele's, each as right as the others: a pixel is
/// unexplained only where none of them explains it. The flat colours are compared with the first.
pub fn compare_with_any(
    ours: &Pixels,
    references: &[&Pixels],
    text: &[[f64; 4]],
    scale: f64,
) -> (Apart, Vec<(u32, u32)>) {
    let theirs = references[0];
    let width = references.iter().map(|other| other.width).fold(ours.width, u32::min);
    let height = references.iter().map(|other| other.height).fold(ours.height, u32::min);
    let mut masked = vec![false; (width * height) as usize];
    for [left, top, right, bottom] in text {
        let span = |from: f64, to: f64, most: u32| {
            ((from * scale).floor().max(0.0) as u32).min(most)..((to * scale).ceil().max(0.0) as u32).min(most)
        };
        for y in span(*top, *bottom, height) {
            for x in span(*left, *right, width) {
                masked[(y * width + x) as usize] = true;
            }
        }
    }
    let mut apart = Apart::default();
    let mut unexplained = vec![false; (width * height) as usize];
    let mut flat_errors = [0usize; 256];
    for y in 0..height {
        for x in 0..width {
            if masked[(y * width + x) as usize] {
                continue;
            }
            apart.compared += 1;
            let (a, b) = (ours.at(x, y), theirs.at(x, y));
            let ((a_low, a_high), (b_low, b_high)) = (around(ours, x, y), around(theirs, x, y));
            let within = |px: [u8; 4], low: [i16; 3], high: [i16; 3]| {
                (0..3).all(|c| (low[c] - NEAR..=high[c] + NEAR).contains(&i16::from(px[c])))
            };
            let explains = |other: &Pixels| {
                let (b, (b_low, b_high)) = (other.at(x, y), around(other, x, y));
                within(a, b_low, b_high) && within(b, a_low, a_high)
            };
            if !references.iter().any(|other| explains(other)) {
                unexplained[(y * width + x) as usize] = true;
                apart.unexplained += 1;
            }
            let flat = |low: [i16; 3], high: [i16; 3]| (0..3).all(|c| high[c] - low[c] <= FLAT);
            if flat(a_low, a_high) && flat(b_low, b_high) {
                let error = (0..3).map(|c| a[c].abs_diff(b[c])).max().expect("three channels");
                flat_errors[usize::from(error)] += 1;
            }
        }
    }
    let flat: usize = flat_errors.iter().sum();
    let mut above = 0;
    for error in (0..256).rev() {
        above += flat_errors[error];
        if above * 1000 > flat {
            apart.flat_error = error as u8;
            break;
        }
    }
    let marked: Vec<(u32, u32)> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&(x, y)| unexplained[(y * width + x) as usize])
        .collect();
    let mut seen = vec![false; (width * height) as usize];
    for &(x, y) in &marked {
        if seen[(y * width + x) as usize] {
            continue;
        }
        let mut blob = 0;
        let mut queue = vec![(x, y)];
        seen[(y * width + x) as usize] = true;
        while let Some((x, y)) = queue.pop() {
            blob += 1;
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let at = (ny * width + nx) as usize;
                    if unexplained[at] && !seen[at] {
                        seen[at] = true;
                        queue.push((nx, ny));
                    }
                }
            }
        }
        if blob > apart.largest_blob {
            (apart.largest_blob, apart.blob_at) = (blob, (x, y));
        }
    }
    (apart, marked)
}

/// `image` greyed, with the pixels of `marked` in red: where two drawings differ.
pub fn marked(image: &Pixels, marked: &[(u32, u32)]) -> Pixels {
    let mut rgba: Vec<u8> = image
        .rgba
        .chunks(4)
        .flat_map(|px| {
            let grey = 160 + ((u16::from(px[0]) + u16::from(px[1]) + u16::from(px[2])) / 3 * 95 / 255) as u8;
            [grey, grey, grey, 255]
        })
        .collect();
    for &(x, y) in marked {
        let at = ((y * image.width + x) * 4) as usize;
        rgba[at..at + 4].copy_from_slice(&[220, 0, 0, 255]);
    }
    Pixels { width: image.width, height: image.height, rgba }
}
