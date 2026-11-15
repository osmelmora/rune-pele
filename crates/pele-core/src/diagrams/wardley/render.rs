//! Pele's `renderWardley` (`render.ts`): the map's frame, pipelines, links, trends, nodes and
//! marks, as a Scene.
//!
//! Pele writes the map into strings while it grows the bounds, and only then writes the title
//! and the translation that frames it. Here the map is drawn twice from the same numbers: once
//! into a scratch drawing to find the bounds, and once inside the framed group.

use super::model::{Flow, NodeClass, Strategy, WardleyModel};
use crate::catalog::DrawContext;
use crate::graph::edges::{Marker, MarkerUse, Tip, draw_marker};
use crate::input::{JsText, JsValue};
use crate::layout::num::hypot;
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Entity, EntityName, Facet, LabelLook, LayerSpec, Number, Paint, PartId, PathProgram, Point, Role, Scene,
    SceneBuilder, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, SQRT_2};

const DEFAULT_STAGES: [&str; 4] = ["Genesis", "Custom Built", "Product", "Commodity"];
// Mermaid's canvas has this much room around the plot on every side.
const CANVAS_MARGIN: f64 = 48.0;
const R: f64 = 6.0;
const SQUARE: f64 = 10.0;
const LABEL_OFFSET: f64 = 8.0;
const PIPELINE_PAD: f64 = 15.0;
const FORCE_WIDTH: f64 = 60.0;
const FORCE_HEIGHT: f64 = 30.0;
const FORCE_HEAD: f64 = 20.0;

fn section<'a>(cx: &'a DrawContext<'_>) -> Option<&'a crate::input::JsObject> {
    cx.config.get("wardley-beta")?.as_record()
}

/// Pele's `setting`: a number of the `wardley-beta` config between `min` and `max`.
fn setting(cx: &DrawContext<'_>, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    section(cx)
        .and_then(|section| section.get(key))
        .and_then(JsValue::as_number)
        .filter(|&value| value >= min && value <= max)
        .unwrap_or(fallback)
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

/// Pele's `dimension`: the size a source or the config gives the canvas, less its margins.
fn dimension(cx: &DrawContext<'_>, given: Option<f64>, key: &str, fallback: f64) -> f64 {
    let value = given.filter(|value| value.is_finite()).unwrap_or_else(|| setting(cx, key, fallback, 1.0, 1e6));
    clamp(value, 200.0, 10000.0) - 2.0 * CANVAS_MARGIN
}

pub(crate) fn scene(model: &WardleyModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let natural = dimension(cx, model.size.map(|size| size.0), "width", 900.0);
    fit::fit_width(cx.options, natural, 160.0, |plot_w, tight| draw(model, cx, plot_w, natural, tight))
}

/// What the drawing reaches past the plot, which the frame grows to hold.
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn grow(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        if x0 < self.min_x {
            self.min_x = x0;
        }
        if y0 < self.min_y {
            self.min_y = y0;
        }
        if x1 > self.max_x {
            self.max_x = x1;
        }
        if y1 > self.max_y {
            self.max_y = y1;
        }
    }
}

/// Where a node is drawn. A pipeline moves its parent onto its box.
#[derive(Clone, Copy)]
struct Placed {
    x: f64,
    y: f64,
}

/// One draw at a plot width: the numbers every part of the map is placed from.
struct Map<'a> {
    model: &'a WardleyModel,
    cx: &'a DrawContext<'a>,
    plot_w: f64,
    plot_h: f64,
    small: f64,
    tiny: f64,
    tight: bool,
}

fn raw(value: f64) -> Number {
    Number::raw(value)
}

fn num(value: f64) -> Number {
    Number::num(value)
}

fn line() -> Paint {
    Paint::Role(Role::Line)
}

impl Map<'_> {
    fn text_within(&self, raw: &[u16], size: f64, max: f64, style: Emphasis) -> Label {
        layout_label(raw, false, self.cx.measure, size, max, style)
    }

    /// Pele's `text`, with its room for a label of the map.
    fn text(&self, raw: &[u16], size: f64) -> Label {
        let room = if self.tight { js::max(64.0, js::min(120.0, self.plot_w / 2.0)) } else { 4000.0 };
        self.text_within(raw, size, room, Emphasis::default())
    }

    fn project_x(&self, value: f64) -> f64 {
        (value / 100.0) * self.plot_w
    }

    fn project_y(&self, value: f64) -> f64 {
        self.plot_h - (value / 100.0) * self.plot_h
    }

    fn anchor_label(&self, label: &JsText) -> Label {
        let room = if self.tight { js::max(64.0, self.plot_w / 2.0) } else { 4000.0 };
        self.text_within(label, self.small, room, Emphasis::BOLD)
    }

    /// Pele's `centered`: a label centred on a point, which the bounds grow to hold.
    #[allow(clippy::too_many_arguments, reason = "Pele's centered, plus the part it draws and the bounds")]
    fn centred(
        out: &mut SceneBuilder,
        bounds: &mut Bounds,
        part: PartId,
        facet: Facet,
        label: &Label,
        at: Point,
        look: LabelLook,
    ) {
        let (half_w, half_h) = (label.width / 2.0, label.height / 2.0);
        bounds.grow(at.x - half_w, at.y - half_h, at.x + half_w, at.y + half_h);
        out.label(part, facet, label, at, look);
    }

    /// Pele's `starting`: a label whose lines start at `x`, its first baseline at `y`.
    #[allow(clippy::too_many_arguments, reason = "Pele's starting, plus the part it draws and the bounds")]
    fn starting(
        out: &mut SceneBuilder,
        bounds: &mut Bounds,
        part: PartId,
        facet: Facet,
        label: &Label,
        x: f64,
        y: f64,
        look: LabelLook,
    ) {
        bounds.grow(x, y - label.size, x + label.width, y - label.size + label.height + 4.0);
        out.start_lines(part, facet, label, x, y, look);
    }

    /// Everything inside the framed group, in paint order, and the bounds it reaches.
    fn content(&self, out: &mut SceneBuilder) -> Bounds {
        let (plot_w, plot_h, small, tiny) = (self.plot_w, self.plot_h, self.small, self.tiny);
        let model = self.model;
        let mut bounds = Bounds { min_x: 0.0, min_y: 0.0, max_x: plot_w, max_y: plot_h };
        let diagram = out.part(Entity::diagram());

        let defaults: Vec<JsText> = DEFAULT_STAGES.iter().map(|&stage| JsText::from(stage)).collect();
        let (stages, boundaries) = match &model.axes {
            Some((stages, boundaries)) if !stages.is_empty() => (stages.as_slice(), Some(boundaries)),
            Some((_, boundaries)) => (defaults.as_slice(), Some(boundaries)),
            None => (defaults.as_slice(), None),
        };
        let custom = boundaries.is_some_and(|boundaries| boundaries.len() == stages.len());
        let count = stages.len() as f64;
        let mut spans = Vec::with_capacity(stages.len());
        let mut previous = 0.0;
        for index in 0..stages.len() {
            let start = if custom { previous } else { index as f64 / count };
            let end = match boundaries {
                Some(boundaries) if custom => clamp(boundaries[index], 0.0, 1.0),
                _ => (index as f64 + 1.0) / count,
            };
            previous = end;
            spans.push((start, end));
        }
        let border = Paint::Role(Role::Border);
        out.layer(LayerSpec::class("pele-wardley-frame").fill(Paint::None), |out| {
            for &(start, _) in spans.iter().skip(1) {
                let path = PathProgram::default().m(num(start * plot_w), num(0.0)).v(num(plot_h));
                let dash = [raw(4.0), raw(4.0)];
                out.decoration().class("pele-wardley-stage").path(path).stroke(border).dash(&dash).finish();
            }
            let grid = section(self.cx).and_then(|section| section.get("showGrid")) == Some(&JsValue::Bool(true));
            if grid {
                for i in 1..4 {
                    let i = f64::from(i);
                    let path = PathProgram::default()
                        .m(num((plot_w * i) / 4.0), num(0.0))
                        .v(num(plot_h))
                        .m(num(0.0), num((plot_h * i) / 4.0))
                        .h(num(plot_w));
                    let dash = [raw(2.0), raw(6.0)];
                    out.decoration().class("pele-wardley-grid").path(path).stroke(border).dash(&dash).finish();
                }
            }
            let axis = PathProgram::default().m(num(0.0), num(0.0)).v(num(plot_h)).h(num(plot_w));
            out.decoration().class("pele-wardley-axis").path(axis).stroke(line()).finish();
        });

        let mut stage_bottom = plot_h;
        for (stage, &(start, end)) in stages.iter().zip(&spans) {
            let room = js::max(48.0, (end - start).abs() * plot_w - 12.0);
            let label = self.text_within(stage, small, room, Emphasis::default());
            let at = Point { x: ((start + end) / 2.0) * plot_w, y: plot_h + 8.0 + label.height / 2.0 };
            let look =
                LabelLook::class("pele-wardley-stage-label").font_size(raw(small)).fill(Some(Paint::Role(Role::Muted)));
            Self::centred(out, &mut bounds, diagram, Facet::Legend, &label, at, look);
            stage_bottom = js::max(stage_bottom, plot_h + 8.0 + label.height);
        }

        let x_title = self.text(&JsText::from("Evolution"), small);
        let y_title = self.text(&JsText::from("Visibility"), small);
        let axis_look =
            || LabelLook::class("pele-wardley-axis-label").font_size(raw(small)).weight(TextWeight::Heading);
        let at = Point { x: plot_w / 2.0, y: stage_bottom + 6.0 + x_title.height / 2.0 };
        Self::centred(out, &mut bounds, diagram, Facet::Label, &x_title, at, axis_look());
        let middle = plot_h / 2.0;
        let turned = axis_look().rotate(raw(-90.0), [raw(-16.0), num(middle)]);
        out.label(diagram, Facet::Label, &y_title, Point { x: -16.0, y: middle }, turned);
        bounds.grow(-16.0 - y_title.height / 2.0, 0.0, 0.0, 0.0);
        let legend_top = stage_bottom + 6.0 + x_title.height + 10.0;

        let index: HashMap<&[u16], usize> =
            model.nodes.iter().enumerate().map(|(i, node)| (node.id.as_ref(), i)).collect();
        let mut positions: Vec<Placed> =
            model.nodes.iter().map(|node| Placed { x: self.project_x(node.x), y: self.project_y(node.y) }).collect();

        // Pipelines: a box around the components, with the parent as a square on its top edge.
        let mut members: HashMap<&[u16], HashSet<&[u16]>> = HashMap::new();
        for pipeline in &model.pipelines {
            members.insert(&pipeline.node_id, pipeline.component_ids.iter().map(|id| id.as_ref()).collect());
            let mut parts: Vec<Placed> =
                pipeline.component_ids.iter().filter_map(|id| index.get(id.as_ref())).map(|&i| positions[i]).collect();
            if parts.is_empty() {
                continue;
            }
            parts.sort_by(|a, b| crate::layout::num::by_difference(a.x - b.x));
            let left = parts[0].x;
            let right = parts[parts.len() - 1].x;
            let y = parts[0].y;
            let top = y - 2.0 * R;
            if let Some(&parent) = index.get(pipeline.node_id.as_ref()) {
                positions[parent] = Placed { x: (left + right) / 2.0, y: top - SQUARE / 6.0 };
            }
            bounds.grow(left - PIPELINE_PAD, top, right + PIPELINE_PAD, top + 4.0 * R);
            let part = out.part(Entity::group(&pipeline.node_id));
            out.layer(
                LayerSpec::class("pele-wardley-pipeline").part(part, Facet::Shape).data_id().keep_empty(),
                |out| {
                    out.element(part, Facet::Shape)
                        .rect(
                            [num(left - PIPELINE_PAD), num(top)],
                            [num(right - left + 2.0 * PIPELINE_PAD), num(4.0 * R)],
                            None,
                        )
                        .fill(Paint::Role(Role::Surface))
                        .stroke(border)
                        .style(Some(Style::rounded(None)))
                        .finish();
                    if parts.len() > 1 {
                        let path = PathProgram::default().m(num(left), num(y)).h(num(right));
                        out.element(part, Facet::Shape).path(path).stroke(line()).dash(&[raw(4.0), raw(4.0)]).finish();
                    }
                },
            );
        }

        // How far from a node's centre a line to it stops.
        let mut anchor_labels: HashMap<usize, Label> = HashMap::new();
        let mut reach = |at: usize, ux: f64, uy: f64| -> f64 {
            let node = &model.nodes[at];
            if node.class == NodeClass::Anchor {
                let label = anchor_labels.entry(at).or_insert_with(|| self.anchor_label(&node.label));
                return 1.0 / hypot(ux / (label.width / 2.0 + 5.0), uy / (small * 0.8));
            }
            if node.is_pipeline_parent {
                return SQUARE / SQRT_2;
            }
            if node.strategy.is_some() { 2.0 * R } else { R }
        };

        // Quadrants around a node that a link runs through: 1 up and right, 2 down and right,
        // 4 up and left, 8 down and left.
        let mut taken: HashMap<usize, u8> = HashMap::new();
        let mut occupy = |at: usize, ux: f64, uy: f64| {
            if ux.abs() < 0.25 || uy.abs() < 0.25 {
                return;
            }
            let quadrant = if ux > 0.0 {
                if uy < 0.0 { 1 } else { 2 }
            } else if uy < 0.0 {
                4
            } else {
                8
            };
            *taken.entry(at).or_insert(0) |= quadrant;
        };

        let trim = Marker::Point.trim();
        let mut ranks: HashMap<EntityName, u32> = HashMap::new();
        out.layer(LayerSpec::class("pele-edges").fill(Paint::None), |out| {
            for link in &model.links {
                let (Some(&from), Some(&to)) = (index.get(link.source.as_ref()), index.get(link.target.as_ref()))
                else {
                    continue;
                };
                if from == to {
                    continue;
                }
                // A pipeline component belongs to its parent already.
                if members.get(link.target.as_ref()).is_some_and(|set| set.contains(link.source.as_ref())) {
                    continue;
                }
                let (a, b) = (positions[from], positions[to]);
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let distance = hypot(dx, dy);
                if distance < 0.01 {
                    continue;
                }
                let ux = dx / distance;
                let uy = dy / distance;
                let start = reach(from, ux, uy);
                let end = reach(to, ux, uy);
                if start + end >= distance {
                    continue;
                }
                occupy(from, ux, uy);
                occupy(to, -ux, -uy);
                let mut x1 = a.x + ux * start;
                let mut y1 = a.y + uy * start;
                let mut x2 = b.x - ux * end;
                let mut y2 = b.y - uy * end;
                let forward = matches!(link.flow, Some(Flow::Forward | Flow::Bidirectional));
                let backward = matches!(link.flow, Some(Flow::Backward | Flow::Bidirectional));
                let end_tip = Tip { x: x2, y: y2, dx: ux, dy: uy };
                let start_tip = Tip { x: x1, y: y1, dx: -ux, dy: -uy };
                if forward {
                    x2 -= ux * trim;
                    y2 -= uy * trim;
                }
                if backward {
                    x1 += ux * trim;
                    y1 += uy * trim;
                }
                let kind = match link.flow {
                    Some(flow) => flow.name(),
                    None => "plain",
                };
                let name = EntityName::Between {
                    from: link.source.to_vec(),
                    to: link.target.to_vec(),
                    kind: if link.dashed { format!("{kind} dashed") } else { kind.to_owned() },
                };
                let rank = ranks.entry(name.clone()).or_insert(0);
                let duplicate = std::mem::replace(rank, *rank + 1);
                let mut written = link.source.clone();
                written.push_str("-");
                written.push_units(&link.target);
                let part = out.part(Entity::edge(&written, name, duplicate));
                let (source, target) = (out.part(Entity::node(&link.source)), out.part(Entity::node(&link.target)));
                out.relation(part, source, target);
                let class = if link.dashed { "pele-edge pele-wardley-dashed" } else { "pele-edge" };
                out.layer(LayerSpec::class(class).part(part, Facet::Shape).data_id().keep_empty(), |out| {
                    let path = PathProgram::default().m(num(x1), num(y1)).l(num(x2), num(y2));
                    let element = out.element(part, Facet::Shape).path(path).stroke(line());
                    if link.dashed { element.dash(&[raw(6.0), raw(6.0)]) } else { element }.finish();
                    if forward {
                        draw_marker(out, part, Facet::EndMarker, Marker::Point, end_tip, MarkerUse::Edge(line()));
                    }
                    if backward {
                        draw_marker(out, part, Facet::StartMarker, Marker::Point, start_tip, MarkerUse::Edge(line()));
                    }
                    if let Some(text) = link.label.as_ref().filter(|text| !text.is_empty()) {
                        let label = self.text_within(text, tiny + 1.0, 200.0, Emphasis::default());
                        // Beside the middle of the line, on its upper side.
                        let side = if ux >= 0.0 { 1.0 } else { -1.0 };
                        let px = uy * side;
                        let py = -ux * side;
                        let away = 5.0 + (px.abs() * label.width) / 2.0 + (py.abs() * label.height) / 2.0;
                        let at = Point { x: (a.x + b.x) / 2.0 + px * away, y: (a.y + b.y) / 2.0 + py * away };
                        let look = LabelLook::class("pele-edge-label")
                            .font_size(raw(tiny + 1.0))
                            .fill(Some(Paint::Role(Role::Muted)));
                        Self::centred(out, &mut bounds, part, Facet::Label, &label, at, look);
                    }
                });
            }
        });

        // Evolution: a dashed arrow to where the component is heading, drawn as an open circle.
        let accent = Paint::Role(Role::Accent);
        out.layer(LayerSpec::class("pele-wardley-trends").fill(Paint::None), |out| {
            for trend in &model.trends {
                let Some(&at) = index.get(trend.node_id.as_ref()) else { continue };
                let origin = positions[at];
                let tx = self.project_x(trend.target_x);
                let ty = self.project_y(trend.target_y);
                let dx = tx - origin.x;
                let dy = ty - origin.y;
                let distance = hypot(dx, dy);
                let start = if distance > 0.0 { reach(at, dx / distance, dy / distance) } else { 0.0 };
                let name = EntityName::Between {
                    from: trend.node_id.to_vec(),
                    to: trend.node_id.to_vec(),
                    kind: "evolve".to_owned(),
                };
                let part = out.part(Entity::edge(&trend.node_id, name, 0));
                let node = out.part(Entity::node(&trend.node_id));
                out.relation(part, node, node);
                out.layer(
                    LayerSpec::class("pele-wardley-trend").part(part, Facet::Shape).data_id().keep_empty(),
                    |out| {
                        if distance > start + R + 4.0 {
                            let ux = dx / distance;
                            let uy = dy / distance;
                            let ex = tx - ux * (R + 2.0);
                            let ey = ty - uy * (R + 2.0);
                            let path = PathProgram::default()
                                .m(num(origin.x + ux * start), num(origin.y + uy * start))
                                .l(num(ex - ux * trim), num(ey - uy * trim));
                            out.element(part, Facet::Shape)
                                .path(path)
                                .stroke(accent)
                                .dash(&[raw(4.0), raw(4.0)])
                                .finish();
                            let tip = Tip { x: ex, y: ey, dx: ux, dy: uy };
                            draw_marker(out, part, Facet::EndMarker, Marker::Point, tip, MarkerUse::Edge(accent));
                        }
                        bounds.grow(tx - R, ty - R, tx + R, ty + R);
                        out.element(part, Facet::Shape)
                            .circle_at([Some(num(tx)), Some(num(ty))], raw(R))
                            .fill(Paint::Role(Role::Background))
                            .stroke(accent)
                            .finish();
                    },
                );
            }
        });

        let tri = 1.2 * R;
        let dot = 0.7 * R;
        let background = Paint::Role(Role::Background);
        for (at, node) in model.nodes.iter().enumerate() {
            let Placed { x, y } = positions[at];
            let anchor = node.class == NodeClass::Anchor;
            let strategy = node.strategy;
            let class = if anchor {
                "pele-node pele-wardley-anchor"
            } else if node.is_pipeline_parent {
                "pele-node pele-wardley-pipeline-parent"
            } else if node.in_pipeline {
                "pele-node pele-wardley-pipeline-component"
            } else {
                "pele-node pele-wardley-component"
            };
            let part = out.part(Entity::node(&node.id));
            out.layer(LayerSpec::class(class).part(part, Facet::Shape).data_id().keep_empty(), |out| {
                if let Some(strategy) = strategy {
                    let fill = match strategy {
                        Strategy::Outsource => Role::Muted,
                        Strategy::Buy => Role::SurfaceAlt,
                        Strategy::Build => Role::Surface,
                        Strategy::Market => Role::Background,
                    };
                    out.element(part, Facet::Shape)
                        .class(&format!("pele-wardley-{}", strategy.name()))
                        .circle_at([Some(num(x)), Some(num(y))], raw(2.0 * R))
                        .fill(Paint::Role(fill))
                        .stroke(line())
                        .finish();
                }
                if strategy == Some(Strategy::Market) {
                    let bx = tri * js::cos(PI / 6.0);
                    let by = y + tri * js::sin(PI / 6.0);
                    let path =
                        PathProgram::default().m(num(x), num(y - tri)).l(num(x - bx), num(by)).h(num(x + bx)).z();
                    out.element(part, Facet::Shape).path(path).fill(Paint::None).stroke(line()).finish();
                    for (cx, cy) in [(x, y - tri), (x - bx, by), (x + bx, by)] {
                        out.element(part, Facet::Shape)
                            .circle_at([Some(num(cx)), Some(num(cy))], num(dot))
                            .fill(background)
                            .stroke(line())
                            .finish();
                    }
                } else if node.is_pipeline_parent {
                    out.element(part, Facet::Shape)
                        .rect([num(x - SQUARE / 2.0), num(y - SQUARE / 2.0)], [raw(SQUARE), raw(SQUARE)], None)
                        .fill(background)
                        .stroke(line())
                        .finish();
                } else if !anchor {
                    out.element(part, Facet::Shape)
                        .circle_at([Some(num(x)), Some(num(y))], raw(R))
                        .fill(background)
                        .stroke(line())
                        .finish();
                }
                if node.inertia == Some(true) {
                    let side = if node.is_pipeline_parent { SQUARE / 2.0 } else { R };
                    let offset = side + 10.0 + if strategy.is_some() { R + 4.0 } else { 0.0 };
                    out.element(part, Facet::Shape)
                        .class("pele-wardley-inertia")
                        .rect([num(x + offset), num(y - R)], [raw(4.0), raw(2.0 * R)], None)
                        .fill(line())
                        .finish();
                }
                let ox = node.label_offset_x.map(|offset| clamp(offset, -5000.0, 5000.0));
                let oy = node.label_offset_y.map(|offset| clamp(offset, -5000.0, 5000.0));
                if anchor {
                    let label = anchor_labels.get(&at).cloned().unwrap_or_else(|| self.anchor_label(&node.label));
                    let look = LabelLook::class("pele-label").font_size(raw(small)).weight(TextWeight::Heading);
                    let at = Point { x: x + ox.unwrap_or(0.0), y: y + oy.unwrap_or(-3.0) };
                    Self::centred(out, &mut bounds, part, Facet::Label, &label, at, look);
                } else {
                    let label = self.text(&node.label, small);
                    let look = || LabelLook::class("pele-label").font_size(raw(small));
                    let lift = LABEL_OFFSET + if strategy.is_some() { 10.0 } else { 0.0 };
                    if ox.is_some() || oy.is_some() {
                        let (sx, sy) = (x + ox.unwrap_or(lift), y + oy.unwrap_or(-lift));
                        Self::starting(out, &mut bounds, part, Facet::Label, &label, sx, sy, look());
                    } else if node.in_pipeline {
                        let at = Point { x, y: y + 2.0 * R + 4.0 + label.height / 2.0 };
                        Self::centred(out, &mut bounds, part, Facet::Label, &label, at, look());
                    } else {
                        // The label goes up and to the right unless a link leaves that way; then
                        // the next free corner.
                        let busy = taken.get(&at).copied().unwrap_or(0);
                        let corner = [1, 2, 4, 8].into_iter().find(|&corner| busy & corner == 0).unwrap_or(1);
                        let right = corner == 1 || corner == 2;
                        let up = corner == 1 || corner == 4;
                        let sx = if right { x + lift } else { x - lift - label.width };
                        let sy = if up { y - lift } else { y + lift + small * 0.75 };
                        Self::starting(out, &mut bounds, part, Facet::Label, &label, sx, sy, look());
                    }
                }
                bounds.grow(x - 2.0 * R, y - 2.0 * R, x + 2.0 * R + 20.0, y + 2.0 * R);
            });
        }

        let mut notes_ranked: HashMap<JsText, u32> = HashMap::new();
        let mut note_part = |out: &mut SceneBuilder, name: &JsText| {
            let rank = notes_ranked.entry(name.clone()).or_insert(0);
            let duplicate = std::mem::replace(rank, *rank + 1);
            out.part(Entity::note(name).ranked(duplicate))
        };
        for annotation in &model.annotations {
            let number = JsText::from(js::to_string(annotation.number).as_str());
            let label = self.text_within(&number, tiny, 4000.0, Emphasis::BOLD);
            let part = note_part(out, &number);
            out.layer(
                LayerSpec::class("pele-wardley-annotation").part(part, Facet::Shape).data_id().keep_empty(),
                |out| {
                    let x = self.project_x(annotation.at.0);
                    let y = self.project_y(annotation.at.1);
                    let r = js::max(10.0, label.width / 2.0 + 4.0);
                    bounds.grow(x - r, y - r, x + r, y + r);
                    out.element(part, Facet::Shape)
                        .circle_at([Some(num(x)), Some(num(y))], num(r))
                        .fill(background)
                        .stroke(line())
                        .finish();
                    let look = LabelLook::default().font_size(raw(tiny)).weight(TextWeight::Heading);
                    out.label(part, Facet::Label, &label, Point { x, y }, look);
                },
            );
        }
        let mut listed: Vec<_> = model.annotations.iter().filter(|annotation| !annotation.text.is_empty()).collect();
        listed.sort_by(|a, b| crate::layout::num::by_difference(a.number - b.number));
        if !listed.is_empty() {
            let room = if self.tight { js::max(80.0, plot_w - 20.0) } else { 320.0 };
            let lines: Vec<Label> = listed
                .iter()
                .map(|annotation| {
                    let mut text = JsText::from(js::to_string(annotation.number).as_str());
                    text.push_str(". ");
                    text.push_units(&annotation.text);
                    self.text_within(&text, small, room, Emphasis::default())
                })
                .collect();
            let mut width = 0.0;
            let mut height = 0.0;
            for line in &lines {
                width = js::max(width, line.width);
                height += line.height;
            }
            width += 20.0;
            height += 14.0;
            // Without a position the list goes under the map, where it covers nothing.
            let (bx, by) = match model.annotations_box {
                Some((x, y)) => (
                    clamp(self.project_x(x), 0.0, js::max(0.0, plot_w - width)),
                    clamp(self.project_y(y), 0.0, js::max(0.0, plot_h - height)),
                ),
                None => (0.0, legend_top),
            };
            bounds.grow(bx, by, bx + width, by + height);
            out.layer(LayerSpec::class("pele-wardley-annotations").part(diagram, Facet::Legend).keep_empty(), |out| {
                out.element(diagram, Facet::Legend)
                    .rect([num(bx), num(by)], [num(width), num(height)], None)
                    .fill(background)
                    .stroke(border)
                    .style(Some(Style::rounded(None)))
                    .finish();
                let mut y = by + 7.0;
                for line in &lines {
                    let baseline = y + line.line_height / 2.0 + small * 0.35;
                    out.start_lines(
                        diagram,
                        Facet::Legend,
                        line,
                        bx + 10.0,
                        baseline,
                        LabelLook::default().font_size(raw(small)),
                    );
                    y += line.height;
                }
            });
        }

        for note in &model.notes {
            let room = js::min(360.0, plot_w * 0.6);
            let label = self.text_within(&note.text, small, room, Emphasis::BOLD);
            let part = note_part(out, &note.text);
            let look = LabelLook::class("pele-wardley-note").font_size(raw(small)).weight(TextWeight::Heading);
            let (x, y) = (self.project_x(note.x), self.project_y(note.y));
            Self::starting(out, &mut bounds, part, Facet::Label, &label, x, y, look);
        }

        // Accelerators point right, deaccelerators left.
        let forces = model.accelerators.iter().map(|force| (force, true));
        for (force, forward) in forces.chain(model.deaccelerators.iter().map(|force| (force, false))) {
            let x = self.project_x(force.x);
            let y = self.project_y(force.y);
            let half = FORCE_HEIGHT / 2.0;
            let tip = if forward { x + FORCE_WIDTH } else { x };
            let tail = if forward { x } else { x + FORCE_WIDTH };
            let neck = if forward { x + FORCE_WIDTH - FORCE_HEAD } else { x + FORCE_HEAD };
            let path = PathProgram::default()
                .m(num(tail), num(y - half))
                .h(num(neck))
                .v(num(y - half - 8.0))
                .l(num(tip), num(y))
                .l(num(neck), num(y + half + 8.0))
                .v(num(y + half))
                .h(num(tail))
                .z();
            bounds.grow(x, y - half - 8.0, x + FORCE_WIDTH, y + half + 8.0);
            let class = if forward { "pele-wardley-accelerator" } else { "pele-wardley-deaccelerator" };
            let part = note_part(out, &force.text);
            out.layer(LayerSpec::class(class).part(part, Facet::Shape).data_id().keep_empty(), |out| {
                out.element(part, Facet::Shape).path(path).fill(Paint::Role(Role::Surface)).stroke(line()).finish();
                let label = self.text_within(&force.text, tiny + 1.0, 160.0, Emphasis::BOLD);
                let at = Point { x: x + FORCE_WIDTH / 2.0, y: y + half + 8.0 + 12.0 };
                let look = LabelLook::default().font_size(raw(tiny + 1.0)).weight(TextWeight::Heading);
                Self::centred(out, &mut bounds, part, Facet::Label, &label, at, look);
            });
        }
        bounds
    }
}

fn draw(
    model: &WardleyModel,
    cx: &DrawContext<'_>,
    plot_w: f64,
    natural_w: f64,
    tight: bool,
) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let pad = cx.options.padding;
    // A narrower map is not as much shorter, so that components keep some room between them.
    let plot_h = dimension(cx, model.size.map(|size| size.1), "height", 600.0) * js::max(0.75, plot_w / natural_w);
    let map = Map { model, cx, plot_w, plot_h, small: js::round(size * 0.8125), tiny: js::round(size * 0.6875), tight };
    let mut scratch = SceneBuilder::new(cx, Size { width: 0.0, height: 0.0 }, model.accessible.clone());
    let bounds = map.content(&mut scratch);

    let title_text = model.title.as_deref().unwrap_or_default();
    let room = if tight { js::max(80.0, plot_w) } else { 4000.0 };
    let title = layout_label(title_text, false, cx.measure, size, room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let left = js::min(bounds.min_x, plot_w / 2.0 - title.width / 2.0);
    let right = js::max(bounds.max_x, plot_w / 2.0 + title.width / 2.0);
    let tx = pad - left;
    let ty = pad + title_height - bounds.min_y;
    let total_width = (right - left + 2.0 * pad).ceil();
    let total_height = (bounds.max_y - bounds.min_y + title_height + 2.0 * pad).ceil();

    let mut out = SceneBuilder::new(cx, Size { width: total_width, height: total_height }, model.accessible.clone());
    let at = Point { x: tx + plot_w / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at });
    out.layer(LayerSpec::group().translate(Point { x: tx, y: ty }), |out| {
        map.content(out);
    });
    out.finish()
}
