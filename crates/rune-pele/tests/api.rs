//! What an app reads of its diagram with no `pele-core` dependency of its own: the types of
//! rune-pele's errors and Scenes, named through `rune_pele::pele_core`.

use std::rc::Rc;

use rune_pele::capture::Capturer;
use rune_pele::pele_core::{ErrorCode, RenderError, Scene};
use rune_pele::rune::{Cx, El, Env, el};
use rune_pele::{Diagram, FontCatalog, ViewError, ViewOptions};

struct App {
    diagram: Diagram,
    source: &'static str,
}

fn view(app: &App, cx: &mut Cx<App>) -> El {
    app.diagram.view(cx, app.source, ViewOptions::new(400.0)).unwrap_or_else(|_| el())
}

/// `read` of the diagram once its view of `source` has been drawn.
fn after_view<T>(source: &'static str, read: impl FnOnce(&Diagram) -> T) -> T {
    let app = App { diagram: Diagram::new("diagram", FontCatalog::rune()), source };
    let capturer = Capturer::new();
    let mut window = capturer.window(app, view, (400.0, 300.0), Env::new(false, 1.0));
    window.capture();
    read(&window.state().diagram)
}

#[test]
fn an_app_reads_where_a_syntax_error_is_through_rune_pele_s_pele_core() {
    let error = after_view("flowchart LR\n  A -->\n", Diagram::error);
    let Some(ViewError::Core(RenderError::Diagnostic(diagnostic))) = error else {
        panic!("pele-core's diagnostic: {error:?}");
    };
    assert_eq!(diagnostic.code, ErrorCode::Syntax);
    let location = diagnostic.location.expect("a syntax error says where it is");
    assert_eq!((location.line, location.column_utf16), (2, 8), "after the arrow on the second line");
}

#[test]
fn an_app_reads_the_laid_out_size_through_rune_pele_s_pele_core() {
    let scene: Option<Rc<Scene>> = after_view("flowchart LR\n  A[Write] --> B[Draw]\n", Diagram::scene);
    let size = scene.expect("drawn natively").size();
    assert!(size.width > size.height && size.height > 0.0, "a flowchart that runs across: {size:?}");
}

#[test]
fn an_app_logs_who_drew_its_diagram_and_how_large_without_the_scene() {
    const SOURCE: &str = "flowchart LR\n  A[Write] --> B[Draw]\n";
    let (shown, size) =
        after_view(SOURCE, |diagram| (format!("{diagram:?}"), diagram.scene().map(|scene| scene.size())));
    assert!(shown.starts_with("Diagram { id: \"diagram\", status: Pele, engine: Some(Pele), error: None,"), "{shown}");
    assert!(shown.contains(&format!("source_len: Some({})", SOURCE.len())), "the source it drew: {shown}");
    assert!(shown.contains(&format!("size: {size:?}")), "the size Diagram::scene gives: {shown}");
    assert!(!shown.contains("Write"), "the Scene stays out: {shown}");
}
