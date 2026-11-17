//! A diagram drawn smaller, or fitted to the width it has, through rune-pele's public API: each
//! corpus source zoomed beside Pele's SVG drawn at the same size, text at each zoom against its
//! zoomed box, a stroke's width at a zoom, a fitted view at widths that reach each of its steps
//! and through a resize, and links and tooltips at a zoom.

mod common;

use std::cell::RefCell;
use std::fmt::Write as _;

use common::{
    Apart, as_a_page_reads, compare, compare_with_any, in_a_row, marked, out_dir_for, over, rasterise, rasterise_fine,
    text_boxes,
};
use pele_core::Scene;
use pele_core::scene::{AnchorKind, Geometry, PaintVisit};
use pele_oracle::PeleRender;
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels, Window};
use rune_pele::geometry::Affine;
use rune_pele::lower::{Drawing, group_map};
use rune_pele::text::Baselines;
use rune_pele::{Diagram, Event, FontCatalog, Link, Palette, RenderStatus, ViewError, ViewOptions};

const RUN: &str = "rune-fit";
const TYPES: [&str; 32] = [
    "flowchart",
    "class",
    "state",
    "er",
    "requirement",
    "usecase",
    "swimlane",
    "agentflow",
    "architecture",
    "treeview",
    "git",
    "radar",
    "timeline",
    "xychart",
    "quadrant",
    "sequence",
    "gantt",
    "block",
    "c4",
    "railroad",
    "info",
    "cynefin",
    "wardley",
    "venn",
    "packet",
    "eventmodeling",
    "treemap",
    "mindmap",
    "journey",
    "kanban",
    "ishikawa",
    "sankey",
];

/// The zooms every corpus source is drawn at beside Pele's: 1.5 and 1 device pixels a point in
/// a 2x window.
const ZOOMS: [f32; 2] = [0.75, 0.5];

/// The zooms the spike drew each type's fixture at with Rune's `El::scale`, and the least a fit
/// draws 16 point text at by default, 11 points.
const FIXTURE_ZOOMS: [f32; 5] = [1.0, 0.75, 0.6875, 0.6, 0.5];

const SCALE: f32 = 2.0;

struct Viewer {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
    /// The box the view is put in, in points.
    width: f32,
    height: f32,
    /// Rune's `El::scale` over the view, as the spike drew a diagram smaller.
    el_scale: Option<f32>,
    error: RefCell<Option<ViewError>>,
    followed: Vec<Link>,
}

fn on_event(viewer: &mut Viewer, event: Event, _: &mut rune::Tx) {
    if let Event::Activate(link) = event {
        viewer.followed.push(link);
    }
}

/// The view in a box `width` points wide, on a white page that fills the window.
fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let page = el().fill(Color::WHITE);
    let within = el().size(s.width, s.height);
    match s.diagram.view_with(cx, &s.source, s.options.clone(), on_event) {
        Ok(drawn) => {
            s.error.replace(None);
            let drawn = match s.el_scale {
                Some(k) => el().size(s.width / k, s.height / k).origin(0.0, 0.0).scale(k).child(drawn),
                None => drawn,
            };
            page.child(within.child(drawn))
        }
        Err(error) => {
            s.error.replace(Some(error));
            page.child(within)
        }
    }
}

fn window<'a>(
    capturer: &'a Capturer,
    source: &str,
    options: ViewOptions,
    size: (f32, f32),
    scale: f32,
) -> Window<'a, Viewer> {
    let viewer = Viewer {
        diagram: Diagram::new("diagram", FontCatalog::rune()),
        source: source.to_owned(),
        options,
        width: size.0,
        height: size.1,
        el_scale: None,
        error: RefCell::new(None),
        followed: Vec::new(),
    };
    capturer.window(viewer, view, size, Env::new(false, scale))
}

/// The view Pele's SVG is compared with: Pele's metrics and colours, no width to fit.
fn as_pele() -> ViewOptions {
    ViewOptions::natural().pele_metrics().palette(Palette::pele())
}

/// Pele's SVG of each source under the id prefix a view lays out under, as `graph_view` draws it.
fn as_pele_draws(sources: &[String]) -> Vec<PeleRender> {
    let cases: Vec<(String, String)> =
        sources.iter().map(|source| (source.clone(), "{\"idPrefix\":\"pele\"}".to_owned())).collect();
    pele_oracle::render_all_with(&cases)
}

fn corpus(kind: &str) -> Vec<String> {
    pele_oracle::corpus().into_iter().filter(|(name, _)| name == kind).flat_map(|(_, sources)| sources).collect()
}

fn fixture(name: &str) -> String {
    let path = pele_oracle::root().join(format!("tests/fixtures/rune/{name}.mmd"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn write_report(name: &str, text: &str) {
    std::fs::create_dir_all(out_dir_for(RUN)).unwrap();
    std::fs::write(out_dir_for(RUN).join(name), text).unwrap();
}

/// How a diagram is drawn smaller.
#[derive(Clone, Copy, PartialEq)]
enum Smaller {
    /// [`ViewOptions::zoom`], in the lowering.
    Lowered,
    /// Rune's `El::scale` over the view at its own size.
    ElScale,
}

/// `source` drawn `zoom` times its size in a 2x window beside Pele's SVG through resvg at the
/// same density, with the text boxes left out: against resvg as it draws, and against either
/// that or resvg's coverage by area from [`rasterise_fine`]. Nothing where the view drew `source`
/// by rune-mermaid.
///
/// At a density that puts edges between resvg's quarter pixels, resvg's own coverage is up to an
/// eighth of a pixel off, which along a dark 1 point line is 17 levels or more. Its coverage by
/// area is exact, but strokes that lie over each other no longer add up where their edges fall
/// between pixels, as two lanes' shared side does. Each is one right drawing of Pele's SVG.
fn zoomed_beside_pele(
    capturer: &Capturer,
    source: &str,
    pele: &PeleRender,
    zoom: f32,
    smaller: Smaller,
    name: &str,
) -> Option<[Apart; 2]> {
    let PeleRender::Svg { svg, width, height, .. } = pele else { panic!("Pele draws {name}: {pele:?}") };
    let size = (width.ceil() as f32 * zoom, height.ceil() as f32 * zoom);
    let options = if smaller == Smaller::Lowered { as_pele().zoom(zoom) } else { as_pele() };
    let mut window = window(capturer, source, options, size, SCALE);
    window.update(|viewer| viewer.el_scale = (smaller == Smaller::ElScale).then_some(zoom));
    let ours = window.capture();
    assert_eq!(*window.state().error.borrow(), None, "{name}");
    let diagram = &window.state().diagram;
    if diagram.status() != RenderStatus::Pele {
        return None;
    }
    let lowered_at = if smaller == Smaller::Lowered { zoom } else { 1.0 };
    assert_eq!(diagram.zoom(), Some(lowered_at), "{name}");
    let scene = diagram.scene().expect("a native scene");
    // The boxes are in the Scene's points, which the window shows at the density Pele's SVG is
    // rasterised at.
    let boxes = text_boxes(&scene);
    let density = f64::from(SCALE * zoom);
    let page = as_a_page_reads(svg);
    let theirs = rasterise(&page, SCALE * zoom);
    let (apart, differ) = compare(&ours, &theirs, &boxes, density);
    let by_area = rasterise_fine(&page, SCALE * zoom, 4);
    let (either, differ_from_both) = compare_with_any(&ours, &[&theirs, &by_area], &boxes, density);
    let row = in_a_row(&[&ours, &theirs, &marked(&ours, &differ), &marked(&ours, &differ_from_both)], 16);
    common::save_in(RUN, &row, &format!("{name}@{zoom}.png"));
    Some([apart, either])
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "980 drawings compared pixel by pixel take minutes unoptimised: run with --release"
)]
fn every_corpus_source_drawn_smaller_is_pele_drawn_at_that_size() {
    let capturer = Capturer::new();
    let mut table = String::from(
        "Each corpus source through Diagram::view at a zoom (Pele's metrics and colours, Inter) in a 2x window,\n\
         against Pele's SVG through resvg (Arial) at 2 x zoom device pixels a point, text boxes left out. `patch` and\n\
         `share` are against resvg as it draws; `either` counts a pixel unexplained only where neither resvg as it draws\n\
         nor resvg's coverage by area (4 x 4 samples a pixel) explains it, and is what the limits hold. Under 2 px a\n\
         point the 1x rules of graph_view hold: patch and share are listed, flat and the 30 px patch held.\n\n\
         type          zoom density sources drawn  worst-patch (source)  worst-share (source)  worst-either  worst-flat (source)  over-sharp\n",
    );
    let mut rows = String::from(
        "type        source  zoom   compared patch  share flat  either:patch  share  over (held to either)\n",
    );
    let mut findings = Vec::new();
    for kind in TYPES {
        let sources = corpus(kind);
        let pele = as_pele_draws(&sources);
        for zoom in ZOOMS {
            let density = f64::from(SCALE * zoom);
            let mut worst: [(f64, String); 4] = std::array::from_fn(|_| (0.0, "-".to_owned()));
            let (mut drawn, mut sharp_count) = (0, 0);
            for (index, (source, pele)) in sources.iter().zip(&pele).enumerate() {
                let name = format!("side-by-side/{kind}/{index:03}");
                let Some([as_drawn, apart]) =
                    zoomed_beside_pele(&capturer, source, pele, zoom, Smaller::Lowered, &name)
                else {
                    continue;
                };
                drawn += 1;
                let (sharp, other) = over(&apart, density);
                let held: Vec<&String> =
                    if density < 2.0 { other.iter().collect() } else { sharp.iter().chain(&other).collect() };
                sharp_count += usize::from(!sharp.is_empty());
                let said: Vec<String> = sharp.iter().chain(&other).cloned().collect();
                writeln!(
                    rows,
                    "{kind:<11} {index:>6} {zoom:>5} {:>10} {:>5} {:>6.2} {:>4} {:>12} {:>6.2}  {}",
                    apart.compared,
                    as_drawn.largest_blob,
                    as_drawn.share(),
                    as_drawn.flat_error,
                    apart.largest_blob,
                    apart.share(),
                    said.join("; ")
                )
                .unwrap();
                if !held.is_empty() {
                    let held: Vec<&str> = held.iter().map(|s| s.as_str()).collect();
                    findings.push(format!("{kind} {index}@{zoom}: {}", held.join("; ")));
                }
                let measures = [
                    as_drawn.largest_blob as f64,
                    as_drawn.share(),
                    apart.largest_blob as f64,
                    f64::from(apart.flat_error),
                ];
                for (which, measure) in measures.into_iter().enumerate() {
                    if measure > worst[which].0 {
                        worst[which] = (measure, index.to_string());
                    }
                }
            }
            writeln!(
                table,
                "{kind:<13} {zoom:>4} {density:>7} {:>7} {drawn:>5}  {:>11.0} px ({:<5})  {:>14.2} ({:<5})  {:>5.0} px ({:<3})  {:>11.0} ({:<5})  {sharp_count:>10}",
                sources.len(),
                worst[0].0,
                worst[0].1,
                worst[1].0,
                worst[1].1,
                worst[2].0,
                worst[2].1,
                worst[3].0,
                worst[3].1,
            )
            .unwrap();
        }
    }
    print!("{table}");
    write_report("zoom-side-by-side.txt", &format!("{table}\n{rows}"));
    assert!(findings.is_empty(), "{} are not drawn as Pele draws them:\n{}", findings.len(), findings.join("\n"));
}

/// Each type's fixture at the spike's zooms, drawn smaller in the lowering and with `El::scale`
/// as the spike drew it, with the text boxes masked where the window shows them.
#[test]
#[cfg_attr(debug_assertions, ignore = "320 drawings compared pixel by pixel: run with --release")]
fn each_fixture_drawn_smaller_in_the_lowering_and_with_el_scale_against_pele() {
    let capturer = Capturer::new();
    let mut table = String::from(
        "Each type's fixture smaller: in the lowering, and with El::scale over the view at its own size as the spike\n\
         drew it. patch, share and flat against resvg as it draws; either: against it or its coverage by area.\n\n\
         type          factor  lowered: patch  share flat  either  share   El::scale: patch  share flat  either  share\n",
    );
    for kind in TYPES {
        let source = fixture(kind);
        let pele = as_pele_draws(std::slice::from_ref(&source)).remove(0);
        for zoom in FIXTURE_ZOOMS {
            let [lowered, scaled] = [Smaller::Lowered, Smaller::ElScale].map(|smaller| {
                let name = format!("fixture/{kind}{}", if smaller == Smaller::ElScale { "-el-scale" } else { "" });
                zoomed_beside_pele(&capturer, &source, &pele, zoom, smaller, &name)
                    .unwrap_or_else(|| panic!("{kind}'s fixture is drawn natively"))
            });
            let columns = |[as_drawn, either]: [Apart; 2]| {
                format!(
                    "{:>5} {:>6.2} {:>4} {:>7} {:>6.2}",
                    as_drawn.largest_blob,
                    as_drawn.share(),
                    as_drawn.flat_error,
                    either.largest_blob,
                    either.share()
                )
            };
            writeln!(table, "{kind:<13} {zoom:>6} {:>15}   {:>27}", columns(lowered), columns(scaled)).unwrap();
        }
    }
    print!("{table}");
    write_report("zoom-fixtures.txt", &table);
}

/// A run of text as the Scene lays it out: its box in its group's coordinates, from a font size
/// above its baseline to 0.3 of one below and along its measured advance, and the map from those
/// coordinates to the diagram's.
struct Laid {
    text: String,
    map: Affine,
    low: [f64; 2],
    high: [f64; 2],
}

fn laid_runs(scene: &Scene) -> Vec<Laid> {
    let mut maps = vec![Affine::IDENTITY];
    let mut runs = Vec::new();
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => maps.push(maps.last().expect("the root").then(&group_map(&group))),
            PaintVisit::End => {
                maps.pop();
            }
            PaintVisit::Draw(draw) => {
                let Geometry::Text(run) = &draw.geometry else { continue };
                if run.text.is_empty() {
                    continue;
                }
                let lead = match run.anchor {
                    AnchorKind::Start => 0.0,
                    AnchorKind::Middle => run.advance / 2.0,
                    AnchorKind::End => run.advance,
                };
                let (x, y, size) = (run.baseline.x - lead, run.baseline.y, run.font.size);
                runs.push(Laid {
                    text: run.text.clone(),
                    map: *maps.last().expect("the root"),
                    low: [x, y - size],
                    high: [x + run.advance, y + 0.3 * size],
                });
            }
        }
    }
    runs
}

fn inverse(map: &Affine) -> Affine {
    let det = map.a * map.d - map.b * map.c;
    Affine {
        a: map.d / det,
        b: -map.b / det,
        c: -map.c / det,
        d: map.a / det,
        e: (map.c * map.f - map.d * map.e) / det,
        f: (map.b * map.e - map.a * map.f) / det,
    }
}

/// A pixel darker than this in every channel is ink: a quarter of black's coverage on white.
const INK: u8 = 192;

/// How far, in device pixels, the farthest ink of `pixels` lies outside the box of every run of
/// `runs` drawn at `zoom`, and the text of the run whose box it is nearest.
fn ink_past(pixels: &Pixels, runs: &[Laid], zoom: f64, scale: f64) -> (f64, String) {
    let to_runs: Vec<Affine> = runs.iter().map(|run| inverse(&Affine::scale(zoom).then(&run.map))).collect();
    let (mut worst, mut whose) = (0.0f64, String::new());
    for y in 0..pixels.height {
        for x in 0..pixels.width {
            if pixels.at(x, y)[..3].iter().any(|&level| level >= INK) {
                continue;
            }
            let at = [(f64::from(x) + 0.5) / scale, (f64::from(y) + 0.5) / scale];
            let mut nearest = (f64::MAX, 0);
            for (index, (run, to_run)) in runs.iter().zip(&to_runs).enumerate() {
                let [lx, ly] = to_run.apply(at);
                let out = (run.low[0] - lx).max(lx - run.high[0]).max(run.low[1] - ly).max(ly - run.high[1]).max(0.0);
                if out < nearest.0 {
                    nearest = (out, index);
                }
                if out == 0.0 {
                    break;
                }
            }
            let device = nearest.0 * zoom * scale;
            if device > worst {
                (worst, whose) = (device, runs[nearest.1].text.clone());
            }
        }
    }
    (worst, whose)
}

/// Each fixture laid out as an app lays it out, in Inter, with its text drawn alone at each zoom:
/// where the lowering sets it at the zoomed size, and where Rune's `El::scale` puts the text of
/// the drawing at its own size.
#[test]
#[cfg_attr(debug_assertions, ignore = "320 captures read pixel by pixel: run with --release")]
fn text_at_each_zoom_keeps_to_its_zoomed_box() {
    let capturer = Capturer::new();
    let mut table = String::from(
        "Ink of each fixture's text drawn alone, in device px at 2x past the box its run is laid out in, zoomed:\n\
         the run's measured advance, from a font size above its baseline to 0.3 below. Ink is a pixel darker than\n\
         192 in every channel. `lowered` sets the text at the zoomed size; `El::scale` scales the drawing at 1.\n\n\
         type          zoom  lowered  El::scale  allowed  worst run (lowered)\n",
    );
    let mut findings = Vec::new();
    for kind in TYPES {
        let source = fixture(kind);
        let mut window = window(&capturer, &source, ViewOptions::natural(), (400.0, 400.0), SCALE);
        window.capture();
        let scene = window.state().diagram.scene().unwrap_or_else(|| panic!("{kind}'s fixture is drawn natively"));
        let kind_of = pele_core::prepare(&source, pele_core::ParseOptions::default()).expect("it parses").kind();
        let runs = laid_runs(&scene);
        let lowered = |zoom: f32| {
            let mut baselines = Baselines::new(&[], "Inter", "Inter", SCALE);
            Drawing::zoomed(kind_of, &scene, &mut baselines, SCALE, zoom).expect("it lowers")
        };
        let whole = lowered(1.0);
        let [w, h] = whole.size();
        let mut at_one = 0.0;
        for zoom in FIXTURE_ZOOMS {
            let size = (w * zoom, h * zoom);
            let ours = capturer.capture(
                el().size(size.0, size.1).fill(Color::WHITE).child(lowered(zoom).text_only(Color::BLACK)),
                size,
                SCALE,
            );
            let scaled = el().size(w, h).origin(0.0, 0.0).scale(zoom).child(whole.text_only(Color::BLACK));
            let theirs = capturer.capture(el().size(size.0, size.1).fill(Color::WHITE).child(scaled), size, SCALE);
            let (past, whose) = ink_past(&ours, &runs, f64::from(zoom), f64::from(SCALE));
            let (scaled_past, _) = ink_past(&theirs, &runs, f64::from(zoom), f64::from(SCALE));
            if zoom == 1.0 {
                at_one = past;
            }
            // What the run's ink reaches past its box at its laid-out size, zoomed, and a device
            // pixel for where Rune rounds a baseline and where a glyph's edge falls.
            let allowed = at_one * f64::from(zoom) + 1.0;
            writeln!(table, "{kind:<13} {zoom:>4} {past:>8.1} {scaled_past:>10.1} {allowed:>8.1}  {whose:?}").unwrap();
            if zoom == 0.5 {
                common::save_in(RUN, &in_a_row(&[&ours, &theirs], 16), &format!("text/{kind}@{zoom}.png"));
            }
            if past > allowed {
                findings.push(format!("{kind}@{zoom}: ink {past:.1} px past the box of {whose:?}, over {allowed:.1}"));
            }
        }
    }
    print!("{table}");
    write_report("text-boxes.txt", &table);
    assert!(findings.is_empty(), "text past its box:\n{}", findings.join("\n"));
}

/// The darkness of the pixels from `from` to `to` along a row or a column, in pixels of black.
fn darkness(pixels: &Pixels, along: impl Iterator<Item = (u32, u32)>) -> f64 {
    along.map(|(x, y)| f64::from(255 - pixels.at(x, y)[0]) / 255.0).sum()
}

#[test]
fn a_stroke_is_zoomed_with_what_it_strokes() {
    // A box with a 4 point border and an edge 4 points wide, black on white.
    let source = "flowchart LR\n    A[Box] --- B[Box]\n    style A fill:#fff,stroke:#000,stroke-width:4px\n    \
                  style B fill:#fff,stroke:#fff\n    linkStyle 0 stroke:#000,stroke-width:4px";
    let capturer = Capturer::new();
    let mut report = String::new();
    for zoom in [1.0f32, 0.5] {
        let options = ViewOptions::natural().palette(Palette::pele()).zoom(zoom);
        let mut window = window(&capturer, source, options, (400.0, 200.0), SCALE);
        let pixels = window.capture();
        common::save_in(RUN, &pixels, &format!("stroke@{zoom}.png"));
        let elements = window.runtime().elements();
        let rect = |key: &str| elements.iter().find(|e| e.key.as_deref() == Some(key)).expect(key).rect;
        let [x, y, _, h] = rect("node:A/shape").map(|v| v * SCALE);
        let middle = (y + h / 2.0) as u32;
        let reach = 12.0 * zoom * SCALE;
        let left = darkness(&pixels, ((x - 2.0) as u32..(x + reach) as u32).map(|x| (x, middle)));
        let [ex, ey, ew, eh] = rect("edge:A~B~arrow_open normal/shape").map(|v| v * SCALE);
        let across = (ex + ew / 2.0) as u32;
        let edge = darkness(&pixels, ((ey - 4.0) as u32..(ey + eh + 4.0) as u32).map(|y| (across, y)));
        let want = 4.0 * f64::from(zoom * SCALE);
        writeln!(report, "zoom {zoom}: border {left:.2} px dark across, edge {edge:.2} px, for {want} px").unwrap();
        assert!((left - want).abs() < 0.5, "zoom {zoom}: the border is {left:.2} px, for {want}");
        assert!((edge - want).abs() < 0.5, "zoom {zoom}: the edge is {edge:.2} px, for {want}");
    }
    print!("{report}");
    write_report("stroke.txt", &report);
}

/// What a fitted view drew in a box `width` points wide.
#[derive(Clone, Debug)]
struct Fitted {
    width: f32,
    /// The drawing's size on screen.
    drawn: [f32; 2],
    /// The width the drawing was laid out at.
    laid: f64,
    zoom: f32,
    layouts: u64,
    lowerings: u64,
    scrolls: bool,
}

fn fitted(window: &mut Window<'_, Viewer>, width: f32) -> Fitted {
    window.update(|viewer| viewer.width = width);
    window.capture();
    let state = window.state();
    assert_eq!(*state.error.borrow(), None, "at {width}");
    assert_eq!(state.diagram.status(), RenderStatus::Pele, "at {width}");
    let elements = window.runtime().elements();
    // The frame the fit reads its width from, keyed with the diagram's id, and inside it the
    // drawing, keyed the same.
    let keyed: Vec<[f32; 4]> =
        elements.iter().filter(|e| e.key.as_deref() == Some("diagram")).map(|e| e.rect).collect();
    let [frame, drawn] = keyed[..] else { panic!("a frame and its drawing at {width}: {keyed:?}") };
    let stats = state.diagram.stats();
    Fitted {
        width,
        drawn: [drawn[2], drawn[3]],
        laid: state.diagram.scene().expect("a native scene").size().width,
        zoom: state.diagram.zoom().expect("a native view"),
        layouts: stats.layouts,
        lowerings: stats.lowerings,
        scrolls: drawn[2] > frame[2] + 0.5,
    }
}

/// The steps of a fit, in the order a narrowing box reaches them first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Natural,
    LaidOutNarrower,
    Zoomed,
    SmallestText,
}

const SMALLEST_ZOOM: f32 = 11.0 / 16.0;

fn step_of(fitted: &Fitted, natural: f64) -> Step {
    if fitted.zoom <= SMALLEST_ZOOM + 1e-4 {
        Step::SmallestText
    } else if fitted.zoom < 1.0 {
        Step::Zoomed
    } else if fitted.laid != natural {
        Step::LaidOutNarrower
    } else {
        Step::Natural
    }
}

#[test]
fn a_fitted_view_draws_its_size_then_lays_out_narrower_then_zooms_then_scrolls() {
    let capturer = Capturer::new();
    let pets = std::fs::read_to_string(pele_oracle::root().join("tests/fixtures/pie-pets.mmd")).expect("pie-pets");
    let cases = [
        ("flowchart that turns", fixture("across"), true),
        ("pie whose plot narrows", pets, true),
        ("er, which cannot narrow", fixture("er"), false),
    ];
    let mut report = String::from(
        "case                      width  step               drawn         laid  zoom    layouts  scrolls\n",
    );
    for (name, source, narrows) in cases {
        let mut window = window(&capturer, &source, ViewOptions::fit(), (2400.0, 2000.0), 1.0);
        let wide = fitted(&mut window, 2400.0);
        let natural = wide.laid;
        assert_eq!((wide.zoom, wide.layouts, wide.scrolls), (1.0, 1, false), "{name}: a wide box draws it as it is");
        // Each change of step as the box narrows.
        let mut steps = vec![(Step::Natural, wide.clone())];
        let mut width = (natural as f32).floor();
        while width >= 40.0 {
            let now = fitted(&mut window, width);
            let step = step_of(&now, natural);
            assert!(now.zoom <= 1.0 && now.zoom >= SMALLEST_ZOOM - 1e-4, "{name} at {width}: zoom {}", now.zoom);
            let laid_drawn = now.laid as f32 * now.zoom;
            assert!((now.drawn[0] - laid_drawn).abs() < 0.01, "{name} at {width}: {now:?}");
            assert_eq!(now.scrolls, step == Step::SmallestText && now.drawn[0] > width + 0.5, "{name} at {width}");
            if step != Step::SmallestText {
                assert!(now.drawn[0] <= width + 0.01, "{name} at {width}: drawn {:?} in {width}", now.drawn);
            }
            if step != steps.last().expect("one").0 {
                steps.push((step, now.clone()));
            }
            width -= if width > 400.0 { 20.0 } else { 10.0 };
        }
        let mut reached: Vec<Step> = Vec::new();
        for (step, _) in &steps {
            if !reached.contains(step) {
                reached.push(*step);
            }
        }
        let expected = if narrows {
            vec![Step::Natural, Step::LaidOutNarrower, Step::Zoomed, Step::SmallestText]
        } else {
            vec![Step::Natural, Step::Zoomed, Step::SmallestText]
        };
        for (step, at) in &steps {
            writeln!(
                report,
                "{name:<25} {:>5}  {:<17} {:>6.1}x{:<6.1} {:>6.1}  {:<6.4}  {:>7}  {}",
                at.width,
                format!("{step:?}"),
                at.drawn[0],
                at.drawn[1],
                at.laid,
                at.zoom,
                at.layouts,
                at.scrolls
            )
            .unwrap();
        }
        assert_eq!(reached, expected, "{name}: the steps it takes as its box narrows");
        if !narrows {
            assert_eq!(
                window.state().diagram.stats().layouts,
                2,
                "{name}: laid out as it is, and once asked to narrow"
            );
        }
    }
    print!("{report}");
    write_report("fit-steps.txt", &report);
}

#[test]
fn a_resize_lays_out_once_a_step_and_lowers_at_most_once_a_frame() {
    let capturer = Capturer::new();
    let mut window = window(&capturer, &fixture("across"), ViewOptions::fit(), (2400.0, 1200.0), 1.0);
    let natural = fitted(&mut window, 2400.0).laid;
    let first = fitted(&mut window, 900.0);
    // The steps a fit lays out at: whole points a twelfth of an octave apart.
    let steps: Vec<f64> = (60..140).map(|k| (f64::from(k) / 12.0).exp2().floor()).collect();
    let step_below = |width: f64| steps.iter().copied().filter(|step| *step <= width).fold(0.0, f64::max);
    let mut crossed = std::collections::BTreeSet::new();
    let mut before = first.clone();
    let mut report = String::new();
    for width in (300..900).rev() {
        let now = fitted(&mut window, width as f32);
        if f64::from(width) < natural {
            crossed.insert(step_below(f64::from(width)) as u64);
        }
        assert!(now.layouts - before.layouts <= 1, "at {width}: {} layouts", now.layouts - before.layouts);
        assert!(now.lowerings - before.lowerings <= 1, "at {width}: {} lowerings", now.lowerings - before.lowerings);
        if now.layouts != before.layouts {
            writeln!(report, "at {width}: laid out at {:.1} wide, zoom {:.4}", now.laid, now.zoom).unwrap();
        }
        before = now;
    }
    let laid_out = before.layouts - first.layouts;
    let lowered = before.lowerings - first.lowerings;
    writeln!(
        report,
        "natural width {natural:.1}; from 900 to 300 in 1 point steps: {laid_out} layouts at {} steps crossed, \
         {lowered} lowerings in 600 resizes; {} layouts before it, at 2400 and 900",
        crossed.len(),
        first.layouts
    )
    .unwrap();

    // Before Rune has a width the view lays the diagram out at its own size, which a fit needs
    // first anyway, and the same commit runs it again with the width.
    let mut narrow = self::window(&capturer, &fixture("across"), ViewOptions::fit(), (400.0, 1200.0), 1.0);
    let at_first = fitted(&mut narrow, 400.0);
    writeln!(
        report,
        "a view 400 points wide from its first frame: laid out at {:.1} wide, zoom {:.4}, {} layouts \
         (its own size, the probe at width 1, the step under 400) and {} lowerings",
        at_first.laid, at_first.zoom, at_first.layouts, at_first.lowerings
    )
    .unwrap();
    print!("{report}");
    write_report("resize.txt", &report);
    assert!(laid_out <= crossed.len() as u64, "{laid_out} layouts for {} steps", crossed.len());
    assert_eq!(at_first.layouts, 3, "the first frame lays out its own size, the probe and one step");
    assert!(at_first.lowerings <= 2, "{} lowerings on the first frame", at_first.lowerings);
}

#[test]
fn a_zoom_lowers_again_and_lays_nothing_out() {
    let capturer = Capturer::new();
    let options = ViewOptions::natural().palette(Palette::pele());
    let mut window = window(&capturer, &fixture("flowchart"), options, (720.0, 900.0), 2.0);
    window.capture();
    let at_one = window.state().diagram.stats();
    window.update(|viewer| viewer.options = viewer.options.clone().zoom(0.5));
    window.capture();
    let at_half = window.state().diagram.stats();
    window.update(|viewer| viewer.options = viewer.options.clone().zoom(1.0));
    window.capture();
    let back = window.state().diagram.stats();
    let report = format!("zoom 1:   {at_one:?}\nzoom 0.5: {at_half:?}\nzoom 1:   {back:?}\n");
    print!("{report}");
    write_report("zoom-stats.txt", &report);
    assert_eq!((at_half.layouts, at_half.lowerings), (at_one.layouts, at_one.lowerings + 1), "a zoom lowers once more");
    assert_eq!((back.layouts, back.lowerings), (at_half.layouts, at_half.lowerings), "a zoom it was drawn at is kept");
}

#[test]
fn a_link_and_a_tooltip_at_half_size_are_where_the_drawing_shows_them() {
    let capturer = Capturer::new();
    let mut report = String::new();
    let mut rects = Vec::new();
    for zoom in [1.0f32, 0.5] {
        let options = ViewOptions::natural().palette(Palette::pele()).zoom(zoom);
        let mut window = window(&capturer, &fixture("flowchart"), options, (720.0, 900.0), 1.0);
        window.capture();
        let elements = window.runtime().elements();
        let link = elements.iter().find(|element| element.role == "link").expect("the Checkout link").rect;
        let plain = elements.iter().find(|e| e.key.as_deref() == Some("node:B/shape")).expect("the decision").rect;
        rects.push(link);
        let [x, y, w, h] = link;
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        window.press(cx, cy);
        window.press(plain[0] + plain[2] / 2.0, plain[1] + plain[3] / 2.0);
        let followed: Vec<String> = window.state().followed.iter().map(|link| link.href.clone()).collect();
        window.rest(cx, cy, 2.0);
        let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
        writeln!(report, "zoom {zoom}: link at {link:?}; pressed {followed:?}; tooltip {shown:?}").unwrap();
        assert_eq!(followed, ["https://example.com/checkout"], "zoom {zoom}: one press on the link, none on the node");
        assert_eq!(shown.as_deref(), Some("Fetch the sources"), "zoom {zoom}");
    }
    let [one, half] = [rects[0], rects[1]];
    // Each box is snapped outwards to device pixels at its own zoom.
    assert!(one.iter().zip(half).all(|(a, b)| (a * 0.5 - b).abs() <= 1.0), "{one:?} at half is {half:?}");

    // A sankey link holds its own tooltip, found in boxes along it that zoom with it.
    let options = ViewOptions::natural().palette(Palette::pele()).zoom(0.5);
    let mut window = window(&capturer, &fixture("sankey"), options, (720.0, 900.0), 1.0);
    window.capture();
    let elements = window.runtime().elements();
    let link = elements.iter().find(|e| e.key.as_deref() == Some("edge:Gas~Power~link/shape")).expect("Gas to Power");
    let [x, y, w, h] = link.rect;
    window.rest(x + w / 2.0, y + h / 2.0, 2.0);
    let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
    writeln!(report, "sankey at 0.5: Gas to Power at {:?}; tooltip {shown:?}", link.rect).unwrap();
    print!("{report}");
    write_report("interaction.txt", &report);
    assert_eq!(shown.as_deref(), Some("Gas \u{2192} Power: 18"));
}
