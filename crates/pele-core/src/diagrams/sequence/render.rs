//! Drawing a sequence diagram, ported from Pele's `src/diagrams/sequence/render.ts`. The
//! lifelines are spaced by what passes between them, the entries are then placed top to bottom,
//! and what that gives is drawn as a Scene.
//!
//! Pele writes each layer's markup as it walks the entries. Here the walk fills a [`Drawing`],
//! one list per layer, and `emit` writes the lists in layer order.

use super::actors::{ActorKind, ActorSize, actor_size, draw_actor};
use super::{Central, Entry, LineType, Placement, SeqActor, SeqMessage, SequenceModel};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::graph::edges::{ARROW, Marker, MarkerUse, Tip, draw_marker};
use crate::input::{JsObject, JsText, JsValue, Units};
use crate::measure::{
    Emphasis, Font, FontFamily, Label, TextMeasure, TextMeasurer, TextRef, break_label, layout_label, width,
};
use crate::scene::{
    Accessible, AnchorKind, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineJoin, Number,
    Paint, PartId, PathProgram, Point, Role, Scene, SceneBuilder, Size, Style, TextWeight, TitleLayout, ink_on,
    resolve_style,
};
use crate::{MonospacePart, RenderError, fit, js};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

/// The head a message line ends in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    None,
    Arrow,
    Cross,
    Open,
    HalfSolid,
    HalfStick,
}

impl Head {
    /// How much of the line's end the head takes.
    fn trim(self) -> f64 {
        match self {
            Head::Arrow => ARROW - 1.0,
            Head::Cross => 4.0,
            _ => 0.0,
        }
    }
}

/// How a message line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineStyle {
    dotted: bool,
    head: Head,
    /// Where the heads are: at the receiver, at the sender only (the reverse arrows), or at both.
    at_start: bool,
    at_end: bool,
    /// Whether a half head sits under the line.
    bottom: bool,
}

/// Pele's `STYLES`: the style of an entry that is a message line.
fn line_style(kind: LineType) -> Option<LineStyle> {
    use LineType as L;
    let style = |dotted, head| LineStyle { dotted, head, at_start: false, at_end: true, bottom: false };
    let both = |dotted| LineStyle { at_start: true, ..style(dotted, Head::Arrow) };
    let reverse = |style: LineStyle| LineStyle { at_start: true, at_end: false, ..style };
    let under = |style: LineStyle| LineStyle { bottom: true, ..style };
    Some(match kind {
        L::Solid => style(false, Head::Arrow),
        L::Dotted => style(true, Head::Arrow),
        L::SolidCross => style(false, Head::Cross),
        L::DottedCross => style(true, Head::Cross),
        L::SolidOpen => style(false, Head::None),
        L::DottedOpen => style(true, Head::None),
        L::SolidPoint => style(false, Head::Open),
        L::DottedPoint => style(true, Head::Open),
        L::BidirectionalSolid => both(false),
        L::BidirectionalDotted => both(true),
        L::SolidTop => style(false, Head::HalfSolid),
        L::SolidBottom => under(style(false, Head::HalfSolid)),
        L::StickTop => style(false, Head::HalfStick),
        L::StickBottom => under(style(false, Head::HalfStick)),
        L::SolidArrowTopReverse => reverse(style(false, Head::HalfSolid)),
        L::SolidArrowBottomReverse => under(reverse(style(false, Head::HalfSolid))),
        L::StickArrowTopReverse => reverse(style(false, Head::HalfStick)),
        L::StickArrowBottomReverse => under(reverse(style(false, Head::HalfStick))),
        L::SolidTopDotted => style(true, Head::HalfSolid),
        L::SolidBottomDotted => under(style(true, Head::HalfSolid)),
        L::StickTopDotted => style(true, Head::HalfStick),
        L::StickBottomDotted => under(style(true, Head::HalfStick)),
        L::SolidArrowTopReverseDotted => reverse(style(true, Head::HalfSolid)),
        L::SolidArrowBottomReverseDotted => under(reverse(style(true, Head::HalfSolid))),
        L::StickArrowTopReverseDotted => reverse(style(true, Head::HalfStick)),
        L::StickArrowBottomReverseDotted => under(reverse(style(true, Head::HalfStick))),
        _ => return None,
    })
}

pub(crate) fn is_line_message(kind: LineType) -> bool {
    line_style(kind).is_some()
}

/// A block of entries with a frame around it, or a shaded `rect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameKind {
    Loop,
    Alt,
    Opt,
    Par,
    Critical,
    Break,
    Rect,
}

impl FrameKind {
    /// Pele's `FRAME_KIND`: the block an entry starts.
    fn started_by(kind: LineType) -> Option<FrameKind> {
        Some(match kind {
            LineType::LoopStart => FrameKind::Loop,
            LineType::AltStart => FrameKind::Alt,
            LineType::OptStart => FrameKind::Opt,
            LineType::ParStart | LineType::ParOverStart => FrameKind::Par,
            LineType::CriticalStart => FrameKind::Critical,
            LineType::BreakStart => FrameKind::Break,
            LineType::RectStart => FrameKind::Rect,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            FrameKind::Loop => "loop",
            FrameKind::Alt => "alt",
            FrameKind::Opt => "opt",
            FrameKind::Par => "par",
            FrameKind::Critical => "critical",
            FrameKind::Break => "break",
            FrameKind::Rect => "rect",
        }
    }
}

const MSG_PAD: f64 = 12.0;
const SELF_W: f64 = 28.0;
const SELF_H: f64 = 22.0;
const ACT_W: f64 = 10.0;
const ACT_STEP: f64 = 4.0;
const ACT_MIN: f64 = 14.0;
const NOTE_GAP: f64 = 10.0;
const NOTE_OVER: f64 = 16.0;
const FRAME_PAD: f64 = 20.0;
const FRAME_NEST: f64 = 10.0;
const TAB_H: f64 = 20.0;
const FRAME_TEXT: f64 = 240.0;
const BADGE_H: f64 = 16.0;
const CENTRAL_R: f64 = 4.0;

/// The number each message line carries, whether or not it is shown.
pub(crate) struct Numbering {
    /// NaN for every entry that is no message line.
    pub(crate) index: Vec<f64>,
    pub(crate) shown: Vec<bool>,
    /// Whether numbers are on after the last statement.
    pub(crate) visible: bool,
}

/// Pele's `numberMessages`: counts message lines the way Mermaid's renderer does while it draws.
pub(crate) fn number_messages(messages: &[SeqMessage], always: bool) -> Numbering {
    let mut index = Vec::with_capacity(messages.len());
    let mut shown = Vec::with_capacity(messages.len());
    let (mut current, mut step, mut visible) = (1.0, 1.0, false);
    // `a || b` for numbers: zero and NaN give way.
    let or = |given: Option<f64>, kept: f64| given.filter(|&given| given != 0.0 && !given.is_nan()).unwrap_or(kept);
    for message in messages {
        index.push(f64::NAN);
        shown.push(false);
        if let Entry::Numbering(spec) = &message.entry {
            current = or(spec.start, current);
            step = or(spec.step, step);
            visible = spec.visible;
        } else if is_line_message(message.kind()) {
            *index.last_mut().expect("pushed above") = current;
            *shown.last_mut().expect("pushed above") = visible || always;
            current = js::round((current + step) * 100.0) / 100.0;
        }
    }
    Numbering { index, shown, visible }
}

/// A participant's lifeline: where it stands and what the walk down the entries leaves on it.
struct Column<'a> {
    id: &'a JsText,
    actor: &'a SeqActor,
    kind: ActorKind,
    label: Label,
    size: ActorSize,
    x: f64,
    /// The message line that creates the participant, where one does.
    create_at: Option<usize>,
    destroy_at: Option<usize>,
    top: f64,
    life_start: f64,
    /// Where the lifeline of a destroyed participant ends.
    life_end: Option<f64>,
    /// The top of each open activation, the outermost first.
    activations: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewKind {
    Line,
    /// A message from a participant to itself.
    Loop,
    Note,
}

/// An entry that is drawn between or beside lifelines.
struct View {
    kind: ViewKind,
    a: usize,
    b: usize,
    label: Label,
    x: f64,
    w: f64,
}

struct Frame {
    kind: FrameKind,
    x0: f64,
    x1: f64,
    top: f64,
    title: Label,
    tab_w: f64,
    /// The colour of a `rect`.
    fill: JsText,
    /// The entries that start the block's sections, the block's own first.
    dividers: Vec<usize>,
    divider_y: Vec<f64>,
    divider_labels: Vec<Label>,
}

/// A box around participants that stand side by side.
struct BoxRun {
    in_box: usize,
    first: usize,
    last: usize,
    title: Label,
    /// How far the box reaches past its participants on each side to hold its title.
    grow: f64,
}

/// Where a message's text sits against its line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Centre,
    Right,
}

/// What the config and the host's options set, with what a tight draw makes of them.
struct Settings {
    tight: bool,
    size: f64,
    small: f64,
    tiny: f64,
    pad: f64,
    max_width: Option<f64>,
    mirror: bool,
    right_angles: bool,
    actor_margin: f64,
    min_w: f64,
    min_h: f64,
    wrap_w: f64,
    msg_gap: f64,
    note_pad: f64,
    box_pad: f64,
    align: Align,
    msg_pad: f64,
    note_over: f64,
    frame_pad: f64,
    frame_nest: f64,
    monospaced: bool,
}

/// A number the config's sequence section gives for `key`, where it is one from 0 to 2000.
fn option(section: Option<&JsObject>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|section| section.get(key)) {
        Some(&JsValue::Number(value)) if (0.0..=2000.0).contains(&value) => value,
        _ => fallback,
    }
}

fn section<'a>(cx: &DrawContext<'a>) -> Option<&'a JsObject> {
    cx.config.get("sequence").and_then(JsValue::as_record)
}

impl Settings {
    fn new(model: &SequenceModel, cx: &DrawContext<'_>, tight: bool) -> Self {
        let conf = section(cx);
        let flag = |key: &str| conf.and_then(|conf| conf.get(key));
        let cap = |value: f64, tightly: f64| if tight { js::min(value, tightly) } else { value };
        let size = cx.options.typography.normal.size;
        let pad = cx.options.padding_or_default();
        let actor_margin = cap(option(conf, "actorMargin", 40.0), 8.0);
        let actors = js::max(1.0, model.actors.len() as f64);
        let room = (cx.options.max_width.unwrap_or(320.0) - 2.0 * pad) / actors - actor_margin - 24.0;
        let align = match flag("messageAlign").and_then(JsValue::as_text) {
            Some(side) if side.is_str("left") => Align::Left,
            Some(side) if side.is_str("right") => Align::Right,
            _ => Align::Centre,
        };
        Settings {
            tight,
            size,
            small: js::round(size * 0.875),
            tiny: js::round(size * 0.75),
            pad,
            max_width: cx.options.max_width,
            mirror: flag("mirrorActors") != Some(&JsValue::Bool(false)),
            right_angles: flag("rightAngles") == Some(&JsValue::Bool(true)),
            actor_margin,
            min_w: cap(option(conf, "width", 64.0), 48.0),
            min_h: option(conf, "height", js::round(size * 1.5) + 20.0),
            wrap_w: cap(option(conf, "width", 160.0), js::max(32.0, room)),
            // Mermaid's default of 35 corresponds to Pele's default gap. Other values move it by the difference.
            msg_gap: js::max(4.0, 14.0 + option(conf, "messageMargin", 35.0) - 35.0),
            note_pad: cap(option(conf, "noteMargin", 10.0), 6.0),
            box_pad: cap(option(conf, "boxMargin", 10.0), 6.0),
            align,
            msg_pad: if tight { 6.0 } else { MSG_PAD },
            note_over: if tight { 8.0 } else { NOTE_OVER },
            frame_pad: if tight { 12.0 } else { FRAME_PAD },
            frame_nest: if tight { 6.0 } else { FRAME_NEST },
            monospaced: cx.options.monospace.contains(&MonospacePart::Messages),
        }
    }
}

/// The host's measurer asked for every text in the monospace font, as messages are measured
/// when the host sets them in it.
struct Monospaced<'a>(&'a dyn TextMeasurer);

impl TextMeasurer for Monospaced<'_> {
    fn measure(&self, text: TextRef<'_>, font: &Font) -> TextMeasure {
        self.0.measure(text, &Font { family: FontFamily::Mono, ..*font })
    }
}

/// The left and right ends of what is drawn.
#[derive(Clone, Copy)]
struct Extent {
    min_x: f64,
    max_x: f64,
}

impl Extent {
    fn empty() -> Self {
        Extent { min_x: f64::INFINITY, max_x: f64::NEG_INFINITY }
    }

    fn extend(&mut self, x0: f64, x1: f64) {
        if x0 < self.min_x {
            self.min_x = x0;
        }
        if x1 > self.max_x {
            self.max_x = x1;
        }
    }
}

/// The lifelines and what stands between them: everything the walk down the entries reads.
struct Stage<'a> {
    model: &'a SequenceModel,
    set: Settings,
    measure: &'a dyn TextMeasurer,
    mono: Monospaced<'a>,
    numbering: Numbering,
    cols: Vec<Column<'a>>,
    col_of: HashMap<&'a [u16], usize>,
    views: Vec<Option<View>>,
    /// A message's text laid out for a width, by the message and the width's bits.
    sized: HashMap<(usize, u64), Label>,
    runs: Vec<BoxRun>,
    /// The distance from each lifeline to the next.
    gaps: Vec<f64>,
    frames: Vec<Frame>,
    /// The block an entry starts or ends, by its place in `frames`.
    frame_at: Vec<Option<usize>>,
    extent: Extent,
}

impl<'a> Stage<'a> {
    fn message_measure(&self) -> &dyn TextMeasurer {
        if self.set.monospaced { &self.mono } else { self.measure }
    }

    fn text(&self, raw: &[u16], size: f64, wrap: bool, width: f64, by: &dyn TextMeasurer) -> Label {
        let room = if self.set.tight || wrap { width } else { f64::INFINITY };
        let label = layout_label(raw, false, by, size, room, Emphasis::default());
        if self.set.tight { break_label(&label, by, js::max(120.0, width)) } else { label }
    }

    fn plain_text(&self, raw: &[u16], size: f64, wrap: bool, width: f64) -> Label {
        self.text(raw, size, wrap, width, self.measure)
    }

    fn message_text(&self, raw: &[u16], wrap: bool, width: f64) -> Label {
        self.text(raw, self.set.small, wrap, width, self.message_measure())
    }

    /// Pele's `sizedText`: a wrapped message's text for a width, laid out once per width.
    fn sized_text(&mut self, i: usize, room: f64) -> Label {
        if let Some(label) = self.sized.get(&(i, room.to_bits())) {
            return label.clone();
        }
        let label = self.message_text(&self.model.messages[i].text, true, room);
        self.sized.insert((i, room.to_bits()), label.clone());
        label
    }

    fn badge_width(&self, i: usize) -> f64 {
        if !self.numbering.shown[i] {
            return 0.0;
        }
        let number: Vec<u16> = js::to_string(self.numbering.index[i]).encode_utf16().collect();
        js::max(BADGE_H, width(self.measure, &number, self.set.tiny, Emphasis::default()) + 8.0)
    }

    fn col(&self, id: Option<&JsText>) -> Option<usize> {
        self.col_of.get(&**id?).copied()
    }

    fn new(model: &'a SequenceModel, cx: &DrawContext<'a>, tight: bool) -> Self {
        let conf = section(cx);
        let flag = |key: &str| conf.and_then(|conf| conf.get(key)) == Some(&JsValue::Bool(true));
        let messages = &model.messages;
        let count = messages.len();
        let mut stage = Stage {
            model,
            set: Settings::new(model, cx, tight),
            measure: cx.measure,
            mono: Monospaced(cx.measure),
            numbering: number_messages(messages, flag("showSequenceNumbers")),
            cols: Vec::new(),
            col_of: HashMap::new(),
            views: Vec::new(),
            sized: HashMap::new(),
            runs: Vec::new(),
            gaps: Vec::new(),
            frames: Vec::new(),
            frame_at: Vec::new(),
            extent: Extent::empty(),
        };
        let (small, wrap_w) = (stage.set.small, stage.set.wrap_w);

        let used: Option<HashSet<&[u16]>> = flag("hideUnusedParticipants")
            .then(|| messages.iter().flat_map(|m| [m.from.as_deref(), m.to.as_deref()]).flatten().collect());

        // The first message line at or after each entry.
        let mut next_line: Vec<Option<usize>> = vec![None; count + 1];
        for i in (0..count).rev() {
            next_line[i] = if is_line_message(messages[i].kind()) { Some(i) } else { next_line[i + 1] };
        }

        for (id, actor) in &model.actors {
            if used.as_ref().is_some_and(|used| !used.contains(&**id)) {
                continue;
            }
            let kind = ActorKind::of(&actor.kind);
            let label = stage.plain_text(&actor.description, stage.set.size, actor.wrap, wrap_w);
            let line_after = |at: Option<&usize>| at.and_then(|&at| next_line[at.min(count)]);
            let create_at = line_after(model.created_actors.get(id)).filter(|&at| messages[at].to.as_ref() == Some(id));
            let destroy_at = line_after(model.destroyed_actors.get(id))
                .filter(|&at| messages[at].to.as_ref() == Some(id) || messages[at].from.as_ref() == Some(id));
            stage.col_of.insert(id, stage.cols.len());
            stage.cols.push(Column {
                id,
                actor,
                kind,
                size: actor_size(kind, &label, stage.set.min_w, stage.set.min_h, tight),
                label,
                x: 0.0,
                create_at,
                destroy_at,
                top: 0.0,
                life_start: 0.0,
                life_end: None,
                activations: Vec::new(),
            });
        }
        let n = stage.cols.len();

        // Widths between lifelines. Every message and note says how far apart the lifelines it touches must be.
        let mut needs: IndexMap<(usize, usize), f64> = IndexMap::new();
        let mut need = |lo: Option<usize>, hi: usize, width: f64| {
            let Some(lo) = lo.filter(|_| hi < n) else {
                return;
            };
            let known = needs.entry((lo, hi)).or_insert(0.0);
            if *known < width {
                *known = width;
            }
        };
        for (i, m) in messages.iter().enumerate() {
            let (Some(a), Some(b)) = (stage.col(m.from.as_ref()), stage.col(m.to.as_ref())) else {
                stage.views.push(None);
                continue;
            };
            let (lo, hi) = (a.min(b), a.max(b));
            let view = match &m.entry {
                Entry::Numbering(_) => None,
                Entry::Note { placement } => {
                    let label = stage.plain_text(&m.text, small, m.wrap, wrap_w);
                    let w = label.width + 2.0 * stage.set.note_pad;
                    match placement {
                        Placement::LeftOf => need(a.checked_sub(1), a, w + 2.0 * NOTE_GAP + ACT_W),
                        Placement::RightOf => need(Some(a), a + 1, w + 2.0 * NOTE_GAP + ACT_W),
                        Placement::Over if a == b => {
                            need(a.checked_sub(1), a, w / 2.0 + NOTE_GAP);
                            need(Some(a), a + 1, w / 2.0 + NOTE_GAP);
                        }
                        Placement::Over => need(Some(lo), hi, w - 2.0 * stage.set.note_over),
                    }
                    Some(View { kind: ViewKind::Note, a, b, label, x: 0.0, w })
                }
                Entry::Signal { kind, central, .. } if is_line_message(*kind) => {
                    let label = stage.message_text(&m.text, m.wrap, wrap_w);
                    let badge = stage.badge_width(i);
                    if a == b {
                        need(Some(a), a + 1, SELF_W + 8.0 + label.width + 16.0 + ACT_W + badge);
                        Some(View { kind: ViewKind::Loop, a, b, label, x: 0.0, w: 0.0 })
                    } else {
                        let created = if stage.cols[b].create_at == Some(i) { stage.cols[b].size.w / 2.0 } else { 0.0 };
                        let central = if central.is_some() { 4.0 * CENTRAL_R } else { 0.0 };
                        let text = if tight { js::min(label.width, js::max(16.0, wrap_w - 16.0)) } else { label.width };
                        need(Some(lo), hi, text + 2.0 * stage.set.msg_pad + ACT_W + badge + created + central);
                        Some(View { kind: ViewKind::Line, a, b, label, x: 0.0, w: 0.0 })
                    }
                }
                Entry::Signal { .. } => None,
            };
            stage.views.push(view);
        }

        for i in 0..n {
            let Some(in_box) = stage.cols[i].actor.in_box else {
                continue;
            };
            if let Some(run) = stage.runs.last_mut().filter(|run| run.in_box == in_box && run.last + 1 == i) {
                run.last = i;
                continue;
            }
            let named = &model.boxes[in_box];
            let title = stage.plain_text(named.name.as_deref().unwrap_or_default(), small, named.wrap, wrap_w);
            stage.runs.push(BoxRun { in_box, first: i, last: i, title, grow: 0.0 });
        }

        let mut gaps = Vec::with_capacity(n.saturating_sub(1));
        for i in 0..n.saturating_sub(1) {
            let (left, right) = (&stage.cols[i], &stage.cols[i + 1]);
            let pad_of = |col: &Column<'_>| if col.actor.in_box.is_some() { stage.set.box_pad } else { 0.0 };
            let boxed = if left.actor.in_box != right.actor.in_box { pad_of(left) + pad_of(right) } else { 0.0 };
            let apart = left.size.w / 2.0 + right.size.w / 2.0 + stage.set.actor_margin + boxed;
            gaps.push(js::max(apart, needs.get(&(i, i + 1)).copied().unwrap_or(0.0)));
        }
        // A message across several lifelines widens each gap it crosses by an equal share of what is missing.
        let mut spans: Vec<(usize, usize, f64)> =
            needs.iter().filter(|((lo, hi), _)| hi - lo > 1).map(|(&(lo, hi), &width)| (lo, hi, width)).collect();
        spans.sort_by_key(|&(lo, hi, _)| (hi - lo, lo));
        for (lo, hi, width) in spans {
            let mut total = 0.0;
            for gap in &gaps[lo..hi] {
                total += gap;
            }
            if total >= width {
                continue;
            }
            let share = (width - total) / (hi - lo) as f64;
            for gap in &mut gaps[lo..hi] {
                *gap += share;
            }
        }
        stage.gaps = gaps;
        stage.place();
        for run in &mut stage.runs {
            let (first, last) = (&stage.cols[run.first], &stage.cols[run.last]);
            let inner = last.x + last.size.w / 2.0 - (first.x - first.size.w / 2.0);
            run.grow = js::max(0.0, (run.title.width - inner) / 2.0);
            if run.grow == 0.0 {
                continue;
            }
            if run.first > 0 {
                stage.gaps[run.first - 1] += run.grow;
            }
            if run.last + 1 < n {
                stage.gaps[run.last] += run.grow;
            }
        }
        stage.layout();
        stage
    }

    fn place(&mut self) {
        for i in 1..self.cols.len() {
            self.cols[i].x = self.cols[i - 1].x + self.gaps[i - 1];
        }
    }

    fn box_edges(&self, run: &BoxRun) -> [f64; 2] {
        let (first, last) = (&self.cols[run.first], &self.cols[run.last]);
        [
            first.x - first.size.w / 2.0 - self.set.box_pad - run.grow,
            last.x + last.size.w / 2.0 + self.set.box_pad + run.grow,
        ]
    }

    /// Places the lifelines by the gaps, then gives every view and block its horizontal extent.
    fn layout(&mut self) {
        self.place();
        let mut extent = Extent::empty();
        for col in &self.cols {
            extent.extend(col.x - col.size.w / 2.0, col.x + col.size.w / 2.0);
        }
        for run in &self.runs {
            let [x0, x1] = self.box_edges(run);
            extent.extend(x0, x1);
        }

        // Horizontal extents of every block, from what it holds.
        let model: &'a SequenceModel = self.model;
        let messages = &model.messages;
        let (frame_pad, frame_nest, note_over, note_pad, msg_pad, wrap_w, tight, small) = (
            self.set.frame_pad,
            self.set.frame_nest,
            self.set.note_over,
            self.set.note_pad,
            self.set.msg_pad,
            self.set.wrap_w,
            self.set.tight,
            self.set.small,
        );
        let span_all = match (self.cols.first(), self.cols.last()) {
            (Some(first), Some(last)) => [first.x - frame_pad, last.x + frame_pad],
            _ => [0.0, 120.0],
        };
        let mut frames: Vec<Frame> = Vec::new();
        let mut frame_at: Vec<Option<usize>> = vec![None; messages.len()];
        let mut open: Vec<usize> = Vec::new();
        for (i, m) in messages.iter().enumerate() {
            let kind = m.kind();
            if let Some(frame_kind) = FrameKind::started_by(kind) {
                frame_at[i] = Some(frames.len());
                open.push(frames.len());
                frames.push(Frame {
                    kind: frame_kind,
                    x0: f64::INFINITY,
                    x1: f64::NEG_INFINITY,
                    top: 0.0,
                    title: Label::default(),
                    tab_w: 0.0,
                    fill: if frame_kind == FrameKind::Rect { m.text.clone() } else { JsText::default() },
                    dividers: vec![i],
                    divider_y: Vec::new(),
                    divider_labels: Vec::new(),
                });
            } else if matches!(kind, LineType::AltElse | LineType::ParAnd | LineType::CriticalOption) {
                if let Some(&current) = open.last() {
                    frames[current].dividers.push(i);
                }
            } else if matches!(
                kind,
                LineType::LoopEnd
                    | LineType::AltEnd
                    | LineType::OptEnd
                    | LineType::ParEnd
                    | LineType::CriticalEnd
                    | LineType::BreakEnd
                    | LineType::RectEnd
            ) {
                let Some(at) = open.pop() else {
                    continue;
                };
                frame_at[i] = Some(at);
                let frame = &mut frames[at];
                if frame.x0 > frame.x1 {
                    [frame.x0, frame.x1] = span_all;
                }
                if frame.kind != FrameKind::Rect {
                    let name: Vec<u16> = frame.kind.name().encode_utf16().collect();
                    frame.tab_w = width(self.measure, &name, self.set.tiny, Emphasis::BOLD) + 14.0;
                    // A block grows to the right to give its texts up to FRAME_TEXT of width before they wrap.
                    for k in 0..frame.dividers.len() {
                        let raw = &messages[frame.dividers[k]].text;
                        let tab = if k == 0 { frame.tab_w + 8.0 } else { 0.0 };
                        let room = frame.x1 - frame.x0 - 16.0 - tab;
                        let label = if raw.is_empty() {
                            Label::default()
                        } else {
                            let mut text = JsText::from("[");
                            text.push_units(raw);
                            text.push_str("]");
                            let least = if tight { js::min(FRAME_TEXT, wrap_w) } else { FRAME_TEXT };
                            self.plain_text(&text, small, true, js::max(room, least))
                        };
                        if label.width > room {
                            frame.x1 += label.width - room;
                        }
                        if k == 0 {
                            frame.title = label;
                        } else {
                            frame.divider_labels.push(label);
                        }
                    }
                }
                let (x0, x1) = (frame.x0, frame.x1);
                extent.extend(x0, x1);
                if let Some(&parent) = open.last() {
                    frames[parent].x0 = js::min(frames[parent].x0, x0 - frame_nest);
                    frames[parent].x1 = js::max(frames[parent].x1, x1 + frame_nest);
                }
            } else if let Some(mut view) = self.views[i].take() {
                let (ax, bx) = (self.cols[view.a].x, self.cols[view.b].x);
                let (x0, x1);
                match view.kind {
                    ViewKind::Note => {
                        let placement = match m.entry {
                            Entry::Note { placement } => placement,
                            _ => Placement::Over,
                        };
                        match placement {
                            Placement::LeftOf => view.x = ax - NOTE_GAP - view.w,
                            Placement::RightOf => view.x = ax + NOTE_GAP,
                            Placement::Over if view.a == view.b => view.x = ax - view.w / 2.0,
                            Placement::Over => {
                                let lo = js::min(ax, bx) - note_over;
                                let hi = js::max(ax, bx) + note_over;
                                if tight || m.wrap {
                                    let room = js::max(wrap_w, hi - lo - 2.0 * note_pad);
                                    view.label = self.plain_text(&m.text, small, true, room);
                                }
                                view.w = js::max(hi - lo, view.label.width + 2.0 * note_pad);
                                view.x = (lo + hi - view.w) / 2.0;
                            }
                        }
                        (x0, x1) = (view.x, view.x + view.w);
                    }
                    ViewKind::Loop => {
                        x0 = ax;
                        x1 = ax + ACT_W + SELF_W + 8.0 + view.label.width + self.badge_width(i);
                    }
                    ViewKind::Line => {
                        let (mut l, mut r) = (js::min(ax, bx), js::max(ax, bx));
                        if tight || m.wrap {
                            view.label = self.sized_text(i, js::max(wrap_w, r - l - 2.0 * msg_pad - ACT_W));
                        }
                        let b = &self.cols[view.b];
                        if b.create_at == Some(i) {
                            if bx > ax {
                                r += b.size.w / 2.0;
                            } else {
                                l -= b.size.w / 2.0;
                            }
                        }
                        (x0, x1) = (l, r);
                    }
                }
                self.views[i] = Some(view);
                extent.extend(x0, x1);
                if let Some(&current) = open.last() {
                    frames[current].x0 = js::min(frames[current].x0, x0 - frame_pad);
                    frames[current].x1 = js::max(frames[current].x1, x1 + frame_pad);
                }
            }
        }
        self.frames = frames;
        self.frame_at = frame_at;
        self.extent = extent;
    }

    /// Widens gaps only if it reduces message wrapping. Answers whether it did, and whether the
    /// gaps then share the spare width by what each is short of.
    fn widen(&mut self, hold: f64, by_need: bool) -> (bool, bool) {
        let Some(max_width) = self.set.max_width.filter(|_| self.set.tight) else {
            return (false, false);
        };
        let (msg_pad, pad) = (self.set.msg_pad, self.set.pad);
        let spare = max_width.floor() - (self.extent.max_x - self.extent.min_x + 2.0 * pad).ceil() - hold;
        let mut stretch = vec![0i32; self.cols.len()];
        let mut want = vec![0.0; self.gaps.len()];
        let mut wrapped = Vec::new();
        let mut lines = 0;
        if spare > 0.0 {
            for i in 0..self.views.len() {
                let Some(view) = self.views[i].as_ref().filter(|view| view.kind == ViewKind::Line) else {
                    continue;
                };
                let natural = self.message_text(&self.model.messages[i].text, false, f64::INFINITY);
                if view.label.lines.len() <= natural.lines.len() {
                    continue;
                }
                wrapped.push(i);
                lines += view.label.lines.len();
                let (lo, hi) = (view.a.min(view.b), view.a.max(view.b));
                stretch[lo] += 1;
                stretch[hi] -= 1;
                let room = self.cols[hi].x - self.cols[lo].x - 2.0 * msg_pad - ACT_W;
                let short = (natural.width - room) / (hi - lo) as f64;
                for gap in &mut want[lo..hi] {
                    *gap = js::max(*gap, short);
                }
            }
        }
        let mut active = 0;
        let mut crossed = 0.0;
        for crossing in stretch.iter_mut().take(self.gaps.len()) {
            active += *crossing;
            *crossing = i32::from(active > 0);
            crossed += f64::from(*crossing);
        }
        if crossed == 0.0 {
            return (false, false);
        }
        let before = self.gaps.clone();
        let sum = |counts: &[usize]| counts.iter().sum::<usize>();
        let mut missing = 0.0;
        for short in &want {
            missing += short;
        }
        let even: Vec<f64> = (0..before.len()).map(|g| (f64::from(stretch[g]) * spare) / crossed).collect();
        // Allow for rounding error.
        let needs: Vec<f64> = want.iter().map(|short| (short * (spare - 0.01)) / missing).collect();
        let mut counts = self.spread(&before, &even, &wrapped);
        let mut uneven = false;
        // Reducing wrapping requires at least one gap wider than the even split.
        if by_need && crossed > 1.0 && missing > 0.0 && needs.iter().zip(&even).any(|(need, even)| need > even) {
            let first = counts;
            counts = self.spread(&before, &needs, &wrapped);
            uneven = sum(&counts) < sum(&first) && counts.iter().zip(&first).all(|(now, then)| now <= then);
            if !uneven {
                counts = self.spread(&before, &even, &wrapped);
            }
        }
        let grown = sum(&counts) < lines;
        if !grown {
            self.gaps = before;
            self.layout();
        }
        (grown, uneven)
    }

    /// Lays out again with each gap wider by its share, and counts the lines of each wrapped message.
    fn spread(&mut self, before: &[f64], share: &[f64], wrapped: &[usize]) -> Vec<usize> {
        for (g, gap) in self.gaps.iter_mut().enumerate() {
            *gap = before[g] + share[g];
        }
        self.layout();
        wrapped.iter().map(|&i| self.views[i].as_ref().map_or(0, |view| view.label.lines.len())).collect()
    }
}

/// A label with what keeps it legible: a plate in the page's colour where a lifeline would
/// strike through it, or else the ink of the fill it lies on.
struct Backed {
    label: Label,
    at: Point,
    plate: bool,
    ink: Option<Role>,
}

/// What lies under the entries: the lifelines, the boxes and the shaded blocks open at the
/// entry the walk has reached.
struct Ground {
    /// The lifelines, left to right.
    xs: Vec<f64>,
    /// Each box's left and right edge and its colour.
    boxes: Vec<([f64; 2], JsText)>,
    /// The colours of the open shaded blocks, the innermost last.
    rects: Vec<JsText>,
    inks: HashMap<JsText, Option<Role>>,
}

/// The shape style a colour gives a fill: Pele's `resolveStyle(['fill:…']).shape`.
fn fill_style(fill: &[u16]) -> Option<Style> {
    let mut declaration = JsText::from("fill:");
    declaration.push_units(fill);
    resolve_style([&declaration]).shape
}

impl Ground {
    /// Whether a lifeline runs between two x positions, where text would be struck through.
    fn crosses(&self, x0: f64, x1: f64) -> bool {
        let (mut lo, mut hi) = (0, self.xs.len());
        while lo < hi {
            let mid = (lo + hi) >> 1;
            if self.xs[mid] <= x0 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo < self.xs.len() && self.xs[lo] < x1
    }

    fn ink_of(&mut self, fill: Option<&JsText>) -> Option<Role> {
        let fill = fill.filter(|fill| !fill.is_empty())?;
        if let Some(&known) = self.inks.get(fill) {
            return known;
        }
        let ink = ink_on(fill_style(fill).as_ref(), None);
        self.inks.insert(fill.clone(), ink);
        ink
    }

    /// Prefers the innermost rect's fill, then a containing box's.
    fn fill_under(&self, l: f64, r: f64) -> Option<JsText> {
        if let Some(fill) = self.rects.last() {
            return Some(fill.clone());
        }
        self.boxes.iter().find(|([x0, x1], _)| l >= *x0 && r <= *x1).map(|(_, fill)| fill.clone())
    }

    fn under(&mut self, l: f64, r: f64) -> Option<Role> {
        let fill = self.fill_under(l, r);
        self.ink_of(fill.as_ref())
    }

    fn backed(&mut self, label: &Label, cx: f64, cy: f64) -> Option<Backed> {
        if label.width == 0.0 {
            return None;
        }
        let l = cx - label.width / 2.0;
        let plate = self.crosses(l, l + label.width);
        let ink = if plate { None } else { self.under(l, l + label.width) };
        Some(Backed { label: label.clone(), at: Point { x: cx, y: cy }, plate, ink })
    }
}

#[derive(Clone, Copy)]
struct HeadAt {
    head: Head,
    x: f64,
    y: f64,
    dir: f64,
    /// The side a half head sits on.
    up: f64,
}

/// What a message draws besides its line and heads, in the order Pele writes them.
enum Mark {
    /// A central connection.
    Circle { cx: f64, cy: f64 },
    /// The message's number.
    Badge { cx: f64, cy: f64, w: f64 },
}

struct MessageDrawn {
    index: usize,
    /// The ink of the fill the message lies on, which it is drawn in.
    ink: Option<Role>,
    d: PathProgram,
    dotted: bool,
    start: Option<HeadAt>,
    end: Option<HeadAt>,
    marks: Vec<Mark>,
}

struct FrameDrawn {
    kind: FrameKind,
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
    tab_w: f64,
    dividers: Vec<(f64, Option<Backed>)>,
    title: Option<Backed>,
}

struct RectDrawn {
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
    fill: JsText,
}

struct ActivationDrawn {
    col: usize,
    x: f64,
    start: f64,
    bottom: f64,
}

struct NoteDrawn {
    index: usize,
    x: f64,
    top: f64,
    w: f64,
    h: f64,
    label: Label,
}

struct ActorDrawn {
    col: usize,
    top: f64,
    footer: bool,
}

struct BoxDrawn {
    run: usize,
    x0: f64,
    x1: f64,
    height: f64,
}

/// One list per layer, each in the order Pele writes it.
#[derive(Default)]
struct Drawing {
    boxes: Vec<BoxDrawn>,
    /// Shaded blocks close from the inside out. They are drawn in reverse, so that the inner
    /// ones end up on top, and so are the activations.
    rects: Vec<RectDrawn>,
    /// Each lifeline's column, top and bottom.
    lifelines: Vec<(usize, f64, f64)>,
    activations: Vec<ActivationDrawn>,
    frames: Vec<FrameDrawn>,
    messages: Vec<MessageDrawn>,
    /// The cross that ends a destroyed participant's lifeline: its column and centre.
    crosses: Vec<(usize, f64, f64)>,
    labels: Vec<(usize, Backed)>,
    notes: Vec<NoteDrawn>,
    actors: Vec<ActorDrawn>,
    title: Option<TitleLayout>,
}

/// One draw of the diagram, before it is written.
struct Attempt<'a> {
    stage: Stage<'a>,
    drawing: Drawing,
    extent: Extent,
    min_y: f64,
    width: f64,
    height: f64,
    /// Whether gaps were widened to reduce wrapping, and whether by what each was short of.
    grown: bool,
    uneven: bool,
}

fn left_edge(col: &Column<'_>, extra: usize) -> f64 {
    if col.activations.len() + extra > 0 { col.x - ACT_W / 2.0 } else { col.x }
}

fn right_edge(col: &Column<'_>, extra: usize) -> f64 {
    let depth = col.activations.len() + extra;
    if depth > 0 { col.x + ACT_W / 2.0 + (depth - 1) as f64 * ACT_STEP } else { col.x }
}

fn end_activation(
    cols: &mut [Column<'_>],
    col: usize,
    end: f64,
    extent: &mut Extent,
    out: &mut Vec<ActivationDrawn>,
) -> f64 {
    let start = cols[col].activations.pop().expect("an open activation");
    let bottom = js::max(end, start + ACT_MIN);
    let x = cols[col].x - ACT_W / 2.0 + cols[col].activations.len() as f64 * ACT_STEP;
    extent.extend(x, x + ACT_W);
    out.push(ActivationDrawn { col, x, start, bottom });
    bottom
}

/// Places every entry top to bottom, as Pele's `draw` does once the lifelines are spaced.
fn attempt<'a>(model: &'a SequenceModel, cx: &DrawContext<'a>, tight: bool, hold: f64, by_need: bool) -> Attempt<'a> {
    let mut stage = Stage::new(model, cx, tight);
    let (grown, uneven) = stage.widen(hold, by_need);
    let messages = &model.messages;
    let mut drawing = Drawing::default();
    let mut extent = stage.extent;
    let (msg_gap, msg_pad, note_pad, box_pad) =
        (stage.set.msg_gap, stage.set.msg_pad, stage.set.note_pad, stage.set.box_pad);
    let (right_angles, align, mirror, pad, size) =
        (stage.set.right_angles, stage.set.align, stage.set.mirror, stage.set.pad, stage.set.size);

    let mut box_title_h = 0.0;
    for run in &stage.runs {
        box_title_h = js::max(box_title_h, run.title.height);
    }
    let titled = if box_title_h > 0.0 { box_title_h + 4.0 } else { 0.0 };
    let box_top = if stage.runs.is_empty() { 0.0 } else { box_pad + titled };
    let mut row_h = 0.0;
    for col in &stage.cols {
        if col.create_at.is_none() {
            row_h = js::max(row_h, col.size.h);
        }
    }
    let head_bottom = box_top + row_h;
    for col in &mut stage.cols {
        col.top = head_bottom - col.size.h;
        col.life_start = head_bottom;
    }

    let mut ground = Ground {
        xs: stage.cols.iter().map(|col| col.x).collect(),
        boxes: stage.runs.iter().map(|run| (stage.box_edges(run), model.boxes[run.in_box].fill.clone())).collect(),
        rects: Vec::new(),
        inks: HashMap::new(),
    };
    let mut stack: Vec<usize> = Vec::new();

    let mut y = head_bottom + 6.0;
    for (i, m) in messages.iter().enumerate() {
        let kind = m.kind();
        let frame = stage.frame_at[i];
        if kind == LineType::ActiveStart {
            if let Some(col) = stage.col(m.from.as_ref()) {
                stage.cols[col].activations.push(y);
            }
        } else if kind == LineType::ActiveEnd {
            if let Some(col) = stage.col(m.from.as_ref()).filter(|&col| !stage.cols[col].activations.is_empty()) {
                y = js::max(y, end_activation(&mut stage.cols, col, y, &mut extent, &mut drawing.activations));
            }
        } else if let Some(at) = frame.filter(|&at| stage.frames[at].dividers[0] == i) {
            stack.push(at);
            let frame = &mut stage.frames[at];
            if frame.kind == FrameKind::Rect {
                ground.rects.push(frame.fill.clone());
                frame.top = y + 8.0;
                y = frame.top;
            } else {
                frame.top = y + 12.0;
                y = frame.top + js::max(TAB_H, frame.title.height + 2.0);
            }
        } else if let Some(at) = frame {
            stack.pop();
            let frame = &stage.frames[at];
            let bottom = y + 12.0;
            if frame.kind == FrameKind::Rect {
                ground.rects.pop();
                let fill = frame.fill.clone();
                drawing.rects.push(RectDrawn { x0: frame.x0, x1: frame.x1, top: frame.top, bottom, fill });
            } else {
                let mut dividers = Vec::with_capacity(frame.divider_y.len());
                for (k, &dy) in frame.divider_y.iter().enumerate() {
                    let label = &frame.divider_labels[k];
                    let text = ground.backed(label, frame.x0 + 8.0 + label.width / 2.0, dy + 2.0 + label.height / 2.0);
                    dividers.push((dy, text));
                }
                let title = ground.backed(
                    &frame.title,
                    frame.x0 + frame.tab_w + 8.0 + frame.title.width / 2.0,
                    frame.top + js::max(TAB_H, frame.title.height) / 2.0,
                );
                drawing.frames.push(FrameDrawn {
                    kind: frame.kind,
                    x0: frame.x0,
                    x1: frame.x1,
                    top: frame.top,
                    bottom,
                    tab_w: frame.tab_w,
                    dividers,
                    title,
                });
            }
            y = bottom;
        } else if matches!(kind, LineType::AltElse | LineType::ParAnd | LineType::CriticalOption) {
            if let Some(&current) = stack.last() {
                let frame = &mut stage.frames[current];
                let height = frame.divider_labels.get(frame.divider_y.len()).map_or(0.0, |label| label.height);
                frame.divider_y.push(y + 12.0);
                y += 12.0 + if height > 0.0 { height } else { 0.0 };
            }
        } else if let Some(view) = stage.views[i].as_ref().filter(|view| view.kind == ViewKind::Note) {
            let a = &stage.cols[view.a];
            let h = view.label.height + 2.0 * js::max(4.0, note_pad - 4.0);
            let top = y + 10.0;
            let mut x = view.x;
            match m.entry {
                Entry::Note { placement: Placement::RightOf } => x += right_edge(a, 0) - a.x,
                Entry::Note { placement: Placement::LeftOf } => x += left_edge(a, 0) - a.x,
                _ => {}
            }
            extent.extend(x, x + view.w);
            drawing.notes.push(NoteDrawn { index: i, x, top, w: view.w, h, label: view.label.clone() });
            y = top + h;
        } else if let Some(view) = &stage.views[i] {
            let st = line_style(kind).expect("a view that is no note is a message line");
            let (ai, bi) = (view.a, view.b);
            let label = &view.label;
            let bw = stage.badge_width(i);
            // The activation a `+` starts belongs to this message, so the arrow stops at its edge.
            let mut starting = 0;
            for later in &messages[i + 1..] {
                match later.kind() {
                    LineType::ActiveStart => starting += usize::from(later.from == m.to),
                    LineType::ActiveEnd | LineType::CentralConnection | LineType::CentralConnectionReverse => {}
                    _ => break,
                }
            }
            let up = if st.bottom { 1.0 } else { -1.0 };
            let central = match m.entry {
                Entry::Signal { central, .. } => central,
                _ => None,
            };
            let circle_to = central.is_some_and(Central::at_receiver);
            let circle_from = central.is_some_and(Central::at_sender);
            let tail_at_end = st.at_start && !st.at_end;
            let head_trim = st.head.trim();
            let mut marks = Vec::new();
            let head = |x: f64, y: f64, dir: f64, up: f64| HeadAt { head: st.head, x, y, dir, up };
            let (ink, d, start, end, line_y);

            if view.kind == ViewKind::Loop {
                let a = &stage.cols[ai];
                let top = y + msg_gap + js::max(0.0, (label.height - SELF_H) / 2.0);
                let bottom = top + SELF_H;
                let mut x0 = if circle_from { a.x + CENTRAL_R } else { right_edge(a, 0) };
                let mut x1 = if circle_to { a.x + CENTRAL_R } else { right_edge(a, starting) };
                let out = js::max(x0, x1) + SELF_W + bw / 2.0;
                ink = ground.under(a.x, out);
                if circle_from {
                    marks.push(Mark::Circle { cx: a.x, cy: top });
                }
                if circle_to {
                    marks.push(Mark::Circle { cx: a.x, cy: bottom });
                }
                if bw > 0.0 {
                    let (cx, cy) = if tail_at_end { (x1, bottom) } else { (x0, top) };
                    marks.push(Mark::Badge { cx, cy, w: bw });
                    if tail_at_end {
                        x1 += bw / 2.0;
                    } else {
                        x0 += bw / 2.0;
                    }
                }
                // The loop leaves to the right and comes back heading left, which turns a half head over.
                start = st.at_start.then(|| head(x0, top, -1.0, up));
                end = st.at_end.then(|| head(x1, bottom, -1.0, -up));
                let from = Number::num(x0 + if st.at_start { head_trim } else { 0.0 });
                let to = Number::num(x1 + if st.at_end { head_trim } else { 0.0 });
                let path = PathProgram::default().m(from, Number::num(top));
                d = if right_angles {
                    path.h(Number::num(out)).v(Number::num(bottom)).h(to)
                } else {
                    let radii = [Number::raw(6.0), Number::raw(6.0)];
                    path.h(Number::num(out - 6.0))
                        .a(radii, true, [Number::num(out), Number::num(top + 6.0)])
                        .v(Number::num(bottom - 6.0))
                        .a(radii, true, [Number::num(out - 6.0), Number::num(bottom)])
                        .h(to)
                };
                extent.extend(a.x, out + 8.0 + label.width);
                if label.width > 0.0 {
                    let ink = ground.under(out + 8.0, out + 8.0 + label.width);
                    let at = Point { x: out + 8.0 + label.width / 2.0, y: (top + bottom) / 2.0 };
                    drawing.labels.push((i, Backed { label: label.clone(), at, plate: false, ink }));
                }
                // A label taller than the loop reaches below it, so the next row starts under the label.
                line_y = js::max(bottom, (top + bottom) / 2.0 + label.height / 2.0);
            } else {
                let (ax, bx) = (stage.cols[ai].x, stage.cols[bi].x);
                let dir = if bx > ax { 1.0 } else { -1.0 };
                let creating = stage.cols[bi].create_at == Some(i);
                ink = ground.under(js::min(ax, bx), js::max(ax, bx));
                let size = stage.cols[bi].size;
                let text_room = if label.height > 0.0 { label.height + 2.0 } else { 6.0 };
                line_y = y + msg_gap + js::max(text_room, if creating { size.h / 2.0 - msg_gap + 6.0 } else { 0.0 });
                let (a, b) = (&stage.cols[ai], &stage.cols[bi]);
                let mut x0 = if dir > 0.0 { right_edge(a, 0) } else { left_edge(a, 0) };
                let mut x1 = if dir > 0.0 { left_edge(b, starting) } else { right_edge(b, starting) };
                if creating {
                    x1 = bx - dir * if b.kind.is_figure() { 18.0 } else { size.w / 2.0 + 1.0 };
                    let created = &mut stage.cols[bi];
                    created.top = line_y - size.h / 2.0;
                    created.life_start = created.top + size.h;
                }
                if circle_from {
                    marks.push(Mark::Circle { cx: ax, cy: line_y });
                    x0 = ax + dir * CENTRAL_R;
                }
                if circle_to {
                    marks.push(Mark::Circle { cx: bx, cy: line_y });
                    x1 = bx - dir * CENTRAL_R;
                }
                if bw > 0.0 {
                    // The number sits on the lifeline at the tail of the arrow, past a connection circle if there is one.
                    if tail_at_end {
                        let cx = if circle_to { x1 - dir * (bw / 2.0 + 2.0) } else { x1 };
                        marks.push(Mark::Badge { cx, cy: line_y, w: bw });
                        x1 = cx - (dir * bw) / 2.0;
                    } else {
                        let cx = if circle_from { x0 + dir * (bw / 2.0 + 2.0) } else { x0 };
                        marks.push(Mark::Badge { cx, cy: line_y, w: bw });
                        x0 = cx + (dir * bw) / 2.0;
                    }
                }
                start = st.at_start.then(|| head(x0, line_y, -dir, up * dir));
                end = st.at_end.then(|| head(x1, line_y, dir, up * dir));
                let from = x0 + if st.at_start { dir * head_trim } else { 0.0 };
                let to = x1 - if st.at_end { dir * head_trim } else { 0.0 };
                d = PathProgram::default().m(Number::num(from), Number::num(line_y)).h(Number::num(to));
                if label.width > 0.0 {
                    let (l, r) = (js::min(x0, x1), js::max(x0, x1));
                    let cx = match align {
                        Align::Left => l + msg_pad + label.width / 2.0,
                        Align::Right => r - msg_pad - label.width / 2.0,
                        Align::Centre => (l + r) / 2.0,
                    };
                    extent.extend(cx - label.width / 2.0, cx + label.width / 2.0);
                    if let Some(text) = ground.backed(label, cx, line_y - 4.0 - label.height / 2.0) {
                        drawing.labels.push((i, text));
                    }
                }
                if creating {
                    y = line_y + size.h / 2.0 - 6.0;
                }
            }
            drawing.messages.push(MessageDrawn { index: i, ink, d, dotted: st.dotted, start, end, marks });
            y = js::max(y, line_y);
            // The lifeline of a destroyed participant ends in a cross a little below the message that ends it.
            let ends =
                [(ai, stage.cols[ai].destroy_at == Some(i)), (bi, stage.cols[bi].destroy_at == Some(i) && bi != ai)];
            for (col, destroyed) in ends {
                if destroyed {
                    let (cx, cy) = (stage.cols[col].x, line_y + 16.0);
                    stage.cols[col].life_end = Some(cy);
                    drawing.crosses.push((col, cx, cy));
                    y = js::max(y, cy + 6.0);
                }
            }
        }
    }

    y += 20.0;
    for col in 0..stage.cols.len() {
        while !stage.cols[col].activations.is_empty() {
            end_activation(&mut stage.cols, col, y, &mut extent, &mut drawing.activations);
        }
    }
    let mut bottom = y;
    if mirror {
        let mut foot_h = 0.0;
        for (col, column) in stage.cols.iter().enumerate() {
            if column.life_end.is_some() {
                continue;
            }
            foot_h = js::max(foot_h, column.size.h);
            drawing.actors.push(ActorDrawn { col, top: y, footer: true });
        }
        bottom = y + foot_h;
    }
    for (col, column) in stage.cols.iter().enumerate() {
        drawing.actors.push(ActorDrawn { col, top: column.top, footer: false });
        let end = column.life_end.unwrap_or(y);
        if end > column.life_start {
            drawing.lifelines.push((col, column.life_start, end));
        }
    }

    for (run, ([x0, x1], _)) in ground.boxes.iter().enumerate() {
        drawing.boxes.push(BoxDrawn { run, x0: *x0, x1: *x1, height: bottom + box_pad });
    }
    if !stage.runs.is_empty() {
        bottom += box_pad;
    }

    let mut min_y = 0.0;
    let room = fit::title_room(cx.options);
    let title = model.title.as_deref().unwrap_or_default();
    let limit = if tight { room } else { f64::INFINITY };
    let plain = layout_label(title, false, cx.measure, size, limit, Emphasis::BOLD);
    let title = if tight { break_label(&plain, cx.measure, js::max(120.0, room)) } else { plain };
    if extent.min_x > extent.max_x {
        extent.extend(0.0, js::max(title.width, 40.0));
    }
    if title.width > 0.0 {
        let cx = (extent.min_x + extent.max_x) / 2.0;
        extent.extend(cx - title.width / 2.0, cx + title.width / 2.0);
        min_y = -title.height - 12.0;
        drawing.title = Some(TitleLayout { at: Point { x: cx, y: min_y + title.height / 2.0 }, label: title });
    }

    let width = (extent.max_x - extent.min_x + 2.0 * pad).ceil();
    let height = (bottom - min_y + 2.0 * pad).ceil();
    Attempt { stage, drawing, extent, min_y, width, height, grown, uneven }
}

/// The entity of each message line and note, by the entry's place among the messages.
fn entry_parts(out: &mut SceneBuilder, stage: &Stage<'_>, actors: &[PartId]) -> Vec<Option<PartId>> {
    let mut ranks: HashMap<EntityName, u32> = HashMap::new();
    let mut rank = |name: &EntityName| {
        let rank = ranks.entry(name.clone()).or_insert(0);
        *rank += 1;
        *rank - 1
    };
    let mut parts = Vec::with_capacity(stage.views.len());
    for (i, view) in stage.views.iter().enumerate() {
        let Some(view) = view else {
            parts.push(None);
            continue;
        };
        let m = &stage.model.messages[i];
        let written = JsText::from(format!("i{i}").as_str());
        let ends = |kind: String| EntityName::Between {
            from: m.from.as_deref().unwrap_or_default().to_vec(),
            to: m.to.as_deref().unwrap_or_default().to_vec(),
            kind,
        };
        let part = match m.entry {
            Entry::Note { placement } => {
                let name = ends(format!("note {}", placement.number()));
                let rank = rank(&name);
                out.part(Entity::note_between(&written, name, rank))
            }
            _ => {
                let name = ends(m.kind().number().to_string());
                let rank = rank(&name);
                let part = out.part(Entity::edge(&written, name, rank));
                out.relation(part, actors[view.a], actors[view.b]);
                part
            }
        };
        parts.push(Some(part));
    }
    parts
}

fn draw_head(out: &mut SceneBuilder, part: PartId, facet: Facet, at: HeadAt, colour: Paint) {
    let HeadAt { head, x, y, dir, up } = at;
    let back = Number::num(x - dir * ARROW);
    let tip = PathProgram::default().m(Number::num(x), Number::num(y));
    match head {
        Head::None => {}
        Head::Arrow => {
            draw_marker(out, part, facet, Marker::Point, Tip { x, y, dx: dir, dy: 0.0 }, MarkerUse::Edge(colour));
        }
        Head::Cross => {
            draw_marker(out, part, facet, Marker::Cross, Tip { x, y, dx: dir, dy: 0.0 }, MarkerUse::Edge(colour));
        }
        Head::Open => {
            let d = PathProgram::default()
                .m(back, Number::num(y - 4.5))
                .l(Number::num(x), Number::num(y))
                .l(back, Number::num(y + 4.5));
            out.element(part, facet).class("pele-marker").path(d).join(LineJoin::Round).finish();
        }
        Head::HalfSolid => {
            let d = tip.l(back, Number::num(y + up * 4.5)).v(Number::num(y)).z();
            out.element(part, facet).class("pele-marker").path(d).fill(colour).stroke(Paint::None).finish();
        }
        Head::HalfStick => {
            let d = tip.l(back, Number::num(y + up * 5.0));
            out.element(part, facet).class("pele-marker").path(d).finish();
        }
    }
}

/// Pele's `backed`: the plate, where there is one, and the label on it.
fn draw_backed(out: &mut SceneBuilder, part: PartId, facet: Facet, text: &Backed, frame: bool) {
    let Backed { label, at, plate, ink } = text;
    if *plate {
        let left = at.x - label.width / 2.0;
        out.element(part, facet)
            .rect(
                [Number::num(left - 3.0), Number::num(at.y - label.height / 2.0 + 2.0)],
                [Number::num(label.width + 6.0), Number::num(label.height - 4.0)],
                Some(Number::raw(3.0)),
            )
            .fill(Paint::Role(Role::Background))
            .stroke(Paint::None)
            .finish();
    }
    let look = if frame {
        LabelLook::class("pele-frame-label")
            .fill(Some(Paint::Role(ink.unwrap_or(Role::Muted))))
            .stroke_paint(Paint::None)
    } else {
        LabelLook::default().fill(ink.map(Paint::Role))
    };
    out.label(part, facet, label, *at, look);
}

/// What follows the geometry of a rectangle a colour may fill: Pele's
/// `rounded((fill ? '' : ' fill-opacity="0.5"') + fill)`.
fn washed(fill: Option<Style>) -> (Option<Number>, Style) {
    (fill.is_none().then(|| Number::raw(0.5)), Style::rounded(fill.as_ref()))
}

fn emit(model: &SequenceModel, cx: &DrawContext<'_>, attempt: Attempt<'_>) -> Result<Scene, RenderError> {
    let Attempt { stage, drawing, extent, min_y, width: total_width, height, .. } = attempt;
    let set = &stage.set;
    let (small, tiny) = (set.small, set.tiny);
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, Size { width: total_width, height }, accessible);
    let actors: Vec<PartId> = stage.cols.iter().map(|col| out.part(Entity::node(col.id))).collect();
    let entries = entry_parts(&mut out, &stage, &actors);
    let entry = |i: usize| entries[i].expect("a drawn entry has a part");
    let pair = |x: f64, y: f64| [Number::num(x), Number::num(y)];
    let rounded = || Some(Style::rounded(None));
    let dash = [Number::raw(4.0), Number::raw(4.0)];
    // Blocks and boxes have no id of their own: each is its kind, or its title, and its place
    // among those named alike.
    let mut ranks: HashMap<JsText, u32> = HashMap::new();
    let mut group = |out: &mut SceneBuilder, name: &[u16]| {
        let name = JsText::from(name);
        let rank = ranks.entry(name.clone()).or_insert(0);
        *rank += 1;
        out.part(Entity::group(&name).ranked(*rank - 1))
    };

    let origin = Point { x: set.pad - extent.min_x, y: set.pad - min_y };
    out.layer(LayerSpec::group().translate(origin).keep_empty(), |out| {
        if let Some(title) = &drawing.title {
            out.title(title);
        }

        out.layer(LayerSpec::class("pele-boxes").font_size(Number::raw(small)), |out| {
            for drawn in &drawing.boxes {
                let run = &stage.runs[drawn.run];
                let named = &model.boxes[run.in_box];
                let part = group(out, named.name.as_deref().unwrap_or_default());
                let mut layer = LayerSpec::class("pele-cluster pele-box").part(part, Facet::Shape);
                if named.name.is_some() {
                    layer = layer.data_id();
                }
                out.layer(layer, |out| {
                    let fill = if named.fill.is_str("transparent") { None } else { fill_style(&named.fill) };
                    let ink = ink_on(fill_style(&named.fill).as_ref(), None).unwrap_or(Role::Muted);
                    let (opacity, style) = washed(fill);
                    let mut rect = out
                        .element(part, Facet::Shape)
                        .rect([Number::num(drawn.x0), Number::raw(0.0)], pair(drawn.x1 - drawn.x0, drawn.height), None)
                        .fill(Paint::Role(Role::SurfaceAlt))
                        .stroke(Paint::Role(Role::Border));
                    if let Some(opacity) = opacity {
                        rect = rect.fill_opacity(opacity);
                    }
                    rect.style(Some(style)).finish();
                    let at =
                        Point { x: (drawn.x0 + drawn.x1) / 2.0, y: set.box_pad / 2.0 + 2.0 + run.title.height / 2.0 };
                    let look = LabelLook::class("pele-cluster-label").fill(Some(Paint::Role(ink)));
                    out.label(part, Facet::Title, &run.title, at, look);
                });
            }
        });

        out.layer(LayerSpec::class("pele-rects").fill(Paint::Role(Role::SurfaceAlt)), |out| {
            let rect: Vec<u16> = "rect".encode_utf16().collect();
            let parts: Vec<PartId> = drawing.rects.iter().map(|_| group(out, &rect)).collect();
            for (drawn, &part) in drawing.rects.iter().zip(&parts).rev() {
                let fill = if drawn.fill.is_empty() { None } else { fill_style(&drawn.fill) };
                let (opacity, style) = washed(fill);
                let mut rect = out.element(part, Facet::Shape).class("pele-rect").rect(
                    pair(drawn.x0, drawn.top),
                    pair(drawn.x1 - drawn.x0, drawn.bottom - drawn.top),
                    None,
                );
                if let Some(opacity) = opacity {
                    rect = rect.fill_opacity(opacity);
                }
                rect.style(Some(style)).finish();
            }
        });

        let lifelines = LayerSpec::class("pele-lifelines").fill(Paint::None).stroke(Paint::Role(Role::Border));
        out.layer(lifelines, |out| {
            for &(col, start, end) in &drawing.lifelines {
                let x = Number::num(stage.cols[col].x);
                let d = PathProgram::default().m(x, Number::num(start)).v(Number::num(end));
                out.element(actors[col], Facet::Shape).class("pele-lifeline").data_id().path(d).finish();
            }
        });

        let activations =
            LayerSpec::class("pele-activations").fill(Paint::Role(Role::Surface)).stroke(Paint::Role(Role::Border));
        out.layer(activations, |out| {
            for drawn in drawing.activations.iter().rev() {
                let size = [Number::raw(ACT_W), Number::num(drawn.bottom - drawn.start)];
                out.element(actors[drawn.col], Facet::Shape)
                    .class("pele-activation")
                    .data_id()
                    .rect(pair(drawn.x, drawn.start), size, Some(Number::raw(2.0)))
                    .finish();
            }
        });

        let frames = LayerSpec::class("pele-frames").font_size(Number::raw(small)).stroke(Paint::Role(Role::Border));
        out.layer(frames, |out| {
            for drawn in &drawing.frames {
                let kind = drawn.kind.name();
                let name: Vec<u16> = kind.encode_utf16().collect();
                let part = group(out, &name);
                let layer = LayerSpec::class(&format!("pele-frame pele-frame-{kind}")).part(part, Facet::Shape);
                out.layer(layer, |out| {
                    let (x, top) = (Number::num(drawn.x0), Number::num(drawn.top));
                    out.element(part, Facet::Shape)
                        .rect([x, top], pair(drawn.x1 - drawn.x0, drawn.bottom - drawn.top), None)
                        .fill(Paint::None)
                        .style(rounded())
                        .finish();
                    for (dy, text) in &drawn.dividers {
                        let d = PathProgram::default().m(x, Number::num(*dy)).h(Number::num(drawn.x1));
                        out.element(part, Facet::Shape).path(d).dash(&dash).finish();
                        if let Some(text) = text {
                            draw_backed(out, part, Facet::Label, text, true);
                        }
                    }
                    out.element(part, Facet::Shape)
                        .rect([x, top], [Number::num(drawn.tab_w), Number::raw(TAB_H)], None)
                        .fill(Paint::Role(Role::Surface))
                        .style(rounded())
                        .finish();
                    let at = pair(drawn.x0 + drawn.tab_w / 2.0, drawn.top + TAB_H / 2.0 + tiny * 0.35);
                    out.text(part, Facet::Stereotype)
                        .class("pele-frame-kind")
                        .at(at)
                        .anchor(AnchorKind::Middle)
                        .font_size(Number::raw(tiny))
                        .weight(TextWeight::Heading)
                        .fill(Some(Paint::Role(Role::Muted)))
                        .stroke(Paint::None)
                        .finish(&name, drawn.tab_w - 14.0, tiny);
                    if let Some(title) = &drawn.title {
                        draw_backed(out, part, Facet::Title, title, true);
                    }
                });
            }
        });

        let edges =
            LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
        out.layer(edges, |out| {
            for drawn in &drawing.messages {
                let part = entry(drawn.index);
                let colour = Paint::Role(drawn.ink.unwrap_or(Role::Line));
                let mut layer = LayerSpec::class("pele-edge pele-message").part(part, Facet::Shape).data_id();
                if let Some(ink) = drawn.ink {
                    layer = layer.stroke(Paint::Role(ink));
                }
                out.layer(layer, |out| {
                    let path = out.element(part, Facet::Shape).path(drawn.d.clone());
                    if drawn.dotted { path.dash(&dash) } else { path }.finish();
                    if let Some(head) = drawn.start {
                        draw_head(out, part, Facet::StartMarker, head, colour);
                    }
                    if let Some(head) = drawn.end {
                        draw_head(out, part, Facet::EndMarker, head, colour);
                    }
                    for mark in &drawn.marks {
                        match *mark {
                            Mark::Circle { cx, cy } => out
                                .element(part, Facet::Shape)
                                .class("pele-central")
                                .circle_at([Some(Number::num(cx)), Some(Number::num(cy))], Number::raw(CENTRAL_R))
                                .fill(colour)
                                .stroke(Paint::None)
                                .finish(),
                            Mark::Badge { cx, cy, w } => out.layer(LayerSpec::class("pele-sequence-number"), |out| {
                                out.element(part, Facet::StartLabel)
                                    .rect(
                                        pair(cx - w / 2.0, cy - BADGE_H / 2.0),
                                        [Number::num(w), Number::raw(BADGE_H)],
                                        Some(Number::raw(BADGE_H / 2.0)),
                                    )
                                    .fill(Paint::Role(Role::Line))
                                    .stroke(Paint::None)
                                    .finish();
                                let number: Vec<u16> =
                                    js::to_string(stage.numbering.index[drawn.index]).encode_utf16().collect();
                                let measured = width(stage.measure, &number, tiny, Emphasis::default());
                                out.text(part, Facet::StartLabel)
                                    .at(pair(cx, cy + tiny * 0.35))
                                    .anchor(AnchorKind::Middle)
                                    .font_size(Number::raw(tiny))
                                    .fill(Some(Paint::Role(Role::Background)))
                                    .stroke(Paint::None)
                                    .finish(&number, measured, tiny);
                            }),
                        }
                    }
                });
            }
        });

        let destroys = LayerSpec::class("pele-destroys")
            .fill(Paint::None)
            .stroke(Paint::Role(Role::Line))
            .stroke_width(Number::raw(1.5))
            .cap(LineCap::Round);
        out.layer(destroys, |out| {
            for &(col, cx, cy) in &drawing.crosses {
                let d = PathProgram::default()
                    .m(Number::num(cx - 6.0), Number::num(cy - 6.0))
                    .l(Number::num(cx + 6.0), Number::num(cy + 6.0))
                    .m(Number::num(cx + 6.0), Number::num(cy - 6.0))
                    .l(Number::num(cx - 6.0), Number::num(cy + 6.0));
                out.element(actors[col], Facet::Shape).class("pele-destroy").data_id().path(d).finish();
            }
        });

        let mut labels = LayerSpec::class("pele-edge-labels").font_size(Number::raw(small));
        if set.monospaced {
            labels = labels.mono();
        }
        out.layer(labels, |out| {
            for (index, text) in &drawing.labels {
                let part = entry(*index);
                out.layer(LayerSpec::class("pele-edge-label").part(part, Facet::Label).data_id(), |out| {
                    draw_backed(out, part, Facet::Label, text, false);
                });
            }
        });

        let notes = LayerSpec::class("pele-notes")
            .font_size(Number::raw(small))
            .fill(Paint::Role(Role::SurfaceAlt))
            .stroke(Paint::Role(Role::Border));
        out.layer(notes, |out| {
            for drawn in &drawing.notes {
                let part = entry(drawn.index);
                out.layer(LayerSpec::class("pele-note").part(part, Facet::Shape).data_id(), |out| {
                    out.element(part, Facet::Shape)
                        .rect(pair(drawn.x, drawn.top), pair(drawn.w, drawn.h), None)
                        .style(rounded())
                        .finish();
                    let at = Point { x: drawn.x + drawn.w / 2.0, y: drawn.top + drawn.h / 2.0 };
                    let look = LabelLook::class("pele-label")
                        .stroke_paint(Paint::None)
                        .fill(Some(Paint::Role(Role::Foreground)));
                    out.label(part, Facet::Label, &drawn.label, at, look);
                });
            }
        });

        out.layer(LayerSpec::class("pele-nodes"), |out| {
            let class = JsText::from("class");
            for drawn in &drawing.actors {
                let col = &stage.cols[drawn.col];
                let part = actors[drawn.col];
                let classes = match col.actor.properties.get(&class) {
                    Some(JsValue::String(names)) => AuthorClasses::new(names),
                    _ => AuthorClasses::default(),
                };
                let footer = if drawn.footer { " pele-actor-footer" } else { "" };
                let own = format!("pele-node pele-actor pele-actor-{}{footer}", col.kind.name());
                out.layer(LayerSpec::classes(&own, &classes).part(part, Facet::Shape).data_id(), |out| {
                    // A figure's label sits on whatever is behind the figure.
                    let behind = col.actor.in_box.filter(|_| col.kind.is_figure());
                    let ink = behind.and_then(|in_box| ink_on(fill_style(&model.boxes[in_box].fill).as_ref(), None));
                    draw_actor(out, part, col.kind, col.x, drawn.top, col.size, &col.label, ink);
                });
            }
        });
    });
    out.finish()
}

/// Pele's `draw`, with the draws it retries: where widening the gaps made the diagram too wide,
/// it draws again with equal shares, and then with room held back for what the gaps cannot give.
fn draw(model: &SequenceModel, cx: &DrawContext<'_>, tight: bool) -> Result<Scene, RenderError> {
    let (mut hold, mut by_need) = (0.0, true);
    loop {
        let attempt = attempt(model, cx, tight, hold, by_need);
        let Some(max_width) = cx.options.max_width.filter(|&max| attempt.grown && attempt.width > max) else {
            return emit(model, cx, attempt);
        };
        if !attempt.uneven {
            hold = if hold > 0.0 { f64::INFINITY } else { attempt.width - max_width.floor() };
        }
        by_need = false;
    }
}

pub(crate) struct Sequence;

impl Diagram for Sequence {
    type Model = SequenceModel;

    fn parse(cx: &ParseContext<'_>) -> Result<SequenceModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderSequence`: drawn with room to spare, and tightly instead where that does
    /// not fit the width the host has and is narrower.
    fn scene(model: &SequenceModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        fit::tighten(cx.options, |tight| draw(model, cx, tight))
    }
}
