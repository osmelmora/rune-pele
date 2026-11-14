//! The SVG writer: a Scene as the bytes Pele writes.
//!
//! The writer never lays anything out and never reorders. It spells each value a node holds, in
//! the order the node recorded, and then writes the root from what the body turned out to use.

use crate::input::{JsText, decode_entities};
use crate::measure::{Emphasis, Script};
use crate::scene::{
    AnchorKind, Field, GeometryField, LineJoin, NodeContent, NodeId, Paint, PathProgram, PathStep, PlacedText, Role,
    Scene, Style, TextEmission, TextWeight, Transform,
};
use crate::{Diagnostic, ErrorCode, ParityScene, RenderError, Rendered, js};

/// Writes a parity drawing as Pele's SVG.
pub fn write(parity: &ParityScene) -> Result<Rendered, RenderError> {
    let scene = parity.scene();
    let mut body = String::new();
    for &root in &scene.roots {
        write_node(scene, root, &mut body);
    }
    let svg = write_root(scene, &body);
    let limit = scene.document.limit_utf16;
    if svg.encode_utf16().count() > limit {
        let message = format!("Diagram output is longer than the limit of {limit} characters.");
        let diagnostic = Diagnostic { kind: Some(scene.document.kind), ..Diagnostic::new(ErrorCode::Limit, message) };
        return Err(RenderError::Diagnostic(diagnostic));
    }
    Ok(Rendered { svg, width: scene.size().width, height: scene.size().height })
}

/// Pele's `esc`: escapes text for XML and drops the characters XML cannot hold, DEL and any
/// surrogate that has no partner.
fn esc(text: &[u16], out: &mut String) {
    for c in char::decode_utf16(text.iter().copied()).flatten() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\0'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}' | '\u{fffe}' | '\u{ffff}' => {}
            _ => out.push(c),
        }
    }
}

fn esc_str(text: &str, out: &mut String) {
    esc(&text.encode_utf16().collect::<Vec<_>>(), out);
}

/// Pele's `escText`, for text that is only ever read as text: entity codes become their
/// characters first.
fn esc_text(text: &JsText, out: &mut String) {
    esc(&decode_entities(text), out);
}

const SERIES: [&str; 8] = ["#4c78a8", "#f58518", "#54a24b", "#e45756", "#72b7b2", "#eeca3b", "#b279a2", "#9d755d"];

fn role(role: Role) -> &'static str {
    match role {
        Role::Background => "var(--_bg)",
        Role::Border => "var(--_b)",
        Role::OnLight => "var(--pele-fg-on-light,#1f1f1f)",
        Role::OnDark => "var(--pele-fg-on-dark,#fff)",
    }
}

/// The value of a paint attribute, or `None` where Pele writes no attribute at all.
fn paint(scene: &Scene, paint: Paint) -> Option<String> {
    Some(match paint {
        Paint::None => "none".to_owned(),
        Paint::Role(named) => role(named).to_owned(),
        Paint::Series(slot) => format!("var(--pele-series-{},{})", slot.index() + 1, SERIES[slot.index()]),
        Paint::OnSeries(slot) => role(scene.document.palette.ink(slot)?).to_owned(),
    })
}

fn write_pair(pair: &[crate::scene::Number; 2], out: &mut String) {
    pair[0].write(out);
    out.push(',');
    pair[1].write(out);
}

fn write_path(path: &PathProgram, out: &mut String) {
    for step in &path.steps {
        match step {
            PathStep::Move(to) => {
                out.push('M');
                write_pair(to, out);
            }
            PathStep::Line(to) => {
                out.push('L');
                write_pair(to, out);
            }
            PathStep::Arc { radii, large, sweep, end } => {
                out.push('A');
                write_pair(radii, out);
                out.push_str(if *large { " 0 1 " } else { " 0 0 " });
                out.push_str(if *sweep { "1 " } else { "0 " });
                write_pair(end, out);
            }
            PathStep::Close => out.push('Z'),
        }
    }
}

/// Writes one attribute of a node, where the node sets it.
fn write_field(scene: &Scene, id: NodeId, field: Field, out: &mut String) {
    let node = scene.node(id);
    let properties = &node.properties;
    let mut number = |name: &str, value: Option<&crate::scene::Number>| {
        if let Some(value) = value {
            out.push_str(name);
            value.write(out);
            out.push('"');
        }
    };
    match field {
        Field::Class => {
            if let Some(class) = properties.class {
                out.push_str(" class=\"");
                out.push_str(class);
                out.push('"');
            }
        }
        Field::DataId => {
            if let Some(part) = node.part {
                out.push_str(" data-id=\"");
                esc_text(&scene.entity(part).author_text, out);
                out.push('"');
            }
        }
        Field::Transform => {
            if let Some(Transform::Translate(x, y)) = &properties.transform {
                out.push_str(" transform=\"translate(");
                write_pair(&[*x, *y], out);
                out.push_str(")\"");
            }
        }
        Field::Geometry(geometry) => match (&node.content, geometry) {
            (NodeContent::Rect { x, .. }, GeometryField::X) => number(" x=\"", Some(x)),
            (NodeContent::Rect { y, .. }, GeometryField::Y) => number(" y=\"", Some(y)),
            (NodeContent::Rect { width, .. }, GeometryField::Width) => number(" width=\"", Some(width)),
            (NodeContent::Rect { height, .. }, GeometryField::Height) => number(" height=\"", Some(height)),
            (NodeContent::Rect { rx, .. }, GeometryField::Rx) => number(" rx=\"", Some(rx)),
            (NodeContent::Circle { radius }, GeometryField::Radius) => number(" r=\"", Some(radius)),
            (NodeContent::Text(PlacedText { emission: TextEmission::Plain { x, .. }, .. }), GeometryField::X) => {
                number(" x=\"", Some(x));
            }
            (NodeContent::Text(PlacedText { emission: TextEmission::Plain { y, .. }, .. }), GeometryField::Y) => {
                number(" y=\"", Some(y));
            }
            (NodeContent::Path(path), GeometryField::D) => {
                out.push_str(" d=\"");
                write_path(path, out);
                out.push('"');
            }
            (content, geometry) => unreachable!("the builder never records {geometry:?} for {content:?}"),
        },
        Field::Fill | Field::Stroke => {
            let (name, value) = match field {
                Field::Fill => (" fill=\"", properties.fill),
                _ => (" stroke=\"", properties.stroke),
            };
            if let Some(value) = value.and_then(|value| paint(scene, value)) {
                out.push_str(name);
                out.push_str(&value);
                out.push('"');
            }
        }
        Field::StrokeWidth => number(" stroke-width=\"", properties.stroke_width.as_ref()),
        Field::StrokeJoin => {
            if let Some(LineJoin::Round) = properties.join {
                out.push_str(" stroke-linejoin=\"round\"");
            }
        }
        Field::FontSize => number(" font-size=\"", properties.font_size.as_ref()),
        Field::FontWeight => match properties.weight {
            Some(TextWeight::Title) => out.push_str(" font-weight=\"var(--_tw)\""),
            Some(TextWeight::Bold) => out.push_str(BOLD),
            None => {}
        },
        Field::FontStyle => out.push_str(ITALIC),
        Field::FontFamily => out.push_str(MONO),
        Field::Anchor => {
            if let Some(anchor) = properties.anchor {
                out.push_str(" text-anchor=\"");
                out.push_str(match anchor {
                    AnchorKind::Start => "start",
                    AnchorKind::Middle => "middle",
                    AnchorKind::End => "end",
                });
                out.push('"');
            }
        }
    }
}

const BOLD: &str = " font-weight=\"var(--_w)\"";
const ITALIC: &str = " font-style=\"italic\"";
const MONO: &str = " font-family=\"var(--_fm)\"";

/// Pele's `spanStyle`: the emphasis of one run that its text element does not already carry.
fn write_run_style(style: Emphasis, shared: Emphasis, out: &mut String) {
    if style.bold && !shared.bold {
        out.push_str(BOLD);
    }
    if style.italic && !shared.italic {
        out.push_str(ITALIC);
    }
    if style.mono && !shared.mono {
        out.push_str(MONO);
    }
    if style.underline {
        out.push_str(" text-decoration=\"underline\"");
    }
    match style.script {
        Script::Normal => {}
        Script::Sub => out.push_str(" baseline-shift=\"sub\" font-size=\"smaller\""),
        Script::Super => out.push_str(" baseline-shift=\"super\" font-size=\"smaller\""),
    }
}

fn write_text(text: &PlacedText, out: &mut String) {
    match &text.emission {
        TextEmission::Plain { .. } => {
            out.push('>');
            for run in text.label.lines.iter().flat_map(|line| &line.runs) {
                esc(&run.text, out);
            }
        }
        TextEmission::Rich { lines, shared } => {
            out.push_str(" xml:space=\"preserve\">");
            for (line, at) in text.label.lines.iter().zip(lines) {
                let position = |out: &mut String| {
                    out.push_str(" x=\"");
                    at[0].write(out);
                    out.push_str("\" y=\"");
                    at[1].write(out);
                    out.push('"');
                };
                for (index, run) in line.runs.iter().enumerate() {
                    out.push_str("<tspan");
                    if index == 0 {
                        position(out);
                    }
                    write_run_style(run.style, *shared, out);
                    out.push('>');
                    esc(&run.text, out);
                    out.push_str("</tspan>");
                }
                if line.runs.is_empty() {
                    out.push_str("<tspan");
                    position(out);
                    out.push_str("> </tspan>");
                }
            }
        }
    }
    out.push_str("</text>");
}

fn write_node(scene: &Scene, id: NodeId, out: &mut String) {
    let node = scene.node(id);
    let tag = match &node.content {
        NodeContent::Group(_) => "g",
        NodeContent::Path(_) => "path",
        NodeContent::Rect { .. } => "rect",
        NodeContent::Circle { .. } => "circle",
        NodeContent::Text(_) => "text",
    };
    out.push('<');
    out.push_str(tag);
    for &field in &node.emission {
        write_field(scene, id, field, out);
    }
    match &node.content {
        NodeContent::Group(children) => {
            out.push('>');
            for &child in children {
                write_node(scene, child, out);
            }
            out.push_str("</g>");
        }
        NodeContent::Text(text) => write_text(text, out),
        NodeContent::Path(_) | NodeContent::Rect { .. } | NodeContent::Circle { .. } => out.push_str("/>"),
    }
}

// Private aliases set once on the root element, so each use is short and the public --pele-*
// tokens keep their fallbacks. Copied from Pele's `ROOT_STYLE` in `src/svg/theme.ts`.
const ROOT_STYLE: [(&str, &str); 9] = [
    ("--_bg", "var(--pele-bg,#fff)"),
    ("--_fg", "var(--pele-fg,#1f1f1f)"),
    ("--_m", "var(--pele-muted,#6e6e6e)"),
    ("--_l", "var(--pele-line,#1f1f1f)"),
    ("--_s", "var(--pele-surface,#f3f3f3)"),
    ("--_a", "var(--pele-surface-alt,#e6e6e6)"),
    ("--_b", "var(--pele-border,#8a8a8a)"),
    ("--_c", "var(--pele-accent,#5b7bd5)"),
    ("color", "var(--_fg)"),
];

/// The root's style in Pele's order: fit, aliases, the radius and weights the body uses, font,
/// then the palette the source sets. The root sets the radius and each bold weight only where
/// the body uses it, so those are read from the bytes of the body.
fn root_style(scene: &Scene, body: &str) -> Style {
    let document = &scene.document;
    let mut style = Style::unterminated();
    if document.responsive {
        style.declare("max-width", "100%");
        style.declare("height", "auto");
    }
    for (property, value) in ROOT_STYLE {
        style.declare(property, value);
    }
    if body.contains("var(--_r)") {
        style.declare("--_r", "var(--pele-radius,4px)");
    }
    for (alias, name) in [("tw", "title"), ("hw", "heading"), ("w", "bold")] {
        if body.contains(&format!("var(--_{alias})")) {
            style.declare(&format!("--_{alias}"), format!("var(--pele-{name}-weight,bold)").as_str());
        }
    }
    style.declare("--_fm", "var(--pele-font-mono,monospace)");
    style.declare("font", format!("{}px sans-serif", js::to_string(document.font_size)).as_str());
    style.declare("letter-spacing", "normal");
    style.declare("word-spacing", "normal");
    style.declare("text-transform", "none");
    style.declare("direction", "ltr");
    style.declare("font-family", "var(--pele-font,sans-serif)");
    for (index, colour) in document.palette.slots() {
        style.declare(&format!("--pele-series-{}", index + 1), colour.spelling.clone());
    }
    style
}

fn write_style(style: &Style, out: &mut String) {
    for (index, declaration) in style.declarations.iter().enumerate() {
        if index > 0 {
            out.push(';');
        }
        out.push_str(&declaration.property);
        out.push(':');
        esc(&declaration.value, out);
    }
    if style.terminated {
        out.push(';');
    }
}

/// Pele's `svgDocument`: the root element around a body that is already written.
fn write_root(scene: &Scene, body: &str) -> String {
    let document = &scene.document;
    let kind = document.kind.name();
    let mut prefix = String::new();
    esc_str(document.id_prefix.as_deref().unwrap_or("pele"), &mut prefix);
    let size = [js::to_string(document.size.width), js::to_string(document.size.height)];

    let mut out = String::with_capacity(body.len() + 1024);
    out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"pele pele-");
    out.push_str(kind);
    out.push_str(&format!("\" viewBox=\"0 0 {0} {1}\" width=\"{0}\" height=\"{1}\" style=\"", size[0], size[1]));
    write_style(&root_style(scene, body), &mut out);
    out.push_str("\" fill=\"var(--_fg)\" role=\"graphics-document document\" aria-roledescription=\"");
    out.push_str(kind);
    out.push('"');

    fn named(text: &Option<JsText>) -> Option<&JsText> {
        text.as_ref().filter(|text| !text.is_empty())
    }
    let (title, description) = (named(&document.accessible.title), named(&document.accessible.description));
    if title.is_some() {
        out.push_str(&format!(" aria-labelledby=\"{prefix}-title\""));
    }
    if description.is_some() {
        out.push_str(&format!(" aria-describedby=\"{prefix}-desc\""));
    }
    out.push('>');
    if let Some(title) = title {
        out.push_str(&format!("<title id=\"{prefix}-title\">"));
        esc_text(title, &mut out);
        out.push_str("</title>");
    }
    if let Some(description) = description {
        out.push_str(&format!("<desc id=\"{prefix}-desc\">"));
        esc_text(description, &mut out);
        out.push_str("</desc>");
    }
    out.push_str(body);
    out.push_str("</svg>");
    out
}
