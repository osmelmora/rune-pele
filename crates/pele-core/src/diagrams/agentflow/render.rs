//! Drawing an agentflow, ported from Pele's `src/diagrams/agentflow/render.ts`: nodes are sized
//! from their labels, laid out with their flows and edges, and drawn as a Scene. A node takes
//! the series colour of its kind, as an outline with a light wash of the same colour inside.

use super::graph::{GraphEdge, GraphNode, GraphShape};
use super::{AgentflowModel, Arrow, VertexKind};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::diagrams::flowchart::{Direction, LabelType, Shape};
use crate::graph::cluster::{Crossings, cluster_title_x};
use crate::graph::edges::{
    Curve, EdgePath, Fill, Marker, MarkerUse, Over, casings_layer, default_reach, draw_casing, draw_edge_label,
    draw_marker, loop_path, over_fills_at_end, route_path,
};
use crate::graph::shapes::{PAD_X, PAD_Y, draw_shape, inset_route, shape_filled, shape_size};
use crate::input::url::{internal_link_allowed, link_allowed};
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities, is_space};
use crate::layout::{
    Axis, CEdge, CNode, Dir, EdgeStyle, LayeredOptions, RoutePoint, compound_layout, inherit_crossed_directions,
    shift_layout,
};
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineLook, Number, Paint,
    PartId, PathProgram, Point, ResolvedStyle, Role, Scene, SceneBuilder, SeriesSlot, ShapeLook, Size, Style,
    TitleLayout, ink_on, paints, resolve_style,
};
use crate::{RenderError, ResourcePolicy, fit, js};
use std::collections::HashMap;

/// The number of series colours, which is the palette Mermaid's slot rules are applied to.
pub(super) const PALETTE: usize = 8;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 48.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
/// The room the row of dots at the foot of a collapsed flow takes.
const DOTS: f64 = 14.0;
/// How opaque the wash inside a flow's frame is, and inside a node.
const FLOW_WASH: f64 = 0.06;
const NODE_WASH: f64 = 0.14;

/// A number the config gives for `key`, where it is one above zero, capped at 2000.
fn number_option(section: Option<&JsObject>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|section| section.get(key)) {
        Some(&JsValue::Number(value)) if value > 0.0 => js::min(value, 2000.0),
        _ => fallback,
    }
}

fn filled(text: Option<&JsText>) -> Option<&JsText> {
    text.filter(|text| !text.is_empty())
}

impl From<Option<Arrow>> for Marker {
    fn from(arrow: Option<Arrow>) -> Marker {
        match arrow {
            Some(Arrow::Point) => Marker::Point,
            Some(Arrow::Cross) => Marker::Cross,
            Some(Arrow::Open) | None => Marker::None,
        }
    }
}

/// What a node's view is drawn as.
#[derive(Clone, Copy, PartialEq)]
enum Drawn {
    /// A flow: a frame around its members.
    Group,
    Node(Form),
}

/// The form of a node that is no frame.
#[derive(Clone, Copy, PartialEq)]
enum Form {
    /// A collapsed flow: a box with a row of dots at its foot.
    Collapsed,
    Shape(Shape),
}

impl Drawn {
    /// The shape whose outline an edge's end is moved onto. A collapsed flow fills its box, as
    /// the shape of that name does, and a flow has no outline of its own.
    fn outline(self) -> Option<Shape> {
        match self {
            Drawn::Group => None,
            Drawn::Node(Form::Collapsed) => Some(Shape::CollapsedGroup),
            Drawn::Node(Form::Shape(shape)) => Some(shape),
        }
    }
}

/// A node or a flow as it is drawn: its style, its label and the size of its shape.
struct NodeView<'a> {
    node: &'a GraphNode,
    style: ResolvedStyle,
    label: Label,
    drawn: Drawn,
    w: f64,
    h: f64,
    dy: f64,
}

/// An edge as it is drawn.
struct EdgeView<'a> {
    edge: &'a GraphEdge,
    part: PartId,
    style: Option<ResolvedStyle>,
    label: Label,
    /// Where the label is centred.
    label_at: Point,
    path: EdgePath,
    over: Over,
    /// The colour of the edge's marker.
    colour: Paint,
}

/// An edge between a node and a flow around it. The layout has no place for one, so it is drawn
/// afterwards, between the node and the flow's border.
struct Held<'a> {
    edge: &'a GraphEdge,
    label: Label,
    src: usize,
    dst: usize,
}

/// The classes a source gives a node, without the `default` every node's list starts with:
/// `classNames(node.cssClasses.replace(/^default\s?/, ''))`.
fn author_classes(node: &GraphNode) -> AuthorClasses {
    let classes = &node.css_classes;
    if classes.starts_at(0, "default") {
        AuthorClasses::new(&classes[7 + usize::from(classes.at(7).is_some_and(is_space))..])
    } else {
        AuthorClasses::new(classes)
    }
}

/// The series colour of a slot.
fn series(slot: usize) -> Paint {
    Paint::Series(SeriesSlot::new(slot))
}

/// The series colour of a flow, collapsed or not.
fn container_colour(node: &GraphNode) -> Paint {
    series(node.color_index.unwrap_or(VertexKind::COUNT))
}

/// The part of each node and edge. An edge's key is the id its author gave it, or else its two
/// ends and its kind, with its place among the edges that share them.
fn parts(out: &mut SceneBuilder, nodes: &[GraphNode], edges: &[&GraphEdge]) -> (Vec<PartId>, Vec<PartId>) {
    let node_parts: Vec<PartId> = nodes
        .iter()
        .map(|node| {
            let entity = if node.is_group() { Entity::group(&node.id) } else { Entity::node(&node.id) };
            out.part(entity.tooltip(filled(node.tooltip.as_ref())))
        })
        .collect();
    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let edge_parts = edges
        .iter()
        .map(|edge| {
            let name = if edge.is_user_defined_id {
                EntityName::Named(edge.id.to_vec())
            } else {
                let kind = format!("{} {}", edge.link.type_name(), edge.link.stroke_name());
                EntityName::Between { from: edge.start.to_vec(), to: edge.end.to_vec(), kind }
            };
            let duplicate = seen.entry(name.clone()).or_insert(0);
            let part = out.part(Entity::edge(&edge.id, name, *duplicate));
            *duplicate += 1;
            part
        })
        .collect();
    (node_parts, edge_parts)
}

/// The side a flow runs toward, with sides numbered clockwise from the top.
fn flow_end(dir: Dir) -> usize {
    match dir {
        Dir::TB => 2,
        Dir::RL => 3,
        Dir::BT => 0,
        Dir::LR => 1,
    }
}

/// Where each node sits in the nesting of flows.
struct Nesting {
    depth: Vec<usize>,
}

impl Nesting {
    fn of(cnodes: &[CNode]) -> Nesting {
        let mut depth: Vec<Option<usize>> = vec![None; cnodes.len()];
        for i in 0..cnodes.len() {
            let mut chain = Vec::new();
            let mut j = Some(i);
            while let Some(at) = j.filter(|&at| depth[at].is_none() && chain.len() <= cnodes.len()) {
                chain.push(at);
                j = cnodes[at].parent;
            }
            let top = j.and_then(|at| depth[at]).map_or(0, |above| above + 1);
            for (below, &at) in chain.iter().rev().enumerate() {
                depth[at] = Some(top + below);
            }
        }
        Nesting { depth: depth.into_iter().map(|depth| depth.unwrap_or(0)).collect() }
    }

    /// Whether the flow `group` holds `i`, at any depth.
    fn contains(&self, cnodes: &[CNode], group: usize, i: usize) -> bool {
        let mut j = Some(i);
        for _ in self.depth[group]..self.depth[i] {
            j = j.and_then(|at| cnodes[at].parent);
        }
        j == Some(group) && i != group
    }
}

/// The direction of the level a node sits in.
fn flow_of(cnodes: &[CNode], i: usize, root: Dir) -> Dir {
    let mut dir = None;
    let mut parent = cnodes[i].parent;
    let mut hops = 0;
    while let Some(p) = parent.filter(|_| dir.is_none() && hops < 10000) {
        dir = cnodes[p].dir;
        parent = cnodes[p].parent;
        hops += 1;
    }
    dir.unwrap_or(root)
}

/// The route of an edge between a node and a flow around it: straight between the node and the
/// flow's border, out along the flow or in against it. If another node is in the way it takes a
/// side instead, and failing that the far end.
fn held_route(
    cnodes: &[CNode],
    outer: usize,
    i: usize,
    out: bool,
    toward_end: usize,
    careful: bool,
) -> [RoutePoint; 2] {
    let (g, n) = (&cnodes[outer], &cnodes[i]);
    let first = if out { toward_end } else { (toward_end + 2) & 3 };
    let toward = |side: usize| {
        let upright = side & 1 == 0;
        let sign = if side == 0 || side == 3 { -1.0 } else { 1.0 };
        let at_node = if upright { (n.x, n.y + (sign * n.h) / 2.0) } else { (n.x + (sign * n.w) / 2.0, n.y) };
        let at_border = if upright { (n.x, g.y + (sign * g.h) / 2.0) } else { (g.x + (sign * g.w) / 2.0, n.y) };
        let axis = if upright { Axis::Vertical } else { Axis::Horizontal };
        let point = |(x, y): (f64, f64)| RoutePoint { x, y, axis };
        if out { [point(at_node), point(at_border)] } else { [point(at_border), point(at_node)] }
    };
    let blocked = |r: &[RoutePoint; 2]| {
        cnodes.iter().enumerate().any(|(k, c)| {
            k != i
                && !c.is_group
                && c.x + c.w / 2.0 > js::min(r[0].x, r[1].x) - 1.0
                && c.x - c.w / 2.0 < js::max(r[0].x, r[1].x) + 1.0
                && c.y + c.h / 2.0 > js::min(r[0].y, r[1].y) - 1.0
                && c.y - c.h / 2.0 < js::max(r[0].y, r[1].y) + 1.0
        })
    };
    let route = toward(first);
    if careful && blocked(&route) {
        for side in [(first + 1) & 3, (first + 3) & 3, (first + 2) & 3] {
            let other = toward(side);
            if !blocked(&other) {
                return other;
            }
        }
    }
    route
}

/// Pele's `draw`. `turned` draws a diagram that runs across as one that runs down.
fn draw(model: &AgentflowModel, cx: &DrawContext<'_>, turned: bool) -> Result<Scene, RenderError> {
    let graph = &model.graph;
    let section = cx.config.get("agentflow").and_then(JsValue::as_record);
    let size = cx.options.typography.normal.size;
    let edge_size = js::round(size * 0.875);
    let wrap_width = number_option(section, "wrappingWidth", 120.0);
    let min_width = number_option(section, "minNodeWidth", 120.0);
    let pad = cx.options.padding_or_default();
    let policy = &cx.options.resources;

    let mut index: HashMap<&[u16], usize> = HashMap::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(&node.id, i);
    }

    let mut views: Vec<NodeView<'_>> = Vec::new();
    let mut cnodes: Vec<CNode> = Vec::new();
    for node in &graph.nodes {
        let style = resolve_style(node.css_compiled_styles.iter().chain(&node.css_styles));
        let text_style = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
        let markdown = node.label_type == LabelType::Markdown;
        let font_size = style.font_size.unwrap_or(size);
        let (view, mut c);
        match node.shape {
            GraphShape::FlowGroup => {
                let label = layout_label(&node.label, markdown, cx.measure, font_size, 4000.0, text_style);
                c = CNode::new(0.0, 0.0);
                c.is_group = true;
                c.dir = node.dir.map(|dir| Dir::named(Some(dir.name())));
                c.pad_x = GROUP_PAD;
                c.pad_top = if label.height > 0.0 { label.height + 16.0 } else { GROUP_PAD };
                c.pad_bottom = GROUP_PAD;
                c.min_w = label.width + 2.0 * GROUP_PAD - 12.0;
                view = NodeView { node, style, label, drawn: Drawn::Group, w: 0.0, h: 0.0, dy: 0.0 };
            }
            shape if shape.is_collapsed() => {
                let label = layout_label(&node.label, markdown, cx.measure, font_size, wrap_width, text_style);
                let w = js::max(label.width, min_width) + 2.0 * PAD_X;
                let h = label.height + 2.0 * PAD_Y + DOTS;
                c = CNode::new(w, h);
                view = NodeView { node, style, label, drawn: Drawn::Node(Form::Collapsed), w, h, dy: -DOTS / 2.0 };
            }
            GraphShape::Node(name) => {
                let shape = name.shape();
                let label = layout_label(&node.label, markdown, cx.measure, font_size, wrap_width, text_style);
                // A diamond grows twice as fast as its label, so it keeps the label's own width.
                let width = if shape == Shape::Diam { label.width } else { js::max(label.width, min_width) };
                let s = shape_size(shape, width, label.height);
                c = CNode::new(s.w, s.h);
                view =
                    NodeView { node, style, label, drawn: Drawn::Node(Form::Shape(shape)), w: s.w, h: s.h, dy: s.dy };
            }
        }
        c.seq = views.len() as f64;
        views.push(view);
        cnodes.push(c);
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node.parent_id.as_ref().and_then(|parent| index.get(&**parent).copied());
    }
    let ends = |edge: &GraphEdge| (index.get(&*edge.start).copied(), index.get(&*edge.end).copied());
    inherit_crossed_directions(&mut cnodes, graph.edges.iter().map(ends));

    let root_dir = if turned { Dir::TB } else { Dir::named(Some(model.direction.name())) };
    let flows_sideways = |cnodes: &[CNode], i: usize| matches!(flow_of(cnodes, i, root_dir), Dir::LR | Dir::RL);
    let nesting = Nesting::of(&cnodes);

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    let mut held: Vec<Held<'_>> = Vec::new();
    for edge in &graph.edges {
        let (Some(src), Some(dst)) = ends(edge) else {
            continue;
        };
        let markdown = edge.label_type == LabelType::Markdown;
        let label = layout_label(&edge.label, markdown, cx.measure, edge_size, wrap_width, Emphasis::default());
        if nesting.contains(&cnodes, src, dst) || nesting.contains(&cnodes, dst, src) {
            held.push(Held { edge, label, src, dst });
            continue;
        }
        if src == dst {
            // A self-loop sits beside the node, across the flow, and the node's box grows to hold it.
            if flows_sideways(&cnodes, src) {
                cnodes[src].h += 2.0 * (LOOP + if label.height > 0.0 { label.height + 8.0 } else { 0.0 });
            } else {
                cnodes[src].w += 2.0 * (LOOP + if label.width > 0.0 { label.width + 12.0 } else { 0.0 });
            }
        }
        let mut cedge = CEdge::new(src, dst);
        cedge.minlen = edge.link.length().map_or(1.0, |length| length as f64);
        cedge.label_w = if label.width > 0.0 { label.width + 12.0 } else { 0.0 };
        cedge.label_h = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
        drawn.push(edge);
        edge_labels.push(label);
        cedges.push(cedge);
    }

    let layout_options = LayeredOptions {
        node_sep: number_option(section, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(section, "rankSpacing", RANK_SEP),
        port_sep: 20.0,
        style: EdgeStyle::Curved,
        apart: false,
        head_room: 0.0,
    };
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &layout_options);

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, cx.measure, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = js::max(layout.width, title.width);
    let ox = pad + (inner - layout.width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();

    let careful = ((held.len() * cnodes.len()) as f64) < 2e6;
    let held_routes: Vec<[RoutePoint; 2]> = held
        .iter()
        .map(|&Held { src, dst, .. }| {
            let out = nesting.contains(&cnodes, dst, src);
            let (outer, i) = if out { (dst, src) } else { (src, dst) };
            let toward_end = flow_end(cnodes[outer].dir.unwrap_or_else(|| flow_of(&cnodes, outer, root_dir)));
            held_route(&cnodes, outer, i, out, toward_end, careful)
        })
        .collect();

    let mut crossings = Crossings::default();
    if cnodes.iter().any(|c| c.is_group) {
        for e in &cedges {
            crossings.mark(&e.route);
        }
        for route in &held_routes {
            crossings.mark(route);
        }
    }

    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width, height }, accessible);
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let all: Vec<&GraphEdge> = drawn.iter().copied().chain(held.iter().map(|held| held.edge)).collect();
    let (node_parts, edge_parts) = parts(&mut out, &graph.nodes, &all);

    let mut fills: Vec<Fill<'_>> = Vec::new();
    for (view, c) in views.iter().zip(&cnodes) {
        let shape = view.style.shape.as_ref();
        if c.is_group && paints(shape) {
            fills.push(Fill { bounds: [c.x - c.w / 2.0, c.y - c.h / 2.0, c.x + c.w / 2.0, c.y + c.h / 2.0], shape });
        }
    }

    // The edges the layout placed, then the ones it had no place for.
    let mut placed: Vec<(&GraphEdge, Label, EdgePath, Point, [usize; 2])> = Vec::new();
    let mut loop_seen: HashMap<usize, f64> = HashMap::new();
    for (i, label) in edge_labels.into_iter().enumerate() {
        let edge = drawn[i];
        let trim = Marker::from(edge.arrow_end()).trim();
        let ce = &mut cedges[i];
        let path = if ce.src == ce.dst {
            let view = &views[ce.src];
            let k = loop_seen.entry(ce.src).or_insert(0.0);
            let centre = Point { x: cnodes[ce.src].x, y: cnodes[ce.src].y };
            let sideways = flows_sideways(&cnodes, ce.src);
            let looped = loop_path(centre, [view.w, view.h], *k, sideways, LOOP, trim, [ce.label_w, ce.label_h]);
            *k += 1.0;
            (ce.label_x, ce.label_y) = (looped.label.x, looped.label.y);
            looped.path
        } else {
            let mut route = ce.route.clone();
            let last = route.len() - 1;
            for (at, node) in [(0, ce.src), (last, ce.dst)] {
                if let Some(shape) = views[node].drawn.outline() {
                    inset_route(&mut route[at], shape, views[node].w, views[node].h, &cnodes[node]);
                }
            }
            let curve = Curve::named(edge.curve.as_ref().and_then(JsValue::as_text).map(|name| &**name));
            route_path(&route, curve, [0.0, trim], [0.0, default_reach(trim)])
        };
        placed.push((edge, label, path, Point { x: ce.label_x, y: ce.label_y }, [ce.src, ce.dst]));
    }
    for (Held { edge, label, src, dst }, route) in held.into_iter().zip(&held_routes) {
        let out_of = nesting.contains(&cnodes, dst, src);
        let i = if out_of { src } else { dst };
        let label_at = Point { x: (route[0].x + route[1].x) / 2.0, y: (route[0].y + route[1].y) / 2.0 };
        let mut route = *route;
        if let Some(shape) = views[i].drawn.outline() {
            inset_route(&mut route[if out_of { 0 } else { 1 }], shape, views[i].w, views[i].h, &cnodes[i]);
        }
        let trim = Marker::from(edge.arrow_end()).trim();
        let path = route_path(&route, Curve::Linear, [0.0, trim], [0.0, default_reach(trim)]);
        placed.push((edge, label, path, label_at, [src, dst]));
    }

    let mut edges: Vec<EdgeView<'_>> = Vec::new();
    for (i, (edge, label, path, label_at, [src, dst])) in placed.into_iter().enumerate() {
        let style = (!edge.style.is_empty() || !edge.css_compiled_styles.is_empty())
            .then(|| resolve_style(edge.css_compiled_styles.iter().chain(&edge.style)));
        let line_style = style.as_ref().and_then(|style| style.line.as_ref());
        let over = over_fills_at_end(&path, &fills, line_style);
        let colour = match (over, style.as_ref().and_then(|style| style.stroke.as_ref())) {
            (Over::Ink(ink), _) => Paint::Role(ink),
            (_, Some(stroke)) => out.colour(stroke, Role::Line),
            (_, None) => Paint::Role(Role::Line),
        };
        out.relation(edge_parts[i], node_parts[src], node_parts[dst]);
        edges.push(EdgeView { edge, part: edge_parts[i], style, label, label_at, path, over, colour });
    }

    out.layer(LayerSpec::class("pele-clusters"), |out| {
        for (i, view) in views.iter().enumerate() {
            if view.drawn != Drawn::Group {
                continue;
            }
            let c = &cnodes[i];
            let part = node_parts[i];
            let classes = author_classes(view.node);
            let group = LayerSpec::classes("pele-cluster pele-flow", &classes).part(part, Facet::Shape).data_id();
            out.layer(group, |out| {
                let look = ShapeLook::tinted(container_colour(view.node), FLOW_WASH, view.style.shape.as_ref());
                out.element(part, Facet::Shape)
                    .rect(
                        [Number::num(c.x - c.w / 2.0), Number::num(c.y - c.h / 2.0)],
                        [Number::num(c.w), Number::num(c.h)],
                        None,
                    )
                    .look(look.rounded())
                    .finish();
                let x = cluster_title_x(c, view.label.width, &crossings, GROUP_PAD);
                let at = Point { x, y: c.y - c.h / 2.0 + 8.0 + view.label.height / 2.0 };
                let ink = ink_on(view.style.shape.as_ref(), view.style.text.as_ref());
                let look = LabelLook::class("pele-cluster-label")
                    .fill(Some(Paint::Role(ink.unwrap_or(Role::Muted))))
                    .style(view.style.text.as_ref());
                out.label(part, Facet::Title, &view.label, at, look);
            });
        }
    });

    out.layer(casings_layer(), |out| {
        for view in edges.iter().filter(|view| view.over == Over::Cased) {
            let line_style = view.style.as_ref().and_then(|style| style.line.as_ref());
            let markers = [Marker::None, view.edge.arrow_end().into()];
            draw_casing(out, view.part, &view.path, markers, line_style, false);
        }
    });

    let edges_layer =
        LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
    out.layer(edges_layer, |out| {
        for view in &edges {
            let end = view.edge.arrow_end();
            let kind = if view.edge.dotted() {
                "reference"
            } else if end == Some(Arrow::Cross) {
                "failure"
            } else {
                "sequence"
            };
            let mut group =
                LayerSpec::class(&format!("pele-edge pele-edge-{kind}")).part(view.part, Facet::Shape).data_id();
            if let Over::Ink(ink) = view.over {
                group = group.stroke(Paint::Role(ink));
            }
            let group = group.style(view.style.as_ref().and_then(|style| style.line.clone()));
            out.layer(group, |out| {
                let path = out.element(view.part, Facet::Shape).path(view.path.d.clone());
                if view.edge.dotted() { path.dash(&[Number::raw(3.0), Number::raw(4.0)]) } else { path }.finish();
                let colour = MarkerUse::Edge(view.colour);
                draw_marker(out, view.part, Facet::EndMarker, end.into(), view.path.end, colour);
            });
        }
    });

    out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(edge_size)), |out| {
        for view in edges.iter().filter(|view| view.label.width > 0.0) {
            let text = view.style.as_ref().and_then(|style| style.text.as_ref());
            draw_edge_label(out, view.part, &view.label, view.label_at, text);
        }
    });

    out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
        for (i, view) in views.iter().enumerate() {
            if let Drawn::Node(form) = view.drawn {
                draw_node(out, node_parts[i], view, form, &cnodes[i], policy);
            }
        }
    });
    out.finish()
}

/// One node: its shape, then its label, inside its link where it has one.
fn draw_node(
    out: &mut SceneBuilder,
    part: PartId,
    view: &NodeView<'_>,
    form: Form,
    c: &CNode,
    policy: &ResourcePolicy,
) {
    let node = view.node;
    let classes = author_classes(node);
    let own = match form {
        Form::Shape(shape) => {
            let kind = node.kind.unwrap_or(VertexKind::Task);
            format!("pele-node pele-shape-{} pele-kind-{}", shape.name(), kind.name())
        }
        Form::Collapsed => "pele-node pele-shape-collapsed".to_owned(),
    };
    let group = LayerSpec::classes(&own, &classes)
        .part(part, Facet::Shape)
        .data_id()
        .translate(Point { x: c.x, y: c.y })
        .keep_empty();

    let body = |out: &mut SceneBuilder| {
        if filled(node.tooltip.as_ref()).is_some() {
            out.tooltip(part);
        }
        let ink = draw_node_shape(out, part, view, form);
        let look = LabelLook::class("pele-label").fill(ink.map(Paint::Role)).style(view.style.text.as_ref());
        let at = Point { x: 0.0, y: view.dy };
        if classes.has("internal-link") && !node.label.is_empty() && policy.links {
            // The label is a note name, but it still ends up in an href, so it gets the same
            // check as a URL.
            let name = decode_entities(&node.label);
            let followable = internal_link_allowed(&name, policy);
            let rel = (!policy.link_rel.is_empty()).then(|| policy.link_rel.clone());
            out.link_inside(part, name.into(), followable, rel, |out| {
                out.label(part, Facet::Label, &view.label, at, look);
            });
        } else {
            out.label(part, Facet::Label, &view.label, at, look);
        }
    };
    out.layer(group, |out| match filled(node.link.as_ref()).filter(|link| link_allowed(link, policy)) {
        Some(link) => {
            let target = node.link_target.map(|target| target.name());
            let rel = (!policy.link_rel.is_empty()).then(|| policy.link_rel.clone());
            out.link_to(part, link.clone(), target, rel, body);
        }
        None => body(out),
    });
}

/// One of the dots at the foot of a collapsed flow, centred on `x`.
fn dot(path: PathProgram, x: f64, y: f64) -> PathProgram {
    let radii = [Number::raw(1.75); 2];
    path.m(Number::num(x - 1.75), Number::num(y))
        .a_by(radii, true, false, [Number::raw(3.5), Number::raw(0.0)])
        .a_by(radii, true, false, [Number::raw(-3.5), Number::raw(0.0)])
        .z()
}

/// What a node is drawn as, behind its label. Answers the ink a label on it needs.
fn draw_node_shape(out: &mut SceneBuilder, part: PartId, view: &NodeView<'_>, form: Form) -> Option<Role> {
    let (w, h) = (view.w, view.h);
    let shape_style: Option<&Style> = view.style.shape.as_ref();
    let line_style = view.style.line.as_ref();
    let ink = ink_on(shape_style, view.style.text.as_ref());
    match form {
        Form::Collapsed => {
            let colour = container_colour(view.node);
            let line = h / 2.0 - DOTS - 2.0;
            let dots = line + (DOTS + 2.0) / 2.0;
            out.element(part, Facet::Shape)
                .rect([Number::num(-w / 2.0), Number::num(-h / 2.0)], [Number::num(w), Number::num(h)], None)
                .look(ShapeLook::tinted(colour, FLOW_WASH, shape_style).rounded())
                .finish();
            let rule = PathProgram::default()
                .m(Number::num(-w / 2.0 + 10.0), Number::num(line))
                .h(Number::num(w / 2.0 - 10.0));
            out.element(part, Facet::Shape)
                .path(rule)
                .fill(Paint::None)
                .stroke(colour)
                .dash(&[Number::raw(3.0), Number::raw(3.0)])
                .style(line_style.cloned())
                .finish();
            let mark = [-9.0, 0.0, 9.0].into_iter().fold(PathProgram::default(), |path, x| dot(path, x, dots));
            out.element(part, Facet::Shape)
                .class("pele-collapsed-mark")
                .path(mark)
                .fill(colour)
                .stroke(Paint::None)
                .finish();
            ink
        }
        Form::Shape(shape) => {
            let colour = series(view.node.kind.unwrap_or(VertexKind::Task).slot());
            let a = ShapeLook::tinted(colour, NODE_WASH, shape_style);
            draw_shape(out, part, shape, w, h, &a, &LineLook::painted(colour, line_style));
            if shape_filled(shape) { ink } else { None }
        }
    }
}

fn runs_across(direction: Direction) -> bool {
    matches!(direction, Direction::LR | Direction::RL)
}

pub(crate) struct Agentflow;

impl Diagram for Agentflow {
    type Model = AgentflowModel;

    fn parse(cx: &ParseContext<'_>) -> Result<AgentflowModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderAgentflow`: an agentflow that runs across is drawn running down where that
    /// fits the width the host has better.
    fn scene(model: &AgentflowModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        if runs_across(model.direction) {
            fit::turn_to_fit(cx.options, |down| draw(model, cx, down))
        } else {
            draw(model, cx, false)
        }
    }
}
