//! Drawing a use case diagram, ported from Pele's `src/diagrams/usecase/render.ts`: actors, use
//! cases, notes and JSON tables are sized from their text, laid out with their boundaries and
//! relationships, and drawn as a Scene.

use super::graph::{GraphEdge, GraphNode, Mark, NodeShape};
use super::{ActorType, BoundaryType, Direction, LabelType, UsecaseModel, build_usecase_graph};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::graph::cluster::Crossings;
use crate::graph::edges::{
    EdgePath, Fill, Marker, MarkerUse, Over, Tip, casings_layer, draw_casing_with, draw_edge_label_as, draw_marker,
    fill_holding, over_fills, own_reach, route_path,
};
use crate::input::{JsObject, JsText, JsValue, Units, is_space};
use crate::layout::num::hypot;
use crate::layout::{CEdge, CNode, Dir, EdgeStyle, LayeredOptions, compound_layout, shift_layout};
use crate::measure::{Emphasis, Label, TextMeasurer, layout_label};
use crate::scene::{
    Accessible, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineJoin, LineLook, Number,
    Paint, PartId, PathProgram, Point, ResolvedStyle, Role, Scene, SceneBuilder, ShapeLook, Size, Style, TextWeight,
    TitleLayout, ink_on, paints, resolve_style,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

const NODE_SEP: f64 = 32.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 72.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const FIGURE_W: f64 = 40.0;
const FIGURE_H: f64 = 60.0;
const FIGURE_GAP: f64 = 6.0;
const TAB_PAD: f64 = 6.0;
const CELL_PAD: f64 = 10.0;
const ROW_PAD: f64 = 8.0;
const FOLD: f64 = 9.0;
const EXTENSION: f64 = 12.0;
const PORT_GAP: f64 = 14.0;

/// How a node is sized, drawn and met by its edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Actor(ActorType),
    Ellipse,
    /// A rectangular use case or a note.
    Box,
    Json,
    Boundary,
}

/// A side of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    /// Whether the side runs up and down.
    fn upright(self) -> bool {
        matches!(self, Side::Right | Side::Left)
    }

    /// Which way the side lies from the middle of its node.
    fn sign(self) -> f64 {
        if matches!(self, Side::Right | Side::Bottom) { 1.0 } else { -1.0 }
    }
}

/// A node or a boundary as it is drawn.
struct View<'a> {
    node: &'a GraphNode,
    kind: Kind,
    style: ResolvedStyle,
    label: Label,
    stereotype: Label,
    /// The size of what is drawn; the layout box is larger when the node carries a self-relationship.
    w: f64,
    h: f64,
    /// The key and the value of each row of a JSON table.
    rows: Vec<(Label, Label)>,
    key_w: f64,
    head_h: f64,
}

/// A number the config gives for `key`, where it is one above zero, capped at 2000.
fn number_option(section: Option<&JsObject>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|section| section.get(key)) {
        Some(&JsValue::Number(value)) if value > 0.0 => js::min(value, 2000.0),
        _ => fallback,
    }
}

/// What lays out the text of one diagram.
struct Texts<'a> {
    measure: &'a dyn TextMeasurer,
}

impl Texts<'_> {
    /// Pele's `text`. A plain label is shown as written: no tags, no markdown, no `\n` breaks.
    /// Its markup characters are handed to the label parser as character references, which it
    /// turns back into characters.
    fn lay(&self, raw: &[u16], markdown: bool, size: f64, wrap: f64, base: Emphasis) -> Label {
        if markdown || raw.is_empty() {
            return layout_label(raw, markdown, self.measure, size, wrap, base);
        }
        let mut literal = JsText::default();
        for &c in raw {
            match u8::try_from(c) {
                Ok(b'&' | b'<' | b'>' | b'\\') => literal.push_str(&format!("&#{c};")),
                _ => literal.push(c),
            }
        }
        layout_label(&literal, false, self.measure, size, wrap, base)
    }

    /// Text inside guillemets, as a stereotype or the kind of a relationship is shown.
    fn quoted(&self, text: Option<&JsText>, size: f64, base: Emphasis) -> Label {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return Label::default();
        };
        let mut quoted = JsText::from("«");
        quoted.push_units(text);
        quoted.push_str("»");
        self.lay(&quoted, false, size, 200.0, base)
    }
}

/// Pele's `anchor`: the point of a node's outline on one of its sides, `along` away from the
/// middle of that side. Lines meet an actor at its figure, not at its label.
fn anchor(view: &View<'_>, c: &CNode, side: Side, along: f64) -> (f64, f64) {
    let upright = side.upright();
    let mut reach = (if upright { view.w } else { view.h }) / 2.0;
    let mut middle = 0.0;
    match view.kind {
        Kind::Actor(_) if upright => {
            reach = FIGURE_W / 2.0 + 3.0;
            middle = (FIGURE_H - view.h) / 2.0;
        }
        Kind::Actor(_) => reach += 2.0,
        Kind::Ellipse => {
            let span = (if upright { view.h } else { view.w }) / 2.0;
            let off = along / span;
            reach *= js::max(1.0 - off * off, 0.0).sqrt();
        }
        Kind::Box | Kind::Json | Kind::Boundary => {}
    }
    if upright { (c.x + side.sign() * reach, c.y + middle + along) } else { (c.x + along, c.y + side.sign() * reach) }
}

/// Pele's `sideLength`: how much of a side edges may spread over.
fn side_length(view: &View<'_>, side: Side) -> f64 {
    let upright = side.upright();
    let length = if upright { view.h } else { view.w };
    match view.kind {
        Kind::Actor(_) if upright => FIGURE_H * 0.5,
        Kind::Actor(_) => FIGURE_W * 0.6,
        Kind::Ellipse => length * 0.6,
        Kind::Box | Kind::Json | Kind::Boundary => length - 16.0,
    }
}

/// Pele's `attachEnds`. The layout brings every edge that ends on one side of a node to the
/// middle of that side. This spaces them along the side in the order they fan out, and puts
/// each on the node's outline.
fn attach_ends(cedges: &mut [CEdge], cnodes: &[CNode], views: &[View<'_>]) {
    /// An end of an edge: the edge, its point, and where the edge heads from there.
    struct End {
        edge: usize,
        point: usize,
        toward: f64,
    }
    let mut sides: indexmap::IndexMap<(usize, Side), Vec<End>> = indexmap::IndexMap::new();
    for (edge, e) in cedges.iter().enumerate() {
        if e.src == e.dst || e.route.len() < 2 {
            continue;
        }
        let last = e.route.len() - 1;
        for (node, point, next) in [(e.src, 0, 1), (e.dst, last, last - 1)] {
            let c = &cnodes[node];
            if c.is_group {
                continue;
            }
            let dx = e.route[point].x - c.x;
            let dy = e.route[point].y - c.y;
            let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
                if dy < 0.0 { Side::Top } else { Side::Bottom }
            } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
                if dx < 0.0 { Side::Left } else { Side::Right }
            } else {
                continue;
            };
            let toward = if side.upright() { e.route[next].y } else { e.route[next].x };
            sides.entry((node, side)).or_default().push(End { edge, point, toward });
        }
    }
    for ((node, side), mut ends) in sides {
        let view = &views[node];
        let count = ends.len() as f64;
        let step = if ends.len() > 1 { js::min(PORT_GAP, side_length(view, side) / (count - 1.0)) } else { 0.0 };
        ends.sort_by(|a, b| a.toward.partial_cmp(&b.toward).unwrap_or(std::cmp::Ordering::Equal));
        for (i, end) in ends.iter().enumerate() {
            let (x, y) = anchor(view, &cnodes[node], side, (i as f64 - (count - 1.0) / 2.0) * step);
            let point = &mut cedges[end.edge].route[end.point];
            (point.x, point.y) = (x, y);
        }
    }
}

/// Pele's `titleX`. A boundary's title sits in the middle of its top edge. Where edges enter
/// through that edge it moves into the stretch between them that is nearest the middle, or the
/// widest if none has room.
fn title_x(c: &CNode, width: f64, crossings: &Crossings) -> f64 {
    let Some(xs) = crossings.xs(c.y - c.h / 2.0).filter(|_| width != 0.0) else {
        return c.x;
    };
    let left = c.x - c.w / 2.0 + 6.0;
    let right = c.x + c.w / 2.0 - 6.0;
    let mut cuts: Vec<f64> = xs.iter().copied().filter(|&x| x > left && x < right).collect();
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cuts.push(right);
    let need = width / 2.0 + 6.0;
    let mut best = c.x;
    let mut best_off = f64::INFINITY;
    let mut widest = 0.0;
    let mut from = left;
    for cut in cuts {
        if cut - from >= 2.0 * need {
            let x = js::min(js::max(c.x, from + need), cut - need);
            if (x - c.x).abs() < best_off {
                best_off = (x - c.x).abs();
                best = x;
            }
        } else if best_off == f64::INFINITY && cut - from > widest {
            widest = cut - from;
            best = (from + cut) / 2.0;
        }
        from = cut;
    }
    best
}

impl Mark {
    /// The marker the shared edge code draws, for every mark but the triangle of a
    /// generalization, which is this diagram's own.
    fn shared(self) -> Marker {
        match self {
            Mark::None | Mark::Extension => Marker::None,
            Mark::Point => Marker::Point,
            Mark::Circle => Marker::Circle,
            Mark::Cross => Marker::Cross,
        }
    }

    /// Pele's `trim`: how much of the edge's end the mark takes.
    fn trim(self) -> f64 {
        if self == Mark::Extension { EXTENSION } else { self.shared().trim() }
    }
}

/// How a mark is painted: on its edge in the edge's colour, with what shows through a hollow
/// one, or under the edge as part of its casing.
#[derive(Clone, Copy)]
enum MarkUse {
    Edge { colour: Paint, inside: Paint },
    Casing,
}

/// Pele's `mark`: a marker whose tip is at `tip`.
fn draw_mark(out: &mut SceneBuilder, part: PartId, facet: Facet, mark: Mark, tip: Tip, used: MarkUse) {
    if mark != Mark::Extension {
        let used = match used {
            MarkUse::Edge { colour, .. } => MarkerUse::Edge(colour),
            MarkUse::Casing => MarkerUse::Casing,
        };
        draw_marker(out, part, facet, mark.shared(), tip, used);
        return;
    }
    // The hollow triangle of a generalization.
    let Tip { x, y, dx, dy } = tip;
    let bx = x - dx * EXTENSION;
    let by = y - dy * EXTENSION;
    let half = EXTENSION * 0.5;
    let triangle = PathProgram::default()
        .m(Number::num(x), Number::num(y))
        .l(Number::num(bx - dy * half), Number::num(by + dx * half))
        .l(Number::num(bx + dy * half), Number::num(by - dx * half))
        .z();
    let background = Paint::Role(Role::Background);
    let (element, fill, stroke) = match used {
        MarkUse::Edge { colour, inside } => (out.element(part, facet).class("pele-marker"), inside, colour),
        MarkUse::Casing => (out.element(part, facet), background, background),
    };
    element.path(triangle).fill(fill).stroke(stroke).join(LineJoin::Round).finish();
}

fn raw(pairs: &[(f64, f64)]) -> Vec<[Number; 2]> {
    pairs.iter().map(|&(x, y)| [Number::raw(x), Number::raw(y)]).collect()
}

/// Pele's `figure`: the drawing of an actor, centred on the middle of its figure.
fn draw_figure(out: &mut SceneBuilder, part: PartId, view: &View<'_>, kind: ActorType) {
    let shape = view.style.shape.as_ref();
    let line = LineLook::new(Role::Border, view.style.line.as_ref());
    let surface = ShapeLook::new(Role::Surface, Role::Border, shape);
    let page = ShapeLook::new(Role::Background, Role::Border, shape);
    let head = |out: &mut SceneBuilder, radius: f64, look: &ShapeLook| {
        let centre = [None, Some(Number::raw(-21.0))];
        out.element(part, Facet::Shape).circle_at(centre, Number::raw(radius)).look(look.plain()).finish();
    };
    let (mut head_y, mut radius) = (-21.0, 9.0);
    match kind {
        ActorType::Hollow => {
            radius = 8.0;
            head(out, radius, &page);
            let outline = [(20.0, 15.5), (12.0, 25.5), (0.0, 12.0), (-12.0, 25.5), (-20.0, 15.5), (-5.5, 0.0)];
            let mut body = PathProgram::default()
                .m(Number::raw(-20.0), Number::raw(-9.0))
                .h(Number::raw(20.0))
                .v(Number::raw(0.0))
                .h(Number::raw(5.5));
            for [x, y] in raw(&outline) {
                body = body.l(x, y);
            }
            let body = body.h(Number::raw(-20.0)).z();
            let element = out.element(part, Facet::Shape).path(body);
            let element = element.fill(Paint::Role(Role::Background)).stroke(Paint::Role(Role::Border));
            element.join(LineJoin::Round).style(shape.cloned()).finish();
        }
        ActorType::Awesome => {
            let n = Number::raw;
            let curve = |path: PathProgram, points: [(f64, f64); 3]| {
                let [first, second, end] = <[[Number; 2]; 3]>::try_from(raw(&points)).expect("three points");
                path.c(first, second, end)
            };
            let turn = |path: PathProgram, y: f64| path.a_large([n(11.0), n(11.0)], true, [n(0.0), n(y)]);
            let mut body = PathProgram::default().m(n(0.0), n(-30.0));
            body = turn(body, -8.0);
            body = turn(body, -30.0).z().m(n(-20.0), n(25.0));
            body = curve(body, [(-20.0, 9.0), (-12.0, -3.0), (0.0, -3.0)]);
            body = curve(body, [(12.0, -3.0), (20.0, 9.0), (20.0, 25.0)]);
            body = curve(body, [(20.0, 28.0), (18.0, 30.0), (15.0, 30.0)]).h(n(-15.0));
            body = curve(body, [(-18.0, 30.0), (-20.0, 28.0), (-20.0, 25.0)]).z();
            let element = out.element(part, Facet::Shape).path(body).fill(Paint::Role(Role::Border));
            element.style(shape.cloned()).finish();
        }
        ActorType::Icon => {
            (head_y, radius) = (-14.0, 12.0);
            out.element(part, Facet::Shape)
                .rect([Number::raw(-22.0), Number::raw(-26.0)], [Number::raw(44.0); 2], None)
                .look(surface.rounded())
                .finish();
            let name = view.node.icon.clone().unwrap_or_default();
            out.icon(part, &name, [Number::raw(-14.0), Number::raw(-18.0)], Number::raw(28.0));
        }
        ActorType::Normal => {
            head(out, radius, &surface);
            let n = Number::raw;
            let body = PathProgram::default()
                .m(n(0.0), n(-12.0))
                .v(n(10.0))
                .m(n(-14.0), n(-3.0))
                .h(n(14.0))
                .m(n(0.0), n(10.0))
                .l(n(-12.0), n(30.0))
                .m(n(0.0), n(10.0))
                .l(n(12.0), n(30.0));
            out.element(part, Facet::Shape).path(body).look(line.open()).finish();
        }
    }
    if view.node.business == Some(true) {
        // The slash of a business actor: a chord across the lower right of the head.
        let slash = PathProgram::default()
            .m(Number::num(0.12 * radius), Number::num(head_y + 0.99 * radius))
            .l(Number::num(0.92 * radius), Number::num(head_y - 0.39 * radius));
        out.element(part, Facet::Shape).class("pele-business").path(slash).look(line.open()).finish();
    }
}

/// The classes a source gives, after the ones Pele's graph puts first: Pele's `classNames` of
/// the list without its first `skip` words.
fn author_classes(classes: &[u16], skip: usize) -> AuthorClasses {
    let mut at = 0;
    for _ in 0..skip {
        at = classes.run(at, |c| !is_space(c));
        at += usize::from(classes.at(at).is_some_and(is_space));
    }
    AuthorClasses::new(&classes[at..])
}

/// An edge as it is drawn.
struct EdgeView<'a> {
    edge: &'a GraphEdge,
    part: PartId,
    style: Option<ResolvedStyle>,
    label: Label,
    /// The middle of the label.
    label_at: Point,
    path: EdgePath,
    over: Over,
    marks: MarkUse,
}

/// Pele's `leftLabel`: a label whose lines each start at `x`.
fn left_label(out: &mut SceneBuilder, part: PartId, label: &Label, x: f64, cy: f64, look: impl Fn() -> LabelLook) {
    if label.lines.len() < 2 {
        out.label(part, Facet::Row, label, Point { x: x + label.width / 2.0, y: cy }, look());
        return;
    }
    for (i, line) in label.lines.iter().enumerate() {
        let at = Point { x: x + line.width / 2.0, y: cy - label.height / 2.0 + label.line_middle(i) };
        out.label(part, Facet::Row, &label.line(i), at, look());
    }
}

/// Pele's `draw`. `turned` draws a diagram that runs across as one that runs down.
fn draw(model: &UsecaseModel, cx: &DrawContext<'_>, turned: bool) -> Result<Scene, RenderError> {
    let graph = build_usecase_graph(model);
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let tiny = js::round(size * 0.8125);
    let texts = Texts { measure: cx.measure };
    let pad = cx.options.padding_or_default();
    let section = cx.config.get("usecase").and_then(JsValue::as_record);
    let root_dir = if turned { Dir::TB } else { Dir::named(Some(model.direction.name())) };
    let sideways = matches!(root_dir, Dir::LR | Dir::RL);

    let mut index: HashMap<&[u16], usize> = HashMap::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(&node.id, i);
    }

    let mut views: Vec<View<'_>> = Vec::new();
    let mut cnodes: Vec<CNode> = Vec::new();
    for node in &graph.nodes {
        let style = resolve_style(node.css_compiled_styles.iter().chain(&node.css_styles));
        let base = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
        let font_size = style.font_size.unwrap_or(size);
        let markdown = node.label_type == LabelType::Markdown;
        let stereotype = texts.quoted(node.stereotype.as_ref(), tiny, base);
        let mut view = View {
            node,
            kind: Kind::Box,
            style,
            label: Label::default(),
            stereotype,
            w: 0.0,
            h: 0.0,
            rows: Vec::new(),
            key_w: 0.0,
            head_h: 0.0,
        };
        let mut c;
        if node.is_group {
            view.kind = Kind::Boundary;
            view.label = texts.lay(&node.label, markdown, font_size, 4000.0, base);
            let title_h = view.label.height;
            c = CNode::new(0.0, 0.0);
            c.is_group = true;
            c.pad_x = GROUP_PAD;
            c.pad_bottom = GROUP_PAD;
            c.pad_top = if node.boundary_type == Some(BoundaryType::Package) {
                title_h + 2.0 * TAB_PAD + 16.0
            } else if title_h > 0.0 {
                title_h + 16.0
            } else {
                GROUP_PAD
            };
            c.min_w = view.label.width + 2.0 * GROUP_PAD;
        } else {
            if let Some(rows) = &node.json_rows {
                view.kind = Kind::Json;
                view.label = texts.lay(&node.label, false, font_size, 4000.0, Emphasis { bold: true, ..base });
                let mut value_w = 0.0;
                for row in rows {
                    let key = texts.lay(&row.key, false, small, 4000.0, base);
                    let value = texts.lay(&row.value, false, small, 260.0, base);
                    view.key_w = js::max(view.key_w, key.width);
                    value_w = js::max(value_w, value.width);
                    view.h += js::max(js::max(key.height, value.height), js::round(small * 1.5)) + ROW_PAD;
                    view.rows.push((key, value));
                }
                // The root of an empty object has a value and no key, and then there is no key column.
                if view.key_w > 0.0 {
                    view.key_w += 2.0 * CELL_PAD;
                }
                view.head_h = js::max(view.label.height, js::round(font_size * 1.5)) + ROW_PAD;
                let body_w = if view.rows.is_empty() { 0.0 } else { view.key_w + value_w + 2.0 * CELL_PAD };
                view.w = js::max(js::max(view.label.width + 2.0 * CELL_PAD, body_w), 72.0);
                view.h += view.head_h;
            } else if let NodeShape::Actor(kind) = node.shape {
                view.kind = Kind::Actor(kind);
                view.label = texts.lay(&node.label, markdown, font_size, 140.0, base);
                view.w = js::max(js::max(FIGURE_W, view.label.width), view.stereotype.width);
                view.h = FIGURE_H + FIGURE_GAP + view.stereotype.height + view.label.height;
            } else {
                let note = node.shape == NodeShape::Note;
                view.label = texts.lay(&node.label, markdown, if note { small } else { font_size }, 180.0, base);
                let tw = js::max(view.label.width, view.stereotype.width);
                let th = view.label.height + view.stereotype.height;
                if matches!(node.shape, NodeShape::Ellipse | NodeShape::Business) {
                    view.kind = Kind::Ellipse;
                    // An oval needs more room around its text than a box, and a business oval room for its slash.
                    let slash = if node.business == Some(true) { 16.0 } else { 4.0 };
                    view.w = js::max(tw + 2.0 * node.padding + slash, 88.0);
                    view.h = js::max(th + 28.0, 48.0);
                } else if note {
                    view.w = tw + 2.0 * node.padding + 6.0;
                    view.h = th + 18.0;
                } else {
                    view.w = js::max(tw + 2.0 * node.padding + 12.0, 80.0);
                    view.h = js::max(th + 24.0, 44.0);
                }
            }
            c = CNode::new(view.w, view.h);
        }
        c.seq = views.len() as f64;
        views.push(view);
        cnodes.push(c);
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node.parent_id.as_ref().and_then(|parent| index.get(&**parent).copied());
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(&*edge.start), index.get(&*edge.end)) else {
            continue;
        };
        let label = if edge.kind.semantic() {
            texts.quoted(edge.label.as_ref(), small, Emphasis::default())
        } else {
            let markdown = edge.label_type == Some(LabelType::Markdown);
            texts.lay(edge.label.as_deref().unwrap_or_default(), markdown, small, 200.0, Emphasis::default())
        };
        if src == dst {
            // A self-relationship loops out across the flow, and the node's box grows to hold it.
            if sideways {
                cnodes[src].h += 2.0 * (LOOP + if label.height > 0.0 { label.height + 8.0 } else { 0.0 });
            } else {
                cnodes[src].w += 2.0 * (LOOP + if label.width > 0.0 { label.width + 12.0 } else { 0.0 });
            }
        }
        let mut cedge = CEdge::new(src, dst);
        cedge.minlen = edge.minlen as f64;
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

    // Where edges cross each boundary's top border, so that a title can keep out of their way.
    let mut crossings = Crossings::default();
    for e in &cedges {
        crossings.mark(&e.route);
    }
    attach_ends(&mut cedges, &cnodes, &views);

    let accessible = Accessible { title: Some(model.acc_title.clone()), description: Some(model.acc_descr.clone()) };
    let mut out = SceneBuilder::new(cx, Size { width, height }, accessible);
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });

    let node_parts: Vec<PartId> = graph
        .nodes
        .iter()
        .map(|node| out.part(if node.is_group { Entity::group(&node.id) } else { Entity::node(&node.id) }))
        .collect();
    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let edge_parts: Vec<PartId> = drawn
        .iter()
        .map(|edge| {
            let name = if edge.explicit_id {
                EntityName::Named(edge.id.to_vec())
            } else {
                let kind = edge.kind.name().to_owned();
                EntityName::Between { from: edge.start.to_vec(), to: edge.end.to_vec(), kind }
            };
            let duplicate = seen.entry(name.clone()).or_insert(0);
            let part = out.part(Entity::edge(&edge.id, name, *duplicate));
            *duplicate += 1;
            part
        })
        .collect();
    for (i, ce) in cedges.iter().enumerate() {
        out.relation(edge_parts[i], node_parts[ce.src], node_parts[ce.dst]);
    }

    let mut fills: Vec<Fill<'_>> = Vec::new();
    for (view, c) in views.iter().zip(&cnodes) {
        let shape = view.style.shape.as_ref();
        if view.kind == Kind::Boundary && paints(shape) {
            let (x, y) = (c.x - c.w / 2.0, c.y - c.h / 2.0);
            fills.push(Fill { bounds: [x, y, x + c.w, y + c.h], shape });
        }
    }

    let mut edges: Vec<EdgeView<'_>> = Vec::new();
    let mut loop_seen: HashMap<usize, f64> = HashMap::new();
    for (i, label) in edge_labels.into_iter().enumerate() {
        let edge = drawn[i];
        let style = (!edge.style.is_empty() || !edge.css_compiled_styles.is_empty())
            .then(|| resolve_style(edge.css_compiled_styles.iter().chain(&edge.style)));
        let (start, end) = (edge.arrow_start, edge.arrow_end);
        let ce = &cedges[i];
        let mut label_at = Point { x: ce.label_x, y: ce.label_y };
        let path = if ce.src == ce.dst {
            let (c, view) = (&cnodes[ce.src], &views[ce.src]);
            let k = loop_seen.entry(ce.src).or_insert(0.0);
            let side = if sideways { Side::Bottom } else { Side::Right };
            let spread = js::min(side_length(view, side) / 2.0, 8.0 + *k * 6.0);
            let reach = LOOP + *k * 8.0;
            *k += 1.0;
            let (ax, ay) = anchor(view, c, side, -spread);
            let (bx, by) = anchor(view, c, side, spread);
            let len = hypot(reach, spread);
            let len = if len == 0.0 || len.is_nan() { 1.0 } else { len };
            let (ux, uy) = if sideways { (spread / len, reach / len) } else { (reach / len, spread / len) };
            let (st, et) = (start.trim(), end.trim());
            // Both ends leave the node leaning away from each other, and the curve joins them outside it.
            let out_of = |x: f64, y: f64, sign: f64| {
                let lean = sign * spread;
                if sideways {
                    [Number::num(x + lean), Number::num(y + reach)]
                } else {
                    [Number::num(x + reach), Number::num(y + lean)]
                }
            };
            let (sx, sy) = if sideways { (ax - ux * st, ay + uy * st) } else { (ax + ux * st, ay - uy * st) };
            let (ex, ey) = (bx + ux * et, by + uy * et);
            label_at = if sideways {
                Point { x: c.x, y: js::max(ay, by) + reach * 0.75 + 6.0 + label.height / 2.0 }
            } else {
                Point { x: js::max(ax, bx) + reach * 0.75 + 8.0 + label.width / 2.0, y: c.y }
            };
            let d = PathProgram::default().m(Number::num(sx), Number::num(sy)).c(
                out_of(ax, ay, -1.0),
                out_of(bx, by, 1.0),
                [Number::num(ex), Number::num(ey)],
            );
            let (sdx, sdy) = if sideways { (ux, -uy) } else { (-ux, uy) };
            EdgePath { d, start: Tip { x: ax, y: ay, dx: sdx, dy: sdy }, end: Tip { x: bx, y: by, dx: -ux, dy: -uy } }
        } else {
            if ce.route.len() < 2 {
                continue;
            }
            let trims = [start.trim(), end.trim()];
            route_path(&ce.route, crate::graph::edges::Curve::Smooth, trims, trims.map(own_reach))
        };
        let line_style = style.as_ref().and_then(|style| style.line.as_ref());
        let over = over_fills(&path, &fills, line_style);
        let colour = match (over, style.as_ref().and_then(|style| style.stroke.as_ref())) {
            (Over::Ink(ink), _) => Paint::Role(ink),
            (_, Some(stroke)) => out.colour(stroke, Role::Line),
            (_, None) => Paint::Role(Role::Line),
        };
        // A hollow marker on a fill shows the fill's colour inside it, where the edge takes the fill's ink.
        let hollow = [start, end].contains(&Mark::Extension) && matches!(over, Over::Ink(_));
        let inside = hollow
            .then(|| fill_holding(&path, &fills).and_then(|fill| fill.shape).and_then(Style::fill_value))
            .flatten()
            .map(|fill| out.colour(&fill.encode_utf16().collect::<Vec<u16>>(), Role::Background));
        let marks = MarkUse::Edge { colour, inside: inside.unwrap_or(Paint::Role(Role::Background)) };
        edges.push(EdgeView { edge, part: edge_parts[i], style, label, label_at, path, over, marks });
    }

    out.layer(LayerSpec::class("pele-clusters"), |out| {
        for (i, view) in views.iter().enumerate() {
            if view.kind != Kind::Boundary {
                continue;
            }
            let (c, part) = (&cnodes[i], node_parts[i]);
            let classes = author_classes(&view.node.css_classes, 1);
            let group = LayerSpec::classes("pele-cluster pele-boundary", &classes).part(part, Facet::Shape).data_id();
            out.layer(group, |out| {
                let shape = view.style.shape.as_ref();
                let (x, y) = (c.x - c.w / 2.0, c.y - c.h / 2.0);
                let frame = |out: &mut SceneBuilder,
                             class: Option<&str>,
                             origin: (f64, f64),
                             size: (f64, f64),
                             round: bool| {
                    let element = out.element(part, Facet::Shape);
                    let element = match class {
                        Some(class) => element.class(class),
                        None => element,
                    };
                    let frame = element.rect(
                        [Number::num(origin.0), Number::num(origin.1)],
                        [Number::num(size.0), Number::num(size.1)],
                        None,
                    );
                    let frame = frame.fill(Paint::Role(Role::SurfaceAlt));
                    let frame =
                        if shape.is_some_and(Style::sets_fill) { frame } else { frame.fill_opacity(Number::raw(0.5)) };
                    let style = if round { Some(Style::rounded(shape)) } else { shape.cloned() };
                    frame.stroke(Paint::Role(Role::Border)).style(style).finish();
                };
                let ink = ink_on(shape, view.style.text.as_ref());
                let look = LabelLook::class("pele-cluster-label")
                    .fill(Some(Paint::Role(ink.unwrap_or(Role::Muted))))
                    .style(view.style.text.as_ref());
                let at = if view.node.boundary_type == Some(BoundaryType::Package) {
                    let tab_w = js::min(view.label.width + 2.0 * TAB_PAD + 8.0, c.w);
                    let tab_h = view.label.height + 2.0 * TAB_PAD;
                    frame(out, Some("pele-boundary-tab"), (x, y), (tab_w, tab_h), false);
                    frame(out, None, (x, y + tab_h), (c.w, c.h - tab_h), false);
                    Point { x: x + tab_w / 2.0, y: y + tab_h / 2.0 }
                } else {
                    frame(out, None, (x, y), (c.w, c.h), true);
                    Point { x: title_x(c, view.label.width, &crossings), y: y + 8.0 + view.label.height / 2.0 }
                };
                out.label(part, Facet::Title, &view.label, at, look);
            });
        }
    });

    out.layer(casings_layer(), |out| {
        for view in edges.iter().filter(|view| view.over == Over::Cased) {
            let line_style = view.style.as_ref().and_then(|style| style.line.as_ref());
            draw_casing_with(out, view.part, &view.path, line_style, |out| {
                draw_mark(out, view.part, Facet::Casing, view.edge.arrow_start, view.path.start, MarkUse::Casing);
                draw_mark(out, view.part, Facet::Casing, view.edge.arrow_end, view.path.end, MarkUse::Casing);
            });
        }
    });

    let edges_layer =
        LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
    out.layer(edges_layer, |out| {
        for view in &edges {
            let edge = view.edge;
            let own = format!("pele-edge pele-{}", edge.kind.name());
            let mut group =
                LayerSpec::classes(&own, &author_classes(&edge.classes, 3)).part(view.part, Facet::Shape).data_id();
            if let Over::Ink(ink) = view.over {
                group = group.stroke(Paint::Role(ink));
            }
            let group = group.style(view.style.as_ref().and_then(|style| style.line.clone()));
            out.layer(group, |out| {
                let path = out.element(view.part, Facet::Shape).path(view.path.d.clone());
                // Mermaid animates these dashes. A still picture keeps the dashes.
                let dashes = if edge.internal {
                    Some([2.0, 3.0])
                } else if edge.dotted {
                    Some([5.0, 4.0])
                } else if edge.animate {
                    Some([9.0, 5.0])
                } else {
                    None
                };
                match dashes {
                    Some(dashes) => path.dash(&dashes.map(Number::raw)),
                    None => path,
                }
                .finish();
                draw_mark(out, view.part, Facet::StartMarker, edge.arrow_start, view.path.start, view.marks);
                draw_mark(out, view.part, Facet::EndMarker, edge.arrow_end, view.path.end, view.marks);
            });
        }
    });

    out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(small)), |out| {
        for view in &edges {
            if view.label.width > 0.0 {
                let muted = view.edge.kind.semantic().then_some(Paint::Role(Role::Muted));
                let text = view.style.as_ref().and_then(|style| style.text.as_ref());
                let look = LabelLook::default().fill(muted).style(text);
                draw_edge_label_as(out, view.part, &view.label, view.label_at, look);
            }
        }
    });

    out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
        for (i, view) in views.iter().enumerate() {
            if view.kind != Kind::Boundary {
                draw_node(out, node_parts[i], view, &cnodes[i], small, tiny);
            }
        }
    });
    out.finish()
}

/// One actor, use case, note or JSON table, centred on its place.
fn draw_node(out: &mut SceneBuilder, part: PartId, view: &View<'_>, c: &CNode, small: f64, tiny: f64) {
    let node = view.node;
    let (w, h) = (view.w, view.h);
    let shape = view.style.shape.as_ref();
    let text = view.style.text.as_ref();
    let line = LineLook::new(Role::Border, view.style.line.as_ref());
    let surface = ShapeLook::new(Role::Surface, Role::Border, shape);
    let ink = ink_on(shape, text).map(Paint::Role);
    let muted = Some(ink.unwrap_or(Paint::Role(Role::Muted)));
    let kind = match view.kind {
        Kind::Actor(_) => "pele-actor",
        Kind::Json => "pele-json",
        Kind::Box if node.shape == NodeShape::Note => "pele-note",
        Kind::Ellipse | Kind::Box | Kind::Boundary => "pele-usecase",
    };
    let own = format!("pele-node {kind}");
    let group = LayerSpec::classes(&own, &author_classes(&node.css_classes, 1))
        .part(part, Facet::Shape)
        .data_id()
        .translate(Point { x: c.x, y: c.y })
        .keep_empty();
    let stereotype = |fill: Option<Paint>| LabelLook::class("pele-stereotype").font_size(Number::raw(tiny)).fill(fill);
    out.layer(group, |out| match view.kind {
        Kind::Actor(actor) => {
            let top = -h / 2.0;
            let figure = LayerSpec::class("pele-actor-figure").translate(Point { x: 0.0, y: top + FIGURE_H / 2.0 });
            out.layer(figure, |out| draw_figure(out, part, view, actor));
            let at = Point { x: 0.0, y: top + FIGURE_H + FIGURE_GAP + view.stereotype.height / 2.0 };
            out.label(part, Facet::Stereotype, &view.stereotype, at, stereotype(Some(Paint::Role(Role::Muted))));
            let at = Point { x: 0.0, y: h / 2.0 - view.label.height / 2.0 };
            out.label(part, Facet::Label, &view.label, at, LabelLook::class("pele-label").style(text));
        }
        Kind::Json => {
            let x0 = -w / 2.0;
            let mut y = -h / 2.0 + view.head_h;
            let mut lines = PathProgram::default().m(Number::num(x0), Number::num(y)).h(Number::num(x0 + w));
            if view.key_w > 0.0 {
                lines = lines.m(Number::num(x0 + view.key_w), Number::num(y)).v(Number::num(h / 2.0));
            }
            out.element(part, Facet::Shape)
                .rect([Number::num(x0), Number::num(-h / 2.0)], [Number::num(w), Number::num(h)], None)
                .look(surface.rounded())
                .finish();
            out.element(part, Facet::Shape).class("pele-json-lines").path(lines).look(line.open()).finish();
            let name = LabelLook::class("pele-label").weight(TextWeight::Heading).fill(ink).style(text);
            out.label(part, Facet::Label, &view.label, Point { x: 0.0, y: -h / 2.0 + view.head_h / 2.0 }, name);
            out.layer(LayerSpec::group().font_size(Number::raw(small)).keep_empty(), |out| {
                for (key, value) in &view.rows {
                    let row_h = js::max(js::max(key.height, value.height), js::round(small * 1.5)) + ROW_PAD;
                    let at = Point { x: x0 + CELL_PAD + key.width / 2.0, y: y + row_h / 2.0 };
                    out.label(part, Facet::Row, key, at, LabelLook::class("pele-json-key").fill(muted));
                    let look = || LabelLook::class("pele-json-value").fill(ink).style(text);
                    left_label(out, part, value, x0 + view.key_w + CELL_PAD, y + row_h / 2.0, look);
                    y += row_h;
                }
            });
        }
        Kind::Ellipse | Kind::Box | Kind::Boundary => {
            let note = node.shape == NodeShape::Note;
            if view.kind == Kind::Ellipse {
                let oval = out.element(part, Facet::Shape).ellipse(Number::num(w / 2.0), Number::num(h / 2.0));
                oval.look(surface.plain()).finish();
                if node.business == Some(true) {
                    // The slash of a business use case, cutting off the right end of the oval beside the text.
                    let (from, to) = (w / 2.0 - 15.0, w / 2.0 - 5.0);
                    let arc = |at: f64| {
                        let off = at / (w / 2.0);
                        (h / 2.0) * (1.0 - off * off).sqrt()
                    };
                    let slash = PathProgram::default()
                        .m(Number::num(from), Number::num(arc(from)))
                        .l(Number::num(to), Number::num(-arc(to)));
                    out.element(part, Facet::Shape).class("pele-business").path(slash).look(line.open()).finish();
                }
            } else if note {
                let (r, b) = (w / 2.0, h / 2.0);
                let sheet = PathProgram::default()
                    .m(Number::num(-r), Number::num(-b))
                    .h(Number::num(r - FOLD))
                    .l(Number::num(r), Number::num(FOLD - b))
                    .v(Number::num(b))
                    .h(Number::num(-r))
                    .z();
                let element = out.element(part, Facet::Shape).path(sheet);
                let element = element.fill(Paint::Role(Role::SurfaceAlt)).stroke(Paint::Role(Role::Border));
                element.join(LineJoin::Round).style(shape.cloned()).finish();
                let fold = PathProgram::default()
                    .m(Number::num(r - FOLD), Number::num(-b))
                    .v(Number::num(FOLD - b))
                    .h(Number::num(r));
                let element = out.element(part, Facet::Shape).path(fold);
                let element = element.fill(Paint::None).stroke(Paint::Role(Role::Border));
                element.join(LineJoin::Round).style(view.style.line.clone()).finish();
            } else {
                out.element(part, Facet::Shape)
                    .rect([Number::num(-w / 2.0), Number::num(-h / 2.0)], [Number::num(w), Number::num(h)], None)
                    .look(surface.rounded())
                    .finish();
            }
            let at = Point { x: 0.0, y: -view.label.height / 2.0 };
            out.label(part, Facet::Stereotype, &view.stereotype, at, stereotype(muted));
            let look = LabelLook::class("pele-label");
            let look = if note { look.font_size(Number::raw(small)) } else { look };
            let at = Point { x: 0.0, y: view.stereotype.height / 2.0 };
            out.label(part, Facet::Label, &view.label, at, look.fill(ink).style(text));
        }
    });
}

pub(crate) struct Usecase;

impl Diagram for Usecase {
    type Model = UsecaseModel;

    fn parse(cx: &ParseContext<'_>) -> Result<UsecaseModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderUsecase`: a diagram that runs across is drawn running down instead where
    /// that is what fits the width the host has.
    fn scene(model: &UsecaseModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        if matches!(model.direction, Direction::LR | Direction::RL) {
            fit::turn_to_fit(cx.options, |down| draw(model, cx, down))
        } else {
            draw(model, cx, false)
        }
    }
}
