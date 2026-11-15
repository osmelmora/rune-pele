//! Pele's `renderMindmap` (`mindmap/render.ts`): each subtree takes a band, the first branches go
//! right and the rest left, and each branch is drawn in its own series colour. Running down, the
//! map is an outline: the root at the top and each level set in under its parent.

use super::{MindmapModel, MindmapNode};
use crate::catalog::DrawContext;
use crate::diagrams::flowchart::Shape;
use crate::diagrams::outline::NodeShape;
use crate::graph::shapes::{LEFT, RIGHT, draw_shape, shape_inset, shape_size};
use crate::input::{JsText, JsValue};
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineLook, Number, Paint, PartId,
    PathProgram, PathStep, Point, Role, Scene, SceneBuilder, SeriesSlot, ShapeLook, Size, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

const GAP_X: f64 = 36.0;
const GAP_Y: f64 = 8.0;
// Between two subtrees, of which at least one has children of its own.
const GROUP_GAP: f64 = 18.0;
const BRANCH_GAP_X: f64 = 52.0;
const BRANCH_GAP_Y: f64 = 28.0;
const TEXT_PAD: f64 = 6.0;
const PILL_PAD: f64 = 12.0;
const TINT: f64 = 0.22;
// Running down, each level is set in by this much, and links leave a node this far from its left edge.
const INDENT: f64 = 28.0;
const STEM: f64 = 12.0;
const BEND: f64 = 8.0;
const ROW_GAP: f64 = 10.0;

/// The shape Mermaid's node type gives a node. A node without delimiters is text on a line,
/// except the root, which is a pill.
fn shape_of(shape: NodeShape, root: bool) -> Option<Shape> {
    match shape {
        NodeShape::RoundedRect => Some(Shape::Rounded),
        NodeShape::Rect => Some(Shape::Rect),
        NodeShape::Circle => Some(Shape::Circle),
        NodeShape::Cloud => Some(Shape::Cloud),
        NodeShape::Bang => Some(Shape::Bang),
        NodeShape::Hexagon => Some(Shape::Hex),
        NodeShape::Default => root.then_some(Shape::Stadium),
    }
}

pub(super) fn draw(model: &MindmapModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    fit::turn_to_fit(cx.options, |down| draw_once(model, cx, down))
}

/// Each node's size and place, for one way of running the map.
struct Placed {
    labels: Vec<Label>,
    shapes: Vec<Option<Shape>>,
    w: Vec<f64>,
    h: Vec<f64>,
    dy: Vec<f64>,
    /// Where links meet the node, measured down from its top: its middle.
    anchor: Vec<f64>,
    parent: Vec<usize>,
    x: Vec<f64>,
    y: Vec<f64>,
    /// Which way from the root a node's branch goes: 1 to the right, -1 to the left.
    side: Vec<f64>,
}

/// Each subtree's band: its height, the offset of a subtree among its siblings, the node's offset
/// inside its band, and the offset of its children's bands.
struct Bands {
    extent: Vec<f64>,
    rel: Vec<f64>,
    own: Vec<f64>,
    kids: Vec<f64>,
}

impl Bands {
    /// Pele's `stack`: stacks subtrees top to bottom and returns the height at which their
    /// parent's anchor belongs, with the height of the stack.
    fn stack(&mut self, nodes: &[MindmapNode], list: &[usize], gap: f64, anchor: &[f64]) -> (f64, f64) {
        let mut y = 0.0;
        let mut last = 0.0;
        for (k, &c) in list.iter().enumerate() {
            self.rel[c] = y;
            let grouped =
                !nodes[c].children.is_empty() || list.get(k + 1).is_some_and(|&next| !nodes[next].children.is_empty());
            last = if grouped { js::max(gap, GROUP_GAP) } else { gap };
            y += self.extent[c] + last;
        }
        let (a, b) = (list[0], list[list.len() - 1]);
        let middle = (self.rel[a] + self.own[a] + anchor[a] + self.rel[b] + self.own[b] + anchor[b]) / 2.0;
        (middle, y - last)
    }
}

fn place(model: &MindmapModel, cx: &DrawContext<'_>, down: bool) -> Placed {
    let size = cx.options.typography.normal.size;
    let measurer = cx.measure;
    let pad = cx.options.padding;
    let given = cx.config.get("mindmap").and_then(JsValue::as_record).and_then(|section| section.get("maxNodeWidth"));
    let wrap = match given.and_then(JsValue::as_number) {
        Some(width) if width > 0.0 => js::min(width, 2000.0),
        _ => 200.0,
    };
    let nodes = &model.nodes;
    let n = nodes.len();
    let mut parent = vec![0; n];
    let mut depth = vec![0usize; n];
    for i in 0..n {
        for &child in &nodes[i].children {
            parent[child] = i;
            depth[child] = depth[i] + 1;
        }
    }
    // An outline has the width the host gives it, less what its level is set in.
    let room = match cx.options.max_width {
        Some(width) if down && width > 0.0 => width - 2.0 * pad,
        _ => f64::INFINITY,
    };
    let mut placed = Placed {
        labels: Vec::with_capacity(n),
        shapes: Vec::with_capacity(n),
        w: vec![0.0; n],
        h: vec![0.0; n],
        dy: vec![0.0; n],
        anchor: vec![0.0; n],
        parent,
        x: vec![0.0; n],
        y: vec![0.0; n],
        side: vec![0.0; n],
    };
    for (i, node) in nodes.iter().enumerate() {
        let fit = js::min(wrap, js::max(72.0, room - depth[i] as f64 * INDENT - 2.0 * TEXT_PAD));
        let label = layout_label(&node.descr, true, measurer, size, fit, Emphasis::default());
        let tw = label.width;
        let th = js::max(label.height, js::round(size * 1.5));
        let mut shape = shape_of(node.shape, i == 0);
        // In an outline the root heads a column, and the trunk drops from under its left end. A
        // circle or a cloud would stand wide of that, so the root is a pill there unless it is a
        // box already.
        if down && i == 0 && !matches!(shape, Some(Shape::Rect | Shape::Rounded)) {
            shape = Some(Shape::Stadium);
        }
        match shape {
            None => {
                // A node with no shape of its own is plain text when it ends a branch, and text in
                // a tinted pill when the branch goes on from it, which sets the levels apart.
                let pill = !node.children.is_empty();
                placed.w[i] = tw + 2.0 * if pill { PILL_PAD } else { TEXT_PAD };
                placed.h[i] = th + if pill { 6.0 } else { 0.0 };
                placed.anchor[i] = placed.h[i] / 2.0;
            }
            Some(shape) => {
                let s = shape_size(shape, tw, th);
                placed.w[i] = s.w;
                placed.h[i] = s.h;
                placed.dy[i] = s.dy;
                placed.anchor[i] = s.h / 2.0;
            }
        }
        placed.labels.push(label);
        placed.shapes.push(shape);
    }

    let mut bands = Bands { extent: vec![0.0; n], rel: vec![0.0; n], own: vec![0.0; n], kids: vec![0.0; n] };
    // Nodes are in depth-first order, so going backwards visits children before their parent.
    for i in (1..n).rev() {
        let children = &nodes[i].children;
        if children.is_empty() {
            bands.extent[i] = placed.h[i];
            continue;
        }
        let (middle, stacked) = bands.stack(nodes, children, GAP_Y, &placed.anchor);
        let top = middle - placed.anchor[i];
        let low = js::min(0.0, top);
        bands.extent[i] = js::max(stacked, top + placed.h[i]) - low;
        bands.own[i] = top - low;
        bands.kids[i] = -low;
    }

    let (x, y) = (&mut placed.x, &mut placed.y);
    if down {
        // Nodes are in depth-first order, which is the order of an outline's rows.
        let mut row = 0.0;
        for i in 0..n {
            if i > 0 && placed.parent[i] == 0 {
                row += ROW_GAP;
            }
            x[i] = depth[i] as f64 * INDENT + placed.w[i] / 2.0;
            y[i] = row;
            row += placed.h[i] + ROW_GAP;
        }
    } else if n > 0 {
        // The first branches go right and the rest left, split where the two sides are closest in
        // height.
        let branches = &nodes[0].children;
        let count = branches.len();
        let mut total = 0.0;
        for &branch in branches {
            total += bands.extent[branch];
        }
        let mut split = count;
        let mut best = f64::INFINITY;
        let mut right = 0.0;
        for k in 1..count {
            right += bands.extent[branches[k - 1]];
            let diff = (2.0 * right - total + (2.0 * k as f64 - count as f64) * BRANCH_GAP_Y).abs();
            if diff < best {
                best = diff;
                split = k;
            }
        }
        let mut band = vec![0.0; n];
        for (side, dir) in [(&branches[..split], 1.0), (&branches[split..], -1.0)] {
            if side.is_empty() {
                continue;
            }
            let (middle, _) = bands.stack(nodes, side, BRANCH_GAP_Y, &placed.anchor);
            for &c in side {
                band[c] = bands.rel[c] - middle;
                placed.side[c] = dir;
            }
        }
        y[0] = -placed.h[0] / 2.0;
        for i in 0..n {
            if i > 0 {
                y[i] = band[i] + bands.own[i];
            }
            for &c in &nodes[i].children {
                if i > 0 {
                    placed.side[c] = placed.side[i];
                    band[c] = band[i] + bands.kids[i] + bands.rel[c];
                }
                let gap = if i > 0 { GAP_X } else { BRANCH_GAP_X };
                x[c] = x[i] + placed.side[c] * (placed.w[i] / 2.0 + gap + placed.w[c] / 2.0);
            }
        }
    }
    placed
}

/// A link from a node's parent to it.
struct Link {
    path: PathProgram,
    from: usize,
    to: usize,
}

/// A first-level branch with everything under it: its links, drawn first, then its nodes.
struct Branch {
    section: usize,
    links: Vec<Link>,
    nodes: Vec<usize>,
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

fn draw_once(model: &MindmapModel, cx: &DrawContext<'_>, down: bool) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let pad = cx.options.padding;
    let nodes = &model.nodes;
    let n = nodes.len();
    let p = place(model, cx, down);

    let (mut min_x, mut max_x, mut min_y, mut max_y) = (0.0, 0.0, 0.0, 0.0);
    let mut branches: Vec<Branch> = Vec::new();
    // Running down: how far the trunk under each node has been drawn.
    let mut trunk = vec![0.0; n];
    // The root is drawn last, on top of the links that leave it.
    for k in 1..=n {
        let i = k % n;
        let (cx_, top, w, h) = (p.x[i], p.y[i], p.w[i], p.h[i]);
        min_x = js::min(min_x, cx_ - w / 2.0);
        max_x = js::max(max_x, cx_ + w / 2.0);
        min_y = js::min(min_y, top);
        max_y = js::max(max_y, top + h);
        if i == 0 {
            break;
        }
        let parent = p.parent[i];
        if parent == 0 {
            branches.push(Branch { section: nodes[i].section.unwrap_or(0), links: Vec::new(), nodes: Vec::new() });
        }
        let shape = p.shapes[i];
        let path = if down {
            // The link drops from under its parent and turns in to the node's left. It starts
            // where the link to the sibling above turned in, so each stretch of a shared trunk has
            // one colour. Under the root's rounded end, the trunk starts where that end has come
            // down to the bottom.
            let stem = if parent == 0 { js::min(js::max(STEM, p.h[0] / 2.0), INDENT - BEND) } else { STEM };
            let x1 = p.x[parent] - p.w[parent] / 2.0 + js::min(stem, p.w[parent] / 2.0);
            let y1 = if trunk[parent] != 0.0 { trunk[parent] } else { p.y[parent] + p.h[parent] };
            let left = cx_ - w / 2.0;
            let x2 = left + shape.map_or(0.0, |shape| shape_inset(shape, w, h, LEFT, 0.0));
            let y2 = top + p.anchor[i];
            let bend = js::max(0.0, js::min(js::min(BEND, x2 - x1), y2 - y1));
            // The next link carries on from where this one leaves the trunk, so the trunk has no gap.
            trunk[parent] = y2 - bend;
            let mut path = PathProgram::default().m(Number::num(x1), Number::num(y1)).v(Number::num(y2 - bend));
            path.steps.push(PathStep::Quad([pair(x1, y2), pair(x1 + bend, y2)]));
            path.h(Number::num(x2))
        } else {
            let dir = p.side[i];
            let from = if dir > 0.0 { RIGHT } else { LEFT };
            let (pw, ph) = (p.w[parent], p.h[parent]);
            let inset = p.shapes[parent].map_or(0.0, |shape| shape_inset(shape, pw, ph, from, 0.0));
            let x1 = p.x[parent] + dir * (pw / 2.0 - inset);
            let y1 = p.y[parent] + p.anchor[parent];
            let near = cx_ - (dir * w) / 2.0;
            let x2 = near + shape.map_or(0.0, |shape| dir * shape_inset(shape, w, h, 4 - from, 0.0));
            let y2 = top + p.anchor[i];
            let mid = Number::num((x1 + x2) / 2.0);
            PathProgram::default().m(Number::num(x1), Number::num(y1)).c(
                [mid, Number::num(y1)],
                [mid, Number::num(y2)],
                pair(x2, y2),
            )
        };
        let branch = branches.last_mut().expect("a node off the root is in a branch");
        branch.links.push(Link { path, from: parent, to: i });
        branch.nodes.push(i);
    }

    let measurer = cx.measure;
    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measurer, size, fit::title_room(cx.options), Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let width = js::max(max_x - min_x, title.width);
    let total = Size { width: (width + 2.0 * pad).ceil(), height: (max_y - min_y + title_height + 2.0 * pad).ceil() };
    let mut out = SceneBuilder::new(cx, total, Accessible::default());
    let at = Point { x: total.width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at });

    let mut ranks: HashMap<&JsText, u32> = HashMap::new();
    let parts: Vec<PartId> = nodes
        .iter()
        .map(|node| {
            let rank = ranks.entry(&node.node_id).or_insert(0);
            let duplicate = std::mem::replace(rank, *rank + 1);
            out.part(Entity::node(&node.node_id).ranked(duplicate))
        })
        .collect();
    let mut edge_ranks: HashMap<EntityName, u32> = HashMap::new();
    let mut edge_part = |out: &mut SceneBuilder, link: &Link| {
        let (from, to) = (&nodes[link.from].node_id, &nodes[link.to].node_id);
        let name = EntityName::Between { from: from.to_vec(), to: to.to_vec(), kind: String::new() };
        let rank = edge_ranks.entry(name.clone()).or_insert(0);
        let duplicate = std::mem::replace(rank, *rank + 1);
        let part = out.part(Entity::edge(to, name, duplicate));
        out.relation(part, parts[link.from], parts[link.to]);
        part
    };

    let shift = Point { x: pad - min_x + (width - max_x + min_x) / 2.0, y: pad + title_height - min_y };
    let draw = Drawing { model, placed: &p, parts: &parts, down };
    out.layer(LayerSpec::group().translate(shift).keep_empty(), |out| {
        for branch in &branches {
            let colour = Paint::Series(SeriesSlot::new(branch.section));
            let class = format!("pele-branch pele-section-{}", branch.section);
            out.layer(LayerSpec::class(&class), |out| {
                let edges =
                    LayerSpec::class("pele-edges").fill(Paint::None).stroke(colour).stroke_width(Number::raw(1.5));
                out.layer(edges, |out| {
                    for link in &branch.links {
                        let part = edge_part(out, link);
                        out.element(part, Facet::Shape).class("pele-edge").path(link.path.clone()).finish();
                    }
                });
                for &i in &branch.nodes {
                    draw.node(out, i, Some(colour));
                }
            });
        }
        if n > 0 {
            draw.node(out, 0, None);
        }
    });
    out.finish()
}

/// What drawing a node needs from the rest of the map.
struct Drawing<'a> {
    model: &'a MindmapModel,
    placed: &'a Placed,
    parts: &'a [PartId],
    down: bool,
}

impl Drawing<'_> {
    /// A node, in its branch's colour, or neutral for the root.
    fn node(&self, out: &mut SceneBuilder, i: usize, colour: Option<Paint>) {
        let (node, p) = (&self.model.nodes[i], self.placed);
        let part = self.parts[i];
        let shape = p.shapes[i];
        let pill = shape.is_none() && !node.children.is_empty();
        let name = shape.map_or(if pill { "branch" } else { "text" }, Shape::name);
        let own = format!("pele-node pele-shape-{name}{}", if i > 0 { "" } else { " pele-root" });
        let classes = AuthorClasses::new(node.class.as_deref().unwrap_or_default());
        let (w, h) = (p.w[i], p.h[i]);
        let group = LayerSpec::classes(&own, &classes)
            .part(part, Facet::Shape)
            .data_id()
            .translate(Point { x: p.x[i], y: p.y[i] + h / 2.0 });
        out.layer(group, |out| {
            // Every node off the root is filled with a tint of its branch's colour and outlined in
            // it, whether its shape was given in the source or is the pill of a node that branches.
            // The root is neutral.
            match (shape, colour) {
                (Some(shape), Some(colour)) => {
                    let look = ShapeLook::tinted(colour, TINT, None);
                    draw_shape(out, part, shape, w, h, &look, &LineLook::painted(colour, None));
                }
                (Some(shape), None) => {
                    let look = ShapeLook::new(Role::Surface, Role::Border, None);
                    draw_shape(out, part, shape, w, h, &look, &LineLook::new(Role::Border, None));
                }
                (None, Some(colour)) if pill => {
                    out.element(part, Facet::Shape)
                        .rect(pair(-w / 2.0, -h / 2.0), [Number::num(w), Number::num(h)], Some(Number::num(h / 2.0)))
                        .look(ShapeLook::tinted(colour, TINT, None).plain())
                        .finish();
                }
                (None, _) => {}
            }
            let label = &p.labels[i];
            let look = || LabelLook::class("pele-label");
            if self.down && shape.is_none() {
                // An outline's text starts at its left, where centred lines would look ragged.
                let left = 0.0 - label.width / 2.0;
                if label.lines.len() < 2 {
                    let at = Point { x: left + label.width / 2.0, y: p.dy[i] };
                    out.label(part, Facet::Label, label, at, look());
                } else {
                    let top = p.dy[i] - label.height / 2.0;
                    for (k, line) in label.lines.iter().enumerate() {
                        let at = Point { x: left + line.width / 2.0, y: top + label.line_middle(k) };
                        out.label(part, Facet::Label, &label.line(k), at, look());
                    }
                }
            } else {
                out.label(part, Facet::Label, label, Point { x: 0.0, y: p.dy[i] }, look());
            }
        });
    }
}
