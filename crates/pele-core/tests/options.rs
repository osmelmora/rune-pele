//! An app's options, made with the builders, drawn as Pele draws the same sources with the same
//! render options. Each option also changes the drawing, so a builder that sets nothing fails.

use pele_core::{
    HostConfig, LayoutOptions, Limits, MonospacePart, ParseOptions, RenderError, Rendered, ResourcePolicy,
};
use pele_oracle::PeleRender;
use serde_json::{Value, json};

const ACROSS: &str = "flowchart LR
  A[Model writes] --> B[Chat streams] --> C[rune-pele draws] --> D[Reader follows] --> E[Done]
";
const LINKED: &str = "flowchart LR
  A[Docs] --> B[Mail] --> C[Picture]
  click A href \"https://example.org/docs\" \"Open the docs\"
  click B href \"mailto:team@example.org\"
  C@{ img: \"https://example.org/c.png\", h: 40, w: 40 }
";
const CLASS: &str = "classDiagram
  class Shape {
    +int sides
    +area() float
  }
";
const SANKEY: &str = "sankey-beta
Sun,Plants,10
Plants,Animals,4
";
const GANTT: &str = "gantt
  dateFormat YYYY-MM-DD
  section Work
  Write :a1, 2026-01-01, 30d
  Review :after a1, 20d
";

fn ours(source: &str, parse: ParseOptions, layout: &LayoutOptions) -> Result<Rendered, RenderError> {
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, layout)?)
}

/// Each case: what it sets, the source, the app's options and Pele's render options.
fn cases() -> Vec<(&'static str, &'static str, ParseOptions, LayoutOptions, Value)> {
    let parse = ParseOptions::default;
    let layout = LayoutOptions::default;
    let resources = ResourcePolicy::default;
    let fixed = HostConfig::from_init_json(r#"{"flowchart":{"useMaxWidth":false}}"#).expect("a JSON object");
    vec![
        ("font_size", ACROSS, parse(), layout().font_size(20.0), json!({ "fontSize": 20 })),
        ("padding", ACROSS, parse(), layout().padding(30.0), json!({ "padding": 30 })),
        ("max_width", ACROSS, parse(), layout().max_width(300.0), json!({ "maxWidth": 300 })),
        ("responsive", ACROSS, parse(), layout().responsive(false), json!({ "responsive": false })),
        ("id_prefix", SANKEY, parse(), layout().id_prefix("app"), json!({ "idPrefix": "app" })),
        (
            "auto_direction",
            ACROSS,
            parse(),
            layout().max_width(300.0).auto_direction(false),
            json!({ "maxWidth": 300, "autoDirection": false }),
        ),
        (
            "direction_breakpoint",
            ACROSS,
            parse(),
            layout().max_width(700.0).direction_breakpoint(800.0),
            json!({ "maxWidth": 700, "directionBreakpoint": 800 }),
        ),
        (
            "monospace",
            CLASS,
            parse(),
            layout().monospace([MonospacePart::Members]),
            json!({ "monospace": ["members"] }),
        ),
        ("now", GANTT, parse(), layout().now(1_775_000_000_000.0), json!({ "now": 1_775_000_000_000.0 })),
        ("links", LINKED, parse(), layout().resources(resources().links(false)), json!({ "links": false })),
        ("images", LINKED, parse(), layout().resources(resources().images(false)), json!({ "images": false })),
        (
            "link_schemes",
            LINKED,
            parse(),
            layout().resources(resources().link_schemes(["https"])),
            json!({ "linkSchemes": ["https"] }),
        ),
        (
            "image_schemes",
            LINKED,
            parse(),
            layout().resources(resources().image_schemes(["data"])),
            json!({ "imageSchemes": ["data"] }),
        ),
        ("link_rel", LINKED, parse(), layout().resources(resources().link_rel("")), json!({ "linkRel": "" })),
        (
            "config",
            ACROSS,
            parse().config(fixed),
            layout(),
            json!({ "config": { "flowchart": { "useMaxWidth": false } } }),
        ),
        ("source_utf16", ACROSS, parse().limits(Limits::default().source_utf16(40)), layout(), json!({ "limit": 40 })),
        (
            "svg_utf16",
            ACROSS,
            parse().limits(Limits::default().svg_utf16(1_000)),
            layout(),
            json!({ "outputLimit": 1_000 }),
        ),
        ("max_edges", ACROSS, parse().limits(Limits::default().max_edges(2)), layout(), json!({ "maxEdges": 2 })),
    ]
}

/// What a render shows: the SVG and its size, or Pele's error message.
fn shown(rendered: Result<Rendered, RenderError>) -> String {
    match rendered {
        Ok(rendered) => format!("{}x{}\n{}", rendered.width, rendered.height, rendered.svg),
        Err(RenderError::Diagnostic(diagnostic)) => format!("PeleError: {}", diagnostic.message),
        Err(error) => panic!("not drawn: {error}"),
    }
}

fn pele_shown(answer: &PeleRender) -> String {
    match answer {
        PeleRender::Svg { svg, width, height, .. } => format!("{width}x{height}\n{svg}"),
        PeleRender::Error { error, .. } => error.clone(),
    }
}

#[test]
fn each_builder_draws_as_pele_does_with_its_option() {
    let cases = cases();
    let requests: Vec<(String, String)> =
        cases.iter().map(|(_, source, _, _, pele)| (source.to_string(), pele.to_string())).collect();
    let answers = pele_oracle::render_all_with(&requests);
    for ((name, source, parse, layout, _), answer) in cases.into_iter().zip(&answers) {
        let plain = shown(ours(source, ParseOptions::default(), &LayoutOptions::default()));
        let drawn = shown(ours(source, parse, &layout));
        assert_eq!(drawn, pele_shown(answer), "{name}: drawn as Pele draws it with the option");
        assert_ne!(drawn, plain, "{name}: the option changes the drawing");
    }
}

#[test]
fn turn_wider_than_turns_a_diagram_pele_s_breakpoint_leaves_across() {
    let at = |layout: LayoutOptions| shown(ours(ACROSS, ParseOptions::default(), &layout));
    let wide = LayoutOptions::default().max_width(700.0);
    let turned = at(wide.clone().turn_wider_than(700.0));
    assert_ne!(turned, at(wide), "over the breakpoint, only turn_wider_than turns it");
    let answers = pele_oracle::render_all_with(&[(
        ACROSS.to_owned(),
        json!({ "maxWidth": 700, "directionBreakpoint": 701 }).to_string(),
    )]);
    assert_eq!(turned, pele_shown(&answers[0]), "turned as Pele turns it under its breakpoint");
}
