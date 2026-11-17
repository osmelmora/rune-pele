//! Pele's `renderKanban` (`kanban/render.ts`): a column per section and a card per item, the
//! columns folded into rows when the board is wider than the host.

use super::{KanbanModel, KanbanNode};
use crate::catalog::DrawContext;
use crate::input::url::{link_allowed, safe_url};
use crate::input::{JsText, JsValue, Units, to_lowercase};
use crate::measure::{self, Emphasis, Label, LabelLine, LabelRun, Script, TextMeasurer, layout_label};
use crate::scene::{
    Accessible, AnchorKind, AuthorClasses, Entity, Facet, LabelLook, LayerSpec, LineCap, Number, Paint, PartId,
    PathProgram, Point, Role, Scene, SceneBuilder, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

const COLUMN_GAP: f64 = 12.0;
const COLUMN_PAD: f64 = 8.0;
const CARD_PAD: f64 = 10.0;
const CARD_GAP: f64 = 8.0;
const META_GAP: f64 = 4.0;

/// The priorities Mermaid names that Pele marks. Medium, like any other name, is not marked.
#[derive(Clone, Copy)]
enum Priority {
    VeryHigh,
    High,
    Low,
    VeryLow,
}

impl Priority {
    /// Pele's `PRIORITY.get(priority.trim().toLowerCase())`.
    fn of(raw: Option<&JsText>) -> Option<Priority> {
        let name = to_lowercase(raw.map_or(&[][..], |raw| raw.trim_js()));
        match String::from_utf16_lossy(&name).as_str() {
            "very high" => Some(Priority::VeryHigh),
            "high" => Some(Priority::High),
            "low" => Some(Priority::Low),
            "very low" => Some(Priority::VeryLow),
            _ => None,
        }
    }

    fn class(self) -> &'static str {
        match self {
            Priority::VeryHigh => " pele-priority-very-high",
            Priority::High => " pele-priority-high",
            Priority::Low => " pele-priority-low",
            Priority::VeryLow => " pele-priority-very-low",
        }
    }

    /// The edge's colour, and whether it is drawn half opaque: the nearer to Medium, the fainter.
    fn mark(self) -> (Role, bool) {
        match self {
            Priority::VeryHigh => (Role::Critical, false),
            Priority::High => (Role::Critical, true),
            Priority::Low => (Role::Muted, true),
            Priority::VeryLow => (Role::Muted, false),
        }
    }
}

/// The code points of `text`, as JavaScript's `for (const ch of text)` gives them: a lone
/// surrogate on its own.
fn code_points(text: &[u16]) -> impl Iterator<Item = &[u16]> {
    let mut at = 0;
    std::iter::from_fn(move || {
        let start = at;
        let first = *text.get(at)?;
        let paired =
            (0xd800..0xdc00).contains(&first) && text.get(at + 1).is_some_and(|c| (0xdc00..0xe000).contains(c));
        at += if paired { 2 } else { 1 };
        Some(&text[start..at])
    })
}

/// Pele's kanban `fitLabel`: text laid out for a fixed width, where a word too long for a line is
/// broken between characters, because a card cannot grow sideways to hold it.
fn fit_label(
    raw: Option<&[u16]>,
    markdown: bool,
    measurer: &dyn TextMeasurer,
    size: f64,
    max: f64,
    base: Emphasis,
) -> Label {
    let label = layout_label(raw.unwrap_or_default(), markdown, measurer, size, max, base);
    if label.width <= max {
        return label;
    }
    let mut lines: Vec<LabelLine> = Vec::new();
    let mut widest: f64 = 0.0;
    let mut line: Vec<LabelRun> = Vec::new();
    let mut width = 0.0;
    let mut commit = |line: &mut Vec<LabelRun>, width: &mut f64| {
        lines.push(LabelLine { runs: std::mem::take(line), width: *width });
        widest = js::max(widest, *width);
        *width = 0.0;
    };
    for whole in &label.lines {
        if whole.width <= max {
            line = whole.runs.clone();
            width = whole.width;
            commit(&mut line, &mut width);
            continue;
        }
        for run in &whole.runs {
            let mut text: Vec<u16> = Vec::new();
            let mut from = width;
            for ch in code_points(&run.text) {
                let w = measure::width(measurer, ch, size, run.style);
                if width > 0.0 && width + w > max {
                    if !text.is_empty() {
                        let text = std::mem::take(&mut text).into();
                        line.push(LabelRun { text, style: run.style, width: width - from });
                    }
                    commit(&mut line, &mut width);
                    from = 0.0;
                }
                text.extend_from_slice(ch);
                width += w;
            }
            if !text.is_empty() {
                line.push(LabelRun { text: text.into(), style: run.style, width: width - from });
            }
        }
        commit(&mut line, &mut width);
    }
    let height = lines.len() as f64 * label.line_height;
    Label { lines, width: widest, height, ..label }
}

/// The attributes a block of text is written with before its own emphasis.
#[derive(Clone, Copy)]
struct TextLook {
    class: &'static str,
    /// Whether it gives its own weight, a heading's, so that bold runs add none.
    heading: bool,
    fill: Option<Paint>,
}

/// Pele's `textBlock`: a label with every line starting at `x`, or ending there, and the first
/// line's box starting at `y`. A line of one run without a shift or underline is written as plain
/// text with its emphasis as attributes; any other line as a label of its own.
#[allow(clippy::too_many_arguments, reason = "Pele's textBlock, plus the part and facet it draws")]
fn text_block(
    out: &mut SceneBuilder,
    part: PartId,
    facet: Facet,
    label: &Label,
    x: f64,
    y: f64,
    look: TextLook,
    end: bool,
) {
    for (i, line) in label.lines.iter().enumerate() {
        let cy = y + label.line_middle(i);
        match line.runs.as_slice() {
            [run] if !run.style.underline && run.style.script == Script::Normal => {
                let mut text = out.text(part, facet).class(look.class);
                if look.heading {
                    text = text.weight(TextWeight::Heading);
                }
                text = text.fill(look.fill);
                if run.style.bold && !look.heading {
                    text = text.weight(TextWeight::Bold);
                }
                if run.style.italic {
                    text = text.italic();
                }
                if run.style.mono {
                    text = text.mono();
                }
                text = text.at([Number::num(x), Number::num(cy + label.size * 0.35)]);
                if end {
                    text = text.anchor(AnchorKind::End);
                }
                text.finish(&run.text, run.width, label.size);
            }
            _ => {
                let mut labelled = LabelLook::class(look.class);
                if look.heading {
                    labelled = labelled.weight(TextWeight::Heading);
                }
                let labelled = labelled.fill(look.fill);
                let shift = if end { -line.width } else { line.width };
                out.label(part, facet, &label.line(i), Point { x: x + shift / 2.0, y: cy }, labelled);
            }
        }
    }
}

/// A card with its text laid out.
struct Card<'a> {
    item: &'a KanbanNode,
    label: Label,
    ticket: Label,
    assigned: Label,
    /// The height of the rows under the text that hold the ticket and the assignee.
    meta: f64,
    height: f64,
}

/// `text.replace('#TICKET#', () => ticket)`: the first mark only, the ticket as it is.
fn ticket_url(base: &[u16], ticket: &[u16]) -> Vec<u16> {
    match base.find_str("#TICKET#", 0) {
        Some(at) => [&base[..at], ticket, &base[at + 8..]].concat(),
        None => base.to_vec(),
    }
}

/// The duplicate rank of each entity of a kind, by its name.
#[derive(Default)]
struct Ranks(HashMap<JsText, u32>);

impl Ranks {
    fn next(&mut self, id: &JsText) -> u32 {
        let rank = self.0.entry(id.clone()).or_insert(0);
        std::mem::replace(rank, *rank + 1)
    }
}

pub(super) fn draw(model: &KanbanModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let line = js::round(size * 1.5);
    let small_line = js::round(small * 1.5);
    let measurer = cx.measure;
    let pad = cx.options.padding_or_default();
    let conf = cx.config.get("kanban").and_then(JsValue::as_record);
    let setting = |key: &str| conf.and_then(|conf| conf.get(key));
    let own_width = match setting("sectionWidth").and_then(JsValue::as_number) {
        Some(width) if width > 0.0 => js::min(js::max(width, 80.0), 2000.0),
        _ => 200.0,
    };
    let room = cx.options.max_width.filter(|&width| width > 0.0).map_or(f64::INFINITY, |width| width - 2.0 * pad);
    let sections = &model.sections;
    let count = sections.len() as f64;
    let fits = count * (own_width + COLUMN_GAP) - COLUMN_GAP <= room;
    let per_row =
        if fits { js::max(count, 1.0) } else { js::max(1.0, ((room + COLUMN_GAP) / (own_width + COLUMN_GAP)).floor()) };
    let column_width = if fits {
        own_width
    } else {
        let shared = js::min(((room + COLUMN_GAP) / per_row).floor() - COLUMN_GAP, own_width * 2.0);
        js::max(js::max(js::min(own_width, room), shared), 0.0)
    };
    let per_row = per_row as usize;
    let base_url = setting("ticketBaseUrl").and_then(JsValue::as_text).cloned().unwrap_or_default();
    let card_width = column_width - 2.0 * COLUMN_PAD;
    let text_width = card_width - 2.0 * CARD_PAD;

    let mut titles = Vec::with_capacity(sections.len());
    let mut count_widths = Vec::with_capacity(sections.len());
    let mut cards: Vec<Vec<Card<'_>>> = Vec::with_capacity(sections.len());
    let mut head_height = line;
    let mut body_heights = Vec::with_capacity(sections.len());
    for section in sections {
        let count_text = JsText::from(section.items.len().to_string().as_str());
        let count_width = measure::width(measurer, &count_text, small, Emphasis::default());
        let room = card_width - 8.0 - (count_width + 12.0);
        let title = fit_label(Some(&section.node.label), true, measurer, size, room, Emphasis::BOLD);
        head_height = js::max(head_height, title.height);
        count_widths.push(count_width);
        titles.push(title);

        let mut list = Vec::with_capacity(section.items.len());
        let mut height = 0.0;
        for item in &section.items {
            let plain = Emphasis::default();
            let label = fit_label(Some(&item.label), true, measurer, small, text_width, plain);
            let ticket = fit_label(item.ticket.as_deref(), false, measurer, small, text_width, plain);
            let assigned = fit_label(item.assigned.as_deref(), false, measurer, small, text_width, plain);
            let meta = if ticket.width + assigned.width + 8.0 > text_width {
                ticket.height + assigned.height
            } else {
                js::max(ticket.height, assigned.height)
            };
            let below = if meta != 0.0 { META_GAP + meta } else { 0.0 };
            let card_height = 2.0 * CARD_PAD + js::max(label.height, small_line) + below;
            list.push(Card { item, label, ticket, assigned, meta, height: card_height });
            height += card_height + CARD_GAP;
        }
        cards.push(list);
        body_heights.push(height);
    }
    head_height += 2.0 * COLUMN_PAD;
    let mut row_heights = Vec::new();
    let mut row_tops = Vec::new();
    let mut y = 0.0;
    for first in (0..sections.len()).step_by(per_row) {
        let tallest =
            body_heights[first..(first + per_row).min(sections.len())].iter().fold(CARD_GAP, |a, &b| js::max(a, b));
        row_tops.push(y);
        row_heights.push(head_height + tallest);
        y += head_height + tallest + COLUMN_GAP;
    }
    let board_height = match (row_tops.last(), row_heights.last()) {
        (Some(top), Some(height)) => top + height,
        _ => 0.0,
    };

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measurer, size, fit::title_room(cx.options), Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let board_width = js::max((sections.len().min(per_row) as f64) * (column_width + COLUMN_GAP) - COLUMN_GAP, 0.0);
    let width = js::max(board_width, title.width);
    let left = pad + (width - board_width) / 2.0;
    let edge = column_width - COLUMN_PAD - 4.0;

    let total = Size { width: (width + 2.0 * pad).ceil(), height: (board_height + title_height + 2.0 * pad).ceil() };
    let mut out = SceneBuilder::new(cx, total, Accessible::default());
    let at = Point { x: total.width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at });

    let policy = &cx.options.resources;
    let rel = (!policy.link_rel.is_empty()).then(|| policy.link_rel.clone());
    let (mut column_ranks, mut card_ranks) = (Ranks::default(), Ranks::default());
    for (index, section) in sections.iter().enumerate() {
        let row = index / per_row;
        let node = &section.node;
        let part = out.part(Entity::group(&node.id).ranked(column_ranks.next(&node.id)));
        let classes = AuthorClasses::new(node.css_classes.as_deref().unwrap_or_default());
        let at = Point {
            x: left + ((index % per_row) as f64) * (column_width + COLUMN_GAP),
            y: pad + title_height + row_tops[row],
        };
        let column = LayerSpec::classes("pele-cluster pele-column", &classes).part(part, Facet::Shape).data_id();
        out.layer(column.translate(at), |out| {
            out.element(part, Facet::Shape)
                .rect_at_origin([Number::raw(column_width), Number::num(row_heights[row])])
                .fill(Paint::Role(Role::Surface))
                .style(Some(Style::rounded(None)))
                .finish();
            let look = TextLook { class: "pele-cluster-label", heading: true, fill: None };
            text_block(out, part, Facet::Label, &titles[index], COLUMN_PAD + 4.0, COLUMN_PAD, look, false);
            let count_text = JsText::from(section.items.len().to_string().as_str());
            out.text(part, Facet::Legend)
                .class("pele-count")
                .at([Number::raw(edge), Number::num(COLUMN_PAD + line / 2.0 + small * 0.35)])
                .anchor(AnchorKind::End)
                .font_size(Number::raw(small))
                .fill(Some(Paint::Role(Role::Muted)))
                .finish(&count_text, count_widths[index], small);

            let mut y = head_height;
            for card in &cards[index] {
                let item = card.item;
                let priority = Priority::of(item.priority.as_ref());
                let part = out.part(Entity::node(&item.id).ranked(card_ranks.next(&item.id)));
                let own = format!("pele-node pele-card{}", priority.map_or("", Priority::class));
                let classes = AuthorClasses::new(item.css_classes.as_deref().unwrap_or_default());
                let group = LayerSpec::classes(&own, &classes)
                    .part(part, Facet::Shape)
                    .data_id()
                    .translate(Point { x: COLUMN_PAD, y })
                    .font_size(Number::raw(small));
                out.layer(group, |out| draw_card(out, part, card, priority, card_width, &base_url, policy, &rel));
                y += card.height + CARD_GAP;
            }
        });
    }
    out.finish()
}

#[allow(clippy::too_many_arguments, reason = "one card and what every card shares")]
fn draw_card(
    out: &mut SceneBuilder,
    part: PartId,
    card: &Card<'_>,
    priority: Option<Priority>,
    card_width: f64,
    base_url: &[u16],
    policy: &crate::ResourcePolicy,
    rel: &Option<String>,
) {
    let item = card.item;
    out.element(part, Facet::Shape)
        .rect_at_origin([Number::raw(card_width), Number::num(card.height)])
        .fill(Paint::Role(Role::Background))
        .stroke(Paint::Role(Role::Border))
        .style(Some(Style::rounded(None)))
        .finish();
    if let Some(priority) = priority {
        let (colour, faint) = priority.mark();
        let path = PathProgram::default().m(Number::raw(2.5), Number::raw(5.0)).v(Number::num(card.height - 5.0));
        let edge = out.element(part, Facet::Shape).class("pele-priority").path(path).fill(Paint::None);
        let edge = edge.stroke(Paint::Role(colour));
        let edge = if faint { edge.opacity(Number::raw(0.5)) } else { edge };
        edge.stroke_width(Number::raw(3.0)).cap(LineCap::Round).finish();
    }
    let look = TextLook { class: "pele-label", heading: false, fill: None };
    text_block(out, part, Facet::Label, &card.label, CARD_PAD, CARD_PAD, look, false);
    let bottom = card.height - CARD_PAD;
    if let Some(ticket) = &item.ticket {
        let top = bottom - card.meta;
        let href = (!base_url.is_empty())
            .then(|| safe_url(&ticket_url(base_url, ticket)))
            .filter(|href| link_allowed(href, policy));
        match href {
            None => {
                let look = TextLook { class: "pele-ticket", heading: false, fill: Some(Paint::Role(Role::Muted)) };
                text_block(out, part, Facet::Attribute, &card.ticket, CARD_PAD, top, look, false);
            }
            Some(href) => out.link_to(part, href, Some("_blank"), rel.clone(), |out| {
                let look = TextLook { class: "pele-ticket", heading: false, fill: Some(Paint::Role(Role::Accent)) };
                text_block(out, part, Facet::Attribute, &card.ticket, CARD_PAD, top, look, false);
            }),
        }
    }
    if item.assigned.is_some() {
        let look = TextLook { class: "pele-assigned", heading: false, fill: Some(Paint::Role(Role::Muted)) };
        let top = bottom - card.assigned.height;
        text_block(out, part, Facet::Annotation, &card.assigned, card_width - CARD_PAD, top, look, true);
    }
}
