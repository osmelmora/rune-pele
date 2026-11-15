//! Drawing an ER diagram, ported from Pele's `src/diagrams/er/render.ts`: entities are sized
//! from their names and attribute tables, laid out with their subgraphs and relationships, and
//! drawn as a Scene. Edges that meet one side of an entity are spread along it, and an entity's
//! relationships to itself loop out of the side the layout keeps free.

use super::graph::{ErGraphEdge, ErGraphNode};
use super::markers::{self, MARKER_LENGTH, MarkerUse};
use super::{Direction, ErModel, Identification, build_er_graph};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::common::parse_generic_types;
use crate::graph::cluster::{Crossings, cluster_title_x};
use crate::graph::edges::{
    Curve, Fill, Tip, Under, casings_layer, default_reach, draw_edge_label, fill_under_path, route_path,
};
use crate::input::url::internal_link_allowed;
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities, is_space};
use crate::layout::num::hypot;
use crate::layout::{CEdge, CNode, Dir, LayeredOptions, RoutePoint, compound_layout, shift_layout};
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, Number, Paint, PartId,
    PathProgram, PathStep, Point, ResolvedStyle, Role, Scene, SceneBuilder, ShapeLook, Size, Style, TitleLayout,
    ink_on, paints, resolve_style,
};
use crate::{MonospacePart, RenderError, fit, js};
use indexmap::IndexMap;
use std::collections::HashMap;

const NODE_SEP: f64 = 48.0;
const EDGE_SEP: f64 = 20.0;
const RANK_SEP: f64 = 64.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 28.0;
const LOOP_BAND: f64 = 38.0;
const MIN_WIDTH: f64 = 96.0;
const BOX_PAD_X: f64 = 18.0;
const BOX_PAD_Y: f64 = 12.0;
const HEAD_PAD_Y: f64 = 9.0;
const CELL_PAD_X: f64 = 12.0;
const CELL_GAP: f64 = 16.0;
const ROW_PAD: f64 = 9.0;
const COMMENT_WRAP: f64 = 260.0;
const NO_WRAP: f64 = 1e6;
const PORT_GAP: f64 = 18.0;
const PORT_MARGIN: f64 = 14.0;

/// A number the config's er section gives for `key`, where it is one above zero, capped at 2000.
fn number_option(config: &JsObject, key: &str, fallback: f64) -> f64 {
    match config.get("er").and_then(JsValue::as_record).and_then(|section| section.get(key)) {
        Some(&JsValue::Number(value)) if value > 0.0 => js::min(value, 2000.0),
        _ => fallback,
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

/// One row of an entity's table: its type, name, keys and comment, and its height.
struct Row {
    cells: [Label; 4],
    h: f64,
}

/// An entity or a subgraph as it is drawn.
struct NodeView<'a> {
    node: &'a ErGraphNode<'a>,
    style: ResolvedStyle,
    label: Label,
    w: f64,
    h: f64,
    head_h: f64,
    rows: Vec<Row>,
    /// The left edge of each column, from the left edge of the box.
    cols: Vec<f64>,
    row_size: f64,
    key_size: f64,
    mono: bool,
    /// Whether relationships to itself loop out of the bottom side rather than the right.
    below: bool,
}

/// An edge as it is drawn: its path, the tips of its markers, and how it stays legible.
struct EdgeLine {
    d: PathProgram,
    start: Tip,
    end: Tip,
    /// The ink of the fill it lies on, which it is stroked in.
    ink: Option<Role>,
    cased: bool,
    /// What fills the rings and diamonds of its markers.
    inside: Paint,
}

/// The class names a source gives a node, without the `default` an entity's list starts with:
/// `classNames(cssClasses.replace(/^default\s?/, ''))`.
fn author_classes(css: &[u16]) -> AuthorClasses {
    if css.starts_at(0, "default") {
        AuthorClasses::new(&css[7 + usize::from(css.at(7).is_some_and(is_space))..])
    } else {
        AuthorClasses::new(css)
    }
}

/// Mermaid reads `~` pairs as angle brackets. They are written as entities so the label parser
/// does not take `<int>` for a tag.
fn cell_text(raw: &[u16]) -> JsText {
    let text = String::from_utf16_lossy(raw);
    let converted = parse_generic_types(&text);
    if converted == text { raw.into() } else { converted.replace('<', "&lt;").replace('>', "&gt;").as_str().into() }
}

/// Pele's `straighten`: a route without the points along a straight run.
fn straighten(route: &[RoutePoint]) -> Vec<RoutePoint> {
    let mut out: Vec<RoutePoint> = route.iter().take(1).copied().collect();
    for k in 1..route.len() {
        let prev = out[out.len() - 1];
        let (here, next) = (route[k], route.get(k + 1));
        let flat = |along: fn(&RoutePoint) -> f64| {
            next.is_some_and(|next| {
                (along(&prev) - along(&here)).abs() < 0.01 && (along(&here) - along(next)).abs() < 0.01
            })
        };
        if flat(|p| p.x) || flat(|p| p.y) {
            continue;
        }
        out.push(here);
    }
    out
}

/// An entity with a relationship to itself is laid out larger than it is drawn. This moves a
/// route end from the side of that larger box onto the entity's own side.
fn inset(point: &mut RoutePoint, view: &NodeView<'_>, c: &CNode) {
    if c.is_group {
        return;
    }
    let dx = point.x - c.x;
    let dy = point.y - c.y;
    if c.w != view.w && (dx.abs() - c.w / 2.0).abs() < 0.5 {
        point.x -= (sign(dx) * (c.w - view.w)) / 2.0;
    }
    if c.h != view.h && (dy.abs() - c.h / 2.0).abs() < 0.5 {
        point.y -= (sign(dy) * (c.h - view.h)) / 2.0;
    }
}

/// Pele's `spreadEnds`. The layout brings every edge that ends on one side of a box to the middle
/// of that side, where their markers would cover each other. This spaces them along the side,
/// in the order they fan out.
fn spread_ends(cedges: &mut [CEdge], cnodes: &[CNode], views: &[NodeView<'_>]) {
    struct End {
        edge: usize,
        at: usize,
        toward: f64,
    }
    let mut sides: IndexMap<usize, Vec<End>> = IndexMap::new();
    for (edge, e) in cedges.iter().enumerate() {
        if e.src == e.dst || e.route.len() < 2 {
            continue;
        }
        let last = e.route.len() - 1;
        for (node, at, next) in [(e.src, 0, 1), (e.dst, last, last - 1)] {
            let c = &cnodes[node];
            let dx = e.route[at].x - c.x;
            let dy = e.route[at].y - c.y;
            let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
                if dy < 0.0 { 0 } else { 2 }
            } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
                if dx < 0.0 { 3 } else { 1 }
            } else {
                continue;
            };
            let toward = if side & 1 == 1 { e.route[next].y } else { e.route[next].x };
            sides.entry(node * 4 + side).or_default().push(End { edge, at, toward });
        }
    }
    for (key, mut ends) in sides {
        if ends.len() < 2 {
            continue;
        }
        let node = key >> 2;
        let vertical = key & 1 == 1;
        let c = &cnodes[node];
        let (w, h) = if c.is_group { (c.w, c.h) } else { (views[node].w, views[node].h) };
        let length = if vertical { h } else { w };
        let count = ends.len() as f64;
        let step = js::max(js::min(PORT_GAP, (length - 2.0 * PORT_MARGIN) / (count - 1.0)), 0.0);
        let center = if vertical { c.y } else { c.x };
        ends.sort_by(|a, b| (a.toward - b.toward).partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal));
        for (i, end) in ends.iter().enumerate() {
            let at = center + (i as f64 - (count - 1.0) / 2.0) * step;
            let point = &mut cedges[end.edge].route[end.at];
            if vertical {
                point.y = at;
            } else {
                point.x = at;
            }
        }
    }
}

/// The part of each node and edge. An edge has no name of its own, so its key is its two ends
/// and its kind, with its place among the edges that share them. Subgraphs may share an id, so a
/// group is ranked among those with its id.
fn parts(
    out: &mut SceneBuilder,
    nodes: &[ErGraphNode<'_>],
    drawn: &[(&ErGraphEdge, CEdge)],
) -> (Vec<PartId>, Vec<PartId>) {
    let name = |node: &ErGraphNode<'_>| if node.is_group { node.id.clone() } else { node.label.clone() };
    let mut ranks: HashMap<JsText, u32> = HashMap::new();
    let node_parts: Vec<PartId> = nodes
        .iter()
        .map(|node| {
            if !node.is_group {
                return out.part(Entity::node(&node.label));
            }
            let rank = ranks.entry(node.id.clone()).or_insert(0);
            let part = out.part(Entity::group(&node.id).ranked(*rank));
            *rank += 1;
            part
        })
        .collect();
    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let edge_parts = drawn
        .iter()
        .map(|(edge, ce)| {
            let pattern = if edge.pattern == Identification::Identifying { "identifying" } else { "non-identifying" };
            let kind =
                format!("{} {pattern} {}", edge.arrow_type_start.marker_name(), edge.arrow_type_end.marker_name());
            let from = name(&nodes[ce.src]).to_vec();
            let to = name(&nodes[ce.dst]).to_vec();
            let key = EntityName::Between { from, to, kind };
            let duplicate = seen.entry(key.clone()).or_insert(0);
            let part = out.part(Entity::edge(&edge.id, key, *duplicate));
            *duplicate += 1;
            part
        })
        .collect();
    (node_parts, edge_parts)
}

/// Pele's `draw`. `turned` draws a diagram that runs across as one that runs down.
fn draw(model: &ErModel, cx: &DrawContext<'_>, turned: bool) -> Result<Scene, RenderError> {
    let graph = build_er_graph(model);
    let monospaced = cx.options.monospace.contains(&MonospacePart::Attributes);
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let measure = cx.measure;
    let pad = cx.options.padding;
    let plain = Emphasis::default();

    let mut index: HashMap<&[u16], usize> = HashMap::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(&node.id, i);
    }

    let mut drawn: Vec<(&ErGraphEdge, CEdge)> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    // The relationships of each entity to itself, by the entity, in the order first met.
    let mut loops: IndexMap<usize, Vec<usize>> = IndexMap::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(&*edge.start), index.get(&*edge.end)) else {
            continue;
        };
        let label = layout_label(&edge.label, true, measure, small, 200.0, plain);
        if src == dst {
            loops.entry(src).or_default().push(drawn.len());
        }
        let mut cedge = CEdge::new(src, dst);
        cedge.label_w = if label.width > 0.0 { label.width + 12.0 } else { 0.0 };
        cedge.label_h = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
        drawn.push((edge, cedge));
        edge_labels.push(label);
    }
    let mut bands = vec![0.0f64; drawn.len()];

    // The direction of the level a node is laid out in: its nearest subgraph that sets one, or
    // the diagram's. Subgraphs can name each other as members, so the walk up is bounded.
    let horizontal = |i: usize| {
        let mut at = i;
        for _ in 0..graph.nodes.len() {
            let Some(&up) = graph.nodes[at].parent_id.as_ref().and_then(|parent| index.get(&**parent)) else {
                break;
            };
            if let Some(dir) = graph.nodes[up].dir {
                return matches!(dir, Direction::LR | Direction::RL);
            }
            at = up;
        }
        matches!(model.direction, Direction::LR | Direction::RL)
    };

    let mut views: Vec<NodeView<'_>> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        let style = resolve_style(node.css_compiled_styles.iter().chain(node.css_styles));
        let base = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
        let mono = monospaced && !style.text.as_ref().is_some_and(|text| text.sets("font-family"));
        let name_size = style.font_size.unwrap_or(size);
        let row_size = style.font_size.unwrap_or(small);
        let key_size = style.font_size.unwrap_or(js::round(size * 0.75));
        let label = if node.is_group {
            layout_label(&node.label, true, measure, name_size, 4000.0, base)
        } else {
            let shown = if node.alias.is_empty() { &node.label } else { &node.alias };
            layout_label(shown, true, measure, name_size, 240.0, base)
        };
        let own: &[usize] = loops.get(&i).map_or(&[], Vec::as_slice);
        // Loops leave by a side the layout keeps free of other edges: the right when ranks run
        // down, the bottom when they run across.
        let below = !own.is_empty() && !node.is_group && horizontal(i);
        let mut view = NodeView {
            node,
            style,
            label,
            w: 0.0,
            h: 0.0,
            head_h: 0.0,
            rows: Vec::new(),
            cols: Vec::new(),
            row_size,
            key_size,
            mono,
            below,
        };
        let mut reach = 0.0;
        let mut band_sum = 0.0;
        for &edge in own {
            let label = &edge_labels[edge];
            bands[edge] = js::max(LOOP_BAND, if below { label.width + 12.0 } else { label.height + 6.0 });
            band_sum += bands[edge];
            reach = js::max(reach, if below { label.height } else { label.width });
        }
        let min_w = js::max(MIN_WIDTH, if below { band_sum + 8.0 } else { 0.0 });
        let mut c;
        if node.is_group {
            c = CNode::new(0.0, 0.0);
            c.is_group = true;
            c.dir = node.dir.map(|dir| Dir::named(Some(dir.name())));
            c.pad_x = GROUP_PAD;
            c.pad_top = if view.label.height > 0.0 { view.label.height + 16.0 } else { GROUP_PAD };
            c.pad_bottom = GROUP_PAD;
            c.min_w = js::max(view.label.width + 2.0 * GROUP_PAD - 12.0, MIN_WIDTH);
        } else {
            let name_h = js::max(view.label.height, js::round(name_size * 1.5));
            if node.attributes.is_empty() {
                view.w = js::max(view.label.width + 2.0 * BOX_PAD_X, min_w);
                view.h = js::max(name_h + 2.0 * BOX_PAD_Y, if below { 0.0 } else { band_sum + 8.0 });
            } else {
                let row_line = js::round(row_size * 1.5);
                let mut widths = [0.0f64; 4];
                let styled = Emphasis { mono, ..base };
                for attribute in node.attributes {
                    let mut keys = JsText::default();
                    for (k, key) in attribute.keys.iter().enumerate() {
                        if k > 0 {
                            keys.push_str(", ");
                        }
                        keys.push_units(key);
                    }
                    let cells = [
                        layout_label(&cell_text(&attribute.kind), true, measure, row_size, NO_WRAP, styled),
                        layout_label(&cell_text(&attribute.name), true, measure, row_size, NO_WRAP, styled),
                        layout_label(&keys, false, measure, key_size, NO_WRAP, styled),
                        layout_label(&cell_text(&attribute.comment), true, measure, row_size, COMMENT_WRAP, base),
                    ];
                    let mut h = row_line;
                    for (k, cell) in cells.iter().enumerate() {
                        if cell.width > widths[k] {
                            widths[k] = cell.width;
                        }
                        if cell.height > h {
                            h = cell.height;
                        }
                    }
                    view.rows.push(Row { cells, h: h + ROW_PAD });
                    view.h += h + ROW_PAD;
                }
                let mut columns = 0.0;
                let mut inner = 0.0;
                for &w in &widths {
                    if w == 0.0 {
                        continue;
                    }
                    inner += w + if columns > 0.0 { CELL_GAP } else { 0.0 };
                    columns += 1.0;
                }
                view.w = js::max(js::max(inner + 2.0 * CELL_PAD_X, view.label.width + 2.0 * BOX_PAD_X), min_w);
                // A box wider than its table shares the spare width between the columns.
                let spare = if columns > 0.0 { (view.w - inner - 2.0 * CELL_PAD_X) / columns } else { 0.0 };
                let mut x = CELL_PAD_X;
                for &w in &widths {
                    view.cols.push(x);
                    if w > 0.0 {
                        x += w + spare + CELL_GAP;
                    }
                }
                view.head_h = name_h + 2.0 * HEAD_PAD_Y;
                view.h += view.head_h;
            }
            // The layout keeps a node centred in its box, so room for loops is added on both sides.
            let room = if own.is_empty() { 0.0 } else { 2.0 * (MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach) };
            c = if below { CNode::new(view.w, view.h + room) } else { CNode::new(view.w + room, view.h) };
        }
        c.seq = i as f64;
        views.push(view);
        cnodes.push(c);
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node.parent_id.as_ref().and_then(|parent| index.get(&**parent).copied());
    }

    let root_dir = if turned { Dir::TB } else { Dir::named(Some(model.direction.name())) };
    let options = LayeredOptions {
        node_sep: number_option(cx.config, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(cx.config, "rankSpacing", RANK_SEP),
        port_sep: 24.0,
        style: Default::default(),
        apart: false,
        head_room: 0.0,
    };
    let mut cedges: Vec<CEdge> = drawn.iter().map(|(_, ce)| ce.clone()).collect();
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &options);
    for (i, view) in views.iter_mut().enumerate() {
        if cnodes[i].is_group {
            view.w = cnodes[i].w;
            view.h = cnodes[i].h;
        }
    }

    spread_ends(&mut cedges, &cnodes, &views);

    // A subgraph gets its size from the layout, so its loops hang outside it and may widen the
    // diagram.
    let mut layout_width = layout.width;
    for (&i, own) in &loops {
        let c = &cnodes[i];
        if !c.is_group {
            continue;
        }
        let mut reach = 0.0;
        for &edge in own {
            reach = js::max(reach, edge_labels[edge].width);
        }
        layout_width = js::max(layout_width, c.x + c.w / 2.0 + MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach);
    }

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measure, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = js::max(layout_width, title.width);
    let ox = pad + (inner - layout_width) / 2.0;
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

    // Where each subgraph's title goes, and the fills edges may run over.
    let mut fills: Vec<Fill<'_>> = Vec::new();
    let mut titles: Vec<Option<Point>> = Vec::with_capacity(views.len());
    for (view, c) in views.iter().zip(&cnodes) {
        if !c.is_group {
            titles.push(None);
            continue;
        }
        let (x, y) = (c.x - c.w / 2.0, c.y - c.h / 2.0);
        let title_x = cluster_title_x(c, view.label.width, &crossings, GROUP_PAD);
        let shape = view.style.shape.as_ref();
        if paints(shape) {
            fills.push(Fill { bounds: [x, y, x + c.w, y + c.h], shape });
        }
        titles.push(Some(Point { x: title_x, y: y + 8.0 + view.label.height / 2.0 }));
    }

    // Relationships to oneself share the side they loop out of, squeezed together if it is too
    // short for them.
    let mut loop_at = vec![0.0f64; drawn.len()];
    for (&i, own) in &loops {
        let view = &views[i];
        let mut total = 0.0;
        for &edge in own {
            total += bands[edge];
        }
        let scale = js::min(1.0, js::max((if view.below { view.w } else { view.h }) - 8.0, 0.0) / total);
        let mut at = (-total * scale) / 2.0;
        for &edge in own {
            bands[edge] *= scale;
            loop_at[edge] = at + bands[edge] / 2.0;
            at += bands[edge];
        }
    }

    let mut lines: Vec<Option<EdgeLine>> = Vec::with_capacity(drawn.len());
    let mut hollow_colours: Vec<(usize, JsText)> = Vec::new();
    for (i, ce) in cedges.iter_mut().enumerate() {
        let label = &edge_labels[i];
        let (d, start, end);
        if ce.src == ce.dst {
            let c = &cnodes[ce.src];
            let view = &views[ce.src];
            let spread = js::max(js::min(bands[i] / 2.0 - 6.0, 13.0), 1.0);
            let far = MARKER_LENGTH + LOOP * 0.75 + 6.0;
            let n = Number::num;
            if view.below {
                let x = c.x + loop_at[i];
                let y = c.y + view.h / 2.0;
                let turn = n(y + MARKER_LENGTH);
                let out = n(y + MARKER_LENGTH + LOOP);
                d = PathProgram::default()
                    .m(n(x - spread), n(y))
                    .v(turn)
                    .c([n(x - spread), out], [n(x + spread), out], [n(x + spread), turn])
                    .v(n(y));
                start = Tip { x: x - spread, y, dx: 0.0, dy: -1.0 };
                end = Tip { x: x + spread, y, dx: 0.0, dy: -1.0 };
                ce.label_x = x;
                ce.label_y = y + far + label.height / 2.0;
            } else {
                let x = c.x + view.w / 2.0;
                let y = c.y + loop_at[i];
                let turn = n(x + MARKER_LENGTH);
                let out = n(x + MARKER_LENGTH + LOOP);
                d = PathProgram::default()
                    .m(n(x), n(y - spread))
                    .h(turn)
                    .c([out, n(y - spread)], [out, n(y + spread)], [turn, n(y + spread)])
                    .h(n(x));
                start = Tip { x, y: y - spread, dx: -1.0, dy: 0.0 };
                end = Tip { x, y: y + spread, dx: -1.0, dy: 0.0 };
                ce.label_x = x + far + label.width / 2.0;
                ce.label_y = y;
            }
        } else {
            let mut route = straighten(&ce.route);
            if route.len() < 2 {
                lines.push(None);
                continue;
            }
            let last = route.len() - 1;
            inset(&mut route[0], &views[ce.src], &cnodes[ce.src]);
            inset(&mut route[last], &views[ce.dst], &cnodes[ce.dst]);
            // A curve stops short of each box so that the marker sits on a straight run of line.
            let dx = (route[last].x - route[0].x).abs();
            let dy = (route[last].y - route[0].y).abs();
            let direct = route.len() == 2 && (dx < 0.01 || dy < 0.01 || hypot(dx, dy) <= 3.0 * MARKER_LENGTH);
            let stub = if direct { 0.0 } else { MARKER_LENGTH };
            let path = route_path(&route, Curve::Smooth, [stub, stub], [default_reach(stub); 2]);
            d = if stub > 0.0 {
                let mut steps = vec![PathStep::Move([Number::num(path.start.x), Number::num(path.start.y)])];
                for (k, step) in path.d.steps.into_iter().enumerate() {
                    steps.push(match step {
                        PathStep::Move(to) if k == 0 => PathStep::Line(to),
                        other => other,
                    });
                }
                steps.push(PathStep::Line([Number::num(path.end.x), Number::num(path.end.y)]));
                PathProgram { steps }
            } else {
                path.d
            };
            (start, end) = (path.start, path.end);
        }
        let (ink, cased, inside) = match fill_under_path(&d, &fills) {
            Under::Page => (None, false, Paint::Role(Role::Background)),
            Under::Border => (None, true, Paint::Role(Role::Background)),
            Under::Fill(fill) => match ink_on(fill.shape, None) {
                Some(ink) => {
                    // Markers with the page's colour inside them take the fill's colour there.
                    if let Some(colour) = fill.shape.and_then(|shape| shape.first("fill")) {
                        hollow_colours.push((i, colour.as_str().into()));
                    }
                    (Some(ink), false, Paint::Role(Role::Background))
                }
                None => (None, true, Paint::Role(Role::Background)),
            },
        };
        lines.push(Some(EdgeLine { d, start, end, ink, cased, inside }));
    }

    let size_of = Size { width, height };
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, size_of, accessible);
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let (node_parts, edge_parts) = parts(&mut out, &graph.nodes, &drawn);
    for (i, ce) in cedges.iter().enumerate() {
        out.relation(edge_parts[i], node_parts[ce.src], node_parts[ce.dst]);
    }
    for (i, colour) in hollow_colours {
        let paint = out.colour(&colour, Role::Background);
        if let Some(line) = &mut lines[i] {
            line.inside = paint;
        }
    }

    out.layer(LayerSpec::class("pele-clusters"), |out| {
        for (i, view) in views.iter().enumerate() {
            if let Some(at) = titles[i] {
                draw_cluster(out, node_parts[i], view, &cnodes[i], at);
            }
        }
    });

    out.layer(casings_layer(), |out| {
        for (i, (edge, _)) in drawn.iter().enumerate() {
            let Some(line) = lines[i].as_ref().filter(|line| line.cased) else {
                continue;
            };
            let part = edge_parts[i];
            out.element(part, Facet::Casing).path(line.d.clone()).finish();
            markers::draw(out, part, Facet::Casing, edge.arrow_type_start, line.start, MarkerUse::Casing);
            markers::draw(out, part, Facet::Casing, edge.arrow_type_end, line.end, MarkerUse::Casing);
        }
    });

    let edges_layer =
        LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
    out.layer(edges_layer, |out| {
        for (i, (edge, _)) in drawn.iter().enumerate() {
            let Some(line) = &lines[i] else {
                continue;
            };
            let part = edge_parts[i];
            let dashed = edge.pattern == Identification::NonIdentifying;
            let class = if dashed { "pele-edge pele-er-non-identifying" } else { "pele-edge pele-er-identifying" };
            let mut group = LayerSpec::class(class).part(part, Facet::Shape).data_id();
            if let Some(ink) = line.ink {
                group = group.stroke(Paint::Role(ink));
            }
            out.layer(group, |out| {
                let path = out.element(part, Facet::Shape).path(line.d.clone());
                if dashed { path.dash(&[Number::raw(4.0), Number::raw(3.0)]) } else { path }.finish();
                let used = MarkerUse::Edge { inside: line.inside };
                markers::draw(out, part, Facet::StartMarker, edge.arrow_type_start, line.start, used);
                markers::draw(out, part, Facet::EndMarker, edge.arrow_type_end, line.end, used);
            });
        }
    });

    out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(small)), |out| {
        for (i, ce) in cedges.iter().enumerate() {
            if lines[i].is_some() && edge_labels[i].width > 0.0 {
                let at = Point { x: ce.label_x, y: ce.label_y };
                draw_edge_label(out, edge_parts[i], &edge_labels[i], at, None);
            }
        }
    });

    out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
        for (i, view) in views.iter().enumerate() {
            if !cnodes[i].is_group {
                let at = Point { x: cnodes[i].x, y: cnodes[i].y };
                draw_entity(out, node_parts[i], view, at, &cx.options.resources);
            }
        }
    });
    out.finish()
}

fn draw_cluster(out: &mut SceneBuilder, part: PartId, view: &NodeView<'_>, c: &CNode, title: Point) {
    let classes = author_classes(&view.node.css_classes);
    out.layer(LayerSpec::classes("pele-cluster", &classes).part(part, Facet::Shape).data_id(), |out| {
        let shape = view.style.shape.as_ref();
        let frame = out
            .element(part, Facet::Shape)
            .rect(
                [Number::num(c.x - c.w / 2.0), Number::num(c.y - c.h / 2.0)],
                [Number::num(c.w), Number::num(c.h)],
                None,
            )
            .fill(Paint::Role(Role::SurfaceAlt));
        let frame = if shape.is_some_and(Style::sets_fill) { frame } else { frame.fill_opacity(Number::raw(0.5)) };
        frame.stroke(Paint::Role(Role::Border)).style(Some(Style::rounded(shape))).finish();
        let ink = ink_on(shape, view.style.text.as_ref()).unwrap_or(Role::Muted);
        let look = LabelLook::class("pele-cluster-label").fill(Some(Paint::Role(ink))).style(view.style.text.as_ref());
        out.label(part, Facet::Title, &view.label, title, look);
    });
}

/// An entity: its box, and under a header with its name, its table of attributes in stripes.
fn draw_entity(out: &mut SceneBuilder, part: PartId, view: &NodeView<'_>, at: Point, policy: &crate::ResourcePolicy) {
    let node = view.node;
    let classes = author_classes(&node.css_classes);
    let group = LayerSpec::classes("pele-node pele-entity", &classes).part(part, Facet::Shape).data_id().translate(at);
    let style = &view.style;
    let (w, h) = (view.w, view.h);
    let (x0, y0) = (-w / 2.0, -h / 2.0);
    let origin = [Number::num(x0), Number::num(y0)];
    let extent = [Number::num(w), Number::num(h)];
    let ink = ink_on(style.shape.as_ref(), style.text.as_ref()).map(Paint::Role);
    let muted = Some(ink.unwrap_or(Paint::Role(Role::Muted)));
    let text = style.text.as_ref();
    out.layer(group, |out| {
        let frame = ShapeLook::new(Role::Surface, Role::Border, style.shape.as_ref());
        out.element(part, Facet::Shape).rect(origin, extent, None).look(frame.rounded()).finish();
        if !view.rows.is_empty() {
            // Keep row shading light on light fills and dark on dark fills to preserve text contrast.
            let shade = match ink_on(style.shape.as_ref(), None) {
                Some(Role::OnLight) => Role::OnDark,
                Some(Role::OnDark) => Role::OnLight,
                _ => Role::Background,
            };
            let mut stripes = LayerSpec::class("pele-er-stripes").fill(Paint::Role(shade));
            // Over a fill the diagram chose, the shading is a tint of that fill. The opacity is
            // on the group because the two parts of the last row overlap.
            if style.shape.as_ref().is_some_and(|shape| shape.css().contains("fill:")) {
                stripes = stripes.opacity(Number::raw(0.6));
            }
            out.layer(stripes, |out| {
                let mut y = y0 + view.head_h;
                for (r, row) in view.rows.iter().enumerate() {
                    if r % 2 == 0 {
                        let stripe = |out: &mut SceneBuilder, h: f64, style: Option<Style>| {
                            let rect = [Number::num(x0), Number::num(y)];
                            let size = [Number::num(w), Number::num(h)];
                            out.element(part, Facet::Shape).row_rect(rect, size).style(style).finish();
                        };
                        if r == view.rows.len() - 1 {
                            // The last row follows the box's rounded bottom corners: a rounded
                            // rect, squared off on top.
                            stripe(out, row.h, Some(Style::rounded(None)));
                            stripe(out, row.h / 2.0, None);
                        } else {
                            stripe(out, row.h, None);
                        }
                    }
                    y += row.h;
                }
            });
            let divider = y0 + view.head_h;
            let line = match &style.stroke {
                Some(stroke) => out.colour(stroke, Role::Border),
                None => Paint::Role(Role::Border),
            };
            let d = PathProgram::default().m(Number::num(x0), Number::num(divider)).h(Number::num(x0 + w));
            out.element(part, Facet::Shape).class("pele-er-divider").path(d).stroke(line).finish();
            out.element(part, Facet::Shape)
                .rect(origin, extent, None)
                .fill(Paint::None)
                .stroke(Paint::Role(Role::Border))
                .style(Some(Style::rounded(style.line.as_ref())))
                .finish();
        }
        let name_at = Point { x: 0.0, y: if view.rows.is_empty() { 0.0 } else { y0 + view.head_h / 2.0 } };
        let look = LabelLook::class("pele-label").fill(ink).style(text);
        if classes.has("internal-link") && !view.label.lines.is_empty() && policy.links {
            // The name ends up in an href, so it gets the same check as a URL.
            let shown = if node.alias.is_empty() { &node.label } else { &node.alias };
            let display = decode_entities(shown);
            let followable = internal_link_allowed(&display, policy);
            let rel = (!policy.link_rel.is_empty()).then(|| policy.link_rel.clone());
            out.link_inside(part, display.into(), followable, rel, |out| {
                out.label(part, Facet::Label, &view.label, name_at, look);
            });
        } else {
            out.label(part, Facet::Label, &view.label, name_at, look);
        }
        if view.rows.is_empty() {
            return;
        }
        let attributes = LayerSpec::class("pele-er-attributes").font_size(Number::raw(view.row_size));
        out.layer(attributes, |out| {
            let row_line = js::round(view.row_size * 1.5);
            let mut y = y0 + view.head_h;
            for row in &view.rows {
                let baseline = y + ROW_PAD / 2.0 + row_line / 2.0 + view.row_size * 0.35;
                let [kind, name, keys, comment] = &row.cells;
                let cell = |class: &str, fill: Option<Paint>| LabelLook::class(class).fill(fill).style(text);
                let left = |column: usize| x0 + view.cols[column];
                out.label_from(part, Facet::Attribute, kind, left(0), baseline, cell("pele-er-type", muted), view.mono);
                out.label_from(part, Facet::Attribute, name, left(1), baseline, cell("pele-er-name", ink), view.mono);
                let key_look =
                    LabelLook::class("pele-er-keys").font_size(Number::raw(view.key_size)).fill(muted).style(text);
                out.label_from(part, Facet::Attribute, keys, left(2), baseline, key_look, view.mono);
                out.label_from(
                    part,
                    Facet::Attribute,
                    comment,
                    left(3),
                    baseline,
                    cell("pele-er-comment", muted),
                    false,
                );
                y += row.h;
            }
        });
    });
}

pub(crate) struct Er;

impl Diagram for Er {
    type Model = ErModel;

    fn parse(cx: &ParseContext<'_>) -> Result<ErModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderEr`: a diagram that runs across is drawn again running down where that fits
    /// the width the host has better.
    fn scene(model: &ErModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        if matches!(model.direction, Direction::LR | Direction::RL) {
            fit::turn_to_fit(cx.options, |down| draw(model, cx, down))
        } else {
            draw(model, cx, false)
        }
    }
}
