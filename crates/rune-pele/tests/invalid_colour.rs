//! A `fill` or `stroke` that is no colour at all, as a typo or a colour still streaming in has
//! it (`fill:blu`, `fill:#0`), is dropped as a page's CSS drops it: the element keeps the paint
//! it has without the declaration, and the diagram is still drawn natively. A colour only a page
//! can resolve still leaves the diagram to rune-mermaid.
//!
//! resvg is not a page here. It paints a fill it cannot parse black and drops such a stroke, so
//! the drawing is held to resvg's drawing of Pele's SVG with the declaration dropped, as a page
//! reads that SVG.

// What only the tests that need rune-mermaid use is unused without `fallback`.
#![cfg_attr(not(feature = "fallback"), allow(dead_code, unused_imports))]

mod common;

use common::{
    FLAT_ERROR, LARGEST_PATCH, MOST_UNEXPLAINED, as_a_page_reads, compare, in_a_row, marked, rasterise, text_boxes,
};
use pele_core::scene::PaintVisit;
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels};
use rune_pele::lower::Unsupported;
use rune_pele::{Diagram, Fallback, FontCatalog, Palette, RenderStatus, ViewOptions};

const RUN: &str = "rune-graph";
const SCALE: f32 = 2.0;

/// Each `style` with a declaration no CSS reads, the declaration Pele writes for it into the
/// SVG, and what the style says without it. `color` is the text's `fill`, and of two `fill`s
/// Pele writes the last.
const INVALID: [(&str, &str, &str); 13] = [
    ("fill:#0", "fill:#0;", ""),
    ("fill:#00", "fill:#00;", ""),
    ("fill:#00000", "fill:#00000;", ""),
    ("fill:#0000000", "fill:#0000000;", ""),
    ("fill:#12345g", "fill:#12345g;", ""),
    ("fill:blu", "fill:blu;", ""),
    ("fill:Blu", "fill:Blu;", ""),
    ("fill:red red", "fill:red red;", ""),
    ("stroke:#0,stroke-width:6px", "stroke:#0;", "stroke-width:6px"),
    ("stroke:blu,stroke-width:6px", "stroke:blu;", "stroke-width:6px"),
    ("color:#0", "fill:#0;", ""),
    ("color:blu", "fill:blu;", ""),
    ("fill:#036,fill:#0", "fill:#0;", ""),
];

fn source(style: &str) -> String {
    format!("flowchart LR\n    A[Total] --> B\n    style A {style}")
}

fn walked(source: &str) -> Vec<PaintVisit> {
    pele_core::parity_scene(source).unwrap_or_else(|error| panic!("{source:?}: {error}")).scene().paint().collect()
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

/// Pele's SVG of `source`, and what its view draws: the pixels, who drew them and why.
fn drawn(capturer: &Capturer, source: &str) -> (String, Pixels, RenderStatus, Option<Fallback>, Vec<[f64; 4]>) {
    let pele = pele_oracle::render_all(&[source.to_owned()]);
    let pele_oracle::PeleRender::Svg { svg, width, height, .. } = &pele[0] else { panic!("Pele draws it: {pele:?}") };
    let size = (width.ceil() as f32, height.ceil() as f32);
    let page = Page { diagram: Diagram::new("diagram", FontCatalog::rune()), source: source.to_owned(), size };
    let mut window = capturer.window(page, page_view, size, Env::new(false, SCALE));
    let ours = window.capture();
    let diagram = &window.state().diagram;
    let text = diagram.scene().map(|scene| text_boxes(&scene)).unwrap_or_default();
    (svg.clone(), ours, diagram.status(), diagram.fallback(), text)
}

/// Holds `ours` to resvg's drawing of `svg` by the limits the corpus is held to at 2x.
fn as_resvg_draws(name: &str, ours: &Pixels, svg: &str, text: &[[f64; 4]]) {
    let theirs = rasterise(&as_a_page_reads(svg), SCALE);
    let (apart, differ) = compare(ours, &theirs, text, f64::from(SCALE));
    let file = name.replace([':', '#', ' ', ','], "_");
    common::save_in(RUN, &in_a_row(&[ours, &theirs, &marked(ours, &differ)], 8), &format!("colour/{file}@2x.png"));
    println!(
        "{name}: patch {} px, {:.2} unexplained in 10,000, flat {} levels",
        apart.largest_blob,
        apart.share(),
        apart.flat_error
    );
    let scale = f64::from(SCALE);
    assert!(apart.largest_blob as f64 <= LARGEST_PATCH * scale * scale, "{name}: a patch of {} px", apart.largest_blob);
    assert!(apart.share() <= MOST_UNEXPLAINED, "{name}: {:.1} unexplained px in 10,000", apart.share());
    assert!(apart.flat_error <= FLAT_ERROR, "{name}: flat colours {} levels apart", apart.flat_error);
}

#[test]
fn a_colour_no_css_reads_is_dropped_and_the_diagram_is_drawn_natively() {
    let capturer = Capturer::new();
    for (style, written, rest) in INVALID {
        let source = source(style);
        let without = if rest.is_empty() { "flowchart LR\n    A[Total] --> B".to_owned() } else { self::source(rest) };
        assert!(walked(&source) == walked(&without), "{style}: the walk draws the source as it draws {without:?}");

        let (svg, ours, status, fallback, text) = drawn(&capturer, &source);
        assert_eq!((status, fallback), (RenderStatus::Pele, None), "{style}: drawn natively");
        assert!(svg.contains(written), "{style}: Pele writes `{written}` into its SVG");
        as_resvg_draws(style, &ours, &svg.replacen(written, "", 1), &text);
    }
}

#[cfg(feature = "fallback")]
#[test]
fn a_colour_only_a_page_can_resolve_still_leaves_the_diagram_to_rune_mermaid() {
    let capturer = Capturer::new();
    for style in ["fill:currentcolor", "fill:inherit", "stroke:currentColor", "fill:Canvas"] {
        let (_, _, status, fallback, _) = drawn(&capturer, &source(style));
        let why = Some(Fallback::NotLowered(Unsupported::Style(style.to_owned())));
        assert_eq!((status, fallback), (RenderStatus::RuneMermaid, why), "{style}");
    }
    for style in ["fill:#0f0", "fill:#0f08", "fill:Lime", "stroke:#f00,stroke-width:6px"] {
        let (svg, ours, status, fallback, text) = drawn(&capturer, &source(style));
        assert_eq!((status, fallback), (RenderStatus::Pele, None), "{style}: a colour the walk reads");
        as_resvg_draws(style, &ours, &svg, &text);
    }
}

/// Where each colour value of `source` starts and ends, as byte offsets: what follows `fill:` or
/// `stroke:` up to the next `,` or the end of its line.
fn colour_values(source: &str) -> Vec<(usize, usize)> {
    let mut values = Vec::new();
    for property in ["fill:", "stroke:"] {
        for (at, _) in source.match_indices(property) {
            let start = at + property.len();
            let end = start + source[start..].find([',', '\n']).unwrap_or(source.len() - start);
            values.push((start, end));
        }
    }
    values.sort_unstable();
    values
}

struct Streamed {
    diagram: Diagram,
    source: String,
}

fn streamed_view(s: &Streamed, cx: &mut Cx<Streamed>) -> El {
    let options = ViewOptions::new(800.0).palette(Palette::pele());
    el().size(800.0, 1000.0).fill(Color::WHITE).children(s.diagram.view(cx, &s.source, options).ok())
}

#[test]
fn a_colour_typed_one_character_at_a_time_never_leaves_the_diagram_to_rune_mermaid() {
    let source = std::fs::read_to_string(pele_oracle::root().join("tests/fixtures/rune/flowchart.mmd"))
        .expect("the flowchart fixture");
    let values = colour_values(&source);
    assert_eq!(values.len(), 3, "the fixture's fill and stroke of F and the stroke of link 2: {values:?}");
    let capturer = Capturer::new();
    let streamed = Streamed { diagram: Diagram::new("diagram", FontCatalog::rune()), source: String::new() };
    let mut window = capturer.window(streamed, streamed_view, (800.0, 1000.0), Env::new(false, 1.0));
    let (mut native, mut kept) = (0, 0);
    for &(start, end) in &values {
        for typed in start + 1..=end {
            let prefix = &source[..typed];
            window.update(|streamed| streamed.source = prefix.to_owned());
            window.capture();
            let diagram = &window.state().diagram;
            let shown = format!("{:?}", &source[start..typed]);
            match diagram.status() {
                RenderStatus::Pele => native += 1,
                RenderStatus::Kept => kept += 1,
                other => panic!("{shown} typed: {other:?}, {:?}, {:?}", diagram.fallback(), diagram.error()),
            }
        }
    }
    println!("colour prefixes of the flowchart fixture: {native} drawn natively, {kept} keeping the last drawing");
    assert!(native > 0);
}
