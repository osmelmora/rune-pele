//! How a radar chart is drawn, ported from Pele's `radar/render.ts`: a graticule of circles or
//! polygons, a spoke per axis with its label, a closed curve or polygon per curve, and a legend
//! beside the chart, or under it where the chart does not fit the host's width.

use super::model::{Graticule, RadarModel};
use crate::catalog::DrawContext;
use crate::input::{JsObject, JsText, JsValue};
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, Entity, Facet, LabelLook, LayerSpec, LineJoin, Number, Paint, PartId, PathProgram, Point, Role, Scene,
    SceneBuilder, SeriesSlot, Size, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;
use std::f64::consts::PI;

const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const GAP: f64 = 24.0;

/// Pele's `option`: a number of the `radar` section at least `min`, capped at `max`, or `fallback`.
fn option(section: Option<&JsObject>, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match section.and_then(|section| section.get(key)) {
        Some(JsValue::Number(value)) if *value >= min => js::min(*value, max),
        _ => fallback,
    }
}

/// Pele's `relativeRadius`: how far from the centre a value lies, clipped to the scale.
fn relative_radius(value: f64, min: f64, max: f64, radius: f64) -> f64 {
    let clipped = js::min(js::max(value, min), max);
    let scaled = radius * (clipped - min);
    // Keep the original rounding unless multiplication overflows.
    if scaled.is_finite() { scaled / (max - min) } else { radius * ((clipped - min) / (max - min)) }
}

fn pair(point: [f64; 2]) -> [Number; 2] {
    [Number::num(point[0]), Number::num(point[1])]
}

/// Pele's `closedRoundCurve`: a closed Catmull-Rom spline through the points, as cubic Bézier
/// segments, with a space before each command after the first.
fn closed_round_curve(points: &[[f64; 2]], tension: f64) -> PathProgram {
    let n = points.len();
    let mut d = PathProgram::default().m(Number::num(points[0][0]), Number::num(points[0][1]));
    for i in 0..n {
        let p0 = points[(i + n - 1) % n];
        let p1 = points[i];
        let p2 = points[(i + 1) % n];
        let p3 = points[(i + 2) % n];
        let first = [p1[0] + (p2[0] - p0[0]) * tension, p1[1] + (p2[1] - p0[1]) * tension];
        let second = [p2[0] - (p3[0] - p1[0]) * tension, p2[1] - (p3[1] - p1[1]) * tension];
        d = d.space().c(pair(first), pair(second), pair(p2));
    }
    d.space().z()
}

fn polygon(points: &[[f64; 2]]) -> Vec<[Number; 2]> {
    points.iter().map(|&point| pair(point)).collect()
}

/// The parts a chart names: each axis and each curve, ranked among those with the same name.
fn parts(out: &mut SceneBuilder, model: &RadarModel) -> (Vec<PartId>, Vec<PartId>) {
    let ranked = |out: &mut SceneBuilder, names: &mut HashMap<JsText, u32>, entity: Entity, name: &JsText| {
        let rank = names.entry(name.clone()).or_insert(0);
        let part = out.part(entity.ranked(*rank));
        *rank += 1;
        part
    };
    let mut names = HashMap::new();
    let axes = model.axes.iter().map(|axis| ranked(out, &mut names, Entity::node(&axis.name), &axis.name)).collect();
    let mut names = HashMap::new();
    let curves = model
        .curves
        .iter()
        .map(|curve| ranked(out, &mut names, Entity::pie_section(&curve.name), &curve.name))
        .collect();
    (axes, curves)
}

/// Pele's `renderRadar`: the chart at its configured width, narrowed to fit the host's.
pub(super) fn scene(model: &RadarModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let section = cx.config.get("radar").and_then(JsValue::as_record);
    let natural = option(section, "width", 320.0, 1.0, 4000.0);
    let height = option(section, "height", 320.0, 1.0, 4000.0);
    fit::fit_width(cx.options, natural, 160.0, |width, tight| {
        draw(model, cx, section, width, (height * width) / natural, tight)
    })
}

/// `tight` is set when the chart does not fit the width it has with its legend beside it.
fn draw(
    model: &RadarModel,
    cx: &DrawContext<'_>,
    section: Option<&JsObject>,
    width: f64,
    height: f64,
    tight: bool,
) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let measurer = cx.measure;
    let pad = cx.options.padding_or_default();

    let axis_scale = option(section, "axisScaleFactor", 1.0, 0.0, 10.0);
    let label_factor = option(section, "axisLabelFactor", 1.05, 0.0, 10.0);
    let tension = option(section, "curveTension", 0.17, 0.0, 1.0);
    let radius = js::min(width, height) / 2.0;

    let (axes, curves) = (&model.axes, &model.curves);
    let options = &model.options;
    let count = axes.len();
    let unit: Vec<[f64; 2]> = (0..count)
        .map(|i| {
            let angle = (2.0 * PI * i as f64) / count as f64 - PI / 2.0;
            [js::cos(angle), js::sin(angle)]
        })
        .collect();

    let max = options.max.unwrap_or_else(|| {
        let mut max = f64::NEG_INFINITY;
        for value in curves.iter().flat_map(|curve| &curve.entries) {
            if *value > max {
                max = *value;
            }
        }
        max
    });

    // The chart is drawn around the origin. Margins reserve space; labels and the legend widen it.
    let mut x0 = -width / 2.0 - option(section, "marginLeft", 16.0, 0.0, 2000.0);
    let mut x1 = width / 2.0 + option(section, "marginRight", 16.0, 0.0, 2000.0);
    let mut y0 = -height / 2.0 - option(section, "marginTop", 16.0, 0.0, 2000.0);
    let mut y1 = height / 2.0 + option(section, "marginBottom", 16.0, 0.0, 2000.0);

    struct AxisLabel {
        label: Label,
        at: Point,
    }
    let labels: Vec<AxisLabel> = unit
        .iter()
        .zip(axes)
        .map(|(u, axis)| {
            let label = layout_label(&axis.label, false, measurer, small, 4000.0, Emphasis::default());
            let at = radius * label_factor + 6.0;
            // Each label hangs off its axis on the side away from the center.
            let side = |u: f64, extent: f64| {
                if u > 0.01 {
                    extent / 2.0
                } else if u < -0.01 {
                    -extent / 2.0
                } else {
                    0.0
                }
            };
            let cx = u[0] * at + side(u[0], label.width);
            let cy = u[1] * at + side(u[1], label.height);
            x0 = js::min(x0, cx - label.width / 2.0);
            x1 = js::max(x1, cx + label.width / 2.0);
            y0 = js::min(y0, cy - label.height / 2.0);
            y1 = js::max(y1, cy + label.height / 2.0);
            AxisLabel { label, at: Point { x: cx, y: cy } }
        })
        .collect();

    let legend: Vec<Label> = if options.show_legend {
        curves
            .iter()
            .map(|curve| layout_label(&curve.label, false, measurer, small, 4000.0, Emphasis::default()))
            .collect()
    } else {
        Vec::new()
    };
    let legend_width = legend.iter().fold(0.0, |widest, label| js::max(widest, label.width));
    let legend_height = legend.len() as f64 * ROW;
    let legend_x = if tight { -(SWATCH + 8.0 + legend_width) / 2.0 } else { x1 + GAP };
    let legend_top = if tight { y1 + GAP / 2.0 } else { -legend_height / 2.0 };
    if !legend.is_empty() && tight {
        x0 = js::min(x0, legend_x);
        x1 = js::max(x1, -legend_x);
        y1 = legend_top + legend_height;
    } else if !legend.is_empty() {
        x1 = legend_x + SWATCH + 8.0 + legend_width;
        y0 = js::min(y0, -legend_height / 2.0);
        y1 = js::max(y1, legend_height / 2.0);
    }

    let title = layout_label(
        model.title.as_deref().unwrap_or_default(),
        false,
        measurer,
        size,
        fit::title_room(cx.options),
        Emphasis::BOLD,
    );
    let title_height = if title.height > 0.0 { title.height + 8.0 } else { 0.0 };
    let body_width = js::max(x1 - x0, title.width);
    let total_width = (body_width + 2.0 * pad).ceil();
    let total_height = (y1 - y0 + title_height + 2.0 * pad).ceil();
    let origin = Point { x: pad + (body_width - (x1 - x0)) / 2.0 - x0, y: pad + title_height - y0 };

    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width: total_width, height: total_height }, accessible);
    out.title(&TitleLayout { at: Point { x: total_width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let (axis_parts, curve_parts) = parts(&mut out, model);
    let num = Number::num;

    out.layer(LayerSpec::group().translate(origin).keep_empty(), |out| {
        let grid = LayerSpec::class("pele-graticule").fill(Paint::None).stroke(Paint::Role(Role::SurfaceAlt));
        out.layer(grid, |out| {
            let mut i = 0.0;
            while i < options.ticks {
                let r = js::min(radius, (radius * (i + 1.0)) / options.ticks);
                match options.graticule {
                    Graticule::Circle => out.decoration().circle(num(r)).finish(),
                    Graticule::Polygon => {
                        let points: Vec<[f64; 2]> = unit.iter().map(|u| [u[0] * r, u[1] * r]).collect();
                        out.decoration().polygon(polygon(&points)).finish();
                    }
                }
                i += 1.0;
            }
        });

        if count > 0 {
            let reach = radius * axis_scale;
            let mut spokes = PathProgram::default();
            for u in &unit {
                spokes = spokes.m(num(0.0), num(0.0)).l(num(u[0] * reach), num(u[1] * reach));
            }
            out.decoration()
                .class("pele-axes")
                .path(spokes)
                .fill(Paint::None)
                .stroke(Paint::Role(Role::Border))
                .finish();
        }

        let shapes = LayerSpec::class("pele-curves")
            .fill_opacity(Number::raw(0.2))
            .stroke_width(Number::raw(1.5))
            .join(LineJoin::Round);
        out.layer(shapes, |out| {
            for (index, (curve, &part)) in curves.iter().zip(&curve_parts).enumerate() {
                // Mermaid leaves out a curve that does not have a value for every axis.
                if curve.entries.len() != count {
                    continue;
                }
                let points: Vec<[f64; 2]> = curve
                    .entries
                    .iter()
                    .zip(&unit)
                    .map(|(&value, u)| {
                        let r = relative_radius(value, options.min, max, radius);
                        let safe = if r.is_finite() { r } else { 0.0 };
                        [u[0] * safe, u[1] * safe]
                    })
                    .collect();
                let colour = Paint::Series(SeriesSlot::new(index));
                let element = out.element(part, Facet::Shape).class("pele-curve").data_id().fill(colour).stroke(colour);
                match options.graticule {
                    Graticule::Circle => element.path(closed_round_curve(&points, tension)).finish(),
                    Graticule::Polygon => element.polygon(polygon(&points)).finish(),
                }
            }
        });

        out.layer(LayerSpec::class("pele-axis-labels").font_size(Number::raw(small)), |out| {
            for (axis, &part) in labels.iter().zip(&axis_parts) {
                let spec = LayerSpec::class("pele-axis-label").part(part, Facet::Label).data_id().keep_empty();
                out.layer(spec, |out| out.label(part, Facet::Label, &axis.label, axis.at, LabelLook::default()));
            }
        });

        out.layer(LayerSpec::class("pele-legend").font_size(Number::raw(small)), |out| {
            for (index, (label, &part)) in legend.iter().zip(&curve_parts).enumerate() {
                let y = legend_top + index as f64 * ROW + ROW / 2.0;
                let colour = Paint::Series(SeriesSlot::new(index));
                out.layer(LayerSpec::class("pele-legend-item").part(part, Facet::Legend).data_id(), |out| {
                    out.element(part, Facet::Legend)
                        .rect(
                            [num(legend_x), num(y - SWATCH / 2.0)],
                            [Number::raw(SWATCH), Number::raw(SWATCH)],
                            Some(Number::raw(2.0)),
                        )
                        .fill(colour)
                        .fill_opacity(Number::raw(0.4))
                        .stroke(colour)
                        .finish();
                    let at = Point { x: legend_x + SWATCH + 8.0 + label.width / 2.0, y };
                    out.label(part, Facet::Legend, label, at, LabelLook::default());
                });
            }
        });
    });
    out.finish()
}
