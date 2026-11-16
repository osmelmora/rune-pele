//! Pele's `renderIshikawa` (`ishikawa/render.ts`): the effect as a head at the end of a spine, the
//! categories meeting the spine in pairs above and below it, and their causes on bones that run
//! level and twigs that slant. Running down, the outline is an indented list.

use super::IshikawaModel;
use crate::catalog::DrawContext;
use crate::graph::edges::{Marker, MarkerUse, Tip, draw_marker};
use crate::layout::num::hypot;
use crate::measure::{Emphasis, Label, TextMeasurer, break_label, layout_label};
use crate::scene::{
    Accessible, Entity, EntityName, Facet, LabelLook, LayerSpec, LineJoin, Number, Paint, PartId, PathProgram, Point,
    Role, Scene, SceneBuilder, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

// How far a bone moves toward the tail for each unit it rises from the spine.
const SLANT: f64 = 0.5;
const FIRST_ROW: f64 = 14.0;
const ROW_GAP: f64 = 6.0;
const BONE_END: f64 = 8.0;
const TWIG_STEP: f64 = 22.0;
const STUB: f64 = 18.0;
const TEXT_GAP: f64 = 5.0;
const PAIR_GAP: f64 = 26.0;
const HEAD_GAP: f64 = 34.0;
const TAIL: f64 = 28.0;
const BOX_PAD_X: f64 = 10.0;
const BOX_PAD_Y: f64 = 5.0;

fn length() -> f64 {
    hypot(SLANT, 1.0)
}

/// Whether a bone at `depth` runs level. One at an odd depth slants away from the spine.
fn runs_level(depth: usize) -> bool {
    depth.is_multiple_of(2)
}

/// Whether the category at `index` meets the spine from above. They alternate, the first above.
fn above_spine(index: usize) -> bool {
    index.is_multiple_of(2)
}

/// A cause below a category. Bones at even depths run level, from the slanted bone above them
/// toward the tail; bones at odd depths slant away from the spine, from the level bone above them.
struct Bone {
    node: usize,
    label: Label,
    depth: usize,
    parent: Option<usize>,
    /// Its place among its siblings, counted from the spine outward.
    slot: usize,
    kids: usize,
    /// How far its row is from the spine.
    rise: f64,
    /// Where it meets its parent, measured level from the category's bone: 0 on it, negative toward
    /// the tail.
    at: f64,
    /// Where its text ends.
    text_end: f64,
}

struct Category {
    node: usize,
    label: Label,
    box_w: f64,
    box_h: f64,
    bones: Vec<Bone>,
    /// The length of its bone, as distance from the spine.
    rise: f64,
    /// How far its causes reach toward the tail, measured level from its bone.
    reach: f64,
}

/// A cause still to be laid out, or, once its bone is made, only its row.
struct Frame {
    node: usize,
    depth: usize,
    parent: Option<usize>,
    slot: usize,
    bone: Option<usize>,
}

/// Pele's `arrange`: gives every cause of a category a row of its own, so that no two texts can
/// meet, and measures how far the whole of it reaches. A level bone takes the row nearest the
/// spine and its twigs the rows beyond; a slanted twig takes the row beyond everything that hangs
/// off it. The outline may be nested as deep as its indentation goes, so the walk keeps a stack of
/// its own.
fn arrange(
    model: &IshikawaModel,
    node: usize,
    up: bool,
    measurer: &dyn TextMeasurer,
    size: f64,
    small: f64,
) -> Category {
    let plain = Emphasis::default();
    let label = layout_label(&model.nodes[node].text, false, measurer, size, 180.0, plain);
    let line_height = js::round(small * 1.5);
    let mut bones: Vec<Bone> = Vec::new();
    let mut rows: Vec<usize> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    // Above the spine the first cause goes farthest out, so that the causes read from the top down.
    let push = |stack: &mut Vec<Frame>, children: &[usize], depth: usize, parent: Option<usize>| {
        let n = children.len();
        for slot in (0..n).rev() {
            let node = children[if up { n - 1 - slot } else { slot }];
            stack.push(Frame { node, depth, parent, slot, bone: None });
        }
    };
    push(&mut stack, &model.nodes[node].children, 2, None);
    while let Some(mut frame) = stack.pop() {
        if let Some(bone) = frame.bone {
            rows.push(bone);
            continue;
        }
        let index = bones.len();
        let cause = &model.nodes[frame.node];
        bones.push(Bone {
            node: frame.node,
            label: layout_label(&cause.text, false, measurer, small, 200.0, plain),
            depth: frame.depth,
            parent: frame.parent,
            slot: frame.slot,
            kids: cause.children.len(),
            rise: 0.0,
            at: 0.0,
            text_end: 0.0,
        });
        let depth = frame.depth;
        if runs_level(depth) {
            rows.push(index);
        } else {
            frame.bone = Some(index);
            stack.push(frame);
        }
        push(&mut stack, &cause.children, depth + 1, Some(index));
    }

    let mut rise = FIRST_ROW;
    for &index in &rows {
        let height = js::max(bones[index].label.height, line_height) + ROW_GAP;
        bones[index].rise = rise + height / 2.0;
        rise += height;
    }
    let box_w = label.width + 2.0 * BOX_PAD_X;
    let mut reach = box_w / 2.0;
    for index in 0..bones.len() {
        let parent = bones[index].parent.map(|parent| (bones[parent].at, bones[parent].kids));
        let bone = &mut bones[index];
        if runs_level(bone.depth) {
            bone.at = parent.map_or(0.0, |(at, _)| at);
            let length = if bone.kids > 0 { (bone.kids as f64 + 1.0) * TWIG_STEP } else { STUB };
            bone.text_end = bone.at - length - TEXT_GAP;
        } else {
            // Twigs stand along their level bone, the one nearest the spine nearest the tail.
            let (at, kids) = parent.expect("a twig hangs off a bone");
            bone.at = at - (kids as f64 - bone.slot as f64) * TWIG_STEP;
            bone.text_end = bone.at - TEXT_GAP;
        }
        reach = js::max(reach, bone.label.width - bone.text_end);
    }
    let box_h = label.height + 2.0 * BOX_PAD_Y;
    Category { node, label, box_w, box_h, bones, rise: rise + BONE_END, reach }
}

/// Each node's part: the effect and the causes are nodes, the categories groups, each ranked
/// among those of its kind named alike.
fn parts(model: &IshikawaModel, out: &mut SceneBuilder) -> Vec<PartId> {
    let mut depth = vec![0usize; model.nodes.len()];
    let mut ranks: HashMap<(bool, &[u16]), u32> = HashMap::new();
    let mut parts = Vec::with_capacity(model.nodes.len());
    for (index, node) in model.nodes.iter().enumerate() {
        for &child in &node.children {
            depth[child] = depth[index] + 1;
        }
        let category = depth[index] == 1;
        let rank = ranks.entry((category, &node.text)).or_insert(0);
        let duplicate = std::mem::replace(rank, *rank + 1);
        let entity = if category { Entity::group(&node.text) } else { Entity::node(&node.text) };
        parts.push(out.part(entity.ranked(duplicate)));
    }
    parts
}

pub(super) fn draw(model: &IshikawaModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    fit::turn_to_fit(cx.options, |down| if down { vertical_causes(model, cx) } else { across(model, cx) })
}

fn num(value: f64) -> Number {
    Number::num(value)
}

fn line() -> Paint {
    Paint::Role(Role::Line)
}

fn across(model: &IshikawaModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let measurer = cx.measure;
    let pad = cx.options.padding;
    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measurer, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let length = length();

    let (mut min_x, mut max_x, mut min_y, mut max_y) = (0.0, 0.0, 0.0, 0.0);
    let mut drawing = None;
    if let Some(root) = model.nodes.first() {
        let head = layout_label(&root.text, false, measurer, size, 200.0, Emphasis::BOLD);
        let head_h = js::max(head.height + 24.0, 48.0);
        let head_w = head.width + 24.0 + head_h / 2.0;
        let mut categories: Vec<Category> = root
            .children
            .iter()
            .enumerate()
            .map(|(i, &node)| arrange(model, node, above_spine(i), measurer, size, small))
            .collect();
        // Even a category without causes has a bone long enough to carry its name clear of the head.
        for category in &mut categories {
            category.rise = js::max(category.rise, head_h / 2.0 + 14.0);
        }

        // Categories meet the spine in pairs, one above and one below, each pair as far toward the
        // tail from the one before as the wider of the two needs.
        let mut xs = Vec::with_capacity(categories.len());
        let mut x = -HEAD_GAP;
        for i in (0..categories.len()).step_by(2) {
            if i > 0 {
                let mut room: f64 = 0.0;
                for side in 0..2 {
                    if let Some(before) = categories.get(i - 2 + side) {
                        let next = categories.get(i + side).map_or(0.0, |next| next.box_w / 2.0);
                        room = js::max(room, before.reach + next);
                    }
                }
                x -= room + PAIR_GAP;
            }
            xs.extend([x, x]);
        }
        let tail = if categories.is_empty() { -HEAD_GAP } else { x - TAIL };
        max_x = head_w;
        min_x = tail;
        min_y = -head_h / 2.0;
        max_y = head_h / 2.0;

        let mut placed = Vec::with_capacity(categories.len());
        for (i, category) in categories.iter().enumerate() {
            let sign = if above_spine(i) { -1.0 } else { 1.0 };
            let ax = xs[i];
            let px = |at: f64, rise: f64| ax + at - rise * SLANT;
            let end_x = px(0.0, category.rise);
            let end_y = sign * category.rise;
            // The bone stops short of the spine to leave its arrowhead a clean point.
            let trim = Marker::Point.trim();
            let mut d = PathProgram::default()
                .m(num(ax - (SLANT / length) * trim), num((sign * trim) / length))
                .l(num(end_x), num(end_y));
            let mut texts = Vec::with_capacity(category.bones.len());
            for bone in &category.bones {
                let y = sign * bone.rise;
                d = if runs_level(bone.depth) {
                    d.m(num(px(bone.at, bone.rise)), num(y)).h(num(px(bone.text_end + TEXT_GAP, bone.rise)))
                } else {
                    let from = category.bones[bone.parent.expect("a twig hangs off a bone")].rise;
                    d.m(num(px(bone.at, from)), num(sign * from)).l(num(px(bone.at, bone.rise)), num(y))
                };
                let right = px(bone.text_end, bone.rise);
                texts.push(Point { x: right - bone.label.width / 2.0, y });
                min_x = js::min(min_x, right - bone.label.width);
            }
            let box_y = end_y + sign * (category.box_h / 2.0);
            min_x = js::min(min_x, end_x - category.box_w / 2.0);
            max_x = js::max(max_x, end_x + category.box_w / 2.0);
            min_y = js::min(min_y, box_y - category.box_h / 2.0);
            max_y = js::max(max_y, box_y + category.box_h / 2.0);
            let tip = Tip { x: ax, y: 0.0, dx: SLANT / length, dy: -sign / length };
            placed.push((d, tip, Point { x: end_x, y: box_y }, texts));
        }
        drawing = Some((head, head_w, head_h, tail, categories, placed));
    }

    let inner = js::max(max_x - min_x, title.width);
    let total = Size { width: (inner + 2.0 * pad).ceil(), height: (max_y - min_y + title_height + 2.0 * pad).ceil() };
    let shift = Point { x: pad + (inner - (max_x - min_x)) / 2.0 - min_x, y: pad + title_height - min_y };
    let mut out = SceneBuilder::new(cx, total, Accessible::default());
    let at = Point { x: total.width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at });
    let parts = parts(model, &mut out);
    let Some((head, head_w, head_h, tail, categories, placed)) = drawing else {
        return out.finish();
    };
    out.layer(LayerSpec::group().translate(shift), |out| {
        let diagram = out.part(Entity::diagram());
        let spine = PathProgram::default().m(num(tail), Number::raw(0.0)).h(num(-Marker::Point.trim()));
        out.decoration().class("pele-ishikawa-spine").path(spine).fill(Paint::None).stroke(line()).finish();
        let tip = Tip { x: 0.0, y: 0.0, dx: 1.0, dy: 0.0 };
        draw_marker(out, diagram, Facet::EndMarker, Marker::Point, tip, MarkerUse::Edge(line()));
        for (category, (d, tip, centre, texts)) in categories.iter().zip(placed) {
            let part = parts[category.node];
            out.layer(LayerSpec::class("pele-ishikawa-category").part(part, Facet::Shape).data_id(), |out| {
                out.element(part, Facet::Shape)
                    .class("pele-ishikawa-bones")
                    .path(d)
                    .fill(Paint::None)
                    .stroke(line())
                    .finish();
                draw_marker(out, part, Facet::EndMarker, Marker::Point, tip, MarkerUse::Edge(line()));
                let (w, h) = (category.box_w, category.box_h);
                out.element(part, Facet::Shape)
                    .rect([num(centre.x - w / 2.0), num(centre.y - h / 2.0)], [num(w), num(h)], None)
                    .fill(Paint::Role(Role::Surface))
                    .stroke(Paint::Role(Role::Border))
                    .style(Some(Style::rounded(None)))
                    .finish();
                out.label(part, Facet::Label, &category.label, centre, LabelLook::class("pele-label"));
                out.layer(LayerSpec::class("pele-ishikawa-causes").font_size(Number::raw(small)), |out| {
                    for (bone, at) in category.bones.iter().zip(texts) {
                        let look = LabelLook::default().fill((bone.depth > 2).then_some(Paint::Role(Role::Muted)));
                        out.label(parts[bone.node], Facet::Label, &bone.label, at, look);
                    }
                });
            });
        }
        let part = parts[0];
        out.layer(LayerSpec::class("pele-ishikawa-head").part(part, Facet::Shape).data_id(), |out| {
            let r = head_h / 2.0;
            let outline = PathProgram::default()
                .m(Number::raw(0.0), num(-r))
                .h(num(head_w - r))
                .a([num(r), num(r)], true, [num(head_w - r), num(r)])
                .h(Number::raw(0.0))
                .z();
            out.element(part, Facet::Shape)
                .path(outline)
                .fill(Paint::Role(Role::Surface))
                .stroke(Paint::Role(Role::Border))
                .join(LineJoin::Round)
                .finish();
            let look = LabelLook::class("pele-label").weight(TextWeight::Heading);
            out.label(part, Facet::Label, &head, Point { x: 12.0 + head.width / 2.0, y: 0.0 }, look);
        });
    });
    out.finish()
}

/// A node of the indented list, placed.
struct Entry {
    node: usize,
    depth: usize,
    parent: Option<usize>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    label: Label,
}

/// Pele's `verticalCauses`: the outline as an indented list, each node under the one before it,
/// and a line from under each parent to the left of each of its children.
fn vertical_causes(model: &IshikawaModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let measurer = cx.measure;
    let pad = cx.options.padding;
    let room = js::max(140.0, cx.options.max_width.unwrap_or(320.0) - 2.0 * pad);
    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measurer, size, fit::title_room(cx.options), Emphasis::BOLD);

    let mut order: Vec<(usize, usize, Option<usize>)> = Vec::with_capacity(model.nodes.len());
    let mut pending: Vec<(usize, usize, Option<usize>)> =
        if model.nodes.is_empty() { Vec::new() } else { vec![(0, 0, None)] };
    let mut deepest = 0;
    while let Some((node, depth, parent)) = pending.pop() {
        let index = order.len();
        order.push((node, depth, parent));
        deepest = deepest.max(depth);
        for &child in model.nodes[node].children.iter().rev() {
            pending.push((child, depth + 1, Some(index)));
        }
    }
    let indent = js::min(20.0, js::max(2.0, (room - 120.0) / js::max(1.0, deepest as f64)));
    let mut y = pad + if title.height != 0.0 { title.height + 12.0 } else { 0.0 };
    let mut width = title.width;
    let mut entries: Vec<Entry> = Vec::with_capacity(order.len());
    for (node, depth, parent) in order {
        let x = pad + depth as f64 * indent;
        let label_room = js::max(60.0, room - depth as f64 * indent - 20.0);
        let (text_size, style) = if depth < 2 { (size, Emphasis::BOLD) } else { (small, Emphasis::default()) };
        let text = &model.nodes[node].text;
        let label =
            break_label(&layout_label(text, false, measurer, text_size, label_room, style), measurer, label_room);
        let (w, h) = (label.width + 20.0, label.height + 12.0);
        entries.push(Entry { node, depth, parent, x, y, w, h, label });
        y += h + 12.0;
        width = js::max(width, x - pad + w);
    }
    let total_width = (width + 2.0 * pad).ceil();
    let total = Size { width: total_width, height: (js::max(pad + title.height, y - 12.0) + pad).ceil() };
    let mut out = SceneBuilder::new(cx, total, Accessible::default());
    let at = Point { x: total_width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at });
    let parts = parts(model, &mut out);

    let mut edge_ranks: HashMap<EntityName, u32> = HashMap::new();
    for entry in &entries {
        let Some(parent) = entry.parent.map(|at| &entries[at]) else {
            continue;
        };
        let (from, to) = (&model.nodes[parent.node].text, &model.nodes[entry.node].text);
        let name = EntityName::Between { from: from.to_vec(), to: to.to_vec(), kind: String::new() };
        let rank = edge_ranks.entry(name.clone()).or_insert(0);
        let duplicate = std::mem::replace(rank, *rank + 1);
        let part = out.part(Entity::edge(to, name, duplicate));
        out.relation(part, parts[parent.node], parts[entry.node]);
        let gx = parent.x + indent / 2.0;
        let path =
            PathProgram::default().m(num(gx), num(parent.y + parent.h)).v(num(entry.y + entry.h / 2.0)).h(num(entry.x));
        out.element(part, Facet::Shape)
            .class("pele-edge")
            .path(path)
            .fill(Paint::None)
            .stroke(Paint::Role(Role::Border))
            .finish();
    }
    for entry in &entries {
        let part = parts[entry.node];
        let class = match entry.depth {
            0 => "pele-node pele-ishikawa-head",
            1 => "pele-node pele-ishikawa-category",
            _ => "pele-node pele-ishikawa-cause",
        };
        out.layer(LayerSpec::class(class).part(part, Facet::Shape).data_id(), |out| {
            if entry.depth < 2 {
                let rx = Number::raw(if entry.depth == 0 { 12.0 } else { 4.0 });
                out.element(part, Facet::Shape)
                    .rect([num(entry.x), num(entry.y)], [num(entry.w), num(entry.h)], Some(rx))
                    .fill(Paint::Role(Role::Surface))
                    .stroke(Paint::Role(Role::Border))
                    .finish();
            }
            let mut look = LabelLook::class("pele-label");
            if entry.depth < 2 {
                look = look.weight(TextWeight::Heading);
            } else {
                look = look.font_size(Number::raw(small));
            }
            let centre = Point { x: entry.x + entry.w / 2.0, y: entry.y + entry.h / 2.0 };
            out.label(part, Facet::Label, &entry.label, centre, look);
        });
    }
    out.finish()
}
