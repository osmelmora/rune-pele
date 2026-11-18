//! Pele's `renderSankey` (`src/diagrams/sankey/render.ts`): the laid out bands and bars, and a
//! label beside each bar, moved to the other side or a line along where it would cover another.

use super::SankeyModel;
use super::layout::{Alignment, LayoutOptions, SankeyLayout, layout_sankey};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities};
use crate::measure::{Emphasis, layout_label, width};
use crate::scene::{
    AnchorKind, Entity, EntityName, Facet, Gradient, GradientId, LayerSpec, LineJoin, Number, Paint, PathProgram,
    Point, Role, Scene, SceneBuilder, SeriesSlot, Size, Style, TitleLayout, resolve_style,
};
use crate::{Diagnostic, ErrorCode, RenderError, fit, js};
use std::collections::HashMap;

const LABEL_GAP: f64 = 6.0;
const VALUE_GAP: f64 = 15.0;
const LABEL_ROOM: f64 = 1.8;
const CAREFUL: usize = 300;

fn number_option(conf: &JsObject, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match conf.get(key) {
        Some(&JsValue::Number(value)) if value >= min => js::min(value, max),
        _ => fallback,
    }
}

fn text_option(conf: &JsObject, key: &str) -> JsText {
    match conf.get(key) {
        Some(JsValue::String(text)) => text.clone(),
        Some(value @ JsValue::Number(number)) if number.is_finite() => value.to_js_string(),
        _ => JsText::default(),
    }
}

/// Two decimals, as Mermaid shows values; numbers too large for that are left as they are.
fn rounded(value: f64) -> f64 {
    if value.abs() < 1e15 { js::round(value * 100.0) / 100.0 } else { value }
}

/// A colour from the configuration, as an inline style that overrides the series colour.
fn fill_style(colour: &JsValue) -> Option<Style> {
    let JsValue::String(colour) = colour else { return None };
    let mut declaration = JsText::from("fill:");
    declaration.push_units(colour);
    resolve_style([&declaration]).shape
}

/// The colour a fill style gives, as `/^ style="fill:(.*);"$/` reads it back: the whole of one
/// `fill` declaration, unless a line break ends the match early.
fn style_colour(style: &Style) -> Option<&str> {
    match style.declarations.as_slice() {
        [only] if only.property == "fill" && style.terminated => {
            (!only.written.contains(['\n', '\r', '\u{2028}', '\u{2029}'])).then_some(only.written.as_str())
        }
        _ => None,
    }
}

/// A value as it was before `esc`, which a fill style keeps escaped: the style's values pass
/// only where they hold no markup, so `&amp;` and `&quot;` are its only escapes.
fn unescaped(written: &str) -> Vec<u16> {
    let mut out = String::with_capacity(written.len());
    let mut rest = written;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let (unit, length) = if rest.starts_with("&amp;") {
            ('&', 5)
        } else if rest.starts_with("&quot;") {
            ('"', 6)
        } else {
            ('&', 1)
        };
        out.push(unit);
        rest = &rest[length..];
    }
    out.push_str(rest);
    out.encode_utf16().collect()
}

/// What the label of a node reads and where it sits.
struct Placed {
    value: JsText,
    width: f64,
    after: bool,
    left: f64,
    y: f64,
}

fn draw(
    model: &SankeyModel,
    conf: &JsObject,
    cx: &DrawContext<'_>,
    chart_width: f64,
    natural: f64,
) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let least = js::round(size * 0.625);
    // Labels shrink with a chart narrowed to fit, as they would if the whole picture were scaled down.
    let mut small = js::max(least, js::round(size * 0.875 * js::min(1.0, chart_width / natural)));
    let pad = cx.options.padding_or_default();
    let plain = Emphasis::default();

    let chart_height = number_option(conf, "height", 400.0, 1.0, 10000.0);
    let node_width = js::min(number_option(conf, "nodeWidth", 10.0, 1.0, 200.0), chart_width);
    let show_values = conf.get("showValues") != Some(&JsValue::Bool(false));
    let prefix = decode_entities(&text_option(conf, "prefix"));
    let suffix = decode_entities(&text_option(conf, "suffix"));
    let outlined = matches!(conf.get("labelStyle"), Some(JsValue::String(style)) if style.is_str("outlined"));
    let alignment = match conf.get("nodeAlignment") {
        Some(JsValue::String(name)) if name.is_str("left") => Alignment::Left,
        Some(JsValue::String(name)) if name.is_str("right") => Alignment::Right,
        Some(JsValue::String(name)) if name.is_str("center") => Alignment::Center,
        _ => Alignment::Justify,
    };
    let link_colour = match conf.get("linkColor") {
        Some(JsValue::String(colour)) => colour.clone(),
        _ => JsText::from("gradient"),
    };

    let padding = number_option(conf, "nodePadding", 12.0, 0.0, 200.0) + if show_values { VALUE_GAP } else { 0.0 };
    let lay = |label_size: f64| -> Result<SankeyLayout, RenderError> {
        let options = LayoutOptions {
            width: chart_width,
            height: chart_height,
            node_width,
            node_padding: padding,
            label_height: label_size * 1.15,
            alignment,
        };
        layout_sankey(model, &options).map_err(|stuck| {
            let mut message = String::from("Sankey diagrams cannot have circular links, and \"");
            message.push_str(&model.nodes[stuck].to_display());
            message.push_str("\" is part of one.");
            RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message))
        })
    };
    let mut layout = lay(small)?;
    // Labels also shrink with the most crowded column, so each keeps about LABEL_ROOM lines.
    let mut rows: HashMap<usize, usize> = HashMap::new();
    let mut fullest = 1;
    for node in &layout.nodes {
        let count = rows.entry(node.layer).or_insert(0);
        *count += 1;
        fullest = fullest.max(*count);
    }
    let crowded = js::max(least, js::round(chart_height / fullest as f64 / LABEL_ROOM));
    if crowded < small {
        small = crowded;
        layout = lay(small)?;
    }
    let SankeyLayout { nodes, links } = layout;
    let line_height = small * 1.5;

    let mut custom: HashMap<JsText, Style> = HashMap::new();
    if let Some(JsValue::Object(colours)) = conf.get("nodeColors") {
        for (id, colour) in colours.iter() {
            if let Some(style) = fill_style(colour) {
                custom.insert(id.clone(), style);
            }
        }
    }
    let node_style = |i: usize| custom.get(&model.nodes[i]).cloned();

    // Labels sit beside the bars, on the side that faces the middle of the chart unless that
    // side is crowded.
    let (mut central, mut most) = (0, 0.0);
    for node in &nodes {
        if node.value > most {
            most = node.value;
            central = node.layer;
        }
    }
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (0.0, chart_width, 0.0, chart_height);
    let names: Vec<Vec<u16>> = model.nodes.iter().map(|id| decode_entities(id)).collect();
    let mut labels: Vec<Placed> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let value = if show_values {
                let mut value = JsText::from(prefix.as_slice());
                value.push_str(&js::to_string(rounded(node.value)));
                value.push_units(&suffix);
                value
            } else {
                JsText::default()
            };
            let text = if value.is_empty() { names[i].clone() } else { [&names[i][..], &[32], &value[..]].concat() };
            let width = width(cx.measure, &text, small, plain);
            let after = if outlined { node.layer >= central } else { node.x < chart_width / 2.0 };
            let x = if after { node.x + node_width + LABEL_GAP } else { node.x - LABEL_GAP };
            let y = node.y + node.height / 2.0;
            min_x = js::min(min_x, if after { x } else { x - width });
            max_x = js::max(max_x, if after { x + width } else { x });
            min_y = js::min(min_y, y - small * 0.75);
            max_y = js::max(max_y, y + small * 0.75);
            Placed { value, width, after, left: if after { x } else { x - width }, y }
        })
        .collect();
    // A label moves at most one line; retain two bands on each side.
    let band = |y: f64| (y / line_height).floor() as i64;
    let mut bands: HashMap<i64, Vec<usize>> = HashMap::new();
    if nodes.len() <= CAREFUL {
        for (index, label) in labels.iter().enumerate() {
            let at = band(label.y);
            for key in at - 2..=at + 2 {
                bands.entry(key).or_default().push(index);
            }
        }
    }

    // Each label's place, settled in node order: a moved label is where later ones look for it.
    for i in 0..nodes.len() {
        let node = nodes[i];
        let overlaps = |labels: &[Placed], left: f64, y: f64| {
            let label = &labels[i];
            bands.get(&band(y)).is_some_and(|nearby| {
                nearby.iter().any(|&index| {
                    let other = &labels[index];
                    index != i
                        && (other.y - y).abs() < line_height
                        && left < other.left + other.width + LABEL_GAP
                        && left + label.width > other.left - LABEL_GAP
                })
            })
        };
        // Try the other side, then an adjacent line when both sides are crowded.
        if nodes.len() <= CAREFUL && overlaps(&labels, labels[i].left, labels[i].y) {
            'place: for dy in [0.0, -line_height, line_height] {
                let side = labels[i].after;
                for after in [side, !side] {
                    let label_width = labels[i].width;
                    let left = if after { node.x + node_width + LABEL_GAP } else { node.x - LABEL_GAP - label_width };
                    let y = labels[i].y + dy;
                    if overlaps(&labels, left, y)
                        || left < min_x
                        || left + label_width > max_x
                        || y - small * 0.75 < min_y
                        || y + small * 0.75 > max_y
                    {
                        continue;
                    }
                    let covers = nodes.iter().enumerate().any(|(j, bar)| {
                        j != i
                            && left < bar.x + node_width + LABEL_GAP
                            && left + label_width > bar.x - LABEL_GAP
                            && y - small * 0.75 < bar.y + bar.height
                            && y + small * 0.75 > bar.y
                    });
                    if covers {
                        continue;
                    }
                    // A label stays nearer its own bar than any other in the column, so it can't name that one.
                    let own = js::max(js::max(0.0, node.y - y), y - node.y - node.height);
                    let nearer = nodes.iter().enumerate().any(|(j, bar)| {
                        j != i && bar.x == node.x && js::max(js::max(0.0, bar.y - y), y - bar.y - bar.height) <= own
                    });
                    if nearer {
                        continue;
                    }
                    let label = &mut labels[i];
                    label.left = left;
                    label.y = y;
                    label.after = after;
                    break 'place;
                }
            }
        }
    }

    let gradient_prefix = cx.options.id_prefix.as_deref().unwrap_or_default();
    let gradient = link_colour.is_str("gradient")
        && !gradient_prefix.is_empty()
        && gradient_prefix.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
    let fixed = if ["gradient", "source", "target"].iter().any(|mode| link_colour.is_str(mode)) {
        None
    } else {
        fill_style(&JsValue::String(link_colour.clone()))
    };

    let title = layout_label(
        model.title.as_deref().unwrap_or_default(),
        false,
        cx.measure,
        size,
        fit::title_room(cx.options),
        Emphasis::BOLD,
    );
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let spread = max_x - min_x;
    let inner = js::max(spread, title.width);
    let ox = pad - min_x + (inner - spread) / 2.0;
    let oy = pad + title_height - min_y;
    let total_width = (inner + 2.0 * pad).ceil();
    let total_height = (max_y - min_y + title_height + 2.0 * pad).ceil();

    let mut out = SceneBuilder::new(cx, Size { width: total_width, height: total_height }, Default::default());
    let title_at = Point { x: total_width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at: title_at });

    // The colour of each node, as the series or as what its fill style writes.
    let mut colours: Vec<Option<Paint>> = vec![None; nodes.len()];
    let mut node_colour = |out: &mut SceneBuilder, i: usize| -> Paint {
        *colours[i].get_or_insert_with(|| match node_style(i).as_ref().and_then(style_colour) {
            Some(written) => out.colour(&unescaped(written), Role::Surface),
            None => Paint::Series(SeriesSlot::new(i)),
        })
    };
    // The gradients and fills of the links, decided before anything is drawn so that the
    // gradients can be defined ahead of the links that use them.
    struct LinkPaint {
        fill: Paint,
        gradient: Option<GradientId>,
        style: Option<Style>,
    }
    let mut paints = Vec::with_capacity(links.len());
    for link in &links {
        let end = if link_colour.is_str("target") { link.target } else { link.source };
        let x0 = nodes[link.source].x + node_width;
        let x1 = nodes[link.target].x;
        if gradient {
            let from = node_colour(&mut out, link.source);
            let to = node_colour(&mut out, link.target);
            let id = out.gradient(Gradient { x: [Number::num(x0), Number::num(x1)], stops: [from, to] });
            paints.push(LinkPaint { fill: from, gradient: Some(id), style: None });
        } else {
            let style = fixed.clone().or_else(|| node_style(end));
            paints.push(LinkPaint { fill: Paint::Series(SeriesSlot::new(end)), gradient: None, style });
        }
    }

    out.layer(LayerSpec::group().translate(Point { x: ox, y: oy }).keep_empty(), |out| {
        let defined: Vec<GradientId> = paints.iter().filter_map(|paint| paint.gradient).collect();
        if !defined.is_empty() {
            out.gradients(defined);
        }
        let node_parts: Vec<_> = model.nodes.iter().map(|id| out.part(Entity::node(id))).collect();
        out.layer(LayerSpec::class("pele-sankey-links").fill_opacity(Number::raw(0.4)).keep_empty(), |out| {
            let mut seen: HashMap<(usize, usize), u32> = HashMap::new();
            for (link, paint) in links.iter().zip(&paints) {
                let half = js::max(link.thickness, 1.0) / 2.0;
                let x0 = nodes[link.source].x + node_width;
                let x1 = nodes[link.target].x;
                let mid = Number::num((x0 + x1) / 2.0);
                let (top0, top1) = (Number::num(link.source_y - half), Number::num(link.target_y - half));
                let (bottom0, bottom1) = (Number::num(link.source_y + half), Number::num(link.target_y + half));
                let band = PathProgram::default()
                    .m(Number::num(x0), top0)
                    .c([mid, top0], [mid, top1], [Number::num(x1), top1])
                    .l(Number::num(x1), bottom1)
                    .c([mid, bottom1], [mid, bottom0], [Number::num(x0), bottom0])
                    .z();
                let (from, to) = (&names[link.source], &names[link.target]);
                let mut written = JsText::from(from.as_slice());
                written.push_str("->");
                written.push_units(to);
                let mut tooltip = JsText::from(from.as_slice());
                tooltip.push_str(" \u{2192} ");
                tooltip.push_units(to);
                tooltip.push_str(": ");
                tooltip.push_units(&prefix);
                tooltip.push_str(&js::to_string(rounded(link.value)));
                tooltip.push_units(&suffix);
                let rank = seen.entry((link.source, link.target)).or_insert(0);
                let name = EntityName::Between {
                    from: model.nodes[link.source].to_vec(),
                    to: model.nodes[link.target].to_vec(),
                    kind: "link".to_owned(),
                };
                let part = out.part(Entity::edge(&written, name, *rank).tooltip(Some(&tooltip)).decoded());
                *rank += 1;
                out.relation(part, node_parts[link.source], node_parts[link.target]);
                let shape =
                    out.element(part, Facet::Shape).class("pele-sankey-link").data_id().path(band).fill(paint.fill);
                let shape = match paint.gradient {
                    Some(gradient) => shape.gradient(gradient),
                    None => shape,
                };
                shape.style(paint.style.clone()).titled().finish();
            }
        });
        out.layer(LayerSpec::class("pele-sankey-nodes").keep_empty(), |out| {
            for (i, node) in nodes.iter().enumerate() {
                let height = js::max(node.height, 1.0);
                out.element(node_parts[i], Facet::Shape)
                    .class("pele-sankey-node")
                    .data_id()
                    .rect(
                        [Number::num(node.x), Number::num(node.y)],
                        [Number::num(node_width), Number::num(height)],
                        Some(Number::num(js::min(2.0, height / 2.0))),
                    )
                    .fill(Paint::Series(SeriesSlot::new(i)))
                    .style(node_style(i))
                    .finish();
            }
        });
        out.layer(LayerSpec::class("pele-labels").font_size(Number::raw(small)).keep_empty(), |out| {
            for (i, node) in nodes.iter().enumerate() {
                let label = &labels[i];
                let part = node_parts[i];
                let x = if label.after { node.x + node_width + LABEL_GAP } else { node.x - LABEL_GAP };
                let at = [Number::num(x), Number::num(label.y + small * 0.35)];
                let anchor = if label.after { AnchorKind::Start } else { AnchorKind::End };
                let name = &names[i];
                out.layer(LayerSpec::class("pele-label").part(part, Facet::Label).data_id(), |out| {
                    if outlined {
                        let text = if label.value.is_empty() {
                            name.clone()
                        } else {
                            [&name[..], &[32], &label.value[..]].concat()
                        };
                        out.text(part, Facet::Halo)
                            .at(at)
                            .anchor(anchor)
                            .fill(Some(Paint::None))
                            .stroke(Paint::Role(Role::Background))
                            .stroke_width(Number::raw(3.0))
                            .join(LineJoin::Round)
                            .finish(&text, label.width, small);
                    }
                    let text = out.text(part, Facet::Label).at(at).anchor(anchor);
                    if label.value.is_empty() {
                        text.finish(name, label.width, small);
                    } else {
                        let mut head = name.clone();
                        head.push(32);
                        let runs = [
                            (&head[..], width(cx.measure, &head, small, plain)),
                            (&label.value[..], width(cx.measure, &label.value, small, plain)),
                        ];
                        text.finish_tailed(&runs, "pele-sankey-value", Paint::Role(Role::Muted), label.width, small);
                    }
                });
            }
        });
    });
    out.finish()
}

pub(crate) struct Sankey;

impl Diagram for Sankey {
    type Model = SankeyModel;

    fn parse(cx: &ParseContext<'_>) -> Result<SankeyModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderSankey`: the chart at its configured width, or narrower to fit the host.
    fn scene(model: &SankeyModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        let empty = JsObject::default();
        let conf = cx.config.get("sankey").and_then(JsValue::as_record).unwrap_or(&empty);
        let natural = number_option(conf, "width", 600.0, 1.0, 10000.0);
        fit::fit_width(cx.options, natural, 160.0, |chart_width, _| draw(model, conf, cx, chart_width, natural))
    }
}
