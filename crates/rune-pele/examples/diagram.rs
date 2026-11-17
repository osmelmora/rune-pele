//! Any Mermaid source drawn by rune-pele, fitted to its window: natively where its type is drawn
//! from the Scene, and by rune-mermaid otherwise. The line above the diagram says which, the
//! width a native diagram is laid out at and the zoom it is shown at, and names the last link
//! pressed.
//!
//! `cargo run -p rune-pele --example diagram -- tests/fixtures/rune/across.mmd` opens the window;
//! narrow it to watch the diagram shrink, then turn to run down where across would scroll, then
//! shrink again, then scroll sideways. Add `--snap out.png`
//! to render it headless, with `--theme light` or `--theme dark` and `--size 900x700`.
//!
//! `--stream` types the file in, three characters a frame at 60 frames a second, as a model
//! streaming it would. While what has arrived does not draw, the view keeps the last drawing
//! that did, fitted to the window as it would be drawn now: dimmed, with the error over its
//! bottom edge, as Pele's `mount` shows it, or as it was with `--kept-unchanged`. The stream
//! runs on the app's clock, so `--snap` with `--at <seconds>` draws the same frame every time.

use std::time::Duration;

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, ScrollSpec, Tx, col, text};
use rune_pele::{Diagram, Event, Fallback, FontCatalog, KeptLook, RenderStatus, ViewOptions};

const SIZE: (f32, f32) = (900.0, 700.0);

/// How fast `--stream` types the file in.
const CHARACTERS_PER_SECOND: f64 = 180.0;
const FRAME: Duration = Duration::from_nanos(16_666_667);

struct State {
    diagram: Diagram,
    source: String,
    /// The bytes of `source` that have arrived: all of it unless it streams in.
    arrived: usize,
    streaming: bool,
    look: KeptLook,
    followed: Option<String>,
}

fn init() -> State {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.iter().find(|arg| arg.ends_with(".mmd")).unwrap_or_else(|| {
        eprintln!(
            "usage: diagram <file.mmd> [--stream] [--kept-unchanged] [--snap <png> [--at <seconds>]] [--theme light|dark] [--size WxH]"
        );
        std::process::exit(64);
    });
    let source = std::fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("{path}: {error}");
        std::process::exit(66);
    });
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let streaming = flag("--stream");
    let look = if flag("--kept-unchanged") { KeptLook::Unchanged } else { KeptLook::DimmedWithError };
    let arrived = if streaming { 0 } else { source.len() };
    State { diagram: Diagram::new("diagram", FontCatalog::rune()), source, arrived, streaming, look, followed: None }
}

/// Lets in what has arrived by `seconds` since the start.
fn poll(state: &mut State, seconds: f64, tx: &mut Tx) -> bool {
    if !state.streaming {
        return false;
    }
    let characters = (seconds * CHARACTERS_PER_SECOND) as usize;
    let arrived = state.source.char_indices().nth(characters).map_or(state.source.len(), |(at, _)| at);
    if arrived < state.source.len() {
        tx.wake_after(FRAME);
    }
    let changed = arrived != state.arrived;
    state.arrived = arrived;
    changed
}

fn on_event(state: &mut State, event: Event, _: &mut Tx) {
    if let Event::Activate(link) = event {
        state.followed = Some(link.href);
    }
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let options = ViewOptions::fit().kept_look(s.look);
    let drawn = s.diagram.view_with(cx, &s.source[..s.arrived], options, on_event);
    let size = match (s.diagram.scene(), s.diagram.zoom()) {
        (Some(scene), Some(zoom)) => {
            format!(", laid out {:.0} points wide, shown at {:.0}%", scene.size().width, zoom * 100.0)
        }
        _ => String::new(),
    };
    let by = match s.diagram.status() {
        RenderStatus::Pele => format!("Drawn by rune-pele{size}"),
        RenderStatus::RuneMermaid => match s.diagram.fallback() {
            Some(Fallback::NotLowered(why)) => format!("Drawn by rune-mermaid: {why}"),
            _ => "Drawn by rune-mermaid: the type is not ported".to_owned(),
        },
        RenderStatus::Kept => {
            let error = s.diagram.error().map(|error| error.to_string()).unwrap_or_default();
            format!("The last drawing that drew, kept{size}: {}", error.lines().next().unwrap_or_default())
        }
        RenderStatus::Unrendered => "Not drawn".to_owned(),
    };
    let by = match &s.followed {
        Some(href) => format!("{by}. Last link: {href}"),
        None => by,
    };
    let by = if s.streaming {
        let (arrived, all) = (s.source[..s.arrived].chars().count(), s.source.chars().count());
        format!("{arrived} of {all} characters. {by}")
    } else {
        by
    };
    let diagram = drawn.unwrap_or_else(|error| text(error.to_string()).color(t.text));
    let status = text(by).key("status").font(type_::BODY).color(t.text_secondary).wrap();
    col()
        .w_full()
        .h_full()
        .fill(t.bg)
        .padding(20.0)
        .gap(12.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(status)
        .child(diagram)
}

fn main() {
    rune::run(AppSpec { poll: Some(poll), ..AppSpec::<State>::new("Diagram", SIZE, init, view) });
}
