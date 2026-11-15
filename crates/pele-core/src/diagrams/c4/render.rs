//! Drawing a C4 diagram, ported from Pele's `src/diagrams/c4/render.ts`. Every relation is routed
//! before any label is placed, so that labels keep off arrowheads; a boundary's heading moves to
//! where no line crosses it; each label slides along its line to the spot that covers least.
//! Where the host's width is short, the diagram is drawn tight, with a wider gap between columns
//! tried while some relation finds no lane of its own.

use super::layout::{self, HEAD_LEFT, HEAD_TOP, Layout, Texts, bracketed};
use super::route::{self, BoxId, Ends, Form, LABEL_PAD, LANE, Lanes, Route, crosses, cyl_ry, queue_rx, truthy};
use super::{Boundary, C4Kind, C4Model, ElementField, Found, Rel, RelField, Shape as C4Shape};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::diagrams::flowchart::Shape;
use crate::graph::edges::{ARROW, Fill, Marker, MarkerUse, Over, casings_layer, draw_casing, draw_marker, over_fills};
use crate::graph::shapes::draw_shape;
use crate::input::JsText;
use crate::input::url::{link_allowed, sanitize_url};
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineLook, Number, Paint, PartId, Point,
    ResolvedStyle, Role, Scene, SceneBuilder, ShapeLook, Size, Style, TextWeight, TitleLayout, ink_on, paints,
    resolve_style,
};
use crate::{Diagnostic, ErrorCode, RenderError, fit, js};
use std::collections::{HashMap, HashSet};

const PAIR_SHIFT: f64 = 7.0;
const NARROW_GAP: f64 = 36.0;
const WIDEN: f64 = 24.0;
const MAX_OFFSET: f64 = 2000.0;
// A label may cover this much of a shape's edge, where there is no text.
const EDGE: f64 = 8.0;
// Places along a relation to try for its label, nearest the middle first.
const SLIDE: [f64; 7] = [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8];
// Above this many box comparisons, labels stay at the middle and lines are not steered round shapes.
const CARE_BUDGET: usize = 2_000_000;

/// Pele's `userStyle`: the colours a diagram gives an element or a relation, as style declarations.
fn user_style(fill: Option<&JsText>, stroke: Option<&JsText>, color: Option<&JsText>) -> ResolvedStyle {
    let mut declarations = Vec::new();
    for (property, value) in [("fill:", fill), ("stroke:", stroke), ("color:", color)] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            let mut declaration = JsText::from(property);
            declaration.push_units(value);
            declarations.push(declaration);
        }
    }
    resolve_style(&declarations)
}

/// Pele's `link`: where a link written in a diagram leads, if the host lets it. Entity encoding
/// has hidden any `#name;` in the address; it is put back, so that the check sees what a browser
/// would.
fn link_href(url: Option<&JsText>, policy: &crate::ResourcePolicy) -> Option<JsText> {
    let url = url.filter(|url| !url.is_empty())?;
    let hidden = url.windows(2).any(|pair| pair == [0xb6, 0xdf]);
    let mut restored = JsText::default();
    let mut at = 0;
    while at < url.len() {
        let next = url.get(at + 1).copied();
        if hidden && url[at] == 0xfb02 && next == Some(0xb0) {
            restored.push(u16::from(b'#'));
            at += if url.get(at + 2) == Some(&0xb0) { 3 } else { 2 };
        } else if hidden && url[at] == 0xb6 && next == Some(0xdf) {
            restored.push(u16::from(b';'));
            at += 2;
        } else {
            restored.push(url[at]);
            at += 1;
        }
    }
    let href = sanitize_url(&restored);
    link_allowed(&href, policy).then_some(href)
}

/// Draws inside a link where there is one.
fn linked(
    out: &mut SceneBuilder,
    part: PartId,
    href: Option<JsText>,
    rel: &Option<String>,
    draw: impl FnOnce(&mut SceneBuilder),
) {
    match href {
        Some(href) => out.link_to(part, href, None, rel.clone(), draw),
        None => draw(out),
    }
}

fn form_shape(form: Form) -> Shape {
    match form {
        Form::Rect => Shape::Rect,
        Form::Person => Shape::Person,
        Form::Cyl => Shape::Cyl,
        Form::HCyl => Shape::HCyl,
    }
}

/// How far the drawing reaches, from the top-left corner of its top level.
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn grow(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.min_x = js::min(self.min_x, x0);
        self.min_y = js::min(self.min_y, y0);
        self.max_x = js::max(self.max_x, x1);
        self.max_y = js::max(self.max_y, y1);
    }
}

/// How much of the rectangle the boxes in `taken` cover, each given as left, top, right, bottom.
fn covered(taken: &[f64], x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let mut area = 0.0;
    for t in taken.chunks(4) {
        let w = js::min(x1, t[2]) - js::max(x0, t[0]);
        if w <= 0.0 {
            continue;
        }
        let h = js::min(y1, t[3]) - js::max(y0, t[1]);
        if h > 0.0 {
            area += w * h;
        }
    }
    area
}

/// An offset the diagram gives a label, capped.
fn offset(value: Option<f64>) -> f64 {
    value.filter(|&value| truthy(value)).map_or(0.0, |value| js::max(-MAX_OFFSET, js::min(MAX_OFFSET, value)))
}

/// A relation's label and its technology, on a patch of the background.
struct Placed {
    label: Label,
    techn: Label,
    cx: f64,
    top: f64,
    lw: f64,
    lh: f64,
    text: ResolvedStyle,
}

/// A relation as it is drawn.
struct RelView<'a> {
    rel: &'a Rel,
    route: Route,
    line: ResolvedStyle,
    over: Over,
    placed: Option<Placed>,
}

/// The box of what an alias names, where it is drawn, and what that is.
fn drawn(model: &C4Model, laid: &Layout, alias: &JsText, attr: bool) -> Option<(BoxId, Found)> {
    if attr {
        return None;
    }
    let found = model.find(alias)?;
    let at = match found {
        Found::Shape(i) => laid.shape_boxes[i],
        Found::Boundary(i) => laid.boundary_boxes[i],
    }?;
    Some((at, found))
}

/// Pele's `draw`: the drawing, and how many relations found no lane of their own. A tight
/// drawing fits the host's width, with `gap` between its columns.
fn draw(model: &C4Model, cx: &DrawContext<'_>, tight: bool, gap: f64) -> Result<(Scene, usize), RenderError> {
    let options = cx.options;
    let size = options.typography.normal.size;
    let small = js::round(size * 0.8125);
    let pad = options.padding;
    let texts = Texts { measurer: cx.measure, size, small };
    let room = tight.then(|| js::max(80.0, options.max_width.unwrap_or(320.0) - 2.0 * pad));
    let mut laid = layout::layout(model, &texts, room, gap);
    let root = laid.boxes[laid.groups[0].at];
    let mut bounds = Bounds { min_x: 0.0, min_y: 0.0, max_x: root.w, max_y: root.h };

    let mut ends = Vec::with_capacity(model.rels.len());
    let mut pairs: HashMap<BoxId, HashSet<BoxId>> = HashMap::new();
    for rel in &model.rels {
        let from = drawn(model, &laid, &rel.from, rel.from_attr.is_some());
        let to = drawn(model, &laid, &rel.to, rel.to_attr.is_some());
        let (Some(from), Some(to)) = (from, to) else {
            let message = format!(
                "C4 rel \"{}\" -> \"{}\" references an unknown shape or boundary",
                rel.from.to_display(),
                rel.to.to_display()
            );
            return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message)));
        };
        pairs.entry(from.0).or_default().insert(to.0);
        ends.push((from, to));
    }

    // What a relation's label should not cover: shapes, arrowheads, boundary headings, and the
    // labels placed before it.
    let mut nodes = Vec::new();
    let mut taken = Vec::new();
    for &g in &laid.order {
        for node in &laid.groups[g].nodes {
            let b = laid.boxes[node.at];
            nodes.push(node.at);
            taken.extend([b.x + EDGE, b.y + EDGE, b.x + b.w - EDGE, b.y + b.h - EDGE]);
        }
    }
    let careful = model.rels.len() * (nodes.len() + laid.order.len() + 3 * model.rels.len()) <= CARE_BUDGET;

    // Every line is drawn before any label is placed, so that labels can keep off the arrowheads.
    let trim = Marker::Point.trim();
    let fill_styles: Vec<(BoxId, ResolvedStyle)> = laid.order[1..]
        .iter()
        .map(|&g| (laid.groups[g].at, user_style(model.boundaries[g].element.field(ElementField::BgColor), None, None)))
        .collect();
    let fills: Vec<Fill<'_>> = fill_styles
        .iter()
        .filter(|(_, style)| paints(style.shape.as_ref()))
        .map(|(at, style)| {
            let b = laid.boxes[*at];
            Fill { bounds: [b.x, b.y, b.x + b.w, b.y + b.h], shape: style.shape.as_ref() }
        })
        .collect();
    let mut lanes = Lanes::default();
    for &g in &laid.order[1..] {
        lanes.frame(&laid.boxes[laid.groups[g].at]);
    }
    let mut views = Vec::with_capacity(model.rels.len());
    for (rel, &((a, _), (b, _))) in model.rels.iter().zip(&ends) {
        let line = user_style(None, rel.field(RelField::LineColor), None);
        let (start, end) = (rel.kind.start_arrow(), rel.kind.end_arrow());
        let reverse = a != b && pairs.get(&b).is_some_and(|targets| targets.contains(&a));
        let ends = Ends {
            boxes: &laid.boxes,
            a,
            b,
            start_trim: if start { trim } else { 0.0 },
            end_trim: if end { trim } else { 0.0 },
        };
        let shift = if reverse { PAIR_SHIFT } else { 0.0 };
        let r = route::route(&ends, shift, careful.then_some(nodes.as_slice()), &mut lanes);
        bounds.grow(js::min(r.x0, r.x1), js::min(r.y0, r.y1), js::max(r.x0, r.x1), js::max(r.y0, r.y1));
        for (arrow, tip) in [(start, r.path.start), (end, r.path.end)] {
            if arrow && careful {
                taken.extend([tip.x - ARROW, tip.y - ARROW, tip.x + ARROW, tip.y + ARROW]);
            }
        }
        let over = over_fills(&r.path, &fills, line.line.as_ref());
        views.push(RelView { rel, route: r, line, over, placed: None });
    }

    // A boundary's heading goes at the left, the middle or the right of its top edge: the first
    // of them no line crosses.
    for &g in &laid.order {
        let group = &laid.groups[g];
        if !careful || group.head_w == 0.0 {
            continue;
        }
        let b = laid.boxes[group.at];
        let top = b.y + HEAD_TOP;
        let bottom = b.y + group.head - LANE;
        let mut fewest = usize::MAX;
        let mut head_x = group.head_x;
        for x in [HEAD_LEFT, (b.w - group.head_w) / 2.0, b.w - HEAD_LEFT - group.head_w] {
            let (x0, x1) = (b.x + x - 4.0, b.x + x + group.head_w + 4.0);
            let count = views.iter().filter(|view| crosses(&view.route.points, x0, top, x1, bottom)).count();
            if count < fewest {
                fewest = count;
                head_x = x;
            }
            if count == 0 {
                break;
            }
        }
        taken.extend([b.x + head_x - 4.0, top, b.x + head_x + group.head_w + 4.0, bottom]);
        laid.groups[g].head_x = head_x;
    }

    for (index, view) in views.iter_mut().enumerate() {
        let (rel, r) = (view.rel, &view.route);
        let text = user_style(None, None, rel.field(RelField::TextColor));
        let mut words = JsText::default();
        if model.c4_type == C4Kind::Dynamic {
            words.push_str(&format!("{}: ", index + 1));
        }
        words.push_units(&rel.label);
        let label = layout_label(&words, false, cx.measure, small, r.room, Emphasis::default());
        let italic = Emphasis { italic: true, ..Emphasis::default() };
        let techn = layout_label(&bracketed(rel.field(RelField::Techn)), false, cx.measure, small, r.room, italic);
        let lw = js::max(label.width, techn.width) + LABEL_PAD;
        let lh = label.height + techn.height;
        if lh == 0.0 {
            continue;
        }

        // The label sits on the line, or just beside it when another relation runs the opposite way.
        let beside = r.nx.abs() * (lw / 2.0) + r.ny.abs() * (lh / 2.0) + 2.0;
        let aside = if truthy(r.lean) { js::max(0.0, js::min(r.lean, beside - 14.0)) } else { beside };
        // Manual offsets are tuned to the original spacing. Reposition compact labels beside
        // their routed arrows so an offset cannot leave them floating outside a boundary.
        let mut dx = r.nx * aside + if tight { 0.0 } else { offset(rel.offset_x) };
        let mut dy = r.ny * aside + if tight { 0.0 } else { offset(rel.offset_y) };
        let mut at = 0.5;
        if careful {
            // Slide along the line to the spot that covers least. Offsets from the diagram are
            // kept unless a spot on the line itself covers less than any they lead to: they are
            // often tuned for Mermaid's spacing.
            let mut least = f64::INFINITY;
            let moved = dx != r.nx * aside || dy != r.ny * aside;
            let passes = if moved { 2 } else { 1 };
            let mut pass = 0;
            while pass < passes && least > 0.0 {
                let px = if pass > 0 { r.nx * aside } else { dx };
                let py = if pass > 0 { r.ny * aside } else { dy };
                for t in SLIDE {
                    let cx = r.x0 + (r.x1 - r.x0) * t + px;
                    let cy = r.y0 + (r.y1 - r.y0) * t + py;
                    let area = covered(&taken, cx - lw / 2.0, cy - lh / 2.0, cx + lw / 2.0, cy + lh / 2.0);
                    if area < least {
                        least = area;
                        at = t;
                        dx = px;
                        dy = py;
                    }
                    if area == 0.0 {
                        break;
                    }
                }
                pass += 1;
            }
        }
        let cx = r.x0 + (r.x1 - r.x0) * at + dx;
        let top = r.y0 + (r.y1 - r.y0) * at + dy - lh / 2.0;
        if careful {
            taken.extend([cx - lw / 2.0, top, cx + lw / 2.0, top + lh]);
        }
        bounds.grow(cx - lw / 2.0, top, cx + lw / 2.0, top + lh);
        view.placed = Some(Placed { label, techn, cx, top, lw, lh, text });
    }

    let title_room = if tight { fit::title_room(options) } else { 4000.0 };
    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, cx.measure, size, title_room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let Bounds { min_x, min_y, max_x, max_y } = bounds;
    let inner = js::max(max_x - min_x, title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;

    let accessible = Accessible { title: None, description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width, height }, accessible);
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });

    let shape_parts =
        ranked_parts(&mut out, model.shapes.iter().map(|shape| (&shape.element.alias, Entity::node as Make)));
    let boundary_parts = ranked_parts(
        &mut out,
        model.boundaries.iter().map(|boundary| (&boundary.element.alias, Entity::group as Make)),
    );
    let part_of = |found: Found| match found {
        Found::Shape(i) => shape_parts[i],
        Found::Boundary(i) => boundary_parts[i],
    };
    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let mut rel_parts = Vec::with_capacity(views.len());
    for (rel, &((_, from), (_, to))) in model.rels.iter().zip(&ends) {
        let name =
            EntityName::Between { from: rel.from.to_vec(), to: rel.to.to_vec(), kind: rel.kind.name().to_owned() };
        let duplicate = seen.entry(name.clone()).or_insert(0);
        let entity = Entity::edge(&rel.id(), name, *duplicate).tooltip(rel.filled(RelField::Descr));
        *duplicate += 1;
        let part = out.part(entity);
        out.relation(part, part_of(from), part_of(to));
        rel_parts.push(part);
    }

    let link_rel = (!options.resources.link_rel.is_empty()).then(|| options.resources.link_rel.clone());
    let policy = &options.resources;
    let scene = Drawing { model, laid: &laid, small, link_rel: &link_rel, policy };
    out.layer(LayerSpec::group().translate(Point { x: ox, y: oy }).keep_empty(), |out| {
        out.layer(LayerSpec::class("pele-clusters"), |out| {
            for &g in &laid.order[1..] {
                scene.boundary(out, g, boundary_parts[g]);
            }
        });
        out.layer(casings_layer(), |out| {
            for (view, &part) in views.iter().zip(&rel_parts).filter(|(view, _)| view.over == Over::Cased) {
                let kind = view.rel.kind;
                let markers = [kind.start_arrow(), kind.end_arrow()]
                    .map(|arrow| if arrow { Marker::Point } else { Marker::None });
                draw_casing(out, part, &view.route.path, markers, view.line.line.as_ref(), false);
            }
        });
        let edges =
            LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
        out.layer(edges, |out| {
            for (view, &part) in views.iter().zip(&rel_parts) {
                scene.relation(out, view, part);
            }
        });
        out.layer(LayerSpec::class("pele-nodes"), |out| {
            for &g in &laid.order {
                for node in &laid.groups[g].nodes {
                    scene.shape(out, node, shape_parts[node.shape]);
                }
            }
        });
        out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(small)), |out| {
            for (view, &part) in views.iter().zip(&rel_parts) {
                scene.label(out, view, part);
            }
        });
    });
    Ok((out.finish()?, lanes.squeezed))
}

type Make = fn(&JsText) -> Entity;

/// One part per shape or boundary. Aliases a source repeats, such as the empty one, are told
/// apart by their place among those that share it.
fn ranked_parts<'a>(out: &mut SceneBuilder, items: impl Iterator<Item = (&'a JsText, Make)>) -> Vec<PartId> {
    let mut seen: HashMap<&JsText, u32> = HashMap::new();
    items
        .map(|(alias, make)| {
            let duplicate = seen.entry(alias).or_insert(0);
            let part = out.part(make(alias).ranked(*duplicate));
            *duplicate += 1;
            part
        })
        .collect()
}

/// What every element of one drawing is drawn with.
struct Drawing<'a> {
    model: &'a C4Model,
    laid: &'a Layout,
    small: f64,
    link_rel: &'a Option<String>,
    policy: &'a crate::ResourcePolicy,
}

impl Drawing<'_> {
    /// A boundary: its frame, dashed unless it is a deployment node, and its heading at the
    /// left of its top edge.
    fn boundary(&self, out: &mut SceneBuilder, g: usize, part: PartId) {
        let Boundary { element, node_type } = &self.model.boundaries[g];
        let group = &self.laid.groups[g];
        let b = self.laid.boxes[group.at];
        let style = user_style(
            element.field(ElementField::BgColor),
            element.field(ElementField::BorderColor),
            element.field(ElementField::FontColor),
        );
        let class = if node_type.is_some() { "pele-cluster pele-c4-node" } else { "pele-cluster pele-c4-boundary" };
        let small = self.small;
        out.layer(LayerSpec::class(class).part(part, Facet::Shape).data_id(), |out| {
            let frame = out
                .element(part, Facet::Shape)
                .rect([Number::num(b.x), Number::num(b.y)], [Number::num(b.w), Number::num(b.h)], None)
                .fill(Paint::None)
                .stroke(Paint::Role(Role::Border));
            let frame = if node_type.is_some() { frame } else { frame.dash(&[Number::raw(6.0), Number::raw(4.0)]) };
            frame.style(Some(Style::rounded(style.shape.as_ref()))).finish();
            let text = style.text.as_ref();
            let ink = ink_on(style.shape.as_ref(), text);
            let colour = ink.map(Paint::Role);
            let muted = Some(Paint::Role(ink.unwrap_or(Role::Muted)));
            let href = link_href(element.field(ElementField::Link), self.policy);
            linked(out, part, href, self.link_rel, |out| {
                let x = b.x + group.head_x;
                let mut y = b.y + HEAD_TOP;
                let [name, kind, descr] = &group.labels[..] else {
                    unreachable!("a drawn boundary has a name, a type and a description")
                };
                let look = LabelLook::class("pele-cluster-label").weight(TextWeight::Heading).fill(colour).style(text);
                out.left_label(part, Facet::Title, name, x, y, look);
                y += name.height;
                let look = LabelLook::class("pele-c4-type").font_size(Number::raw(small)).fill(muted).style(text);
                out.left_label(part, Facet::Title, kind, x, y, look);
                y += kind.height;
                let look = LabelLook::class("pele-c4-descr").font_size(Number::raw(small)).fill(muted).style(text);
                out.left_label(part, Facet::Title, descr, x, y, look);
            });
        });
    }

    /// A shape in the shared shape its form names, with its type, name, technology and
    /// description stacked in the middle of it.
    fn shape(&self, out: &mut SceneBuilder, node: &layout::Node, part: PartId) {
        let C4Shape { element, kind } = &self.model.shapes[node.shape];
        let b = self.laid.boxes[node.at];
        let external = kind.external;
        let style = user_style(
            element.field(ElementField::BgColor),
            element.field(ElementField::BorderColor),
            element.field(ElementField::FontColor),
        );
        let fill = if external { Role::Background } else { Role::Surface };
        let mut a = ShapeLook::new(fill, Role::Border, style.shape.as_ref());
        let mut line = LineLook::new(Role::Border, style.line.as_ref());
        if external {
            let dashes = [Number::raw(4.0), Number::raw(3.0)];
            a = a.dashed(&dashes);
            line = line.dashed(&dashes);
        }
        let (w, h, form) = (b.w, b.h, b.form);
        let dx = if form == Form::HCyl { -queue_rx(h) / 2.0 } else { 0.0 };
        let dy = match form {
            Form::Person => route::HEAD / 2.0,
            Form::Cyl => cyl_ry(w) / 2.0,
            _ => 0.0,
        };
        let text = style.text.as_ref();
        let ink = ink_on(style.shape.as_ref(), text);
        let colour = ink.map(Paint::Role);
        let muted = Some(Paint::Role(ink.unwrap_or(Role::Muted)));
        let class = format!(
            "pele-node pele-shape-{} pele-c4-{}{}",
            form.name(),
            kind.name(),
            if external { " pele-c4-external" } else { "" }
        );
        let centre = Point { x: b.x + w / 2.0, y: b.y + h / 2.0 };
        let small = self.small;
        let href = link_href(element.field(ElementField::Link), self.policy);
        out.layer(LayerSpec::class(&class).part(part, Facet::Shape).data_id().translate(centre).keep_empty(), |out| {
            linked(out, part, href, self.link_rel, |out| {
                draw_shape(out, part, form_shape(form), w, h, &a, &line);
                let [kind, name, techn, descr] = &node.labels;
                let mut y = dy - node.text_h / 2.0;
                let look = LabelLook::class("pele-c4-type").font_size(Number::raw(small - 1.0)).fill(muted).style(text);
                out.label(part, Facet::Stereotype, kind, Point { x: dx, y: y + kind.height / 2.0 }, look);
                y += kind.height;
                let look = LabelLook::class("pele-label").weight(TextWeight::Heading).fill(colour).style(text);
                out.label(part, Facet::Label, name, Point { x: dx, y: y + name.height / 2.0 }, look);
                y += name.height;
                let look = LabelLook::class("pele-c4-techn").font_size(Number::raw(small)).fill(muted).style(text);
                out.label(part, Facet::Label, techn, Point { x: dx, y: y + techn.height / 2.0 }, look);
                y += techn.height + 4.0;
                let look = LabelLook::class("pele-c4-descr").font_size(Number::raw(small)).fill(colour).style(text);
                out.label(part, Facet::Label, descr, Point { x: dx, y: y + descr.height / 2.0 }, look);
            });
        });
    }

    /// A relation's line, with an arrowhead at each end its kind draws one at.
    fn relation(&self, out: &mut SceneBuilder, view: &RelView<'_>, part: PartId) {
        let mut group = LayerSpec::class("pele-edge").part(part, Facet::Shape).data_id();
        if let Over::Ink(ink) = view.over {
            group = group.stroke(Paint::Role(ink));
        }
        out.layer(group.style(view.line.line.clone()), |out| {
            out.element(part, Facet::Shape).path(view.route.path.d.clone()).finish();
            let kind = view.rel.kind;
            if !kind.start_arrow() && !kind.end_arrow() {
                return;
            }
            let colour = match (view.over, &view.line.stroke) {
                (Over::Ink(ink), _) => Paint::Role(ink),
                (_, Some(stroke)) => out.colour(stroke, Role::Line),
                (_, None) => Paint::Role(Role::Line),
            };
            if kind.start_arrow() {
                draw_marker(
                    out,
                    part,
                    Facet::StartMarker,
                    Marker::Point,
                    view.route.path.start,
                    MarkerUse::Edge(colour),
                );
            }
            if kind.end_arrow() {
                draw_marker(out, part, Facet::EndMarker, Marker::Point, view.route.path.end, MarkerUse::Edge(colour));
            }
        });
    }

    /// A relation's label and technology on a patch of the background, with its description as
    /// a tooltip.
    fn label(&self, out: &mut SceneBuilder, view: &RelView<'_>, part: PartId) {
        let Some(placed) = &view.placed else {
            return;
        };
        let Placed { label, techn, cx, top, lw, lh, text } = placed;
        let (cx, top) = (*cx, *top);
        let href = link_href(view.rel.field(RelField::Link), self.policy);
        out.layer(LayerSpec::class("pele-edge-label").part(part, Facet::Label).data_id(), |out| {
            if view.rel.filled(RelField::Descr).is_some() {
                out.tooltip(part);
            }
            out.element(part, Facet::Label)
                .rect(
                    [Number::num(cx - lw / 2.0), Number::num(top)],
                    [Number::num(*lw), Number::num(*lh)],
                    Some(Number::raw(3.0)),
                )
                .fill(Paint::Role(Role::Background))
                .finish();
            let text = text.text.as_ref();
            linked(out, part, href, self.link_rel, |out| {
                let at = Point { x: cx, y: top + label.height / 2.0 };
                out.label(part, Facet::Label, label, at, LabelLook::default().style(text));
                let at = Point { x: cx, y: top + label.height + techn.height / 2.0 };
                let look = LabelLook::class("pele-c4-techn").fill(Some(Paint::Role(Role::Muted))).style(text);
                out.label(part, Facet::Label, techn, at, look);
            });
        });
    }
}

pub(crate) struct C4;

impl Diagram for C4 {
    type Model = C4Model;

    fn parse(cx: &ParseContext<'_>) -> Result<C4Model, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderC4`: drawn with room to spare, or tight where that does not fit the host's
    /// width. A tight drawing that leaves some relation without a lane of its own is tried again
    /// with wider gaps between its columns, and the one that leaves fewest wins.
    fn scene(model: &C4Model, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        fit::tighten(cx.options, |tight| {
            if !tight {
                return draw(model, cx, false, NARROW_GAP).map(|(scene, _)| scene);
            }
            let (mut best, mut squeezed) = draw(model, cx, true, NARROW_GAP)?;
            let mut gap = NARROW_GAP + WIDEN;
            while squeezed > 0 && gap <= NARROW_GAP + 3.0 * WIDEN {
                let (wider, left) = draw(model, cx, true, gap)?;
                if left < squeezed {
                    best = wider;
                    squeezed = left;
                }
                gap += WIDEN;
            }
            Ok(best)
        })
    }
}
