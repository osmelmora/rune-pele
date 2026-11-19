//! What the drawing a view shows can do, read through `Diagram::engine`, held to what that
//! drawing does: a press on a link, the pointer resting on it, and a zoom. A drawing rune-mermaid
//! draws has no links or tooltips and ignores the zoom, whether the source drew it or the view
//! kept it.

#![cfg(feature = "fallback")]

use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Window};
use rune_pele::{Diagram, Engine, Event, FontCatalog, Link, RenderStatus, ViewOptions};

/// The review's fixture: a link with a tooltip on A, and a line through B's text, which the
/// lowering does not draw.
const FALLBACK: &str = "flowchart LR
  accTitle: Deploy process
  accDescr: Review happens before release
  A[Read the guide] --> B[Release]
  click A href \"https://example.org/guide\" \"Open the guide\"
  style B text-decoration:line-through
";

/// The same flowchart without the line through B, which is drawn natively.
fn native() -> String {
    FALLBACK.replace("  style B text-decoration:line-through\n", "")
}

/// A source that fails after either of the two.
fn failing(source: &str) -> String {
    format!("{source}  B -->\n")
}

const LABEL: &str = "Read the guide";
const SIZE: (f32, f32) = (600.0, 300.0);

struct Viewer {
    diagram: Diagram,
    source: String,
    zoom: f32,
    followed: Vec<Link>,
}

fn on_event(viewer: &mut Viewer, event: Event, _: &mut rune::Tx) {
    if let Event::Activate(link) = event {
        viewer.followed.push(link);
    }
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let page = el().fill(Color::WHITE);
    match s.diagram.view_with(cx, &s.source, ViewOptions::new(SIZE.0).zoom(s.zoom), on_event) {
        Ok(drawn) => page.child(drawn),
        Err(_) => page,
    }
}

fn window<'a>(capturer: &'a Capturer, source: &str) -> Window<'a, Viewer> {
    let viewer = Viewer {
        diagram: Diagram::new("diagram", FontCatalog::rune()),
        source: source.to_owned(),
        zoom: 1.0,
        followed: Vec::new(),
    };
    let mut window = capturer.window(viewer, view, SIZE, Env::new(false, 1.0));
    window.capture();
    window
}

/// What a drawing can do, as the engine says or as a user finds it.
#[derive(Debug, PartialEq)]
struct Can {
    links: bool,
    tooltips: bool,
    zoom: bool,
}

fn claimed(engine: Engine) -> Can {
    Can { links: engine.has_links(), tooltips: engine.has_tooltips(), zoom: engine.zooms() }
}

/// The widest element keyed by the diagram's id: the drawing, or the kept look around it.
fn drawn_width(window: &Window<'_, Viewer>) -> f32 {
    let elements = window.runtime().elements();
    let keyed = elements.iter().filter(|element| element.key.as_deref() == Some("diagram"));
    keyed.map(|element| element.rect[2]).fold(0.0, f32::max)
}

/// What the drawing on screen does: a press on the linked node's label, the pointer resting
/// there, and the view's zoom halved.
fn found(window: &mut Window<'_, Viewer>) -> Can {
    window.capture();
    let elements = window.runtime().elements();
    let label = elements
        .iter()
        .find(|element| [&element.text, &element.label].iter().any(|shown| shown.as_deref() == Some(LABEL)))
        .unwrap_or_else(|| panic!("the drawing shows {LABEL:?}"));
    let [x, y, w, h] = label.rect;
    let (x, y) = (x + w / 2.0, y + h / 2.0);
    // From a corner, so that the pointer arrives at the label and its hover starts afresh.
    window.rest(0.0, 0.0, 0.0);
    window.rest(x, y, 2.0);
    let tooltips = window.runtime().help_tooltip().is_some();
    let before = window.state().followed.len();
    window.press(x, y);
    let links = window.state().followed.len() > before;

    let whole = drawn_width(window);
    window.update(|viewer| viewer.zoom = 0.5);
    window.capture();
    let half = drawn_width(window);
    window.update(|viewer| viewer.zoom = 1.0);
    window.capture();
    Can { links, tooltips, zoom: half < 0.75 * whole }
}

#[test]
fn the_engine_says_what_the_drawing_shown_can_do() {
    let capturer = Capturer::new();
    let report = |name: &str, window: &mut Window<'_, Viewer>| {
        window.capture();
        let diagram = &window.state().diagram;
        let (status, engine) = (diagram.status(), diagram.engine());
        let can = found(window);
        println!("{name:<22} {status:?}, engine {engine:?}, {can:?}");
        (status, engine, can)
    };

    let mut drawn = window(&capturer, &native());
    let (status, engine, can) = report("native", &mut drawn);
    assert_eq!((status, engine), (RenderStatus::Pele, Some(Engine::Pele)));
    assert_eq!(can, Can { links: true, tooltips: true, zoom: true }, "a native drawing does all three");
    assert_eq!(claimed(Engine::Pele), can, "the native engine says what its drawing does");

    let mut fallback = window(&capturer, FALLBACK);
    let (status, engine, can) = report("rune-mermaid", &mut fallback);
    assert_eq!((status, engine), (RenderStatus::RuneMermaid, Some(Engine::RuneMermaid)));
    assert_eq!(can, Can { links: false, tooltips: false, zoom: false }, "rune-mermaid's drawing does none");
    assert_eq!(claimed(Engine::RuneMermaid), can, "rune-mermaid says what its drawing does");

    // A kept drawing is the drawing of the last source that drew, and can do what it could.
    for (name, mut window, engine) in
        [("kept native", drawn, Engine::Pele), ("kept rune-mermaid", fallback, Engine::RuneMermaid)]
    {
        window.update(|viewer| viewer.source = failing(&viewer.source));
        let (status, kept, can) = report(name, &mut window);
        assert_eq!((status, kept), (RenderStatus::Kept, Some(engine)), "{name}");
        assert_eq!(claimed(engine), can, "{name}: the kept drawing's engine says what it does");
    }

    let nothing = window(&capturer, &failing(&native()));
    let diagram = &nothing.state().diagram;
    assert_eq!((diagram.status(), diagram.engine()), (RenderStatus::Unrendered, None), "no drawing, no engine");
}
