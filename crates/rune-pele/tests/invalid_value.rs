//! A style declaration whose value its property does not take, as a typo or a value still
//! streaming in has it (`stroke-width:2p`, `opacity:0.`, `font-weight:bol`), is dropped as a
//! page's CSS drops it: the element keeps what it has without it, and the diagram is still drawn
//! natively. A value a page applies and the walk cannot draw still leaves the diagram to
//! rune-mermaid.
//!
//! As in `invalid_colour`, resvg is not a page: a dropped declaration is held to resvg's drawing
//! of Pele's SVG with the declaration taken out, as a page reads that SVG.

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

/// What a page does with a declaration, and so what the view draws.
#[derive(Clone, Copy, Debug)]
enum Page {
    /// The page drops these declarations, as Pele writes them into the SVG, and draws the rest.
    Drops(&'static [&'static str]),
    /// The page applies it, and so does the walk.
    Draws,
    /// The page applies it, and the walk cannot: rune-mermaid draws the diagram.
    LeavesToRuneMermaid,
}

/// Every value of these properties that the corpus's sources write.
const CORPUS: [(&str, &str); 9] = [
    ("stroke-width", "4px"),
    ("stroke-width", "2px"),
    ("stroke-width", "3px"),
    ("stroke-width", "10px"),
    ("stroke-dasharray", "5 5"),
    ("stroke-dasharray", "9"),
    ("font-weight", "bold"),
    ("font-style", "italic"),
    ("stroke-dashoffset", "900"),
];

/// Node A's `style`, and what a page does with it, from CSS's grammar for each property. Every
/// prefix of each value of `CORPUS` comes first, as it streams in.
const STYLES: [(&str, Page); 59] = [
    ("stroke-width:4", Page::Draws),
    ("stroke-width:4p", Page::Drops(&["stroke-width:4p;"])),
    ("stroke-width:4px", Page::Draws),
    ("stroke-width:2", Page::Draws),
    ("stroke-width:2p", Page::Drops(&["stroke-width:2p;"])),
    ("stroke-width:2px", Page::Draws),
    ("stroke-width:3", Page::Draws),
    ("stroke-width:3p", Page::Drops(&["stroke-width:3p;"])),
    ("stroke-width:3px", Page::Draws),
    ("stroke-width:1", Page::Draws),
    ("stroke-width:10", Page::Draws),
    ("stroke-width:10p", Page::Drops(&["stroke-width:10p;"])),
    ("stroke-width:10px", Page::Draws),
    ("stroke-dasharray:5", Page::Draws),
    ("stroke-dasharray:5 ", Page::Draws),
    ("stroke-dasharray:5 5", Page::Draws),
    ("stroke-dasharray:9", Page::Draws),
    ("font-weight:b", Page::Drops(&["font-weight:b;"])),
    ("font-weight:bo", Page::Drops(&["font-weight:bo;"])),
    ("font-weight:bol", Page::Drops(&["font-weight:bol;"])),
    ("font-weight:bold", Page::Draws),
    ("font-style:i", Page::Drops(&["font-style:i;"])),
    ("font-style:it", Page::Drops(&["font-style:it;"])),
    ("font-style:ita", Page::Drops(&["font-style:ita;"])),
    ("font-style:ital", Page::Drops(&["font-style:ital;"])),
    ("font-style:itali", Page::Drops(&["font-style:itali;"])),
    ("font-style:italic", Page::Draws),
    ("stroke-dashoffset:9", Page::LeavesToRuneMermaid),
    ("stroke-dashoffset:90", Page::LeavesToRuneMermaid),
    ("stroke-dashoffset:900", Page::LeavesToRuneMermaid),
    ("stroke-width:2.", Page::Drops(&["stroke-width:2.;"])),
    ("stroke-width:1.5.2", Page::Drops(&["stroke-width:1.5.2;"])),
    ("stroke-width:-1px", Page::Drops(&["stroke-width:-1px;"])),
    ("stroke-width:thick", Page::Drops(&["stroke-width:thick;"])),
    ("opacity:0.", Page::Drops(&["opacity:0.;"])),
    ("fill-opacity:0.5.", Page::Drops(&["fill-opacity:0.5.;"])),
    ("stroke-opacity:a", Page::Drops(&["stroke-opacity:a;"])),
    ("stroke-dasharray:5p", Page::Drops(&["stroke-dasharray:5p;"])),
    ("stroke-dasharray:5 -5", Page::Drops(&["stroke-dasharray:5 -5;"])),
    ("rx:5p", Page::Drops(&["rx:5p;"])),
    ("rx:-5", Page::Drops(&["rx:-5;"])),
    ("font-size:20p", Page::Drops(&["font-size:20p;"])),
    ("font-weight:0", Page::Drops(&["font-weight:0;"])),
    ("font-style:oblique 91deg", Page::Drops(&["font-style:oblique 91deg;"])),
    ("font-family:1a", Page::Drops(&["font-family:1a;"])),
    ("text-decoration:underlin", Page::Drops(&["text-decoration:underlin;"])),
    ("text-decoration:underline underline", Page::Drops(&["text-decoration:underline underline;"])),
    ("stroke-linecap:rou", Page::Drops(&["stroke-linecap:rou;"])),
    ("stroke-linejoin:mit", Page::Drops(&["stroke-linejoin:mit;"])),
    ("letter-spacing:2p", Page::Drops(&["letter-spacing:2p;"])),
    ("word-spacing:norma", Page::Drops(&["word-spacing:norma;"])),
    ("stroke-width:2em", Page::LeavesToRuneMermaid),
    ("stroke-width:1pt", Page::LeavesToRuneMermaid),
    ("font-family:Ari", Page::LeavesToRuneMermaid),
    ("text-decoration:line-through", Page::LeavesToRuneMermaid),
    ("stroke-linecap:square", Page::LeavesToRuneMermaid),
    ("letter-spacing:2px", Page::LeavesToRuneMermaid),
    ("rx:auto", Page::LeavesToRuneMermaid),
    ("rx:5em", Page::LeavesToRuneMermaid),
];

const PLAIN: &str = "flowchart LR\n    A[Total] --> B";

fn source(style: &str) -> String {
    format!("{PLAIN}\n    style A {style}")
}

fn walked(source: &str) -> Vec<PaintVisit> {
    pele_core::parity_scene(source).unwrap_or_else(|error| panic!("{source:?}: {error}")).scene().paint().collect()
}

struct Viewer {
    diagram: Diagram,
    source: String,
    size: (f32, f32),
}

fn page_view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let options = ViewOptions::natural().pele_metrics().palette(Palette::pele());
    let drawn = s.diagram.view(cx, &s.source, options).expect("the source draws");
    el().size(s.size.0, s.size.1).fill(Color::WHITE).child(drawn)
}

struct Drawn {
    svg: String,
    pixels: Pixels,
    status: RenderStatus,
    fallback: Option<Fallback>,
    text: Vec<[f64; 4]>,
}

/// Pele's SVG of each source, and what a view of it draws.
fn drawn(capturer: &Capturer, sources: &[String]) -> Vec<Drawn> {
    let pele = pele_oracle::render_all(sources);
    sources
        .iter()
        .zip(pele)
        .map(|(source, pele)| {
            let pele_oracle::PeleRender::Svg { svg, width, height, .. } = pele else { panic!("Pele draws {source:?}") };
            let size = (width.ceil() as f32, height.ceil() as f32);
            let page = Viewer { diagram: Diagram::new("diagram", FontCatalog::rune()), source: source.clone(), size };
            let mut window = capturer.window(page, page_view, size, Env::new(false, SCALE));
            let pixels = window.capture();
            let diagram = &window.state().diagram;
            let text = diagram.scene().map(|scene| text_boxes(&scene)).unwrap_or_default();
            Drawn { svg, pixels, status: diagram.status(), fallback: diagram.fallback(), text }
        })
        .collect()
}

/// Holds `ours` to resvg's drawing of `svg` by the limits the corpus is held to at 2x.
fn as_resvg_draws(name: &str, ours: &Pixels, svg: &str, text: &[[f64; 4]]) {
    let theirs = rasterise(&as_a_page_reads(svg), SCALE);
    let (apart, differ) = compare(ours, &theirs, text, f64::from(SCALE));
    let file: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    common::save_in(RUN, &in_a_row(&[ours, &theirs, &marked(ours, &differ)], 8), &format!("value/{file}@2x.png"));
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
fn every_prefix_of_each_value_the_corpus_writes_is_in_the_table() {
    for (property, value) in CORPUS {
        for end in 1..=value.len() {
            let style = format!("{property}:{}", &value[..end]);
            assert!(STYLES.iter().any(|(listed, _)| *listed == style), "{style} is in the table");
        }
    }
}

#[test]
fn a_value_its_property_does_not_take_is_dropped_and_the_rest_is_drawn_as_a_page_draws_it() {
    let capturer = Capturer::new();
    let sources: Vec<String> = STYLES.iter().map(|(style, _)| source(style)).collect();
    let (mut dropped, mut drawn_natively, mut left) = (0, 0, 0);
    for ((style, page), drawn) in STYLES.iter().zip(drawn(&capturer, &sources)) {
        let source = source(style);
        let why = (drawn.status, drawn.fallback.clone());
        match page {
            Page::Drops(written) => {
                assert_eq!(why, (RenderStatus::Pele, None), "{style}: drawn natively");
                assert!(walked(&source) == walked(PLAIN), "{style}: the walk draws the source as it draws it without");
                let mut svg = drawn.svg.clone();
                for declaration in *written {
                    assert!(svg.contains(declaration), "{style}: Pele writes `{declaration}` into its SVG");
                    svg = svg.replace(declaration, "");
                }
                as_resvg_draws(style, &drawn.pixels, &svg, &drawn.text);
                dropped += 1;
            }
            Page::Draws => {
                assert_eq!(why, (RenderStatus::Pele, None), "{style}: drawn natively");
                as_resvg_draws(style, &drawn.pixels, &drawn.svg, &drawn.text);
                drawn_natively += 1;
            }
            Page::LeavesToRuneMermaid => {
                let reason = Some(Fallback::NotLowered(Unsupported::Style((*style).to_owned())));
                assert_eq!(why, (RenderStatus::RuneMermaid, reason), "{style}: a value only a page draws");
                left += 1;
            }
        }
    }
    println!(
        "styles: {dropped} dropped as a page drops them, {drawn_natively} drawn natively, {left} left to rune-mermaid"
    );
}

/// A use case diagram whose edge `e1` is cased where it crosses a boundary, with `{style}` as
/// the edge's style. A casing is drawn as wide as its edge's stroke and a margin, so Pele writes
/// a width it cannot add up as `calc()`.
const CASED: &str = "usecase-beta\ndirection TB\nclassDef dark fill:#123,stroke:#9cf\nclassDef light fill:#ffd\n\
    classDef faint fill:#369,opacity:0.3\nsystemBoundary S:::dark\n  A(Alpha)\n  B(Beta)\n  C(Gamma)\nend\n\
    systemBoundary T:::light\n  D(Delta)\n  E(Epsilon)\nend\nsystemBoundary U:::faint\n  F(Phi)\n  G(Chi)\nend\n\
    A --|> B\nB e1@--> C\nD --|> E\nD --o E\nF --|> G\nA --> D\nE --x F\nstyle e1 {style}";

#[test]
fn a_length_typed_into_an_edge_style_is_dropped_from_its_line_and_its_casing() {
    let capturer = Capturer::new();
    let source = CASED.replace("{style}", "stroke:red,stroke-width:3p");
    let drawn = drawn(&capturer, std::slice::from_ref(&source)).remove(0);
    assert_eq!((drawn.status, drawn.fallback), (RenderStatus::Pele, None), "drawn natively");
    let mut svg = drawn.svg.clone();
    for declaration in ["stroke-width:3p;", "stroke-width:calc(3p + 4px);"] {
        assert!(svg.contains(declaration), "Pele writes `{declaration}` into its SVG");
        svg = svg.replace(declaration, "");
    }
    as_resvg_draws("e1 stroke:red,stroke-width:3p", &drawn.pixels, &svg, &drawn.text);
}

#[test]
fn a_function_left_open_takes_in_the_declarations_after_it() {
    let capturer = Capturer::new();
    let plain = "stateDiagram-v2\n    A --> B";
    let source = format!("{plain}\n    classDef bad fill:rgb(1 2 3,stroke:#f00,stroke-width:6px\n    class A bad");
    let drawn = drawn(&capturer, std::slice::from_ref(&source)).remove(0);
    assert_eq!((drawn.status, drawn.fallback), (RenderStatus::Pele, None), "drawn natively");
    assert!(walked(&source) == walked(plain), "the walk draws none of the declarations, as a page draws none");
    let written = "fill:rgb(1 2 3;stroke:#f00;stroke-width:6px;";
    assert!(drawn.svg.contains(written), "Pele writes `{written}` into its SVG");
    as_resvg_draws(
        "fill:rgb(1 2 3,stroke:#f00,stroke-width:6px",
        &drawn.pixels,
        &drawn.svg.replace(written, ""),
        &drawn.text,
    );
}
