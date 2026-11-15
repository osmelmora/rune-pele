//! Pele's `renderGantt`: tasks as bars on rows, grouped in shaded sections, under a time axis.

use super::axis::{auto_ticks, interval_ticks, time_format};
use super::date::JsDate;
use super::dates::{Unit, add, add_days};
use super::db::Scheduled;
use super::{GanttModel, Scheduler, Task};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::url::link_allowed;
use crate::input::{JsText, JsValue, Units};
use crate::measure::{Emphasis, Label, layout_label, width as text_width};
use crate::scene::{
    Accessible, AnchorKind, Entity, Facet, LabelLook, LayerSpec, LineJoin, Number, Paint, PartId, PathProgram, Point,
    Role, Scene, SceneBuilder, Size, Style, TitleLayout, resolve_style,
};
use crate::{RenderError, fit, js};
use indexmap::IndexMap;
use std::collections::HashMap;

pub(crate) struct Gantt;

impl Diagram for Gantt {
    type Model = GanttModel;

    fn parse(cx: &ParseContext<'_>) -> Result<GanttModel, RenderError> {
        super::parse(cx)
    }

    fn scene(model: &GanttModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        draw(model, cx)
    }
}

const WIDTH: f64 = 800.0;
const MIN_PLOT: f64 = 240.0;
const LABEL_GAP: f64 = 8.0;

/// Pele's `clock`: the time given when it is a finite number, otherwise the current time.
fn clock(now: Option<f64>) -> f64 {
    now.filter(|now| now.is_finite()).unwrap_or_else(|| {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        since.map_or(0.0, |since| since.as_millis() as f64)
    })
}

/// The chart's settings from Mermaid's `gantt` config section.
struct Config<'a> {
    section: Option<&'a crate::input::JsObject>,
}

impl Config<'_> {
    fn value(&self, key: &str) -> Option<&JsValue> {
        self.section?.get(key)
    }

    /// A finite number held to `min..=max`.
    fn setting(&self, key: &str, min: f64, max: f64) -> Option<f64> {
        let value = self.value(key)?.as_number().filter(|value| value.is_finite())?;
        Some(js::min(max, js::max(min, value)))
    }

    fn text(&self, key: &str) -> Option<&JsText> {
        self.value(key)?.as_text()
    }

    fn flag(&self, key: &str) -> bool {
        self.value(key) == Some(&JsValue::Bool(true))
    }
}

/// `<br>` in any case, with spaces before an optional slash: `/<br\s*\/?>/gi`.
fn without_breaks(raw: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        let open = raw.get(at..at + 3).is_some_and(|head| {
            head[0] == u16::from(b'<')
                && head[1..].iter().zip(b"br").all(|(&c, &b)| u8::try_from(c).is_ok_and(|c| c.eq_ignore_ascii_case(&b)))
        });
        if open {
            let mut end = at + 3 + raw[at + 3..].iter().take_while(|&&c| crate::input::is_space(c)).count();
            if raw.get(end) == Some(&u16::from(b'/')) {
                end += 1;
            }
            if raw.get(end) == Some(&u16::from(b'>')) {
                out.push(32);
                at = end + 1;
                continue;
            }
        }
        out.push(raw[at]);
        at += 1;
    }
    out
}

/// A task label sits on one row, so only its first line is kept.
fn one_line(raw: &[u16], cx: &DrawContext<'_>, size: f64) -> Label {
    let label = layout_label(&without_breaks(raw), false, cx.measure, size, 4000.0, Emphasis::default());
    if label.lines.len() <= 1 { label } else { label.line(0) }
}

/// Whether scheduling gave the task a start and an end.
fn scheduled(task: &Scheduled) -> bool {
    let finite = |date: Option<JsDate>| date.is_some_and(|date| date.time().is_finite());
    finite(task.start) && finite(task.end)
}

fn time(date: Option<JsDate>) -> f64 {
    date.map_or(f64::NAN, JsDate::time)
}

/// Mermaid's compact mode: within a section, a task goes on the first row that is free when it
/// starts. Returns how many rows the section takes.
fn pack_rows(
    group: &[usize],
    offset: usize,
    tasks: &[Task],
    schedule: &[Scheduled],
    row_of: &mut HashMap<usize, usize>,
) -> usize {
    let mut sorted: Vec<usize> = group.iter().copied().filter(|&i| scheduled(&schedule[i])).collect();
    sorted.sort_by(|&a, &b| {
        let by_start = time(schedule[a].start) - time(schedule[b].start);
        let by_order = tasks[a].order.unwrap_or(0) as f64 - tasks[b].order.unwrap_or(0) as f64;
        let key = if by_start != 0.0 { by_start } else { by_order };
        key.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut free: Vec<f64> = Vec::new();
    for &task in &sorted {
        let start = time(schedule[task].start);
        let mut row = 0;
        while row < free.len() && start < free[row] {
            row += 1;
        }
        if row == free.len() {
            free.push(0.0);
        }
        free[row] = time(schedule[task].end);
        row_of.insert(task, offset + row);
    }
    let mut rows = free.len();
    for &task in group {
        if let std::collections::hash_map::Entry::Vacant(entry) = row_of.entry(task) {
            entry.insert(offset + rows);
            rows += 1;
        }
    }
    rows.max(1)
}

/// Where a row task is drawn: its bar, which an unscheduled task has none of, and its label.
struct Bar {
    task: usize,
    label: Label,
    row: usize,
    /// The bar's left and right ends.
    span: Option<(f64, f64)>,
    label_x: f64,
    inside: bool,
}

/// Each task's entity: the id the author gave it, or else its trimmed label, which unlike the number
/// Pele gives it stays put when a task is added before it. A task is ranked among those keyed
/// by the same text, whichever way, so no two share a key.
fn task_parts(out: &mut SceneBuilder, model: &GanttModel) -> Vec<PartId> {
    let mut seen: HashMap<JsText, u32> = HashMap::new();
    model
        .tasks
        .iter()
        .map(|task| {
            let name: JsText = if task.own_id { task.id.clone() } else { task.task.trim_js().into() };
            let rank = seen.entry(name.clone()).or_default();
            let part = out.part(Entity::node_written_as(&name, &task.id).ranked(*rank));
            *rank += 1;
            part
        })
        .collect()
}

fn draw(model: &GanttModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let pad = cx.options.padding;
    let now = clock(cx.options.now);
    let config = Config { section: cx.config.get("gantt").and_then(JsValue::as_record) };
    let plain = Emphasis::default();

    let label_size = config.setting("fontSize", 4.0, 72.0).unwrap_or_else(|| js::round(size * 0.8125));
    let section_size = config.setting("sectionFontSize", 4.0, 72.0).unwrap_or(label_size);
    let tick_size = js::round(size * 0.75);
    let bar_height = config.setting("barHeight", 4.0, 400.0).unwrap_or_else(|| js::round(label_size * 1.85));
    let bar_gap = config.setting("barGap", 0.0, 400.0).unwrap_or_else(|| js::round(bar_height / 4.0));
    let row_height = bar_height + bar_gap;
    let section_styles = js::round(config.setting("numberSectionStyles", 1.0, 1000.0).unwrap_or(4.0));
    let compact = model.display_mode.is_str("compact");
    let top_axis = model.top_axis || config.flag("topAxis");

    let mut scheduler = Scheduler::new(model, now);
    scheduler.run()?;
    let schedule = scheduler.tasks.clone();
    let row_tasks: Vec<usize> = (0..model.tasks.len()).filter(|&i| !model.tasks[i].tags.vert).collect();
    let verts: Vec<usize> =
        (0..model.tasks.len()).filter(|&i| model.tasks[i].tags.vert && scheduled(&schedule[i])).collect();

    let mut categories: IndexMap<&JsText, usize> = IndexMap::new();
    for &task in &row_tasks {
        let next = categories.len();
        categories.entry(&model.tasks[task].section).or_insert(next);
    }

    let mut row_of: HashMap<usize, usize> = HashMap::new();
    let mut row_section: Vec<&JsText> = Vec::new();
    if compact {
        let mut groups: IndexMap<&JsText, Vec<usize>> = IndexMap::new();
        for &task in &row_tasks {
            groups.entry(&model.tasks[task].section).or_default().push(task);
        }
        for (section, group) in groups {
            let rows = pack_rows(&group, row_section.len(), &model.tasks, &schedule, &mut row_of);
            row_section.extend(std::iter::repeat_n(section, rows));
        }
    } else {
        for &task in &row_tasks {
            row_of.insert(task, row_section.len());
            row_section.push(&model.tasks[task].section);
        }
    }
    let row_count = row_section.len();

    let (mut start, mut stop) = (f64::INFINITY, f64::NEG_INFINITY);
    for task in schedule.iter().filter(|task| scheduled(task)) {
        start = js::min(start, time(task.start));
        stop = js::max(stop, time(task.end));
    }
    let has_plot = start <= stop;

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, cx.measure, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };

    if !has_plot {
        let width = (js::max(title.width, 1.0) + 2.0 * pad).ceil();
        let height = (js::max(title.height, 1.0) + 2.0 * pad).ceil();
        let mut out = SceneBuilder::new(cx, Size { width, height }, accessible);
        let at = Point { x: width / 2.0, y: pad + title.height / 2.0 };
        out.title(&TitleLayout { label: title, at });
        return out.finish();
    }

    let mut section_labels: HashMap<&JsText, Label> = HashMap::new();
    let mut section_width: f64 = 0.0;
    for &section in categories.keys() {
        let label = layout_label(section, false, cx.measure, section_size, 180.0, plain);
        section_width = js::max(section_width, label.width);
        section_labels.insert(section, label);
    }

    let axis_format: JsText = if !model.axis_format.is_empty() {
        model.axis_format.clone()
    } else if model.date_format.is_str("D") {
        "%d".into()
    } else {
        config.text("axisFormat").cloned().unwrap_or_else(|| "%Y-%m-%d".into())
    };
    let tick_label = |t: f64| time_format(&axis_format, JsDate::from_time(t));
    let sample_width = js::max(
        text_width(cx.measure, &tick_label(start), tick_size, plain),
        text_width(cx.measure, &tick_label(stop), tick_size, plain),
    );

    let left = pad
        + config.setting("leftPadding", 0.0, 2000.0).unwrap_or_else(|| {
            js::max(if section_width > 0.0 { section_width + 24.0 } else { 0.0 }, sample_width / 2.0)
        });
    let right_padding =
        config.setting("rightPadding", 0.0, 2000.0).unwrap_or_else(|| js::max(16.0, sample_width / 2.0));
    let natural = config.setting("useWidth", 100.0, 20000.0).unwrap_or(WIDTH);
    let target = fit::cap_width(cx.options, natural, 280.0, 0.0);
    let plot_width = js::max(MIN_PLOT, target - left - right_padding - pad);
    let right = left + plot_width;
    let scale = if stop > start { plot_width / (stop - start) } else { 0.0 };
    let x = |t: f64| if scale == 0.0 { left + plot_width / 2.0 } else { left + (t - start) * scale };

    let tick_height = js::round(tick_size * 1.75);
    let top_padding = config.setting("topPadding", 0.0, 2000.0);
    let grid_start = config.setting("gridLineStartPadding", 0.0, 2000.0);
    let head_height = pad + title_height + if top_axis { tick_height } else { 0.0 };
    let plot_top = js::max(head_height, top_padding.unwrap_or(0.0));
    // Mermaid starts its grid lines above the first row by the difference between these two
    // settings. Here that only uses room a larger `topPadding` leaves free.
    let grid_lead = match (top_padding, grid_start) {
        (Some(top), Some(grid)) => js::max(0.0, js::min(top - grid, plot_top - head_height)),
        _ => 0.0,
    };
    let plot_height = (row_count.max(1) as f64) * row_height;
    let plot_bottom = plot_top + plot_height;

    // Bars and where their labels go: inside when they fit, otherwise beside the bar on the side with room.
    let mut bars: Vec<Bar> = Vec::new();
    let mut extent = right + right_padding;
    for &task_index in &row_tasks {
        let task = &model.tasks[task_index];
        let when = &schedule[task_index];
        let label = one_line(&task.task, cx, label_size);
        let row = row_of[&task_index];
        if !scheduled(when) {
            let label_x = left + LABEL_GAP + label.width / 2.0;
            extent = js::max(extent, label_x + label.width / 2.0);
            bars.push(Bar { task: task_index, label, row, span: None, label_x, inside: false });
            continue;
        }
        let milestone = task.tags.milestone;
        let mut x0 = x(time(when.start));
        let mut x1 = js::max(x(time(when.render_end.or(when.end))), x0 + if milestone { 0.0 } else { 2.0 });
        if milestone {
            let mid = x0 + (x(time(when.end)) - x0) / 2.0;
            x0 = mid - bar_height / 2.0;
            x1 = mid + bar_height / 2.0;
        }
        let inside = !milestone && label.width + 2.0 * LABEL_GAP <= x1 - x0;
        let mut label_x = (x0 + x1) / 2.0;
        if !inside {
            let fits_right = x1 + LABEL_GAP + label.width <= right + right_padding;
            let fits_left = x0 - LABEL_GAP - label.width >= left;
            label_x = if fits_right || !fits_left {
                x1 + LABEL_GAP + label.width / 2.0
            } else {
                x0 - LABEL_GAP - label.width / 2.0
            };
            extent = js::max(extent, label_x + label.width / 2.0);
        }
        bars.push(Bar { task: task_index, label, row, span: Some((x0, x1)), label_x, inside });
    }

    let interval = match &model.tick_interval {
        Some(interval) if !interval.is_empty() => interval.clone(),
        _ => config.text("tickInterval").filter(|text| !text.is_empty()).cloned().unwrap_or_default(),
    };
    let weekday: JsText = match model.weekday {
        Some(day) => day.into(),
        None => config.text("weekday").cloned().unwrap_or_else(|| "sunday".into()),
    };
    let count = js::max(2.0, js::min(10.0, (plot_width / (sample_width + 24.0)).floor()));
    let ticks = interval_ticks(&interval, &weekday, start, stop, (plot_width / 3.0).ceil())
        .unwrap_or_else(|| auto_ticks(start, stop, count));

    struct AxisLabel {
        text: Vec<u16>,
        width: f64,
        x: f64,
    }
    let mut grid = PathProgram::default();
    let mut axis_labels: Vec<AxisLabel> = Vec::new();
    let mut last_right = f64::NEG_INFINITY;
    let mut last_label: Vec<u16> = Vec::new();
    let grid_top = plot_top - grid_lead;
    for &t in &ticks {
        let tx = x(t);
        grid = grid.m(Number::num(tx), Number::num(grid_top)).v(Number::num(plot_bottom));
        let label = tick_label(t);
        let width = text_width(cx.measure, &label, tick_size, plain);
        // A label is skipped when it would run into the one before it or only repeat it; its grid line stays.
        if tx - width / 2.0 < last_right + 8.0 || label == last_label {
            continue;
        }
        last_right = tx + width / 2.0;
        extent = js::max(extent, last_right);
        last_label = label.clone();
        axis_labels.push(AxisLabel { text: label, width, x: tx });
    }
    let bottom_y = plot_bottom + tick_height / 2.0 + 2.0;
    let top_y = plot_top - grid_lead - tick_height / 2.0 - 2.0;

    // Vertical markers take no row. Their labels go under the axis, on as many lines as it takes to keep them apart.
    struct Vert {
        task: usize,
        x: f64,
        label: Label,
        centre: Point,
    }
    let mut vert_list: Vec<Vert> = Vec::new();
    let mut vert_ends: Vec<f64> = Vec::new();
    let vert_top = plot_bottom + tick_height + 4.0;
    let vert_line = js::round(label_size * 1.5);
    let mut sorted_verts = verts.clone();
    sorted_verts.sort_by(|&a, &b| {
        (time(schedule[a].start) - time(schedule[b].start)).partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
    });
    for task in sorted_verts {
        let vx = x(time(schedule[task].start));
        let label = one_line(&model.tasks[task].task, cx, label_size);
        let centre_x = js::max(pad + label.width / 2.0, vx);
        let mut line = 0;
        while line < vert_ends.len() && centre_x - label.width / 2.0 < vert_ends[line] + 8.0 {
            line += 1;
        }
        if line == vert_ends.len() {
            vert_ends.push(0.0);
        }
        vert_ends[line] = centre_x + label.width / 2.0;
        extent = js::max(extent, vert_ends[line]);
        let centre = Point { x: centre_x, y: vert_top + line as f64 * vert_line + vert_line / 2.0 };
        vert_list.push(Vert { task, x: vx, label, centre });
    }

    let width = (js::max(extent, pad + title.width) + pad).ceil();
    let vert_room = if vert_ends.is_empty() { 0.0 } else { 4.0 + vert_ends.len() as f64 * vert_line };
    let height = (plot_bottom + tick_height + vert_room + pad).ceil();

    // Excluded days, as Mermaid marks them: a day at a time from the first task's start, and not
    // at all when the chart spans more than five years.
    let mut excluded: Vec<(f64, f64)> = Vec::new();
    if (!model.excludes.is_empty() || !model.includes.is_empty())
        && add(JsDate::from_time(start), 6.0, Unit::Year).time() > stop
    {
        let mut run: Option<(JsDate, JsDate)> = None;
        let mut flush = |run: &mut Option<(JsDate, JsDate)>| {
            if let Some((from, to)) = run.take() {
                let mut day_start = from;
                day_start.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0));
                let mut day_end = to;
                day_end.set_hours(23.0, Some(59.0), Some(59.0), Some(999.0));
                let ex0 = js::max(left, x(day_start.time()));
                let ex1 = js::min(right, x(day_end.time()));
                if ex1 > ex0 {
                    excluded.push((ex0, ex1));
                }
            }
        };
        let mut day = JsDate::from_time(start);
        while day.time() <= stop {
            if scheduler.is_invalid_date(day)? {
                run = Some((run.map_or(day, |(from, _)| from), day));
            } else {
                flush(&mut run);
            }
            day = add_days(day, 1.0);
        }
        flush(&mut run);
    }

    let mut out = SceneBuilder::new(cx, Size { width, height }, accessible);
    let parts = task_parts(&mut out, model);
    let diagram = out.part(Entity::diagram());
    let at = Point { x: width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at });

    // Section bands, one per run of rows that share a section, shaded on alternate sections.
    struct Band<'a> {
        section: &'a JsText,
        y: f64,
        h: f64,
    }
    let mut bands: Vec<Band<'_>> = Vec::new();
    let mut row = 0;
    while row < row_count {
        let section = row_section[row];
        let mut end = row + 1;
        while end < row_count && row_section[end] == section {
            end += 1;
        }
        bands.push(Band { section, y: plot_top + row as f64 * row_height, h: (end - row) as f64 * row_height });
        row = end;
    }
    out.layer(LayerSpec::class("pele-sections"), |out| {
        for band in bands.iter().filter(|band| categories.len() > 1 || !band.section.is_empty()) {
            let index = categories[&band.section] as f64 % section_styles;
            let part = out.part(Entity::group(band.section));
            out.element(part, Facet::Shape)
                .class(&format!("pele-section pele-section-{}", js::to_string(index)))
                .rect([Number::raw(0.0), Number::num(band.y)], [Number::raw(width), Number::num(band.h)], None)
                .fill(if index % 2.0 == 0.0 { Paint::Role(Role::Surface) } else { Paint::None })
                .finish();
        }
    });

    out.layer(LayerSpec::class("pele-excluded").fill(Paint::Role(Role::SurfaceAlt)), |out| {
        for &(ex0, ex1) in &excluded {
            out.element(diagram, Facet::Shape)
                .rect(
                    [Number::num(ex0), Number::num(plot_top)],
                    [Number::num(ex1 - ex0), Number::num(plot_height)],
                    None,
                )
                .finish();
        }
    });

    let mut axis_line = PathProgram::default().m(Number::num(left), Number::num(plot_bottom)).h(Number::num(right));
    if top_axis {
        axis_line = axis_line.m(Number::num(left), Number::num(grid_top)).h(Number::num(right));
    }
    out.layer(LayerSpec::class("pele-grid").fill(Paint::None), |out| {
        out.element(diagram, Facet::Shape)
            .path(grid)
            .stroke(Paint::Role(Role::Border))
            .dash(&[Number::raw(1.0), Number::raw(3.0)])
            .finish();
        out.element(diagram, Facet::Shape).path(axis_line).stroke(Paint::Role(Role::Border)).finish();
    });

    let verts_layer = LayerSpec::class("pele-verts")
        .fill(Paint::None)
        .stroke(Paint::Role(Role::Line))
        .stroke_width(Number::raw(1.5))
        .dash(&[Number::raw(4.0), Number::raw(3.0)]);
    out.layer(verts_layer, |out| {
        for vert in &vert_list {
            let path = PathProgram::default().m(Number::num(vert.x), Number::num(grid_top)).v(Number::num(plot_bottom));
            out.element(parts[vert.task], Facet::Shape).class("pele-vert").data_id().path(path).finish();
        }
    });

    if !model.today_marker.is_str("off") && now >= start && now <= stop && stop > start {
        let tx = x(now);
        let top = grid_top;
        let style: Option<Style> = if model.today_marker.is_empty() {
            None
        } else {
            let declarations: Vec<JsText> =
                model.today_marker.split(|&c| c == u16::from(b',')).map(JsText::from).collect();
            resolve_style(declarations.iter()).line
        };
        out.layer(LayerSpec::class("pele-today"), |out| {
            let line = PathProgram::default().m(Number::num(tx), Number::num(top)).v(Number::num(plot_bottom));
            let arrow = style.is_none();
            out.element(diagram, Facet::Shape)
                .path(line)
                .fill(Paint::None)
                .stroke(Paint::Role(Role::Accent))
                .stroke_width(Number::raw(2.0))
                .style(style)
                .finish();
            if arrow {
                let head = PathProgram::default()
                    .m(Number::num(tx - 4.0), Number::num(top - 5.0))
                    .h(Number::num(tx + 4.0))
                    .l(Number::num(tx), Number::num(top + 1.0))
                    .z();
                out.element(diagram, Facet::Shape).path(head).fill(Paint::Role(Role::Accent)).finish();
            }
        });
    }

    let policy = &cx.options.resources;
    let rel = (!policy.link_rel.is_empty()).then(|| policy.link_rel.clone());
    out.layer(LayerSpec::class("pele-tasks").font_size(Number::num(label_size)).keep_empty(), |out| {
        for bar in &bars {
            let task = &model.tasks[bar.task];
            let part = parts[bar.task];
            let cy = plot_top + bar.row as f64 * row_height + row_height / 2.0;
            let tags = task.tags;
            let state = if tags.active {
                "active"
            } else if tags.done {
                "done"
            } else {
                ""
            };
            let mut classes = String::from("pele-task");
            if !state.is_empty() {
                classes.push_str(" pele-");
                classes.push_str(state);
            }
            if tags.crit {
                classes.push_str(" pele-crit");
            }
            if tags.milestone {
                classes.push_str(" pele-milestone");
            }
            if task.classes.iter().any(|class| class.is_str("clickable")) {
                classes.push_str(" pele-clickable");
            }
            if bar.span.is_none() {
                classes.push_str(" pele-unscheduled");
            }
            let text_fill = match bar.span {
                None => Some(Paint::Role(Role::Muted)),
                Some(_) if bar.inside => (state.is_empty()).then_some(Paint::Role(Role::Background)),
                Some(_) => (state == "done").then_some(Paint::Role(Role::Muted)),
            };
            let body = |out: &mut SceneBuilder| {
                if let Some((x0, x1)) = bar.span {
                    // A critical task takes the critical color in place of the accent. Done, it keeps the fill
                    // of finished work and only its border is in that color.
                    let colour = Paint::Role(if tags.crit { Role::Critical } else { Role::Accent });
                    let fill = if state == "done" { Paint::Role(Role::SurfaceAlt) } else { colour };
                    let stroke = if state == "done" && !tags.crit { Paint::Role(Role::Border) } else { colour };
                    let shape = if tags.milestone {
                        let cx = (x0 + x1) / 2.0;
                        let r = bar_height / 2.0;
                        let path = PathProgram::default()
                            .m(Number::num(cx), Number::num(cy - r))
                            .l(Number::num(cx + r), Number::num(cy))
                            .l(Number::num(cx), Number::num(cy + r))
                            .l(Number::num(cx - r), Number::num(cy))
                            .z();
                        out.element(part, Facet::Shape).path(path)
                    } else {
                        out.element(part, Facet::Shape).rect(
                            [Number::num(x0), Number::num(cy - bar_height / 2.0)],
                            [Number::num(x1 - x0), Number::num(bar_height)],
                            None,
                        )
                    };
                    let mut shape = shape.fill(fill);
                    if state == "active" {
                        shape = shape.fill_opacity(Number::raw(0.25));
                    }
                    let shape = shape.stroke(stroke);
                    if tags.milestone {
                        shape.join(LineJoin::Round).finish();
                    } else {
                        shape.style(Some(Style::rounded(None))).finish();
                    }
                }
                let look = LabelLook::class("pele-label").fill(text_fill);
                out.label(part, Facet::Label, &bar.label, Point { x: bar.label_x, y: cy }, look);
            };
            let spec = LayerSpec::class(&classes).part(part, Facet::Shape).data_id();
            out.layer(spec.keep_empty(), |out| {
                match model.links.get(&task.id).filter(|url| link_allowed(url, policy)) {
                    Some(href) => out.link_to(part, href.clone(), None, rel.clone(), body),
                    None => body(out),
                }
            });
        }
    });

    out.layer(LayerSpec::class("pele-section-labels").font_size(Number::num(section_size)).keep_empty(), |out| {
        for band in &bands {
            let label = &section_labels[&band.section];
            let part = out.part(Entity::group(band.section));
            let at = Point { x: pad + 8.0 + label.width / 2.0, y: band.y + band.h / 2.0 };
            out.label(part, Facet::Label, label, at, LabelLook::class("pele-section-label").data_id());
        }
    });

    let axis = LayerSpec::class("pele-axis").font_size(Number::raw(tick_size)).fill(Paint::Role(Role::Muted));
    out.layer(axis.keep_empty(), |out| {
        let rows: &[f64] = if top_axis { &[bottom_y, top_y] } else { &[bottom_y] };
        for &y in rows {
            for label in &axis_labels {
                out.text(diagram, Facet::Label)
                    .at([Number::num(label.x), Number::num(y + tick_size * 0.35)])
                    .anchor(AnchorKind::Middle)
                    .finish(&label.text, label.width, tick_size);
            }
        }
    });

    out.layer(LayerSpec::class("pele-vert-labels").font_size(Number::num(label_size)), |out| {
        for vert in &vert_list {
            let look = LabelLook::class("pele-vert-label").data_id();
            out.label(parts[vert.task], Facet::Label, &vert.label, vert.centre, look);
        }
    });

    out.finish()
}
