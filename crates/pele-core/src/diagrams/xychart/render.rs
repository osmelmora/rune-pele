//! Pele's `renderXyChart` (`render.ts`): the axes and their marks, the bars and lines, their
//! values, the axis titles and the legend, as a Scene.

use super::db::{Orientation, XAxis, XyChartDb};
use super::ticks::{tick_step, ticks_between, value_axis};
use crate::catalog::DrawContext;
use crate::input::{JsObject, JsText, JsValue, Units};
use crate::measure::{Emphasis, Label, layout_label, width as measured};
use crate::scene::{
    AnchorKind, Entity, Facet, LabelLook, LayerSpec, LineCap, LineJoin, Number, Paint, PartId, PathProgram, Point,
    Role, Scene, SceneBuilder, SeriesSlot, Size, TitleLayout,
};
use crate::{Diagnostic, ErrorCode, RenderError, fit, js};
use std::collections::HashMap;

const TICK: f64 = 4.0;
const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const LEGEND_GAP: f64 = 24.0;
const LIMIT: f64 = 1e300;

/// A labelled place along an axis, as a fraction of its length.
struct Mark {
    at: f64,
    label: Label,
}

fn record(value: Option<&JsValue>) -> Option<&JsObject> {
    value?.as_record()
}

/// Pele's `on`: a flag of the config, or `fallback` where it is not a boolean.
fn on(config: Option<&JsObject>, key: &str, fallback: bool) -> bool {
    match config.and_then(|config| config.get(key)) {
        Some(JsValue::Bool(value)) => *value,
        _ => fallback,
    }
}

fn dimension(value: Option<&JsValue>, fallback: f64) -> f64 {
    match value.and_then(JsValue::as_number) {
        Some(value) if value > 0.0 => js::round(js::min(js::max(value, 100.0), 4000.0)),
        _ => fallback,
    }
}

/// Pele's `finite`: a number held within ±1e300, or none.
fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite()).map(|value| js::min(js::max(value, -LIMIT), LIMIT))
}

fn widest(marks: &[Mark]) -> f64 {
    let mut width = 0.0;
    for mark in marks {
        width = js::max(width, mark.label.width);
    }
    width
}

fn num(value: f64) -> Number {
    Number::num(value)
}

fn raw(value: f64) -> Number {
    Number::raw(value)
}

/// Pele's `aligned`: a label with its left edge, centre or right edge at `x`. A plain one-line
/// label keeps its place when the text is wider or narrower than measured.
fn aligned(out: &mut SceneBuilder, part: PartId, facet: Facet, label: &Label, x: f64, y: f64, anchor: AnchorKind) {
    if let [line] = label.lines.as_slice()
        && let [run] = line.runs.as_slice()
        && run.style.is_plain()
    {
        out.text(part, facet)
            .at([num(x), num(y + label.size * 0.35)])
            .anchor(anchor)
            .finish(&run.text, run.width, label.size);
        return;
    }
    let shift = match anchor {
        AnchorKind::Start => label.width / 2.0,
        AnchorKind::End => -label.width / 2.0,
        AnchorKind::Middle => 0.0,
    };
    out.label(part, facet, label, Point { x: x + shift, y }, LabelLook::default());
}

fn semantic(message: &str) -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message.to_owned()))
}

pub(crate) fn scene(model: &XyChartDb, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    if model.plots.is_empty() {
        return Err(semantic("No Plot to render, please provide a plot with some data"));
    }
    let chart = record(cx.config.get("xyChart"));
    let get = |key: &str| chart.and_then(|chart| chart.get(key));
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let tiny = js::round(size * 0.75);
    let measure = cx.measure;
    let pad = cx.options.padding;
    let natural = dimension(get("width"), 700.0);
    let width = fit::cap_width(cx.options, natural, 240.0, 0.0);
    // A narrower chart is not as much shorter, so that the plot keeps room for its marks.
    let height = js::round(dimension(get("height"), 500.0) * js::max(0.6, width / natural));
    let horizontal = match model.orientation {
        Some(orientation) => orientation == Orientation::Horizontal,
        None => get("chartOrientation").and_then(JsValue::as_text).is_some_and(|text| text.is_str("horizontal")),
    };
    // The value of a bar is always written past its end, which is what showDataLabelOutsideBar
    // asks for in Mermaid. Inside, whether it fits changes from bar to bar and with the width.
    let show_values = on(chart, "showDataLabel", false);
    let x = &model.x_axis;
    let plots = &model.plots;
    let text = |raw: Option<&[u16]>, font: f64, max: f64| -> Label {
        layout_label(raw.unwrap_or_default(), false, measure, font, max, Emphasis::default())
    };
    let zero = JsText::from("0");
    let line_height = text(Some(&zero), small, 4000.0).height;
    let tiny_height = text(Some(&zero), tiny, 4000.0).height;

    let title_text = if on(chart, "showTitle", true) { model.title.as_deref() } else { None };
    let title = layout_label(title_text.unwrap_or_default(), false, measure, size, width - 2.0 * pad, Emphasis::BOLD);
    let title_space = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };

    let mut legend: Vec<(usize, Label)> = Vec::new();
    let mut legend_width = 0.0;
    if on(chart, "showLegend", true) {
        for (index, plot) in plots.iter().enumerate() {
            if plot.title.is_empty() {
                continue;
            }
            let label = text(Some(&plot.title), small, width * 0.3);
            legend_width = js::max(legend_width, label.width + SWATCH + 8.0);
            legend.push((index, label));
        }
    }
    let legend_space = if legend.is_empty() { 0.0 } else { legend_width + LEGEND_GAP };

    // The value range: as written, or taken from the data, where bars stand on zero and the
    // range grows to whole steps.
    let bars = plots.iter().filter(|plot| !plot.line).count();
    let mut lo = finite(Some(model.y_axis.min)).unwrap_or(f64::INFINITY);
    let mut hi = finite(Some(model.y_axis.max)).unwrap_or(f64::NEG_INFINITY);
    let exact = model.has_set_y_axis && lo.is_finite() && hi.is_finite() && lo != hi;
    if !exact {
        lo = f64::INFINITY;
        hi = f64::NEG_INFINITY;
        for plot in plots {
            for &(_, raw) in &plot.data {
                let Some(value) = finite(raw) else { continue };
                if value < lo {
                    lo = value;
                }
                if value > hi {
                    hi = value;
                }
            }
        }
        if lo > hi {
            (lo, hi) = (0.0, 0.0);
        }
        if bars > 0 {
            lo = js::min(lo, 0.0);
            hi = js::max(hi, 0.0);
        }
        if lo == hi {
            if lo == 0.0 {
                hi = 1.0;
            } else {
                lo -= lo.abs() / 2.0;
                hi += hi.abs() / 2.0;
            }
        }
    }

    let number_label = |value: f64| text(Some(&JsText::from(js::to_string(value).as_str())), small, 4000.0);
    let value_marks = |length: f64, spacing: f64, lo: &mut f64, hi: &mut f64| -> Vec<Mark> {
        let (low, high, ticks) = value_axis(*lo, *hi, length, spacing, exact);
        (*lo, *hi) = (low, high);
        ticks.into_iter().map(|value| Mark { at: (value - low) / (high - low), label: number_label(value) }).collect()
    };

    // Places along the category axis are fractions of its length. Categories and bars sit in the
    // middle of equal slots; the points of a line on a numeric axis run from end to end.
    let band = matches!(x, XAxis::Band { .. });
    let count = match x {
        XAxis::Band { categories, .. } => categories.len().max(1),
        XAxis::Linear { .. } => plots.iter().map(|plot| plot.data.len()).fold(1, usize::max),
    } as f64;
    let slot = 1.0 / count;
    let inset = if band || bars > 0 { slot / 2.0 } else { 0.0 };
    let (x0, x1) = match x {
        XAxis::Linear { min, max, .. } => (finite(Some(*min)), finite(Some(*max))),
        XAxis::Band { .. } => (None, None),
    };
    let ranged = matches!((x0, x1), (Some(a), Some(b)) if a != b);
    let place = |i: usize, n: usize| -> f64 {
        if band {
            return (i as f64 + 0.5) * slot;
        }
        let fraction = if n > 1 {
            i as f64 / (n as f64 - 1.0)
        } else if ranged {
            0.0
        } else {
            0.5
        };
        inset + fraction * (1.0 - 2.0 * inset)
    };

    let category_conf = record(get("xAxis"));
    let value_conf = record(get("yAxis"));
    let (left_conf, bottom_conf) = if horizontal { (category_conf, value_conf) } else { (value_conf, category_conf) };
    let (left_raw, bottom_raw) =
        if horizontal { (x.title(), &model.y_axis.title) } else { (&model.y_axis.title, x.title()) };
    let left_title = text(on(left_conf, "showTitle", true).then_some(&left_raw[..]), small, height * 0.8);
    let bottom_title = text(on(bottom_conf, "showTitle", true).then_some(&bottom_raw[..]), small, width * 0.8);
    let left_tick = if on(left_conf, "showTick", true) { TICK } else { 0.0 };
    let bottom_tick = if on(bottom_conf, "showTick", true) { TICK } else { 0.0 };
    let left_labels = on(left_conf, "showLabel", true);
    let bottom_labels = on(bottom_conf, "showLabel", true);

    let wrap_at =
        if horizontal { width * 0.3 } else { js::max(48.0, (width - 2.0 * pad - 64.0 - legend_space) * slot - 8.0) };
    let categories: Vec<Label> = match x {
        XAxis::Band { categories, .. } if on(category_conf, "showLabel", true) => {
            categories.iter().map(|category| text(Some(category), small, wrap_at)).collect()
        }
        _ => Vec::new(),
    };
    let mut category_width = 0.0;
    let mut category_height = 0.0;
    for label in &categories {
        category_width = js::max(category_width, label.width);
        category_height = js::max(category_height, label.height);
    }

    // The labelled places of the category axis. Labels that would run into each other are thinned out.
    let category_marks = |length: f64| -> Vec<Mark> {
        let mut marks = Vec::new();
        match x {
            XAxis::Band { categories: names, .. } => {
                let need = if categories.is_empty() {
                    8.0
                } else if horizontal {
                    category_height
                } else {
                    category_width + 12.0
                };
                let every = js::max(1.0, (need / (length * slot)).ceil());
                let mut i = 0.0;
                while i < names.len() as f64 {
                    let label = categories.get(i as usize).cloned().unwrap_or_else(|| text(None, small, 0.0));
                    marks.push(Mark { at: (i + 0.5) * slot, label });
                    i += every;
                }
            }
            XAxis::Linear { .. } if ranged => {
                let (x0, x1) = (x0.expect("a range"), x1.expect("a range"));
                let spacing = if horizontal { 32.0 } else { 80.0 };
                let step = tick_step((x1 - x0).abs(), js::max(2.0, js::min(10.0, (length / spacing).floor())));
                for value in ticks_between(js::min(x0, x1), js::max(x0, x1), step) {
                    let at = inset + ((value - x0) / (x1 - x0)) * (1.0 - 2.0 * inset);
                    marks.push(Mark { at, label: number_label(value) });
                }
            }
            XAxis::Linear { .. } => {
                if let Some(x0) = x0 {
                    marks.push(Mark { at: 0.5, label: number_label(x0) });
                }
            }
        }
        marks
    };

    // Room past the end of the value axis for what is written beside a point or a bar.
    let mut point_label_width = 0.0;
    let mut value_width = 0.0;
    for plot in plots {
        for label in plot.point_labels.iter().flatten() {
            point_label_width = js::max(point_label_width, measured(measure, label, tiny, Emphasis::default()));
        }
        if show_values && horizontal && !plot.line {
            for &(_, value) in &plot.data {
                let written = finite(value).map(js::to_string).unwrap_or_default();
                let written: Vec<u16> = written.encode_utf16().collect();
                value_width = js::max(value_width, measured(measure, &written, tiny, Emphasis::default()));
            }
        }
    }

    let bottom_label_height = if !bottom_labels {
        0.0
    } else if horizontal || !band {
        line_height
    } else {
        category_height
    };
    let bottom_space = bottom_tick
        + if bottom_label_height > 0.0 { bottom_label_height + 4.0 } else { 0.0 }
        + if bottom_title.height > 0.0 { bottom_title.height + 4.0 } else { 0.0 };
    let headroom = if horizontal {
        0.0
    } else if point_label_width > 0.0 || show_values {
        tiny_height + 4.0
    } else if left_labels {
        line_height / 2.0
    } else {
        0.0
    };
    let top = pad + title_space + headroom;
    let plot_height = js::max(1.0, height - pad - bottom_space - top);
    let bottom = top + plot_height;

    let left_marks =
        if horizontal { category_marks(plot_height) } else { value_marks(plot_height, 56.0, &mut lo, &mut hi) };
    let left_label_width = if left_labels { widest(&left_marks) } else { 0.0 };
    let left = pad
        + if left_title.height > 0.0 { left_title.height + 6.0 } else { 0.0 }
        + if left_label_width > 0.0 { left_label_width + 6.0 } else { 0.0 }
        + left_tick;
    let beside = js::max(
        if point_label_width > 0.0 { point_label_width + 12.0 } else { 0.0 },
        if value_width > 0.0 { value_width + 8.0 } else { 0.0 },
    );
    let reach = if horizontal { js::min(width * 0.25, beside) } else { 0.0 };
    let mut plot_width = js::max(1.0, width - pad - left - legend_space - reach);
    let bottom_marks =
        if horizontal { value_marks(plot_width, 80.0, &mut lo, &mut hi) } else { category_marks(plot_width) };
    if bottom_labels && legend_space + reach == 0.0 {
        let mut over = 0.0;
        for mark in &bottom_marks {
            over = js::max(over, mark.at * plot_width + mark.label.width / 2.0 - plot_width);
        }
        plot_width = js::max(1.0, plot_width - over);
    }
    let right = left + plot_width;

    let px = |c: f64, v: f64| if horizontal { left + v * plot_width } else { left + c * plot_width };
    let py = |c: f64, v: f64| if horizontal { top + c * plot_height } else { bottom - v * plot_height };
    let scale = |value: f64| (value - lo) / (hi - lo);
    let clamp = |v: f64| js::min(js::max(v, 0.0), 1.0);

    // A line across the plot at a place on the value axis.
    let rule = |path: PathProgram, v: f64| {
        if horizontal {
            path.m(num(px(0.0, v)), num(top)).v(num(bottom))
        } else {
            path.m(num(left), num(py(0.0, v))).h(num(right))
        }
    };

    let mut grid = PathProgram::default();
    for mark in if horizontal { &bottom_marks } else { &left_marks } {
        grid = rule(grid, mark.at);
    }

    let mut axis = PathProgram::default();
    if on(left_conf, "showAxisLine", true) {
        axis = axis.m(num(left), num(top)).v(num(bottom));
    }
    if on(bottom_conf, "showAxisLine", true) {
        axis = axis.m(num(left), num(bottom)).h(num(right));
    }
    let mut labels: Vec<(&Label, f64, f64, AnchorKind)> = Vec::new();
    for mark in &left_marks {
        let y = if horizontal { top + mark.at * plot_height } else { bottom - mark.at * plot_height };
        if left_tick > 0.0 {
            axis = axis.m(num(left), num(y)).h_by(raw(-left_tick));
        }
        if left_labels {
            labels.push((&mark.label, left - left_tick - 6.0, y, AnchorKind::End));
        }
    }
    for mark in &bottom_marks {
        let at = left + mark.at * plot_width;
        if bottom_tick > 0.0 {
            axis = axis.m(num(at), num(bottom)).v_by(raw(bottom_tick));
        }
        if bottom_labels {
            let y = bottom + bottom_tick + 4.0 + mark.label.height / 2.0;
            labels.push((&mark.label, at, y, AnchorKind::Middle));
        }
    }

    let span = (if horizontal { plot_height } else { plot_width }) * slot;
    let group = span * 0.7;
    let thickness = group / js::max(bars as f64, 1.0);
    let gap = if bars > 1 { js::min(2.0, thickness * 0.2) } else { 0.0 };
    let base = clamp(scale(0.0));
    // Bars that go both ways get a line to stand on.
    if bars > 0 && base > 0.0 && base < 1.0 {
        axis = rule(axis, base);
    }

    let mut out = SceneBuilder::new(cx, Size { width, height }, model.accessible());
    let diagram = out.part(Entity::diagram());
    let title_at = Point { x: width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at: title_at });
    if !grid.steps.is_empty() {
        out.decoration().class("pele-grid").path(grid).stroke(Paint::Role(Role::SurfaceAlt)).finish();
    }

    // A series keeps the colour of its place in the source, and its key the place among the
    // series with its title.
    let mut ranks: HashMap<&JsText, u32> = HashMap::new();
    let parts: Vec<PartId> = plots
        .iter()
        .map(|plot| {
            let rank = ranks.entry(&plot.title).or_insert(0);
            let duplicate = std::mem::replace(rank, *rank + 1);
            out.part(Entity::series(&plot.title).ranked(duplicate))
        })
        .collect();
    let series_layer = |class: &str, index: usize| {
        let spec = LayerSpec::class(class).part(parts[index], Facet::Shape);
        let spec = if plots[index].title.is_empty() { spec } else { spec.data_id() };
        spec.fill(Paint::Series(SeriesSlot::new(index))).keep_empty()
    };
    let points_in = |index: usize| match x {
        XAxis::Band { .. } => plots[index].data.len().min(count as usize),
        XAxis::Linear { .. } => plots[index].data.len(),
    };

    let mut values: Vec<(PartId, JsText, f64, f64, f64, AnchorKind)> = Vec::new();
    let mut point_labels: Vec<(PartId, Label, f64, f64, AnchorKind)> = Vec::new();
    // Lines are drawn over the bars wherever they were declared, as a bar would hide part of one
    // under it.
    out.layer(LayerSpec::class("pele-plot").keep_empty(), |out| {
        let mut bar = 0.0;
        for (index, plot) in plots.iter().enumerate().filter(|(_, plot)| !plot.line) {
            let part = parts[index];
            let offset = (bar - (bars as f64 - 1.0) / 2.0) * thickness - (thickness - gap) / 2.0;
            bar += 1.0;
            let n = points_in(index);
            out.layer(series_layer("pele-series pele-bars", index), |out| {
                for (i, &(_, raw)) in plot.data.iter().enumerate().take(n) {
                    let Some(value) = finite(raw) else { continue };
                    let c = place(i, n);
                    let v = clamp(scale(value));
                    let from = js::min(base, v);
                    let to = js::max(base, v);
                    let bx = if horizontal { px(c, from) } else { px(c, 0.0) + offset };
                    let by = if horizontal { py(c, 0.0) + offset } else { py(c, to) };
                    let bw = if horizontal { (to - from) * plot_width } else { thickness - gap };
                    let bh = if horizontal { thickness - gap } else { (to - from) * plot_height };
                    out.element(part, Facet::Shape).rect([num(bx), num(by)], [num(bw), num(bh)], None).finish();
                    if !show_values {
                        continue;
                    }
                    let label = JsText::from(js::to_string(value).as_str());
                    let w = measured(measure, &label, tiny, Emphasis::default());
                    // Towards the end of the bar: 1 when it runs up or right from the baseline,
                    // -1 the other way.
                    let out_of = if v >= base { 1.0 } else { -1.0 };
                    let mut tx = bx + bw / 2.0;
                    let mut ty = by + bh / 2.0;
                    let mut anchor = AnchorKind::Middle;
                    if horizontal {
                        if bh < tiny {
                            continue;
                        }
                        tx = (if out_of > 0.0 { bx + bw } else { bx }) + out_of * 6.0;
                        anchor = if out_of > 0.0 { AnchorKind::Start } else { AnchorKind::End };
                    } else {
                        if w + 2.0 > span / bars as f64 {
                            continue;
                        }
                        ty = (if out_of > 0.0 { by } else { by + bh }) - out_of * (tiny_height / 2.0 + 2.0);
                    }
                    values.push((part, label, w, tx, ty + tiny * 0.35, anchor));
                }
            });
        }
        for (index, plot) in plots.iter().enumerate().filter(|(_, plot)| plot.line) {
            let part = parts[index];
            let colour = Paint::Series(SeriesSlot::new(index));
            let n = points_in(index);
            // A line is cut where it leaves the value range, so it stays inside the plot.
            let mut d = PathProgram::default();
            let mut dots = Vec::new();
            let mut pen = false;
            let mut pc = 0.0;
            let mut pv = f64::NAN;
            for (i, &(_, raw)) in plot.data.iter().enumerate().take(n) {
                let Some(value) = finite(raw) else {
                    pv = f64::NAN;
                    continue;
                };
                let c = place(i, n);
                let v = scale(value);
                let visible = (0.0..=1.0).contains(&v);
                if pv.is_nan() {
                    if visible {
                        d = d.m(num(px(c, v)), num(py(c, v)));
                    }
                    pen = visible;
                } else {
                    let dv = v - pv;
                    let mut t0 = 0.0;
                    let mut t1 = 1.0;
                    if dv != 0.0 {
                        let a = -pv / dv;
                        let b = (1.0 - pv) / dv;
                        t0 = js::max(t0, js::min(a, b));
                        t1 = js::min(t1, js::max(a, b));
                    } else if !visible {
                        t0 = 2.0;
                    }
                    if t0 <= t1 {
                        let c0 = pc + (c - pc) * t0;
                        let v0 = pv + dv * t0;
                        let c1 = pc + (c - pc) * t1;
                        let v1 = pv + dv * t1;
                        if !pen || t0 > 0.0 {
                            d = d.m(num(px(c0, v0)), num(py(c0, v0)));
                        }
                        d = d.l(num(px(c1, v1)), num(py(c1, v1)));
                    }
                    pen = visible;
                }
                pc = c;
                pv = v;
                if !visible {
                    continue;
                }
                let cx = px(c, v);
                let cy = py(c, v);
                if span >= 12.0 {
                    dots.push((cx, cy));
                }
                let written = plot.point_labels.as_ref().and_then(|labels| labels.get(i));
                let label = text(written.map(|label| &label[..]), tiny, 4000.0);
                if label.height > 0.0 {
                    let half = label.width / 2.0;
                    if horizontal {
                        point_labels.push((part, label, cx + 10.0, cy, AnchorKind::Start));
                    } else {
                        let x = js::max(js::min(cx, right - half), left + half);
                        let y = cy - 6.0 - label.height / 2.0;
                        point_labels.push((part, label, x, y, AnchorKind::Middle));
                    }
                }
            }
            out.layer(series_layer("pele-series pele-line", index), |out| {
                if !d.steps.is_empty() {
                    out.element(part, Facet::Shape)
                        .path(d)
                        .fill(Paint::None)
                        .stroke(colour)
                        .stroke_width(raw(2.0))
                        .cap(LineCap::Round)
                        .join(LineJoin::Round)
                        .finish();
                }
                for (cx, cy) in dots {
                    out.element(part, Facet::Shape).circle_at([Some(num(cx)), Some(num(cy))], raw(3.0)).finish();
                }
            });
        }
    });

    if !axis.steps.is_empty() {
        out.decoration().class("pele-axis").path(axis).fill(Paint::None).stroke(Paint::Role(Role::Border)).finish();
    }
    let muted = Paint::Role(Role::Muted);
    out.layer(LayerSpec::class("pele-axis-labels").font_size(raw(small)).fill(muted), |out| {
        for &(label, x, y, anchor) in &labels {
            aligned(out, diagram, Facet::Label, label, x, y, anchor);
        }
    });
    out.layer(LayerSpec::class("pele-axis-titles").font_size(raw(small)), |out| {
        if left_title.height > 0.0 {
            let at = Point { x: pad + left_title.height / 2.0, y: top + plot_height / 2.0 };
            out.layer(LayerSpec::group().translate_rotate(at, -90.0), |out| {
                let origin = Point { x: 0.0, y: 0.0 };
                out.label(diagram, Facet::Legend, &left_title, origin, LabelLook::default());
            });
        }
        let at = Point { x: left + plot_width / 2.0, y: height - pad - bottom_title.height / 2.0 };
        out.label(diagram, Facet::Legend, &bottom_title, at, LabelLook::default());
    });
    out.layer(LayerSpec::class("pele-data-labels").font_size(raw(tiny)), |out| {
        for (part, label, w, x, y, anchor) in &values {
            out.text(*part, Facet::Label).at([num(*x), num(*y)]).anchor(*anchor).finish(label, *w, tiny);
        }
    });
    out.layer(LayerSpec::class("pele-point-labels").font_size(raw(tiny)), |out| {
        for (part, label, x, y, anchor) in &point_labels {
            aligned(out, *part, Facet::Label, label, *x, *y, *anchor);
        }
    });

    let mut legend_height = 0.0;
    for (_, label) in &legend {
        legend_height += js::max(ROW, label.height + 6.0);
    }
    let mut ly = top + js::max(0.0, (plot_height - legend_height) / 2.0);
    let lx = right + reach + LEGEND_GAP;
    out.layer(LayerSpec::class("pele-legend").font_size(raw(small)), |out| {
        for (index, label) in &legend {
            let row = js::max(ROW, label.height + 6.0);
            if ly + row > height {
                break;
            }
            let y = ly + row / 2.0;
            let part = parts[*index];
            let colour = Paint::Series(SeriesSlot::new(*index));
            out.layer(LayerSpec::class("pele-legend-item").part(part, Facet::Legend).data_id(), |out| {
                if plots[*index].line {
                    let path = PathProgram::default().m(num(lx), num(y)).h_by(raw(SWATCH));
                    out.element(part, Facet::Legend)
                        .path(path)
                        .stroke(colour)
                        .stroke_width(raw(2.0))
                        .cap(LineCap::Round)
                        .finish();
                } else {
                    out.element(part, Facet::Legend)
                        .rect([num(lx), num(y - SWATCH / 2.0)], [raw(SWATCH), raw(SWATCH)], Some(raw(2.0)))
                        .fill(colour)
                        .finish();
                }
                let at = Point { x: lx + SWATCH + 8.0 + label.width / 2.0, y };
                out.label(part, Facet::Legend, label, at, LabelLook::default());
            });
            ly += row;
        }
    });
    out.finish()
}
