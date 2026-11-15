//! Any Mermaid source drawn by rune-pele: natively where its type is drawn from the Scene, and
//! by rune-mermaid otherwise. The line above the diagram says which, and names the last link
//! pressed.
//!
//! `cargo run -p rune-pele --example diagram -- tests/fixtures/flow.mmd` opens the window.
//! Add `--snap out.png` to render it headless, with `--theme light` or `--theme dark` and
//! `--size 900x700`.

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, col, text};
use rune_pele::{Diagram, Event, Fallback, FontCatalog, RenderStatus, ViewOptions};

const SIZE: (f32, f32) = (900.0, 700.0);

struct State {
    diagram: Diagram,
    source: String,
    followed: Option<String>,
}

fn init() -> State {
    let path = std::env::args().skip(1).find(|arg| arg.ends_with(".mmd")).unwrap_or_else(|| {
        eprintln!("usage: diagram <file.mmd> [--snap <png>] [--theme light|dark] [--size WxH]");
        std::process::exit(64);
    });
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        eprintln!("{path}: {error}");
        std::process::exit(66);
    });
    State { diagram: Diagram::new("diagram", FontCatalog::rune()), source, followed: None }
}

fn on_event(state: &mut State, event: Event, _: &mut rune::Tx) {
    if let Event::Activate(link) = event {
        state.followed = Some(link.href);
    }
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let drawn = s.diagram.view_with(cx, &s.source, ViewOptions::new(SIZE.0 - 40.0), on_event);
    let by = match (s.diagram.status(), s.diagram.fallback()) {
        (RenderStatus::Pele, _) => "Drawn by rune-pele".to_owned(),
        (_, Some(Fallback::NotLowered(why))) => format!("Drawn by rune-mermaid: {why}"),
        (RenderStatus::RuneMermaid, _) => "Drawn by rune-mermaid: the type is not ported".to_owned(),
        (RenderStatus::Unrendered, _) => "Not drawn".to_owned(),
    };
    let by = match &s.followed {
        Some(href) => format!("{by}. Last link: {href}"),
        None => by,
    };
    let diagram = drawn.unwrap_or_else(|error| text(error.to_string()).color(t.text));
    let status = text(by).key("status").font(type_::BODY).color(t.text_secondary);
    col().w_full().h_full().fill(t.bg).padding(20.0).gap(12.0).child(status).child(diagram)
}

fn main() {
    rune::run(AppSpec::<State>::new("Diagram", SIZE, init, view));
}
