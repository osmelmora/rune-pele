//! Text placed by its baseline, measured in pixels from Rune's own renderer, headless.

mod common;

use std::fmt::Write as _;

use common::{out_dir, save};
use rune::{Color, El, TextStyle, el};
use rune_pele::capture::{Capturer, Pixels};
use rune_pele::text::{Anchor, BaselinePoint, Baselines};

const SIZES: [f32; 3] = [12.0, 14.0, 16.0];
const SCALES: [f32; 2] = [1.0, 2.0];
const GLYPHS: [&str; 2] = ["H", "x"];
/// Baselines on a whole point, and a third of the way between two.
const BASELINE_OFFSETS: [f32; 2] = [26.0, 26.3];
const CELL: [f32; 2] = [70.0, 36.0];
const GLYPH_X: f32 = 34.0;
const RULE: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };

/// How much ink covers a pixel on white: black text and the red rule both have no green.
fn ink(pixels: &Pixels, x: u32, y: u32) -> f32 {
    (255.0 - pixels.at(x, y)[1] as f32) / 255.0
}

fn rows(top: f32, height: f32, scale: f32) -> std::ops::Range<u32> {
    (top * scale).floor() as u32..((top + height) * scale).ceil() as u32
}

fn columns(left: f32, right: f32, scale: f32) -> std::ops::Range<u32> {
    (left * scale).floor() as u32..(right * scale).ceil() as u32
}

/// The bottom edge, in device px, of the lowest row that is more than half ink.
fn ink_bottom(pixels: &Pixels, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> Option<f32> {
    ys.rev().find(|&y| xs.clone().any(|x| ink(pixels, x, y) > 0.5)).map(|y| (y + 1) as f32)
}

/// The top edge, in device px, of a horizontal rule one device pixel thick: its ink's centre less
/// half a pixel, so an antialiased rule between two rows still reads where it was drawn.
fn rule_top(pixels: &Pixels, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> f32 {
    let (mut weight, mut moment) = (0.0, 0.0);
    for y in ys {
        let row: f32 = xs.clone().map(|x| ink(pixels, x, y)).sum::<f32>() / xs.len() as f32;
        weight += row;
        moment += row * (y as f32 + 0.5);
    }
    assert!(weight > 0.5, "the rule drew");
    moment / weight - 0.5
}

struct Cell {
    size: f32,
    glyph: &'static str,
    left: f32,
    top: f32,
    baseline: f32,
}

fn grid() -> Vec<Cell> {
    let mut cells = Vec::new();
    for (row, &size) in SIZES.iter().enumerate() {
        let top = row as f32 * CELL[1];
        let columns = BASELINE_OFFSETS.iter().flat_map(|&offset| GLYPHS.iter().map(move |&glyph| (offset, glyph)));
        for (column, (offset, glyph)) in columns.enumerate() {
            let left = column as f32 * CELL[0];
            cells.push(Cell { size, glyph, left, top, baseline: top + offset });
        }
    }
    cells
}

fn grid_view(cells: &[Cell], baselines: &mut Baselines, scale: f32) -> El {
    let mut root = el().size(CELL[0] * 4.0, CELL[1] * 3.0).fill(Color::WHITE);
    for cell in cells {
        let style = TextStyle::new(cell.size, 400.0);
        let x = cell.left + GLYPH_X;
        let advance = baselines.line(cell.glyph, style).advance;
        let at = BaselinePoint { x, y: cell.baseline, anchor: Anchor::Start };
        root = root
            .child(
                el().absolute(cell.left + 4.0, cell.baseline).size(x - 4.0 - (cell.left + 4.0), 1.0 / scale).fill(RULE),
            )
            .child(
                el().absolute(x + advance + 4.0, cell.baseline)
                    .size(cell.left + CELL[0] - 4.0 - (x + advance + 4.0), 1.0 / scale)
                    .fill(RULE),
            )
            .child(baselines.place(cell.glyph, style, at).color(Color::BLACK));
    }
    root
}

#[test]
fn glyph_baselines_land_on_the_rule_at_every_size_and_scale() {
    let capturer = Capturer::new();
    let cells = grid();
    let mut table =
        String::from("scale size glyph baseline_pt  ink_bottom_px rule_top_px exact_px  err_vs_rule err_vs_exact\n");
    let (mut worst, mut off_rule, mut past_rounding) = (0.0f32, 0.0f32, 0.0f32);
    for scale in SCALES {
        let mut baselines = Baselines::new(&[], "Inter", "Inter", scale);
        let view = grid_view(&cells, &mut baselines, scale);
        let pixels = capturer.capture(view, (CELL[0] * 4.0, CELL[1] * 3.0), scale);
        save(&pixels, &format!("baseline-grid@{scale}x.png"));
        for cell in &cells {
            let style = TextStyle::new(cell.size, 400.0);
            let x = cell.left + GLYPH_X;
            let advance = baselines.line(cell.glyph, style).advance;
            let ys = rows(cell.top, CELL[1], scale);
            let bottom = ink_bottom(&pixels, columns(x, x + advance, scale), ys.clone())
                .unwrap_or_else(|| panic!("{} at {}pt, {scale}x, drew no ink", cell.glyph, cell.size));
            let rule = rule_top(&pixels, columns(cell.left + 6.0, x - 6.0, scale), ys);
            let exact = cell.baseline * scale;
            let (vs_rule, vs_exact) = (bottom - rule, bottom - exact);
            worst = worst.max(vs_rule.abs()).max(vs_exact.abs());
            off_rule = off_rule.max(vs_rule.abs());
            past_rounding = past_rounding.max(vs_exact.abs());
            writeln!(
                table,
                "{scale:>5} {:>4} {:>5} {:>11.1}  {bottom:>13.2} {rule:>11.2} {exact:>8.2}  {vs_rule:>+11.2} {vs_exact:>+12.2}",
                cell.size, cell.glyph, cell.baseline
            )
            .unwrap();
        }
    }
    writeln!(table, "worst |error| {worst:.2} device px").unwrap();
    print!("{table}");
    std::fs::create_dir_all(out_dir()).unwrap();
    std::fs::write(out_dir().join("baseline-errors.txt"), &table).unwrap();
    assert!(worst <= 1.0, "every baseline within 1 device pixel:\n{table}");
    assert!(
        off_rule == 0.0 && past_rounding <= 0.5,
        "every baseline on the rule's row, y rounded to a pixel:\n{table}"
    );
}

#[test]
fn anchors_shift_the_run_by_its_advance() {
    let scale = 2.0;
    let capturer = Capturer::new();
    let mut baselines = Baselines::new(&[], "Inter", "Inter", scale);
    let style = TextStyle::new(16.0, 400.0);
    let advance = baselines.line("Hxa", style).advance;
    let anchors = [Anchor::Start, Anchor::Middle, Anchor::End];
    let mut root =
        el().size(160.0, 90.0).fill(Color::WHITE).child(el().absolute(80.0, 0.0).size(1.0 / scale, 90.0).fill(RULE));
    for (i, &anchor) in anchors.iter().enumerate() {
        let at = BaselinePoint { x: 80.0, y: 24.0 + 28.0 * i as f32, anchor };
        root = root.child(baselines.place("Hxa", style, at).color(Color::BLACK));
    }
    let pixels = capturer.capture(root, (160.0, 90.0), scale);
    save(&pixels, "anchors@2x.png");
    let ink_left = |i: usize| {
        let ys = rows(24.0 + 28.0 * i as f32 - 14.0, 12.0, scale);
        let x = (0..pixels.width).find(|&x| ys.clone().any(|y| pixels.at(x, y)[0] < 128));
        x.expect("the run drew") as f32
    };
    let (start, middle, end) = (ink_left(0), ink_left(1), ink_left(2));
    let want = advance * scale;
    assert!(
        (start - middle - want / 2.0).abs() <= 1.0,
        "middle sits half an advance left of start: {start} {middle} {want}"
    );
    assert!((start - end - want).abs() <= 1.0, "end sits a whole advance left of start: {start} {end} {want}");
}

struct Shown {
    diagram: rune_pele::Diagram,
    source: String,
}

fn shown_view(s: &Shown, cx: &mut rune::Cx<Shown>) -> El {
    s.diagram.view(cx, &s.source, rune_pele::ViewOptions::natural()).expect("it draws")
}

/// `FontCatalog::rune()` measures in the families a window has when the app names none, so each
/// label, sans or monospace, is laid out as wide as the window draws it. Railroad's terminals and
/// event modeling's fields are monospace.
#[test]
fn runes_own_catalog_measures_each_label_in_the_face_a_default_window_draws_it_in() {
    use pele_core::scene::{Geometry, PaintVisit};
    for fixture in ["railroad", "eventmodeling"] {
        let path = pele_oracle::root().join(format!("tests/fixtures/rune/{fixture}.mmd"));
        let source = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let shown = Shown { diagram: rune_pele::Diagram::new("diagram", rune_pele::FontCatalog::rune()), source };
        let mut rt = rune::Runtime::new(shown, shown_view, (1600.0, 1200.0), rune::Env::new(false, 2.0));
        rt.frame(0.0);
        let drawn: std::collections::HashMap<String, f32> =
            rt.elements().into_iter().filter_map(|element| Some((element.key?, element.rect[2]))).collect();
        let scene = rt.state.diagram.scene().expect("drawn natively");
        let mut runs = [0, 0];
        for visit in scene.paint() {
            let PaintVisit::Draw(draw) = visit else { continue };
            let Geometry::Text(run) = &draw.geometry else { continue };
            if run.text.trim().is_empty() {
                continue;
            }
            let key = draw.key.to_string();
            let width = drawn.get(&key).unwrap_or_else(|| panic!("{fixture}: {key} is an element"));
            assert!(
                (f64::from(*width) - run.advance).abs() < 0.01,
                "{fixture}: {:?} in {:?} measured {} wide and drawn {width} wide",
                run.text,
                run.font.family,
                run.advance
            );
            runs[usize::from(run.font.family == pele_core::FontFamily::Mono)] += 1;
        }
        assert!(runs[1] > 0, "{fixture} draws monospace labels: {runs:?} sans and mono");
    }
}
