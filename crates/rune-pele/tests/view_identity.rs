//! What a view is in its window, through rune-pele's public API: a `Diagram` that replaces
//! another under the same id draws and links its own source.

use rune::{Cx, El, Env, Runtime, Tx, el};
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
