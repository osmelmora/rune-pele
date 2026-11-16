//! The graph types drawn natively through rune-pele's public API, captured headless: each
//! source of Pele's documentation corpus beside Pele's own SVG rasterised by resvg, then keys,
//! themes, links, tooltips and the fallback.

mod common;

use std::cell::RefCell;
use std::fmt::Write as _;

use common::{
    FLAT_ERROR, LARGEST_PATCH, LARGEST_PATCH_1X, MOST_UNEXPLAINED, around_pixel, as_a_page_reads, compare, in_a_row,
    magnified, marked, out_dir_for, over, rasterise, text_boxes,
};
use pele_oracle::PeleRender;
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Window};
use rune_pele::{Diagram, Event, FontCatalog, Link, Palette, RenderStatus, ViewError, ViewOptions};

const RUN: &str = "rune-graph";
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

struct Viewer {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
    size: (f32, f32),
    error: RefCell<Option<ViewError>>,
    followed: Vec<Link>,
}

fn on_event(viewer: &mut Viewer, event: Event, _: &mut rune::Tx) {
    if let Event::Activate(link) = event {
        viewer.followed.push(link);
    }
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let root = el().size(s.size.0, s.size.1).fill(Color::WHITE);
    match s.diagram.view_with(cx, &s.source, s.options.clone(), on_event) {
        Ok(drawn) => {
            s.error.replace(None);
            root.child(drawn)
        }
        Err(error) => {
            s.error.replace(Some(error));
            root
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
        size,
        error: RefCell::new(None),
        followed: Vec::new(),
    };
    capturer.window(viewer, view, size, Env::new(false, scale))
}

/// The view Pele's SVG is compared with: Pele's metrics, Pele's colours, no width to fit.
fn as_pele() -> ViewOptions {
    ViewOptions::natural().pele_metrics().palette(Palette::pele())
}

/// Pele's SVG of each source under the id prefix a view lays out under, `pele`, as Pele's
/// `mount` draws in a page under one of its own. Without one, Pele draws a sankey's links in one
/// colour each.
fn as_pele_draws(sources: &[String]) -> Vec<PeleRender> {
    let cases: Vec<(String, String)> =
        sources.iter().map(|source| (source.clone(), "{\"idPrefix\":\"pele\"}".to_owned())).collect();
    pele_oracle::render_all_with(&cases)
}

fn corpus(kind: &str) -> Vec<String> {
    pele_oracle::corpus().into_iter().filter(|(name, _)| name == kind).flat_map(|(_, sources)| sources).collect()
}

fn write_report(name: &str, text: &str) {
    std::fs::create_dir_all(out_dir_for(RUN)).unwrap();
    std::fs::write(out_dir_for(RUN).join(name), text).unwrap();
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "490 drawings compared pixel by pixel take minutes unoptimised: run with --release"
)]
fn every_corpus_source_of_the_graph_types_draws_as_pele_draws_it_at_1x_and_2x() {
    let capturer = Capturer::new();
    let mut report = String::from(
        "Each corpus source through Diagram::view (Pele's metrics and colours, Inter) against Pele's SVG through\n\
         resvg (Arial), text boxes left out. patch: largest patch of unexplained px. share: unexplained px in 10,000.\n\
         flat: error in levels at the 99.9th percentile of flat px.\n\n\
         type        source scale   compared patch  share flat  over\n",
    );
    let mut table = String::from(
        "type         sources drawn fell-back  worst-patch@2x (source)  worst-share@2x (source)  worst-patch@1x (source)  worst-share@1x (source)  worst-flat (source@scale)  over-at-1x\n",
    );
    let mut findings = Vec::new();
    let mut hairlines = Vec::new();
    for kind in TYPES {
        let sources = corpus(kind);
        let pele = as_pele_draws(&sources);
        let (mut drawn, mut fell_back, mut sharp_at_1x) = (0, 0, 0);
        let mut worst: [(f64, String); 5] = std::array::from_fn(|_| (0.0, "-".to_owned()));
        for (index, (source, pele)) in sources.iter().zip(&pele).enumerate() {
            let PeleRender::Svg { svg, width, height, .. } = pele else {
                panic!("Pele draws every source of its corpus: {kind} {index}: {pele:?}")
            };
            let size = (width.ceil() as f32, height.ceil() as f32);
            let page = as_a_page_reads(svg);
            for scale in [1.0f32, 2.0] {
                let mut window = window(&capturer, source, as_pele(), size, scale);
                let ours = window.capture();
                let diagram = &window.state().diagram;
                assert_eq!(*window.state().error.borrow(), None, "{kind} {index}");
                if diagram.status() != RenderStatus::Pele {
                    let why = diagram.fallback().expect("a view that fell back says why");
                    if scale == 1.0 {
                        fell_back += 1;
                        writeln!(
                            report,
                            "{kind:<11} {index:>6}     -          -     -      -    -  fell back: {why:?}"
                        )
                        .unwrap();
                    }
                    continue;
                }
                let scene = diagram.scene().expect("a native scene");
                assert_eq!(
                    (scene.size().width, scene.size().height),
                    (*width, *height),
                    "{kind} {index} is Pele's size"
                );
                let theirs = rasterise(&page, scale);
                let (apart, differ) = compare(&ours, &theirs, &text_boxes(&scene), f64::from(scale));
                let row = in_a_row(&[&ours, &theirs, &marked(&ours, &differ)], (16.0 * scale) as u32);
                common::save_in(RUN, &row, &format!("side-by-side/{kind}/{index:03}@{scale}x.png"));
                let (sharp, other) = over(&apart, f64::from(scale));
                // The sharp limits hold at 2x. At 1x a source over them is listed, not failed.
                let failed: Vec<&String> =
                    if scale == 1.0 { other.iter().collect() } else { sharp.iter().chain(&other).collect() };
                writeln!(
                    report,
                    "{kind:<11} {index:>6} {scale:>5} {:>10} {:>5} {:>6.2} {:>4}  {}",
                    apart.compared,
                    apart.largest_blob,
                    apart.share(),
                    apart.flat_error,
                    sharp.iter().chain(&other).cloned().collect::<Vec<_>>().join("; ")
                )
                .unwrap();
                if scale == 1.0 && !sharp.is_empty() {
                    sharp_at_1x += 1;
                    hairlines.push(format!("{kind} {index}: {}", sharp.join("; ")));
                }
                if !failed.is_empty() {
                    findings.push(format!(
                        "{kind} {index}@{scale}x: {}",
                        failed.iter().map(|f| f.as_str()).collect::<Vec<_>>().join("; ")
                    ));
                }
                if !sharp.is_empty() || !other.is_empty() {
                    let near = |image: &rune_pele::capture::Pixels| around_pixel(image, apart.blob_at, 24);
                    let close = in_a_row(&[&near(&ours), &near(&theirs), &near(&marked(&ours, &differ))], 4);
                    common::save_in(
                        RUN,
                        &magnified(&close, 8),
                        &format!("side-by-side/{kind}/{index:03}@{scale}x.over.png"),
                    );
                }
                let at = if scale == 2.0 { 0 } else { 2 };
                let measures = [
                    (at, apart.largest_blob as f64, format!("{index}")),
                    (at + 1, apart.share(), format!("{index}")),
                    (4, f64::from(apart.flat_error), format!("{index}@{scale}x")),
                ];
                for (which, measure, source) in measures {
                    if measure > worst[which].0 {
                        worst[which] = (measure, source);
                    }
                }
                drawn += usize::from(scale == 1.0);
            }
        }
        writeln!(
            table,
            "{kind:<12} {:>7} {drawn:>5} {fell_back:>9}  {:>11.0} px ({:<5})  {:>14.2} ({:<5})  {:>11.0} px ({:<5})  {:>14.2} ({:<5})  {:>10.0} ({:<9})  {sharp_at_1x:>10}",
            sources.len(),
            worst[0].0,
            worst[0].1,
            worst[1].0,
            worst[1].1,
            worst[2].0,
            worst[2].1,
            worst[3].0,
            worst[3].1,
            worst[4].0,
            worst[4].1
        )
        .unwrap();
    }
    let limits = format!(
        "limits: patch {LARGEST_PATCH} square points ({} px at 2x) and share {MOST_UNEXPLAINED} in 10,000, held at 2x and listed \
         at 1x; flat {FLAT_ERROR} levels; patch at 1x {LARGEST_PATCH_1X} px.\n\
         {} sources are over the patch or share limit at 1x.\n",
        LARGEST_PATCH * 4.0,
        hairlines.len()
    );
    print!("{table}{limits}");
    write_report(
        "side-by-side.txt",
        &format!("{table}{limits}\n{report}\nover the sharp limits at 1x:\n{}\n", hairlines.join("\n")),
    );
    assert!(
        findings.is_empty(),
        "{} sources are not drawn as Pele draws them:\n{}",
        findings.len(),
        findings.join("\n")
    );
}

/// One source of each opened type, and an edit of it that changes a value and keeps what it
/// names: `tests/fixtures/rune/<type>.mmd`.
const FIXTURES: [(&str, &str, [&str; 2]); 32] = [
    ("flowchart", include_str!("../../../tests/fixtures/rune/flowchart.mmd"), ["Checkout", "Clone the repository"]),
    ("class", include_str!("../../../tests/fixtures/rune/class.mmd"), ["float radius", "float diameter"]),
    ("state", include_str!("../../../tests/fixtures/rune/state.mmd"), ["Keeps the last error", "Remembers why"]),
    ("er", include_str!("../../../tests/fixtures/rune/er.mmd"), ["places", "makes"]),
    ("requirement", include_str!("../../../tests/fixtures/rune/requirement.mmd"), ["99.9%", "99.99%"]),
    ("usecase", include_str!("../../../tests/fixtures/rune/usecase.mmd"), ["Browse the catalogue", "Look around"]),
    ("swimlane", include_str!("../../../tests/fixtures/rune/swimlane.mmd"), ["[Pack]", "[Wrap it up]"]),
    ("agentflow", include_str!("../../../tests/fixtures/rune/agentflow.mmd"), ["Gather sources", "Collect sources"]),
    ("architecture", include_str!("../../../tests/fixtures/rune/architecture.mmd"), ["Primary store", "Main store"]),
    ("treeview", include_str!("../../../tests/fixtures/rune/treeview.mmd"), ["entry point", "where it starts"]),
    ("git", include_str!("../../../tests/fixtures/rune/git.mmd"), ["v1.0", "v1.1"]),
    ("radar", include_str!("../../../tests/fixtures/rune/radar.mmd"), ["{4, 3, 2, 5, 3}", "{4, 3, 3, 5, 3}"]),
    ("timeline", include_str!("../../../tests/fixtures/rune/timeline.mmd"), ["Public beta", "Open beta"]),
    ("xychart", include_str!("../../../tests/fixtures/rune/xychart.mmd"), ["7500", "7900"]),
    ("quadrant", include_str!("../../../tests/fixtures/rune/quadrant.mmd"), ["[0.3, 0.6]", "[0.35, 0.62]"]),
    ("sequence", include_str!("../../../tests/fixtures/rune/sequence.mmd"), ["Place the order", "Send the order"]),
    ("gantt", include_str!("../../../tests/fixtures/rune/gantt.mmd"), ["Implement", "Write code"]),
    ("block", include_str!("../../../tests/fixtures/rune/block.mmd"), ["(\"Store\")", "(\"Data store\")"]),
    ("c4", include_str!("../../../tests/fixtures/rune/c4.mmd"), ["Takes payments", "Takes card payments"]),
    ("railroad", include_str!("../../../tests/fixtures/rune/railroad.mmd"), ["\"Numbers\"", "\"Whole numbers\""]),
    ("info", include_str!("../../../tests/fixtures/rune/info.mmd"), [" showInfo", ""]),
    ("cynefin", include_str!("../../../tests/fixtures/rune/cynefin.mmd"), ["Incident Response", "Incident Handling"]),
    ("wardley", include_str!("../../../tests/fixtures/rune/wardley.mmd"), ["[0.43, 0.35]", "[0.45, 0.35]"]),
    ("venn", include_str!("../../../tests/fixtures/rune/venn.mmd"), ["A good feature", "A great feature"]),
    ("packet", include_str!("../../../tests/fixtures/rune/packet.mmd"), ["UDP Packet", "UDP Datagram"]),
    (
        "eventmodeling",
        include_str!("../../../tests/fixtures/rune/eventmodeling.mmd"),
        ["description: string", "description: text"],
    ),
    ("treemap", include_str!("../../../tests/fixtures/rune/treemap.mmd"), ["120", "121"]),
    ("mindmap", include_str!("../../../tests/fixtures/rune/mindmap.mmd"), ["Ship it", "Ship the release"]),
    ("journey", include_str!("../../../tests/fixtures/rune/journey.mmd"), ["Make tea: 5", "Make tea: 3"]),
    ("kanban", include_str!("../../../tests/fixtures/rune/kanban.mmd"), ["Write the docs", "Write the guide"]),
    ("ishikawa", include_str!("../../../tests/fixtures/rune/ishikawa.mmd"), ["as of March", "as of April"]),
    ("sankey", include_str!("../../../tests/fixtures/rune/sankey.mmd"), ["Gas,Power,18", "Gas,Power,12"]),
];

const VIEWPORT: (f32, f32) = (720.0, 900.0);

/// The keys of the diagram's elements: one per drawing, and one ending `~group` per group that
/// is an element. Rune renames a key it meets twice among siblings, so no key here was.
fn element_keys(window: &Window<'_, Viewer>) -> Vec<String> {
    let keys = window.runtime().elements().into_iter().filter_map(|element| element.key);
    let mut keys: Vec<String> = keys.filter(|key| key != "diagram").collect();
    keys.sort();
    keys
}

/// The keys of the drawings of `scene` that paint something: a shape with a fill or a stroke, a
/// run of text with ink or an outline, an icon's built-in glyph.
fn drawn_keys(scene: &pele_core::Scene) -> Vec<String> {
    use pele_core::scene::{Geometry, Paint, PaintVisit};
    let mut keys: Vec<String> = scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => Some(draw),
            _ => None,
        })
        .filter(|draw| match &draw.geometry {
            Geometry::Text(run) => !run.text.is_empty(),
            Geometry::Icon { glyph, .. } => glyph.is_some(),
            Geometry::Image { .. } => false,
            _ => true,
        })
        .filter(|draw| {
            matches!(draw.geometry, Geometry::Icon { .. })
                || draw.style.fill != Paint::None
                || draw.style.stroke.is_some()
        })
        .map(|draw| draw.key.to_string())
        .collect();
    keys.sort();
    keys
}

/// A palette that differs from Pele's in every role and series the opened types draw in.
fn other_palette() -> Palette {
    let srgb = |r: f32, g: f32, b: f32| {
        let [r, g, b] = [r, g, b].map(rune::color::srgb_decode);
        Color { r, g, b, a: 1.0 }
    };
    let series = [
        srgb(0.85, 0.37, 0.65),
        srgb(0.40, 0.76, 0.65),
        srgb(0.99, 0.55, 0.38),
        srgb(0.55, 0.63, 0.80),
        srgb(0.65, 0.85, 0.33),
        srgb(1.00, 0.85, 0.18),
        srgb(0.90, 0.77, 0.58),
        srgb(0.70, 0.70, 0.70),
    ];
    let palette =
        series.into_iter().enumerate().fold(Palette::pele(), |palette, (at, colour)| palette.with_series(at, colour));
    palette
        .with(rune_pele::Role::Accent, srgb(0.95, 0.60, 0.20))
        .with(rune_pele::Role::Background, srgb(0.10, 0.11, 0.13))
        .with(rune_pele::Role::Foreground, srgb(0.93, 0.93, 0.95))
        .with(rune_pele::Role::Line, srgb(0.80, 0.82, 0.88))
        .with(rune_pele::Role::Border, srgb(0.50, 0.55, 0.65))
        .with(rune_pele::Role::Surface, srgb(0.18, 0.20, 0.24))
        .with(rune_pele::Role::SurfaceAlt, srgb(0.24, 0.26, 0.31))
        .with(rune_pele::Role::Muted, srgb(0.65, 0.67, 0.72))
}

fn changed_pixels(a: &rune_pele::capture::Pixels, b: &rune_pele::capture::Pixels) -> usize {
    a.rgba.chunks(4).zip(b.rgba.chunks(4)).filter(|(p, q)| p != q).count()
}

#[test]
fn every_opened_type_keys_each_drawing_once_and_keeps_its_keys() {
    let capturer = Capturer::new();
    let mut report = String::new();
    for (kind, source, [from, to]) in FIXTURES {
        assert!(source.contains(from), "{kind}'s edit finds {from:?}");
        let options = ViewOptions::new(VIEWPORT.0).palette(Palette::pele());
        let mut window = window(&capturer, source, options, VIEWPORT, 1.0);
        window.capture();
        assert_eq!(*window.state().error.borrow(), None, "{kind}");
        assert_eq!(window.state().diagram.status(), RenderStatus::Pele, "{kind} is drawn natively");
        let keys = element_keys(&window);
        let scene = window.state().diagram.scene().expect("a native scene");
        let (groups, drawings): (Vec<String>, Vec<String>) =
            keys.iter().cloned().partition(|key| key.ends_with("~group"));
        assert_eq!(drawings, drawn_keys(&scene), "{kind}: every drawing is one element, keyed by its fragment");
        let unique: std::collections::HashSet<&String> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len(), "{kind}: no key is used twice");
        assert!(keys.iter().all(|key| !key.contains("#dup")), "{kind}: Rune renamed no key");

        window.update(|viewer| viewer.options = viewer.options.clone().id_prefix("elsewhere"));
        window.capture();
        assert_eq!(element_keys(&window), keys, "{kind}: an idPrefix names SVG ids, not elements");

        window.update(|viewer| viewer.source = source.replace(from, to));
        window.capture();
        assert_eq!(*window.state().error.borrow(), None, "{kind} after the edit");
        assert_eq!(element_keys(&window), keys, "{kind}: a new value keeps every element");
        assert_eq!(window.state().diagram.stats().layouts, 3, "{kind}: each of the three inputs laid out once");
        writeln!(
            report,
            "{kind:<12} {:>4} drawings, {:>3} groups: keys unique, kept by idPrefix and by {from:?} -> {to:?}",
            drawings.len(),
            groups.len()
        )
        .unwrap();
    }
    print!("{report}");
    write_report("keys.txt", &report);
}

#[test]
fn a_palette_change_repaints_every_opened_type_without_laying_out_or_measuring() {
    let capturer = Capturer::new();
    let mut report = String::new();
    for (kind, source, _) in FIXTURES {
        let options = ViewOptions::new(VIEWPORT.0).palette(Palette::pele());
        let mut window = window(&capturer, source, options, VIEWPORT, 2.0);
        let first = window.capture();
        common::save_in(RUN, &first, &format!("native/{kind}@2x.png"));
        let laid_out = window.state().diagram.stats();
        assert_eq!((laid_out.layouts, laid_out.paints), (1, 1), "{kind}");
        assert!(laid_out.measurements > 0, "{kind}: layout measured its labels");

        let again = window.capture();
        assert_eq!(window.state().diagram.stats(), laid_out, "{kind}: an unchanged view reuses its paint");
        assert_eq!(changed_pixels(&first, &again), 0, "{kind}");

        window.update(|viewer| viewer.options = viewer.options.clone().palette(other_palette()));
        let repainted = window.capture();
        common::save_in(RUN, &repainted, &format!("native/{kind}.other-palette@2x.png"));
        let after = window.state().diagram.stats();
        let changed = changed_pixels(&first, &repainted);
        let drawn = first.rgba.chunks(4).filter(|px| px[..3] != [255, 255, 255]).count();
        writeln!(
            report,
            "{kind:<12} before {laid_out:?}\n{kind:<12} after  {after:?}, {changed} px changed of {drawn} drawn"
        )
        .unwrap();
        assert_eq!(
            after,
            rune_pele::ViewStats { paints: laid_out.paints + 1, ..laid_out },
            "{kind}: a palette change paints once more and lays out and measures nothing"
        );
        assert!(2 * changed > drawn, "{kind}: most of the drawing changed colour: {changed} of {drawn} pixels");
    }
    print!("{report}");
    write_report("theme-change.txt", &report);
}

fn one(kind: &str) -> &'static str {
    FIXTURES.iter().find(|(name, ..)| *name == kind).expect("a fixture of the type").1
}

#[test]
fn what_is_not_drawn_natively_is_drawn_by_rune_mermaid_and_says_why() {
    use rune_pele::Fallback;
    use rune_pele::lower::Unsupported;
    let capturer = Capturer::new();
    let options = || ViewOptions::new(VIEWPORT.0).palette(Palette::pele());
    let report = RefCell::new(String::new());
    let fell_back = |name: &str, source: &str| {
        let mut window = window(&capturer, source, options(), VIEWPORT, 1.0);
        let pixels = window.capture();
        common::save_in(RUN, &pixels, &format!("fallback/{name}.png"));
        let diagram = &window.state().diagram;
        let (status, why, error) = (diagram.status(), diagram.fallback(), window.state().error.borrow().clone());
        writeln!(report.borrow_mut(), "{name:<22} {status:?}, fallback {why:?}, error {error:?}").unwrap();
        (status, why, error)
    };

    // Every type of Pele 1.13.0 is ported and lowered, so no source falls back as `NotPorted`
    // or for its kind. A kind a later pin adds stays with rune-mermaid until it joins `NATIVE`.
    let unopened: Vec<_> =
        pele_core::DiagramKind::ALL.iter().filter(|kind| !rune_pele::lower::NATIVE.contains(kind)).collect();
    assert_eq!(unopened, Vec::<&pele_core::DiagramKind>::new(), "every kind is drawn natively");

    // A line through text is a declaration the Scene's walk does not read.
    let struck = include_str!("../../../tests/fixtures/rune/unlowered.mmd");
    let unread = Fallback::NotLowered(Unsupported::Style("text-decoration:line-through".to_owned()));
    assert_eq!(fell_back("flowchart-line-through", struck), (RenderStatus::RuneMermaid, Some(unread), None));

    let picture = "flowchart LR\n    A@{ img: \"https://example.com/a.png\", label: \"Pictured\", w: 60, h: 60 } --> B";
    let image = Fallback::NotLowered(Unsupported::Image);
    assert_eq!(fell_back("flowchart-picture", picture), (RenderStatus::RuneMermaid, Some(image), None));

    // The same flowchart without the declaration is drawn natively.
    let plain = struck.lines().take(2).collect::<Vec<_>>().join("\n");
    assert_eq!(fell_back("flowchart-plain", &plain), (RenderStatus::Pele, None, None));

    // Agentflow is drawn from its Scene. rune-mermaid has no agentflow, so there is nothing to
    // fall back to.
    let agentflow = corpus("agentflow").remove(0);
    assert_eq!(fell_back("agentflow", &agentflow), (RenderStatus::Pele, None, None));

    // A source pele-core rejects is its error, not another grammar's drawing.
    let (status, _, error) = fell_back("flowchart-bad", "flowchart LR\n    A --> ");
    assert!(matches!(error, Some(ViewError::Core(pele_core::RenderError::Diagnostic(_)))), "{error:?}");
    assert_eq!(status, RenderStatus::Unrendered);
    let report = report.into_inner();
    print!("{report}");
    write_report("fallback.txt", &report);
}

#[test]
fn a_link_tells_the_app_and_a_tooltip_shows_where_the_pointer_rests() {
    let capturer = Capturer::new();
    let options = ViewOptions::new(VIEWPORT.0).palette(Palette::pele());
    let mut window = window(&capturer, one("flowchart"), options, VIEWPORT, 1.0);
    window.capture();
    let elements = window.runtime().elements();
    let links: Vec<_> = elements.iter().filter(|element| element.role == "link").collect();
    let labels: Vec<Option<&str>> = links.iter().map(|link| link.label.as_deref()).collect();
    assert_eq!(labels, [Some("https://example.com/checkout"), Some("https://example.com/releases")]);
    let helps: Vec<Option<&str>> = links.iter().map(|link| link.help.as_deref()).collect();
    assert_eq!(helps, [Some("Fetch the sources"), Some("Where releases go")], "a link's group carries its tooltip");

    // The node "Checkout" is a link: a press inside its box tells the app, and opens nothing.
    let [x, y, w, h] = links[0].rect;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    assert!(window.state().followed.is_empty());
    window.press(cx, cy);
    let followed = &window.state().followed;
    assert_eq!(followed.len(), 1, "one press, one event: {followed:?}");
    assert_eq!(
        (followed[0].href.as_str(), followed[0].target, followed[0].internal),
        ("https://example.com/checkout", Some("_blank"), false)
    );

    // A press on a node that is no link tells the app nothing.
    let plain = elements.iter().find(|element| element.key.as_deref() == Some("node:B/shape")).expect("the decision");
    window.press(plain.rect[0] + plain.rect[2] / 2.0, plain.rect[1] + plain.rect[3] / 2.0);
    assert_eq!(window.state().followed.len(), 1);

    // Resting the pointer on the node shows its tooltip, and resting it elsewhere shows none.
    assert_eq!(window.runtime().help_tooltip(), None);
    window.rest(cx, cy, 2.0);
    let shown = window.runtime().help_tooltip().map(|(text, at)| (text.to_owned(), at));
    window.rest(plain.rect[0] + plain.rect[2] / 2.0, plain.rect[1] + plain.rect[3] / 2.0, 2.0);
    let elsewhere = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
    let report = format!(
        "links: {labels:?}\ntooltips: {helps:?}\nfollowed after a press at ({cx}, {cy}): {:?}\n\
         tooltip after resting there: {shown:?}\ntooltip after resting on a node without one: {elsewhere:?}\n",
        window.state().followed
    );
    print!("{report}");
    write_report("interaction.txt", &report);
    assert_eq!(shown.map(|(text, _)| text).as_deref(), Some("Fetch the sources"));
    assert_eq!(elsewhere, None);
}

/// What the pointer can do on one opened type's fixture.
struct Interaction {
    kind: &'static str,
    /// Each link's address and the browsing context it asks for, in paint order.
    links: &'static [(&'static str, Option<&'static str>)],
    tooltips: &'static [&'static str],
}

/// The fixtures of the opened types whose Scenes have links or tooltips.
const INTERACTIONS: [Interaction; 5] = [
    Interaction { kind: "gantt", links: &[("https://example.com/implement", None)], tooltips: &[] },
    Interaction {
        kind: "c4",
        links: &[("https://example.com/checkout", None)],
        tooltips: &["Card details go over TLS"],
    },
    Interaction { kind: "kanban", links: &[("https://example.com/browse/DOC-12", Some("_blank"))], tooltips: &[] },
    Interaction { kind: "treemap", links: &[], tooltips: &["Marketing and outreach to new customers"] },
    Interaction { kind: "journey", links: &[], tooltips: &["Me", "Cat"] },
];

#[test]
fn the_links_of_each_opened_type_tell_the_app_and_its_tooltips_show_where_the_pointer_rests() {
    let capturer = Capturer::new();
    let mut report = String::new();
    for Interaction { kind, links, tooltips } in INTERACTIONS {
        let options = ViewOptions::new(VIEWPORT.0).palette(Palette::pele());
        let mut window = window(&capturer, one(kind), options, VIEWPORT, 1.0);
        window.capture();
        assert_eq!(window.state().diagram.status(), RenderStatus::Pele, "{kind} is drawn natively");
        let elements = window.runtime().elements();
        let centre = |rect: [f32; 4]| (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);

        let pressable: Vec<_> = elements.iter().filter(|element| element.role == "link").collect();
        let labels: Vec<Option<&str>> = pressable.iter().map(|link| link.label.as_deref()).collect();
        let hrefs: Vec<Option<&str>> = links.iter().map(|(href, _)| Some(*href)).collect();
        assert_eq!(labels, hrefs, "{kind}: one pressable element per link");
        for (link, (href, target)) in pressable.iter().zip(links) {
            let before = window.state().followed.len();
            let (x, y) = centre(link.rect);
            window.press(x, y);
            let followed = &window.state().followed[before..];
            assert_eq!(followed.len(), 1, "{kind}: one press, one event: {followed:?}");
            assert_eq!((followed[0].href.as_str(), followed[0].target), (*href, *target), "{kind}");
            writeln!(report, "{kind:<8} press at ({x}, {y}) told the app {:?}", followed[0]).unwrap();
        }

        let helps: std::collections::BTreeSet<&str> =
            elements.iter().filter_map(|element| element.help.as_deref()).collect();
        assert_eq!(helps, tooltips.iter().copied().collect(), "{kind}: the tooltips the groups carry");
        for tooltip in tooltips {
            let holder =
                elements.iter().find(|element| element.help.as_deref() == Some(*tooltip)).expect("found above");
            let (x, y) = centre(holder.rect);
            window.rest(x, y, 2.0);
            let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
            writeln!(report, "{kind:<8} rest at ({x}, {y}) showed {shown:?}").unwrap();
            assert_eq!(shown.as_deref(), Some(*tooltip), "{kind}: the tooltip shows where the pointer rests");
        }
    }
    print!("{report}");
    write_report("interaction-opened.txt", &report);
}

/// The x of a gantt chart's today marker, its line in the accent 2 wide, and the left edge and
/// width of the bar of the fixture's task `design`.
fn today_and_design(scene: &pele_core::Scene) -> (Option<f64>, [f64; 2]) {
    use pele_core::scene::{Geometry, Paint, PaintVisit, PathCommand, Role};
    let (mut today, mut design) = (None, None);
    for visit in scene.paint() {
        let PaintVisit::Draw(draw) = visit else { continue };
        let marker = draw.style.stroke.as_ref().is_some_and(|s| s.paint == Paint::Role(Role::Accent) && s.width == 2.0);
        match &draw.geometry {
            Geometry::Path(commands) if marker => {
                if let Some(PathCommand::Move(at)) = commands.first() {
                    today = Some(draw.origin.x + at.x);
                }
            }
            Geometry::Rect { origin, size, .. } if draw.key.to_string() == "node:design/shape" => {
                design = Some([draw.origin.x + origin.x, size.width]);
            }
            _ => {}
        }
    }
    (today, design.expect("the fixture's design bar"))
}

#[test]
fn a_gantt_chart_lays_out_again_for_another_now_and_not_for_the_same_one() {
    let capturer = Capturer::new();
    // Noon UTC on 8 and on 14 January 2026, inside the fixture's chart.
    let (eighth, fourteenth) = (1_767_873_600_000.0, 1_768_392_000_000.0);
    let options = |now: f64| ViewOptions::new(VIEWPORT.0).palette(Palette::pele()).now(now);
    let mut window = window(&capturer, one("gantt"), options(eighth), VIEWPORT, 1.0);
    let before = window.capture();
    let scene = window.state().diagram.scene().expect("a native scene");
    let (today, [left, width]) = today_and_design(&scene);
    // The design bar runs the five days from 5 January, so noon on the 8th is 3.5 of them in.
    // Pele writes hundredths, so the marker and the bar are each within 0.005 of where they are.
    let expected = left + 3.5 * width / 5.0;
    assert!((today.expect("a marker on the 8th") - expected).abs() < 0.02, "{today:?} against {expected}");
    assert_eq!(window.state().diagram.stats().layouts, 1);

    window.update(|viewer| viewer.options = options(eighth));
    let again = window.capture();
    assert_eq!(window.state().diagram.stats().layouts, 1, "the same now lays nothing out");
    assert_eq!(changed_pixels(&before, &again), 0);

    window.update(|viewer| viewer.options = options(fourteenth));
    let after = window.capture();
    let scene = window.state().diagram.scene().expect("a native scene");
    let (moved, _) = today_and_design(&scene);
    let expected = left + 9.5 * width / 5.0;
    assert!((moved.expect("a marker on the 14th") - expected).abs() < 0.02, "{moved:?} against {expected}");
    assert_eq!(window.state().diagram.stats().layouts, 2, "another now lays the chart out once more");
    assert!(changed_pixels(&before, &after) > 0, "the marker moved on screen");
    let report =
        format!("now 8 Jan noon: marker at {today:?}; again: 1 layout; 14 Jan noon: marker at {moved:?}, 2 layouts\n");
    print!("{report}");
    write_report("gantt-now.txt", &report);
}

/// Pele fills a sankey link along a gradient only where the drawing has an id prefix to name
/// the gradient by, which its corpus is drawn without. So each sankey source of the corpus,
/// and the fixture, is drawn here with one, beside Pele's SVG with the same.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "18 drawings compared pixel by pixel take a minute unoptimised: run with --release"
)]
fn every_sankey_source_fills_its_links_along_gradients_as_pele_does_in_a_page() {
    let capturer = Capturer::new();
    let mut sources = corpus("sankey");
    sources.push(one("sankey").to_owned());
    let mut report = String::from("source scale   compared patch  share flat  gradients  over\n");
    let mut findings = Vec::new();
    for (index, (source, pele)) in sources.iter().zip(&as_pele_draws(&sources)).enumerate() {
        let PeleRender::Svg { svg, width, height, .. } = pele else { panic!("Pele draws sankey {index}: {pele:?}") };
        let gradients = svg.matches("<linearGradient ").count();
        assert!(gradients > 0, "sankey {index}: Pele fills its links along gradients");
        let size = (width.ceil() as f32, height.ceil() as f32);
        let page = as_a_page_reads(svg);
        for scale in [1.0f32, 2.0] {
            let mut window = window(&capturer, source, as_pele(), size, scale);
            let ours = window.capture();
            let diagram = &window.state().diagram;
            assert_eq!((window.state().error.borrow().clone(), diagram.fallback()), (None, None), "sankey {index}");
            let scene = diagram.scene().expect("a native scene");
            let graded = scene.paint().filter(|visit| match visit {
                pele_core::scene::PaintVisit::Draw(draw) => draw.style.gradient.is_some(),
                _ => false,
            });
            assert!(graded.count() >= gradients, "sankey {index}: the Scene has a gradient for each of Pele's");
            let theirs = rasterise(&page, scale);
            let (apart, differ) = compare(&ours, &theirs, &text_boxes(&scene), f64::from(scale));
            let row = in_a_row(&[&ours, &theirs, &marked(&ours, &differ)], (16.0 * scale) as u32);
            common::save_in(RUN, &row, &format!("side-by-side/sankey-gradient/{index:03}@{scale}x.png"));
            let (sharp, other) = over(&apart, f64::from(scale));
            let failed: Vec<&String> =
                if scale == 1.0 { other.iter().collect() } else { sharp.iter().chain(&other).collect() };
            writeln!(
                report,
                "{index:>6} {scale:>5} {:>10} {:>5} {:>6.2} {:>4} {gradients:>10}  {}",
                apart.compared,
                apart.largest_blob,
                apart.share(),
                apart.flat_error,
                sharp.iter().chain(&other).cloned().collect::<Vec<_>>().join("; ")
            )
            .unwrap();
            if !failed.is_empty() {
                let near = |image: &rune_pele::capture::Pixels| around_pixel(image, apart.blob_at, 24);
                let close = in_a_row(&[&near(&ours), &near(&theirs), &near(&marked(&ours, &differ))], 4);
                let name = format!("side-by-side/sankey-gradient/{index:03}@{scale}x.over.png");
                common::save_in(RUN, &magnified(&close, 8), &name);
                let failed: Vec<&str> = failed.iter().map(|f| f.as_str()).collect();
                findings.push(format!("sankey {index}@{scale}x: {}", failed.join("; ")));
            }
        }
    }
    print!("{report}");
    write_report("gradients.txt", &report);
    assert!(findings.is_empty(), "{} are not drawn as Pele draws them:\n{}", findings.len(), findings.join("\n"));
}

/// A sankey link as its Scene draws it: a band between two S curves, in the window's points.
struct Band {
    key: String,
    tooltip: String,
    /// Where the upper curve starts and ends. Its handles are level with them, half way along.
    ends: [[f64; 2]; 2],
    depth: f64,
}

impl Band {
    /// The upper curve at `t`, from 0 at its start to 1 at its end.
    fn top(&self, t: f64) -> [f64; 2] {
        let ([from, to], u) = (self.ends, 1.0 - t);
        let middle = (from[0] + to[0]) / 2.0;
        let x = from[0] * u * u * u + 3.0 * middle * u * t + to[0] * t * t * t;
        [x, from[1] + (to[1] - from[1]) * (3.0 - 2.0 * t) * t * t]
    }

    /// A point on the band's middle line.
    fn middle(&self, t: f64) -> (f32, f32) {
        let [x, y] = self.top(t);
        (x as f32, (y + self.depth / 2.0) as f32)
    }

    fn covers(&self, (x, y): (f32, f32)) -> bool {
        let (x, y) = (f64::from(x), f64::from(y));
        let along = (0..=400).map(|i| self.top(f64::from(i) / 400.0));
        let nearest = along.min_by(|a, b| (a[0] - x).abs().total_cmp(&(b[0] - x).abs()));
        nearest.is_some_and(|[at, top]| (at - x).abs() < 1.0 && (top - 1.0..=top + self.depth + 1.0).contains(&y))
    }
}

/// The links of a sankey Scene in paint order, each with the tooltip of the group it is in.
fn bands(scene: &pele_core::Scene) -> Vec<Band> {
    use pele_core::scene::{Geometry, PaintVisit, PathCommand};
    let mut bands = Vec::new();
    let mut tooltip = None;
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => tooltip = group.tooltip,
            PaintVisit::Draw(draw) => {
                let (Some(tooltip), Geometry::Path(commands)) = (tooltip.take(), &draw.geometry) else { continue };
                let [PathCommand::Move(start), PathCommand::Cubic { end, .. }, PathCommand::Line(below), ..] =
                    commands[..]
                else {
                    panic!("a link is two curves and two sides: {commands:?}")
                };
                let ends = [start, end].map(|p| [draw.origin.x + p.x, draw.origin.y + p.y]);
                bands.push(Band { key: draw.key.to_string(), tooltip, ends, depth: below.y - end.y });
            }
            PaintVisit::End => {}
        }
    }
    bands
}

/// Three points along the middle of the fixture's link from `from` to `to`.
fn link_of(scene: &pele_core::Scene, from: &str, to: &str) -> [(f32, f32); 3] {
    let key = format!("edge:{from}~{to}~link/shape");
    let band = bands(scene).into_iter().find(|band| band.key == key).unwrap_or_else(|| panic!("no {key}"));
    [0.02, 0.5, 0.98].map(|t| band.middle(t))
}

#[test]
fn a_shape_that_holds_its_tooltip_shows_it_wherever_the_pointer_rests_on_the_shape() {
    let capturer = Capturer::new();
    let options = ViewOptions::new(VIEWPORT.0).palette(Palette::pele());
    let mut window = window(&capturer, one("sankey"), options, VIEWPORT, 1.0);
    window.capture();
    assert_eq!(window.state().diagram.status(), RenderStatus::Pele);
    let scene = window.state().diagram.scene().expect("a native scene");
    let links = [
        ("Coal", "Power", "Coal \u{2192} Power: 25"),
        ("Gas", "Power", "Gas \u{2192} Power: 18"),
        ("Solar", "Power", "Solar \u{2192} Power: 7"),
        ("Power", "Homes", "Power \u{2192} Homes: 22"),
        ("Power", "Industry", "Power \u{2192} Industry: 20"),
        ("Power", "Losses", "Power \u{2192} Losses: 8"),
    ];
    let mut report = String::new();
    assert_eq!(window.runtime().help_tooltip(), None);
    for (from, to, tooltip) in links {
        for (x, y) in link_of(&scene, from, to) {
            window.rest(x, y, 2.0);
            let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
            writeln!(report, "resting at ({x:.1}, {y:.1}) on {from} to {to}: {shown:?}").unwrap();
            assert_eq!(shown.as_deref(), Some(tooltip), "at ({x}, {y}) on the link from {from} to {to}");
        }
    }
    // A node holds no tooltip, and neither does the page.
    let elements = window.runtime().elements();
    let node = elements.iter().find(|element| element.key.as_deref() == Some("node:Power/shape")).expect("a node");
    for (x, y) in [(node.rect[0] + node.rect[2] / 2.0, node.rect[1] + node.rect[3] / 2.0), (2.0, 2.0)] {
        window.rest(x, y, 2.0);
        let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
        writeln!(report, "resting at ({x:.1}, {y:.1}) off every link: {shown:?}").unwrap();
        assert_eq!(shown, None, "at ({x}, {y})");
    }

    // Where links lie close, the box round one takes in its neighbours. Rune finds the pointer
    // in boxes, so a link is found in a row of narrow ones along it. In the corpus's densest
    // sankey, 68 links in 600 points, a point on a link then shows that link's tooltip nine
    // times in ten. One box round each link shows it four times in ten.
    let dense = corpus("sankey").remove(0);
    let mut window = self::window(&capturer, &dense, as_pele(), (616.0, 424.0), 1.0);
    window.capture();
    let links = bands(&window.state().diagram.scene().expect("a native scene"));
    let (mut own, mut other) = (0, 0);
    for (index, link) in links.iter().enumerate() {
        for point in [0.1, 0.3, 0.5, 0.7, 0.9].map(|t| link.middle(t)) {
            // A link drawn later over the point is rightly the one found there.
            if links[index + 1..].iter().any(|later| later.covers(point)) {
                continue;
            }
            window.rest(point.0, point.1, 2.0);
            let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
            if shown.as_deref() == Some(link.tooltip.as_str()) { own += 1 } else { other += 1 }
        }
    }
    let close = links.len();
    writeln!(report, "{close} close links, 5 points on each: {own} show the link's own tooltip, {other} another's")
        .unwrap();
    print!("{report}");
    write_report("shape-tooltips.txt", &report);
    assert!(own >= 8 * (own + other) / 10, "{own} points show their link's tooltip and {other} another's");
}

#[test]
fn a_palette_change_recolours_a_gradient_without_laying_out_or_measuring() {
    let capturer = Capturer::new();
    let srgb = |levels: [u8; 3]| {
        let [r, g, b] = levels.map(|level| rune::color::srgb_decode(f32::from(level) / 255.0));
        Color { r, g, b, a: 1.0 }
    };
    // Gas is the third node and Power the second: the link between them runs from the third
    // series colour to the second, at Pele's 40% for links, over the white page.
    let half_way = |gas: [u8; 3], power: [u8; 3]| {
        [0, 1, 2].map(|c| (255.0 + ((f64::from(gas[c]) + f64::from(power[c])) / 2.0 - 255.0) * 0.4).round() as u8)
    };
    let close = |got: [u8; 4], want: [u8; 3]| got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 3);
    let options = ViewOptions::new(VIEWPORT.0).palette(Palette::pele());
    let mut window = window(&capturer, one("sankey"), options, VIEWPORT, 2.0);
    let first = window.capture();
    let scene = window.state().diagram.scene().expect("a native scene");
    let (x, y) = link_of(&scene, "Gas", "Power")[1];
    let at = |pixels: &rune_pele::capture::Pixels| pixels.at((x * 2.0) as u32, (y * 2.0) as u32);
    let pele = half_way([0x54, 0xa2, 0x4b], [0xf5, 0x85, 0x18]);
    assert!(close(at(&first), pele), "half way along, in Pele's colours: {:?} for {pele:?}", at(&first));
    let laid_out = window.state().diagram.stats();

    let (red, blue) = ([200, 30, 30], [20, 40, 220]);
    let other = Palette::pele().with_series(2, srgb(red)).with_series(1, srgb(blue));
    window.update(|viewer| viewer.options = viewer.options.clone().palette(other));
    let repainted = window.capture();
    common::save_in(RUN, &repainted, "native/sankey-gradient.other-palette@2x.png");
    let want = half_way(red, blue);
    assert!(close(at(&repainted), want), "half way along, in the new colours: {:?} for {want:?}", at(&repainted));
    let after = window.state().diagram.stats();
    assert_eq!(
        after,
        rune_pele::ViewStats { paints: laid_out.paints + 1, ..laid_out },
        "a palette change paints once more and lays out and measures nothing"
    );
    let report = format!(
        "half way along the link from Gas to Power: {:?} in Pele's palette, {:?} in the other\nbefore {laid_out:?}\nafter  {after:?}\n",
        at(&first),
        at(&repainted)
    );
    print!("{report}");
    write_report("gradient-theme-change.txt", &report);
}

#[test]
fn a_casing_lets_the_pointer_through_and_a_view_without_a_handler_has_no_links() {
    fn inert_view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
        let drawn = s.diagram.view(cx, &s.source, s.options.clone()).expect("the fixture draws");
        el().size(s.size.0, s.size.1).fill(Color::WHITE).child(drawn)
    }
    let capturer = Capturer::new();
    // The edge into the styled group crosses a filled border, which Pele cases.
    let source = "flowchart TD\nsubgraph G [Group]\n  A[Inside]\nend\nB[Outside] --> A\nstyle G fill:#036\nclick A \"https://example.com/a\"";
    let viewer = Viewer {
        diagram: Diagram::new("diagram", FontCatalog::rune()),
        source: source.to_owned(),
        options: ViewOptions::new(VIEWPORT.0).palette(Palette::pele()),
        size: VIEWPORT,
        error: RefCell::new(None),
        followed: Vec::new(),
    };
    let mut window = capturer.window(viewer, inert_view, VIEWPORT, Env::new(false, 1.0));
    window.capture();
    let elements = window.runtime().elements();
    let casing =
        elements.iter().find(|element| element.key.as_deref().is_some_and(|key| key.contains("/casing~group")));
    assert!(casing.expect("the casings are a group of their own").inert, "the pointer passes through a casing");
    assert!(elements.iter().all(|element| element.role != "link"), "no handler, no link to press");
}

/// The source of `kind` with the most drawings among those harvested from Pele's tests.
fn largest(kind: &str) -> (String, usize) {
    use pele_core::scene::PaintVisit;
    let path = pele_oracle::root().join(format!("tests/fixtures/{kind}/harvest.json"));
    let harvest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("a harvest")).expect("JSON");
    let lists = harvest.as_object().expect("an object").values().filter_map(|list| list.as_array());
    let sources = lists.flatten().filter_map(|source| source.as_str());
    let drawings = |source: &str| {
        let scene = pele_core::parity_scene(source).ok()?;
        Some(scene.scene().paint().filter(|visit| matches!(visit, PaintVisit::Draw(_))).count())
    };
    let (source, count) = sources
        .filter_map(|source| Some((source, drawings(source)?)))
        .max_by_key(|(_, count)| *count)
        .expect("a source that draws");
    (source.to_owned(), count)
}

/// Times one view of `source`: its first frame, which lays out and paints, and a repaint in
/// another palette.
fn timed(capturer: &Capturer, source: &str, size: (f32, f32)) -> String {
    let options = ViewOptions::new(size.0).palette(Palette::pele());
    let started = std::time::Instant::now();
    let mut window = window(capturer, source, options, size, 2.0);
    let first_frame = started.elapsed();
    let diagram = &window.state().diagram;
    if diagram.status() != RenderStatus::Pele {
        return format!("not drawn natively: {:?} {:?}", diagram.fallback(), window.state().error.borrow());
    }
    let (first, census) = (diagram.times(), diagram.census().expect("a native view"));
    let elements = window.runtime().elements().len();
    let instances = window.runtime().instances.len();
    let started = std::time::Instant::now();
    window.capture();
    let capture = started.elapsed();
    window.update(|viewer| viewer.options = viewer.options.clone().palette(other_palette()));
    let started = std::time::Instant::now();
    window.capture();
    let repaint = started.elapsed();
    let again = window.state().diagram.times();
    format!(
        "layout {:>8.2?} lower {:>8.2?} paint {:>8.2?} first-frame {:>8.2?} capture {:>8.2?} | repaint: paint {:>8.2?} frame+capture {:>8.2?} | keyed {:>6} ops {:>7} deepest {} elements {:>6} instances {:>7}",
        first.layout,
        first.lower,
        first.paint,
        first_frame,
        capture,
        again.paint,
        repaint,
        census.elements,
        census.shape_ops,
        census.deepest,
        elements,
        instances
    )
}

/// Lowering and paint times and element counts for the largest harvested source of each type
/// and for Pele's nested state stress case. Run in release: `cargo test --release ... -- --ignored`.
#[test]
#[ignore = "a measurement, not a check: run it in release with --ignored --nocapture"]
fn how_long_the_largest_sources_take() {
    let capturer = Capturer::new();
    let mut report = String::from(
        "Times at 2x in a headless window: the first view (parse and layout, lowering, building elements), then a palette change.\n",
    );
    for kind in TYPES {
        let (source, drawings) = largest(kind);
        writeln!(report, "{kind:<12} {drawings:>5} drawings  {}", timed(&capturer, &source, (2000.0, 2000.0))).unwrap();
    }
    for nested in [8, 200, 3000] {
        let open: String = (0..nested).map(|i| format!("state s{i}{{\n")).collect();
        let source = format!("stateDiagram-v2\n{open}a-->b\n{}", "}\n".repeat(nested));
        writeln!(report, "state, {nested:>4} composites nested  {}", timed(&capturer, &source, (4000.0, 4000.0)))
            .unwrap();
    }
    let wide: String = (0..400).map(|i| format!("    n{i}[Step {i}] --> n{}[Step {}]\n", i + 1, i + 1)).collect();
    writeln!(
        report,
        "flowchart, 401 nodes in a chain  {}",
        timed(&capturer, &format!("flowchart TD\n{wide}"), (4000.0, 4000.0))
    )
    .unwrap();
    print!("{report}");
    write_report("performance.txt", &report);
}
