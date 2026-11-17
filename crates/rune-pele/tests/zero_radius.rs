//! A shape SVG does not render, such as a circle of radius 0, draws nothing natively either:
//! neither its fill nor its stroke.

mod common;

use common::{as_a_page_reads, in_a_row, rasterise};
use pele_core::scene::{Geometry, Point, Size};
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels};
use rune_pele::{Diagram, FontCatalog, Palette, RenderStatus, ViewOptions};

const RUN: &str = "rune-graph";

/// A quadrant chart whose first point is stroked 6 pixels wide in cyan, at `radius`.
fn quadrant(radius: u32) -> String {
    format!(
        "quadrantChart\n    title Reach and engagement\n    x-axis Low Reach --> High Reach\n    \
         y-axis Low Engagement --> High Engagement\n    \
         Campaign A: [0.3, 0.6] radius: {radius}, stroke-color: #10f0f0, stroke-width: 6px\n    \
         Campaign B: [0.7, 0.4] radius: 10\n"
    )
}

/// Pixels in the point's cyan, which nothing else in the chart is drawn in.
fn cyan(pixels: &Pixels) -> usize {
    pixels.rgba.chunks_exact(4).filter(|c| c[3] > 128 && c[0] < 90 && c[1] > 185 && c[2] > 185).count()
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

/// The source drawn natively and Pele's SVG of it through resvg, both at 2x.
fn both(capturer: &Capturer, source: &str) -> (Pixels, Pixels) {
    let pele = pele_oracle::render_all(&[source.to_owned()]);
    let pele_oracle::PeleRender::Svg { svg, width, height, .. } = &pele[0] else { panic!("Pele draws it: {pele:?}") };
    let theirs = rasterise(&as_a_page_reads(svg), 2.0);
    let size = (width.ceil() as f32, height.ceil() as f32);
    let page = Page { diagram: Diagram::new("quadrant", FontCatalog::rune()), source: source.to_owned(), size };
    let mut window = capturer.window(page, page_view, size, Env::new(false, 2.0));
    let ours = window.capture();
    assert_eq!(window.state().diagram.status(), RenderStatus::Pele, "quadrant is drawn natively");
    (ours, theirs)
}

#[test]
fn a_point_of_radius_zero_draws_nothing_as_svg_draws_nothing() {
    let capturer = Capturer::new();
    let (ours, theirs) = both(&capturer, &quadrant(0));
    let (ours_1, theirs_1) = both(&capturer, &quadrant(1));
    common::save_in(RUN, &in_a_row(&[&ours, &theirs, &ours_1, &theirs_1], 8), "lowering/radius-zero@2x.png");
    println!(
        "cyan px: radius 0 drawn here {}, by resvg {}; radius 1 drawn here {}, by resvg {}",
        cyan(&ours),
        cyan(&theirs),
        cyan(&ours_1),
        cyan(&theirs_1)
    );
    assert!(cyan(&theirs_1) > 100 && cyan(&ours_1) > 100, "a point of radius 1 shows its cyan stroke either way");
    assert_eq!(cyan(&theirs), 0, "resvg draws no point of radius 0");
    assert_eq!(cyan(&ours), 0, "nor does the native view, stroke and all");
}

/// A thick stroke round each shape, which shows wherever the shape is drawn at all.
fn stroked(element: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"60\" height=\"60\">\
         <g transform=\"translate(30,30)\" fill=\"#10f0f0\" stroke=\"#10f0f0\" stroke-width=\"6\">{element}</g></svg>"
    )
}

#[test]
fn a_shape_with_a_side_or_radius_of_nothing_is_not_rendered_as_resvg_renders_none() {
    let at = Point { x: 0.0, y: 0.0 };
    let rect = |width: f64, height: f64| Geometry::Rect { origin: at, size: Size { width, height }, radius: 0.0 };
    let ellipse = |rx: f64, ry: f64| Geometry::Ellipse { centre: at, radii: Size { width: rx, height: ry } };
    let cases = [
        ("<rect width=\"0\" height=\"10\"/>", rect(0.0, 10.0)),
        ("<rect width=\"10\" height=\"0\"/>", rect(10.0, 0.0)),
        ("<rect width=\"-4\" height=\"10\"/>", rect(-4.0, 10.0)),
        ("<rect width=\"10\" height=\"10\"/>", rect(10.0, 10.0)),
        ("<circle r=\"0\"/>", Geometry::Circle { centre: at, radius: 0.0 }),
        ("<circle r=\"1\"/>", Geometry::Circle { centre: at, radius: 1.0 }),
        ("<ellipse rx=\"0\" ry=\"8\"/>", ellipse(0.0, 8.0)),
        ("<ellipse rx=\"8\" ry=\"0\"/>", ellipse(8.0, 0.0)),
        ("<ellipse rx=\"8\" ry=\"4\"/>", ellipse(8.0, 4.0)),
    ];
    for (element, geometry) in cases {
        let drawn = cyan(&rasterise(&stroked(element), 2.0)) > 0;
        assert_eq!(geometry.renders(), drawn, "{element}: resvg draws it: {drawn}");
    }
}
