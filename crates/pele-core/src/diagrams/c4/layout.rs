//! Where shapes and boundaries go, ported from Pele's `src/diagrams/c4/layout.ts`: Mermaid's
//! grid. In each boundary its shapes go in rows of `c4ShapeInRow`, then the boundaries inside it
//! in rows of `c4BoundaryInRow`, each sized around its own content.

use super::route::{Box, BoxId, Form, HEAD, LABEL_PAD, LANE, NARROW};
use super::{C4Model, ElementField, Shape, Store, Tier};
use crate::graph::edges::ARROW;
use crate::input::{JsText, Units, split_unit, to_lowercase};
use crate::js;
use crate::measure::{Emphasis, Label, TextMeasurer, layout_label};

const SHAPE_W: f64 = 216.0;
const SHAPE_MIN_H: f64 = 64.0;
const PAD_X: f64 = 14.0;
const PAD_Y: f64 = 12.0;
// Room between two columns for the label of a line across them, clear of an arrowhead on either
// side, with an arrowhead's length to spare for a word a little wider than the label room.
const GAP_X: f64 = NARROW + LABEL_PAD + 3.0 * ARROW;
const GAP_Y: f64 = 88.0;
// Room for a relation to run along the inside of a boundary.
pub(super) const GROUP_PAD: f64 = LANE + 8.0;
const GROUP_GAP: f64 = 48.0;
// Where a boundary's heading starts, from its top-left corner.
pub(super) const HEAD_LEFT: f64 = 16.0;
pub(super) const HEAD_TOP: f64 = 10.0;

/// A shape in its boundary.
pub(super) struct Node {
    pub(super) at: BoxId,
    /// Its place in the model's shapes.
    pub(super) shape: usize,
    /// Type, name, technology, description.
    pub(super) labels: [Label; 4],
    pub(super) text_h: f64,
}

/// A boundary with what it holds.
pub(super) struct Group {
    pub(super) at: BoxId,
    /// The boundaries inside it, by their place in the model.
    pub(super) kids: Vec<usize>,
    pub(super) nodes: Vec<Node>,
    /// Name, type, description; none for the top level.
    pub(super) labels: Vec<Label>,
    pub(super) head: f64,
    pub(super) head_w: f64,
    pub(super) head_x: f64,
}

pub(super) struct Layout {
    pub(super) boxes: Vec<Box>,
    /// One group per boundary of the model, in its order.
    pub(super) groups: Vec<Group>,
    /// The boundaries that are drawn, parents before children. The first is the unnamed top level.
    pub(super) order: Vec<usize>,
}

/// Shape keywords Mermaid accepts in `$shape`, `$sprite` or `$tags`, as names of the shared shapes.
fn form_named(name: &[u16]) -> Option<Form> {
    let name = to_lowercase(name);
    let forms = [
        ("person", Form::Person),
        ("box", Form::Rect),
        ("rounded", Form::Rect),
        ("component", Form::Rect),
        ("cylinder", Form::Cyl),
        ("database", Form::Cyl),
        ("db", Form::Cyl),
        ("queue", Form::HCyl),
        ("pipe", Form::HCyl),
    ];
    forms.iter().find(|(word, _)| name.is_str(word)).map(|&(_, form)| form)
}

fn form_of(shape: &Shape) -> Form {
    let element = &shape.element;
    let named = |field| form_named(element.field(field).map_or(&[][..], |text| &**text));
    if let Some(form) = named(ElementField::Shape).or_else(|| named(ElementField::Sprite)) {
        return form;
    }
    let tagged = element
        .filled(ElementField::Tags)
        .and_then(|tags| split_unit(tags, u16::from(b',')).into_iter().find_map(|tag| form_named(tag.trim_js())));
    if let Some(form) = tagged {
        return form;
    }
    match (shape.kind.tier, shape.kind.store) {
        (Tier::Person, _) => Form::Person,
        (_, Store::Db) => Form::Cyl,
        (_, Store::Queue) => Form::HCyl,
        _ => Form::Rect,
    }
}

/// What lays out the text of one drawing.
pub(super) struct Texts<'a> {
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) size: f64,
    pub(super) small: f64,
}

impl Texts<'_> {
    pub(super) fn label(&self, raw: &[u16], size: f64, room: f64, base: Emphasis) -> Label {
        layout_label(raw, false, self.measurer, size, room, base)
    }
}

/// `[text]`, where there is text.
pub(super) fn bracketed(text: Option<&JsText>) -> JsText {
    let mut out = JsText::default();
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        out.push_str("[");
        out.push_units(text);
        out.push_str("]");
    }
    out
}

fn measure(shape: &Shape, index: usize, texts: &Texts<'_>, width: f64, boxes: &mut Vec<Box>) -> Node {
    let form = form_of(shape);
    let cap_w = if form == Form::HCyl { 36.0 } else { 0.0 };
    let cap_h = match form {
        Form::Person => HEAD,
        Form::Cyl => 24.0,
        _ => 0.0,
    };
    let inner = width - 2.0 * PAD_X - cap_w;
    let element = &shape.element;
    let mut kind = JsText::from("«");
    kind.push_str(&shape.kind.name().replace('_', " "));
    kind.push_str("»");
    let plain = Emphasis::default();
    let labels = [
        texts.label(&kind, texts.small - 1.0, inner, plain),
        texts.label(&element.label, texts.size, inner, Emphasis::BOLD),
        texts.label(&bracketed(element.field(ElementField::Techn)), texts.small, inner, plain),
        texts.label(element.field(ElementField::Descr).map_or(&[][..], |text| &**text), texts.small, inner, plain),
    ];
    let mut tw: f64 = 0.0;
    let mut text_h = if labels[3].height > 0.0 { 4.0 } else { 0.0 };
    for label in &labels {
        tw = js::max(tw, label.width);
        text_h += label.height;
    }
    let at = boxes.len();
    boxes.push(Box {
        x: 0.0,
        y: 0.0,
        w: js::max(width, tw + 2.0 * PAD_X + cap_w),
        h: js::max(SHAPE_MIN_H, text_h + 2.0 * PAD_Y + cap_h),
        form,
    });
    Node { at, shape: index, labels, text_h }
}

/// Places boxes left to right in rows of `per_row`, from (0, 0), and answers the width and
/// height the rows take. Shapes are stretched to the height of their row; boundaries keep their
/// own.
fn rows(boxes: &mut [Box], ids: &[BoxId], per_row: f64, gap_x: f64, gap_y: f64, stretch: bool) -> (f64, f64) {
    let mut width: f64 = 0.0;
    let mut y = 0.0;
    // A row count is a whole number of at least one, or infinity.
    let step = if per_row >= ids.len() as f64 { ids.len().max(1) } else { per_row as usize };
    for row in ids.chunks(step) {
        let mut x = 0.0;
        let mut row_h: f64 = 0.0;
        for &id in row {
            row_h = js::max(row_h, boxes[id].h);
        }
        for &id in row {
            let b = &mut boxes[id];
            b.x = x;
            b.y = y;
            if stretch {
                b.h = row_h;
            }
            x += b.w + gap_x;
        }
        width = js::max(width, x - gap_x);
        y += row_h + gap_y;
    }
    (width, if ids.is_empty() { 0.0 } else { y - gap_y })
}

/// Pele's `layoutC4`. `room` is the width the drawing has when it is drawn tight, and `gap` the
/// gap between its columns then.
pub(super) fn layout(model: &C4Model, texts: &Texts<'_>, room: Option<f64>, gap: f64) -> Layout {
    let group_pad = if room.is_none() { GROUP_PAD } else { 12.0 };
    let mut boxes = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    for boundary in &model.boundaries {
        let label_room = room.map_or(4000.0, |room| js::max(80.0, room - 2.0 * HEAD_LEFT - 2.0 * GROUP_PAD * 2.0));
        let element = &boundary.element;
        let labels = if element.parent.is_some() {
            let descr = element.field(ElementField::Descr).map_or(&[][..], |text| &**text);
            vec![
                texts.label(&element.label, texts.size, label_room, Emphasis::BOLD),
                texts.label(
                    &bracketed(element.field(ElementField::Type)),
                    texts.small,
                    label_room,
                    Emphasis::default(),
                ),
                texts.label(descr, texts.small, label_room, Emphasis::default()),
            ]
        } else {
            Vec::new()
        };
        let mut head = 0.0;
        let mut head_w: f64 = 0.0;
        for label in &labels {
            head += label.height;
            head_w = js::max(head_w, label.width);
        }
        let at = boxes.len();
        boxes.push(Box { x: 0.0, y: 0.0, w: 0.0, h: 0.0, form: Form::Rect });
        groups.push(Group {
            at,
            kids: Vec::new(),
            nodes: Vec::new(),
            labels,
            head: if head > 0.0 { head + HEAD_TOP + 6.0 + LANE } else { group_pad },
            head_w,
            head_x: HEAD_LEFT,
        });
    }
    for (index, boundary) in model.boundaries.iter().enumerate() {
        if let Some(parent) = boundary.element.parent {
            groups[parent].kids.push(index);
        }
    }
    // A boundary that ends up inside itself is not reachable from the top, as in Mermaid.
    let mut order = vec![0];
    let mut depths: Vec<Option<f64>> = vec![None; groups.len()];
    depths[0] = Some(0.0);
    let mut i = 0;
    while i < order.len() {
        let group = order[i];
        for kid in groups[group].kids.clone() {
            depths[kid] = Some(depths[group].expect("a group in the order has a depth") + 1.0);
            order.push(kid);
        }
        i += 1;
    }
    for (index, shape) in model.shapes.iter().enumerate() {
        let Some(parent) = shape.element.parent else {
            continue;
        };
        let depth = depths[parent].unwrap_or(0.0);
        let width =
            room.map_or(SHAPE_W, |room| js::max(100.0, js::min(SHAPE_W, room - 2.0 * depth * group_pad - 32.0)));
        let node = measure(shape, index, texts, width, &mut boxes);
        groups[parent].nodes.push(node);
    }

    let per_row = js::max(1.0, model.shape_in_row);
    let groups_per_row = js::max(1.0, model.boundary_in_row);
    let mut kids_top = vec![0.0; groups.len()];
    for &g in order.iter().rev() {
        let gap_x = if room.is_none() { GAP_X } else { gap };
        let count = |boxes: &[Box], ids: &[BoxId], limit: f64, gap: f64| match room {
            None => limit,
            Some(room) => {
                let widest = ids.iter().fold(1.0, |widest, &id| js::max(widest, boxes[id].w));
                js::max(1.0, js::min(limit, ((room + gap - 2.0 * group_pad) / (widest + gap)).floor()))
            }
        };
        let nodes: Vec<BoxId> = groups[g].nodes.iter().map(|node| node.at).collect();
        let kids: Vec<BoxId> = groups[g].kids.iter().map(|&kid| groups[kid].at).collect();
        let nodes_per_row = count(&boxes, &nodes, per_row, gap_x);
        let (grid_w, grid_h) = rows(&mut boxes, &nodes, nodes_per_row, gap_x, GAP_Y, true);
        let kids_per_row = count(&boxes, &kids, groups_per_row, GROUP_GAP);
        let (kids_w, kids_h) = rows(&mut boxes, &kids, kids_per_row, GROUP_GAP, GROUP_GAP, false);
        let between = if grid_h > 0.0 && kids_h > 0.0 { GROUP_GAP } else { 0.0 };
        let frame = if g == 0 { 0.0 } else { group_pad };
        kids_top[g] = grid_h + between;
        let group = &groups[g];
        let b = &mut boxes[group.at];
        b.w = js::max(js::max(grid_w, kids_w), group.head_w + 2.0 * HEAD_LEFT - 2.0 * frame) + 2.0 * frame;
        b.h = (if frame != 0.0 { group.head } else { frame }) + grid_h + between + kids_h + frame;
    }

    for &g in &order {
        let group = &groups[g];
        let own = boxes[group.at];
        let left = own.x + if g == 0 { 0.0 } else { group_pad };
        let top = own.y + if g == 0 { 0.0 } else { group.head };
        for node in &group.nodes {
            boxes[node.at].x += left;
            boxes[node.at].y += top;
        }
        for &kid in &group.kids {
            let at = groups[kid].at;
            boxes[at].x += left;
            boxes[at].y += top + kids_top[g];
        }
    }
    Layout { boxes, groups, order }
}
