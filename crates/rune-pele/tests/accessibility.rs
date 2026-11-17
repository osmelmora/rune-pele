//! What assistive technology and the keyboard reach of a native drawing: the diagram's name and
//! description, as its SVG gives them, and each link as a Tab stop named by its label that Enter
//! follows as a press does.

use rune::{Color, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx, el};
use rune_pele::{Diagram, Event, FontCatalog, KeptLook, Link, Palette, RenderStatus, ViewOptions};
use serde_json::Value;

const SOURCE: &str = "flowchart LR\n  accTitle: Deploy process\n  accDescr: Review happens before release\n  \
                      A[Read the guide] --> B[Release]\n  click A href \"https://example.org/guide\" \"Open the guide\"";

struct App {
    diagram: Diagram,
    source: String,
    followed: Vec<Link>,
}

fn on_event(app: &mut App, event: Event, _: &mut Tx) {
    if let Event::Activate(link) = event {
        app.followed.push(link);
    }
}

fn view(app: &App, cx: &mut Cx<App>) -> El {
    let options = ViewOptions::natural().palette(Palette::pele());
    let drawn = app.diagram.view_with(cx, &app.source, options, on_event).expect("the source draws");
    el().size(400.0, 300.0).fill(Color::WHITE).child(drawn)
}

fn runtime(source: &str) -> Runtime<App> {
    let app =
        App { diagram: Diagram::new("deploy", FontCatalog::rune()), source: source.to_owned(), followed: Vec::new() };
    let mut runtime = Runtime::new(app, view, (400.0, 300.0), Env::new(false, 1.0));
    runtime.frame(0.0);
    runtime
}

fn press(runtime: &mut Runtime<App>, key: Key) {
    runtime.key(KeyEvent { key, mods: Mods::default() });
    runtime.frame(0.0);
}

/// Every node of the accessibility tree with `role`, depth first.
fn with_role<'a>(node: &'a Value, role: &str, into: &mut Vec<&'a Value>) {
    if node["role"] == role {
        into.push(node);
    }
    for child in node["children"].as_array().into_iter().flatten() {
        with_role(child, role, into);
    }
}

fn nodes<'a>(tree: &'a Value, role: &str) -> Vec<&'a Value> {
    let mut found = Vec::new();
    with_role(tree, role, &mut found);
    found
}

#[test]
fn the_diagram_is_named_and_described_as_its_svg_is() {
    let pele = pele_oracle::render_all(&[SOURCE.to_owned()]);
    let pele_oracle::PeleRender::Svg { svg, .. } = &pele[0] else { panic!("Pele draws it: {pele:?}") };
    assert!(svg.contains(">Deploy process</title>") && svg.contains(">Review happens before release</desc>"));

    let tree = runtime(SOURCE).accessibility_json();
    let figures = nodes(&tree, "figure");
    let [figure] = &figures[..] else { panic!("one figure: {figures:?}") };
    assert_eq!(figure["name"], "Deploy process", "the diagram's name is its accTitle");
    assert_eq!(figure["description"], "Review happens before release", "and its description its accDescr");

    let untitled = runtime("flowchart LR\n  A --> B").accessibility_json();
    let figures = nodes(&untitled, "figure");
    let [figure] = &figures[..] else { panic!("one figure: {figures:?}") };
    assert_eq!((figure.get("name"), figure.get("description")), (None, None), "a source that gives neither");
}

#[test]
fn a_link_is_a_tab_stop_named_by_its_label_that_enter_follows() {
    for full_keyboard_access in [true, false] {
        let mut runtime = runtime(SOURCE);
        runtime.set_full_keyboard_access(full_keyboard_access);
        let tree = runtime.accessibility_json();
        let links = nodes(&tree, "link");
        let [link] = &links[..] else { panic!("one link: {links:?}") };
        assert_eq!(link["name"], "Read the guide", "a link is named by the label it shows");

        let elements = runtime.elements();
        let link = elements.iter().find(|element| element.role == "link").expect("the link");
        assert!(link.focusable, "the link takes focus");
        press(&mut runtime, Key::Tab);
        let focused: Vec<_> = runtime.elements().into_iter().filter(|element| element.focused).collect();
        let roles: Vec<&str> = focused.iter().map(|element| element.role).collect();
        assert_eq!(roles, ["link"], "Tab reaches the link, with full keyboard access {full_keyboard_access}");
        assert!(runtime.state.followed.is_empty(), "focus alone follows nothing");

        press(&mut runtime, Key::Enter);
        let followed: Vec<&str> = runtime.state.followed.iter().map(|link| link.href.as_str()).collect();
        assert_eq!(followed, ["https://example.org/guide"], "Enter follows the focused link, once");
    }
}

struct Fitted {
    diagram: Diagram,
    source: String,
    look: KeptLook,
}

/// The box a fitted view is put in, narrower than the diagram.
const BOX: (f32, f32) = (200.0, 300.0);

fn fitted_view(app: &Fitted, cx: &mut Cx<Fitted>) -> El {
    let options = ViewOptions::fit().kept_look(app.look).palette(Palette::pele());
    let drawn = app.diagram.view(cx, &app.source, options).expect("the source, or the drawing kept, draws");
    el().size(BOX.0, BOX.1).fill(Color::WHITE).child(drawn)
}

/// What a fitted view shows: who drew it, its figures and the rects of its elements keyed with
/// the diagram's id, in the order Rune lays them out.
fn fitted(runtime: &Runtime<Fitted>) -> (RenderStatus, Vec<Value>, Vec<[f32; 4]>) {
    let tree = runtime.accessibility_json();
    let figures = nodes(&tree, "figure").into_iter().cloned().collect();
    let keyed = runtime.elements().into_iter().filter(|e| e.key.as_deref() == Some("deploy")).map(|e| e.rect).collect();
    (runtime.state.diagram.status(), figures, keyed)
}

/// The fit's frame, the dimmed look's wrapper and the drawing are all keyed with the diagram's
/// id. The frame comes first, so the fit reads the box's width, and only the drawing is a
/// figure, named and described, whether the source drew or the drawing is kept in either look.
#[test]
fn a_fitted_view_is_one_named_figure_whether_drawn_or_kept() {
    let unfitted = runtime(SOURCE).state.diagram.scene().expect("a native drawing").size().width;
    assert!(unfitted > f64::from(BOX.0), "unfitted, the diagram is {unfitted} wide, wider than the box");
    for (look, kept_keys) in [(KeptLook::DimmedWithError, 3), (KeptLook::Unchanged, 2)] {
        let app = Fitted { diagram: Diagram::new("deploy", FontCatalog::rune()), source: SOURCE.to_owned(), look };
        let mut runtime = Runtime::new(app, fitted_view, BOX, Env::new(false, 1.0));
        runtime.frame(0.0);
        runtime.frame(0.1);
        let drew = fitted(&runtime);
        let natural = runtime.state.diagram.scene().expect("a native drawing").size().width;
        let zoom = runtime.state.diagram.zoom().expect("a native drawing");
        runtime.update(|app, _| app.source = format!("{SOURCE}\n  Z -->"));
        runtime.frame(1.0);
        let kept = fitted(&runtime);
        println!(
            "{look:?}: {unfitted:.1} wide unfitted, laid out {natural:.1} wide, zoom {zoom:.4}; drew {} figure, keyed {:?}; kept {} figure, keyed {:?}",
            drew.1.len(),
            drew.2,
            kept.1.len(),
            kept.2
        );
        for ((status, figures, keyed), want, keys) in
            [(drew, RenderStatus::Pele, 2), (kept, RenderStatus::Kept, kept_keys)]
        {
            assert_eq!(status, want, "{look:?}");
            let [figure] = &figures[..] else { panic!("{look:?}, {want:?}: one figure, not {figures:?}") };
            assert_eq!(figure["name"], "Deploy process", "{look:?}, {want:?}");
            assert_eq!(figure["description"], "Review happens before release", "{look:?}, {want:?}");
            assert_eq!(keyed.len(), keys, "{look:?}, {want:?}: the frame, any wrapper and the drawing: {keyed:?}");
            assert_eq!(keyed[0][2], BOX.0, "{look:?}, {want:?}: the first keyed is the frame, as wide as the box");
            let drawing = keyed[keys - 1][2];
            assert!(drawing <= BOX.0, "{look:?}, {want:?}: the drawing, {drawing} wide, fits the box");
        }
    }
}
