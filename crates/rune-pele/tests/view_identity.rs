//! What a view is in its window, through rune-pele's public API: a `Diagram` that replaces
//! another under the same id draws and links its own source, and a fitted view reads its own
//! frame's width whatever its neighbours are keyed.

use rune::{Cx, El, Env, Runtime, Tx, col, el};
use rune_pele::{Diagram, Event, FontCatalog, RenderStatus, ViewOptions};

struct Linked {
    diagram: Diagram,
    source: String,
    followed: Vec<String>,
}

fn followed(state: &mut Linked, event: Event, _: &mut Tx) {
    if let Event::Activate(link) = event {
        state.followed.push(link.href);
    }
}

fn linked_view(state: &Linked, cx: &mut Cx<Linked>) -> El {
    let drawn = state.diagram.view_with(cx, &state.source, ViewOptions::natural(), followed).expect("it draws");
    el().size(800.0, 500.0).child(drawn)
}

fn texts(rt: &Runtime<Linked>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|element| element.text).collect()
}

/// Presses the one link the window shows, and returns every address the app has been told.
fn press_the_link(rt: &mut Runtime<Linked>, at: f64) -> Vec<String> {
    let links: Vec<[f32; 4]> =
        rt.elements().into_iter().filter(|element| element.role == "link").map(|element| element.rect).collect();
    let [[x, y, w, h]] = links.as_slice() else { panic!("one link, not {links:?}") };
    let (x, y) = (x + w / 2.0, y + h / 2.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(at);
    rt.state.followed.clone()
}

const ORIGINAL: &str = "flowchart LR\n  A[Original] --> B[Review]\n  click A href \"https://example.org/original\"";
const REPLACEMENT: &str =
    "flowchart LR\n  X[Replacement] --> Y[Done]\n  click X href \"https://example.org/replacement\"";

#[test]
fn a_diagram_that_replaces_another_under_its_id_draws_and_links_its_own_source() {
    let state = Linked {
        diagram: Diagram::new("diagram", FontCatalog::rune()),
        source: ORIGINAL.to_owned(),
        followed: Vec::new(),
    };
    let mut rt = Runtime::new(state, linked_view, (1000.0, 600.0), Env::new(false, 1.0));
    rt.frame(0.0);
    assert_eq!(texts(&rt), ["Original", "Review"]);
    assert_eq!(press_the_link(&mut rt, 0.5), ["https://example.org/original"]);

    rt.update(|state, _| {
        state.diagram = Diagram::new("diagram", FontCatalog::rune());
        state.source = REPLACEMENT.to_owned();
    });
    rt.frame(1.0);
    assert_eq!(rt.state.diagram.status(), RenderStatus::Pele);
    assert_eq!(texts(&rt), ["Replacement", "Done"], "the replacement's own drawing");
    assert_eq!(
        press_the_link(&mut rt, 1.5),
        ["https://example.org/original", "https://example.org/replacement"],
        "the replacement's own link"
    );

    // A fresh Diagram drawing the same source under the same id draws it as the one before.
    rt.update(|state, _| state.diagram = Diagram::new("diagram", FontCatalog::rune()));
    rt.frame(2.0);
    assert_eq!(texts(&rt), ["Replacement", "Done"]);
    assert_eq!(press_the_link(&mut rt, 2.5)[2..], ["https://example.org/replacement"]);
}

struct Pair {
    fitted: Diagram,
    natural: Diagram,
    natural_first: bool,
}

const ACROSS: &str = "flowchart LR\n A[Build and deploy] --> B[Confirm metrics] --> C[Review report]";

fn pair_view(pair: &Pair, cx: &mut Cx<Pair>) -> El {
    let fitted = pair.fitted.view(cx, ACROSS, ViewOptions::fit()).expect("it draws");
    let natural = pair.natural.view(cx, "flowchart LR\n Z[Ready]", ViewOptions::natural()).expect("it draws");
    let children = if pair.natural_first { [natural, fitted] } else { [fitted, natural] };
    col().w(500.0).children(children)
}

/// A neighbour whose id is the fitted view's with `~fit` after it cannot lend the fitted view
/// its width: in either order the fitted diagram, which is narrower than the column, is drawn at
/// its own size.
#[test]
fn a_fitted_view_reads_its_own_frame_whatever_its_neighbour_is_keyed() {
    let mut zooms = Vec::new();
    for natural_first in [false, true] {
        let pair = Pair {
            fitted: Diagram::new("network", FontCatalog::rune()),
            natural: Diagram::new("network~fit", FontCatalog::rune()),
            natural_first,
        };
        let mut rt = Runtime::new(pair, pair_view, (500.0, 400.0), Env::new(false, 1.0));
        rt.frame(1.0);
        let width = rt.state.fitted.scene().expect("a native scene").size().width;
        assert!(width < 500.0, "the fitted diagram is {width} wide, under the column's 500");
        zooms.push(rt.state.fitted.zoom().expect("a native view"));
    }
    assert_eq!(zooms, [1.0, 1.0], "with the fitted view first and then second");
}
