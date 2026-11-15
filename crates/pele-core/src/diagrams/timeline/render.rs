//! Pele's `renderTimeline` (`timeline/render.ts`): an axis with an arrowhead, each period a chip
//! on it with a dot, its events as cards beside it, and each section a header over its periods.
//! A diagram that runs across is drawn running down where that fits the host better.

use super::{Direction, TimelineModel};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::fit::turn_to_fit;
use crate::graph::edges::{ARROW, Marker, MarkerUse, Tip, draw_marker};
use crate::input::{JsText, JsValue, Units};
use crate::measure::{Emphasis, Label, fit_label, plain_label};
use crate::scene::{
    Accessible, Entity, Facet, LabelLook, LayerSpec, Number, Paint, PartId, PathProgram, Point, Role, Scene,
    SceneBuilder, SeriesSlot, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, js};
use std::collections::HashMap;

const PAD_X: f64 = 12.0;
const PAD_Y: f64 = 8.0;
const MIN_TEXT: f64 = 152.0;
const MAX_TEXT: f64 = 216.0;
const EVENT_TEXT_TD: f64 = 264.0;
const GAP: f64 = 20.0;
const AXIS: f64 = 20.0;
const SPINE: f64 = 14.0;
const EVENT_GAP: f64 = 8.0;
const SECTION_GAP: f64 = 8.0;
const TAIL: f64 = 12.0;

/// The periods drawn together: those of one section, or those before the first section.
struct Group {
    section: Option<usize>,
    periods: Vec<usize>,
}

/// A rounded box with a label centred in it.
struct Chip {
    label: Label,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// A period as it is drawn: its chip, the line from it to its events, its dot on the axis and
/// its event cards.
struct PeriodView {
    index: usize,
    colour: Paint,
    ink: Paint,
    chip: Chip,
    line: PathProgram,
    dot: Point,
    events: Vec<Chip>,
}

struct GroupView {
    section: Option<usize>,
    header: Option<Chip>,
    periods: Vec<PeriodView>,
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

/// Each name's place among the earlier ones that are the same once trimmed, so that a key does
/// not move when another entity is added after it.
fn ranks<'a>(names: impl Iterator<Item = &'a [u16]>) -> Vec<u32> {
    let mut seen: HashMap<&[u16], u32> = HashMap::new();
    names
        .map(|name| {
            let rank = seen.entry(name.trim_js()).or_insert(0);
            *rank += 1;
            *rank - 1
        })
        .collect()
}

#[allow(clippy::too_many_lines, reason = "one pass of Pele's draw, whose steps share its sizes")]
fn draw(model: &TimelineModel, cx: &DrawContext<'_>, vertical: bool) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let measurer = cx.measure;
    let pad = cx.options.padding;
    let disabled =
        cx.config.get("timeline").and_then(JsValue::as_record).and_then(|section| section.get("disableMulticolor"));
    let multicolor = disabled != Some(&JsValue::Bool(true));
    let periods = &model.periods;
    let has_sections = !model.sections.is_empty();

    // Mermaid draws nothing for periods that come before the first section; here they lead, in a
    // neutral colour.
    let mut groups: Vec<Group> =
        (0..model.sections.len()).map(|section| Group { section: Some(section), periods: Vec::new() }).collect();
    let mut leading = Group { section: None, periods: Vec::new() };
    for (i, period) in periods.iter().enumerate() {
        match period.section_index {
            Some(section) => groups[section].periods.push(i),
            None => leading.periods.push(i),
        }
    }
    if !leading.periods.is_empty() {
        groups.insert(0, leading);
    }

    // Columns widen, up to a point, for a word that would not fit; past that the word is cut.
    // Running down, the two columns share the width the host has.
    let room = match cx.options.max_width {
        Some(width) if vertical && width > 0.0 => width - 2.0 * pad,
        _ => f64::INFINITY,
    };
    let taken = AXIS + SPINE * 2.0 + 4.0 * PAD_X;
    let period_text = js::min(MIN_TEXT, js::max(80.0, js::round((room - taken) * 0.38)));
    let event_text = js::min(EVENT_TEXT_TD, js::max(110.0, room - taken - period_text));
    let mut text_w = period_text;
    let event_text_w = |text_w: f64| if vertical { event_text } else { text_w };
    let plain = Emphasis::default();
    let mut period_labels: Vec<Label> =
        periods.iter().map(|p| plain_label(Some(&p.text), measurer, size, text_w, plain)).collect();
    let mut event_labels: Vec<Vec<Label>> = periods
        .iter()
        .map(|p| p.events.iter().map(|e| plain_label(Some(e), measurer, small, event_text_w(text_w), plain)).collect())
        .collect();
    let mut widest: f64 = 0.0;
    for label in &period_labels {
        widest = js::max(widest, label.width);
    }
    if !vertical {
        for label in event_labels.iter().flatten() {
            widest = js::max(widest, label.width);
        }
    }
    let mut overflow = widest > text_w;
    if vertical {
        for label in event_labels.iter().flatten() {
            overflow = overflow || label.width > event_text;
        }
    }
    if overflow {
        let most = if room == f64::INFINITY { MAX_TEXT } else { period_text };
        text_w = js::min(js::max(widest.ceil(), period_text), most);
        period_labels = periods.iter().map(|p| fit_label(Some(&p.text), measurer, size, text_w, plain)).collect();
        event_labels = periods
            .iter()
            .map(|p| {
                p.events.iter().map(|e| fit_label(Some(e), measurer, small, event_text_w(text_w), plain)).collect()
            })
            .collect();
    }

    let line_height = js::round(size * 1.5);
    let col_w = text_w + 2.0 * PAD_X;
    let event_w = event_text_w(text_w) + 2.0 * PAD_X;
    let chip_height = |label: &Label| js::max(label.height, line_height) + 2.0 * PAD_Y;
    let event_height = |label: &Label| js::max(label.height, js::round(small * 1.5)) + 2.0 * PAD_Y;
    let series_of = |group: &Group, period: usize| match group.section {
        Some(section) => Some(section),
        None if has_sections => None,
        None => Some(if multicolor { period } else { 0 }),
    };
    let colour_of = |series: Option<usize>| match series {
        Some(series) => Paint::Series(SeriesSlot::new(series)),
        None => Paint::Role(Role::Muted),
    };
    // Pele's `onColor`: the palette's text colour for the series where it knows one, else the page's.
    let on_colour = |series: Option<usize>| match series.map(SeriesSlot::new) {
        Some(slot) if cx.palette.ink(slot).is_some() => Paint::OnSeries(slot),
        _ => Paint::Role(Role::Background),
    };

    let has_events = event_labels.iter().any(|labels| !labels.is_empty());
    let slots: usize = groups.iter().map(|group| group.periods.len().max(1)).sum();
    let pitch = col_w + GAP;
    let drawn = !periods.is_empty();
    let body_w = if vertical {
        col_w
            + if has_events {
                AXIS + SPINE * 2.0 + event_w
            } else if drawn {
                AXIS + TAIL
            } else {
                0.0
            }
    } else if slots > 0 {
        slots as f64 * pitch - GAP + if drawn { TAIL + ARROW } else { 0.0 }
    } else {
        0.0
    };
    let title = fit_label(model.title.as_deref(), measurer, size, js::max(body_w, 320.0), Emphasis::BOLD);
    let title_h = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let width = js::max(body_w, title.width);
    let left = pad + (width - body_w) / 2.0;
    let top = pad + title_h;

    let section_label = |section: usize, w: f64| {
        fit_label(Some(&model.sections[section]), measurer, size, w - 2.0 * PAD_X, Emphasis::BOLD)
    };

    let mut views: Vec<GroupView> = Vec::with_capacity(groups.len());
    let axis: Option<(PathProgram, Tip)>;
    let height;
    if vertical {
        let axis_x = left + col_w + AXIS;
        let spine_x = axis_x + SPINE;
        let events_x = spine_x + SPINE;
        let mut y = top;
        for group in &groups {
            let mut view = GroupView { section: group.section, header: None, periods: Vec::new() };
            if let Some(section) = group.section {
                let label = section_label(section, body_w);
                let h = chip_height(&label);
                view.header = Some(Chip { label, x: left, y, w: body_w, h });
                y += h + SECTION_GAP + 4.0;
            }
            for &i in &group.periods {
                let series = series_of(group, i);
                let h = chip_height(&period_labels[i]);
                // A period and its first event share one center line, so the line between them
                // runs straight.
                let first = event_labels[i].first().map_or(h, event_height);
                let cy = y + js::max(h, first) / 2.0;
                let mut line =
                    PathProgram::default().m(Number::num(left + col_w), Number::num(cy)).h(Number::num(axis_x));
                let mut events = Vec::new();
                let mut ey = cy - first / 2.0;
                let mut last = cy;
                for label in &event_labels[i] {
                    let eh = event_height(label);
                    last = ey + eh / 2.0;
                    line = line.m(Number::num(spine_x), Number::num(last)).h(Number::num(events_x));
                    events.push(Chip { label: label.clone(), x: events_x, y: ey, w: event_w, h: eh });
                    ey += eh + EVENT_GAP;
                }
                if !events.is_empty() {
                    line = line.m(Number::num(axis_x), Number::num(cy)).h(Number::num(spine_x)).v(Number::num(last));
                }
                let chip = Chip { label: period_labels[i].clone(), x: left, y: cy - h / 2.0, w: col_w, h };
                let (colour, ink, dot) = (colour_of(series), on_colour(series), Point { x: axis_x, y: cy });
                view.periods.push(PeriodView { index: i, colour, ink, chip, line, dot, events });
                y = js::max(cy + h / 2.0, ey - EVENT_GAP) + GAP;
            }
            if group.periods.is_empty() {
                y += GAP - SECTION_GAP - 4.0;
            }
            views.push(view);
        }
        if !groups.is_empty() {
            y -= GAP;
        }
        axis = drawn.then(|| {
            let tip = y + TAIL + ARROW;
            y = tip;
            let path =
                PathProgram::default().m(Number::num(axis_x), Number::num(top)).v(Number::num(tip - ARROW + 1.0));
            (path, Tip { x: axis_x, y: tip, dx: 0.0, dy: 1.0 })
        });
        height = y + pad;
    } else {
        let mut header_h: f64 = 0.0;
        let section_labels: Vec<Option<Label>> = groups
            .iter()
            .map(|group| {
                let label = section_label(group.section?, group.periods.len().max(1) as f64 * pitch - GAP);
                header_h = js::max(header_h, chip_height(&label));
                Some(label)
            })
            .collect();
        let mut chip_h: f64 = 0.0;
        for label in &period_labels {
            chip_h = js::max(chip_h, chip_height(label));
        }
        let periods_y = top + if has_sections { header_h + SECTION_GAP } else { 0.0 };
        let axis_y = periods_y + chip_h + if chip_h > 0.0 { AXIS } else { 0.0 };
        let events_y = axis_y + AXIS;
        let mut bottom = if drawn { axis_y + TAIL } else { top + header_h };
        let mut slot = 0;
        for (group, label) in groups.iter().zip(section_labels) {
            let span = group.periods.len().max(1);
            let gx = left + slot as f64 * pitch;
            let header = label.map(|label| Chip { label, x: gx, y: top, w: span as f64 * pitch - GAP, h: header_h });
            let mut view = GroupView { section: group.section, header, periods: Vec::new() };
            for (k, &i) in group.periods.iter().enumerate() {
                let series = series_of(group, i);
                let x = gx + k as f64 * pitch;
                let centre = x + col_w / 2.0;
                let end = if event_labels[i].is_empty() { axis_y } else { events_y };
                let mut line =
                    PathProgram::default().m(Number::num(centre), Number::num(periods_y + chip_h)).v(Number::num(end));
                let mut events = Vec::new();
                let mut ey = events_y;
                for label in &event_labels[i] {
                    let eh = event_height(label);
                    if ey > events_y {
                        line = line.m(Number::num(centre), Number::num(ey - EVENT_GAP)).v(Number::num(ey));
                    }
                    events.push(Chip { label: label.clone(), x, y: ey, w: event_w, h: eh });
                    ey += eh + EVENT_GAP;
                }
                bottom = js::max(bottom, ey - EVENT_GAP);
                let chip = Chip { label: period_labels[i].clone(), x, y: periods_y, w: col_w, h: chip_h };
                let (colour, ink, dot) = (colour_of(series), on_colour(series), Point { x: centre, y: axis_y });
                view.periods.push(PeriodView { index: i, colour, ink, chip, line, dot, events });
            }
            views.push(view);
            slot += span;
        }
        axis = drawn.then(|| {
            let tip = left + body_w;
            let path =
                PathProgram::default().m(Number::num(left), Number::num(axis_y)).h(Number::num(tip - ARROW + 1.0));
            (path, Tip { x: tip, y: axis_y, dx: 1.0, dy: 0.0 })
        });
        height = bottom + pad;
    }

    let total = Size { width: (width + 2.0 * pad).ceil(), height: js::max(height, pad * 2.0).ceil() };
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, total, accessible);
    out.title(&TitleLayout { at: Point { x: total.width / 2.0, y: pad + title.height / 2.0 }, label: title });

    if let Some((path, tip)) = axis {
        out.decoration().class("pele-axis").path(path).fill(Paint::None).stroke(Paint::Role(Role::Line)).finish();
        let part = out.part(Entity::diagram());
        draw_marker(&mut out, part, Facet::EndMarker, Marker::Point, tip, MarkerUse::Edge(Paint::Role(Role::Line)));
    }

    let section_ranks = ranks(model.sections.iter().map(|name| &name[..]));
    let period_ranks = ranks(periods.iter().map(|period| &period.text[..]));
    let chip_box = |out: &mut SceneBuilder, part: PartId, facet: Facet, chip: &Chip, fill: Paint, wash: Option<f64>| {
        let rect = out.element(part, facet).rect(pair(chip.x, chip.y), pair(chip.w, chip.h), None).fill(fill);
        let rect = match wash {
            Some(wash) => rect.fill_opacity(Number::raw(wash)),
            None => rect,
        };
        rect.style(Some(Style::rounded(None))).finish();
    };
    let centre = |chip: &Chip| Point { x: chip.x + chip.w / 2.0, y: chip.y + chip.h / 2.0 };
    let draw_period = |out: &mut SceneBuilder, period: &PeriodView| {
        let text: &JsText = &periods[period.index].text;
        let part = out.part(Entity::node(&JsText::from(text.trim_js())).ranked(period_ranks[period.index]));
        out.layer(LayerSpec::class("pele-node pele-period").part(part, Facet::Shape).data_id(), |out| {
            chip_box(out, part, Facet::Shape, &period.chip, period.colour, None);
            let look = LabelLook::default().fill(Some(period.ink));
            out.label(part, Facet::Label, &period.chip.label, centre(&period.chip), look);
            out.element(part, Facet::Shape)
                .class("pele-edge")
                .path(period.line.clone())
                .fill(Paint::None)
                .stroke(Paint::Role(Role::Border))
                .finish();
            out.element(part, Facet::Shape)
                .class("pele-dot")
                .circle_at([Some(Number::num(period.dot.x)), Some(Number::num(period.dot.y))], Number::raw(4.0))
                .fill(period.colour)
                .stroke(Paint::Role(Role::Background))
                .stroke_width(Number::raw(2.0))
                .finish();
            for event in &period.events {
                out.layer(LayerSpec::class("pele-node pele-event"), |out| {
                    chip_box(out, part, Facet::Attribute, event, period.colour, Some(0.16));
                    let look = LabelLook::default().font_size(Number::raw(small));
                    out.label(part, Facet::Attribute, &event.label, centre(event), look);
                });
            }
        });
    };
    for view in &views {
        let Some(section) = view.section else {
            for period in &view.periods {
                draw_period(&mut out, period);
            }
            continue;
        };
        let name: JsText = model.sections[section].trim_js().into();
        let part = out.part(Entity::group(&name).ranked(section_ranks[section]));
        out.layer(LayerSpec::class("pele-cluster pele-section").part(part, Facet::Shape).data_id(), |out| {
            if let Some(header) = &view.header {
                chip_box(out, part, Facet::Shape, header, Paint::Series(SeriesSlot::new(section)), None);
                let look = LabelLook::default().weight(TextWeight::Heading).fill(Some(on_colour(Some(section))));
                out.label(part, Facet::Label, &header.label, centre(header), look);
            }
            for period in &view.periods {
                draw_period(out, period);
            }
        });
    }
    out.finish()
}

pub(crate) struct Timeline;

impl Diagram for Timeline {
    type Model = TimelineModel;

    fn parse(cx: &ParseContext<'_>) -> Result<TimelineModel, RenderError> {
        super::parse(cx)
    }

    fn scene(model: &TimelineModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        match model.direction {
            Direction::Down => draw(model, cx, true),
            Direction::Across => turn_to_fit(cx.options, |down| draw(model, cx, down)),
        }
    }
}
