//! An app that takes Rune as `rune_pele::rune`, with no `rune` of its own: its view is written
//! against that Rune, and the view rune-pele gives fits in it.

use rune_pele::capture::Capturer;
use rune_pele::rune::{Color, Cx, El, Env, el, text};
use rune_pele::{Diagram, FontCatalog, RenderStatus, ViewOptions};

struct App {
    diagram: Diagram,
}

fn view(app: &App, cx: &mut Cx<App>) -> El {
    let source = "flowchart LR\n  A[Model writes] --> B[rune-pele draws]\n";
    let drawn = app.diagram.view(cx, source, ViewOptions::fit()).unwrap_or_else(|error| text(error.to_string()));
    el().w_full().fill(Color::WHITE).child(drawn)
}

#[test]
fn an_app_on_rune_pele_s_rune_draws_its_diagram_natively() {
    let app = App { diagram: Diagram::new("diagram", FontCatalog::rune()) };
    let capturer = Capturer::new();
    let mut window = capturer.window(app, view, (400.0, 300.0), Env::new(false, 1.0));
    let pixels = window.capture();
    let diagram = &window.state().diagram;
    assert_eq!(diagram.status(), RenderStatus::Pele, "{:?}", diagram.error());
    let page = pixels.at(0, 0);
    let ink = pixels.rgba.chunks(4).filter(|pixel| *pixel != page).count();
    assert!(ink > 1_000, "the drawing shows: {ink} pixels differ from the page");
}
