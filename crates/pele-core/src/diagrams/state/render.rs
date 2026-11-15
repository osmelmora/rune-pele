//! Drawing a state diagram, ported from Pele's `src/diagrams/state/render.ts`: states are sized
//! from their labels, laid out with their composites, regions, notes and transitions, and drawn
//! as a Scene.

use super::graph::{GraphEdge, GraphNode, Shape};
use super::{Direction, StateModel};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::graph::edges::{
    Curve, EdgePath, Fill, Marker, MarkerUse, Over, SideReach, Tip, casings_layer, draw_casing, draw_edge_label,
    draw_marker, over_fills_at_end, route_path,
};
use crate::graph::shapes::end_side;
use crate::input::url::{link_allowed, safe_url};
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities};
use crate::layout::num::hypot;
use crate::layout::{Axis, CEdge, CNode, Dir, EdgeStyle, LayeredOptions, RoutePoint, compound_layout};
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, AuthorClasses, ElementStart, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, Number, Paint,
    PartId, PathProgram, Point, ResolvedStyle, Role, Scene, SceneBuilder, Size, Style, TitleLayout, ink_on, paints,
    resolve_style,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 44.0;
const GROUP_PAD: f64 = 16.0;
const REGION_PAD: f64 = 10.0;
const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 10.0;
const LOOP: f64 = 26.0;
const FOLD: f64 = 8.0;
const NOTE_WRAP: f64 = 320.0;
const CORNER: f64 = 10.0;
const BAR: f64 = 8.0;

/// What a state is drawn as. The order is Pele's: the kinds from `Region` on are not drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum K {
    State,
    Described,
    Start,
    End,
    Choice,
    Fork,
    Join,
    Note,
    Composite,
    Region,
    /// An unseen helper the layout needs: an anchor inside a composite, or the box that holds a
    /// state's notes or loops.
    Hidden,
}

impl K {
    fn of(node: &GraphNode) -> K {
        if node.is_group {
            return if node.shape == Some(Shape::Divider) { K::Region } else { K::Composite };
        }
        match node.shape {
            Some(Shape::RectWithTitle) => K::Described,
            Some(Shape::StateStart) => K::Start,
            Some(Shape::StateEnd) => K::End,
            Some(Shape::Choice) => K::Choice,
            Some(Shape::Fork) => K::Fork,
            Some(Shape::Join) => K::Join,
            Some(Shape::Note) => K::Note,
            Some(Shape::Divider) => K::Hidden,
            _ => K::State,
        }
    }

    /// The classes of the group a drawn kind is written in.
    fn classes(self) -> &'static str {
        match self {
            K::State | K::Described => "pele-node pele-state",
            K::Start => "pele-node pele-state-start",
            K::End => "pele-node pele-state-end",
            K::Choice => "pele-node pele-state-choice",
            K::Fork => "pele-node pele-state-fork",
            K::Join => "pele-node pele-state-join",
            K::Note => "pele-note",
            K::Composite | K::Region | K::Hidden => "pele-cluster pele-state-composite",
        }
    }

    fn bar(self) -> bool {
        matches!(self, K::Fork | K::Join)
    }
}

/// A state as it is drawn: its style, its label and the size of its shape.
struct View<'a> {
    /// Absent for the unseen helpers the layout needs.
    node: Option<&'a GraphNode>,
    kind: K,
    style: ResolvedStyle,
    label: Label,
    lines: Vec<Label>,
    w: f64,
    h: f64,
    title_h: f64,
}

impl<'a> View<'a> {
    fn new(node: Option<&'a GraphNode>, kind: K) -> Self {
        let label = Label::default();
        View { node, kind, style: ResolvedStyle::default(), label, lines: Vec::new(), w: 0.0, h: 0.0, title_h: 0.0 }
    }
}

fn node_of<'a>(views: &[View<'a>], i: usize) -> &'a GraphNode {
    views[i].node.expect("a state has a node")
}

/// Adds an unseen helper the layout needs.
fn add_hidden(views: &mut Vec<View<'_>>, cnodes: &mut Vec<CNode>, c: CNode) -> usize {
    views.push(View::new(None, K::Hidden));
    cnodes.push(c);
    views.len() - 1
}

fn vertical(dir: Dir) -> bool {
    matches!(dir, Dir::TB | Dir::BT)
}

fn named_dir(dir: Option<Direction>) -> Dir {
    Dir::named(dir.map(Direction::name))
}

/// Whether the flow around state `i` runs top to bottom or bottom to top. As in Mermaid, a
/// composite that sets no direction runs top to bottom whatever surrounds it.
fn flows_down(views: &[View<'_>], logical: &[Option<usize>], root_dir: Dir, i: usize) -> bool {
    vertical(logical[i].map_or(root_dir, |composite| named_dir(node_of(views, composite).dir)))
}

/// A number the `state` config gives for `key`, where it is one above zero, capped at 2000.
fn number_option(section: Option<&JsObject>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|section| section.get(key)) {
        Some(&JsValue::Number(value)) if value > 0.0 => js::min(value, 2000.0),
        _ => fallback,
    }
}

/// Mermaid keeps the quotes of a click statement's strings and strips them when it draws.
fn unquote(text: &[u16]) -> &[u16] {
    let from = text.run(0, |c| c == 34);
    let to = text.iter().rposition(|&c| c != 34).map_or(from, |last| (last + 1).max(from));
    &text[from..to]
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `Math.sign`
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

fn group(dir: Dir, pad_x: f64, pad_top: f64, pad_bottom: f64) -> CNode {
    let mut c = CNode::new(0.0, 0.0);
    c.is_group = true;
    c.dir = Some(dir);
    c.pad_x = pad_x;
    c.pad_top = pad_top;
    c.pad_bottom = pad_bottom;
    c
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

/// `classes.replace(/(?:^| )statediagram-(?:state|cluster-alt|cluster|note)(?= |$)/g, '')`: the
/// classes Mermaid gives every state, which Pele writes as classes of its own.
fn without_mermaid_classes(classes: &[u16]) -> Vec<u16> {
    const NAMES: [&str; 4] =
        ["statediagram-state", "statediagram-cluster-alt", "statediagram-cluster", "statediagram-note"];
    // Where a name that starts at `at` and is followed by a space or the end ends.
    let name_end = |at: usize| {
        NAMES
            .iter()
            .filter(|name| classes.starts_at(at, name))
            .map(|name| at + name.len())
            .find(|&end| classes.get(end).is_none_or(|&c| c == 32))
    };
    let mut out = Vec::with_capacity(classes.len());
    let mut at = 0;
    while at < classes.len() {
        let found = (at == 0)
            .then(|| name_end(0))
            .flatten()
            .or_else(|| (classes[at] == 32).then(|| name_end(at + 1)).flatten());
        match found {
            Some(end) => at = end,
            None => {
                out.push(classes[at]);
                at += 1;
            }
        }
    }
    out
}

/// A rounded y as a map key, where `-0` is `0` and every NaN is one key, as in a JavaScript `Map`.
fn crossing_key(y: f64) -> u64 {
    let rounded = js::round(y);
    if rounded.is_nan() { f64::NAN.to_bits() } else { (rounded + 0.0).to_bits() }
}

/// The x of every route point inside a route, by its rounded y: the edges that cross a
/// composite's title bar.
#[derive(Default)]
struct Crossings {
    by_y: HashMap<u64, Vec<f64>>,
}

/// Pele's `titleX`: where a composite's title sits in its title bar so that edges entering from
/// above do not cross it.
fn title_x(c: &CNode, width: f64, crossings: &Crossings) -> f64 {
    let Some(xs) = crossings.by_y.get(&crossing_key(c.y - c.h / 2.0)).filter(|_| width != 0.0) else {
        return c.x;
    };
    let left = c.x - c.w / 2.0;
    let half = width / 2.0;
    let mut best = c.x;
    let mut best_gap = -1.0;
    for x in [c.x, left + GROUP_PAD + half, left + c.w - GROUP_PAD - half] {
        let mut gap = f64::INFINITY;
        for &cx in xs {
            if cx > left && cx < left + c.w {
                gap = js::min(gap, (cx - x).abs() - half);
            }
        }
        if gap >= 6.0 {
            return x;
        }
        if gap > best_gap + 0.5 {
            best_gap = gap;
            best = x;
        }
    }
    best
}

/// A dashed line between two regions, or the outline of a region that is neither beside nor
/// above another. `part` is the composite the regions are in, or none at the root.
enum Divider {
    Down { part: Option<PartId>, at: f64, from: f64, to: f64 },
    Across { part: Option<PartId>, at: f64, from: f64, to: f64 },
    Outline { part: Option<PartId>, x: f64, y: f64, w: f64, h: f64 },
}

/// Starts a divider: a part of its composite, or of the diagram at the root.
fn start_divider(out: &mut SceneBuilder, part: Option<PartId>) -> ElementStart<'_> {
    match part {
        Some(part) => out.element(part, Facet::Shape),
        None => out.decoration(),
    }
}

/// An edge as it is drawn.
struct EdgeView<'a> {
    edge: &'a GraphEdge,
    part: PartId,
    path: EdgePath,
    over: Over,
    end: Marker,
    label: Label,
    label_at: Point,
}

/// Pele's `draw`. `turned` draws a diagram that runs across as one that runs down.
fn draw(model: &StateModel, cx: &DrawContext<'_>, turned: bool) -> Result<Scene, RenderError> {
    let graph = &model.graph;
    // State diagrams follow the flowchart curve.
    let flow = cx.config.get("flowchart").and_then(JsValue::as_record);
    let curve_name = flow.and_then(|flow| flow.get("curve")).and_then(JsValue::as_text);
    let section = cx.config.get("state").and_then(JsValue::as_record);
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let wrap_width = number_option(section, "wrappingWidth", 200.0);
    let pad = cx.options.padding;
    let policy = &cx.options.resources;
    let root_dir = if turned { Dir::TB } else { named_dir(Some(model.direction())) };
    let text = |raw: Option<&JsText>, font_size: f64, wrap: f64, style: Emphasis| {
        layout_label(raw.map_or(&[][..], |raw| raw), true, cx.measure, font_size, wrap, style)
    };

    let mut index: HashMap<&[u16], usize> = HashMap::new();
    let mut views: Vec<View<'_>> = Vec::new();
    let mut cnodes: Vec<CNode> = Vec::new();
    for node in &graph.nodes {
        if node.shape == Some(Shape::NoteGroup) {
            continue;
        }
        index.insert(&node.id, views.len());
        views.push(View::new(Some(node), K::of(node)));
    }
    let states = views.len();

    let mut logical: Vec<Option<usize>> = (0..states)
        .map(|i| {
            let parent = node_of(&views, i).parent_id.as_ref().and_then(|id| index.get(&**id).copied());
            parent.filter(|&parent| parent != i && node_of(&views, parent).is_group)
        })
        .collect();
    // Composites that are redeclared inside each other form a ring with no way in from the
    // root. One member of each ring moves to the root, which leaves a tree.
    {
        let mut kids: Vec<Vec<usize>> = vec![Vec::new(); states];
        let mut queue: Vec<usize> = Vec::new();
        let mut placed = vec![false; states];
        for i in 0..states {
            match logical[i] {
                None => {
                    placed[i] = true;
                    queue.push(i);
                }
                Some(parent) => kids[parent].push(i),
            }
        }
        let mut done = 0;
        let settle = |queue: &mut Vec<usize>, placed: &mut Vec<bool>, done: &mut usize| {
            while *done < queue.len() {
                for &kid in &kids[queue[*done]] {
                    if !placed[kid] {
                        placed[kid] = true;
                        queue.push(kid);
                    }
                }
                *done += 1;
            }
        };
        settle(&mut queue, &mut placed, &mut done);
        let mut seen: Vec<Option<usize>> = vec![None; states];
        for i in 0..states {
            if placed[i] {
                continue;
            }
            let mut ring = i;
            while seen[ring] != Some(i) {
                seen[ring] = Some(i);
                ring = logical[ring].expect("a state off the tree has a parent");
            }
            logical[ring] = None;
            placed[ring] = true;
            queue.push(ring);
            settle(&mut queue, &mut placed, &mut done);
        }
    }
    let mut owners: Vec<Option<usize>> = vec![None; states];
    for i in 0..states {
        let owner = node_of(&views, i).note_for.as_ref().and_then(|id| index.get(&**id).copied());
        let Some(owner) = owner else {
            continue;
        };
        owners[i] = Some(owner);
        logical[i] = logical[owner];
    }

    let mut degree = vec![0u32; states];
    let mut noted = vec![false; states];
    // When each state first takes part in a transition. Layout follows this order rather than
    // the order of declaration, so a state declared up front does not become the top of the flow.
    let mut appear: Vec<f64> = vec![f64::INFINITY; states];
    let mut appeared = 0.0;
    let mut counts = vec![0u32; 2 * states];
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(&*edge.start), index.get(&*edge.end)) else {
            continue;
        };
        if edge.dashed {
            noted[if views[src].kind == K::Note { dst } else { src }] = true;
            continue;
        }
        counts[2 * src] += 1;
        degree[src] = degree[src].max(counts[2 * src]);
        counts[2 * dst + 1] += 1;
        degree[dst] = degree[dst].max(counts[2 * dst + 1]);
        for end in [src, dst] {
            if appear[end] == f64::INFINITY {
                appear[end] = appeared;
                appeared += 1.0;
            }
        }
    }

    for i in 0..states {
        let node = node_of(&views, i);
        let style = resolve_style(node.css_compiled_styles.iter().chain(&node.css_styles));
        let text_style = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
        let font_size = style.font_size.unwrap_or(size);
        let down = flows_down(&views, &logical, root_dir, i);
        let view = &mut views[i];
        let kind = view.kind;
        let mut c = None;
        match kind {
            K::Composite => {
                view.label = text(node.label.as_ref(), font_size, 4000.0, text_style);
                view.title_h = if view.label.height > 0.0 { view.label.height + 12.0 } else { 0.0 };
                let mut g = group(named_dir(node.dir), GROUP_PAD, view.title_h + GROUP_PAD, GROUP_PAD);
                g.min_w = view.label.width + 2.0 * GROUP_PAD;
                c = Some(g);
            }
            K::Region => c = Some(group(named_dir(node.dir), REGION_PAD, REGION_PAD, REGION_PAD)),
            K::Start => (view.w, view.h) = (14.0, 14.0),
            K::End => (view.w, view.h) = (18.0, 18.0),
            K::Choice => (view.w, view.h) = (30.0, 30.0),
            K::Fork | K::Join => {
                let length = clamp(40.0 * f64::from(degree[i]) + 16.0, 72.0, 320.0);
                (view.w, view.h) = if down { (length, BAR) } else { (BAR, length) };
            }
            K::Hidden => {}
            K::Note => {
                view.label = text(node.label.as_ref(), small, js::max(wrap_width, NOTE_WRAP), Emphasis::default());
                view.w = view.label.width + 24.0;
                view.h = view.label.height + 16.0;
            }
            K::State | K::Described => {
                view.label = text(node.label.as_ref(), font_size, wrap_width, text_style);
                let mut width = view.label.width;
                let mut height = view.label.height + 2.0 * PAD_Y;
                if kind == K::Described {
                    height += 14.0;
                    for line in &node.description {
                        let line = if line.is_empty() { JsText::from(" ") } else { line.clone() };
                        let label = text(Some(&line), small, wrap_width, text_style);
                        if label.width > width {
                            width = label.width;
                        }
                        height += label.height;
                        view.lines.push(label);
                    }
                }
                view.w = js::max(width + 2.0 * PAD_X, 56.0);
                view.h = height;
            }
        }
        view.style = style;
        let mut c = c.unwrap_or_else(|| CNode::new(view.w, view.h));
        // The transitions on one side of a state's box spread along it, clear of its corners.
        if matches!(kind, K::State | K::Described) {
            c.span = (if down { view.w } else { view.h }) - 24.0;
        }
        cnodes.push(c);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    // The transitions each state makes to itself: how many, their widest label, and their
    // labels' height.
    let mut loop_count = vec![0u32; states];
    let mut loop_width = vec![0.0; states];
    let mut loop_height = vec![0.0; states];
    // A transition between a composite and a state inside it runs between that state and a
    // point just inside the composite, and is then drawn on to the composite's border.
    let mut anchors: HashMap<usize, usize> = HashMap::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(&*edge.start), index.get(&*edge.end)) else {
            continue;
        };
        let label = text(Some(&edge.label), small, wrap_width, Emphasis::default());
        let label_w = if label.width > 0.0 { label.width + 12.0 } else { 0.0 };
        let encloses = |composite: usize, item: usize| {
            if !cnodes[composite].is_group {
                return false;
            }
            let mut p = logical[item];
            while let Some(at) = p {
                if at == composite {
                    return true;
                }
                p = logical[at];
            }
            false
        };
        let (mut src, mut dst) = (src, dst);
        if src == dst {
            loop_count[src] += 1;
            loop_width[src] = js::max(loop_width[src], label_w);
            loop_height[src] += label.height;
        } else if encloses(src, dst) {
            anchors.insert(cedges.len(), src);
            appear.push(-1.0);
            let mut anchor = CNode::new(0.0, 0.0);
            anchor.parent = Some(src);
            src = add_hidden(&mut views, &mut cnodes, anchor);
        } else if encloses(dst, src) {
            anchors.insert(cedges.len(), dst);
            appear.push(f64::INFINITY);
            let mut anchor = CNode::new(0.0, 0.0);
            anchor.parent = Some(dst);
            dst = add_hidden(&mut views, &mut cnodes, anchor);
        }
        let mut cedge = CEdge::new(src, dst);
        cedge.label_w = label_w;
        cedge.label_h = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
        drawn.push(edge);
        edge_labels.push(label);
        cedges.push(cedge);
    }

    // A state with notes, or a composite with self-transitions, sits in an unseen group of its
    // own: the notes go beside the state, across the flow, without disturbing its transitions.
    for i in 0..states {
        if owners[i].is_some() {
            continue;
        }
        let down = flows_down(&views, &logical, root_dir, i);
        let item = &mut cnodes[i];
        // Loops hang off the right, or below when the flow is horizontal, with their labels
        // beyond them.
        let mut room = 0.0;
        if loop_count[i] > 0 {
            let reach = LOOP + f64::from(loop_count[i] - 1) * 8.0;
            room = if down { reach + loop_width[i] } else { reach * 0.75 + 4.0 + loop_height[i] };
            if !item.is_group {
                if down {
                    item.w += 2.0 * room;
                    item.h = js::max(item.h, loop_height[i]);
                } else {
                    item.h += 2.0 * room;
                    item.w = js::max(item.w, loop_width[i]);
                }
                room = 0.0;
            }
        }
        if !noted[i] && room == 0.0 {
            item.parent = logical[i];
            continue;
        }
        let mut c = if down { group(Dir::LR, room, 0.0, 0.0) } else { group(Dir::TB, 0.0, room, room) };
        c.parent = logical[i];
        appear.push(appear[i]);
        let hidden = add_hidden(&mut views, &mut cnodes, c);
        cnodes[i].parent = Some(hidden);
    }
    for i in 0..states {
        if let Some(owner) = owners[i] {
            cnodes[i].parent = cnodes[owner].parent;
        }
    }

    {
        // Numbers the tree so that, at every level, initial states come first and the rest
        // follow in the order the transitions introduce them.
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); cnodes.len()];
        let mut top: Vec<usize> = Vec::new();
        for (i, c) in cnodes.iter().enumerate() {
            match c.parent {
                Some(p) => members[p].push(i),
                None => top.push(i),
            }
        }
        let initial = |i: usize| u8::from(views[i].kind != K::Start);
        let by_flow = |a: &usize, b: &usize| {
            initial(*a)
                .cmp(&initial(*b))
                .then_with(|| appear[*a].partial_cmp(&appear[*b]).expect("no appearance is NaN"))
                .then_with(|| a.cmp(b))
        };
        let mut seq = 0.0;
        top.sort_by(by_flow);
        top.reverse();
        let mut pending = top;
        while let Some(i) = pending.pop() {
            cnodes[i].seq = seq;
            seq += 1.0;
            let mut inside = std::mem::take(&mut members[i]);
            inside.sort_by(by_flow);
            pending.extend(inside.into_iter().rev());
        }
    }

    let stepped =
        curve_name.is_some_and(|name| name.is_str("step") || name.is_str("stepBefore") || name.is_str("stepAfter"));
    let options = LayeredOptions {
        node_sep: number_option(section, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(section, "rankSpacing", RANK_SEP),
        port_sep: 20.0,
        style: if stepped {
            EdgeStyle::Stepped
        } else if curve_name.is_some_and(|name| name.is_str("linear")) {
            EdgeStyle::Linear
        } else {
            EdgeStyle::Curved
        },
        apart: false,
        head_room: Marker::Point.trim(),
    };
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &options);

    let width = (layout.width + 2.0 * pad).ceil();
    let height = (layout.height + 2.0 * pad).ceil();

    // Route points by rounded y, to find the edges that cross a composite's title bar.
    let mut crossings = Crossings::default();
    for e in &cedges {
        for point in e.route.iter().skip(1).take(e.route.len().saturating_sub(2)) {
            crossings.by_y.entry(crossing_key(point.y)).or_default().push(point.x);
        }
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); cnodes.len()];
    let mut order: Vec<usize> = Vec::new();
    for (i, c) in cnodes.iter().enumerate() {
        match c.parent {
            Some(p) => children[p].push(i),
            None => order.push(i),
        }
    }
    let roots = order.clone();
    let mut q = 0;
    while q < order.len() {
        order.extend(children[order[q]].iter().copied());
        q += 1;
    }

    // Pele's `withTitle`: a title above the drawing, which then moves down and to the middle.
    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, cx.measure, size, 4000.0, Emphasis::BOLD);
    let (full, full_height, shift) = if title.height == 0.0 {
        (width, height, None)
    } else {
        let shift = title.height + 12.0;
        let full = js::max(width, (title.width + 2.0 * pad).ceil());
        (full, (height + shift).ceil(), Some(Point { x: (full - width) / 2.0, y: shift }))
    };

    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width: full, height: full_height }, accessible);
    if shift.is_some() {
        out.title(&TitleLayout { at: Point { x: full / 2.0, y: pad + title.height / 2.0 }, label: title });
    }

    // Each state's part, with the tooltip its click statement gives.
    let tooltips: Vec<Option<JsText>> = (0..states)
        .map(|i| {
            let link = model.links.get(&node_of(&views, i).id)?;
            let tooltip = unquote(&link.tooltip);
            (!decode_entities(tooltip).is_empty()).then(|| tooltip.into())
        })
        .collect();
    let node_parts: Vec<PartId> = (0..states)
        .map(|i| {
            let node = node_of(&views, i);
            let entity = if node.is_group { Entity::group(&node.id) } else { Entity::node(&node.id) };
            out.part(entity.tooltip(tooltips[i].as_ref()))
        })
        .collect();
    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let edge_parts: Vec<PartId> = drawn
        .iter()
        .map(|edge| {
            let kind = if edge.dashed { "note" } else { "transition" };
            let name = EntityName::Between { from: edge.start.to_vec(), to: edge.end.to_vec(), kind: kind.to_owned() };
            let duplicate = seen.entry(name.clone()).or_insert(0);
            let part = out.part(Entity::edge(&edge.id, name, *duplicate));
            *duplicate += 1;
            part
        })
        .collect();
    for (i, edge) in drawn.iter().enumerate() {
        if let (Some(&from), Some(&to)) = (index.get(&*edge.start), index.get(&*edge.end)) {
            out.relation(edge_parts[i], node_parts[from], node_parts[to]);
        }
    }

    let mut fills: Vec<Fill<'_>> = Vec::new();
    let mut dividers: Vec<Divider> = Vec::new();
    let mut clusters: Vec<usize> = Vec::new();
    let mut shapes: Vec<usize> = Vec::new();
    let mut notes: Vec<usize> = Vec::new();
    // Draws the dashed lines between the regions among a composite's members.
    let divide = |dividers: &mut Vec<Divider>, part: Option<PartId>, items: &[usize], bounds: [f64; 4]| {
        let [left, top, right, bottom] = bounds;
        let regions: Vec<usize> = items.iter().copied().filter(|&i| views[i].kind == K::Region).collect();
        if regions.len() < 2 {
            return;
        }
        let lo = |i: usize, x: bool| if x { cnodes[i].x - cnodes[i].w / 2.0 } else { cnodes[i].y - cnodes[i].h / 2.0 };
        let hi = |i: usize, x: bool| if x { cnodes[i].x + cnodes[i].w / 2.0 } else { cnodes[i].y + cnodes[i].h / 2.0 };
        for x in [true, false] {
            let mut sorted = regions.clone();
            sorted.sort_by(|&a, &b| lo(a, x).partial_cmp(&lo(b, x)).expect("a layout is finite").then(a.cmp(&b)));
            if sorted.windows(2).any(|pair| hi(pair[0], x) > lo(pair[1], x) + 0.5) {
                continue;
            }
            for pair in sorted.windows(2) {
                let at = (hi(pair[0], x) + lo(pair[1], x)) / 2.0 + pad;
                dividers.push(if x {
                    Divider::Down { part, at, from: top, to: bottom }
                } else {
                    Divider::Across { part, at, from: left, to: right }
                });
            }
            return;
        }
        // Regions that sit neither side by side nor in a stack each get an outline.
        for &i in &regions {
            let c = &cnodes[i];
            dividers.push(Divider::Outline {
                part,
                x: c.x - c.w / 2.0 + pad,
                y: c.y - c.h / 2.0 + pad,
                w: c.w,
                h: c.h,
            });
        }
    };
    divide(&mut dividers, None, &roots, [pad / 2.0, pad / 2.0, width - pad / 2.0, height - pad / 2.0]);

    for &i in &order {
        let view = &views[i];
        if view.node.is_none() || view.kind >= K::Region {
            continue;
        }
        match view.kind {
            K::Composite => {
                let c = &cnodes[i];
                let left = c.x + pad - c.w / 2.0;
                let top = c.y + pad - c.h / 2.0;
                if paints(view.style.shape.as_ref()) {
                    fills.push(Fill { bounds: [left, top, left + c.w, top + c.h], shape: view.style.shape.as_ref() });
                }
                let bounds = [left, top + view.title_h, left + c.w, top + c.h];
                divide(&mut dividers, Some(node_parts[i]), &children[i], bounds);
                clusters.push(i);
            }
            K::Note => notes.push(i),
            _ => shapes.push(i),
        }
    }

    // Moves a route end from the layout box onto the drawn shape, and spreads the ends along
    // a bar.
    let attach = |route: &mut [RoutePoint], at: usize, i: usize| {
        let c = &cnodes[i];
        if c.is_group {
            return;
        }
        let view = &views[i];
        let other = if at == 0 { 1 } else { at - 1 };
        for down in [true, false] {
            let (extent, drawn_extent, breadth) = if down { (c.h, view.h, view.w) } else { (c.w, view.w, view.h) };
            let (center, cross) = if down { (c.y + pad, c.x + pad) } else { (c.x + pad, c.y + pad) };
            let along = if down { route[at].y } else { route[at].x };
            let delta = along - center;
            if (delta.abs() - extent / 2.0).abs() >= 0.5 {
                continue;
            }
            let moved = along - (sign(delta) * (extent - drawn_extent)) / 2.0;
            let neighbour = if down { route[other].x } else { route[other].y };
            let spread = (view.kind.bar() && breadth > drawn_extent)
                .then(|| clamp(neighbour, cross - breadth / 2.0 + 6.0, cross + breadth / 2.0 - 6.0));
            let point = &mut route[at];
            if down {
                point.y = moved;
                point.x = spread.unwrap_or(point.x);
            } else {
                point.x = moved;
                point.y = spread.unwrap_or(point.y);
            }
            return;
        }
    };

    let mut reach = SideReach::default();
    for (ce, edge) in cedges.iter().zip(&drawn) {
        if ce.src == ce.dst || ce.route.len() < 2 || edge.dashed {
            continue;
        }
        reach.add(ce.dst, end_side(&ce.route[ce.route.len() - 1], &cnodes[ce.dst]), Marker::Point);
    }

    let curve = Curve::named(curve_name.map(|name| &**name));
    let mut edges: Vec<EdgeView<'_>> = Vec::new();
    let mut loop_seen = vec![0u32; states];
    let mut loop_text = vec![0.0; states];
    for (i, label) in edge_labels.into_iter().enumerate() {
        let edge = drawn[i];
        let end = if edge.dashed { Marker::None } else { Marker::Point };
        let ce = &mut cedges[i];
        let path = if ce.src == ce.dst {
            let a = &cnodes[ce.src];
            let view = &views[ce.src];
            let k = f64::from(loop_seen[ce.src]);
            loop_seen[ce.src] += 1;
            // The loop hangs off a side the flow does not use: the right, or below when the flow
            // is horizontal.
            let side = flows_down(&views, &logical, root_dir, ce.src);
            let w = if a.is_group { a.w } else { view.w };
            let h = if a.is_group { a.h } else { view.h };
            let out_by = (if side { w } else { h }) / 2.0;
            let spread = js::max(js::min((if side { h } else { w }) / 2.0 - 4.0, 8.0 + k * 6.0), 2.0);
            let loop_reach = LOOP + k * 8.0;
            let length = hypot(loop_reach, spread);
            let trim = end.trim() / if length == 0.0 || length.is_nan() { 1.0 } else { length };
            // Offsets are given for a loop on the right and turned a quarter for one below.
            let px = |dx: f64, dy: f64| a.x + pad + if side { out_by + dx } else { -dy };
            let py = |dx: f64, dy: f64| a.y + pad + if side { dy } else { out_by + dx };
            let at = |dx: f64, dy: f64| pair(px(dx, dy), py(dx, dy));
            let first = at(0.0, -spread);
            let d = PathProgram::default().m(first[0], first[1]).c(
                at(loop_reach, -2.0 * spread),
                at(loop_reach, 2.0 * spread),
                at(loop_reach * trim, spread + spread * trim),
            );
            let mut edx = px(-loop_reach, -spread) - px(0.0, 0.0);
            let mut edy = py(-loop_reach, -spread) - py(0.0, 0.0);
            let length = hypot(edx, edy);
            let length = if length == 0.0 || length.is_nan() { 1.0 } else { length };
            edx /= length;
            edy /= length;
            let tip = Tip { x: px(0.0, spread), y: py(0.0, spread), dx: edx, dy: edy };
            // Labels stack beyond the outermost loop, in the order of their transitions.
            let furthest = LOOP + f64::from(loop_count[ce.src] - 1) * 8.0;
            let below = loop_text[ce.src] + label.height / 2.0;
            loop_text[ce.src] += label.height;
            ce.label_x = if side { a.x + out_by + furthest + 4.0 + ce.label_w / 2.0 } else { a.x };
            ce.label_y = if side {
                a.y - loop_height[ce.src] / 2.0 + below
            } else {
                a.y + out_by + furthest * 0.75 + 4.0 + below
            };
            EdgePath { d, start: Tip { x: 0.0, y: 0.0, dx: 0.0, dy: 0.0 }, end: tip }
        } else if ce.route.len() < 2 {
            // A route of one point has no direction to draw along. The layout gives every edge
            // between two nodes at least its two ends, so none is expected here.
            continue;
        } else {
            let mut route = ce.route.clone();
            let last = route.len() - 1;
            let leaves = reach.of(ce.src, end_side(&route[0], &cnodes[ce.src]));
            let arrives = reach.of(ce.dst, end_side(&route[last], &cnodes[ce.dst]));
            for point in &mut route {
                point.x += pad;
                point.y += pad;
            }
            attach(&mut route, 0, ce.src);
            attach(&mut route, last, ce.dst);
            if let Some(&composite) = anchors.get(&i) {
                let g = &cnodes[composite];
                let dir = g.dir.unwrap_or(Dir::TB);
                let inward = views[ce.src].kind == K::Hidden;
                let at = if inward { 0 } else { last };
                let near = matches!(dir, Dir::TB | Dir::LR) == inward;
                let point = if vertical(dir) {
                    let y = g.y + pad + if near { views[composite].title_h - g.h / 2.0 } else { g.h / 2.0 };
                    RoutePoint { x: route[at].x, y, axis: Axis::Vertical }
                } else {
                    let x = g.x + pad + if near { -g.w / 2.0 } else { g.w / 2.0 };
                    RoutePoint { x, y: route[at].y, axis: Axis::Horizontal }
                };
                if inward {
                    route.insert(0, point);
                } else {
                    route.push(point);
                }
            }
            route_path(&route, curve, [0.0, end.trim()], [leaves, arrives])
        };
        let over = over_fills_at_end(&path, &fills, None);
        let label_at = Point { x: ce.label_x + pad, y: ce.label_y + pad };
        edges.push(EdgeView { edge, part: edge_parts[i], path, over, end, label, label_at });
    }

    let link_rel = (!policy.link_rel.is_empty()).then(|| policy.link_rel.clone());
    let draw_state = |out: &mut SceneBuilder, i: usize| {
        let view = &views[i];
        let node = node_of(&views, i);
        let part = node_parts[i];
        let c = &cnodes[i];
        let classes = AuthorClasses::new(&without_mermaid_classes(&node.css_classes));
        let mut spec = LayerSpec::classes(view.kind.classes(), &classes).part(part, Facet::Shape).data_id();
        if view.kind != K::Composite {
            spec = spec.translate(Point { x: c.x + pad, y: c.y + pad });
        }
        let contents = |out: &mut SceneBuilder| {
            if tooltips[i].is_some() {
                out.tooltip(part);
            }
            draw_body(out, part, view, c, pad, small, &crossings);
        };
        let link = model.links.get(&node.id).map(|link| safe_url(unquote(&link.url)));
        out.layer(spec.keep_empty(), |out| match link.filter(|url| link_allowed(url, policy)) {
            Some(href) => out.link_to(part, href, Some("_blank"), link_rel.clone(), contents),
            None => contents(out),
        });
    };

    let layers = |out: &mut SceneBuilder| {
        out.layer(LayerSpec::class("pele-clusters"), |out| {
            for &i in &clusters {
                draw_state(out, i);
            }
        });

        let dashes = [Number::raw(4.0), Number::raw(4.0)];
        let dividers_layer =
            LayerSpec::class("pele-dividers").dash(&dashes).fill(Paint::None).stroke(Paint::Role(Role::Border));
        out.layer(dividers_layer, |out| {
            for line in &dividers {
                match *line {
                    Divider::Down { part, at, from, to } => {
                        let d = PathProgram::default().m(Number::num(at), Number::num(from)).v(Number::num(to));
                        start_divider(out, part).class("pele-divider").path(d).finish();
                    }
                    Divider::Across { part, at, from, to } => {
                        let d = PathProgram::default().m(Number::num(from), Number::num(at)).h(Number::num(to));
                        start_divider(out, part).class("pele-divider").path(d).finish();
                    }
                    Divider::Outline { part, x, y, w, h } => {
                        start_divider(out, part)
                            .rect(pair(x, y), pair(w, h), None)
                            .class("pele-divider")
                            .rx(Number::raw(CORNER))
                            .finish();
                    }
                }
            }
        });

        out.layer(casings_layer(), |out| {
            for view in edges.iter().filter(|view| view.over == Over::Cased) {
                draw_casing(out, view.part, &view.path, [Marker::None, view.end], None, false);
            }
        });

        let edges_layer =
            LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
        out.layer(edges_layer, |out| {
            for view in &edges {
                let ink = match view.over {
                    Over::Ink(ink) => Some(Paint::Role(ink)),
                    Over::Page | Over::Cased => None,
                };
                let spec = if view.edge.dashed {
                    LayerSpec::class("pele-edge pele-note-edge")
                        .stroke(ink.unwrap_or(Paint::Role(Role::Border)))
                        .dash(&[Number::raw(3.0), Number::raw(4.0)])
                } else {
                    let spec = LayerSpec::class("pele-edge pele-transition");
                    match ink {
                        Some(ink) => spec.stroke(ink),
                        None => spec,
                    }
                };
                out.layer(spec.part(view.part, Facet::Shape).data_id(), |out| {
                    out.element(view.part, Facet::Shape).path(view.path.d.clone()).finish();
                    let colour = MarkerUse::Edge(ink.unwrap_or(Paint::Role(Role::Line)));
                    draw_marker(out, view.part, Facet::EndMarker, view.end, view.path.end, colour);
                });
            }
        });

        out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(small)), |out| {
            for view in edges.iter().filter(|view| view.label.width > 0.0) {
                draw_edge_label(out, view.part, &view.label, view.label_at, None);
            }
        });

        out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
            for &i in shapes.iter().chain(&notes) {
                draw_state(out, i);
            }
        });
    };
    match shift {
        Some(shift) => out.layer(LayerSpec::group().translate(shift), layers),
        None => layers(&mut out),
    }
    out.finish()
}

/// What a state is drawn as inside its group: its shape, then its label and description.
fn draw_body(
    out: &mut SceneBuilder,
    part: PartId,
    view: &View<'_>,
    c: &CNode,
    pad: f64,
    small: f64,
    crossings: &Crossings,
) {
    let style = &view.style;
    let shape = style.shape.clone();
    let line = style.line.clone();
    let ink = ink_on(style.shape.as_ref(), style.text.as_ref()).map(Paint::Role);
    let (w, h) = (view.w, view.h);
    let rule = |out: &mut SceneBuilder, d: PathProgram, line: Option<Style>| {
        let element = out.element(part, Facet::Shape).path(d);
        element.fill(Paint::None).stroke(Paint::Role(Role::Border)).style(line).finish();
    };
    match view.kind {
        K::Composite => {
            let left = c.x + pad - c.w / 2.0;
            let top = c.y + pad - c.h / 2.0;
            let frame = out
                .element(part, Facet::Shape)
                .rect(pair(left, top), pair(c.w, c.h), Some(Number::raw(CORNER)))
                .fill(Paint::Role(Role::SurfaceAlt));
            let frame = if style.shape.as_ref().is_some_and(Style::sets_fill) {
                frame
            } else {
                frame.fill_opacity(Number::raw(0.5))
            };
            frame.stroke(Paint::Role(Role::Border)).style(shape).finish();
            if view.title_h > 0.0 {
                let d = PathProgram::default()
                    .m(Number::num(left), Number::num(top + view.title_h))
                    .h(Number::num(left + c.w));
                rule(out, d, line);
                let at = Point { x: title_x(c, view.label.width, crossings) + pad, y: top + view.title_h / 2.0 };
                let look = LabelLook::class("pele-cluster-label").fill(ink).style(style.text.as_ref());
                out.label(part, Facet::Title, &view.label, at, look);
            }
        }
        K::Start => {
            out.element(part, Facet::Shape)
                .circle(Number::raw(7.0))
                .fill(Paint::Role(Role::Line))
                .style(shape)
                .finish();
        }
        K::End => {
            out.element(part, Facet::Shape)
                .circle(Number::raw(8.5))
                .fill(Paint::Role(Role::Background))
                .stroke(Paint::Role(Role::Line))
                .style(line)
                .finish();
            out.element(part, Facet::Shape)
                .circle(Number::raw(5.0))
                .fill(Paint::Role(Role::Line))
                .style(shape)
                .finish();
        }
        K::Choice => {
            let zero = Number::num(0.0);
            let points = vec![
                [zero, Number::num(-h / 2.0)],
                [Number::num(w / 2.0), zero],
                [zero, Number::num(h / 2.0)],
                [Number::num(-w / 2.0), zero],
            ];
            out.element(part, Facet::Shape)
                .polygon(points)
                .fill(Paint::Role(Role::Surface))
                .stroke(Paint::Role(Role::Border))
                .style(shape)
                .finish();
        }
        K::Fork | K::Join => {
            out.element(part, Facet::Shape)
                .rect(pair(-w / 2.0, -h / 2.0), pair(w, h), Some(Number::raw(2.0)))
                .fill(Paint::Role(Role::Line))
                .style(shape)
                .finish();
        }
        K::Note => {
            let r = Number::num(w / 2.0);
            let b = Number::num(h / 2.0);
            let (minus_r, minus_b) = (Number::minus_num(w / 2.0), Number::minus_num(h / 2.0));
            let fold = Number::num(w / 2.0 - FOLD);
            let corner = Number::num(FOLD - h / 2.0);
            let sheet = PathProgram::default().m(minus_r, minus_b).h(fold).l(r, corner).v(b).h(minus_r).z();
            out.element(part, Facet::Shape)
                .path(sheet)
                .fill(Paint::Role(Role::Background))
                .stroke(Paint::Role(Role::Border))
                .finish();
            rule(out, PathProgram::default().m(fold, minus_b).v(corner).h(r), None);
            let look =
                LabelLook::class("pele-label").font_size(Number::raw(small)).fill(Some(Paint::Role(Role::Muted)));
            out.label(part, Facet::Label, &view.label, Point { x: 0.0, y: 0.0 }, look);
        }
        K::State | K::Described => {
            out.element(part, Facet::Shape)
                .rect(pair(-w / 2.0, -h / 2.0), pair(w, h), Some(Number::num(js::min(CORNER, h / 2.0))))
                .fill(Paint::Role(Role::Surface))
                .stroke(Paint::Role(Role::Border))
                .style(shape)
                .finish();
            let look = || LabelLook::class("pele-label").fill(ink).style(style.text.as_ref());
            if view.kind == K::State {
                out.label(part, Facet::Label, &view.label, Point { x: 0.0, y: 0.0 }, look());
                return;
            }
            let mut at = PAD_Y - h / 2.0 + view.label.height;
            let d = PathProgram::default().m(Number::num(-w / 2.0), Number::num(at + 6.0)).h(Number::num(w / 2.0));
            rule(out, d, line);
            out.label(part, Facet::Label, &view.label, Point { x: 0.0, y: at - view.label.height / 2.0 }, look());
            at += 14.0;
            for label in &view.lines {
                let look = LabelLook::class("pele-state-description")
                    .font_size(Number::raw(small))
                    .fill(ink)
                    .style(style.text.as_ref());
                out.label(part, Facet::Label, label, Point { x: 0.0, y: at + label.height / 2.0 }, look);
                at += label.height;
            }
        }
        K::Region | K::Hidden => unreachable!("only drawn kinds are drawn"),
    }
}

pub(crate) struct State;

impl Diagram for State {
    type Model = StateModel;

    fn parse(cx: &ParseContext<'_>) -> Result<StateModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderState`: a diagram that runs across is drawn running down instead where that
    /// fits the host's width better.
    fn scene(model: &StateModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        if matches!(model.direction(), Direction::LR | Direction::RL) {
            fit::turn_to_fit(cx.options, |down| draw(model, cx, down))
        } else {
            draw(model, cx, false)
        }
    }
}
