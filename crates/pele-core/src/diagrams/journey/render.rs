//! Pele's `renderJourney` (`journey/render.ts`): each task a card with its actors' dots, a face
//! for its score on a chart of five levels, the sections as bands over their tasks, and a legend
//! of the actors. Where the host is too narrow, the tasks are drawn as steps running down.

use super::{JourneyModel, JourneyTask};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::fit::turn_to_fit;
use crate::input::{JsText, Units};
use crate::measure::{Emphasis, Label, fit_label, plain_label, width as measured};
use crate::scene::{
    Accessible, AnchorKind, Entity, Facet, LabelLook, LayerSpec, LineCap, LineJoin, Number, Paint, PartId, PathProgram,
    Point, Role, Scene, SceneBuilder, SeriesSlot, ShapeElement, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, js};
use std::collections::HashMap;

const PAD_X: f64 = 10.0;
const PAD_Y: f64 = 8.0;
const MIN_TEXT: f64 = 124.0;
const MAX_TEXT: f64 = 200.0;
const GAP: f64 = 12.0;
const BAND_PAD: f64 = 6.0;
const GUTTER: f64 = 24.0;
const LEVEL: f64 = 26.0;
const FACE: f64 = 11.0;
const DOT: f64 = 5.0;
const DOT_STEP: f64 = 13.0;
const LEGEND_GAP: f64 = 20.0;

/// The face a score draws.
#[derive(Clone, Copy)]
enum Mood {
    Happy,
    Neutral,
    Sad,
}

impl Mood {
    fn of(score: f64) -> Mood {
        if score > 3.0 {
            Mood::Happy
        } else if score < 3.0 {
            Mood::Sad
        } else {
            Mood::Neutral
        }
    }

    fn class(self) -> &'static str {
        match self {
            Mood::Happy => "pele-face pele-face-happy",
            Mood::Neutral => "pele-face pele-face-neutral",
            Mood::Sad => "pele-face pele-face-sad",
        }
    }

    /// Pele's `MOUTHS`.
    fn mouth(self) -> PathProgram {
        let raw = Number::raw;
        match self {
            Mood::Happy => PathProgram::default().m(raw(-5.0), raw(2.0)).a([raw(6.0); 2], false, [raw(5.0), raw(2.0)]),
            Mood::Neutral => PathProgram::default().m(raw(-4.5), raw(4.5)).h(raw(4.5)),
            Mood::Sad => PathProgram::default().m(raw(-5.0), raw(6.5)).a([raw(6.0); 2], true, [raw(5.0), raw(6.5)]),
        }
    }
}

/// Pele's `face`: a circle with two eyes and a mouth, centred on `at`.
fn face(out: &mut SceneBuilder, part: PartId, at: Point, mood: Mood) {
    out.layer(LayerSpec::class(mood.class()).translate(at), |out| {
        out.element(part, Facet::Annotation)
            .circle(Number::raw(FACE))
            .fill(Paint::Role(Role::Background))
            .stroke(Paint::Role(Role::Line))
            .finish();
        out.layer(LayerSpec::group().fill(Paint::Role(Role::Line)), |out| {
            for x in [-3.6, 3.6] {
                let centre = [Some(Number::raw(x)), Some(Number::raw(-2.8))];
                out.element(part, Facet::Annotation).circle_at(centre, Number::raw(1.3)).finish();
            }
        });
        out.element(part, Facet::Annotation)
            .path(mood.mouth())
            .fill(Paint::None)
            .stroke(Paint::Role(Role::Line))
            .cap(LineCap::Round)
            .finish();
    });
}

/// A line of the diagram with nothing inside it.
fn open_line<'a>(out: &'a mut SceneBuilder, class: &str, path: PathProgram, stroke: Role) -> ShapeElement<'a> {
    out.decoration().class(class).path(path).fill(Paint::None).stroke(Paint::Role(stroke))
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

fn series(index: usize) -> Paint {
    Paint::Series(SeriesSlot::new(index))
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

/// The actors that are drawn, each with its part, in code unit order. `Task: 5:` names an actor
/// with no name; Mermaid gives it a dot and an empty legend row, and Pele draws neither.
fn actor_parts<'a>(out: &mut SceneBuilder, model: &'a JourneyModel) -> Vec<(&'a JsText, PartId)> {
    let names = model.actors().into_iter().filter(|name| !name.is_empty());
    names.map(|name| (name, out.part(Entity::series(name).tooltip(Some(name))))).collect()
}

/// A legend row.
struct Row<'a> {
    name: &'a JsText,
    label: Label,
    x: f64,
    y: f64,
    h: f64,
}

/// A run of tasks that share a section, as in Mermaid. A section with no tasks draws nothing.
struct Run {
    start: usize,
    count: usize,
    label: Option<Label>,
}

#[allow(clippy::too_many_lines, reason = "one pass of Pele's draw, whose steps share its sizes")]
fn draw(model: &JourneyModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let tiny = js::round(size * 0.75);
    let measurer = cx.measure;
    let pad = cx.options.padding;
    let tasks = &model.tasks;
    let n = tasks.len();
    let names: Vec<&JsText> = model.actors().into_iter().filter(|name| !name.is_empty()).collect();
    let actor: HashMap<&[u16], usize> = names.iter().enumerate().map(|(i, name)| (&name[..], i)).collect();
    let plain = Emphasis::default();

    // Cards widen, up to a point, for a word that would not fit; past that the word is cut.
    let mut text_w = MIN_TEXT;
    let mut labels: Vec<Label> =
        tasks.iter().map(|task| plain_label(Some(&task.task), measurer, small, text_w, plain)).collect();
    let mut widest: f64 = 0.0;
    for label in &labels {
        widest = js::max(widest, label.width);
    }
    if widest > text_w {
        text_w = js::min(widest.ceil(), MAX_TEXT);
        labels = tasks.iter().map(|task| fit_label(Some(&task.task), measurer, small, text_w, plain)).collect();
    }

    let card_w = text_w + 2.0 * PAD_X;
    let pitch = card_w + GAP;
    let plot_w = if n > 0 { n as f64 * pitch - GAP } else { 0.0 };

    let line_h = js::round(small * 1.5);
    let limit = js::max(plot_w, 240.0);
    let mut legend: Vec<Row<'_>> = Vec::new();
    let (mut legend_w, mut legend_h, mut lx): (f64, f64, f64) = (0.0, 0.0, 0.0);
    for &name in &names {
        let label = fit_label(Some(name), measurer, small, limit - 2.0 * DOT - 6.0, plain);
        let w = 2.0 * DOT + 6.0 + label.width;
        if lx > 0.0 && lx + w > limit {
            lx = 0.0;
            legend_h += 4.0;
        }
        let y = if lx > 0.0 { legend.last().map_or(legend_h, |row| row.y) } else { legend_h };
        let h = js::max(label.height, line_h);
        legend.push(Row { name, label, x: lx, y, h });
        lx += w;
        legend_w = js::max(legend_w, lx);
        legend_h = js::max(legend_h, y + h);
        lx += LEGEND_GAP;
    }

    let body_w = if n > 0 { GUTTER + plot_w } else { 0.0 };
    let title = fit_label(model.title.as_deref(), measurer, size, js::max(body_w, 320.0), Emphasis::BOLD);
    let title_h = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let width = js::max(js::max(body_w, title.width), if n > 0 { GUTTER + legend_w } else { 0.0 });
    let left = pad + (width - body_w) / 2.0 + GUTTER;
    let top = pad + title_h;

    let mut runs: Vec<Run> = Vec::new();
    let mut band_h: f64 = 0.0;
    let mut i = 0;
    while i < n {
        let mut end = i + 1;
        while end < n && tasks[end].section == tasks[i].section {
            end += 1;
        }
        let mut label = None;
        if !tasks[i].section.is_empty() {
            let room = (end - i) as f64 * pitch - GAP - 2.0 * PAD_X;
            let section = fit_label(Some(&tasks[i].section), measurer, small, room, Emphasis::BOLD);
            band_h = js::max(band_h, js::max(section.height, line_h) + 2.0 * BAND_PAD);
            label = Some(section);
        }
        runs.push(Run { start: i, count: end - i, label });
        i = end;
    }

    let mut text_h = line_h;
    for label in &labels {
        text_h = js::max(text_h, label.height);
    }
    let dots_h = if names.is_empty() { 0.0 } else { 2.0 * DOT + 6.0 };
    let card_h = text_h + dots_h + 2.0 * PAD_Y;
    let cards_y = top + if band_h > 0.0 { band_h + BAND_PAD } else { 0.0 };
    let chart_top = cards_y + card_h + FACE + 16.0;
    let chart_bottom = chart_top + 4.0 * LEVEL;
    let right = left + plot_w;

    let mut bottom = top;
    if n > 0 {
        bottom = chart_bottom + FACE;
    }
    let legend_at = (!legend.is_empty() && n > 0).then(|| {
        let at = Point { x: left + (plot_w - legend_w) / 2.0, y: bottom + 16.0 };
        bottom = at.y + legend_h;
        at
    });

    let total = Size { width: (width + 2.0 * pad).ceil(), height: (js::max(bottom, pad) + pad).ceil() };
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, total, accessible);
    out.title(&TitleLayout { at: Point { x: pad + width / 2.0, y: pad + title.height / 2.0 }, label: title });
    if n == 0 {
        return out.finish();
    }
    let actors = actor_parts(&mut out, model);
    let task_ranks = ranks(tasks.iter().map(|task| &task.task[..]));
    let task_parts: Vec<PartId> = tasks
        .iter()
        .zip(&task_ranks)
        .map(|(task, &rank)| out.part(Entity::node(&JsText::from(task.task.trim_js())).ranked(rank)))
        .collect();

    let mut grid = PathProgram::default();
    for score in (1..=5).rev() {
        let y = chart_top + f64::from(5 - score) * LEVEL;
        grid = grid.m(Number::num(left - 6.0), Number::num(y)).h(Number::num(right));
    }
    out.decoration().class("pele-grid").path(grid).fill(Paint::None).stroke(Paint::Role(Role::SurfaceAlt)).finish();
    let scale = LayerSpec::class("pele-scale")
        .font_size(Number::raw(tiny))
        .anchor(AnchorKind::End)
        .fill(Paint::Role(Role::Muted));
    out.layer(scale, |out| {
        let part = out.part(Entity::diagram());
        for score in (1..=5).rev() {
            let y = chart_top + f64::from(5 - score) * LEVEL;
            let text: Vec<u16> = score.to_string().encode_utf16().collect();
            let label =
                Label::single(&text, plain, measured(measurer, &text, tiny, plain), tiny, js::round(tiny * 1.5));
            out.baseline_text(part, label, pair(left - 12.0, y + tiny * 0.35), None);
        }
    });

    let level = |task: &JourneyTask| {
        let level = if task.score.is_nan() { 3.0 } else { js::min(5.0, js::max(1.0, task.score)) };
        chart_top + (5.0 - level) * LEVEL
    };
    let centre = |i: usize| left + i as f64 * pitch + card_w / 2.0;
    let mut line = PathProgram::default();
    let mut stems = PathProgram::default();
    for (i, task) in tasks.iter().enumerate() {
        let (x, y) = (Number::num(centre(i)), Number::num(level(task)));
        line = if i == 0 { line.m(x, y) } else { line.l(x, y) };
        stems = stems.m(x, Number::num(cards_y + card_h)).v(Number::num(level(task) - FACE));
    }
    open_line(&mut out, "pele-stems", stems, Role::Border).dash(&[Number::raw(2.0), Number::raw(3.0)]).finish();
    if n > 1 {
        open_line(&mut out, "pele-edge", line, Role::Line).join(LineJoin::Round).finish();
    }
    for (i, task) in tasks.iter().enumerate() {
        face(&mut out, task_parts[i], Point { x: centre(i), y: level(task) }, Mood::of(task.score));
    }

    let part_of = |name: &JsText| actors.iter().find(|(actor, _)| *actor == name).map(|&(_, part)| part);
    let card = |out: &mut SceneBuilder, i: usize| {
        let task = &tasks[i];
        let part = task_parts[i];
        let cx = centre(i);
        out.layer(LayerSpec::class("pele-node pele-task").part(part, Facet::Shape).data_id(), |out| {
            out.element(part, Facet::Shape)
                .rect(pair(left + i as f64 * pitch, cards_y), pair(card_w, card_h), None)
                .fill(Paint::Role(Role::Surface))
                .stroke(Paint::Role(Role::Border))
                .style(Some(Style::rounded(None)))
                .finish();
            let look = LabelLook::default().font_size(Number::raw(small));
            out.label(part, Facet::Label, &labels[i], Point { x: cx, y: cards_y + PAD_Y + text_h / 2.0 }, look);
            let people: Vec<&JsText> = task.people.iter().filter(|name| !name.is_empty()).collect();
            let spread = (people.len() as f64 - 1.0).max(1.0);
            let step = js::min(DOT_STEP, (card_w - 2.0 * PAD_X - 2.0 * DOT) / spread);
            for (k, &name) in people.iter().enumerate() {
                let at = cx + (k as f64 - (people.len() as f64 - 1.0) / 2.0) * step;
                let colour = series(actor.get(&name[..]).copied().unwrap_or(0));
                let dot = part_of(name).expect("every named actor has a part");
                out.element(dot, Facet::Shape)
                    .class("pele-actor")
                    .circle_at(
                        [Some(Number::num(at)), Some(Number::num(cards_y + card_h - PAD_Y - DOT))],
                        Number::raw(DOT),
                    )
                    .fill(colour)
                    .stroke(Paint::Role(Role::Surface))
                    .titled()
                    .finish();
            }
        });
    };
    let band_ranks = ranks(runs.iter().filter(|run| run.label.is_some()).map(|run| &tasks[run.start].section[..]));
    let mut bands = band_ranks.into_iter();
    for run in &runs {
        let members = run.start..run.start + run.count;
        let Some(label) = &run.label else {
            for i in members {
                card(&mut out, i);
            }
            continue;
        };
        let x = left + run.start as f64 * pitch;
        let w = run.count as f64 * pitch - GAP;
        let name = JsText::from(tasks[run.start].section.trim_js());
        let part = out.part(Entity::group(&name).ranked(bands.next().expect("a rank for each band")));
        out.layer(LayerSpec::class("pele-cluster pele-section").part(part, Facet::Shape).data_id(), |out| {
            out.element(part, Facet::Shape)
                .rect(pair(x, top), pair(w, band_h), None)
                .fill(Paint::Role(Role::SurfaceAlt))
                .style(Some(Style::rounded(None)))
                .finish();
            let look = LabelLook::default().font_size(Number::raw(small)).weight(TextWeight::Heading);
            out.label(part, Facet::Label, label, Point { x: x + w / 2.0, y: top + band_h / 2.0 }, look);
            for i in members {
                card(out, i);
            }
        });
    }

    if let Some(origin) = legend_at {
        out.layer(LayerSpec::class("pele-legend").font_size(Number::raw(small)), |out| {
            for row in &legend {
                let part = part_of(row.name).expect("every legend row is an actor");
                let cy = origin.y + row.y + row.h / 2.0;
                out.layer(LayerSpec::class("pele-legend-item").part(part, Facet::Legend).data_id(), |out| {
                    out.element(part, Facet::Legend)
                        .circle_at([Some(Number::num(origin.x + row.x + DOT)), Some(Number::num(cy))], Number::raw(DOT))
                        .fill(series(actor[&row.name[..]]))
                        .finish();
                    let at = Point { x: origin.x + row.x + 2.0 * DOT + 6.0 + row.label.width / 2.0, y: cy };
                    out.label(part, Facet::Legend, &row.label, at, LabelLook::default());
                });
            }
        });
    }
    out.finish()
}

/// One task as a step of the journey running down.
struct Step<'a> {
    head: Option<(Label, f64)>,
    task: &'a JourneyTask,
    label: Label,
    people: Vec<(&'a JsText, Label)>,
    y: f64,
    w: f64,
    h: f64,
    text_h: f64,
}

/// Pele's `verticalJourney`.
fn vertical(model: &JourneyModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let measurer = cx.measure;
    let pad = cx.options.padding;
    let plain = Emphasis::default();
    let room = js::max(140.0, cx.options.max_width.unwrap_or(320.0) - 2.0 * pad);
    let title = fit_label(model.title.as_deref(), measurer, size, room, Emphasis::BOLD);
    let names: Vec<&JsText> = model.actors().into_iter().filter(|name| !name.is_empty()).collect();
    let actor: HashMap<&[u16], usize> = names.iter().enumerate().map(|(i, name)| (&name[..], i)).collect();
    let mut y = pad + if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let mut section: Option<&JsText> = None;
    let mut width = title.width;
    let mut steps = Vec::with_capacity(model.tasks.len());
    for task in &model.tasks {
        let mut head = None;
        if section != Some(&task.section) {
            section = Some(&task.section);
            let label = fit_label(Some(&task.section), measurer, size, room - 16.0, Emphasis::BOLD);
            if label.height > 0.0 {
                width = js::max(width, label.width + 16.0);
                let top = y;
                y += label.height + 8.0;
                head = Some((label, top));
            }
        }
        let label = fit_label(Some(&task.task), measurer, small, room - 72.0, plain);
        let people: Vec<(&JsText, Label)> = task
            .people
            .iter()
            .filter(|name| !name.is_empty())
            .map(|name| (name, fit_label(Some(name), measurer, small, room - 44.0, plain)))
            .collect();
        let text_h = js::max(label.height, 42.0);
        let mut h = 2.0 * PAD_Y + text_h;
        for (_, person) in &people {
            h += js::max(16.0, person.height) + 4.0;
        }
        let w = js::max(room, label.width + 72.0);
        steps.push(Step { head, task, label, people, y, w, h, text_h });
        y += h + 16.0;
        width = js::max(width, w);
    }

    let total_width = (width + 2.0 * pad).ceil();
    let last = if model.tasks.is_empty() { 0.0 } else { 16.0 };
    let height = (js::max(pad + title.height, y - last) + pad).ceil();
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width: total_width, height }, accessible);
    out.title(&TitleLayout { at: Point { x: total_width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let actors = actor_parts(&mut out, model);
    let part_of = |name: &JsText| actors.iter().find(|(actor, _)| *actor == name).map(|&(_, part)| part);
    let task_ranks = ranks(model.tasks.iter().map(|task| &task.task[..]));
    let section_ranks = ranks(steps.iter().filter(|step| step.head.is_some()).map(|step| &step.task.section[..]));
    let mut heads = section_ranks.into_iter();
    for (index, step) in steps.iter().enumerate() {
        if let Some((label, top)) = &step.head {
            let name = JsText::from(step.task.section.trim_js());
            let part = out.part(Entity::group(&name).ranked(heads.next().expect("a rank for each head")));
            let look = LabelLook::class("pele-cluster-label").weight(TextWeight::Heading);
            out.label(
                part,
                Facet::Label,
                label,
                Point { x: pad + 8.0 + label.width / 2.0, y: top + label.height / 2.0 },
                look,
            );
        }
        let written = JsText::from(index.to_string().as_str());
        let entity =
            Entity::node_written_as(&JsText::from(step.task.task.trim_js()), &written).ranked(task_ranks[index]);
        let part = out.part(entity);
        let y = step.y;
        out.layer(LayerSpec::class("pele-node pele-journey-step").part(part, Facet::Shape).data_id(), |out| {
            out.element(part, Facet::Shape)
                .rect(pair(pad, y), pair(step.w, step.h), None)
                .fill(Paint::Role(Role::Surface))
                .stroke(Paint::Role(Role::Border))
                .style(Some(Style::rounded(None)))
                .finish();
            let at = Point { x: pad + PAD_X + step.label.width / 2.0, y: y + PAD_Y + step.label.height / 2.0 };
            out.label(part, Facet::Label, &step.label, at, LabelLook::class("pele-label"));
            let cx = pad + step.w - 28.0;
            face(out, part, Point { x: cx, y: y + PAD_Y + FACE }, Mood::of(step.task.score));
            if !step.task.score.is_nan() {
                let score = js::num(js::min(5.0, js::max(1.0, step.task.score)));
                let text: Vec<u16> = score.encode_utf16().collect();
                out.text(part, Facet::Annotation)
                    .class("pele-journey-score")
                    .at(pair(cx, y + PAD_Y + 2.0 * FACE + small))
                    .font_size(Number::raw(small))
                    .anchor(AnchorKind::Middle)
                    .finish(&text, measured(measurer, &text, small, plain), small);
            }
            let mut py = y + PAD_Y + step.text_h + 4.0;
            for (name, person) in &step.people {
                let ph = js::max(16.0, person.height);
                let dot = part_of(name).expect("every named actor has a part");
                out.element(dot, Facet::Shape)
                    .class("pele-actor")
                    .circle_at([Some(Number::num(pad + 16.0)), Some(Number::num(py + ph / 2.0))], Number::raw(DOT))
                    .fill(series(actor[&name[..]]))
                    .finish();
                let look = LabelLook::class("pele-journey-actor").font_size(Number::raw(small));
                out.label(
                    dot,
                    Facet::Label,
                    person,
                    Point { x: pad + 28.0 + person.width / 2.0, y: py + ph / 2.0 },
                    look,
                );
                py += ph + 4.0;
            }
        });
        if index + 1 < steps.len() {
            let path = PathProgram::default()
                .m(Number::num(pad + step.w / 2.0), Number::num(y + step.h))
                .v_by(Number::raw(16.0));
            out.decoration().class("pele-edge").path(path).fill(Paint::None).stroke(Paint::Role(Role::Line)).finish();
        }
    }
    out.finish()
}

pub(crate) struct Journey;

impl Diagram for Journey {
    type Model = JourneyModel;

    fn parse(cx: &ParseContext<'_>) -> Result<JourneyModel, RenderError> {
        super::parse(cx)
    }

    fn scene(model: &JourneyModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        turn_to_fit(cx.options, |down| if down { vertical(model, cx) } else { draw(model, cx) })
    }
}
