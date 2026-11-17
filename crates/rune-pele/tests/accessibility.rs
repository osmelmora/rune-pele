//! What assistive technology and the keyboard reach of a native drawing: the diagram's name and
//! description, as its SVG gives them, and each link as a Tab stop named by its label that Enter
//! follows as a press does.

use rune::{Color, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx, el};
use rune_pele::{Diagram, Event, FontCatalog, Link, Palette, ViewOptions};
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
