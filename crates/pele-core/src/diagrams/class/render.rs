//! Drawing a class diagram, ported from Pele's `src/diagrams/class/render.ts`: classes are sized
//! from their names and members, laid out with their namespaces and relations, and drawn as a
//! Scene. Every edge meets the middle of a side, spread along it where several share it, with
//! the cardinalities at its ends set beside it.

use super::graph::{GraphEdge, GraphNode, NodeShape, Pattern};
use super::{ClassModel, ClassNode, Classifier, Direction, build_class_graph, hierarchical, markers};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::common::parse_generic_types;
use crate::graph::cluster::{Crossings, cluster_title_x, draw_struck_title, title_crossed};
use crate::graph::edges::{Curve, EdgePath, Tip, default_reach, draw_edge_label, route_path};
use crate::graph::shapes::end_side;
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities, is_space, split_unit, to_lowercase};
use crate::layout::{CEdge, CNode, Dir, LayeredOptions, RoutePoint, compound_layout, wrap_components};
use crate::measure::{Emphasis, Label, layout_label, width};
use crate::scene::{
    Accessible, AnchorKind, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineJoin, Number,
    Paint, PartId, PathProgram, Point, ResolvedStyle, Role, Scene, SceneBuilder, Size, Style, TextWeight, TitleLayout,
    ink_on, resolve_style,
};
use crate::{MonospacePart, RenderError, fit, js};
use indexmap::IndexMap;
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 56.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const PAD_X: f64 = 12.0;
const TITLE_PAD: f64 = 8.0;
const SECTION_PAD: f64 = 6.0;
const EMPTY_SECTION: f64 = 10.0;
const MIN_WIDTH: f64 = 64.0;
const FOLD: f64 = 8.0;

// The sides of a node, as `end_side` numbers them.
const TOP: i32 = 0;
const RIGHT: i32 = 1;
const LEFT: i32 = 3;

/// A number the config gives for `key`, where it is one above zero, capped at 2000.
fn number_option(section: Option<&JsObject>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|section| section.get(key)) {
        Some(&JsValue::Number(value)) if value > 0.0 => js::min(value, 2000.0),
        _ => fallback,
    }
}

/// One attribute or method as it is drawn.
struct Line {
    text: JsText,
    width: f64,
    classifier: Classifier,
}

/// A node or a namespace as it is drawn.
struct View<'a> {
    node: &'a GraphNode,
    style: ResolvedStyle,
    label: Label,
    w: f64,
    h: f64,
    /// A class's stereotypes, as they are written, with their widths.
    annotations: Vec<(JsText, f64)>,
    attributes: Vec<Line>,
    methods: Vec<Line>,
    title_h: f64,
    attributes_h: f64,
    font_size: f64,
}

impl<'a> View<'a> {
    fn plain(node: &'a GraphNode, label: Label, w: f64, h: f64, font_size: f64) -> Self {
        View {
            node,
            style: ResolvedStyle::default(),
            label,
            w,
            h,
            annotations: Vec::new(),
            attributes: Vec::new(),
            methods: Vec::new(),
            title_h: 0.0,
            attributes_h: 0.0,
            font_size,
        }
    }
}

/// Where an edge meets the side of a node it is spread along.
#[derive(Clone, Copy)]
struct End {
    edge: usize,
    /// The start of the edge, or its end.
    first: bool,
    /// The coordinate along the side of the route's next point inward.
    toward: f64,
    order: usize,
    /// The width of the text at this end.
    room: f64,
    side: i32,
    /// Whether the text of this end goes on the right of, or below, its line.
    after: bool,
}

/// A cardinality: the text at one end of an edge, centred at `x`, `y` before the offset.
struct Card {
    text: JsText,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// The node it is beside.
    parent: usize,
    edge: usize,
    facet: Facet,
}

/// Whether `across` names the y coordinate, as Pele's `route[at + across]` does.
fn coord(point: &RoutePoint, across: usize) -> f64 {
    if across == 1 { point.y } else { point.x }
}

fn coord_mut(point: &mut RoutePoint, across: usize) -> &mut f64 {
    if across == 1 { &mut point.y } else { &mut point.x }
}

/// The class names a source gives a class, without the `default` every list starts with:
/// `classNames(cssClasses.replace(/^default\s?/, ''))`.
fn author_classes(css: &[u16]) -> AuthorClasses {
    if css.starts_at(0, "default") {
        AuthorClasses::new(&css[7 + usize::from(css.at(7).is_some_and(is_space))..])
    } else {
        AuthorClasses::new(css)
    }
}

/// Pele's `classStyle`: a class may carry the declarations of many classDefs many times over.
/// Each distinct declaration is checked once, and only the last value of each property that
/// has an effect is resolved.
#[derive(Default)]
struct ClassStyles {
    /// The property each declaration sets, or nothing for one with no effect.
    effect: HashMap<JsText, Option<JsText>>,
    /// What the classDefs of each list of class names keep.
    from_defs: HashMap<JsText, Vec<(JsText, JsText)>>,
}

impl ClassStyles {
    fn keep(&mut self, kept: &mut IndexMap<JsText, JsText>, declaration: &[u16]) {
        let property = self.effect.entry(declaration.into()).or_insert_with(|| {
            let one = resolve_style([&JsText::from(declaration)]);
            if one.shape.is_none() && one.text.is_none() {
                return None;
            }
            let colon = declaration.find_unit(58, 0).expect("a declaration with an effect has a colon");
            Some(to_lowercase(declaration[..colon].trim_js()).into())
        });
        if let Some(property) = property {
            kept.insert(property.clone(), declaration.into());
        }
    }

    fn of(&mut self, model: &ClassModel, class: &ClassNode) -> ResolvedStyle {
        let mut kept: IndexMap<JsText, JsText> = IndexMap::new();
        match self.from_defs.get(&class.css_classes) {
            Some(base) => kept.extend(base.iter().cloned()),
            None => {
                for name in split_unit(&class.css_classes, 32) {
                    if let Some(def) = model.style_classes.get(name) {
                        for style in &def.styles {
                            for part in split_unit(style, 44) {
                                self.keep(&mut kept, part);
                            }
                        }
                    }
                }
                let base = kept.iter().map(|(property, declaration)| (property.clone(), declaration.clone())).collect();
                self.from_defs.insert(class.css_classes.clone(), base);
            }
        }
        for style in &class.styles {
            self.keep(&mut kept, style);
        }
        resolve_style(kept.values())
    }
}

/// The part of each node and edge. An edge has no name of its own, so its key is its two ends
/// and its kind, with its place among the edges that share them. A node that shares its name
/// with an earlier one of its kind, such as a lollipop interface named as a class, is ranked.
fn parts(
    out: &mut SceneBuilder,
    model: &ClassModel,
    nodes: &[GraphNode],
    edges: &[&GraphEdge],
) -> (Vec<PartId>, Vec<PartId>) {
    let mut ranks: HashMap<(bool, &[u16]), u32> = HashMap::new();
    let node_parts: Vec<PartId> = nodes
        .iter()
        .map(|node| {
            if node.is_group {
                return out.part(Entity::group(&node.id));
            }
            let note = node.shape == NodeShape::Note;
            let rank = ranks.entry((note, &node.id)).or_insert(0);
            let entity = if note { Entity::note(&node.id) } else { Entity::node(&node.id) };
            let tooltip =
                node.class.and_then(|index| model.classes[index].tooltip.as_ref()).filter(|tip| !tip.is_empty());
            let part = out.part(entity.ranked(*rank).tooltip(tooltip));
            *rank += 1;
            part
        })
        .collect();
    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let edge_parts = edges
        .iter()
        .map(|edge| {
            let kind = if edge.pattern == Pattern::Dotted {
                "note".to_owned()
            } else {
                let (start, end) = (edge.arrow_type_start.marker_name(), edge.arrow_type_end.marker_name());
                format!("{start} {end} {}", edge.pattern.name())
            };
            let name = EntityName::Between { from: edge.start.to_vec(), to: edge.end.to_vec(), kind };
            let duplicate = seen.entry(name.clone()).or_insert(0);
            let part = out.part(Entity::edge(&edge.id, name, *duplicate));
            *duplicate += 1;
            part
        })
        .collect();
    (node_parts, edge_parts)
}

/// Pele's `draw`. `turned` draws a diagram that runs across as one that runs down. `tight` gives
/// up the room that keeps a namespace's name clear of an edge coming in beside it.
fn draw(model: &ClassModel, cx: &DrawContext<'_>, turned: bool, tight: bool) -> Result<Scene, RenderError> {
    let conf = super::section(cx.config);
    let graph = build_class_graph(model, hierarchical(cx.config));
    let hide_empty = conf.and_then(|conf| conf.get("hideEmptyMembersBox")) == Some(&JsValue::Bool(true));
    let monospaced = cx.options.monospace.contains(&MonospacePart::Members);
    // Explicit class fonts override monospace.
    let mono = |style: &ResolvedStyle| monospaced && !style.text.as_ref().is_some_and(|text| text.sets("font-family"));
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let tiny = js::round(size * 0.8);
    let tiny_height = js::round(tiny * 1.4);
    let measure = cx.measure;
    let pad = cx.options.padding;
    let plain = Emphasis::default();
    let card_width =
        |raw: &[u16]| if raw.is_empty() { 0.0 } else { width(measure, &decode_entities(raw), tiny, plain) };
    let root_dir = if turned { Dir::TB } else { Dir::named(Some(model.direction.name())) };
    let down = matches!(root_dir, Dir::TB | Dir::BT);

    let n = graph.nodes.len();
    let mut children = vec![0usize; n];
    for node in &graph.nodes {
        if let Some(parent) = node.parent {
            children[parent] += 1;
        }
    }

    // Namespaces that an edge may cross into are made wide enough for the title to sit beside it.
    let mut crossed = vec![false; n];
    for edge in &graph.edges {
        let (Some(source), Some(target)) = (edge.source, edge.target) else {
            continue;
        };
        let (a, b) = (graph.nodes[source].parent, graph.nodes[target].parent);
        if a == b {
            continue;
        }
        for start in [a, b] {
            let mut group = start;
            while let Some(g) = group.filter(|&g| !crossed[g]) {
                crossed[g] = true;
                group = graph.nodes[g].parent;
            }
        }
    }

    let mut styles = ClassStyles::default();
    let member_line = |member: &super::ClassMember, size: f64, mono: bool| {
        let text: JsText = decode_entities(&member.display_text()).trim_js().into();
        let style = Emphasis { italic: member.classifier == Classifier::Abstract, mono, ..Emphasis::default() };
        Line { width: width(measure, &text, size, style), text, classifier: member.classifier }
    };
    let mut views: Vec<View<'_>> = Vec::with_capacity(n);
    let mut cnodes: Vec<CNode> = Vec::with_capacity(n);
    for (i, node) in graph.nodes.iter().enumerate() {
        let (view, mut c);
        if node.is_group {
            let label = layout_label(&node.label, false, measure, size, 4000.0, plain);
            let mut group = View::plain(node, label, 0.0, 0.0, size);
            if children[i] > 0 {
                let label = &group.label;
                c = CNode::new(0.0, 0.0);
                c.is_group = true;
                c.pad_x = GROUP_PAD;
                c.pad_top = if label.height > 0.0 { label.height + 16.0 } else { GROUP_PAD };
                c.pad_bottom = GROUP_PAD;
                let beside = if crossed[i] && !tight { js::min(label.width + 24.0, 240.0) } else { 0.0 };
                c.min_w = label.width + 2.0 * GROUP_PAD - 12.0 + beside;
            } else {
                group.w = js::max(group.label.width + 2.0 * GROUP_PAD, 80.0);
                group.h = group.label.height + 2.0 * GROUP_PAD;
                c = CNode::new(group.w, group.h);
            }
            view = group;
        } else if let Some(index) = node.class {
            let class = &model.classes[index];
            let style = styles.of(model, class);
            let font_size = style.font_size.unwrap_or(size);
            let member_size = js::round(font_size * 0.875);
            let line_height = js::round(member_size * 1.5);
            // The generic type is escaped so that `<T>` is not read as a tag.
            let mut text = class.label.clone();
            if !class.generic.is_empty() {
                let generic = parse_generic_types(&String::from_utf16_lossy(&class.generic));
                text.push_str("&lt;");
                text.push_str(&generic.replace('<', "&lt;").replace('>', "&gt;"));
                text.push_str("&gt;");
            }
            let emphasis = Emphasis { bold: true, italic: style.italic, ..Emphasis::default() };
            let label = layout_label(&text, false, measure, font_size, 4000.0, emphasis);
            let annotations: Vec<(JsText, f64)> = class
                .annotations
                .iter()
                .map(|annotation| {
                    let mut text = JsText::from("\u{ab}");
                    text.push_units(decode_entities(annotation).trim_js());
                    text.push_str("\u{bb}");
                    let w = width(measure, &text, tiny, plain);
                    (text, w)
                })
                .collect();
            let mono = mono(&style);
            let attributes: Vec<Line> = class.members.iter().map(|m| member_line(m, member_size, mono)).collect();
            let methods: Vec<Line> = class.methods.iter().map(|m| member_line(m, member_size, mono)).collect();
            let mut inner = label.width;
            let mut annotations_h = 0.0;
            for (_, w) in &annotations {
                inner = js::max(inner, *w);
                annotations_h += tiny_height;
            }
            for line in attributes.iter().chain(&methods) {
                inner = js::max(inner, line.width);
            }
            let sections = !(hide_empty && attributes.is_empty() && methods.is_empty());
            let section = |count: usize| {
                if !sections {
                    0.0
                } else if count > 0 {
                    2.0 * SECTION_PAD + count as f64 * line_height
                } else {
                    EMPTY_SECTION
                }
            };
            let title_h = 2.0 * TITLE_PAD + annotations_h + js::max(label.height, js::round(font_size * 1.5));
            let attributes_h = section(attributes.len());
            let w = js::max(inner + 2.0 * PAD_X, MIN_WIDTH);
            let h = title_h + attributes_h + section(methods.len());
            c = CNode::new(w, h);
            view =
                View { node, style, label, w, h, annotations, attributes, methods, title_h, attributes_h, font_size };
        } else if node.shape == NodeShape::Note {
            let label = layout_label(&node.label, true, measure, small, 260.0, plain);
            let (w, h) = (js::max(label.width + 2.0 * PAD_X, 40.0), label.height + 2.0 * TITLE_PAD);
            view = View::plain(node, label, w, h, small);
            c = CNode::new(w, h);
        } else {
            let label = layout_label(&node.label, false, measure, small, 200.0, plain);
            let (w, h) = (js::max(label.width, 12.0) + 4.0, js::max(label.height, 12.0));
            view = View::plain(node, label, w, h, small);
            c = CNode::new(w, h);
        }
        c.parent = node.parent;
        c.seq = i as f64;
        views.push(view);
        cnodes.push(c);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut start_room: Vec<f64> = Vec::new();
    let mut end_room: Vec<f64> = Vec::new();
    // Edges leave one side of a class and arrive at the other, so a class with many of them is
    // widened until they, and the text at their ends, fit side by side. Per node: how many
    // edges start there and the widest text at their starts, then the same for the ends.
    let mut crowd = vec![0.0f64; n * 4];
    let mut meet = |item: usize, slot: usize, room: f64| {
        crowd[item * 4 + slot] += 1.0;
        if room > crowd[item * 4 + slot + 1] {
            crowd[item * 4 + slot + 1] = room;
        }
    };
    for edge in &graph.edges {
        let (Some(src), Some(dst)) = (edge.source, edge.target) else {
            continue;
        };
        let label = layout_label(edge.label.as_deref().unwrap_or_default(), true, measure, small, 200.0, plain);
        let start_w = card_width(&edge.start_label_right);
        let end_w = card_width(&edge.end_label_left);
        if src == dst {
            cnodes[src].w += 2.0 * (LOOP + if label.width > 0.0 { label.width + 12.0 } else { 0.0 });
        } else {
            meet(src, 0, start_w);
            meet(dst, 2, end_w);
        }
        start_room.push(start_w);
        end_room.push(end_w);
        drawn.push(edge);
        let mut cedge = CEdge::new(src, dst);
        cedge.label_w = if label.width > 0.0 { label.width + 12.0 } else { 0.0 };
        cedge.label_h = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
        edge_labels.push(label);
        cedges.push(cedge);
    }

    if down {
        for i in 0..views.len() {
            if views[i].node.class.is_none() {
                continue;
            }
            let mut need = 0.0;
            for slot in [0, 2] {
                let count = crowd[i * 4 + slot];
                let room = crowd[i * 4 + slot + 1];
                if count > 1.0 {
                    need = js::max(need, (count - 1.0) * (if room > 0.0 { room + 24.0 } else { 14.0 }) + 24.0);
                }
            }
            need = js::min(need, 480.0);
            if need > views[i].w {
                cnodes[i].w += need - views[i].w;
                views[i].w = need;
            }
        }
    }

    let lettered = start_room.iter().zip(&end_room).any(|(&start, &end)| start > 0.0 || end > 0.0);
    // Cardinalities at both ends of a vertical edge need room for their height between the ranks.
    let mut tall_cards = 0.0;
    if down {
        let card_height = |raw: &[u16]| if raw.is_empty() { 0.0 } else { tiny_height };
        for edge in &drawn {
            let both = card_height(&edge.start_label_right) + card_height(&edge.end_label_left);
            tall_cards = js::max(tall_cards, both - 2.0 * tiny_height);
        }
    }
    let layout_options = LayeredOptions {
        node_sep: number_option(conf, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(conf, "rankSpacing", if lettered { RANK_SEP + 16.0 } else { RANK_SEP }) + tall_cards,
        port_sep: 20.0,
        style: Default::default(),
        apart: true,
        head_room: 0.0,
    };
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &layout_options);

    // Every edge reaches the middle of a side. Where several share a side they are spread along
    // it, in the order of where they come from. An end with text runs straight for the length of
    // the text, which sits beside it on the side the line then bends toward, away from the
    // neighbors that bend the other way. Neighbors are set apart far enough for the text between
    // them.
    let mut sides: IndexMap<usize, Vec<End>> = IndexMap::new();
    let mut attach = |cedges: &mut [CEdge], edge: usize, first: bool, item: usize, room: f64| -> Option<End> {
        let route = &mut cedges[edge].route;
        let at = if first { 0 } else { route.len() - 1 };
        let c = &cnodes[item];
        let side = end_side(&route[at], c);
        if side < 0 {
            return None;
        }
        let slack = (c.w - views[item].w) / 2.0;
        if side == RIGHT {
            route[at].x -= slack;
        } else if side == LEFT {
            route[at].x += slack;
        }
        let across = (side & 1) as usize;
        let toward = coord(&route[if first { 1 } else { at - 1 }], across);
        let bend = toward - coord(&route[at], across);
        let after = if across == 1 { bend > 0.0 } else { bend >= 0.0 };
        let end = End { edge, first, toward, order: edge, room, side, after };
        sides.entry(item * 4 + side as usize).or_default().push(end);
        Some(end)
    };
    let mut starts: Vec<Option<End>> = Vec::with_capacity(cedges.len());
    let mut ends: Vec<Option<End>> = Vec::with_capacity(cedges.len());
    for i in 0..cedges.len() {
        let (src, dst) = (cedges[i].src, cedges[i].dst);
        let routed = src != dst && cedges[i].route.len() >= 2;
        starts.push(if routed { attach(&mut cedges, i, true, src, start_room[i]) } else { None });
        ends.push(if routed { attach(&mut cedges, i, false, dst, end_room[i]) } else { None });
    }
    // Whether a point lies on the border of one of the namespaces around a node.
    let on_border = |cnodes: &[CNode], item: usize, x: f64, y: f64| {
        let mut group = cnodes[item].parent;
        let mut hops = 0;
        while let Some(g) = group.filter(|_| hops < 10000) {
            let c = &cnodes[g];
            let dx = (x - c.x).abs();
            let dy = (y - c.y).abs();
            if ((dy - c.h / 2.0).abs() < 0.5 && dx <= c.w / 2.0 + 0.5)
                || ((dx - c.w / 2.0).abs() < 0.5 && dy <= c.h / 2.0 + 0.5)
            {
                return true;
            }
            group = c.parent;
            hops += 1;
        }
        false
    };
    let mut gaps: Vec<f64> = Vec::new();
    for (&key, list) in &mut sides {
        if list.len() < 2 {
            continue;
        }
        let item = key >> 2;
        let across = key & 1;
        let length = if across == 1 { views[item].h } else { views[item].w };
        list.sort_by(|a, b| {
            let difference = a.toward - b.toward;
            if difference != 0.0 && !difference.is_nan() {
                difference.partial_cmp(&0.0).expect("a number")
            } else {
                a.order.cmp(&b.order)
            }
        });
        let mut total = 0.0;
        for j in 0..list.len() - 1 {
            let (a, b) = (&list[j], &list[j + 1]);
            let mut gap = if across == 1 { 20.0 } else { 24.0 };
            if a.room > 0.0 && a.after {
                gap += if across == 1 { tiny_height + 6.0 } else { a.room };
            }
            if b.room > 0.0 && !b.after {
                gap += if across == 1 { tiny_height + 6.0 } else { b.room };
            }
            if j < gaps.len() {
                gaps[j] = gap;
            } else {
                gaps.push(gap);
            }
            total += gap;
        }
        let scale = js::min(1.0, (length - 24.0) / total);
        if scale <= 0.0 {
            continue;
        }
        let mut at = (if across == 1 { cnodes[item].y } else { cnodes[item].x }) - (total * scale) / 2.0;
        for (j, end) in list.iter().enumerate() {
            let route = &mut cedges[end.edge].route;
            let last = route.len() as isize - 1;
            let tip = if end.first { 0 } else { last };
            let step: isize = if end.first { 1 } else { -1 };
            let first = tip + step;
            // An edge that crosses the border of the node's namespace close by runs straight to
            // it: the point where it crosses moves into line with its end.
            let mut border = None;
            let mut k = first;
            while k >= 0
                && k <= last
                && (coord(&route[k as usize], across) - coord(&route[first as usize], across)).abs() < 0.5
            {
                if on_border(&cnodes, item, route[k as usize].x, route[k as usize].y) {
                    border = Some(k);
                    break;
                }
                k += step;
            }
            if let Some(border) = border
                && (coord(&route[first as usize], across) - at).abs() <= 24.0
            {
                let mut k = first;
                while k != border + step {
                    *coord_mut(&mut route[k as usize], across) = at;
                    k += step;
                }
            }
            *coord_mut(&mut route[tip as usize], across) = at;
            at += gaps.get(j).copied().unwrap_or(f64::NAN) * scale;
        }
    }

    let straighten = |route: &mut Vec<RoutePoint>, end: Option<End>, room: f64, first: bool| {
        let Some(end) = end else {
            return;
        };
        if room <= 0.0 {
            return;
        }
        let across = (end.side & 1) as usize;
        let along = 1 - across;
        let at = if first { 0 } else { route.len() - 1 };
        let reach = (coord(&route[if first { 1 } else { at - 1 }], along) - coord(&route[at], along)).abs();
        let length = js::min(if across == 1 { room + 10.0 } else { tiny_height + 6.0 }, reach * 0.35);
        if length < 4.0 {
            return;
        }
        let mut point = route[at];
        *coord_mut(&mut point, along) += (if end.side == TOP || end.side == LEFT { -1.0 } else { 1.0 }) * length;
        route.insert(if first { 1 } else { at }, point);
    };
    for (i, cedge) in cedges.iter_mut().enumerate() {
        straighten(&mut cedge.route, starts[i], start_room[i], true);
        straighten(&mut cedge.route, ends[i], end_room[i], false);
    }

    let (mut min_x, mut min_y) = (0.0, 0.0);
    let (mut max_x, mut max_y) = (layout.width, layout.height);
    let mut cards: Vec<Card> = Vec::new();
    let mut card =
        |edge: usize, facet: Facet, parent: usize, raw: &[u16], w: f64, at: Point, side: i32, after: bool| {
            if raw.is_empty() {
                return;
            }
            let h = tiny_height;
            let (mut x, mut y) = (at.x, at.y);
            if side & 1 == 1 {
                x += (if side == LEFT { -1.0 } else { 1.0 }) * (5.0 + w / 2.0);
                y += (if after { 1.0 } else { -1.0 }) * (7.0 + h / 2.0);
            } else {
                x += (if after { 1.0 } else { -1.0 }) * (9.0 + w / 2.0);
                y += (if side == TOP { -1.0 } else { 1.0 }) * (3.0 + h / 2.0);
            }
            cards.push(Card { text: decode_entities(raw).into(), x, y, w, h, parent, edge, facet });
            min_x = js::min(min_x, x - w / 2.0);
            max_x = js::max(max_x, x + w / 2.0);
            min_y = js::min(min_y, y - h / 2.0);
            max_y = js::max(max_y, y + h / 2.0);
        };
    let mut loop_seen: HashMap<usize, f64> = HashMap::new();
    let mut loop_at = vec![0.0; cedges.len()];
    for (i, ce) in cedges.iter_mut().enumerate() {
        let edge = drawn[i];
        if ce.src == ce.dst {
            let seen = loop_seen.entry(ce.src).or_insert(0.0);
            let k = *seen;
            *seen += 1.0;
            loop_at[i] = k;
            let (c, view) = (&cnodes[ce.src], &views[ce.src]);
            let x = c.x + view.w / 2.0;
            let spread = js::min(view.h / 2.0 - 4.0, 8.0 + k * 6.0);
            let start = Point { x, y: c.y - spread };
            card(i, Facet::StartLabel, ce.src, &edge.start_label_right, start_room[i], start, RIGHT, false);
            card(
                i,
                Facet::EndLabel,
                ce.dst,
                &edge.end_label_left,
                end_room[i],
                Point { x, y: c.y + spread },
                RIGHT,
                true,
            );
            ce.label_x = c.x + view.w / 2.0 + LOOP + k * 8.0 + 4.0 + ce.label_w / 2.0;
            ce.label_y = c.y;
        } else if ce.route.len() >= 2 {
            let (first, last) = (ce.route[0], ce.route[ce.route.len() - 1]);
            let (from, to) = (starts[i], ends[i]);
            let start = Point { x: first.x, y: first.y };
            let (side, after) = from.map_or((2, true), |end| (end.side, end.after));
            card(i, Facet::StartLabel, ce.src, &edge.start_label_right, start_room[i], start, side, after);
            let (side, after) = to.map_or((TOP, true), |end| (end.side, end.after));
            card(
                i,
                Facet::EndLabel,
                ce.dst,
                &edge.end_label_left,
                end_room[i],
                Point { x: last.x, y: last.y },
                side,
                after,
            );
        }
    }

    if let Some(available) = cx.options.max_width.filter(|&available| available > 0.0)
        && max_x - min_x + 2.0 * pad > available
    {
        let mut all = cnodes.clone();
        all.extend(cards.iter().map(|card| {
            let mut c = CNode::new(card.w, card.h);
            c.parent = Some(card.parent);
            c.x = card.x;
            c.y = card.y;
            c
        }));
        let gap = number_option(conf, "nodeSpacing", NODE_SEP);
        let wrapped = wrap_components(&mut all, &mut cedges, js::max(1.0, available - 2.0 * pad), gap);
        cnodes.clone_from_slice(&all[..n]);
        if let Some(wrapped) = wrapped {
            for (card, c) in cards.iter_mut().zip(&all[n..]) {
                (card.x, card.y) = (c.x, c.y);
            }
            (min_x, min_y) = (0.0, 0.0);
            (max_x, max_y) = (wrapped.width, wrapped.height);
        }
    }

    let ox = pad - min_x;
    let oy = pad - min_y;
    let width = (max_x - min_x + 2.0 * pad).ceil();
    let height = (max_y - min_y + 2.0 * pad).ceil();

    let mut crossings = Crossings::default();
    if graph.nodes.first().is_some_and(|node| node.is_group) {
        for e in &cedges {
            crossings.mark(&e.route);
        }
    }

    // Where each namespace's title goes, and whether an edge runs through it.
    let titles: Vec<Option<(Point, bool)>> = views
        .iter()
        .zip(&cnodes)
        .map(|(view, c)| {
            if !view.node.is_group {
                return None;
            }
            let title_x = if c.is_group { cluster_title_x(c, view.label.width, &crossings, GROUP_PAD) } else { c.x };
            let h = if c.is_group { c.h } else { view.h };
            let title_y = c.y + oy - h / 2.0 + 8.0 + view.label.height / 2.0;
            let struck = c.is_group && title_crossed(c, view.label.width, title_x, &crossings);
            Some((Point { x: title_x + ox, y: title_y }, struck))
        })
        .collect();

    let paths: Vec<EdgePath> = cedges
        .iter_mut()
        .enumerate()
        .map(|(i, ce)| {
            let edge = drawn[i];
            let start_trim = markers::trim(edge.arrow_type_start);
            let end_trim = markers::trim(edge.arrow_type_end);
            if ce.src == ce.dst {
                let (c, view) = (&cnodes[ce.src], &views[ce.src]);
                let k = loop_at[i];
                let x = c.x + ox + view.w / 2.0;
                let y = c.y + oy;
                let spread = js::min(view.h / 2.0 - 4.0, 8.0 + k * 6.0);
                let reach = LOOP + k * 8.0;
                let d = PathProgram::default().m(Number::num(x + start_trim), Number::num(y - spread)).c(
                    [Number::num(x + reach + start_trim), Number::num(y - spread)],
                    [Number::num(x + reach + end_trim), Number::num(y + spread)],
                    [Number::num(x + end_trim), Number::num(y + spread)],
                );
                EdgePath {
                    d,
                    start: Tip { x, y: y - spread, dx: -1.0, dy: 0.0 },
                    end: Tip { x, y: y + spread, dx: -1.0, dy: 0.0 },
                }
            } else {
                for point in &mut ce.route {
                    point.x += ox;
                    point.y += oy;
                }
                let trim = [start_trim, end_trim];
                route_path(&ce.route, Curve::Smooth, trim, trim.map(default_reach))
            }
        })
        .collect();

    let title = layout_label(model.title.as_deref().unwrap_or_default(), false, measure, size, 4000.0, Emphasis::BOLD);
    let (full, shift) = if title.height > 0.0 {
        (js::max(width, (title.width + 2.0 * pad).ceil()), title.height + 12.0)
    } else {
        (width, 0.0)
    };
    let total = Size { width: full, height: if title.height > 0.0 { (height + shift).ceil() } else { height } };
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, total, accessible);
    let (node_parts, edge_parts) = parts(&mut out, model, &graph.nodes, &drawn);
    for (i, ce) in cedges.iter().enumerate() {
        out.relation(edge_parts[i], node_parts[ce.src], node_parts[ce.dst]);
    }
    let body = |out: &mut SceneBuilder| {
        draw_clusters(out, &views, &cnodes, &titles, &node_parts, ox, oy);
        draw_edges(out, &drawn, &paths, &edge_parts);
        draw_struck_titles(out, &views, &titles, &node_parts);
        out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(small)), |out| {
            for (i, ce) in cedges.iter().enumerate() {
                if edge_labels[i].width > 0.0 {
                    let at = Point { x: ce.label_x + ox, y: ce.label_y + oy };
                    draw_edge_label(out, edge_parts[i], &edge_labels[i], at, None);
                }
            }
        });
        let cardinalities =
            LayerSpec::class("pele-cardinalities").font_size(Number::raw(tiny)).fill(Paint::Role(Role::Muted));
        out.layer(cardinalities, |out| {
            for card in &cards {
                let at = [Number::num(card.x + ox), Number::num(card.y + oy + tiny * 0.35)];
                out.text(edge_parts[card.edge], card.facet)
                    .class("pele-cardinality")
                    .at(at)
                    .anchor(AnchorKind::Middle)
                    .finish(&card.text, card.w, tiny);
            }
        });
        let look = NodeLook { size, small, tiny, tiny_height, monospaced, policy: &cx.options.resources };
        out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
            for (i, view) in views.iter().enumerate() {
                if view.node.is_group {
                    continue;
                }
                let at = Point { x: cnodes[i].x + ox, y: cnodes[i].y + oy };
                draw_node(out, model, node_parts[i], view, at, &look);
            }
        });
    };
    if title.height > 0.0 {
        out.title(&TitleLayout { at: Point { x: full / 2.0, y: pad + title.height / 2.0 }, label: title });
        out.layer(LayerSpec::group().translate(Point { x: (full - width) / 2.0, y: shift }), body);
    } else {
        body(&mut out);
    }
    out.finish()
}

fn draw_clusters(
    out: &mut SceneBuilder,
    views: &[View<'_>],
    cnodes: &[CNode],
    titles: &[Option<(Point, bool)>],
    parts: &[PartId],
    ox: f64,
    oy: f64,
) {
    out.layer(LayerSpec::class("pele-clusters"), |out| {
        for (i, view) in views.iter().enumerate() {
            let Some((at, struck)) = titles[i] else {
                continue;
            };
            let c = &cnodes[i];
            let (w, h) = if c.is_group { (c.w, c.h) } else { (view.w, view.h) };
            let (x, y) = (c.x + ox, c.y + oy);
            let part = parts[i];
            out.layer(LayerSpec::class("pele-cluster pele-namespace").part(part, Facet::Shape).data_id(), |out| {
                out.element(part, Facet::Shape)
                    .rect([Number::num(x - w / 2.0), Number::num(y - h / 2.0)], [Number::num(w), Number::num(h)], None)
                    .fill(Paint::Role(Role::SurfaceAlt))
                    .fill_opacity(Number::raw(0.5))
                    .stroke(Paint::Role(Role::Border))
                    .style(Some(Style::rounded(None)))
                    .finish();
                if !struck {
                    out.label(part, Facet::Title, &view.label, at, cluster_look());
                }
            });
        }
    });
}

fn cluster_look() -> LabelLook {
    LabelLook::class("pele-cluster-label").fill(Some(Paint::Role(Role::Muted)))
}

fn draw_struck_titles(out: &mut SceneBuilder, views: &[View<'_>], titles: &[Option<(Point, bool)>], parts: &[PartId]) {
    out.layer(LayerSpec::class("pele-cluster-titles"), |out| {
        for (i, view) in views.iter().enumerate() {
            let Some((at, true)) = titles[i] else {
                continue;
            };
            let mut depth = 1;
            let mut parent = view.node.parent;
            while let Some(p) = parent.filter(|_| depth < 8) {
                depth += 1;
                parent = views[p].node.parent;
            }
            draw_struck_title(out, parts[i], &view.label, at, cluster_look(), depth);
        }
    });
}

fn draw_edges(out: &mut SceneBuilder, drawn: &[&GraphEdge], paths: &[EdgePath], parts: &[PartId]) {
    let layer = LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
    out.layer(layer, |out| {
        for (i, edge) in drawn.iter().enumerate() {
            let part = parts[i];
            let path = &paths[i];
            let class =
                if edge.pattern == Pattern::Dotted { "pele-edge pele-note-edge" } else { "pele-edge pele-relation" };
            out.layer(LayerSpec::class(class).part(part, Facet::Shape).data_id(), |out| {
                let line = out.element(part, Facet::Shape).path(path.d.clone());
                match edge.pattern {
                    Pattern::Dashed => line.dash(&[Number::raw(6.0), Number::raw(4.0)]),
                    Pattern::Dotted => line.dash(&[Number::raw(2.0), Number::raw(4.0)]),
                    Pattern::Solid => line,
                }
                .finish();
                let colour = Paint::Role(Role::Line);
                markers::draw(out, part, Facet::StartMarker, edge.arrow_type_start, path.start, colour);
                markers::draw(out, part, Facet::EndMarker, edge.arrow_type_end, path.end, colour);
            });
        }
    });
}

/// What every node is drawn with.
struct NodeLook<'a> {
    size: f64,
    small: f64,
    tiny: f64,
    tiny_height: f64,
    monospaced: bool,
    policy: &'a crate::ResourcePolicy,
}

fn draw_node(
    out: &mut SceneBuilder,
    model: &ClassModel,
    part: PartId,
    view: &View<'_>,
    at: Point,
    look: &NodeLook<'_>,
) {
    let node = view.node;
    match node.shape {
        NodeShape::ClassBox => {
            let class = &model.classes[node.class.expect("a class box draws a class")];
            draw_class(out, class, part, view, at, look);
        }
        NodeShape::Note => {
            let group = LayerSpec::class("pele-node pele-note").part(part, Facet::Shape).data_id().translate(at);
            out.layer(group, |out| {
                let r = view.w / 2.0;
                let t = -view.h / 2.0;
                let page = PathProgram::default()
                    .m(Number::num(-r), Number::num(t))
                    .h(Number::num(r - FOLD))
                    .l(Number::num(r), Number::num(t + FOLD))
                    .v(Number::num(-t))
                    .h(Number::num(-r))
                    .z();
                out.element(part, Facet::Shape)
                    .path(page)
                    .fill(Paint::Role(Role::SurfaceAlt))
                    .stroke(Paint::Role(Role::Border))
                    .join(LineJoin::Round)
                    .finish();
                let fold = PathProgram::default()
                    .m(Number::num(r - FOLD), Number::num(t))
                    .v(Number::num(t + FOLD))
                    .h(Number::num(r));
                out.element(part, Facet::Shape)
                    .path(fold)
                    .fill(Paint::None)
                    .stroke(Paint::Role(Role::Border))
                    .join(LineJoin::Round)
                    .finish();
                let label = LabelLook::class("pele-label").font_size(Number::raw(look.small));
                out.label(part, Facet::Label, &view.label, Point { x: 0.0, y: 0.0 }, label);
            });
        }
        NodeShape::Interface | NodeShape::Rect => {
            let group = LayerSpec::class("pele-node pele-interface").part(part, Facet::Shape).data_id().translate(at);
            out.layer(group, |out| {
                let label = LabelLook::class("pele-label").font_size(Number::raw(look.small));
                out.label(part, Facet::Label, &view.label, Point { x: 0.0, y: 0.0 }, label);
            });
        }
    }
}

/// Pele's `classSvg`: the box, its dividers, its stereotypes, its name and its members, inside
/// its link where it has one.
fn draw_class(
    out: &mut SceneBuilder,
    class: &ClassNode,
    part: PartId,
    view: &View<'_>,
    at: Point,
    look: &NodeLook<'_>,
) {
    let classes = author_classes(&class.css_classes);
    let group = LayerSpec::classes("pele-node pele-class", &classes).part(part, Facet::Shape).data_id().translate(at);
    let style = &view.style;
    let (w, h) = (view.w, view.h);
    let left = -w / 2.0;
    let top = -h / 2.0;
    let member_size = js::round(view.font_size * 0.875);
    let line_height = js::round(member_size * 1.5);
    let ink = ink_on(style.shape.as_ref(), style.text.as_ref()).map(Paint::Role);
    let first = top + view.title_h;
    let second = first + view.attributes_h;
    let mono = look.monospaced && !style.text.as_ref().is_some_and(|text| text.sets("font-family"));
    let body = |out: &mut SceneBuilder| {
        if class.tooltip.as_ref().is_some_and(|tip| !tip.is_empty()) {
            out.tooltip(part);
        }
        out.element(part, Facet::Shape)
            .rect([Number::num(left), Number::num(top)], [Number::num(w), Number::num(h)], None)
            .fill(Paint::Role(Role::Surface))
            .stroke(Paint::Role(Role::Border))
            .style(Some(Style::rounded(style.shape.as_ref())))
            .finish();
        if h > view.title_h {
            let stroke = style.stroke.as_ref().map(|stroke| {
                let mut declaration = Style::unterminated();
                declaration.declare("stroke", stroke.clone());
                declaration
            });
            let dividers = PathProgram::default()
                .m(Number::num(left), Number::num(first))
                .h(Number::num(-left))
                .m(Number::num(left), Number::num(second))
                .h(Number::num(-left));
            out.element(part, Facet::Shape)
                .class("pele-divider")
                .path(dividers)
                .fill(Paint::None)
                .stroke(Paint::Role(Role::Border))
                .style(stroke)
                .finish();
        }
        let mut y = top + TITLE_PAD;
        for (text, width) in &view.annotations {
            let baseline = [Number::num(0.0), Number::num(y + look.tiny_height / 2.0 + look.tiny * 0.35)];
            out.text(part, Facet::Annotation)
                .class("pele-annotation")
                .at(baseline)
                .anchor(AnchorKind::Middle)
                .font_size(Number::raw(look.tiny))
                .fill(Some(ink.unwrap_or(Paint::Role(Role::Muted))))
                .finish(text, *width, look.tiny);
            y += look.tiny_height;
        }
        let mut name = LabelLook::class("pele-label");
        if view.font_size != look.size {
            name = name.font_size(Number::num(view.font_size));
        }
        let name = name.weight(TextWeight::Heading).fill(ink).style(style.text.as_ref());
        out.label(part, Facet::Label, &view.label, Point { x: 0.0, y: (y + first - TITLE_PAD) / 2.0 }, name);
        if view.attributes.is_empty() && view.methods.is_empty() {
            return;
        }
        let mut members = LayerSpec::class("pele-members").font_size(Number::raw(member_size));
        if let Some(ink) = ink {
            members = members.fill(ink);
        }
        if mono {
            members = members.mono();
        }
        out.layer(members, |out| {
            for (lines, kind, facet, from) in [
                (&view.attributes, "attribute", Facet::Attribute, first),
                (&view.methods, "method", Facet::Method, second),
            ] {
                let mut baseline = from + SECTION_PAD + line_height / 2.0 + member_size * 0.35;
                for line in lines {
                    let at = [Number::num(left + PAD_X), Number::num(baseline)];
                    let text = out.text(part, facet);
                    let text = match line.classifier {
                        Classifier::Static => text
                            .class(&format!("pele-member pele-{kind} pele-static"))
                            .style(Some(Style::underlined(style.text.as_ref())))
                            .underline(),
                        Classifier::Abstract => text
                            .class(&format!("pele-member pele-{kind} pele-abstract"))
                            .italic()
                            .style(style.text.clone()),
                        Classifier::None => text.class(&format!("pele-member pele-{kind}")).style(style.text.clone()),
                    };
                    text.at(at).finish(&line.text, line.width, member_size);
                    baseline += line_height;
                }
            }
        });
    };
    out.layer(group, |out| {
        let link = class.link.as_ref().filter(|link| crate::input::url::link_allowed(link, look.policy));
        match link {
            Some(link) => {
                let target = class.link_target.map(|target| target.name());
                let rel = (!look.policy.link_rel.is_empty()).then(|| look.policy.link_rel.clone());
                out.link_to(part, link.clone(), target, rel, body);
            }
            None => body(out),
        }
    });
}

fn runs_across(direction: Direction) -> bool {
    matches!(direction, Direction::LR | Direction::RL)
}

pub(crate) struct Class;

impl Diagram for Class {
    type Model = ClassModel;

    fn parse(cx: &ParseContext<'_>) -> Result<ClassModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderClass`: with room to spare, then tightly, and for a diagram that runs
    /// across, both again running down.
    fn scene(model: &ClassModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        let fitted = |down: bool| fit::tighten(cx.options, |tight| draw(model, cx, down, tight));
        if runs_across(model.direction) { fit::turn_to_fit(cx.options, fitted) } else { fitted(false) }
    }
}
