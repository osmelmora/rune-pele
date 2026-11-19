//! rune-pele built without its `fallback` feature, as an app that turns its default features off
//! has it: a source the native view does not draw, which rune-mermaid would draw, is an error, or
//! after a source that drew keeps that drawing.

#![cfg(not(feature = "fallback"))]

use std::cell::RefCell;

use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Window};
use rune_pele::lower::Unsupported;
use rune_pele::{Diagram, Engine, FontCatalog, RenderStatus, ViewError, ViewOptions};

const DRAWN: &str = "flowchart LR\n  A[Plain] --> B[Struck]\n";

/// The same flowchart with a line through B's text, which the lowering does not draw.
fn struck() -> String {
    format!("{DRAWN}  style B text-decoration:line-through\n")
}

fn unlowered() -> ViewError {
    ViewError::Geometry(Unsupported::Style("text-decoration:line-through".to_owned()))
}

struct Viewer {
    diagram: Diagram,
    source: String,
    /// What the last view returned in place of a drawing.
    error: RefCell<Option<ViewError>>,
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let page = el().fill(Color::WHITE);
    let drawn = s.diagram.view(cx, &s.source, ViewOptions::new(600.0));
    *s.error.borrow_mut() = drawn.as_ref().err().cloned();
    match drawn {
        Ok(drawn) => page.child(drawn),
        Err(_) => page,
    }
}

fn window<'a>(capturer: &'a Capturer, source: &str) -> Window<'a, Viewer> {
    let viewer = Viewer {
        diagram: Diagram::new("diagram", FontCatalog::rune()),
        source: source.to_owned(),
        error: RefCell::new(None),
    };
    let mut window = capturer.window(viewer, view, (600.0, 300.0), Env::new(false, 1.0));
    window.capture();
    window
}

#[test]
fn a_source_the_native_view_does_not_draw_is_an_error_with_nothing_drawn_before_it() {
    let capturer = Capturer::new();
    let window = window(&capturer, &struck());
    let diagram = &window.state().diagram;
    assert_eq!(window.state().error.borrow().clone(), Some(unlowered()), "the view is the error");
    assert_eq!(diagram.error(), Some(unlowered()));
    assert_eq!((diagram.status(), diagram.engine(), diagram.fallback()), (RenderStatus::Unrendered, None, None));
}

#[test]
fn a_source_the_native_view_does_not_draw_keeps_the_drawing_before_it() {
    let capturer = Capturer::new();
    let mut window = window(&capturer, DRAWN);
    assert_eq!(window.state().diagram.status(), RenderStatus::Pele, "{:?}", window.state().diagram.error());
    window.update(|viewer| viewer.source = struck());
    window.capture();
    let diagram = &window.state().diagram;
    assert_eq!(window.state().error.borrow().clone(), None, "the view is the kept drawing");
    assert_eq!(diagram.error(), Some(unlowered()), "and says why the source is not drawn");
    assert_eq!(
        (diagram.status(), diagram.engine(), diagram.fallback()),
        (RenderStatus::Kept, Some(Engine::Pele), None)
    );
}
