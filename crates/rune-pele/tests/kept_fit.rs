//! A kept drawing in a fitted view: while a source fails after one that drew, and the width the
//! view has goes from 900 points to 300, each frame shows the last good source drawn as a view
//! of it draws it at that width, in either look, from one frame whose width the fit reads.

mod common;

use std::cell::RefCell;
use std::fmt::Write as _;

use common::{in_a_row, out_dir_for};
use pele_core::ParseOptions;
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels, Window};
use rune_pele::{Diagram, FontCatalog, KeptLook, RenderStatus, ViewError, ViewOptions, ViewStats};

const RUN: &str = "kept-fit";
const WINDOW: (f32, f32) = (900.0, 700.0);
/// The key of the frame a fitted view reads its width from: the diagram's id, `~fit`.
const FRAME: &str = "diagram~fit";

struct Viewer {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
    /// The width of the box the view is put in, in points.
    width: f32,
    error: RefCell<Option<ViewError>>,
}

/// The view in a box `width` points wide, on a white page that fills the window.
fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let page = el().fill(Color::WHITE);
    let within = el().size(s.width, WINDOW.1);
    match s.diagram.view(cx, &s.source, s.options.clone()) {
        Ok(drawn) => {
            s.error.replace(None);
            page.child(within.child(drawn))
        }
        Err(error) => {
            s.error.replace(Some(error));
            page.child(within)
        }
    }
}

fn window<'a>(capturer: &'a Capturer, source: &str, look: KeptLook, width: f32) -> Window<'a, Viewer> {
    let viewer = Viewer {
        diagram: Diagram::new("diagram", FontCatalog::rune()),
        source: source.to_owned(),
        options: ViewOptions::fit().kept_look(look),
        width,
        error: RefCell::new(None),
    };
    capturer.window(viewer, view, WINDOW, Env::new(false, 1.0))
}

fn fixture(name: &str) -> String {
    let path = pele_oracle::root().join(format!("tests/fixtures/rune/{name}.mmd"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// What a view showed in a box `width` points wide.
struct Shown {
    pixels: Pixels,
    status: RenderStatus,
    error: Option<ViewError>,
    /// The rect of the frame the fit reads its width from.
    frame: [f32; 4],
    zoom: f32,
    /// The size the shown diagram was laid out at.
    laid: [f64; 2],
    stats: ViewStats,
}

fn shown(window: &mut Window<'_, Viewer>, width: f32) -> Shown {
    window.update(|viewer| viewer.width = width);
    let pixels = window.capture();
    let elements = window.runtime().elements();
    let frame = elements.iter().find(|e| e.key.as_deref() == Some(FRAME)).expect("the fit's frame").rect;
    let diagram = &window.state().diagram;
    let size = diagram.scene().expect("a native drawing").size();
    Shown {
        pixels,
        status: diagram.status(),
        error: diagram.error(),
        frame,
        zoom: diagram.zoom().expect("a native drawing"),
        laid: [size.width, size.height],
        stats: diagram.stats(),
    }
}

/// How many pixels of `a` and `b` differ.
fn changed(a: &Pixels, b: &Pixels) -> usize {
    a.rgba.chunks(4).zip(b.rgba.chunks(4)).filter(|(p, q)| p != q).count()
}

#[test]
fn a_kept_drawing_follows_the_fit_as_the_last_good_source_draws_at_each_width() {
    let capturer = Capturer::new();
    let good = fixture("across");
    let failing = format!("{}\n    Z -->", good.trim_end());
    assert!(pele_core::prepare(&failing, ParseOptions::default()).is_err(), "the failing source does not parse");

    let start = WINDOW.0;
    let mut unchanged = window(&capturer, &good, KeptLook::Unchanged, start);
    let mut dimmed = window(&capturer, &good, KeptLook::DimmedWithError, start);
    // A view of the last good source that never fails, resized alongside.
    let mut drawing = window(&capturer, &good, KeptLook::Unchanged, start);
    for window in [&mut unchanged, &mut dimmed, &mut drawing] {
        let first = shown(window, start);
        assert_eq!(first.status, RenderStatus::Pele, "the good source draws");
    }
    for window in [&mut unchanged, &mut dimmed] {
        window.update(|viewer| viewer.source = failing.clone());
    }

    let mut report = String::from(
        "The across fixture drew; then a line that does not parse is added, and the box the fitted view is in narrows\n\
         from 900 points to 300, one point a frame. `unchanged` and `dimmed` keep the drawing in each look; `drawing`\n\
         is a view of the good source alone. layouts and lowerings are counted from the frame the source failed.\n\n",
    );
    let at_start = [&mut unchanged, &mut dimmed, &mut drawing].map(|window| shown(window, start).stats);
    let (mut fresh_compared, mut sizes) = (0, 0);
    let mut last_laid = [0.0; 2];
    for width in (300..=900).rev().map(|width| width as f32) {
        let kept = shown(&mut unchanged, width);
        let faded = shown(&mut dimmed, width);
        let good_one = shown(&mut drawing, width);
        for (look, frame) in [("unchanged", &kept), ("dimmed", &faded)] {
            assert_eq!(frame.status, RenderStatus::Kept, "{look} at {width}: the drawing is kept");
            assert!(matches!(frame.error, Some(ViewError::Core(_))), "{look} at {width}: {:?}", frame.error);
            assert_eq!(frame.frame[2], width, "{look} at {width}: the frame the fit reads is as wide as the box");
            assert_eq!(
                (frame.zoom, frame.laid),
                (good_one.zoom, good_one.laid),
                "{look} at {width}: laid out and zoomed as a view of the good source"
            );
        }
        assert_eq!(good_one.frame[2], width, "at {width}: the frame of a view that draws");
        assert!(
            kept.pixels.rgba == good_one.pixels.rgba,
            "at {width}: the kept frame is the good source drawn at this width, {} px apart",
            changed(&kept.pixels, &good_one.pixels)
        );
        let since =
            |stats: ViewStats, before: ViewStats| (stats.layouts - before.layouts, stats.lowerings - before.lowerings);
        let work =
            [since(kept.stats, at_start[0]), since(faded.stats, at_start[1]), since(good_one.stats, at_start[2])];
        assert_eq!(
            work[0], work[2],
            "at {width}: the unchanged look lays out and lowers what a view of the good source does"
        );
        assert_eq!(
            work[1], work[2],
            "at {width}: the dimmed look lays out and lowers what a view of the good source does"
        );
        if (width as u32).is_multiple_of(25) {
            let mut fresh = window(&capturer, &good, KeptLook::Unchanged, width);
            let alone = shown(&mut fresh, width);
            assert_eq!(alone.status, RenderStatus::Pele);
            assert!(
                kept.pixels.rgba == alone.pixels.rgba,
                "at {width}: the kept frame is a fresh view of the good source, {} px apart",
                changed(&kept.pixels, &alone.pixels)
            );
            fresh_compared += 1;
            if (width as u32).is_multiple_of(300) {
                let row = in_a_row(&[&kept.pixels, &faded.pixels, &alone.pixels], 16);
                common::save_in(RUN, &row, &format!("kept-at-{width}.png"));
            }
        }
        if kept.laid != last_laid {
            sizes += 1;
            writeln!(
                report,
                "at {width}: laid out {:.1}x{:.1}, zoom {:.4}; layouts and lowerings so far: unchanged {:?}, dimmed {:?}, drawing {:?}",
                kept.laid[0], kept.laid[1], kept.zoom, work[0], work[1], work[2]
            )
            .unwrap();
            last_laid = kept.laid;
        }
    }
    let [kept, faded, good_one] = [&unchanged, &dimmed, &drawing].map(|window| window.state().diagram.stats());
    writeln!(
        report,
        "\n601 widths: {sizes} sizes laid out; layouts {}, {}, {} and lowerings {}, {}, {} (unchanged, dimmed, drawing) since the \
         source failed; each kept frame pixel-equal to the view of the good source, and {fresh_compared} to a fresh one",
        kept.layouts - at_start[0].layouts,
        faded.layouts - at_start[1].layouts,
        good_one.layouts - at_start[2].layouts,
        kept.lowerings - at_start[0].lowerings,
        faded.lowerings - at_start[1].lowerings,
        good_one.lowerings - at_start[2].lowerings,
    )
    .unwrap();
    print!("{report}");
    std::fs::create_dir_all(out_dir_for(RUN)).unwrap();
    std::fs::write(out_dir_for(RUN).join("resize.txt"), report).unwrap();
}

#[test]
fn sources_that_lay_out_and_then_fail_do_not_make_the_kept_drawing_lay_out_again() {
    let capturer = Capturer::new();
    let good = fixture("across");
    let width = 300.0;
    let mut window = window(&capturer, &good, KeptLook::Unchanged, width);
    let drawn = shown(&mut window, width);
    assert_eq!(drawn.status, RenderStatus::Pele);
    let mut report = format!(
        "The across fixture drew {width} points wide, laid out {:.1} wide at zoom {:.4}: {:?}. Then one source a frame \
         that lays out and fails, as no lowering or rune-mermaid draws it: too wide for the frame, each lays out its own \
         size, the probe at width 1 and a step, three of the four layouts the view keeps.\n",
        drawn.laid[0], drawn.zoom, drawn.stats
    );
    let mut before = drawn.stats;
    for line in 1..=6 {
        let failing = format!(
            "swimlane-beta LR\n  a --> b --> c --> d --> e --> f --> g --> h\n  style a text-decoration:line-through\n  x{line}"
        );
        window.update(|viewer| viewer.source = failing);
        let kept = shown(&mut window, width);
        assert_eq!(kept.status, RenderStatus::Kept, "source {line}");
        assert!(matches!(kept.error, Some(ViewError::Geometry(_))), "source {line}: {:?}", kept.error);
        assert!(kept.pixels.rgba == drawn.pixels.rgba, "source {line}: the kept frame is the last drawing");
        let work = (kept.stats.layouts - before.layouts, kept.stats.lowerings - before.lowerings);
        writeln!(report, "failing source {line}: {:?}, {work:?} more layouts and lowerings", kept.stats).unwrap();
        assert_eq!(work, (3, 1), "source {line}: only the failing source is laid out and lowered");
        assert_eq!(kept.stats.paints, drawn.stats.paints, "source {line}: the kept drawing is not painted again");
        before = kept.stats;
    }
    print!("{report}");
    std::fs::create_dir_all(out_dir_for(RUN)).unwrap();
    std::fs::write(out_dir_for(RUN).join("evicted.txt"), report).unwrap();
}
