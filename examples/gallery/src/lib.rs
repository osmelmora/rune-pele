//! rune-pele's gallery: every diagram type Pele 1.13.0 draws, drawn natively in Rune's
//! Playground, each with controls for its zoom and its light or dark beside it, and a live editor
//! that draws Mermaid as it is typed and keeps the last drawing that drew while the source fails.
//!
//! `mise run gallery` opens the window on the Playground. `mise run gallery -- --playground
//! "Sequence" --snap out.png` renders that story's stage headless, `--whole-window` the window
//! with the Playground's list and side column, and `--theme light` in light mode, as headless
//! renders are dark otherwise. `mise run gallery-web` builds the same gallery for the web.

use rune::tok::{self, Theme, type_};
use rune::{
    App, AppSpec, Color, Cx, El, Key, KeyEvent, Play, Playground, Stage, Story, TextEdit, Tx, col, el, row, text,
    text_input,
};
use rune_pele::{Diagram, Event, Fallback, FontCatalog, Icon, IconPaint, Palette, RenderStatus, ViewOptions};

#[cfg(test)]
mod tests;

/// A story that draws one file of the repository's fixtures.
struct Fixture {
    /// Its `"Group/Name"` path in the Playground's list.
    path: &'static str,
    /// The file, under `tests/fixtures`.
    file: &'static str,
    source: &'static str,
}

macro_rules! fixture {
    ($path:literal, $file:literal) => {
        Fixture { path: $path, file: $file, source: include_str!(concat!("../../../tests/fixtures/", $file)) }
    };
}

/// A story for every file of `tests/fixtures/rune`: one of each type there, then one flowchart
/// wider than most windows and one that falls back to rune-mermaid. Pie, which has none there,
/// draws the pie example's.
static FIXTURES: [Fixture; 35] = [
    fixture!("Graphs/Flowchart", "rune/flowchart.mmd"),
    fixture!("Graphs/State", "rune/state.mmd"),
    fixture!("Graphs/Class", "rune/class.mmd"),
    fixture!("Graphs/Entity relationship", "rune/er.mmd"),
    fixture!("Graphs/Requirement", "rune/requirement.mmd"),
    fixture!("Graphs/Use case", "rune/usecase.mmd"),
    fixture!("Graphs/Block", "rune/block.mmd"),
    fixture!("Graphs/C4", "rune/c4.mmd"),
    fixture!("Graphs/Architecture", "rune/architecture.mmd"),
    fixture!("Graphs/Agent flow", "rune/agentflow.mmd"),
    fixture!("Graphs/Swimlane", "rune/swimlane.mmd"),
    fixture!("Time and process/Sequence", "rune/sequence.mmd"),
    fixture!("Time and process/Gantt", "rune/gantt.mmd"),
    fixture!("Time and process/Timeline", "rune/timeline.mmd"),
    fixture!("Time and process/Journey", "rune/journey.mmd"),
    fixture!("Time and process/Git", "rune/git.mmd"),
    fixture!("Time and process/Event modeling", "rune/eventmodeling.mmd"),
    fixture!("Time and process/Kanban", "rune/kanban.mmd"),
    fixture!("Charts/Pie", "pie-pets.mmd"),
    fixture!("Charts/XY chart", "rune/xychart.mmd"),
    fixture!("Charts/Quadrant", "rune/quadrant.mmd"),
    fixture!("Charts/Radar", "rune/radar.mmd"),
    fixture!("Charts/Sankey", "rune/sankey.mmd"),
    fixture!("Charts/Treemap", "rune/treemap.mmd"),
    fixture!("Charts/Venn", "rune/venn.mmd"),
    fixture!("Charts/Packet", "rune/packet.mmd"),
    fixture!("Maps and trees/Mindmap", "rune/mindmap.mmd"),
    fixture!("Maps and trees/Tree view", "rune/treeview.mmd"),
    fixture!("Maps and trees/Ishikawa", "rune/ishikawa.mmd"),
    fixture!("Maps and trees/Cynefin", "rune/cynefin.mmd"),
    fixture!("Maps and trees/Wardley", "rune/wardley.mmd"),
    fixture!("Notation/Railroad", "rune/railroad.mmd"),
    fixture!("Notation/Info", "rune/info.mmd"),
    fixture!("Behaviour/Fits the width", "rune/across.mmd"),
    fixture!("Behaviour/Falls back to rune-mermaid", "rune/unlowered.mmd"),
];

/// The live editor's story.
const EDITOR: &str = "Try it/Live editor";

/// The groups in the order the list shows them.
const GROUPS: &[&str] = &["Try it", "Graphs", "Time and process", "Charts", "Maps and trees", "Notation", "Behaviour"];

/// What the live editor starts with.
const STARTER: &str = "flowchart LR
    A[Type Mermaid here] --> B{Does it parse?}
    B -->|yes| C[Drawn as you type]
    B -->|no| D[The last drawing stays]
";

/// What the editor's B adds: an edge with no end, which does not parse.
const BROKEN_LINE: &str = "    D -->";

/// What the editor's S types in, a few characters a frame, as a model streaming it would.
const STREAMED: &str = include_str!("../../../tests/fixtures/rune/sequence.mmd");
/// How many bytes arrive at a time, and how often.
const CHUNK: usize = 6;
const CHUNK_EVERY: f64 = 1.0 / 30.0;

/// The zooms the side column steps through, on the size a fit draws the diagram at.
const ZOOMS: [f32; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0];
const ZOOM_AT_START: usize = 2;

/// Whose light or dark a story is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Appearance {
    /// The stage's: the Playground's sun and moon button sets it, and `--theme` headless.
    #[default]
    Stage,
    Light,
    Dark,
}

impl Appearance {
    const ALL: [Appearance; 3] = [Appearance::Stage, Appearance::Light, Appearance::Dark];

    fn label(self) -> &'static str {
        match self {
            Appearance::Stage => "Stage",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }

    fn is_dark(self, stage: bool) -> bool {
        match self {
            Appearance::Stage => stage,
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }
}

/// A control of the side column: ↑ and ↓ pick one while the column has the keyboard, and ← and
/// → step it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Control {
    #[default]
    Zoom,
    Appearance,
}

/// How a story draws its diagram, as its side column sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Look {
    /// An index into [`ZOOMS`].
    zoom: usize,
    appearance: Appearance,
    /// The control the keyboard steps.
    at: Control,
}

impl Default for Look {
    fn default() -> Self {
        Look { zoom: ZOOM_AT_START, appearance: Appearance::default(), at: Control::default() }
    }
}

impl Look {
    fn zoom(&self) -> f32 {
        ZOOMS[self.zoom]
    }

    /// `control` a step one way or the other, as far as it goes.
    fn step(&mut self, control: Control, by: isize) {
        let stepped = |at: usize, len: usize| at.saturating_add_signed(by).min(len - 1);
        match control {
            Control::Zoom => self.zoom = stepped(self.zoom, ZOOMS.len()),
            Control::Appearance => {
                let at = Appearance::ALL.iter().position(|&appearance| appearance == self.appearance).unwrap_or(0);
                self.appearance = Appearance::ALL[stepped(at, Appearance::ALL.len())];
            }
        }
    }
}

/// A story's own state: the diagram it draws, what it draws it from, a fixture's source or the
/// editor's text, and how.
struct Shown<T> {
    diagram: Diagram,
    source: T,
    look: Look,
    /// Where the link last pressed in the drawing leads.
    followed: Option<String>,
}

impl<T> Shown<T> {
    fn new(source: T) -> Self {
        // One story shows at a time, so every story's diagram can have the same key.
        Shown { diagram: Diagram::new("diagram", FontCatalog::rune()), source, look: Look::default(), followed: None }
    }
}

/// The window's own state. The gallery is its Playground, which opens on the first frame.
#[derive(Default)]
struct Home {
    opened: bool,
}

/// Opens the gallery: in a window natively, and in the page on the web.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn start() {
    App::new(spec()).playground(playground()).run();
}

fn spec() -> AppSpec<Home> {
    AppSpec { poll: Some(open_once), ..AppSpec::new("rune-pele gallery", (1280.0, 820.0), Home::default, home) }
}

fn playground() -> Playground<Home> {
    let fixtures = FIXTURES.iter().fold(Playground::<Home>::new(), |playground, fixture| {
        playground.story(
            Story::new(fixture.path, || Shown::new(fixture), fixture_view)
                .stage(Stage::Scroll)
                .side_keys(controls, control_key),
        )
    });
    fixtures
        .story(
            Story::new(EDITOR, || Shown::new(TextEdit::new(STARTER)), editor_view)
                .stage(Stage::Scroll)
                .side_keys(controls, control_key)
                .trigger('b', "Break the source", |s, _| {
                    let broken = format!("{}\n{BROKEN_LINE}\n", s.source.text.trim_end());
                    s.source.replace_text(broken);
                })
                .trigger('s', "Stream a sequence diagram in", stream),
        )
        .group_order(GROUPS)
        .sizes(&[("Phone", (390.0, 844.0)), ("Laptop", (1280.0, 800.0))])
        // On in release builds and on the web too, where the gallery is the whole app.
        .enabled(true)
}

/// The window behind the Playground, after Back.
fn home(_: &Home, cx: &mut Cx<Home>) -> El {
    let t = tok::theme(cx.system_dark());
    let open = el()
        .key("open")
        .role("button")
        .label("Open the gallery")
        .focusable(true)
        .padding(10.0)
        .radius(8.0)
        .fill(t.card)
        .hover(t.hover)
        .on_press_tx(|_: &mut Home, tx: &mut Tx| tx.open_playground())
        .child(text("Open the gallery").font(type_::BODY).color(t.text));
    col()
        .w_full()
        .h_full()
        .center()
        .gap(12.0)
        .fill(t.bg)
        .child(text("rune-pele: Mermaid drawn natively in Rune").font(type_::TITLE).color(t.text))
        .child(open)
        .child(text("⌘⇧D, or Ctrl+Shift+D, opens it too").font(type_::LABEL).color(t.text_secondary))
}

fn open_once(home: &mut Home, _: f64, tx: &mut Tx) -> bool {
    if home.opened {
        return false;
    }
    home.opened = true;
    tx.open_playground();
    true
}

/// A fixture's drawing, under the name of its file.
fn fixture_view(s: &Shown<&'static Fixture>, cx: &mut Cx<Shown<&'static Fixture>>) -> El {
    let dark = s.look.appearance.is_dark(cx.system_dark());
    let file = text(format!("tests/fixtures/{}", s.source.file))
        .key("file")
        .font(type_::LABEL.mono())
        .color(tok::theme(dark).text_secondary);
    page(s, s.source.source, dark, Some(file), cx)
}

/// The editor: the field the source is typed in, over its drawing.
fn editor_view(s: &Shown<TextEdit>, cx: &mut Cx<Shown<TextEdit>>) -> El {
    let dark = s.look.appearance.is_dark(cx.system_dark());
    let t = tok::theme(dark);
    let field = text_input(&s.source, type_::BODY.mono())
        .key("source")
        .label("Mermaid source")
        .placeholder("Type Mermaid here", t.text_secondary)
        .color(t.text)
        .caret(t.text, tok::accent(0, dark).alpha(0.3))
        .w_full()
        .padding(12.0)
        .radius(8.0)
        .fill(t.card)
        .outline(1.0, 0.0, t.card_border)
        .on_edit(|s: &mut Shown<TextEdit>| &mut s.source)
        // Return starts a line, as in any editor, where a field's Return would submit.
        .on_submit(|s: &mut Shown<TextEdit>, _: &mut Tx| s.source.insert("\n"));
    page(s, &s.source.text, dark, Some(field), cx)
}

/// `source` drawn in the story's look, fitted to the stage, under a line that says who drew it,
/// or why the source does not draw: `above`, the fixture's file or the editor's field, goes over
/// both.
fn page<T: 'static>(s: &Shown<T>, source: &str, dark: bool, above: Option<El>, cx: &mut Cx<Shown<T>>) -> El {
    let t = tok::theme(dark);
    let options = ViewOptions::fit().zoom(s.look.zoom()).palette(Palette::from_tokens(&t)).icons(icons);
    // An error is drawn as nothing: the status line says why.
    let drawing = s.diagram.view_with(cx, source, options, follow::<T>).unwrap_or_else(|_| el());
    let (word, colour, detail) = status(&s.diagram, dark);
    let detail = match &s.followed {
        Some(href) => format!("{detail}. Link pressed: {href}"),
        None => detail,
    };
    let chip = row()
        .key("status-chip")
        .h(20.0)
        .px(8.0)
        .center()
        .radius(10.0)
        .fill(colour.alpha(0.18))
        .child(text(word).key("status-word").font(type_::LABEL).color(colour));
    let line = row()
        .key("status")
        .w_full()
        .gap(8.0)
        .child(chip)
        .child(text(detail).key("status-detail").font(type_::BODY).color(t.text_secondary).wrap().shrink());
    col().w_full().padding(20.0).gap(12.0).radius(12.0).fill(t.bg).children(above).child(line).child(drawing)
}

/// The status line's word, its colour, and what follows it, for the last view of `diagram`.
fn status(diagram: &Diagram, dark: bool) -> (&'static str, Color, String) {
    let error = || diagram.error().map(|error| error.to_string()).unwrap_or_default();
    match diagram.status() {
        RenderStatus::Pele => {
            let zoom = diagram.zoom().unwrap_or(1.0);
            ("Drawn", tok::accent(4, dark), format!("natively by rune-pele, at {:.0}% of its size", zoom * 100.0))
        }
        RenderStatus::RuneMermaid => {
            let why = match diagram.fallback() {
                Some(Fallback::NotLowered(why)) => why.to_string(),
                _ => "its type is not ported".to_owned(),
            };
            ("Fallback", tok::accent(1, dark), format!("drawn by rune-mermaid: {why}"))
        }
        RenderStatus::Kept => {
            ("Kept", tok::accent(3, dark), format!("the last drawing that drew, while the source fails: {}", error()))
        }
        _ => ("Error", tok::accent(2, dark), error()),
    }
}

fn follow<T>(s: &mut Shown<T>, event: Event, _: &mut Tx) {
    if let Event::Activate(link) = event {
        s.followed = Some(link.href);
    }
}

/// The side column: the story's zoom and appearance, set by their buttons, or by ← and → on the
/// one ↑ and ↓ pick while the column has the keyboard.
fn controls<T: 'static>(s: &Shown<T>, active: bool, cx: &mut Cx<Shown<T>>) -> El {
    let dark = cx.system_dark();
    let t = tok::theme(dark);
    let look = s.look;
    let zoom = control_row(t, dark, "Zoom", active && look.at == Control::Zoom)
        .child(step_button::<T>(t, "zoom-out", "−", look.zoom > 0, -1))
        .child(
            el().key("zoom-value")
                .w(48.0)
                .center()
                .child(text(format!("{:.0}%", look.zoom() * 100.0)).font(type_::BODY).color(t.text)),
        )
        .child(step_button::<T>(t, "zoom-in", "+", look.zoom + 1 < ZOOMS.len(), 1));
    let appearance = control_row(t, dark, "Theme", active && look.at == Control::Appearance).children(
        Appearance::ALL.map(|appearance| {
            let on = look.appearance == appearance;
            el().key(format!("theme-{}", appearance.label().to_lowercase()))
                .role("radio")
                .label(appearance.label())
                .h(24.0)
                .px(8.0)
                .center()
                .radius(6.0)
                .fill(if on { t.control_track } else { Color::TRANSPARENT })
                .hover(if on { t.control_track } else { t.hover })
                .on_press(move |s: &mut Shown<T>| s.look.appearance = appearance)
                .child(text(appearance.label()).font(type_::LABEL).color(if on { t.text } else { t.text_secondary }))
        }),
    );
    col().key("look").w_full().gap(6.0).child(zoom).child(appearance)
}

fn control_row(t: Theme, dark: bool, label: &'static str, picked: bool) -> El {
    let ring = if picked { tok::accent(0, dark) } else { t.card_border };
    row()
        .key(format!("control-{}", label.to_lowercase()))
        .w_full()
        .h(36.0)
        .pl(12.0)
        .pr(6.0)
        .gap(2.0)
        .center_y()
        .radius(8.0)
        .fill(t.card)
        .outline(1.0, 0.0, ring)
        .child(text(label).font(type_::BODY).color(t.text))
        .child(el().grow(1.0))
}

fn step_button<T: 'static>(t: Theme, key: &'static str, label: &'static str, enabled: bool, by: isize) -> El {
    let button = el().key(key).role("button").label(key).size(24.0, 24.0).center().radius(6.0);
    let button = button.child(text(label).font(type_::BODY).color(t.text));
    if !enabled {
        return button.opacity(0.3);
    }
    button.hover(t.hover).press(t.press).on_press(move |s: &mut Shown<T>| s.look.step(Control::Zoom, by))
}

/// The side column's keys: true for one it took.
fn control_key<T>(s: &mut Shown<T>, key: &KeyEvent, _: &mut Tx) -> bool {
    let look = &mut s.look;
    match key.key {
        Key::Up if look.at == Control::Appearance => look.at = Control::Zoom,
        Key::Down if look.at == Control::Zoom => look.at = Control::Appearance,
        Key::Left => look.step(look.at, -1),
        Key::Right => look.step(look.at, 1),
        _ => return false,
    }
    true
}

/// Empties the editor, then types [`STREAMED`] in.
fn stream(s: &mut Shown<TextEdit>, play: &mut Play<'_, Shown<TextEdit>>) {
    play.clear();
    s.source.replace_text("");
    arrive(play, 0);
}

/// Queues the next chunk of [`STREAMED`] after `at`, and the one after it.
fn arrive(play: &mut Play<'_, Shown<TextEdit>>, at: usize) {
    play.after(CHUNK_EVERY, move |s, play| {
        let mut next = (at + CHUNK).min(STREAMED.len());
        while !STREAMED.is_char_boundary(next) {
            next += 1;
        }
        s.source.replace_text(&STREAMED[..next]);
        if next < STREAMED.len() {
            arrive(play, next);
        }
    });
}

/// The icons the fixtures name: a lambda and code brackets stroked in the diagram's foreground,
/// as Pele's icon box strokes, and an amber folder in place of treeview's own.
fn icons(name: &str) -> Option<Icon> {
    let grid = Icon::new([0.0, 0.0, 24.0, 24.0]).ok()?;
    let icon = match name {
        "logos:aws-lambda" => grid.stroke("M7 4h2.5L18 20M12.8 11.5 7 20", 2.0, IconPaint::Current),
        "logos:react" => grid.stroke("M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16", 2.0, IconPaint::Current),
        "mermaid-treeview:folder" => grid.fill("M3 6h6l2 2h10v11H3z", IconPaint::Fixed(rune::color::hex("#e0a030")?)),
        _ => return None,
    };
    icon.ok()
}
