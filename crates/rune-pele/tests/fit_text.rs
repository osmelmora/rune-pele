//! What a fitted view draws its text at and when it turns, through rune-pele's public API in
//! Rune's headless runtime: as the width goes from 900 points to 200, no text a fixture draws at
//! the plain size or larger at its own size is drawn under the smallest text size, and a diagram
//! that runs across turns to run down where across would scroll.

// What only the tests that need rune-mermaid use is unused without `fallback`.
#![cfg_attr(not(feature = "fallback"), allow(dead_code, unused_imports))]

mod common;

use std::collections::HashMap;
use std::fmt::Write as _;

use common::out_dir_for;
use pele_core::Scene;
use pele_core::scene::{Geometry, PaintVisit};
use rune::{Cx, El, Env, Runtime, el};
use rune_pele::{Diagram, FontCatalog, RenderStatus, ViewOptions};

const RUN: &str = "rune-fit";
/// Pele's plain text size, which a view draws its labels at unless the app sets another.
const PLAIN: f64 = 16.0;
/// The least size a fitted view draws plain text at unless the app sets another.
const SMALLEST: f64 = 11.0;
/// Wider than any box the view is put in, and taller than any fixture drawn.
const WINDOW: (f32, f32) = (2000.0, 6000.0);

struct Boxed {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
    /// The width of the box the view is put in, in points.
    width: f32,
}

/// The view in a box `width` points wide, on a page: the window stretches its root to its size.
fn boxed_view(s: &Boxed, cx: &mut Cx<Boxed>) -> El {
    let drawn = s.diagram.view(cx, &s.source, s.options.clone()).expect("it draws");
    el().child(el().size(s.width, WINDOW.1).child(drawn))
}

fn runtime(source: &str, options: ViewOptions, width: f32) -> Runtime<Boxed> {
    let boxed =
        Boxed { diagram: Diagram::new("diagram", FontCatalog::rune()), source: source.to_owned(), options, width };
    let mut rt = Runtime::new(boxed, boxed_view, WINDOW, Env::new(false, 1.0));
    rt.frame(0.0);
    rt
}

fn resize(rt: &mut Runtime<Boxed>, width: f32, at: f64) {
    rt.update(|s, _| s.width = width);
    rt.frame(at);
}

fn fixture(name: &str) -> String {
    let path = pele_oracle::root().join(format!("tests/fixtures/rune/{name}.mmd"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn write_report(name: &str, text: &str) {
    std::fs::create_dir_all(out_dir_for(RUN)).unwrap();
    std::fs::write(out_dir_for(RUN).join(name), text).unwrap();
}

/// Each run of text the Scene draws, by its key: its text and its size in points.
fn text_runs(scene: &Scene) -> HashMap<String, (String, f64)> {
    scene
        .paint()
        .filter_map(|visit| {
            let PaintVisit::Draw(draw) = visit else { return None };
            let Geometry::Text(run) = draw.geometry else { return None };
            (!run.text.trim().is_empty()).then(|| (draw.key.to_string(), (run.text, run.font.size)))
        })
        .collect()
}

/// The smallest a run of text was drawn at: its size on screen, its text and the box's width.
#[derive(Clone, Debug)]
struct Least {
    drawn: f64,
    text: String,
    width: f32,
}

fn note(least: &mut Option<Least>, drawn: f64, text: &str, width: f32) {
    if least.as_ref().is_none_or(|least| drawn < least.drawn) {
        *least = Some(Least { drawn, text: text.to_owned(), width });
    }
}

fn shown(least: &Option<Least>) -> String {
    least
        .as_ref()
        .map_or("none".to_owned(), |least| format!("{:>6.2} at {:>3}  {:?}", least.drawn, least.width, least.text))
}

/// Every fixture the view draws natively, fitted in a box from 900 points wide to 200, ten points
/// at a time as a window's edge is dragged. A run the fixture draws at the plain size or larger at
/// its own size is plain text, and is never drawn under the smallest text size: not by the zoom,
/// and not by a narrower layout that sets it smaller.
#[cfg(feature = "fallback")]
#[test]
fn fitted_text_keeps_to_the_smallest_size_at_every_width() {
    let mut names: Vec<String> = std::fs::read_dir(pele_oracle::root().join("tests/fixtures/rune"))
        .expect("the fixtures")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "mmd"))
        .map(|path| path.file_stem().expect("a name").to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut report =
        String::from("fixture        scrolls from  smallest text                      smallest plain text\n");
    let mut under = Vec::new();
    let mut traced = String::new();
    for name in &names {
        let source = fixture(name);
        let natural = runtime(&source, ViewOptions::natural(), WINDOW.0);
        if natural.state.diagram.status() != RenderStatus::Pele {
            writeln!(report, "{name:<14} drawn by rune-mermaid, not fitted here").unwrap();
            continue;
        }
        let at_own_size = text_runs(&natural.state.diagram.scene().expect("a native scene"));
        let mut rt = runtime(&source, ViewOptions::fit(), 900.0);
        let (mut least, mut least_plain, mut scrolls) = (None, None, None);
        for (frame, width) in (200..=900).rev().step_by(10).enumerate() {
            let width = width as f32;
            resize(&mut rt, width, frame as f64);
            let now = fitted(&rt);
            if now.scrolls {
                scrolls.get_or_insert(width);
            }
            // Treemap sets its labels smaller in small cells, and packet runs across or down.
            if ["treemap", "packet"].contains(&name.as_str()) {
                writeln!(
                    traced,
                    "{name} at {width}: laid out {:.1}x{:.1}, zoom {:.4}, scrolls {}",
                    now.laid[0], now.laid[1], now.zoom, now.scrolls
                )
                .unwrap();
            }
            let diagram = &rt.state.diagram;
            assert_eq!(diagram.status(), RenderStatus::Pele, "{name} at {width}");
            let zoom = f64::from(diagram.zoom().expect("a native view"));
            for (key, (text, size)) in text_runs(&diagram.scene().expect("a native scene")) {
                let drawn = size * zoom;
                note(&mut least, drawn, &text, width);
                if at_own_size.get(&key).is_some_and(|(_, own)| *own >= PLAIN) {
                    note(&mut least_plain, drawn, &text, width);
                    if drawn < SMALLEST - 1e-6 {
                        under
                            .push(format!("{name} at {width}: {text:?} drawn at {drawn:.2}, {size} at zoom {zoom:.4}"));
                    }
                }
            }
        }
        let scrolls = scrolls.map_or("never".to_owned(), |width| format!("{width}"));
        writeln!(report, "{name:<14} {scrolls:<13} {:<34} {}", shown(&least), shown(&least_plain)).unwrap();
    }
    report.push_str(&traced);
    writeln!(report, "{} runs of plain text under {SMALLEST} points", under.len()).unwrap();
    for line in &under {
        writeln!(report, "  {line}").unwrap();
    }
    print!("{report}");
    write_report("fit-text.txt", &report);
    assert!(under.is_empty(), "plain text under {SMALLEST} points: {under:#?}");
}

/// What a fitted view drew in a box: the size it was laid out at, its zoom, and whether it is
/// wider than its frame, so that it scrolls.
#[derive(Clone, Copy, Debug)]
struct Fitted {
    laid: [f64; 2],
    zoom: f32,
    scrolls: bool,
}

fn fitted(rt: &Runtime<Boxed>) -> Fitted {
    let keyed: Vec<[f32; 4]> =
        rt.elements().into_iter().filter(|e| e.key.as_deref() == Some("diagram")).map(|e| e.rect).collect();
    let [frame, drawn] = keyed[..] else { panic!("a frame and its drawing: {keyed:?}") };
    let size = rt.state.diagram.scene().expect("a native scene").size();
    Fitted {
        laid: [size.width, size.height],
        zoom: rt.state.diagram.zoom().expect("a native view"),
        scrolls: drawn[2] > frame[2] + 0.5,
    }
}

/// The widest of the steps a fit lays out at, whole points a twelfth of an octave apart, that is
/// no wider than `width`.
fn step_below(width: f64) -> f64 {
    (60..140).map(|k| (f64::from(k) / 12.0).exp2().floor()).filter(|step| *step <= width).fold(0.0, f64::max)
}

/// Pele turns a flowchart that runs across to run down only under 640 points. A fitted view
/// turns it wherever across, at the smallest text, would still be wider than the width the view
/// lays it out at, so that it would scroll, and zooms it across where that fits.
#[test]
fn a_fitted_flowchart_that_runs_across_turns_where_across_would_scroll() {
    let source = fixture("across");
    let least = (SMALLEST / PLAIN) as f32;
    let mut rt = runtime(&source, ViewOptions::fit(), 1180.0);
    let across = fitted(&rt);
    assert_eq!(across.zoom, 1.0, "laid out narrower, across, at 1180: {across:?}");
    let mut report = format!("across, laid out narrower at 1180: {:.1}x{:.1}\n", across.laid[0], across.laid[1]);
    for (frame, width) in
        [1000.0f32, 900.0, 860.0, 820.0, 800.0, 780.0, 760.0, 720.0, 680.0, 660.0, 640.0, 600.0].into_iter().enumerate()
    {
        resize(&mut rt, width, frame as f64 + 1.0);
        let now = fitted(&rt);
        let turned = now.laid[0] < across.laid[0] * f64::from(least);
        writeln!(
            report,
            "at {width}: laid out {:.1}x{:.1}, zoom {:.4}, {}, scrolls {}",
            now.laid[0],
            now.laid[1],
            now.zoom,
            if turned { "turned" } else { "across" },
            now.scrolls
        )
        .unwrap();
        assert!(!now.scrolls, "at {width}: {now:?}");
        assert!(now.zoom >= least - 1e-4, "at {width}: {now:?}");
        // Across at the smallest text would scroll: it turns. Across fits the step the fit lays
        // out at: it stays across. Between the two, the fit decides at that step.
        let at_least = across.laid[0] * f64::from(least);
        if at_least > f64::from(width) {
            assert!(turned, "at {width}: {now:?}");
        }
        if at_least <= step_below(f64::from(width)) {
            assert!(!turned, "at {width}: {now:?}");
        }
    }
    // Outside a fit, Pele's breakpoint decides, as in Pele: across at 700, down at 600.
    for (width, turns) in [(700.0f32, false), (600.0, true)] {
        let rt = runtime(&source, ViewOptions::new(width), 1200.0);
        let size = rt.state.diagram.scene().expect("a native scene").size();
        writeln!(report, "ViewOptions::new({width}): laid out {:.1}x{:.1}", size.width, size.height).unwrap();
        assert_eq!(size.width < across.laid[0] * f64::from(least), turns, "ViewOptions::new({width})");
    }
    print!("{report}");
    write_report("fit-turn.txt", &report);
}
