//! Drawing a flowchart, ported from Pele's `src/diagrams/flowchart/render.ts`: nodes are sized
//! from their labels, laid out with their groups and edges, and drawn as a Scene.

use super::graph::{ArrowHead, GraphEdge, GraphNode};
use super::shapes::{Shape, ShapeName};
use super::{Direction, FlowchartModel, LabelType, Link, Stroke, build_flow_graph, default_curve};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::graph::cluster::{Crossings, cluster_title_x, draw_struck_title, title_crossed};
use crate::graph::edges::{
    Curve, EdgePath, Fill, Marker, MarkerUse, Over, SideReach, casings_layer, draw_casing, draw_edge_label,
    draw_marker, loop_path, over_fills, route_path,
};
use crate::graph::shapes::{draw_shape, end_side, inset_route, shape_filled, shape_has_label, shape_size, shape_span};
use crate::input::url::{image_allowed, internal_link_allowed, link_allowed, safe_url};
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities, is_space};
use crate::layout::{
    CEdge, CNode, Dir, EdgeStyle, LayeredOptions, compound_layout, inherit_crossed_directions, shift_layout,
};
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, Aspect, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineLook, Number,
    Paint, PartId, Point, ResolvedStyle, Role, Scene, SceneBuilder, ShapeLook, Size, Style, TitleLayout, ink_on,
    paints, resolve_style,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 48.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;

/// A number the config gives for `key`, where it is one above zero, capped at 2000.
fn number_option(section: Option<&JsObject>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|section| section.get(key)) {
        Some(&JsValue::Number(value)) if value > 0.0 => js::min(value, 2000.0),
        _ => fallback,
    }
}

fn asset_size(value: Option<f64>, fallback: f64) -> f64 {
    value.filter(|&value| value > 0.0 && value <= 4000.0).unwrap_or(fallback)
}

fn filled(text: Option<&JsText>) -> Option<&JsText> {
    text.filter(|text| !text.is_empty())
}

impl From<ArrowHead> for Marker {
    fn from(head: ArrowHead) -> Marker {
        match head {
            ArrowHead::None => Marker::None,
            ArrowHead::Point => Marker::Point,
            ArrowHead::Circle => Marker::Circle,
            ArrowHead::Cross => Marker::Cross,
        }
    }
}

/// A node or a group as it is drawn: its style, its label and the size of its shape.
struct NodeView<'a> {
    node: &'a GraphNode,
    style: ResolvedStyle,
    label: Label,
    /// The shape of a node. A group has none.
    shape: Option<Shape>,
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
    /// Nothing for an invisible edge.
    line: Option<EdgeLine>,
}

struct EdgeLine {
    path: EdgePath,
    over: Over,
    /// The colour of the edge's markers.
    colour: Paint,
    markers: [Marker; 2],
}

/// The title of a group: where it is centred, and whether an edge runs through it.
struct ClusterTitle {
    at: Point,
    struck: bool,
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

/// Whether a node shows a picture or an icon in place of a shape.
fn has_asset(node: &GraphNode) -> bool {
    filled(node.img.as_ref()).is_some() || filled(node.icon.as_ref()).is_some()
}

fn stroke_of(edge: &GraphEdge) -> Option<Stroke> {
    match edge.link {
        Link::Valid { stroke, .. } => Some(stroke),
        Link::Invalid => None,
    }
}

/// The part of each node and edge. An edge's key is the id its author gave it, or else its two
/// ends and its kind, with its place among the edges that share them.
fn parts(out: &mut SceneBuilder, nodes: &[GraphNode], edges: &[&GraphEdge]) -> (Vec<PartId>, Vec<PartId>) {
    let node_parts: Vec<PartId> = nodes
        .iter()
        .map(|node| {
            let entity = if node.is_group { Entity::group(&node.id) } else { Entity::node(&node.id) };
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

/// Pele's `draw`. `turned` draws a flowchart that runs across as one that runs down. `tight`
/// sets nodes closer together.
fn draw(model: &FlowchartModel, cx: &DrawContext<'_>, turned: bool, tight: bool) -> Result<Scene, RenderError> {
    let group_pad = if tight { 12.0 } else { GROUP_PAD };
    let flow = cx.config.get("flowchart").and_then(JsValue::as_record);
    let config_curve = default_curve(cx.config);
    let graph = build_flow_graph(model, config_curve);
    let size = cx.options.typography.normal.size;
    let edge_size = js::round(size * 0.875);
    let wrap_width = number_option(flow, "wrappingWidth", 200.0);
    let pad = cx.options.padding;
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
        let has_asset = has_asset(node);
        let (view, mut c);
        if node.is_group {
            let label = layout_label(&node.label, markdown, cx.measure, font_size, 4000.0, text_style);
            c = CNode::new(0.0, 0.0);
            c.is_group = true;
            c.dir = node.dir.map(|dir| Dir::named(Some(dir.name())));
            c.pad_x = group_pad;
            c.pad_top = if label.height > 0.0 { label.height + 16.0 } else { GROUP_PAD };
            c.pad_bottom = group_pad;
            c.min_w = label.width + 2.0 * group_pad - 12.0;
            view = NodeView { node, style, label, shape: None, w: 0.0, h: 0.0, dy: 0.0 };
        } else {
            let shape = node.shape.shape();
            let text: &[u16] = if has_asset || shape_has_label(shape) { &node.label } else { &[] };
            let label = layout_label(text, markdown, cx.measure, font_size, wrap_width, text_style);
            let (w, h, dy);
            if has_asset {
                let side = if filled(node.img.as_ref()).is_some() { 80.0 } else { 48.0 };
                let aw = asset_size(node.asset_width, side);
                let ah = asset_size(node.asset_height, side);
                w = js::max(aw, label.width + 8.0);
                h = ah + if label.height > 0.0 { label.height + 6.0 } else { 0.0 };
                let top = node.pos.as_ref().is_some_and(|pos| pos.is_str("t"));
                dy = if label.height == 0.0 {
                    0.0
                } else if top {
                    -ah / 2.0
                } else {
                    ah / 2.0
                };
            } else {
                let s = shape_size(shape, label.width, label.height);
                (w, h, dy) = (s.w, s.h, s.dy);
            }
            c = CNode::new(w, h);
            view = NodeView { node, style, label, shape: Some(shape), w, h, dy };
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
    let flows_sideways = |cnodes: &[CNode], i: usize| {
        let mut dir = None;
        let mut parent = cnodes[i].parent;
        let mut hops = 0;
        while let Some(p) = parent.filter(|_| dir.is_none() && hops < 10000) {
            dir = cnodes[p].dir;
            parent = cnodes[p].parent;
            hops += 1;
        }
        matches!(dir.unwrap_or(root_dir), Dir::LR | Dir::RL)
    };
    // The edges on one side of a node spread along it; a picture or an icon keeps them at its middle.
    for (i, view) in views.iter().enumerate() {
        if view.node.is_group {
            cnodes[i].span = f64::INFINITY;
        } else if let Some(shape) = view.shape
            && !has_asset(view.node)
        {
            cnodes[i].span = shape_span(shape, view.w, view.h, flows_sideways(&cnodes, i));
        }
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    for edge in &graph.edges {
        let (Some(src), Some(dst)) = ends(edge) else {
            continue;
        };
        let markdown = edge.label_type == LabelType::Markdown;
        let label = layout_label(&edge.label, markdown, cx.measure, edge_size, wrap_width, Emphasis::default());
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
        node_sep: number_option(flow, "nodeSpacing", NODE_SEP) * if tight { 0.5 } else { 1.0 },
        edge_sep: EDGE_SEP,
        rank_sep: number_option(flow, "rankSpacing", RANK_SEP) * if tight { 0.75 } else { 1.0 },
        port_sep: 20.0,
        style: if config_curve.is_some_and(|curve| curve.starts_at(0, "step")) {
            EdgeStyle::Stepped
        } else if config_curve.is_some_and(|curve| curve.is_str("linear")) {
            EdgeStyle::Linear
        } else {
            EdgeStyle::Curved
        },
        apart: false,
        head_room: Marker::Point.trim(),
    };
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &layout_options);

    let title_room = if tight { fit::title_room(cx.options) } else { 4000.0 };
    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, cx.measure, size, title_room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = js::max(layout.width, title.width);
    let ox = pad + (inner - layout.width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();

    let mut crossings = Crossings::default();
    if cnodes.iter().any(|c| c.is_group) {
        for e in &cedges {
            crossings.mark(&e.route);
        }
    }

    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width, height }, accessible);
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let (node_parts, edge_parts) = parts(&mut out, &graph.nodes, &drawn);

    let mut fills: Vec<Fill<'_>> = Vec::new();
    let mut titles: Vec<Option<ClusterTitle>> = Vec::new();
    for (view, c) in views.iter().zip(&cnodes) {
        if !c.is_group {
            titles.push(None);
            continue;
        }
        let shape = view.style.shape.as_ref();
        if paints(shape) {
            fills.push(Fill { bounds: [c.x - c.w / 2.0, c.y - c.h / 2.0, c.x + c.w / 2.0, c.y + c.h / 2.0], shape });
        }
        let x = cluster_title_x(c, view.label.width, &crossings, group_pad);
        let y = c.y - c.h / 2.0 + 8.0 + view.label.height / 2.0;
        let struck = title_crossed(c, view.label.width, x, &crossings);
        titles.push(Some(ClusterTitle { at: Point { x, y }, struck }));
    }
    // A title drawn over the edges sits on a halo in the theme's colours.
    let title_look = |view: &NodeView<'_>, struck: bool| {
        let ink = if struck { None } else { ink_on(view.style.shape.as_ref(), view.style.text.as_ref()) };
        LabelLook::class("pele-cluster-label")
            .fill(Some(Paint::Role(ink.unwrap_or(Role::Muted))))
            .style(view.style.text.as_ref())
    };

    let mut reach = SideReach::default();
    for (ce, edge) in cedges.iter().zip(&drawn) {
        if ce.src == ce.dst || stroke_of(edge) == Some(Stroke::Invisible) || ce.route.len() < 2 {
            continue;
        }
        let (start, end) = edge.arrows();
        reach.add(ce.src, end_side(&ce.route[0], &cnodes[ce.src]), start.into());
        reach.add(ce.dst, end_side(&ce.route[ce.route.len() - 1], &cnodes[ce.dst]), end.into());
    }

    let mut edges: Vec<EdgeView<'_>> = Vec::new();
    let mut loop_seen: HashMap<usize, f64> = HashMap::new();
    for (i, label) in edge_labels.into_iter().enumerate() {
        let edge = drawn[i];
        let style = (!edge.style.is_empty() || !edge.css_compiled_styles.is_empty())
            .then(|| resolve_style(edge.css_compiled_styles.iter().chain(&edge.style)));
        let mut line = None;
        // A route of one point has no direction to draw along. The layout gives every edge
        // between two nodes at least its two ends, so none is expected here.
        let drawable = cedges[i].src == cedges[i].dst || cedges[i].route.len() >= 2;
        if stroke_of(edge) != Some(Stroke::Invisible) && drawable {
            let (start, end) = edge.arrows();
            let markers: [Marker; 2] = [start.into(), end.into()];
            let ce = &mut cedges[i];
            let path = if ce.src == ce.dst {
                let view = &views[ce.src];
                let k = loop_seen.entry(ce.src).or_insert(0.0);
                let centre = Point { x: cnodes[ce.src].x, y: cnodes[ce.src].y };
                let sideways = flows_sideways(&cnodes, ce.src);
                let room = [ce.label_w, ce.label_h];
                let drawn = loop_path(centre, [view.w, view.h], *k, sideways, LOOP, markers[1].trim(), room);
                *k += 1.0;
                (ce.label_x, ce.label_y) = (drawn.label.x, drawn.label.y);
                drawn.path
            } else {
                let mut route = ce.route.clone();
                let (first, last) = (0, route.len() - 1);
                let leaves = reach.of(ce.src, end_side(&route[first], &cnodes[ce.src]));
                let arrives = reach.of(ce.dst, end_side(&route[last], &cnodes[ce.dst]));
                for (at, node) in [(first, ce.src), (last, ce.dst)] {
                    if let Some(shape) = views[node].shape {
                        inset_route(&mut route[at], shape, views[node].w, views[node].h, &cnodes[node]);
                    }
                }
                let curve = Curve::named(edge.curve.as_ref().and_then(JsValue::as_text).map(|name| &**name));
                route_path(&route, curve, markers.map(Marker::trim), [leaves, arrives])
            };
            let line_style = style.as_ref().and_then(|style| style.line.as_ref());
            let over = over_fills(&path, &fills, line_style);
            let colour = match (over, style.as_ref().and_then(|style| style.stroke.as_ref())) {
                (Over::Ink(ink), _) => Paint::Role(ink),
                (_, Some(stroke)) => out.colour(stroke, Role::Line),
                (_, None) => Paint::Role(Role::Line),
            };
            line = Some(EdgeLine { path, over, colour, markers });
        }
        edges.push(EdgeView { edge, part: edge_parts[i], style, label, line });
    }
    for (i, ce) in cedges.iter().enumerate() {
        if let (Some(&from), Some(&to)) = (node_parts.get(ce.src), node_parts.get(ce.dst)) {
            out.relation(edge_parts[i], from, to);
        }
    }

    out.layer(LayerSpec::class("pele-clusters"), |out| {
        for (i, view) in views.iter().enumerate() {
            let Some(title) = &titles[i] else {
                continue;
            };
            let c = &cnodes[i];
            let part = node_parts[i];
            let classes = author_classes(view.node);
            out.layer(LayerSpec::classes("pele-cluster", &classes).part(part, Facet::Shape).data_id(), |out| {
                let shape = view.style.shape.as_ref();
                let frame = out.element(part, Facet::Shape).rect(
                    [Number::num(c.x - c.w / 2.0), Number::num(c.y - c.h / 2.0)],
                    [Number::num(c.w), Number::num(c.h)],
                    None,
                );
                let frame = frame.fill(Paint::Role(Role::SurfaceAlt));
                let frame =
                    if shape.is_some_and(Style::sets_fill) { frame } else { frame.fill_opacity(Number::raw(0.5)) };
                frame.stroke(Paint::Role(Role::Border)).style(Some(Style::rounded(shape))).finish();
                if !title.struck {
                    out.label(part, Facet::Title, &view.label, title.at, title_look(view, false));
                }
            });
        }
    });

    out.layer(casings_layer(), |out| {
        for view in &edges {
            if let Some(line) = view.line.as_ref().filter(|line| line.over == Over::Cased) {
                let line_style = view.style.as_ref().and_then(|style| style.line.as_ref());
                let thick = stroke_of(view.edge) == Some(Stroke::Thick);
                draw_casing(out, view.part, &line.path, line.markers, line_style, thick);
            }
        }
    });

    let edges_layer =
        LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
    out.layer(edges_layer, |out| {
        for view in &edges {
            let Some(line) = &view.line else {
                continue;
            };
            let mut group = LayerSpec::class("pele-edge").part(view.part, Facet::Shape).data_id();
            if let Over::Ink(ink) = line.over {
                group = group.stroke(Paint::Role(ink));
            }
            let group = group.style(view.style.as_ref().and_then(|style| style.line.clone()));
            out.layer(group, |out| {
                let path = out.element(view.part, Facet::Shape).path(line.path.d.clone());
                match stroke_of(view.edge) {
                    Some(Stroke::Thick) => path.stroke_width(Number::raw(2.5)),
                    Some(Stroke::Dotted) => path.dash(&[Number::raw(3.0), Number::raw(4.0)]),
                    _ => path,
                }
                .finish();
                let colour = MarkerUse::Edge(line.colour);
                draw_marker(out, view.part, Facet::StartMarker, line.markers[0], line.path.start, colour);
                draw_marker(out, view.part, Facet::EndMarker, line.markers[1], line.path.end, colour);
            });
        }
    });

    out.layer(LayerSpec::class("pele-cluster-titles"), |out| {
        for (i, view) in views.iter().enumerate() {
            let Some(title) = titles[i].as_ref().filter(|title| title.struck) else {
                continue;
            };
            let mut depth = 1;
            let mut parent = cnodes[i].parent;
            while let Some(p) = parent.filter(|_| depth < 8) {
                depth += 1;
                parent = cnodes[p].parent;
            }
            draw_struck_title(out, node_parts[i], &view.label, title.at, title_look(view, true), depth);
        }
    });

    out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(edge_size)), |out| {
        for (view, ce) in edges.iter().zip(&cedges) {
            if view.label.width > 0.0 {
                let text = view.style.as_ref().and_then(|style| style.text.as_ref());
                draw_edge_label(out, view.part, &view.label, Point { x: ce.label_x, y: ce.label_y }, text);
            }
        }
    });

    out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
        for (i, view) in views.iter().enumerate() {
            if let Some(shape) = view.shape {
                draw_node(out, node_parts[i], view, shape, &cnodes[i], policy);
            }
        }
    });
    out.finish()
}

/// One node: its picture, icon or shape, then its label, inside its link where it has one.
fn draw_node(
    out: &mut SceneBuilder,
    part: PartId,
    view: &NodeView<'_>,
    shape: Shape,
    c: &CNode,
    policy: &crate::ResourcePolicy,
) {
    let node = view.node;
    let classes = author_classes(node);
    let own = format!("pele-node pele-shape-{}", shape.name());
    let group = LayerSpec::classes(&own, &classes)
        .part(part, Facet::Shape)
        .data_id()
        .translate(Point { x: c.x, y: c.y })
        .keep_empty();

    let body = |out: &mut SceneBuilder| {
        if filled(node.tooltip.as_ref()).is_some() {
            out.tooltip(part);
        }
        let ink = draw_node_shape(out, part, view, shape, policy);
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

/// What a node is drawn as, behind its label. Answers the ink a label on it needs.
fn draw_node_shape(
    out: &mut SceneBuilder,
    part: PartId,
    view: &NodeView<'_>,
    shape: Shape,
    policy: &crate::ResourcePolicy,
) -> Option<Role> {
    let node = view.node;
    let h = view.h;
    let top_label = node.pos.as_ref().is_some_and(|pos| pos.is_str("t"));
    // Where the picture or icon starts: under a label at the top, above one at the bottom,
    // and centred on the node with no label.
    let top = |side: f64| {
        if view.label.height > 0.0 { if top_label { h / 2.0 - side } else { -h / 2.0 } } else { -side / 2.0 }
    };
    let a = ShapeLook::new(Role::Surface, Role::Border, view.style.shape.as_ref());
    if let Some(img) = filled(node.img.as_ref()) {
        let aw = asset_size(node.asset_width, 80.0);
        let ah = asset_size(node.asset_height, 80.0);
        let src = safe_url(img);
        if image_allowed(&src, policy) {
            let constrained = node.constraint.as_ref().is_some_and(|constraint| constraint.is_str("on"));
            let aspect = if constrained { Aspect::Meet } else { Aspect::Stretch };
            let origin = [Number::num(-aw / 2.0), Number::num(top(ah))];
            out.image(part, src, origin, [Number::num(aw), Number::num(ah)], aspect);
        }
        return None;
    }
    if let Some(icon) = filled(node.icon.as_ref()) {
        let side = asset_size(node.asset_height, 48.0);
        let top = top(side);
        if node.shape == ShapeName::ICON_CIRCLE {
            let centre = [None, Some(Number::num(top + side / 2.0))];
            out.element(part, Facet::Shape).circle_at(centre, Number::num(side / 2.0)).look(a.plain()).finish();
        } else if node.shape != ShapeName::ICON {
            let rx = if node.shape == ShapeName::ICON_ROUNDED { 8.0 } else { 0.0 };
            out.element(part, Facet::Shape)
                .rect([Number::num(-side / 2.0), Number::num(top)], [Number::num(side); 2], Some(Number::raw(rx)))
                .look(a.plain())
                .finish();
        }
        let glyph = side * 0.6;
        let origin = [Number::num(-glyph / 2.0), Number::num(top + (side - glyph) / 2.0)];
        out.icon(part, icon, origin, Number::num(glyph));
        return None;
    }
    let line = LineLook::new(Role::Border, view.style.line.as_ref());
    draw_shape(out, part, shape, view.w, h, &a, &line);
    if shape_filled(shape) { ink_on(view.style.shape.as_ref(), view.style.text.as_ref()) } else { None }
}

fn runs_across(direction: Direction) -> bool {
    matches!(direction, Direction::LR | Direction::RL)
}

pub(crate) struct Flowchart;

impl Diagram for Flowchart {
    type Model = FlowchartModel;

    fn parse(cx: &ParseContext<'_>) -> Result<FlowchartModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderFlowchart`: with room to spare, then tightly, and for a flowchart that runs
    /// across, both again running down.
    fn scene(model: &FlowchartModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        let fitted = |down: bool| fit::tighten(cx.options, |tight| draw(model, cx, down, tight));
        if runs_across(model.direction) { fit::turn_to_fit(cx.options, fitted) } else { fitted(false) }
    }
}
