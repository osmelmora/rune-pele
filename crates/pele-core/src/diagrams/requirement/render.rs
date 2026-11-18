//! Drawing a requirement diagram, ported from Pele's `src/diagrams/requirement/render.ts`: each
//! requirement and element is a box with its kind, its name and a table of its fields, laid out
//! with its relationships by the compound layout.

use super::{Relation, Relationship, RequirementModel, Styling};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::graph::edges::Curve;
use crate::graph::edges::{EdgePath, Tip, default_reach, draw_edge_label, route_path};
use crate::input::{JsText, Units, is_space};
use crate::layout::num::by_difference;
use crate::layout::{CEdge, CNode, Dir, LayeredOptions, compound_layout};
use crate::measure::{Emphasis, Label, TextMeasurer, layout_label, width};
use crate::scene::{
    Accessible, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineJoin, Number, Paint,
    PartId, PathProgram, Point, ResolvedStyle, Role, Scene, SceneBuilder, ShapeLook, Size, TextWeight, TitleLayout,
    ink_on, resolve_style,
};
use crate::{RenderError, fit, js};
use indexmap::IndexMap;
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 56.0;
const PAD_X: f64 = 14.0;
const PAD_Y: f64 = 10.0;
const ROW_GAP: f64 = 2.0;
const COLUMN_GAP: f64 = 12.0;
const MIN_WIDTH: f64 = 120.0;
const NAME_WRAP: f64 = 280.0;
const VALUE_WRAP: f64 = 240.0;
const LOOP: f64 = 26.0;
const CONTAINS_RADIUS: f64 = 6.0;
const ARROW_LENGTH: f64 = 9.0;
const ARROW_HALF: f64 = 4.5;
const END_STEP: f64 = 28.0;
const END_MARGIN: f64 = 14.0;

/// `classNames(cssClasses.replace(/^default\s?/, ''))`
fn author_classes(styling: &Styling) -> AuthorClasses {
    let classes = styling.css_classes();
    if classes.starts_at(0, "default") {
        AuthorClasses::new(&classes[7 + usize::from(classes.at(7).is_some_and(is_space))..])
    } else {
        AuthorClasses::new(&classes)
    }
}

/// Pele's `restyle`: adds bold or italic to a whole label after its markdown is read, since
/// emphasis markers only toggle styles the label does not start with. Each run is measured again.
fn restyle(mut label: Label, add: Emphasis, measurer: &dyn TextMeasurer) -> Label {
    if !(add.bold || add.italic) || label.lines.is_empty() {
        return label;
    }
    let mut widest = 0.0;
    for line in &mut label.lines {
        let mut total = 0.0;
        for run in &mut line.runs {
            run.style.bold |= add.bold;
            run.style.italic |= add.italic;
            run.width = width(measurer, &run.text, label.size, run.style);
            total += run.width;
        }
        line.width = total;
        if total > widest {
            widest = total;
        }
    }
    label.width = widest;
    label
}

struct Row {
    key: Label,
    value: Label,
    height: f64,
}

/// A requirement or an element as it is drawn.
struct NodeView<'a> {
    id: &'a JsText,
    styling: &'a Styling,
    requirement: bool,
    style: ResolvedStyle,
    kind: Label,
    name: Label,
    rows: Vec<Row>,
    key_width: f64,
    head_height: f64,
    w: f64,
    h: f64,
}

/// One end of an edge on a side of a node: the edge, which end, and the coordinate across the
/// side of the route point next to it.
struct End {
    edge: usize,
    last: bool,
    toward: f64,
}

/// Pele's `spreadEnds`. The layout attaches every edge to the middle of a node's side. Ends that
/// share a side are spread along it, in the order of where they head, so their markers do not
/// sit on each other.
fn spread_ends(cnodes: &[CNode], cedges: &mut [CEdge], views: &[NodeView<'_>]) {
    let mut sides: IndexMap<usize, Vec<End>> = IndexMap::new();
    for (index, edge) in cedges.iter().enumerate() {
        let route = &edge.route;
        if edge.src == edge.dst || route.len() < 2 {
            continue;
        }
        for last in [false, true] {
            let (at, next, node) = if last { (route.len() - 1, route.len() - 2, edge.dst) } else { (0, 1, edge.src) };
            let c = &cnodes[node];
            let dx = route[at].x - c.x;
            let dy = route[at].y - c.y;
            let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
                if dy < 0.0 { 0 } else { 2 }
            } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
                if dx < 0.0 { 3 } else { 1 }
            } else {
                continue;
            };
            let toward = if side % 2 == 0 { route[next].x } else { route[next].y };
            sides.entry(node * 4 + side).or_default().push(End { edge: index, last, toward });
        }
    }
    for (key, mut list) in sides {
        if list.len() < 2 {
            continue;
        }
        let node = key >> 2;
        let vertical = key % 2 == 1;
        let span = (if vertical { views[node].h } else { views[node].w }) - 2.0 * END_MARGIN;
        let step = js::max(0.0, js::min(END_STEP, span / (list.len() - 1) as f64));
        list.sort_by(|a, b| by_difference(a.toward - b.toward));
        let middle = (list.len() - 1) as f64 / 2.0;
        for (j, end) in list.iter().enumerate() {
            let offset = (j as f64 - middle) * step;
            let route = &mut cedges[end.edge].route;
            let at = if end.last { route.len() - 1 } else { 0 };
            if vertical {
                route[at].y = cnodes[node].y + offset;
            } else {
                route[at].x = cnodes[node].x + offset;
            }
        }
    }
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

/// The containment mark: a circled plus whose rim touches the tip, where its direction points
/// at the node.
fn draw_contains_marker(out: &mut SceneBuilder, part: PartId, tip: Tip) {
    let r = CONTAINS_RADIUS;
    let Tip { x, y, dx, dy } = tip;
    let cx = x - dx * r;
    let cy = y - dy * r;
    let centre = pair(cx, cy).map(Some);
    out.element(part, Facet::StartMarker).class("pele-marker").circle_at(centre, Number::raw(r)).finish();
    let plus = PathProgram::default()
        .m(Number::num(cx - dx * r), Number::num(cy - dy * r))
        .l(Number::num(cx + dx * r), Number::num(cy + dy * r))
        .m(Number::num(cx + dy * r), Number::num(cy - dx * r))
        .l(Number::num(cx - dy * r), Number::num(cy + dx * r));
    out.element(part, Facet::StartMarker).class("pele-marker").path(plus).finish();
}

/// An open arrowhead with its tip at the tip, pointing along its direction.
fn draw_arrow_marker(out: &mut SceneBuilder, part: PartId, tip: Tip) {
    let Tip { x, y, dx, dy } = tip;
    let bx = x - dx * ARROW_LENGTH;
    let by = y - dy * ARROW_LENGTH;
    let head = PathProgram::default()
        .m(Number::num(bx - dy * ARROW_HALF), Number::num(by + dx * ARROW_HALF))
        .l(Number::num(x), Number::num(y))
        .l(Number::num(bx + dy * ARROW_HALF), Number::num(by - dx * ARROW_HALF));
    out.element(part, Facet::EndMarker).class("pele-marker").path(head).join(LineJoin::Round).finish();
}

/// Sizes a requirement or an element from its kind, its name and its fields.
fn node_view<'a>(
    id: &'a JsText,
    styling: &'a Styling,
    kind: &str,
    fields: &[(&str, &[u16])],
    requirement: bool,
    size: f64,
    measurer: &dyn TextMeasurer,
) -> NodeView<'a> {
    let small = js::round(size * 0.875);
    let style = resolve_style(&styling.css_styles);
    let base = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
    let name_size = style.font_size.unwrap_or(size);
    let body_size = style.font_size.unwrap_or(small);
    let plain = Emphasis::default();
    let kind: Vec<u16> = format!("\u{ab}{kind}\u{bb}").encode_utf16().collect();
    let kind = restyle(layout_label(&kind, false, measurer, body_size, 4000.0, plain), base, measurer);
    let name = restyle(
        layout_label(id, true, measurer, name_size, NAME_WRAP, plain),
        Emphasis { bold: true, ..base },
        measurer,
    );
    let mut rows = Vec::new();
    let (mut key_width, mut value_width, mut body_height) = (0.0, 0.0, 0.0);
    for &(label, text) in fields {
        if text.is_empty() {
            continue;
        }
        let label: Vec<u16> = label.encode_utf16().collect();
        let key = restyle(layout_label(&label, false, measurer, body_size, 4000.0, plain), base, measurer);
        let value = restyle(layout_label(text, true, measurer, body_size, VALUE_WRAP, plain), base, measurer);
        let height = js::max(key.height, value.height);
        key_width = js::max(key_width, key.width);
        value_width = js::max(value_width, value.width);
        body_height += height + ROW_GAP;
        rows.push(Row { key, value, height });
    }
    if !rows.is_empty() {
        body_height += 2.0 * PAD_Y - ROW_GAP;
    }
    let head_height = 2.0 * PAD_Y + kind.height + name.height;
    let table = if rows.is_empty() { 0.0 } else { key_width + COLUMN_GAP + value_width + 2.0 * PAD_X };
    let w = js::max(js::max(MIN_WIDTH, js::max(kind.width, name.width) + 2.0 * PAD_X), table);
    let h = head_height + body_height;
    NodeView { id, styling, requirement, style, kind, name, rows, key_width, head_height, w, h }
}

/// A relationship as it is drawn.
struct EdgeView {
    part: PartId,
    contains: bool,
    label: Label,
    path: EdgePath,
    label_at: Point,
}

/// Pele's `draw`. `turned` draws a diagram that runs across as one that runs down.
fn draw(model: &RequirementModel, cx: &DrawContext<'_>, turned: bool) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let measurer = cx.measure;
    let pad = cx.options.padding_or_default();

    let mut views: Vec<NodeView<'_>> = Vec::new();
    for requirement in model.requirements.values() {
        let fields: [(&str, &[u16]); 4] = [
            ("ID", &requirement.requirement_id),
            ("Text", &requirement.text),
            ("Risk", &requirement.risk.map_or(JsText::default(), |risk| risk.name().into())),
            ("Verification", &requirement.verify_method.map_or(JsText::default(), |method| method.name().into())),
        ];
        let kind = requirement.kind.name();
        views.push(node_view(&requirement.name, &requirement.styling, kind, &fields, true, size, measurer));
    }
    for element in model.elements.values() {
        let fields: [(&str, &[u16]); 2] = [("Type", &element.kind), ("Doc Ref", &element.doc_ref)];
        views.push(node_view(&element.name, &element.styling, "Element", &fields, false, size, measurer));
    }
    // A requirement and an element may share a name; relationships then go to the requirement.
    let mut index: HashMap<&[u16], usize> = HashMap::new();
    let mut cnodes: Vec<CNode> = Vec::new();
    for (i, view) in views.iter().enumerate() {
        index.entry(view.id).or_insert(i);
        let mut c = CNode::new(view.w, view.h);
        c.seq = i as f64;
        cnodes.push(c);
    }

    let root_dir = if turned { Dir::TB } else { model.direction };
    let sideways = matches!(root_dir, Dir::LR | Dir::RL);
    let mut cedges: Vec<CEdge> = Vec::new();
    let mut drawn: Vec<(usize, &Relation)> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    for (position, relation) in model.relations.iter().enumerate() {
        let (Some(&src), Some(&dst)) = (index.get(&*relation.src), index.get(&*relation.dst)) else {
            continue;
        };
        let text: Vec<u16> = format!("\u{ab}{}\u{bb}", relation.kind.name()).encode_utf16().collect();
        let label = layout_label(&text, false, measurer, small, 4000.0, Emphasis::default());
        if src == dst {
            // A self-loop sits beside the node, across the flow, and the node's box grows to hold it.
            if sideways {
                cnodes[src].h += 2.0 * (LOOP + label.height + 8.0);
            } else {
                cnodes[src].w += 2.0 * (LOOP + label.width + 12.0);
            }
        }
        let mut cedge = CEdge::new(src, dst);
        cedge.label_w = label.width + 12.0;
        cedge.label_h = label.height + 4.0;
        drawn.push((position, relation));
        edge_labels.push(label);
        cedges.push(cedge);
    }

    let options = LayeredOptions {
        node_sep: NODE_SEP,
        edge_sep: EDGE_SEP,
        rank_sep: RANK_SEP,
        port_sep: 20.0,
        style: crate::layout::EdgeStyle::Curved,
        apart: false,
        head_room: 0.0,
    };
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &options);
    spread_ends(&cnodes, &mut cedges, &views);

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measurer, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = js::max(layout.width, title.width);
    let ox = pad + (inner - layout.width) / 2.0;
    let oy = pad + title_height;
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();

    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width, height }, accessible);
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let node_parts: Vec<PartId> = views.iter().map(|view| out.part(Entity::node(view.id))).collect();

    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let mut loop_seen: HashMap<usize, f64> = HashMap::new();
    let mut edges: Vec<EdgeView> = Vec::new();
    for ((ce, (position, relation)), label) in cedges.iter().zip(&drawn).zip(edge_labels) {
        let name = EntityName::Between {
            from: relation.src.to_vec(),
            to: relation.dst.to_vec(),
            kind: relation.kind.name().to_owned(),
        };
        let duplicate = seen.entry(name.clone()).or_insert(0);
        let part = out.part(Entity::edge(&relation.id(*position), name, *duplicate));
        *duplicate += 1;
        out.relation(part, node_parts[ce.src], node_parts[ce.dst]);

        let contains = relation.kind == Relationship::Contains;
        let start_trim = if contains { 2.0 * CONTAINS_RADIUS } else { 0.0 };
        let mut label_at = Point { x: ce.label_x + ox, y: ce.label_y + oy };
        let path = if ce.src == ce.dst {
            let view = &views[ce.src];
            let cx = cnodes[ce.src].x + ox;
            let cy = cnodes[ce.src].y + oy;
            let k = loop_seen.entry(ce.src).or_insert(0.0);
            let half = (if sideways { view.w } else { view.h }) / 2.0;
            let spread = js::min(half - 4.0, 12.0 + *k * 6.0);
            let reach = LOOP + *k * 8.0;
            *k += 1.0;
            // Local frame: `out` points away from the node, `along` runs along its side.
            let at = |out: f64, along: f64| {
                if sideways {
                    pair(cx + along, cy + view.h / 2.0 + out)
                } else {
                    pair(cx + view.w / 2.0 + out, cy + along)
                }
            };
            // A cubic whose control points are this far out peaks at `reach`.
            let pull = reach / 0.75;
            let [x, y] = at(start_trim, -spread);
            let d = PathProgram::default().m(x, y).c(at(pull, -spread), at(pull, spread), at(0.0, spread));
            let (start, end) = if sideways {
                (
                    Tip { x: cx - spread, y: cy + view.h / 2.0, dx: 0.0, dy: -1.0 },
                    Tip { x: cx + spread, y: cy + view.h / 2.0, dx: 0.0, dy: -1.0 },
                )
            } else {
                (
                    Tip { x: cx + view.w / 2.0, y: cy - spread, dx: -1.0, dy: 0.0 },
                    Tip { x: cx + view.w / 2.0, y: cy + spread, dx: -1.0, dy: 0.0 },
                )
            };
            label_at = if sideways {
                Point { x: cx, y: cy + view.h / 2.0 + reach + 4.0 + ce.label_h / 2.0 }
            } else {
                Point { x: cx + view.w / 2.0 + reach + 4.0 + ce.label_w / 2.0, y: cy }
            };
            EdgePath { d, start, end }
        } else {
            let mut route = ce.route.clone();
            for point in &mut route {
                point.x += ox;
                point.y += oy;
            }
            route_path(&route, Curve::Smooth, [start_trim, 0.0], [default_reach(start_trim), default_reach(0.0)])
        };
        edges.push(EdgeView { part, contains, label, path, label_at });
    }

    let edges_layer =
        LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
    out.layer(edges_layer, |out| {
        for edge in &edges {
            let class = if edge.contains {
                "pele-edge pele-relationship-contains"
            } else {
                "pele-edge pele-relationship-arrow"
            };
            out.layer(LayerSpec::class(class).part(edge.part, Facet::Shape).data_id(), |out| {
                let path = out.element(edge.part, Facet::Shape).path(edge.path.d.clone());
                if edge.contains { path } else { path.dash(&[Number::raw(5.0), Number::raw(4.0)]) }.finish();
                if edge.contains {
                    draw_contains_marker(out, edge.part, edge.path.start);
                } else {
                    draw_arrow_marker(out, edge.part, edge.path.end);
                }
            });
        }
    });

    let labels_layer =
        LayerSpec::class("pele-edge-labels").font_size(Number::raw(small)).fill(Paint::Role(Role::Muted));
    out.layer(labels_layer, |out| {
        for edge in &edges {
            draw_edge_label(out, edge.part, &edge.label, edge.label_at, None);
        }
    });

    out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
        for (i, view) in views.iter().enumerate() {
            let at = Point { x: cnodes[i].x + ox, y: cnodes[i].y + oy };
            draw_node(out, node_parts[i], view, at);
        }
    });
    out.finish()
}

/// One requirement or element: its box, its kind and name, and its fields.
fn draw_node(out: &mut SceneBuilder, part: PartId, view: &NodeView<'_>, at: Point) {
    let style = &view.style;
    let ink = ink_on(style.shape.as_ref(), style.text.as_ref());
    let colour = ink.map(Paint::Role);
    let muted = Some(Paint::Role(ink.unwrap_or(Role::Muted)));
    let text = style.text.as_ref();
    let (w, h) = (view.w, view.h);
    let left = -w / 2.0;
    let top = -h / 2.0;
    let own = if view.requirement { "pele-node pele-requirement" } else { "pele-node pele-element" };
    let group = LayerSpec::classes(own, &author_classes(view.styling)).part(part, Facet::Shape).data_id().translate(at);
    out.layer(group.keep_empty(), |out| {
        let look = ShapeLook::new(Role::Surface, Role::Border, style.shape.as_ref()).rounded();
        out.element(part, Facet::Shape).rect(pair(left, top), pair(w, h), None).look(look).finish();
        let kind_look =
            LabelLook::class("pele-stereotype").font_size(Number::raw(view.kind.size)).fill(muted).style(text);
        let kind_at = Point { x: 0.0, y: top + PAD_Y + view.kind.height / 2.0 };
        out.label(part, Facet::Label, &view.kind, kind_at, kind_look);
        let name_look = LabelLook::class("pele-label")
            .font_size(Number::raw(view.name.size))
            .weight(TextWeight::Heading)
            .fill(colour)
            .style(text);
        let name_at = Point { x: 0.0, y: top + PAD_Y + view.kind.height + view.name.height / 2.0 };
        out.label(part, Facet::Label, &view.name, name_at, name_look);
        if view.rows.is_empty() {
            return;
        }
        let divider = top + view.head_height;
        let line = PathProgram::default().m(Number::num(left), Number::num(divider)).h(Number::num(-left));
        let divider_path = out.element(part, Facet::Shape).class("pele-divider").path(line);
        divider_path.stroke(Paint::Role(Role::Border)).style(style.line.clone()).finish();
        let fields = LayerSpec::class("pele-fields").font_size(Number::raw(view.rows[0].key.size));
        out.layer(fields, |out| {
            let mut y = divider + PAD_Y;
            for row in &view.rows {
                let key_look = LabelLook::class("pele-field").fill(muted).style(text);
                out.start_text(part, Facet::Label, &row.key, left + PAD_X, y, key_look);
                let value_look = LabelLook::class("pele-value").fill(colour).style(text);
                let value_x = left + PAD_X + view.key_width + COLUMN_GAP;
                out.start_text(part, Facet::Label, &row.value, value_x, y, value_look);
                y += row.height + ROW_GAP;
            }
        });
    });
}

fn runs_across(direction: Dir) -> bool {
    matches!(direction, Dir::LR | Dir::RL)
}

pub(crate) struct RequirementDiagram;

impl Diagram for RequirementDiagram {
    type Model = RequirementModel;

    fn parse(cx: &ParseContext<'_>) -> Result<RequirementModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderRequirement`: a diagram that runs across is drawn running down instead
    /// where that fits the host's width better.
    fn scene(model: &RequirementModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        if !runs_across(model.direction) {
            return draw(model, cx, false);
        }
        fit::turn_to_fit(cx.options, |down| draw(model, cx, down))
    }
}
