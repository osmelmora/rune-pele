//! How a tree view is drawn, ported from Pele's `treeview/render.ts`: one row per entry in
//! depth-first order, guides from each folder to its entries, and an icon, a name and a
//! description on each row. Drawn tightly, names and descriptions wrap to the host's width.

use super::icons::{BUILTIN_PACK, IconConfig, glyph};
use super::model::{NodeType, TreeViewModel};
use crate::catalog::DrawContext;
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities};
use crate::measure::{Emphasis, Label, Script, layout_label, literal_label, width};
use crate::scene::{
    Accessible, AuthorClasses, Entity, Facet, IconGlyph, LabelLook, LayerSpec, Number, Paint, PartId, PathProgram,
    Point, Role, Scene, SceneBuilder, Size, Style, TextWeight, TitleLayout,
};
use crate::{MonospacePart, RenderError, fit, js};
use std::collections::HashMap;

const ICON_GAP: f64 = 6.0;
const DESC_GAP: f64 = 16.0;
const GUIDE_INSET: f64 = 7.0;
const ELBOW: f64 = 4.0;
const HIGHLIGHT_OVERHANG: f64 = 4.0;

/// Mermaid's layout options, with Pele's defaults: each a number in its range, or the default.
struct Settings {
    padding_x: f64,
    padding_y: f64,
    row_indent: f64,
    line_thickness: f64,
}

impl Settings {
    fn of(section: Option<&JsObject>) -> Self {
        let setting = |key: &str, fallback: f64, min: f64, max: f64| match section.and_then(|section| section.get(key))
        {
            Some(JsValue::Number(value)) if *value >= min && *value <= max => *value,
            _ => fallback,
        };
        Settings {
            padding_x: setting("paddingX", 5.0, 0.0, 100.0),
            padding_y: setting("paddingY", 2.0, 0.0, 100.0),
            row_indent: setting("rowIndent", 15.0, 0.0, 400.0),
            line_thickness: setting("lineThickness", 1.0, 0.25, 12.0),
        }
    }
}

struct Row {
    node: usize,
    x: f64,
    cy: f64,
    icon: Option<JsText>,
    text: JsText,
    label_x: f64,
    highlighted: bool,
    label: Option<Label>,
    description: Option<Label>,
    top: f64,
    height: f64,
}

/// Pele's `renderTreeView`: drawn with room to spare, and tightly where that does not fit.
pub(super) fn scene(model: &TreeViewModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    fit::tighten(cx.options, |tight| draw(model, cx, tight))
}

/// Each entry's key: the names of the folders it is in and its own, ranked among entries with
/// the same path. Its `data-id` is its name.
fn parts(out: &mut SceneBuilder, model: &TreeViewModel) -> Vec<PartId> {
    let mut paths: Vec<JsText> = vec![JsText::default(); model.nodes.len()];
    let mut seen: HashMap<JsText, u32> = HashMap::new();
    let mut parts = vec![None; model.nodes.len()];
    let mut work: Vec<usize> = model.nodes[0].children.iter().rev().copied().collect();
    while let Some(id) = work.pop() {
        let node = &model.nodes[id];
        let path = paths[id].clone();
        let mut own = path.clone();
        if !own.is_empty() {
            own.push_str("/");
        }
        own.push_units(&node.name);
        let rank = seen.entry(own.clone()).or_insert(0);
        parts[id] = Some(out.part(Entity::node_written_as(&own, &node.name).ranked(*rank)));
        *rank += 1;
        for &child in node.children.iter().rev() {
            paths[child] = own.clone();
            work.push(child);
        }
    }
    parts.into_iter().map(|part| part.unwrap_or_else(|| out.part(Entity::diagram()))).collect()
}

fn draw(model: &TreeViewModel, cx: &DrawContext<'_>, tight: bool) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let measurer = cx.measure;
    let pad = cx.options.padding_or_default();
    let section = cx.config.get("treeView").and_then(JsValue::as_record);
    let icons = IconConfig::of(section);
    let mono = cx.options.monospace.contains(&MonospacePart::Files);
    let Settings { padding_x, padding_y, row_indent, line_thickness } = Settings::of(section);
    let nodes = &model.nodes;
    let host = cx.options.max_width.unwrap_or(320.0);

    let mut max_depth = 0;
    if tight {
        let mut depths: Vec<(usize, usize)> = nodes[0].children.iter().map(|&id| (id, 0)).collect();
        let mut i = 0;
        while i < depths.len() {
            let (id, depth) = depths[i];
            max_depth = max_depth.max(depth);
            depths.extend(nodes[id].children.iter().map(|&child| (child, depth + 1)));
            i += 1;
        }
    }
    let step = if tight {
        js::min(row_indent + padding_x, js::max(2.0, (host - 120.0) / js::max(1.0, max_depth as f64)))
    } else {
        row_indent + padding_x
    };
    let row_height = js::round(size * 1.5) + 2.0 * padding_y;
    let icon_size = size;

    let title_room = if tight { fit::title_room(cx.options) } else { 4000.0 };
    let title =
        layout_label(model.title.as_deref().unwrap_or_default(), false, measurer, size, title_room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };

    // Siblings line up: when one entry of a folder has an icon, the others leave room for one.
    let has_icon = |list: &[usize]| list.iter().any(|&id| icons.icon(&nodes[id]).is_some());
    let mut rows: Vec<Row> = Vec::with_capacity(nodes.len());
    let mut row_of: HashMap<usize, usize> = HashMap::new();
    let mut pending: Vec<(usize, usize, bool)> = Vec::new();
    let push = |pending: &mut Vec<(usize, usize, bool)>, list: &[usize], depth: usize| {
        let slot = has_icon(list);
        pending.extend(list.iter().rev().map(|&id| (id, depth, slot)));
    };
    push(&mut pending, &nodes[0].children, 0);
    let (mut label_right, mut any_highlight, mut rows_height) = (0.0, false, 0.0);
    while let Some((id, depth, slot)) = pending.pop() {
        let node = &nodes[id];
        let x = depth as f64 * step;
        let text: JsText = decode_entities(&node.name).into();
        let label_x = x + if slot { icon_size + ICON_GAP } else { 0.0 };
        let room = js::max(40.0, host - 2.0 * pad - 2.0 * HIGHLIGHT_OVERHANG - label_x);
        let style = Emphasis {
            bold: node.node_type == NodeType::Directory,
            italic: false,
            mono,
            underline: false,
            script: Script::Normal,
        };
        let italic = Emphasis { italic: true, ..Emphasis::default() };
        let label = tight.then(|| literal_label(&text, measurer, size, room, style));
        let description = node
            .description
            .as_ref()
            .filter(|_| tight)
            .map(|description| literal_label(&decode_entities(description), measurer, size, room, italic));
        let w = label.as_ref().map_or_else(|| width(measurer, &text, size, style), |label| label.width);
        if label_x + w > label_right {
            label_right = label_x + w;
        }
        let highlighted = node.class.as_ref().is_some_and(|class| class.is_str("highlight"));
        any_highlight |= highlighted;
        let height = if tight {
            let label_height = label.as_ref().map_or(0.0, |label| label.height);
            js::max(row_height, label_height + description.as_ref().map_or(0.0, |d| d.height) + 2.0 * padding_y)
        } else {
            row_height
        };
        let cy = rows_height
            + if tight {
                padding_y + js::max(icon_size, label.as_ref().map_or(0.0, |label| label.height)) / 2.0
            } else {
                height / 2.0
            };
        let icon = icons.icon(node);
        row_of.insert(id, rows.len());
        rows.push(Row {
            node: id,
            x,
            cy,
            icon,
            text,
            label_x,
            highlighted,
            label,
            description,
            top: rows_height,
            height,
        });
        rows_height += height;
        push(&mut pending, &node.children, depth + 1);
    }

    let desc_x = label_right + DESC_GAP;
    let mut width_all = label_right;
    for row in &rows {
        let Some(description) = &nodes[row.node].description else {
            continue;
        };
        let italic = Emphasis { italic: true, ..Emphasis::default() };
        let w = row
            .description
            .as_ref()
            .map_or_else(|| width(measurer, &decode_entities(description), size, italic), |label| label.width);
        width_all = js::max(width_all, if tight { row.label_x } else { desc_x } + w);
    }
    width_all = js::max(width_all, title.width);
    let inset = pad + if any_highlight { HIGHLIGHT_OVERHANG } else { 0.0 };
    let top = pad + title_height;
    let total_width = (width_all + 2.0 * inset).ceil();
    let total_height = (top + rows_height + pad).ceil();

    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width: total_width, height: total_height }, accessible);
    out.title(&TitleLayout { at: Point { x: total_width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let parts = parts(&mut out, model);
    let num = Number::num;

    let guides = LayerSpec::class("pele-tree-guides")
        .fill(Paint::None)
        .stroke(Paint::Role(Role::Border))
        .stroke_width(num(line_thickness));
    out.layer(guides, |out| {
        for row in &rows {
            let children = &nodes[row.node].children;
            let Some(&last_child) = children.last() else {
                continue;
            };
            let gx = inset + row.x + GUIDE_INSET;
            let tick = js::max(gx, inset + row.x + step - padding_x);
            let last = top + rows[row_of[&last_child]].cy;
            let r = js::min(ELBOW, tick - gx);
            let mut d = PathProgram::default().m(num(gx), num(top + row.top + row.height - 2.0)).v(num(last - r));
            if r > 0.0 {
                d = d.q([num(gx), num(last)], [num(gx + r), num(last)]).h(num(tick));
            }
            if tick > gx {
                for child in &children[..children.len() - 1] {
                    d = d.m(num(gx), num(top + rows[row_of[child]].cy)).h(num(tick));
                }
            }
            out.element(parts[row.node], Facet::Shape).path(d).finish();
        }
    });

    let baseline = size * 0.35;
    for row in &rows {
        let node = &nodes[row.node];
        let part = parts[row.node];
        let cy = top + row.cy;
        let directory = node.node_type == NodeType::Directory;
        let kind = if directory { "dir" } else { "file" };
        let author = AuthorClasses::new(node.class.as_deref().unwrap_or_default());
        let spec = LayerSpec::classes(&format!("pele-node pele-tree-{kind}"), &author)
            .part(part, Facet::Shape)
            .data_id()
            .keep_empty();
        out.layer(spec, |out| {
            if row.highlighted {
                let x = inset + row.x - HIGHLIGHT_OVERHANG;
                out.element(part, Facet::Shape)
                    .class("pele-tree-highlight")
                    .rect(
                        [num(x), num(top + row.top + 1.0)],
                        [num(total_width - inset + HIGHLIGHT_OVERHANG - x), num(row.height - 2.0)],
                        None,
                    )
                    .fill(Paint::Role(Role::Accent))
                    .fill_opacity(Number::raw(0.14))
                    .style(Some(Style::rounded(None)))
                    .finish();
            }
            if let Some(icon) = &row.icon {
                let builtin =
                    icon.strip_prefix(format!("{BUILTIN_PACK}:").encode_utf16().collect::<Vec<_>>().as_slice());
                let drawn =
                    builtin.and_then(glyph).unwrap_or(if directory { IconGlyph::Folder } else { IconGlyph::File });
                let origin = [num(inset + row.x), num(cy - icon_size / 2.0)];
                out.icon_with_glyph(part, icon, origin, num(icon_size), drawn);
            }
            let mut name_look = LabelLook::class("pele-label");
            if directory {
                name_look = name_look.weight(TextWeight::Heading);
            }
            if mono {
                name_look = name_look.mono();
            }
            if let Some(label) = &row.label {
                out.left_label(part, Facet::Label, label, inset + row.label_x, top + row.top + padding_y, name_look);
            } else if !row.text.is_empty() {
                let mut text =
                    out.text(part, Facet::Label).class("pele-label").at([num(inset + row.label_x), num(cy + baseline)]);
                if directory {
                    text = text.weight(TextWeight::Heading);
                }
                if mono {
                    text = text.mono();
                }
                let style = Emphasis { bold: directory, mono, ..Emphasis::default() };
                text.preserve_space().finish(&row.text, width(measurer, &row.text, size, style), size);
            }
            let description_look =
                || LabelLook::class("pele-tree-description").italic().fill(Some(Paint::Role(Role::Muted)));
            if let Some(description) = &row.description {
                let below = top + row.top + padding_y + row.label.as_ref().map_or(0.0, |label| label.height);
                out.left_label(part, Facet::Annotation, description, inset + row.label_x, below, description_look());
            } else if let Some(description) = &node.description {
                let text = decode_entities(description);
                let italic = Emphasis { italic: true, ..Emphasis::default() };
                out.text(part, Facet::Annotation)
                    .class("pele-tree-description")
                    .at([num(inset + desc_x), num(cy + baseline)])
                    .italic()
                    .fill(Some(Paint::Role(Role::Muted)))
                    .preserve_space()
                    .finish(&text, width(measurer, &text, size, italic), size);
            }
        });
    }
    out.finish()
}
