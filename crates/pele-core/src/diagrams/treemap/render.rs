//! Pele's `renderTreemap` (`treemap/render.ts`): each section as a tint of its branch's colour with
//! its name and value in a header, and each leaf as a box of that colour with its name and value
//! fitted inside.

use super::format::Formatter;
use super::layout::{Insets, Tile, layout};
use super::{Content, TreemapModel, TreemapNode};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::fit::{cap_width, title_room};
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities};
use crate::measure::{Emphasis, fit_text, layout_label, width as measured};
use crate::scene::{
    Accessible, AnchorKind, AuthorClasses, Entity, Facet, LayerSpec, Number, Paint, PartId, Point, Role, Scene,
    SceneBuilder, SeriesSlot, Size, Style, TextWeight, TitleLayout, ink_on, resolve_style, xml_drops,
};
use crate::{RenderError, js};
use std::collections::HashMap;

const HEADER: f64 = 28.0;
const SHORT_HEADER: f64 = 18.0;
const SIDE: f64 = 6.0;
const LINE: f64 = 1.25;

/// The `treemap` section of the config, where it is an object.
fn section<'a>(cx: &DrawContext<'a>) -> Option<&'a JsObject> {
    cx.config.get("treemap").and_then(JsValue::as_record)
}

/// Pele's `option`: a number of the `treemap` section, if it is at least `min`, and no more than `max`.
fn option(cx: &DrawContext<'_>, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match section(cx).and_then(|section| section.get(key)).and_then(JsValue::as_number) {
        Some(value) if value >= min => js::min(value, max),
        _ => fallback,
    }
}

/// Pele's `leafCount`: the leaves with a positive value.
fn leaf_count(roots: &[TreemapNode]) -> usize {
    let mut count = 0;
    let mut stack: Vec<&TreemapNode> = roots.iter().collect();
    while let Some(node) = stack.pop() {
        match &node.content {
            Content::Section(children) => stack.extend(children),
            Content::Leaf(value) if value.is_finite() && *value > 0.0 => count += 1,
            Content::Leaf(_) => {}
        }
    }
    count
}

/// Pele's `titlePart`, `esc(decodeEntities(raw).trim()).trim()`, as the text it escapes to.
fn title_part(raw: &[u16]) -> Vec<u16> {
    let decoded = decode_entities(raw);
    let mut kept = Vec::with_capacity(decoded.len());
    for c in char::decode_utf16(decoded.trim_js().iter().copied()).flatten().filter(|&c| !xml_drops(c)) {
        let mut units = [0; 2];
        kept.extend_from_slice(c.encode_utf16(&mut units));
    }
    kept.trim_js().to_vec()
}

/// Each node's place among the nodes of its kind with the same name, in source order, so that
/// its key does not move when a value change reorders the boxes.
fn ranks(roots: &[TreemapNode]) -> HashMap<*const TreemapNode, u32> {
    let mut seen: HashMap<(bool, &[u16]), u32> = HashMap::new();
    let mut out = HashMap::new();
    let mut stack: Vec<&TreemapNode> = roots.iter().rev().collect();
    while let Some(node) = stack.pop() {
        let is_section = matches!(node.content, Content::Section(_));
        let rank = seen.entry((is_section, &node.name)).or_insert(0);
        out.insert(node as *const TreemapNode, *rank);
        *rank += 1;
        if let Content::Section(children) = &node.content {
            stack.extend(children.iter().rev());
        }
    }
    out
}

/// One line of text as Pele writes it here: a fitted run at a baseline point.
struct Line {
    class: Option<&'static str>,
    facet: Facet,
    text: JsText,
    width: f64,
    size: f64,
    at: [Number; 2],
    weight: Option<TextWeight>,
    font_size: Option<Number>,
    anchor: Option<AnchorKind>,
    fill: Option<Paint>,
}

fn write_line(out: &mut SceneBuilder, part: PartId, line: Line, text_style: Option<&Style>) {
    let mut text = out.text(part, line.facet);
    if let Some(class) = line.class {
        text = text.class(class);
    }
    text = text.at(line.at);
    if let Some(weight) = line.weight {
        text = text.weight(weight);
    }
    if let Some(size) = line.font_size {
        text = text.font_size(size);
    }
    if let Some(anchor) = line.anchor {
        text = text.anchor(anchor);
    }
    text.fill(line.fill).style(text_style.cloned()).finish(&line.text, line.width, line.size);
}

/// Pele's `corners`: the theme's radius, capped at a quarter of a small box's shorter side.
fn corners(shape: Option<&Style>, b: &Tile<'_>) -> Style {
    let cap = js::min(b.w, b.h) / 4.0;
    if cap < 4.0 { Style::rounded_at_most(shape, cap) } else { Style::rounded(shape) }
}

#[allow(clippy::too_many_lines, reason = "one pass of Pele's renderTreemap, whose steps share its sizes")]
fn draw(model: &TreemapModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let tiny = js::round(size * 0.75);
    let least = js::round(size * 0.5625);
    let measurer = cx.measure;
    let pad = cx.options.padding.unwrap_or_else(|| option(cx, "diagramPadding", 8.0, 0.0, 2000.0));

    // Mermaid sizes the diagram as ten times `nodeWidth` by ten times `nodeHeight`.
    let width = js::round(cap_width(cx.options, option(cx, "nodeWidth", 64.0, 1.0, 400.0) * 10.0, 200.0, 2.0 * pad));
    let height = js::round(option(cx, "nodeHeight", 40.0, 1.0, 400.0) * 10.0);
    let show_values = section(cx).and_then(|section| section.get("showValues")) != Some(&JsValue::Bool(false));
    let spec: JsText = match section(cx).and_then(|section| section.get("valueFormat")).and_then(JsValue::as_text) {
        Some(spec) => spec.clone(),
        None => ",".into(),
    };
    let format = Formatter::new(&spec);
    // Reduce gaps and insets as average leaf area shrinks.
    let leaves = leaf_count(&model.roots).max(1) as f64;
    let crowd = js::min(1.0, js::max(0.0, js::log10((width * height) / leaves / 800.0)));
    let insets = Insets {
        gap: option(cx, "padding", js::round(2.0 + 2.0 * crowd), 0.0, 200.0),
        header: HEADER,
        short: SHORT_HEADER,
        side: js::round(3.0 + 3.0 * crowd),
    };
    let laid = layout(&model.roots, width, height, insets);

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measurer, size, title_room(cx.options), Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 8.0 } else { 0.0 };
    let left = pad + js::max(0.0, (title.width - width) / 2.0);
    let top = pad + title_height;

    let total = Size {
        width: (js::max(width, title.width) + 2.0 * pad).ceil(),
        height: (height + title_height + 2.0 * pad).ceil(),
    };
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, total, accessible);
    out.title(&TitleLayout { at: Point { x: total.width / 2.0, y: pad + title.height / 2.0 }, label: title });

    let ranks = ranks(&model.roots);
    let is_section = |at: &usize| matches!(laid.tiles[*at].node.content, Content::Section(_));
    let (sections, leaves): (Vec<usize>, Vec<usize>) = laid.placed.iter().copied().partition(is_section);

    // Draws one box: its group, its tooltip where Pele writes one, its shape and its text.
    let draw_box = |out: &mut SceneBuilder, at: usize| {
        let b = &laid.tiles[at];
        let node = b.node;
        let style = resolve_style(node.css_compiled_styles.iter().flatten());
        let slot = SeriesSlot::new(b.series);
        let colour = Paint::Series(slot);
        let (x, y) = (left + b.x, top + b.y);
        let name: Vec<u16> = decode_entities(&node.name).trim_js().to_vec();
        let value: Vec<u16> = if show_values { format.format(b.value).encode_utf16().collect() } else { Vec::new() };
        // Include headerless ancestors in tooltips because their children cover them.
        let own = title_part(&node.name);
        let mut path: Vec<Vec<u16>> = if own.is_empty() { Vec::new() } else { vec![own.clone()] };
        let mut parent = b.parent;
        while let Some(p) = parent.map(|p| &laid.tiles[p]).filter(|p| !p.headed) {
            let section = title_part(&p.node.name);
            if !section.is_empty() {
                path.insert(0, section);
            }
            parent = p.parent;
        }
        let separator: Vec<u16> = " \u{203a} ".encode_utf16().collect();
        let full: Vec<u16> = path.join(separator.as_slice());
        // Inline font sizes override SVG attributes, so truncate instead of shrinking.
        let fixed = style.font_size;
        let text_style = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
        let shape = style.shape.as_ref();
        let fills = shape.is_some_and(Style::sets_fill);
        // The text colour Pele writes, where it writes one.
        let written_ink = if fills {
            ink_on(shape, style.text.as_ref()).map(Paint::Role)
        } else {
            cx.palette.ink(slot).map(|_| Paint::OnSeries(slot))
        };
        let text = style.text.as_ref();
        let rect = |b: &Tile<'_>| ([Number::num(x), Number::num(y)], [Number::num(b.w), Number::num(b.h)]);

        let mut lines = Vec::new();
        let drawn: Option<JsText> = match &node.content {
            Content::Section(_) => {
                let mut drawn = None;
                if b.headed {
                    let short = b.header < HEADER;
                    let label_size = fixed.unwrap_or(if short { tiny } else { small });
                    let value_size = fixed.unwrap_or(tiny);
                    let room = b.w - 2.0 * SIDE - 4.0;
                    let value_width =
                        if value.is_empty() { 0.0 } else { measured(measurer, &value, value_size, text_style) };
                    let with_value = !short && value_width > 0.0 && room - value_width - 12.0 >= 3.0 * label_size;
                    let label = if label_size * LINE < b.header {
                        let max = if with_value { room - value_width - 12.0 } else { room };
                        let bold = Emphasis { bold: true, ..text_style };
                        fit_text(&name, max, label_size, label_size, measurer, bold)
                    } else {
                        None
                    };
                    let baseline = y + b.header / 2.0 + 1.0;
                    if let Some(label) = &label {
                        lines.push(Line {
                            class: None,
                            facet: Facet::Label,
                            text: label.text.clone(),
                            width: label.width,
                            size: label_size,
                            at: [Number::num(x + SIDE + 2.0), Number::num(baseline + label_size * 0.35)],
                            weight: Some(TextWeight::Heading),
                            font_size: short.then(|| Number::raw(label_size)),
                            anchor: None,
                            fill: written_ink,
                        });
                    }
                    if with_value && value_size * LINE <= HEADER {
                        lines.push(Line {
                            class: Some("pele-value"),
                            facet: Facet::Attribute,
                            text: value.as_slice().into(),
                            width: value_width,
                            size: value_size,
                            at: [Number::num(x + b.w - SIDE - 2.0), Number::num(baseline + value_size * 0.35)],
                            weight: None,
                            font_size: Some(Number::raw(tiny)),
                            anchor: Some(AnchorKind::End),
                            fill: Some(written_ink.unwrap_or(Paint::Role(Role::Muted))),
                        });
                    }
                    drawn = label.map(|label| label.text);
                }
                drawn
            }
            Content::Leaf(_) => {
                // Leaves sit on their series colour, which Pele's text colour contrasts with
                // wherever the palette knows how.
                let ink = if fills { written_ink } else { Some(Paint::OnSeries(slot)) };
                let room = b.w - 8.0;
                let smallest = fixed.unwrap_or(least);
                let tallest = match fixed {
                    None => js::min(size, ((b.h - 4.0) / LINE).floor()),
                    Some(fixed) if fixed * LINE <= b.h - 4.0 => fixed,
                    Some(_) => 0.0,
                };
                let label = if tallest >= smallest {
                    fit_text(&name, room, tallest, smallest, measurer, text_style)
                } else {
                    None
                };
                if let Some(label) = &label {
                    let centre = Number::num(x + b.w / 2.0);
                    let cy = y + b.h / 2.0;
                    let value_size = fixed.unwrap_or(js::max(least, js::round(label.size * 0.8)));
                    let both = (label.size + value_size) * LINE;
                    let value_width =
                        if value.is_empty() { 0.0 } else { measured(measurer, &value, value_size, text_style) };
                    let name_line = |y: f64| Line {
                        class: None,
                        facet: Facet::Label,
                        text: label.text.clone(),
                        width: label.width,
                        size: label.size,
                        at: [centre, Number::num(y + label.size * 0.35)],
                        weight: None,
                        font_size: Some(Number::num(label.size)),
                        anchor: None,
                        fill: ink,
                    };
                    if !value.is_empty() && both <= b.h - 4.0 && value_width <= room {
                        let first = cy - both / 2.0 + (label.size * LINE) / 2.0;
                        let second = cy + both / 2.0 - (value_size * LINE) / 2.0;
                        lines.push(name_line(first));
                        lines.push(Line {
                            class: Some("pele-value"),
                            facet: Facet::Attribute,
                            text: value.as_slice().into(),
                            width: value_width,
                            size: value_size,
                            at: [centre, Number::num(second + value_size * 0.35)],
                            weight: None,
                            font_size: Some(Number::num(value_size)),
                            anchor: None,
                            fill: ink,
                        });
                    } else {
                        lines.push(name_line(cy));
                    }
                }
                label.map(|label| label.text)
            }
        };

        let titled = !(full.is_empty() || (drawn.as_deref() == Some(name.as_slice()) && full == own));
        let rank = ranks[&(node as *const TreemapNode)];
        let (own_class, entity) = match node.content {
            Content::Section(_) => ("pele-node pele-section", Entity::group(&node.name)),
            Content::Leaf(_) => ("pele-node pele-leaf", Entity::node(&node.name)),
        };
        let entity = entity.ranked(rank);
        let part = out.part(if titled { entity.decoded_tooltip(full.into()) } else { entity });
        let classes = AuthorClasses::new(node.class_selector.as_deref().unwrap_or_default());
        out.layer(LayerSpec::classes(own_class, &classes).part(part, Facet::Shape).data_id(), |out| {
            if titled {
                out.tooltip(part);
            }
            let (origin, extent) = rect(b);
            let shape_element = out.element(part, Facet::Shape).rect(origin, extent, None);
            match &node.content {
                // A section is a tint of its colour, unless a class gives it a fill of its own.
                // Headerless sections draw no box, so their children blend into the grid.
                Content::Section(_) if b.headed => {
                    let tinted = shape_element.fill(colour);
                    let tinted = if fills { tinted } else { tinted.fill_opacity(Number::raw(0.12)) };
                    tinted.stroke(colour).style(Some(corners(shape, b))).finish();
                }
                Content::Section(_) => shape_element.fill(Paint::None).finish(),
                Content::Leaf(_) => shape_element.fill(colour).style(Some(corners(shape, b))).finish(),
            }
            for line in lines {
                write_line(out, part, line, text);
            }
        });
    };

    out.layer(LayerSpec::class("pele-sections").font_size(Number::raw(small)), |out| {
        for &at in &sections {
            draw_box(out, at);
        }
    });
    out.layer(LayerSpec::class("pele-leaves").anchor(AnchorKind::Middle).fill(Paint::Role(Role::Background)), |out| {
        for &at in &leaves {
            draw_box(out, at);
        }
    });
    out.finish()
}

pub(crate) struct Treemap;

impl Diagram for Treemap {
    type Model = TreemapModel;

    fn parse(cx: &ParseContext<'_>) -> Result<TreemapModel, RenderError> {
        super::parse(cx)
    }

    fn scene(model: &TreemapModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        draw(model, cx)
    }
}
