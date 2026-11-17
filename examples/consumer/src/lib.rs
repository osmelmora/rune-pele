//! A Rune app that draws one Mermaid diagram with rune-pele, fitted to its window. `cargo run`
//! opens the window, and the tests draw the same view headless.

use rune::{Cx, El, Tx, col, text, tok};
use rune_pele::{Diagram, Event, FontCatalog, Link, ViewOptions};

pub const SOURCE: &str = "flowchart LR
  A[Model writes] --> B[Chat streams] --> C[rune-pele draws] --> D[Reader follows]
  click D href \"https://example.org/guide\" \"Open the guide\"
";

pub struct App {
    pub diagram: Diagram,
    pub source: String,
    /// The last link pressed.
    pub followed: Option<Link>,
}

pub fn init() -> App {
    App { diagram: Diagram::new("diagram", FontCatalog::rune()), source: SOURCE.to_owned(), followed: None }
}

fn on_event(app: &mut App, event: Event, _: &mut Tx) {
    if let Event::Activate(link) = event {
        app.followed = Some(link);
    }
}

pub fn view(app: &App, cx: &mut Cx<App>) -> El {
    let t = tok::theme(cx.system_dark());
    let diagram = app
        .diagram
        .view_with(cx, &app.source, ViewOptions::fit(), on_event)
        .unwrap_or_else(|error| text(error.to_string()).color(t.text));
    col().w_full().h_full().fill(t.bg).padding(16.0).child(diagram)
}
