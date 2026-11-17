//! Spaces in a label are drawn as Pele's SVG shows them. Where Pele writes the text plainly, a
//! run of spaces is one space and a space at either end is none, as SVG's default `xml:space`
//! says. Where Pele writes `xml:space="preserve"`, as for a markdown label or a tree view's row,
//! every space is kept.

mod common;

use common::{as_a_page_reads, in_a_row, rasterise};
use pele_core::scene::{Geometry, PaintVisit, Scene};
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels};
use rune_pele::{Diagram, FontCatalog, Palette, RenderStatus, ViewOptions};

const RUN: &str = "rune-graph";
const SCALE: f32 = 2.0;

fn scene(source: &str) -> Scene {
    pele_core::parity_scene(source).unwrap_or_else(|error| panic!("{source:?}: {error}")).scene().clone()
}

/// The text of each run `scene` draws for `node`'s label, in paint order.
fn label_runs(scene: &Scene, node: &str) -> Vec<String> {
    let label = format!("node:{node}/label/");
    let runs = scene.paint().filter_map(|visit| match visit {
        PaintVisit::Draw(draw) if draw.key.to_string().starts_with(&label) => match draw.geometry {
            Geometry::Text(run) => Some(run.text),
            _ => None,
        },
        _ => None,
    });
    runs.collect()
}

#[test]
fn the_walk_draws_a_label_with_the_spaces_the_svg_shows() {
    let plain = scene("flowchart LR\n    A[\"Total:      42\"] --> B[\"a\t   b\"] --> C[\"x  \u{a0}  y\"]");
    assert_eq!(label_runs(&plain, "A"), ["Total: 42"], "a run of spaces is one");
    assert_eq!(label_runs(&plain, "B"), ["a b"], "a tab is a space");
    assert_eq!(label_runs(&plain, "C"), ["x \u{a0} y"], "a no-break space is no space SVG collapses");

    let marked = scene("flowchart LR\n    A[\"`Total:      **42**`\"] --> B");
    assert_eq!(label_runs(&marked, "A"), ["Total:      ", "42"], "a markdown label keeps its spaces");

    let tree = scene("treeView-beta\n    root\n        a    b");
    let rows = tree.paint().filter_map(|visit| match visit {
        PaintVisit::Draw(draw) => match draw.geometry {
            Geometry::Text(run) => Some(run.text),
            _ => None,
        },
        _ => None,
    });
    assert!(rows.into_iter().any(|row| row == "a    b"), "a tree view's row keeps its spaces");
}

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

/// How wide the ink of node A's label is, in points, inside its box less 4 points all round.
/// The label is the only ink there that is darker than the node's border.
fn label_ink(pixels: &Pixels, scene: &Scene) -> f64 {
    let rect = scene.paint().find_map(|visit| match visit {
        PaintVisit::Draw(draw) if draw.key.to_string() == "node:A/shape" => match draw.geometry {
            Geometry::Rect { origin, size, .. } => {
                Some([draw.origin.x + origin.x, draw.origin.y + origin.y, size.width, size.height])
            }
            _ => None,
        },
        _ => None,
    });
    let [x, y, w, h] = rect.expect("node A is a rectangle");
    let px = |points: f64| (points * f64::from(SCALE)).round() as u32;
    let (mut left, mut right) = (u32::MAX, 0);
    for row in px(y + 4.0)..px(y + h - 4.0) {
        for column in px(x + 4.0)..px(x + w - 4.0) {
            if pixels.at(column, row)[..3].iter().all(|&c| c < 100) {
                left = left.min(column);
                right = right.max(column + 1);
            }
        }
    }
    assert!(left < right, "node A's label has ink");
    f64::from(right - left) / f64::from(SCALE)
}

/// How wide the ink of node A's label is natively and in resvg's drawing of Pele's SVG.
fn widths(capturer: &Capturer, source: &str, name: &str) -> (f64, f64) {
    let pele = pele_oracle::render_all(&[source.to_owned()]);
    let pele_oracle::PeleRender::Svg { svg, width, height, .. } = &pele[0] else { panic!("Pele draws it: {pele:?}") };
    let theirs = rasterise(&as_a_page_reads(svg), SCALE);
    let size = (width.ceil() as f32, height.ceil() as f32);
    let page = Page { diagram: Diagram::new("diagram", FontCatalog::rune()), source: source.to_owned(), size };
    let mut window = capturer.window(page, page_view, size, Env::new(false, SCALE));
    let ours = window.capture();
    assert_eq!(window.state().diagram.status(), RenderStatus::Pele, "{name} is drawn natively");
    let scene = window.state().diagram.scene().expect("a native scene");
    common::save_in(RUN, &in_a_row(&[&ours, &theirs], 8), &format!("whitespace/{name}@2x.png"));
    (label_ink(&ours, &scene), label_ink(&theirs, &scene))
}

#[test]
fn a_run_of_spaces_is_as_wide_as_the_svg_shows_it() {
    let capturer = Capturer::new();
    let plain = |spaces: &str| format!("flowchart LR\n    A[\"Total:{spaces}42\"] --> B");
    let (ours_one, theirs_one) = widths(&capturer, &plain(" "), "plain-one-space");
    let (ours_six, theirs_six) = widths(&capturer, &plain("      "), "plain-six-spaces");
    let marked = |spaces: &str| format!("flowchart LR\n    A[\"`Total:{spaces}**42**`\"] --> B");
    let (ours_kept_one, theirs_kept_one) = widths(&capturer, &marked(" "), "markdown-one-space");
    let (ours_kept_six, theirs_kept_six) = widths(&capturer, &marked("      "), "markdown-six-spaces");
    println!(
        "label ink, one space and six, in points: plain {ours_one:.1} and {ours_six:.1} here, {theirs_one:.1} and \
         {theirs_six:.1} in resvg; markdown {ours_kept_one:.1} and {ours_kept_six:.1} here, {theirs_kept_one:.1} \
         and {theirs_kept_six:.1} in resvg"
    );
    assert!((theirs_six - theirs_one).abs() <= 1.0, "resvg shows six plain spaces as one");
    assert!((ours_six - ours_one).abs() <= 1.0, "so does the native view: {ours_one} and {ours_six}");
    let (theirs_wider, ours_wider) = (theirs_kept_six - theirs_kept_one, ours_kept_six - ours_kept_one);
    assert!(theirs_wider > 16.0, "resvg keeps the spaces of a markdown label: {theirs_wider} points wider");
    assert!((ours_wider - theirs_wider).abs() <= 2.0, "so does the native view: {ours_wider} points wider");
}
