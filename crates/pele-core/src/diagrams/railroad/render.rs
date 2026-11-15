//! Laying out and drawing the rule model, ported from Pele's `railroad/render.ts`. Each rule is
//! measured into boxes bottom-up, the boxes are placed with an explicit stack, and the placed
//! track, arrowheads and boxes are drawn in the order Pele writes them.

use super::{Grammar, Node, NodeId, RailroadModel};
use crate::catalog::DrawContext;
use crate::input::{JsObject, JsText, JsValue, Units, decode_entities};
use crate::measure::{Emphasis, TextMeasurer, layout_label, width};
use crate::scene::{
    AnchorKind, Entity, Facet, LayerSpec, Number, Paint, PartId, PathProgram, PathStep, Point, Role, Scene,
    SceneBuilder, ShapeElement, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, fit, js};

const LEAD: f64 = 12.0;
const HEAD_GAP: f64 = 6.0;
const RULE_GAP: f64 = 18.0;
const COUNT_SIZE: f64 = 12.0;
const ARROW: f64 = 7.0;

const MONO: Emphasis = Emphasis { mono: true, ..PLAIN };
const ITALIC: Emphasis = Emphasis { italic: true, ..PLAIN };
const PLAIN: Emphasis =
    Emphasis { bold: false, italic: false, mono: false, underline: false, script: crate::measure::Script::Normal };

struct Metrics<'a> {
    measure: &'a dyn TextMeasurer,
    size: f64,
    padding: f64,
    height: f64,
    /// Space between elements in a sequence, and between stacked branches.
    gap: f64,
    sep: f64,
    arc: f64,
}

/// A number from the `railroad` section of the config: one that is not negative, at most `max`.
fn option(config: &JsObject, key: &str, fallback: f64, max: f64) -> f64 {
    let value = config.get("railroad").and_then(JsValue::as_record).and_then(|section| section.get(key));
    match value.and_then(JsValue::as_number) {
        Some(value) if value >= 0.0 => js::min(value, max),
        _ => fallback,
    }
}

/// Text is literal here: a terminal such as `<br>` or `*` is itself, not markup. Mermaid's `#35;`
/// codes still stand for characters, and escapes that would be invisible are written out.
fn shown(text: &[u16]) -> Vec<u16> {
    let decoded;
    let text = if text.has("¶ß") {
        let mut escaped = Vec::with_capacity(text.len());
        for &c in text {
            if c == u16::from(b'&') {
                escaped.extend("&amp;".encode_utf16());
            } else {
                escaped.push(c);
            }
        }
        decoded = decode_entities(&escaped);
        decoded.as_slice()
    } else {
        text
    };
    let mut out = Vec::with_capacity(text.len());
    for &c in text {
        match c {
            10 => out.extend("\\n".encode_utf16()),
            13 => out.extend("\\r".encode_utf16()),
            9 => out.extend("\\t".encode_utf16()),
            _ => out.push(c),
        }
    }
    out
}

fn even(value: f64) -> f64 {
    (value / 2.0).ceil() * 2.0
}

/// What a leaf box shows, and what it refers to.
enum Leaf {
    Terminal,
    /// The name of the rule it stands for.
    NonTerminal(JsText),
    Special,
}

/// What a laid out box is made of.
enum Shape {
    Leaf {
        leaf: Leaf,
        text: JsText,
        /// How wide the text is, before the padding around it.
        advance: f64,
    },
    Sequence(Vec<usize>),
    /// A sequence too wide for its room, as rows inside rails.
    Folded(Vec<usize>),
    Choice(Vec<usize>),
    /// A bypass over the box: `reach` is how far it rises above the baseline.
    Bypass {
        kid: usize,
        reach: f64,
    },
    /// A loop back under the box: `reach` is how far it drops below the baseline. `count` is the
    /// count written on it, or empty.
    Loop {
        kid: usize,
        reach: f64,
        count: JsText,
    },
}

/// A laid out element. The track enters at the left edge and leaves at the right edge on one
/// baseline, with `up` above it and `down` below it.
struct Laid {
    width: f64,
    up: f64,
    down: f64,
    shape: Shape,
}

/// Pele's `count`: the count a loop is written with, or nothing for one or more.
fn count(min: f64, max: f64) -> String {
    if max == f64::INFINITY && min <= 1.0 {
        return String::new();
    }
    let text = |value: f64| if value.is_finite() { js::to_string(value) } else { "∞".to_owned() };
    if min == max {
        return text(min) + "×";
    }
    if max == f64::INFINITY {
        return format!("≥{}×", text(min));
    }
    if min == 0.0 { format!("≤{}×", text(max)) } else { format!("{}–{}×", text(min), text(max)) }
}

/// The boxes of one rule, made bottom-up.
struct Boxes<'a> {
    m: &'a Metrics<'a>,
    laid: Vec<Laid>,
}

impl Boxes<'_> {
    fn add(&mut self, laid: Laid) -> usize {
        self.laid.push(laid);
        self.laid.len() - 1
    }

    fn leaf(&mut self, leaf: Leaf, text: Vec<u16>, style: Emphasis) -> usize {
        let m = self.m;
        let half = m.height / 2.0;
        let advance = width(m.measure, &text, m.size, style);
        let width = js::max(even(advance + 2.0 * m.padding), m.height);
        self.add(Laid { width, up: half, down: half, shape: Shape::Leaf { leaf, text: text.into(), advance } })
    }

    /// Where the next branch of a choice sits below the baseline, given the one above it.
    fn below(&self, offset: f64, above: usize, branch: usize) -> f64 {
        js::max(offset + self.laid[above].down + self.m.sep + self.laid[branch].up, 2.0 * self.m.arc)
    }

    fn bypass(&mut self, kid: usize) -> usize {
        let m = self.m;
        let reach = js::max(self.laid[kid].up + m.sep, 2.0 * m.arc);
        let (width, down) = (self.laid[kid].width + 4.0 * m.arc, self.laid[kid].down);
        self.add(Laid { width, up: reach, down, shape: Shape::Bypass { kid, reach } })
    }

    fn sequence(&mut self, kids: Vec<usize>, room: f64) -> usize {
        let m = self.m;
        let mut width = js::max(0.0, kids.len() as f64 - 1.0) * m.gap;
        let (mut up, mut down) = (0.0, 0.0);
        for &kid in &kids {
            width += self.laid[kid].width;
            up = js::max(up, self.laid[kid].up);
            down = js::max(down, self.laid[kid].down);
        }
        if width <= room || kids.len() < 2 {
            return self.add(Laid { width, up, down, shape: Shape::Sequence(kids) });
        }
        let mut rows: Vec<(Vec<usize>, f64)> = Vec::new();
        let mut row: Vec<usize> = Vec::new();
        let mut row_width = 0.0;
        for &kid in &kids {
            let kid_width = self.laid[kid].width;
            if !row.is_empty() && row_width + m.gap + kid_width > js::max(40.0, room - 6.0 * m.arc) {
                rows.push((std::mem::take(&mut row), row_width));
                row_width = 0.0;
            }
            row_width += (if row.is_empty() { 0.0 } else { m.gap }) + kid_width;
            row.push(kid);
        }
        rows.push((row, row_width));
        if rows.len() < 2 {
            return self.add(Laid { width, up, down, shape: Shape::Sequence(kids) });
        }
        let rows: Vec<usize> = rows
            .into_iter()
            .map(|(row, row_width)| {
                let (mut up, mut down) = (0.0, 0.0);
                for &kid in &row {
                    up = js::max(up, self.laid[kid].up);
                    down = js::max(down, self.laid[kid].down);
                }
                self.add(Laid { width: row_width, up, down, shape: Shape::Sequence(row) })
            })
            .collect();
        let (mut folded_width, mut offset) = (0.0, 0.0);
        for i in 0..rows.len() {
            folded_width = js::max(folded_width, self.laid[rows[i]].width);
            if i > 0 {
                offset += self.laid[rows[i - 1]].down + js::max(m.sep, 2.0 * m.arc) + self.laid[rows[i]].up;
            }
        }
        let up = self.laid[rows[0]].up;
        let down = offset + self.laid[rows[rows.len() - 1]].down;
        self.add(Laid { width: folded_width + 6.0 * m.arc, up, down, shape: Shape::Folded(rows) })
    }

    fn choice(&mut self, kids: Vec<usize>) -> usize {
        let m = self.m;
        let (mut inner, mut offset) = (0.0, 0.0);
        for i in 0..kids.len() {
            inner = js::max(inner, self.laid[kids[i]].width);
            if i > 0 {
                offset = self.below(offset, kids[i - 1], kids[i]);
            }
        }
        let up = kids.first().map_or(0.0, |&first| self.laid[first].up);
        let down = kids.last().map_or(0.0, |&last| offset + self.laid[last].down);
        self.add(Laid { width: inner + 4.0 * m.arc, up, down, shape: Shape::Choice(kids) })
    }

    fn repetition(&mut self, kid: usize, min: f64, max: f64) -> usize {
        if min == 1.0 && max == 1.0 {
            return kid;
        }
        let m = self.m;
        let text = count(min, max);
        let reach = js::max(self.laid[kid].down + m.sep, 2.0 * m.arc);
        let label = if text.is_empty() {
            0.0
        } else {
            even(width(m.measure, &text.encode_utf16().collect::<Vec<_>>(), COUNT_SIZE, PLAIN))
        };
        let below = if text.is_empty() { ARROW / 2.0 + 1.0 } else { COUNT_SIZE + 6.0 };
        let (kid_width, up) = (self.laid[kid].width, self.laid[kid].up);
        let looped = self.add(Laid {
            width: js::max(kid_width, label) + 2.0 * m.arc,
            up,
            down: reach + below,
            shape: Shape::Loop { kid, reach, count: text.as_str().into() },
        });
        if min == 0.0 { self.bypass(looped) } else { looped }
    }

    /// Pele's `measure`: the boxes of a rule's definition, bottom-up and without recursion. With a
    /// room, a sequence that does not fit folds, and its parts are sized for the rows it has.
    fn measure(&mut self, grammar: &Grammar, root: NodeId, room: f64) -> usize {
        struct Frame {
            node: NodeId,
            room: f64,
            kids: Vec<usize>,
        }
        let m = self.m;
        let mut stack = vec![Frame { node: root, room, kids: Vec::new() }];
        loop {
            let top = stack.last_mut().expect("the root is on the stack");
            let parts = grammar.parts(top.node);
            if top.kids.len() < parts.len() {
                let around = match grammar.node(top.node) {
                    Node::Choice(_) | Node::Optional(_) => 4.0 * m.arc,
                    Node::Repetition { min, .. } => (if *min == 0.0 { 6.0 } else { 2.0 }) * m.arc,
                    Node::Sequence(_) => 6.0 * m.arc,
                    _ => 0.0,
                };
                let child =
                    Frame { node: parts[top.kids.len()], room: js::max(40.0, top.room - around), kids: Vec::new() };
                stack.push(child);
                continue;
            }
            let Frame { node, room, kids } = stack.pop().expect("the frame above");
            let laid = match grammar.node(node) {
                Node::Terminal(value) => {
                    let text = shown(value);
                    let text = if !text.is_empty() && text.iter().all(|&c| c == 32) {
                        "␣".encode_utf16().cycle().take(text.len()).collect()
                    } else {
                        text
                    };
                    self.leaf(Leaf::Terminal, text, MONO)
                }
                Node::NonTerminal(name) => self.leaf(Leaf::NonTerminal(name.clone()), shown(name), PLAIN),
                Node::Special(text) => {
                    let mut shown_text: Vec<u16> = "? ".encode_utf16().collect();
                    shown_text.extend(shown(text));
                    shown_text.extend(" ?".encode_utf16());
                    self.leaf(Leaf::Special, shown_text, ITALIC)
                }
                Node::Sequence(_) => self.sequence(kids, room),
                Node::Choice(_) => self.choice(kids),
                Node::Optional(_) => self.bypass(kids[0]),
                Node::Repetition { min, max, .. } => self.repetition(kids[0], *min, *max),
            };
            match stack.last_mut() {
                Some(parent) => parent.kids.push(laid),
                None => return laid,
            }
        }
    }
}

/// What placing draws on its own, in the order Pele writes it.
enum Item {
    /// A leaf box, at its left edge and its baseline.
    Leaf { laid: usize, x: f64, y: f64 },
    /// The count written under a loop, centred on `x` with its loop at `y`.
    Count { count: JsText, x: f64, y: f64 },
}

/// One rule laid out: its boxes, where the definition starts, and what placing it drew.
struct RuleLayout {
    boxes: Vec<Laid>,
    name: Vec<u16>,
    name_width: f64,
    /// The top of the rule's heading.
    top: f64,
    baseline: f64,
    end: f64,
    track: PathProgram,
    arrows: PathProgram,
    items: Vec<Item>,
}

/// The arcs a track turns with, each a quarter turn of the arc's radius.
struct Turns {
    east_to_south: PathStep,
    south_to_east: PathStep,
    east_to_north: PathStep,
    north_to_east: PathStep,
    loop_down: PathStep,
    loop_up: PathStep,
}

impl Turns {
    fn new(arc: f64) -> Self {
        let turn = |sweep: bool, west: bool, north: bool| {
            let along = |back: bool| if back { Number::minus_num(arc) } else { Number::num(arc) };
            PathStep::ArcBy { radii: [Number::num(arc); 2], large: false, sweep, end: [along(west), along(north)] }
        };
        Turns {
            east_to_south: turn(true, false, false),
            south_to_east: turn(false, false, false),
            east_to_north: turn(false, false, true),
            north_to_east: turn(true, false, true),
            loop_down: turn(false, true, false),
            loop_up: turn(false, true, true),
        }
    }
}

/// What placing a rule's boxes draws, in the order Pele writes it.
struct Placing<'a> {
    m: &'a Metrics<'a>,
    turns: Turns,
    track: PathProgram,
    arrows: PathProgram,
    items: Vec<Item>,
}

impl Placing<'_> {
    fn line(&mut self, x1: f64, y: f64, x2: f64) {
        if x2 > x1 {
            self.track
                .steps
                .extend([PathStep::Move([Number::num(x1), Number::num(y)]), PathStep::Horizontal(Number::num(x2))]);
        }
    }

    fn turn(&mut self, pick: fn(&Turns) -> &PathStep) {
        let step = pick(&self.turns).clone();
        self.track.steps.push(step);
    }

    fn to(&mut self, x: f64, y: f64) {
        self.track.steps.push(PathStep::Move([Number::num(x), Number::num(y)]));
    }

    fn h(&mut self, x: f64) {
        self.track.steps.push(PathStep::Horizontal(Number::num(x)));
    }

    fn v(&mut self, y: f64) {
        self.track.steps.push(PathStep::Vertical(Number::num(y)));
    }

    /// Pele's `place`: draws a rule's boxes from `left` on `baseline`, with a stack of boxes and
    /// their places rather than by recursion.
    fn place(&mut self, boxes: &[Laid], root: usize, left: f64, baseline: f64) {
        let (gap, sep, arc) = (self.m.gap, self.m.sep, self.m.arc);
        let mut stack = vec![root];
        let mut at = vec![left, baseline];
        while let Some(id) = stack.pop() {
            let y = at.pop().expect("a place for each box");
            let x = at.pop().expect("a place for each box");
            let laid = &boxes[id];
            let right = x + laid.width;
            match &laid.shape {
                Shape::Sequence(kids) => {
                    let mut kx = right;
                    for (i, &kid) in kids.iter().enumerate().rev() {
                        kx -= boxes[kid].width;
                        stack.push(kid);
                        at.extend([kx, y]);
                        if i > 0 {
                            self.line(kx - gap, y, kx);
                        }
                        kx -= gap;
                    }
                }
                Shape::Folded(rows) => {
                    let start = x + 2.0 * arc;
                    let from = stack.len();
                    let mut offset = 0.0;
                    self.line(x, y, start);
                    for (i, &row) in rows.iter().enumerate() {
                        let kid = &boxes[row];
                        let ky = y + offset;
                        stack.push(row);
                        at.extend([ky, start]);
                        if let Some(&next) = rows.get(i + 1) {
                            let next_offset = offset + kid.down + js::max(sep, 2.0 * arc) + boxes[next].up;
                            let mid = y + offset + kid.down + js::max(sep, 2.0 * arc) / 2.0;
                            self.to(start + kid.width, ky);
                            self.h(right - 2.0 * arc);
                            self.v(mid);
                            self.h(x + arc);
                            self.v(y + next_offset);
                            self.h(start);
                            self.arrows.steps.extend([
                                PathStep::Move([
                                    Number::num(start - ARROW),
                                    Number::num(y + next_offset - ARROW / 2.0),
                                ]),
                                PathStep::LineBy([Number::raw(ARROW), Number::raw(ARROW / 2.0)]),
                                PathStep::LineBy([Number::raw(-ARROW), Number::raw(ARROW / 2.0)]),
                                PathStep::CloseBy,
                            ]);
                            offset = next_offset;
                        } else {
                            self.to(start + kid.width, ky);
                            self.h(right - arc);
                            self.v(y);
                            self.h(right);
                        }
                    }
                    stack[from..].reverse();
                    at[2 * from..].reverse();
                }
                Shape::Choice(kids) => {
                    let inner = laid.width - 4.0 * arc;
                    let rail_in = x + arc;
                    let rail_out = right - arc;
                    let from = stack.len();
                    let mut offset = 0.0;
                    for (i, &kid) in kids.iter().enumerate() {
                        if i > 0 {
                            offset = js::max(offset + boxes[kids[i - 1]].down + sep + boxes[kid].up, 2.0 * arc);
                        }
                        let kid_width = boxes[kid].width;
                        let ky = y + offset;
                        let kx = x + 2.0 * arc + (inner - kid_width) / 2.0;
                        let last = i == kids.len() - 1;
                        if i == 0 {
                            self.line(x, y, kx);
                            self.line(kx + kid_width, y, right);
                        } else {
                            if last {
                                self.to(x, y);
                                self.turn(|turns| &turns.east_to_south);
                                self.v(ky - arc);
                            } else {
                                self.to(rail_in, ky - arc);
                            }
                            self.turn(|turns| &turns.south_to_east);
                            let inset = kx > rail_in + arc;
                            if inset {
                                self.h(kx);
                            }
                            self.to(kx + kid_width, ky);
                            if inset {
                                self.h(rail_out - arc);
                            }
                            self.turn(|turns| &turns.east_to_north);
                            if last {
                                self.v(y + arc);
                                self.turn(|turns| &turns.north_to_east);
                            }
                        }
                        stack.push(kid);
                        at.extend([ky, kx]);
                    }
                    // Reversed, so that the branches come off the stack top to bottom.
                    stack[from..].reverse();
                    at[2 * from..].reverse();
                }
                Shape::Bypass { kid, reach } => {
                    let kx = x + 2.0 * arc;
                    self.line(x, y, kx);
                    self.line(kx + boxes[*kid].width, y, right);
                    self.to(x, y);
                    self.turn(|turns| &turns.east_to_north);
                    self.v(y - reach + arc);
                    self.turn(|turns| &turns.north_to_east);
                    self.h(right - 2.0 * arc);
                    self.turn(|turns| &turns.east_to_south);
                    self.v(y - arc);
                    self.turn(|turns| &turns.south_to_east);
                    stack.push(*kid);
                    at.extend([kx, y]);
                }
                Shape::Loop { kid, reach, count } => {
                    let kx = x + (laid.width - boxes[*kid].width) / 2.0;
                    let loop_y = y + reach;
                    let middle = x + laid.width / 2.0;
                    self.line(x, y, kx);
                    self.line(kx + boxes[*kid].width, y, right);
                    self.to(x + arc, y);
                    self.turn(|turns| &turns.loop_down);
                    self.v(loop_y - arc);
                    self.turn(|turns| &turns.south_to_east);
                    self.h(right - arc);
                    self.turn(|turns| &turns.east_to_north);
                    self.v(y + arc);
                    self.turn(|turns| &turns.loop_up);
                    self.arrows.steps.extend([
                        PathStep::Move([Number::num(middle - ARROW / 2.0), Number::num(loop_y)]),
                        PathStep::LineBy([Number::raw(ARROW), Number::raw(-ARROW / 2.0)]),
                        PathStep::VerticalBy(Number::raw(ARROW)),
                        PathStep::CloseBy,
                    ]);
                    if !count.is_empty() {
                        self.items.push(Item::Count { count: count.clone(), x: middle, y: loop_y });
                    }
                    stack.push(*kid);
                    at.extend([kx, y]);
                }
                Shape::Leaf { .. } => self.items.push(Item::Leaf { laid: id, x, y }),
            }
        }
    }
}

/// How each rule's drawing is painted, from the config and the font size.
struct Look {
    head_size: f64,
    head_height: f64,
    radius: f64,
    stroke_width: Option<Number>,
}

/// One attempt at drawing the diagram: Pele's `draw`.
pub(super) fn draw(model: &RailroadModel, cx: &DrawContext<'_>, tight: bool) -> Result<Scene, RenderError> {
    let config = cx.config;
    let base = cx.options.typography.normal.size;
    let font_size = option(config, "fontSize", 0.0, 200.0);
    let size = if font_size != 0.0 { font_size } else { js::round(base * 0.875) };
    let head_size = if font_size != 0.0 { font_size } else { base };
    let padding = option(config, "padding", -1.0, 200.0);
    let line_height = js::round(size * 1.5);
    let m = Metrics {
        measure: cx.measure,
        size,
        padding: if padding < 0.0 { 12.0 } else { padding },
        height: even(line_height + if padding < 0.0 { 9.0 } else { 2.0 * padding }),
        gap: js::min(option(config, "horizontalSeparation", 16.0, 500.0), if tight { 12.0 } else { f64::INFINITY }),
        sep: option(config, "verticalSeparation", 10.0, 500.0),
        arc: js::min(option(config, "arcRadius", 10.0, 200.0), if tight { 6.0 } else { f64::INFINITY }),
    };
    let section = config.get("railroad").and_then(JsValue::as_record);
    let hidden = section.and_then(|section| section.get("showMarkers")) == Some(&JsValue::Bool(false));
    let radius = if hidden { 0.0 } else { option(config, "markerRadius", 4.0, 50.0) };
    let stroke_width = option(config, "strokeWidth", 0.0, 20.0);
    let pad = cx.options.padding;
    let look = Look {
        head_size,
        head_height: js::round(head_size * 1.5),
        radius,
        stroke_width: (stroke_width > 0.0).then(|| Number::num(stroke_width)),
    };

    let room = if tight { fit::title_room(cx.options) } else { 4000.0 };
    let title = layout_label(model.title.as_deref().unwrap_or_default(), false, cx.measure, base, room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };

    let mut rules = Vec::with_capacity(model.grammar.rules.len());
    let mut width_so_far = title.width;
    let mut top = pad + title_height;
    let mut bottom = pad + title.height;
    for rule in &model.grammar.rules {
        let room = if tight {
            js::max(80.0, cx.options.max_width.unwrap_or(320.0) - 2.0 * pad - 4.0 * radius - 2.0 * LEAD - 1.0)
        } else {
            f64::INFINITY
        };
        let mut boxes = Boxes { m: &m, laid: Vec::new() };
        let root = boxes.measure(&model.grammar, rule.definition, room);
        let boxes = boxes.laid;
        let name = shown(&rule.name);
        let baseline = top + look.head_height + HEAD_GAP + boxes[root].up;
        let start = pad + 2.0 * radius;
        let left = start + LEAD;
        let end = left + boxes[root].width + LEAD;
        let mut placing = Placing {
            m: &m,
            turns: Turns::new(m.arc),
            track: PathProgram::default(),
            arrows: PathProgram::default(),
            items: Vec::new(),
        };
        placing.line(start, baseline, left);
        placing.line(left + boxes[root].width, baseline, end);
        placing.place(&boxes, root, left, baseline);
        let name_width = width(cx.measure, &name, head_size, Emphasis::BOLD);
        width_so_far = js::max(js::max(width_so_far, end + 2.0 * radius - pad), name_width);
        let rule_top = top;
        bottom = baseline + boxes[root].down;
        top = bottom + m.sep + RULE_GAP;
        let Placing { track, arrows, items, .. } = placing;
        rules.push(RuleLayout { boxes, name, name_width, top: rule_top, baseline, end, track, arrows, items });
    }

    let total_width = (width_so_far + 2.0 * pad).ceil();
    let total_height = (bottom + pad).ceil();
    let mut out = SceneBuilder::new(cx, Size { width: total_width, height: total_height }, model.accessible.clone());
    out.title(&TitleLayout { at: Point { x: total_width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let layer = LayerSpec::class("pele-rules")
        .translate(Point { x: 0.5, y: 0.5 })
        .font_size(Number::num(size))
        .anchor(AnchorKind::Middle);
    out.layer(layer, |out| {
        let mut drawn: Vec<&JsText> = Vec::new();
        for (rule, layout) in model.grammar.rules.iter().zip(rules) {
            let duplicate = drawn.iter().filter(|&&name| *name == rule.name).count();
            drawn.push(&rule.name);
            let part = out.part(Entity::group(&rule.name).ranked(duplicate as u32));
            draw_rule(out, part, layout, &m, &look, pad);
        }
    });
    out.finish()
}

/// The stroke width the config sets, written after a shape's stroke.
fn stroked<'a>(element: ShapeElement<'a>, look: &Look) -> ShapeElement<'a> {
    match look.stroke_width {
        Some(width) => element.stroke_width(width),
        None => element,
    }
}

/// One rule as Pele writes it: its heading, its track, the arrowheads on its rails, the markers
/// at its ends, and its boxes.
fn draw_rule(out: &mut SceneBuilder, part: PartId, layout: RuleLayout, m: &Metrics<'_>, look: &Look, pad: f64) {
    let RuleLayout { boxes, name, name_width, top, baseline, end, track, arrows, items } = layout;
    out.layer(LayerSpec::class("pele-rule").part(part, Facet::Shape).data_id(), |out| {
        let heading = [Number::num(pad), Number::num(top + look.head_height / 2.0 + look.head_size * 0.35)];
        out.text(part, Facet::Title)
            .class("pele-rule-name")
            .at(heading)
            .font_size(Number::num(look.head_size))
            .weight(TextWeight::Heading)
            .anchor(AnchorKind::Start)
            .finish(&name, name_width, look.head_size);
        let element = out.element(part, Facet::Shape).class("pele-edge pele-track").path(track);
        stroked(element.fill(Paint::None).stroke(Paint::Role(Role::Line)), look).finish();
        if !arrows.steps.is_empty() {
            out.element(part, Facet::Shape).class("pele-marker").path(arrows).fill(Paint::Role(Role::Line)).finish();
        }
        if look.radius > 0.0 {
            let radius = Number::num(look.radius);
            for (facet, class, x) in [
                (Facet::StartMarker, "pele-marker pele-start", pad + look.radius),
                (Facet::EndMarker, "pele-marker pele-end", end + look.radius),
            ] {
                out.element(part, facet)
                    .class(class)
                    .circle_at([Some(Number::num(x)), Some(Number::num(baseline))], radius)
                    .fill(Paint::Role(Role::Line))
                    .finish();
            }
        }
        for item in items {
            match item {
                Item::Leaf { laid, x, y } => draw_leaf(out, part, &boxes[laid], x, y, m, look),
                Item::Count { count, x, y } => {
                    let count_width = width(m.measure, &count, COUNT_SIZE, PLAIN);
                    out.text(part, Facet::Label)
                        .class("pele-edge-label pele-count")
                        .at([Number::num(x), Number::num(y + COUNT_SIZE + 3.0)])
                        .font_size(Number::raw(COUNT_SIZE))
                        .fill(Some(Paint::Role(Role::Muted)))
                        .finish(&count, count_width, COUNT_SIZE);
                }
            }
        }
    });
}

/// A leaf box: its shape, then its text.
fn draw_leaf(out: &mut SceneBuilder, rule: PartId, laid: &Laid, x: f64, y: f64, m: &Metrics<'_>, look: &Look) {
    let Shape::Leaf { leaf, text, advance } = &laid.shape else {
        unreachable!("placing makes leaf items of leaf boxes only")
    };
    let advance = *advance;
    let box_top = m.height / 2.0;
    let space = text.first() == Some(&32) || text.last() == Some(&32) || text.has("  ");
    let origin = [Number::num(x), Number::num(y - box_top)];
    let size = [Number::num(laid.width), Number::num(m.height)];
    let at = [Number::num(x + laid.width / 2.0), Number::num(y + m.size * 0.35)];
    match leaf {
        Leaf::Terminal => out.layer(LayerSpec::class("pele-node pele-terminal"), |out| {
            let rect = out.element(rule, Facet::Shape).rect(origin, size, Some(Number::num(box_top)));
            stroked(rect.fill(Paint::Role(Role::Surface)).stroke(Paint::Role(Role::Border)), look).finish();
            let label = out.text(rule, Facet::Label).at(at);
            let label = if space { label.preserve_space() } else { label };
            label.mono().finish(text, advance, m.size);
        }),
        Leaf::NonTerminal(name) => {
            let part = out.part(Entity::node(name));
            out.layer(LayerSpec::class("pele-node pele-nonterminal").part(part, Facet::Shape).data_id(), |out| {
                let rect = out.element(part, Facet::Shape).rect(origin, size, None);
                let rect = stroked(rect.fill(Paint::Role(Role::Background)).stroke(Paint::Role(Role::Border)), look);
                rect.style(Some(Style::rounded(None))).finish();
                let label = out.text(part, Facet::Label).at(at);
                let label = if space { label.preserve_space() } else { label };
                label.finish(text, advance, m.size);
            });
        }
        Leaf::Special => out.layer(LayerSpec::class("pele-node pele-special"), |out| {
            let rect = out.element(rule, Facet::Shape).rect(origin, size, None);
            let rect = rect.fill(Paint::Role(Role::Background)).stroke(Paint::Role(Role::Border));
            let rect = stroked(rect.dash(&[Number::raw(4.0), Number::raw(3.0)]), look);
            rect.style(Some(Style::rounded(None))).finish();
            let inner = &text[2..text.len() - 2];
            let run = |run: &[u16]| width(m.measure, run, m.size, ITALIC);
            let open: Vec<u16> = "? ".encode_utf16().collect();
            let close: Vec<u16> = " ?".encode_utf16().collect();
            let muted = Some(Paint::Role(Role::Muted));
            let runs = [
                (open.as_slice(), run(&open), muted),
                (inner, run(inner), None),
                (close.as_slice(), run(&close), muted),
            ];
            let label = out.text(rule, Facet::Label).at(at);
            let label = if space { label.preserve_space() } else { label };
            label.italic().finish_painted(&runs, m.size);
        }),
    }
}
