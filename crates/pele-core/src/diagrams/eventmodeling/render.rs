//! How an event model is drawn, ported from Pele's `eventmodeling/render.ts`. Across, time runs
//! left to right through lanes stacked by what their frames hold. Down, each frame is a card
//! under its lane's heading, and flows that skip cards run down rails beside them.

use super::model::{EmBox, EntityKind, EventModel, Relation};
use crate::catalog::DrawContext;
use crate::graph::edges::{Marker, MarkerUse, Tip, draw_marker};
use crate::input::{JsText, decode_entities};
use crate::measure::{Emphasis, Label, LabelLine, LabelRun, Script, break_label, layout_label, literal_label, width};
use crate::scene::{
    Accessible, AnchorKind, Entity, EntityName, Facet, LabelLook, LayerSpec, Number, Paint, PartId, PathProgram, Point,
    Role, Scene, SceneBuilder, SeriesSlot, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

const BOX_PAD_X: f64 = 12.0;
const BOX_PAD_Y: f64 = 10.0;
const BOX_MIN_WIDTH: f64 = 96.0;
const BOX_GAP: f64 = 16.0;
const STAGGER: f64 = 80.0;
const LANE_PAD: f64 = 12.0;
const LANE_GAP: f64 = 8.0;
const DATA_COLUMNS: usize = 48;
const NOTE_COLUMNS: usize = 80;
const CARD_GAP: f64 = 8.0;
const RAIL_GAP: f64 = 8.0;
const CARD_MIN: f64 = 120.0;
const ARROW_ROOM: f64 = 20.0;

/// Pele's `SERIES`: the series slot of each kind of entity. A UI has none and is drawn plain.
fn series(kind: EntityKind) -> Option<SeriesSlot> {
    match kind {
        EntityKind::Command => Some(SeriesSlot::new(0)),
        EntityKind::Event => Some(SeriesSlot::new(1)),
        EntityKind::ReadModel => Some(SeriesSlot::new(2)),
        EntityKind::Processor => Some(SeriesSlot::new(6)),
        EntityKind::Ui => None,
    }
}

const MONO: Emphasis = Emphasis { bold: false, italic: false, mono: true, underline: false, script: Script::Normal };

fn rounded() -> Option<Style> {
    Some(Style::rounded(None))
}

/// Pele's `hardWrap`: text in lines of at most `columns` characters, keeping its own line breaks.
fn hard_wrap(text: &[u16], columns: usize) -> Vec<JsText> {
    let mut out = Vec::new();
    for line in decode_entities(text).split(|&c| c == 10) {
        if line.len() <= columns {
            out.push(line.into());
            continue;
        }
        let points = code_points(line);
        if points.len() <= columns {
            out.push(line.into());
            continue;
        }
        for chunk in points.chunks(columns) {
            out.push(chunk.concat().as_slice().into());
        }
    }
    out
}

/// The text's code points, each as its code units. A lone surrogate is one point.
fn code_points(text: &[u16]) -> Vec<&[u16]> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let pair =
            (0xd800..0xdc00).contains(&text[at]) && text.get(at + 1).is_some_and(|c| (0xdc00..0xe000).contains(c));
        let length = if pair { 2 } else { 1 };
        out.push(&text[at..at + length]);
        at += length;
    }
    out
}

/// Rows of text measured at `size`, one run each, for [`rows`].
fn row_label(rows: &[JsText], measurer: &dyn crate::TextMeasurer, size: f64, style: Emphasis, step: f64) -> Label {
    let mut widest = 0.0;
    let lines = rows
        .iter()
        .map(|row| {
            let w = width(measurer, row, size, style);
            widest = js::max(widest, w);
            LabelLine { runs: vec![LabelRun { text: row.clone(), style, width: w }], width: w }
        })
        .collect::<Vec<_>>();
    Label { height: lines.len() as f64 * step, lines, width: widest, size, line_height: step }
}

/// Pele's `lines`: each row that holds text as a `tspan` starting at `x`, the first on the
/// baseline `y` and each next one `step` below the row before it.
#[allow(clippy::too_many_arguments, reason = "the arguments of Pele's lines, plus the part it draws")]
fn rows(out: &mut SceneBuilder, part: PartId, facet: Facet, label: &Label, x: f64, y: f64, step: f64, look: LabelLook) {
    let baselines: Vec<[Number; 2]> =
        (0..label.lines.len()).map(|i| [Number::num(x), Number::num(y + i as f64 * step)]).collect();
    out.rows(part, facet, label, &baselines, look);
}

fn entity_classes(item: &EmBox) -> String {
    format!("pele-node pele-em-{}{}", item.kind.name(), if item.reset { " pele-em-reset" } else { "" })
}

/// The frame of a box: a plain rectangle for a UI, and otherwise a wash of its series colour
/// over the page, outlined in the same colour.
fn box_frame(out: &mut SceneBuilder, part: PartId, item: &EmBox, origin: [f64; 2], size: [f64; 2], outlined: bool) {
    let origin = [Number::num(origin[0]), Number::num(origin[1])];
    let size = [Number::num(size[0]), Number::num(size[1])];
    let slot = series(item.kind);
    let plate = out.element(part, Facet::Shape).rect(origin, size, None).fill(Paint::Role(Role::Background));
    if slot.is_none() || outlined {
        plate.stroke(Paint::Role(Role::Border)).style(rounded()).finish();
    } else {
        plate.style(rounded()).finish();
    }
    if let Some(slot) = slot {
        out.element(part, Facet::Shape)
            .rect(origin, size, None)
            .fill(Paint::Series(slot))
            .fill_opacity(Number::raw(0.22))
            .stroke(Paint::Series(slot))
            .style(rounded())
            .finish();
    }
}

/// What a note or specification card shows: its heading and its text.
struct CardText {
    kind: &'static str,
    frame: JsText,
    heading: JsText,
    body: Vec<JsText>,
}

/// Pele's cards: each note, then each specification, headed by the frame it belongs to and
/// that frame's name.
fn card_texts(model: &EventModel, wrap: bool) -> Vec<CardText> {
    let names: HashMap<&[u16], &JsText> = model.boxes.iter().map(|item| (&item.id[..], &item.name)).collect();
    let heading = |title: &str, frame: &JsText| {
        let mut text = JsText::from(format!("{title} \u{b7} ").as_str());
        text.push_units(frame);
        if let Some(name) = names.get(&frame[..]) {
            text.push_str(" ");
            text.push_units(name);
        }
        text
    };
    let mut out = Vec::new();
    for note in &model.notes {
        let body = if wrap { hard_wrap(&note.text, NOTE_COLUMNS) } else { vec![note.text.clone()] };
        out.push(CardText { kind: "note", frame: note.frame.clone(), heading: heading("Note", &note.frame), body });
    }
    for spec in &model.specifications {
        let joined = |head: &str, names: &[JsText]| {
            let mut row = JsText::from(head);
            for (index, name) in names.iter().enumerate() {
                if index > 0 {
                    row.push_str(", ");
                }
                row.push_units(name);
            }
            row
        };
        let mut lines = vec![joined("Given  ", &spec.given)];
        if !spec.when.is_empty() {
            lines.push(joined("When  ", &spec.when));
        }
        lines.push(joined("Then  ", &spec.then));
        let body = if wrap {
            lines.iter().flat_map(|row| hard_wrap(row, NOTE_COLUMNS)).collect()
        } else {
            let mut text = JsText::default();
            for (index, line) in lines.iter().enumerate() {
                if index > 0 {
                    text.push(10);
                }
                text.push_units(line);
            }
            vec![text]
        };
        let heading = heading("Given / When / Then", &spec.frame);
        out.push(CardText { kind: "specification", frame: spec.frame.clone(), heading, body });
    }
    out
}

/// The parts of the entities a drawing names: lanes, boxes, flows and cards.
struct Parts {
    lanes: Vec<PartId>,
    boxes: Vec<PartId>,
}

impl Parts {
    fn new(out: &mut SceneBuilder, model: &EventModel) -> Self {
        Parts {
            lanes: model.lanes.iter().map(|lane| out.part(Entity::group(&lane.label))).collect(),
            boxes: model.boxes.iter().map(|item| out.part(Entity::node(&item.id))).collect(),
        }
    }

    fn flow(&self, out: &mut SceneBuilder, model: &EventModel, relation: Relation) -> PartId {
        let (source, target) = (&model.boxes[relation.source], &model.boxes[relation.target]);
        let mut written = source.id.clone();
        written.push_str("-");
        written.push_units(&target.id);
        let name = EntityName::Between { from: source.id.to_vec(), to: target.id.to_vec(), kind: "flow".to_owned() };
        let part = out.part(Entity::edge(&written, name, 0));
        out.relation(part, self.boxes[relation.source], self.boxes[relation.target]);
        part
    }
}

/// Each card's part: the notes and specifications of one frame are ranked in the order drawn.
fn card_parts(out: &mut SceneBuilder, cards: &[CardText]) -> Vec<PartId> {
    let mut seen: HashMap<&[u16], u32> = HashMap::new();
    cards
        .iter()
        .map(|card| {
            let rank = seen.entry(&card.frame[..]).or_insert(0);
            let part = out.part(Entity::note(&card.frame).ranked(*rank));
            *rank += 1;
            part
        })
        .collect()
}

fn edge_layer(out: &mut SceneBuilder, part: PartId, path: PathProgram, tip: Tip) {
    out.layer(LayerSpec::class("pele-edge").part(part, Facet::Shape).data_id(), |out| {
        out.element(part, Facet::Shape).path(path).fill(Paint::None).stroke(Paint::Role(Role::Line)).finish();
        draw_marker(out, part, Facet::EndMarker, Marker::Point, tip, MarkerUse::Edge(Paint::Role(Role::Line)));
    });
}

fn heading_look(small: f64) -> LabelLook {
    LabelLook::default().font_size(Number::raw(small)).weight(TextWeight::Heading)
}

fn lane_look(small: f64) -> LabelLook {
    LabelLook::class("pele-cluster-label")
        .font_size(Number::raw(small))
        .weight(TextWeight::Heading)
        .fill(Some(Paint::Role(Role::Muted)))
}

fn name_look(size: f64) -> LabelLook {
    LabelLook::class("pele-label").font_size(Number::raw(size)).weight(TextWeight::Heading)
}

fn data_look(mono: f64) -> LabelLook {
    LabelLook::class("pele-em-data").mono().font_size(Number::raw(mono))
}

/// A box placed on its lane.
struct Placed {
    name: Label,
    data: Label,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Pele's `renderEventModel`: across, or down where the host's width calls for it.
pub(super) fn scene(model: &EventModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    fit::turn_to_fit(cx.options, |down| if down { down_scene(model, cx) } else { across(model, cx) })
}

fn accessible(model: &EventModel) -> Accessible {
    Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() }
}

fn across(model: &EventModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let (name_size, small, mono) = (js::round(size * 0.875), js::round(size * 0.8125), js::round(size * 0.75));
    let mono_step = js::round(mono * 1.4);
    let small_step = js::round(small * 1.5);
    let measurer = cx.measure;
    let pad = cx.options.padding;
    let widest = |rows: &[JsText], size: f64, style: Emphasis| {
        rows.iter().fold(0.0, |widest, row| js::max(widest, width(measurer, row, size, style)))
    };

    struct Band {
        label: Label,
        right: f64,
        y: f64,
        h: f64,
    }
    let mut label_column: f64 = 0.0;
    let mut bands: Vec<Band> = model
        .lanes
        .iter()
        .map(|lane| {
            let label = layout_label(&lane.label, false, measurer, small, 4000.0, Emphasis::BOLD);
            label_column = js::max(label_column, label.width);
            Band { label, right: f64::NEG_INFINITY, y: 0.0, h: 0.0 }
        })
        .collect();
    label_column += 2.0 * LANE_PAD;

    // Time runs left to right. A frame in a new lane starts under the end of the one before it,
    // so the arrow between them can drop straight down; a frame in the same lane follows on.
    let mut placed: Vec<Placed> = Vec::with_capacity(model.boxes.len());
    for (index, item) in model.boxes.iter().enumerate() {
        let name = layout_label(&item.name, false, measurer, name_size, 4000.0, Emphasis::BOLD);
        let data = item
            .data
            .as_ref()
            .filter(|data| !data.is_empty())
            .map_or_else(Vec::new, |data| hard_wrap(data, DATA_COLUMNS));
        let w = js::max(BOX_MIN_WIDTH, js::max(name.width, widest(&data, mono, MONO)) + 2.0 * BOX_PAD_X);
        let rows = if data.is_empty() { 0.0 } else { 6.0 + data.len() as f64 * mono_step };
        let h = 2.0 * BOX_PAD_Y + name.height + rows;
        let band = &bands[item.lane];
        let mut x = 0.0;
        if index > 0 {
            let previous = &placed[index - 1];
            let stagger = previous.x + js::max(previous.w / 2.0, previous.w - STAGGER);
            x = if model.boxes[index - 1].lane == item.lane {
                band.right + BOX_GAP
            } else {
                js::max(stagger, band.right + BOX_GAP)
            };
        }
        let band = &mut bands[item.lane];
        band.right = x + w;
        band.h = js::max(band.h, h);
        let data = row_label(&data, measurer, mono, MONO, mono_step);
        placed.push(Placed { name, data, x, y: 0.0, w, h });
    }

    let content_right = placed.iter().fold(0.0, |right, item| js::max(right, item.x + item.w));
    let lane_width = label_column + content_right + LANE_PAD;

    let title = layout_label(model.title.as_deref().unwrap_or_default(), false, measurer, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let left = pad;
    let top = pad + title_height;

    let mut y = top;
    for band in &mut bands {
        band.y = y;
        band.h += 2.0 * LANE_PAD;
        y += band.h + LANE_GAP;
    }
    let mut bottom = if bands.is_empty() { top } else { y - LANE_GAP };
    for (item, placed) in model.boxes.iter().zip(&mut placed) {
        let band = &bands[item.lane];
        placed.x += left + label_column;
        placed.y = band.y + (band.h - placed.h) / 2.0;
    }

    let texts = card_texts(model, true);
    struct Card {
        head: Label,
        body: Label,
        y: f64,
        w: f64,
        h: f64,
    }
    let mut cards = Vec::with_capacity(texts.len());
    let mut cards_right: f64 = 0.0;
    for text in &texts {
        let head = layout_label(&text.heading, false, measurer, small, 4000.0, Emphasis::BOLD);
        let w = js::max(head.width, widest(&text.body, small, Emphasis::default())) + 2.0 * BOX_PAD_X;
        let h = 2.0 * BOX_PAD_Y + head.height + text.body.len() as f64 * small_step;
        let cy = bottom + if cards.is_empty() && bands.is_empty() { 0.0 } else { CARD_GAP };
        let body = row_label(&text.body, measurer, small, Emphasis::default(), small_step);
        cards.push(Card { head, body, y: cy, w, h });
        bottom = cy + h;
        cards_right = js::max(cards_right, w);
    }

    let width = js::max(js::max(if bands.is_empty() { 0.0 } else { lane_width }, cards_right), title.width);
    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (bottom + pad).ceil();

    let mut out = SceneBuilder::new(cx, Size { width: total_width, height: total_height }, accessible(model));
    out.title(&TitleLayout { at: Point { x: total_width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let parts = Parts::new(&mut out, model);
    let num = Number::num;
    for (band, &part) in bands.iter().zip(&parts.lanes) {
        let spec = LayerSpec::class("pele-cluster pele-em-lane").part(part, Facet::Shape).data_id();
        out.layer(spec, |out| {
            out.element(part, Facet::Shape)
                .rect([num(left), num(band.y)], [num(lane_width), num(band.h)], None)
                .fill(Paint::Role(Role::Surface))
                .style(rounded())
                .finish();
            let at = Point { x: left + LANE_PAD + band.label.width / 2.0, y: band.y + band.h / 2.0 };
            out.label(part, Facet::Label, &band.label, at, lane_look(small));
        });
    }

    let trim = Marker::Point.trim();
    for &relation in &model.relations {
        let (s, t) = (&placed[relation.source], &placed[relation.target]);
        if relation.source == relation.target {
            continue;
        }
        let part = parts.flow(&mut out, model, relation);
        let (path, tip) = if model.boxes[relation.source].lane == model.boxes[relation.target].lane {
            // Along the lane, from the side of one box to the side of the other.
            let forward = t.x >= s.x;
            let sx = if forward { s.x + s.w } else { s.x };
            let tx = if forward { t.x } else { t.x + t.w };
            let (sy, ty) = (s.y + s.h / 2.0, t.y + t.h / 2.0);
            let dir = if tx >= sx { 1.0 } else { -1.0 };
            let path = PathProgram::default().m(num(sx), num(sy)).l(num(tx - dir * trim), num(ty));
            (path, Tip { x: tx, y: ty, dx: dir, dy: 0.0 })
        } else {
            // Between lanes, straight down or up where the boxes overlap, else in an S.
            let down = t.y > s.y;
            let from = js::max(s.x, t.x);
            let to = js::min(s.x + s.w, t.x + t.w);
            let inset = |w: f64| js::min(24.0, w / 4.0);
            let overlap = to - from >= 16.0;
            let sx = if overlap {
                (from + to) / 2.0
            } else if t.x > s.x {
                s.x + s.w - inset(s.w)
            } else {
                s.x + inset(s.w)
            };
            let tx = if overlap {
                sx
            } else if t.x > s.x {
                t.x + inset(t.w)
            } else {
                t.x + t.w - inset(t.w)
            };
            let sy = if down { s.y + s.h } else { s.y };
            let ty = if down { t.y } else { t.y + t.h };
            let dir = if down { 1.0 } else { -1.0 };
            let end = ty - dir * trim;
            let mid = (sy + end) / 2.0;
            let path = if sx == tx {
                PathProgram::default().m(num(sx), num(sy)).v(num(end))
            } else {
                PathProgram::default().m(num(sx), num(sy)).c(
                    [num(sx), num(mid)],
                    [num(tx), num(mid)],
                    [num(tx), num(end)],
                )
            };
            (path, Tip { x: tx, y: ty, dx: 0.0, dy: dir })
        };
        edge_layer(&mut out, part, path, tip);
    }

    for ((item, placed), &part) in model.boxes.iter().zip(&placed).zip(&parts.boxes) {
        let spec = LayerSpec::class(&entity_classes(item)).part(part, Facet::Shape).data_id();
        out.layer(spec, |out| {
            box_frame(out, part, item, [placed.x, placed.y], [placed.w, placed.h], false);
            let name_y = placed.y + BOX_PAD_Y + placed.name.height / 2.0;
            let at = Point { x: placed.x + placed.w / 2.0, y: name_y };
            out.label(part, Facet::Label, &placed.name, at, name_look(name_size));
            let first = placed.y + BOX_PAD_Y + placed.name.height + 6.0 + mono_step / 2.0 + mono * 0.35;
            let x = placed.x + BOX_PAD_X;
            rows(out, part, Facet::Attribute, &placed.data, x, first, mono_step, data_look(mono));
        });
    }

    let card_parts = card_parts(&mut out, &texts);
    for ((text, card), &part) in texts.iter().zip(&cards).zip(&card_parts) {
        draw_card(&mut out, part, text.kind, [left, card.y], [card.w, card.h], &card.head, &card.body, small);
    }
    out.finish()
}

#[allow(clippy::too_many_arguments, reason = "a card's place, size and two texts")]
fn draw_card(
    out: &mut SceneBuilder,
    part: PartId,
    kind: &str,
    origin: [f64; 2],
    size: [f64; 2],
    head: &Label,
    body: &Label,
    small: f64,
) {
    let num = Number::num;
    let [x, y] = origin;
    let spec = LayerSpec::class(&format!("pele-em-{kind}")).part(part, Facet::Shape).data_id();
    out.layer(spec, |out| {
        out.element(part, Facet::Shape)
            .rect([num(x), num(y)], [num(size[0]), num(size[1])], None)
            .fill(Paint::Role(Role::Background))
            .stroke(Paint::Role(Role::Border))
            .style(rounded())
            .finish();
        let at = Point { x: x + BOX_PAD_X + head.width / 2.0, y: y + BOX_PAD_Y + head.height / 2.0 };
        out.label(part, Facet::Title, head, at, heading_look(small));
        let first = y + BOX_PAD_Y + head.height + body.line_height / 2.0 + small * 0.35;
        let look = LabelLook::default().font_size(Number::raw(small));
        rows(out, part, Facet::Label, body, x + BOX_PAD_X, first, body.line_height, look);
    });
}

/// Pele's `verticalEventModel`.
fn down_scene(model: &EventModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let mono = js::round(size * 0.75);
    let pad = cx.options.padding;
    let measurer = cx.measure;
    let room = js::max(160.0, cx.options.max_width.unwrap_or(320.0) - 2.0 * pad);
    let trim = Marker::Point.trim();

    // A relation that skips a card runs down a rail beside the cards. Relations into one card
    // share a rail; others whose runs overlap take the next rail out, so that no two follow the
    // same line. A relation between neighbouring cards passes the lower card's lane heading.
    let mut runs: Vec<(usize, [usize; 2])> = Vec::new();
    let mut passed = vec![false; model.boxes.len()];
    for relation in &model.relations {
        let (s, t) = (relation.source, relation.target);
        if s == t {
            continue;
        }
        if s.abs_diff(t) < 2 {
            passed[s.max(t)] = true;
            continue;
        }
        match runs.iter_mut().find(|(target, _)| *target == t) {
            Some((_, run)) => *run = [run[0].min(s), run[1].max(s)],
            None => runs.push((t, [s.min(t), s.max(t)])),
        }
    }
    runs.sort_by_key(|(_, run)| run[0]);
    let mut rail_of: HashMap<usize, usize> = HashMap::new();
    let mut rail_ends: Vec<usize> = Vec::new();
    for &(target, [from, to]) in &runs {
        let rail = rail_ends.iter().position(|&end| end < from).unwrap_or(rail_ends.len());
        if rail == rail_ends.len() {
            rail_ends.push(to);
        } else {
            rail_ends[rail] = to;
        }
        rail_of.insert(target, rail);
    }
    let gutter = 24.0 + RAIL_GAP * rail_ends.len().saturating_sub(1) as f64;
    // Rails are never shared to save room: past what the width allows, the drawing is wider.
    let card_width = js::max(CARD_MIN, room - gutter);
    let text_room = card_width - 2.0 * BOX_PAD_X;
    let title = layout_label(model.title.as_deref().unwrap_or_default(), false, measurer, size, room, Emphasis::BOLD);
    let mut y = pad + if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let mut bottom = pad + title.height;
    let mut width = js::max(title.width, card_width + gutter);
    let text = |raw: &[u16], size: f64, style: Emphasis| {
        literal_label(&decode_entities(raw), measurer, size, text_room, style)
    };

    struct Card {
        lane: Label,
        name: Label,
        data: Option<Label>,
        top: f64,
        y: f64,
        w: f64,
        h: f64,
    }
    let mut placed: Vec<Card> = Vec::with_capacity(model.boxes.len());
    for (index, item) in model.boxes.iter().enumerate() {
        let lane_room = if passed[index] { text_room - ARROW_ROOM } else { text_room };
        let lane_text = &model.lanes[item.lane].label;
        let lane = layout_label(lane_text, false, measurer, small, lane_room, Emphasis::BOLD);
        let lane = break_label(&lane, measurer, lane_room);
        let name = layout_label(&item.name, false, measurer, small, text_room - 36.0, Emphasis::BOLD);
        let data = item.data.as_ref().filter(|data| !data.is_empty()).map(|data| text(data, mono, MONO));
        let data_width = data.as_ref().map_or(0.0, |data| data.width);
        let w = js::max(
            js::max(card_width, lane.width + 2.0 * BOX_PAD_X),
            js::max(name.width + 2.0 * BOX_PAD_X + 36.0, data_width + 2.0 * BOX_PAD_X),
        );
        let head_h = lane.height + 12.0;
        let h = 2.0 * BOX_PAD_Y + name.height + data.as_ref().map_or(0.0, |data| data.height + 6.0);
        let cy = y + head_h;
        width = js::max(width, w + gutter);
        bottom = cy + h;
        placed.push(Card { lane, name, data, top: y, y: cy, w, h });
        y = bottom + 28.0;
    }
    // The rails run beside the widest box. Cards drawn below them may widen the drawing further.
    let rails_width = width;

    let texts = card_texts(model, false);
    struct Note {
        head: Label,
        body: Label,
        y: f64,
        w: f64,
        h: f64,
    }
    let mut notes = Vec::with_capacity(texts.len());
    for card in &texts {
        let head = layout_label(&card.heading, false, measurer, small, text_room, Emphasis::BOLD);
        let body = text(&card.body[0], small, Emphasis::default());
        let w = js::max(js::max(card_width, head.width + 2.0 * BOX_PAD_X), body.width + 2.0 * BOX_PAD_X);
        let h = 2.0 * BOX_PAD_Y + head.height + body.height;
        notes.push(Note { head, body, y, w, h });
        bottom = y + h;
        y = bottom + CARD_GAP;
        width = js::max(width, w);
    }
    let total_width = (width + 2.0 * pad).ceil();
    let height = (bottom + pad).ceil();

    let mut out = SceneBuilder::new(cx, Size { width: total_width, height }, accessible(model));
    out.title(&TitleLayout { at: Point { x: total_width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let parts = Parts::new(&mut out, model);
    let num = Number::num;

    for &relation in &model.relations {
        let (s, t) = (&placed[relation.source], &placed[relation.target]);
        if relation.source == relation.target {
            continue;
        }
        let part = parts.flow(&mut out, model, relation);
        let down = t.y > s.y;
        let dir = if down { 1.0 } else { -1.0 };
        let sy = if down { s.y + s.h } else { s.y };
        let ty = if down { t.y } else { t.y + t.h };
        let (path, tip) = if relation.source.abs_diff(relation.target) == 1 {
            // The lower card's lane heading sits in the gap, and wraps to leave the arrow room beside it.
            let lower = if down { t } else { s };
            let x = pad + js::max(js::min(s.w, t.w) / 2.0, BOX_PAD_X + lower.lane.width + 16.0);
            (PathProgram::default().m(num(x), num(sy)).v(num(ty - dir * trim)), Tip { x, y: ty, dx: 0.0, dy: dir })
        } else {
            let rail = pad + rails_width - gutter
                + 12.0
                + RAIL_GAP * rail_of.get(&relation.target).copied().unwrap_or(0) as f64;
            // A card can send one relation and receive another: they leave above and arrive below its middle.
            let (from, to) = (s.y + s.h / 2.0 - 5.0, t.y + t.h / 2.0 + 5.0);
            let path =
                PathProgram::default().m(num(pad + s.w), num(from)).h(num(rail)).v(num(to)).h(num(pad + t.w + trim));
            (path, Tip { x: pad + t.w, y: to, dx: -1.0, dy: 0.0 })
        };
        edge_layer(&mut out, part, path, tip);
    }

    for ((item, card), &part) in model.boxes.iter().zip(&placed).zip(&parts.boxes) {
        let lane_part = parts.lanes[item.lane];
        let spec = LayerSpec::class("pele-cluster pele-em-lane").part(lane_part, Facet::Shape).data_id();
        out.layer(spec, |out| {
            out.left_label(lane_part, Facet::Label, &card.lane, pad + BOX_PAD_X, card.top, lane_look(small));
            let spec = LayerSpec::class(&entity_classes(item)).part(part, Facet::Shape).data_id();
            out.layer(spec, |out| {
                box_frame(out, part, item, [pad, card.y], [card.w, card.h], true);
                let at = Point {
                    x: pad + BOX_PAD_X + card.name.width / 2.0,
                    y: card.y + BOX_PAD_Y + card.name.height / 2.0,
                };
                out.label(part, Facet::Label, &card.name, at, name_look(small));
                let frame_width = width_of(measurer, &item.id, mono);
                out.text(part, Facet::Annotation)
                    .class("pele-em-frame")
                    .at([num(pad + card.w - BOX_PAD_X), num(card.y + BOX_PAD_Y + small)])
                    .anchor(AnchorKind::End)
                    .font_size(Number::raw(mono))
                    .fill(Some(Paint::Role(Role::Muted)))
                    .finish(&item.id, frame_width, mono);
                if let Some(data) = &card.data {
                    let first = card.y + BOX_PAD_Y + card.name.height + 6.0 + data.line_height / 2.0 + mono * 0.35;
                    rows(out, part, Facet::Attribute, data, pad + BOX_PAD_X, first, data.line_height, data_look(mono));
                }
            });
        });
    }

    let card_parts = card_parts(&mut out, &texts);
    for ((text, note), &part) in texts.iter().zip(&notes).zip(&card_parts) {
        draw_card(&mut out, part, text.kind, [pad, note.y], [note.w, note.h], &note.head, &note.body, small);
    }
    out.finish()
}

fn width_of(measurer: &dyn crate::TextMeasurer, text: &[u16], size: f64) -> f64 {
    width(measurer, text, size, Emphasis::default())
}
