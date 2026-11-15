//! The SVG writer: a Scene as the bytes Pele writes.
//!
//! The writer never lays anything out and never reorders. It spells each value a node holds, in
//! the order the node recorded, and then writes the root from what the body turned out to use.

use crate::input::{JsText, decode_entities};
use crate::measure::{Emphasis, Script};
use crate::scene::{
    AnchorKind, Aspect, Field, GeometryField, GradientId, LineCap, LineJoin, Node, NodeContent, NodeId, Number,
    PELE_SERIES, Paint, PathProgram, PathStep, PlacedText, Role, Scene, Style, TextEmission, TextWeight, Transform,
    esc, own_emphasis,
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

/// The body of a drawing without its root element, for a test that holds one helper's drawing
/// to the fragment Pele's helper returns.
pub(crate) fn write_fragment(scene: &Scene) -> String {
    let mut body = String::new();
    for &root in &scene.roots {
        write_node(scene, root, &mut body);
    }
    body
}

fn esc_str(text: &str, out: &mut String) {
    esc(&text.encode_utf16().collect::<Vec<_>>(), out);
}

/// Pele's `escText`, for text that is only ever read as text: entity codes become their
/// characters first.
fn esc_text(text: &JsText, out: &mut String) {
    esc(&decode_entities(text), out);
}

pub(crate) fn role(role: Role) -> &'static str {
    match role {
        Role::Background => "var(--_bg)",
        Role::Foreground => "var(--_fg)",
        Role::Border => "var(--_b)",
        Role::OnLight => "var(--pele-fg-on-light,#1f1f1f)",
        Role::OnDark => "var(--pele-fg-on-dark,#fff)",
        Role::Line => "var(--_l)",
        Role::Surface => "var(--_s)",
        Role::SurfaceAlt => "var(--_a)",
        Role::Muted => "var(--_m)",
        Role::Accent => "var(--_c)",
        Role::Critical => "var(--pele-critical,#e45756)",
    }
}

/// The value of a paint attribute, or `None` where Pele writes no attribute at all.
pub(crate) fn paint(scene: &Scene, paint: Paint) -> Option<String> {
    Some(match paint {
        Paint::None => "none".to_owned(),
        Paint::Role(named) => role(named).to_owned(),
        Paint::Series(slot) => format!("var(--pele-series-{},{})", slot.index() + 1, PELE_SERIES[slot.index()]),
        Paint::OnSeries(slot) => role(scene.document.palette.ink(slot)?).to_owned(),
        Paint::Source(id) => {
            let mut spelling = String::new();
            esc(&scene.colour(id).spelling, &mut spelling);
            spelling
        }
        Paint::Mix { tint, percent, base } => {
            format!("color-mix(in srgb,{} {}%,{})", role(tint), js::num(percent), role(base))
        }
    })
}

fn write_pair(pair: &[Number; 2], out: &mut String) {
    pair[0].write(out);
    out.push(',');
    pair[1].write(out);
}

/// A command letter, then its pairs with a space between them.
fn write_pairs(letter: char, pairs: &[[Number; 2]], out: &mut String) {
    out.push(letter);
    for (index, pair) in pairs.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        write_pair(pair, out);
    }
}

pub(crate) fn write_path(path: &PathProgram, out: &mut String) {
    for step in &path.steps {
        match step {
            PathStep::Move(to) => write_pairs('M', &[*to], out),
            PathStep::Line(to) => write_pairs('L', &[*to], out),
            PathStep::Horizontal(x) => {
                out.push('H');
                x.write(out);
            }
            PathStep::Vertical(y) => {
                out.push('V');
                y.write(out);
            }
            PathStep::Cubic(pairs) => write_pairs('C', pairs, out),
            PathStep::Quad(pairs) => write_pairs('Q', pairs, out),
            PathStep::Arc { radii, large, sweep, end } => {
                out.push('A');
                write_pair(radii, out);
                out.push_str(if *large { " 0 1 " } else { " 0 0 " });
                out.push_str(if *sweep { "1 " } else { "0 " });
                write_pair(end, out);
            }
            PathStep::Close => out.push('Z'),
            PathStep::LineBy(to) => write_pairs('l', &[*to], out),
            PathStep::HorizontalBy(x) => {
                out.push('h');
                x.write(out);
            }
            PathStep::QuadBy(pairs) => write_pairs('q', pairs, out),
            PathStep::SmoothQuadBy(to) => write_pairs('t', &[*to], out),
            PathStep::ArcBy { radii, large, sweep, end } => {
                out.push('a');
                write_pair(radii, out);
                out.push_str(if *large { " 0 1 " } else { " 0 0 " });
                out.push_str(if *sweep { "1 " } else { "0 " });
                write_pair(end, out);
            }
        }
    }
}

fn write_attribute(name: &str, out: &mut String, value: impl FnOnce(&mut String)) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    value(out);
    out.push('"');
}

/// Writes one geometry attribute of a node. The builder records only the fields its content has.
fn write_geometry(node: &Node, field: GeometryField, out: &mut String) {
    use GeometryField as G;
    let number = |name: &str, value: &Number, out: &mut String| write_attribute(name, out, |out| value.write(out));
    match (&node.content, field) {
        (NodeContent::Rect { x, .. } | NodeContent::Image { x, .. } | NodeContent::Icon { x, .. }, G::X) => {
            number("x", x, out);
        }
        (NodeContent::Rect { y, .. } | NodeContent::Image { y, .. } | NodeContent::Icon { y, .. }, G::Y) => {
            number("y", y, out);
        }
        (NodeContent::Rect { width, .. } | NodeContent::Image { width, .. }, G::Width) => number("width", width, out),
        (NodeContent::Rect { height, .. } | NodeContent::Image { height, .. }, G::Height) => {
            number("height", height, out);
        }
        (NodeContent::Icon { size, .. }, G::Width) => number("width", size, out),
        (NodeContent::Icon { size, .. }, G::Height) => number("height", size, out),
        (NodeContent::Rect { rx: Some(rx), .. } | NodeContent::Ellipse { rx, .. }, G::Rx) => number("rx", rx, out),
        (NodeContent::Ellipse { ry, .. }, G::Ry) => number("ry", ry, out),
        (NodeContent::Circle { cx: Some(cx), .. }, G::Cx) => number("cx", cx, out),
        (NodeContent::Circle { cy: Some(cy), .. }, G::Cy) => number("cy", cy, out),
        (NodeContent::Circle { radius, .. }, G::Radius) => number("r", radius, out),
        (
            NodeContent::Text(PlacedText {
                emission: TextEmission::Plain { x, .. } | TextEmission::Line { x, .. } | TextEmission::Tailed { x, .. },
                ..
            }),
            G::X,
        ) => number("x", x, out),
        (
            NodeContent::Text(PlacedText {
                emission: TextEmission::Plain { y, .. } | TextEmission::Line { y, .. } | TextEmission::Tailed { y, .. },
                ..
            }),
            G::Y,
        ) => number("y", y, out),
        (NodeContent::Path(path), G::D) => write_attribute("d", out, |out| write_path(path, out)),
        (NodeContent::Polygon(points), G::Points) => write_attribute("points", out, |out| {
            for (index, point) in points.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                write_pair(point, out);
            }
        }),
        (NodeContent::Image { aspect, .. }, G::Aspect) => write_attribute("preserveAspectRatio", out, |out| {
            out.push_str(match aspect {
                Aspect::Stretch => "none",
                Aspect::Meet => "xMidYMid meet",
            });
        }),
        (NodeContent::Icon { name, .. }, G::Icon) => write_attribute("data-icon", out, |out| esc_text(name, out)),
        (content, field) => unreachable!("the builder never records {field:?} for {content:?}"),
    }
}

/// Writes one attribute of a node, where the node sets it.
fn write_field(scene: &Scene, id: NodeId, field: Field, out: &mut String) {
    let node = scene.node(id);
    let properties = &node.properties;
    let number = |name: &str, value: Option<&Number>, out: &mut String| {
        if let Some(value) = value {
            write_attribute(name, out, |out| value.write(out));
        }
    };
    let link = match &node.content {
        NodeContent::Link { link, .. } => Some(link),
        _ => None,
    };
    match field {
        Field::Class => {
            if let Some(class) = &properties.class {
                write_attribute("class", out, |out| out.push_str(class));
            }
        }
        Field::DataId => {
            if let Some(part) = node.part {
                let entity = scene.entity(part);
                write_attribute("data-id", out, |out| {
                    if entity.decoded {
                        esc(&entity.author_text, out);
                    } else {
                        esc_text(&entity.author_text, out);
                    }
                });
            }
        }
        Field::Transform => match &properties.transform {
            Some(Transform::Translate(x, y)) => write_attribute("transform", out, |out| {
                out.push_str("translate(");
                write_pair(&[*x, *y], out);
                out.push(')');
            }),
            Some(Transform::Rotate { degrees, centre }) => write_attribute("transform", out, |out| {
                out.push_str("rotate(");
                degrees.write(out);
                out.push(' ');
                centre[0].write(out);
                out.push(' ');
                centre[1].write(out);
                out.push(')');
            }),
            None => {}
        },
        Field::Geometry(geometry) => write_geometry(node, geometry, out),
        Field::Fill | Field::Stroke => {
            let (name, value) = match field {
                Field::Fill => ("fill", properties.fill),
                _ => ("stroke", properties.stroke),
            };
            if let Some(value) = value.and_then(|value| paint(scene, value)) {
                write_attribute(name, out, |out| {
                    if let Some(gradient) = properties.gradient.filter(|_| field == Field::Fill) {
                        out.push_str("url(#");
                        out.push_str(&gradient_id(scene, gradient));
                        out.push_str(") ");
                    }
                    out.push_str(&value);
                });
            }
        }
        Field::FillOpacity => number("fill-opacity", properties.fill_opacity.as_ref(), out),
        Field::Opacity => number("opacity", properties.opacity.as_ref(), out),
        Field::StrokeWidth => number("stroke-width", properties.stroke_width.as_ref(), out),
        Field::DashArray => write_attribute("stroke-dasharray", out, |out| {
            for (index, length) in properties.dash.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                length.write(out);
            }
        }),
        Field::StrokeCap => match properties.cap {
            Some(LineCap::Butt) => out.push_str(" stroke-linecap=\"butt\""),
            Some(LineCap::Round) => out.push_str(" stroke-linecap=\"round\""),
            None => {}
        },
        Field::StrokeJoin => match properties.join {
            Some(LineJoin::Miter) => out.push_str(" stroke-linejoin=\"miter\""),
            Some(LineJoin::Round) => out.push_str(" stroke-linejoin=\"round\""),
            None => {}
        },
        Field::PointerEvents => {
            if properties.untouchable {
                out.push_str(" pointer-events=\"none\"");
            }
        }
        Field::FontSize => number("font-size", properties.font_size.as_ref(), out),
        Field::FontWeight => match properties.weight {
            Some(TextWeight::Title) => out.push_str(" font-weight=\"var(--_tw)\""),
            Some(TextWeight::Bold) => out.push_str(BOLD),
            Some(TextWeight::Heading) => out.push_str(" font-weight=\"var(--_hw)\""),
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
        Field::Style => {
            if let Some(style) = &properties.style {
                write_attribute("style", out, |out| style.write(out));
            }
        }
        Field::Href => {
            let href = match &node.content {
                NodeContent::Image { href, .. } => Some(href),
                _ => link.and_then(|link| link.href.as_ref()),
            };
            if let Some(href) = href {
                write_attribute("href", out, |out| esc(href, out));
            }
        }
        Field::Target => {
            if let Some(target) = link.and_then(|link| link.target) {
                write_attribute("target", out, |out| esc_str(target, out));
            }
        }
        Field::Rel => {
            if let Some(rel) = link.and_then(|link| link.rel.as_deref()) {
                write_attribute("rel", out, |out| esc_str(rel, out));
            }
        }
        Field::DataHref => {
            if let Some(note) = link.and_then(|link| link.note.as_ref()) {
                write_attribute("data-href", out, |out| esc(note, out));
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

fn write_text(scene: &Scene, text: &PlacedText, out: &mut String) {
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
        TextEmission::Tailed { class, fill, .. } => {
            out.push('>');
            let runs = text.label.lines.iter().flat_map(|line| &line.runs).collect::<Vec<_>>();
            if let Some((tail, head)) = runs.split_last() {
                for run in head {
                    esc(&run.text, out);
                }
                out.push_str("<tspan class=\"");
                out.push_str(class);
                out.push('"');
                if let Some(fill) = paint(scene, *fill) {
                    out.push_str(" fill=\"");
                    out.push_str(&fill);
                    out.push('"');
                }
                out.push('>');
                esc(&tail.text, out);
                out.push_str("</tspan>");
            }
        }
        TextEmission::Line { shared, .. } => {
            out.push_str(" xml:space=\"preserve\">");
            for run in text.label.lines.iter().flat_map(|line| &line.runs) {
                if own_emphasis(run.style, *shared).is_plain() {
                    esc(&run.text, out);
                } else {
                    out.push_str("<tspan");
                    write_run_style(run.style, *shared, out);
                    out.push('>');
                    esc(&run.text, out);
                    out.push_str("</tspan>");
                }
            }
        }
    }
    out.push_str("</text>");
}

// What every icon's box sets for the drawing the host puts in it.
const ICON_FRAME: &str = " viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"";

fn write_node(scene: &Scene, id: NodeId, out: &mut String) {
    let node = scene.node(id);
    let tag = match &node.content {
        NodeContent::Group(_) => "g",
        NodeContent::Link { .. } => "a",
        NodeContent::Path(_) => "path",
        NodeContent::Rect { .. } => "rect",
        NodeContent::Circle { .. } => "circle",
        NodeContent::Ellipse { .. } => "ellipse",
        NodeContent::Polygon(_) => "polygon",
        NodeContent::Text(_) => "text",
        NodeContent::Image { .. } => "image",
        NodeContent::Icon { .. } => "svg",
        NodeContent::Tooltip => "title",
        NodeContent::Gradients(_) => "defs",
    };
    out.push('<');
    out.push_str(tag);
    for &field in &node.emission {
        write_field(scene, id, field, out);
    }
    match &node.content {
        NodeContent::Group(children) | NodeContent::Link { children, .. } => {
            out.push('>');
            for &child in children {
                write_node(scene, child, out);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        NodeContent::Text(text) => write_text(scene, text, out),
        NodeContent::Icon { .. } => {
            out.push_str(ICON_FRAME);
            out.push_str("></svg>");
        }
        NodeContent::Tooltip => {
            out.push('>');
            write_tooltip(scene, node, out);
            out.push_str("</title>");
        }
        NodeContent::Gradients(gradients) => {
            out.push('>');
            let mut written = std::collections::HashSet::new();
            for &gradient in gradients {
                // Ids come from the gradient itself, so one that is already defined is the same.
                let id = gradient_id(scene, gradient);
                if written.insert(id.clone()) {
                    out.push_str(&format!("<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" "));
                    out.push_str(&gradient_stops(scene, gradient));
                    out.push_str("</linearGradient>");
                }
            }
            out.push_str("</defs>");
        }
        NodeContent::Path(_)
        | NodeContent::Rect { .. }
        | NodeContent::Circle { .. }
        | NodeContent::Ellipse { .. }
        | NodeContent::Polygon(_)
        | NodeContent::Image { .. } => {
            if node.properties.titled {
                out.push_str("><title>");
                write_tooltip(scene, node, out);
                out.push_str("</title></");
                out.push_str(tag);
                out.push('>');
            } else {
                out.push_str("/>");
            }
        }
    }
}

/// The tooltip of a node's entity, escaped.
fn write_tooltip(scene: &Scene, node: &Node, out: &mut String) {
    if let Some(tooltip) = node.part.and_then(|part| scene.entity(part).tooltip_text()) {
        esc(&tooltip, out);
    }
}

/// What follows a gradient's `gradientUnits`, as Pele's sankey writes it: where it runs, then
/// its two stops.
fn gradient_stops(scene: &Scene, id: GradientId) -> String {
    let gradient = scene.gradient(id);
    let mut out = String::from("x1=\"");
    gradient.x[0].write(&mut out);
    out.push_str("\" x2=\"");
    gradient.x[1].write(&mut out);
    out.push_str("\">");
    for (offset, stop) in gradient.stops.iter().enumerate() {
        let colour = paint(scene, *stop).unwrap_or_default();
        out.push_str(&format!("<stop offset=\"{offset}\" stop-color=\"{colour}\"/>"));
    }
    out
}

/// Pele's `contentId`: two 32-bit hashes of what defines a gradient, so different gradients
/// practically never share an id.
fn content_id(text: &str) -> String {
    let (mut a, mut b) = (0x811c_9dc5_u32, 5381_u32);
    for unit in text.encode_utf16() {
        a = (a ^ u32::from(unit)).wrapping_mul(0x0100_0193);
        b = b.wrapping_mul(33).wrapping_add(u32::from(unit));
    }
    format!("{}{}", radix36(a), radix36(b))
}

/// `n.toString(36)`.
fn radix36(mut n: u32) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(char::from_digit(n % 36, 36).expect("a digit below 36"));
        n /= 36;
        if n == 0 {
            break;
        }
    }
    digits.iter().rev().collect()
}

/// The id of a gradient: the drawing's id prefix, its type and a hash of the gradient.
fn gradient_id(scene: &Scene, id: GradientId) -> String {
    let prefix = scene.document.id_prefix.as_deref().unwrap_or_default();
    format!("{prefix}-{}-{}", scene.document.kind.name(), content_id(&gradient_stops(scene, id)))
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
    root_style(scene, body).write(&mut out);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Palette;

    /// The fallback Pele's SVG spells for a role, through the root's private aliases.
    fn fallback(role_spelling: &str) -> String {
        let spelling = match ROOT_STYLE.iter().find(|(alias, _)| role_spelling == format!("var({alias})")) {
            Some((_, value)) => *value,
            None => role_spelling,
        };
        let (_, rest) = spelling.split_once(',').expect("a variable with a fallback");
        rest.trim_end_matches(')').to_owned()
    }

    fn bytes(hex: &str) -> [u8; 4] {
        let digits: String = hex[1..].chars().flat_map(|c| if hex.len() == 4 { vec![c, c] } else { vec![c] }).collect();
        let byte = |i: usize| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).expect("hex");
        [byte(0), byte(1), byte(2), 255]
    }

    #[test]
    fn the_pele_palette_is_the_fallbacks_the_svg_spells() {
        let pele = Palette::pele();
        let roles = [
            Role::Background,
            Role::Foreground,
            Role::Border,
            Role::OnLight,
            Role::OnDark,
            Role::Line,
            Role::Surface,
            Role::SurfaceAlt,
            Role::Muted,
            Role::Accent,
            Role::Critical,
        ];
        for named in roles {
            let spelled = fallback(role(named));
            assert_eq!(pele.role(named).to_srgb8(), bytes(&spelled), "{named:?} falls back to {spelled}");
        }
        for (index, hex) in PELE_SERIES.iter().enumerate() {
            assert_eq!(pele.series(index).to_srgb8(), bytes(hex), "series {}", index + 1);
        }
    }
}
