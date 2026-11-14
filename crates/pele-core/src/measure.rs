//! Text measurement and label layout, ported from Pele's `src/text`.
//!
//! A label is laid out once into lines of styled runs with measured widths. Everything that
//! places or writes text reads that layout, and nothing lays text out a second time.

use crate::input::{JsText, Units, decode_entities, is_space, is_word};
use crate::js;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontFamily {
    Sans,
    Mono,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontWeight {
    Normal,
    Bold,
}

/// The font one run of text is measured in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    pub family: FontFamily,
    /// In pixels.
    pub size: f64,
    pub weight: FontWeight,
    pub italic: bool,
}

/// The text settings a layout is made with. Colours are not among them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    /// The font of plain label text. Its size is Pele's `fontSize` option.
    pub normal: Font,
}

impl Default for Typography {
    fn default() -> Self {
        Typography { normal: Font { family: FontFamily::Sans, size: 16.0, weight: FontWeight::Normal, italic: false } }
    }
}

/// Text to measure, as the UTF-16 code units Pele counts. A surrogate may stand alone.
#[derive(Clone, Copy, Debug)]
pub struct TextRef<'a> {
    units: &'a [u16],
}

impl<'a> TextRef<'a> {
    pub(crate) fn new(units: &'a [u16]) -> Self {
        TextRef { units }
    }

    pub fn utf16_units(&self) -> &'a [u16] {
        self.units
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasure {
    pub width: f64,
}

/// How wide text is. Layout asks nothing else of a font.
pub trait TextMeasurer {
    fn measure(&self, text: TextRef<'_>, font: &Font) -> TextMeasure;
}

// Advance widths of printable ASCII in thousandths of an em, two base-36 digits each.
// Copied from Pele's `src/text/metrics.ts`, which generates them from Arial.
const REGULAR: &str = "7q7q9vfgfgopij5b9999atg87q997q7qfgfgfgfgfgfgfgfgfgfg7q7qg8g8g8fgs7ijijk2k2ijgzlmk27qdwijfgn5k2lmijlmk2ijgzk2ijq8ijijgz7q7q7qd1fg99fgfgdwfgfg7qfgfg6666dw66n5fgfgfgfg99dw7qfgdwk2dwdwdw9a789ag8";
const BOLD: &str = "7q99d6fgfgopk26m9999atg87q997q7qfgfgfgfgfgfgfgfgfgfg9999g8g8g8gzr3k2k2k2k2ijgzlmk27qfgk2gzn5k2lmijlmk2ijgzk2ijq8ijijgz997q99g8fg99fggzfggzfg99gzgz7q7qfg7qopgzgzgzgzatfg99gzfglmfgfgdwat7satg8";

fn unpack(packed: &str) -> [u16; 95] {
    std::array::from_fn(|i| u16::from_str_radix(&packed[i * 2..i * 2 + 2], 36).expect("two base-36 digits"))
}

static REGULAR_ADVANCES: LazyLock<[u16; 95]> = LazyLock::new(|| unpack(REGULAR));
static BOLD_ADVANCES: LazyLock<[u16; 95]> = LazyLock::new(|| unpack(BOLD));

fn is_wide(c: u16) -> bool {
    matches!(
        c,
        0x1100..=0x115f
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe30..=0xfe4f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0xd800..=0xdbff
    )
}

fn is_zero_width(c: u16) -> bool {
    matches!(c, 0xad | 0x300..=0x36f | 0x200b..=0x200f | 0xdc00..=0xdfff | 0xfe00..=0xfe0f | 0xfeff)
}

/// Pele's `metricsMeasurer`: widths from a table of Arial advances, one UTF-16 code unit at a time.
pub(crate) struct ArialMetrics;

impl TextMeasurer for ArialMetrics {
    fn measure(&self, text: TextRef<'_>, font: &Font) -> TextMeasure {
        let mono = font.family == FontFamily::Mono;
        let table = if font.weight == FontWeight::Bold { &*BOLD_ADVANCES } else { &*REGULAR_ADVANCES };
        let mut units = 0u64;
        for &c in text.utf16_units() {
            units += match c {
                0..32 => 0,
                32..127 if mono => 600,
                32..127 => u64::from(table[usize::from(c - 32)]),
                _ if is_zero_width(c) => 0,
                _ if is_wide(c) && mono => 1200,
                _ if is_wide(c) => 1000,
                _ if mono => 600,
                _ => 580,
            };
        }
        TextMeasure { width: (units as f64 * font.size) / 1000.0 }
    }
}

/// Where a run of text sits against its line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Script {
    #[default]
    Normal,
    Sub,
    Super,
}

/// The styling of one run. Pele's `Style` bit set, with the two shifts made exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct Emphasis {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) mono: bool,
    pub(crate) underline: bool,
    pub(crate) script: Script,
}

impl Emphasis {
    pub(crate) const BOLD: Emphasis =
        Emphasis { bold: true, italic: false, mono: false, underline: false, script: Script::Normal };

    pub(crate) fn is_plain(self) -> bool {
        self == Emphasis::default()
    }
}

// Approximate divisor for CSS font-size="smaller".
pub(crate) const SMALLER: f64 = 1.2;

/// The width of `text` at `size` pixels with `style`, as Pele's `measurer.width(text, size, style)`.
pub(crate) fn width(measurer: &dyn TextMeasurer, text: &[u16], size: f64, style: Emphasis) -> f64 {
    let font = Font {
        family: if style.mono { FontFamily::Mono } else { FontFamily::Sans },
        size: if style.script == Script::Normal { size } else { size / SMALLER },
        weight: if style.bold { FontWeight::Bold } else { FontWeight::Normal },
        italic: style.italic,
    };
    measurer.measure(TextRef::new(text), &font).width
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LabelRun {
    pub(crate) text: JsText,
    pub(crate) style: Emphasis,
    pub(crate) width: f64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct LabelLine {
    pub(crate) runs: Vec<LabelRun>,
    pub(crate) width: f64,
}

/// A measured label. An empty one has no lines and no size.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Label {
    pub(crate) lines: Vec<LabelLine>,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) size: f64,
    pub(crate) line_height: f64,
}

impl Label {
    /// One line holding one run.
    pub(crate) fn single(text: &[u16], style: Emphasis, width: f64, size: f64, line_height: f64) -> Self {
        let line = LabelLine { runs: vec![LabelRun { text: text.into(), style, width }], width };
        Label { lines: vec![line], width, height: line_height, size, line_height }
    }
}

const LINE_HEIGHT: f64 = 1.5;
// Lines of one wrapped label sit closer, so the label reads as one block.
const WRAPPED_LINE_HEIGHT: f64 = 1.3;

const HTML_TAGS: &str = "a abbr address article aside audio b bdi bdo big blockquote body br button caption center cite code \
col colgroup data dd del details dfn dialog div dl dt em embed fieldset figcaption figure font \
footer form h1 h2 h3 h4 h5 h6 head header hr html i iframe img input ins kbd label legend li link \
main map mark meta meter nav noscript object ol optgroup option output p picture pre progress q rp \
rt ruby s samp script section select small source span strike strong style sub summary sup table \
tbody td template textarea tfoot th thead time title tr track tt u ul var video wbr";

// Break between any two Chinese or Japanese characters. Hangul is left out: Korean puts spaces between words.
fn is_cjk(c: u16) -> bool {
    matches!(c, 0x2e80..=0x9fff | 0xf900..=0xfaff | 0xff00..=0xff60)
}

const CLOSE: &str = "、。，．：；！？）］｝」』】〕〉》〙〗ー…‥・ゝゞぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮ,.:;!?)]}";
const OPEN: &str = "（［｛「『【〔〈《〘〖([{";

fn is_close(c: u16) -> bool {
    CLOSE.encode_utf16().any(|close| close == c)
}

fn is_open(c: u16) -> bool {
    OPEN.encode_utf16().any(|open| open == c)
}

// Break hints: a zero-width space, and a soft hyphen, which shows as '-' where a line breaks.
const SOFT_HYPHEN: u16 = 0xad;

fn is_hint(c: u16) -> bool {
    c == 0x200b || c == SOFT_HYPHEN
}

/// The end of what follows the leading whitespace of one `RE_PIECE` match, tried at `at`:
/// `[OPEN]*[CJK][CLOSE]*|[^\sCJK HINT]+[HINT]*[CLOSE]*|[HINT]+`.
fn piece_core(text: &[u16], at: usize) -> Option<usize> {
    let opened = text.run(at, is_open);
    if let Some(cjk) = (at..=opened).rev().find(|&q| text.at(q).is_some_and(is_cjk)) {
        return Some(text.run(cjk + 1, is_close));
    }
    let word = text.run(at, |c| !is_space(c) && !is_cjk(c) && !is_hint(c));
    if word > at {
        return Some(text.run(text.run(word, is_hint), is_close));
    }
    let hints = text.run(at, is_hint);
    (hints > at).then_some(hints)
}

/// `text.match(RE_PIECE)`: the units a line may break between, each with the whitespace around it.
fn pieces(text: &[u16]) -> Vec<&[u16]> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let spaces = text.run(at, is_space);
        // `\s*` gives whitespace back one unit at a time, which matters only for U+3000: it is
        // whitespace and a CJK character.
        let core = (at..=spaces).rev().find_map(|lead| piece_core(text, lead));
        let end = core.map_or(spaces, |end| text.run(end, is_space));
        out.push(&text[at..end]);
        at = end;
    }
    out
}

/// `RE_MARKUP`: `/[<\\\n*_]|fa[bklrs]?:fa-/`
fn has_markup(raw: &[u16]) -> bool {
    raw.iter().any(|&c| u8::try_from(c).is_ok_and(|c| b"<\\\n*_".contains(&c))) || next_icon(raw, 0, false).is_some()
}

/// The next match of `/(fa[bklrs]?):fa-([\w-]+)/` from `from`, or of its prefix up to `:fa-`
/// when no name is asked for.
fn next_icon(text: &[u16], from: usize, named: bool) -> Option<std::ops::Range<usize>> {
    let word = |c: u16| is_word(c) || c == u16::from(b'-');
    (from..text.len()).find_map(|at| {
        if !text.starts_at(at, "fa") {
            return None;
        }
        let family = u8::try_from(text.at(at + 2)?).is_ok_and(|c| b"bklrs".contains(&c));
        let colon = at + 2 + usize::from(family);
        if !text.starts_at(colon, ":fa-") {
            return None;
        }
        let name = text.run(colon + 4, word);
        if named && name == colon + 4 { None } else { Some(at..name) }
    })
}

#[derive(Clone, Copy)]
enum Mark {
    Bold,
    Italic,
    Mono,
    Underline,
    Shift(Script),
}

/// Pele's `TAG_STYLES`.
fn tag_mark(tag: &str) -> Option<Mark> {
    Some(match tag {
        "b" | "strong" => Mark::Bold,
        "i" | "em" | "cite" | "dfn" | "var" => Mark::Italic,
        "code" | "kbd" | "samp" | "tt" => Mark::Mono,
        "sub" => Mark::Shift(Script::Sub),
        "sup" => Mark::Shift(Script::Super),
        "u" | "ins" => Mark::Underline,
        _ => return None,
    })
}

struct Tag {
    end: usize,
    name: String,
    closing: bool,
}

/// `RE_TAG` at `at`: `/<\/?([a-zA-Z][\w-]*)(?:\s[^<>]*)?\/?>/y`
fn match_tag(source: &[u16], at: usize) -> Option<Tag> {
    let closing = source.at(at + 1) == Some(u16::from(b'/'));
    let name_start = at + 1 + usize::from(closing);
    if !u8::try_from(source.at(name_start)?).is_ok_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let name_end = source.run(name_start, |c| is_word(c) || c == u16::from(b'-'));
    let close = if source.at(name_end).is_some_and(is_space) {
        source.run(name_end + 1, |c| c != u16::from(b'<') && c != u16::from(b'>'))
    } else {
        name_end + usize::from(source.at(name_end) == Some(u16::from(b'/')))
    };
    if source.at(close) != Some(u16::from(b'>')) {
        return None;
    }
    let name = source[name_start..name_end].iter().map(|&c| char::from(c as u8).to_ascii_lowercase()).collect();
    Some(Tag { end: close + 1, name, closing })
}

/// The emphasis that HTML tags have opened so far. It carries across the lines of one label.
struct Open {
    style: Emphasis,
    // Sub/sup stack; only the innermost tag shifts text.
    shifts: Vec<Script>,
}

fn push_plain(line: &mut Vec<LabelRun>, text: &[u16], style: Emphasis) {
    if text.is_empty() {
        return;
    }
    let decoded = decode_entities(text);
    match line.last_mut() {
        Some(last) if last.style == style => last.text.push_units(&decoded),
        _ => line.push(LabelRun { text: decoded.into(), style, width: 0.0 }),
    }
}

/// Pele's `pushIcons` for a host with no icons: an icon name leaves no gap, nor the space that
/// set it off from the text.
fn push_icons(line: &mut Vec<LabelRun>, text: &[u16], style: Emphasis) {
    if !text.has(":fa-") {
        push_plain(line, text, style);
        return;
    }
    let mut last = 0;
    while let Some(icon) = next_icon(text, last, true) {
        push_plain(line, &text[last..icon.start], style);
        last = icon.end;
        if text.at(last) == Some(32) {
            last += 1;
        } else if last == text.len()
            && let Some(before) = line.last_mut()
        {
            let trimmed = before.text.trim_end_js().len();
            before.text.truncate(trimmed);
        }
    }
    push_plain(line, &text[last..], style);
}

/// Pele's `inline` for a label that is not Markdown: splits inline HTML into styled runs.
fn inline(line: &mut Vec<LabelRun>, source: &[u16], base: Emphasis, open: &mut Open) {
    let flush = |line: &mut Vec<LabelRun>, text: &mut Vec<u16>, open: &Open| {
        let script = open.shifts.last().copied().unwrap_or(open.style.script);
        push_icons(line, text, Emphasis { script, ..open.style });
        text.clear();
    };
    let mut text = Vec::new();
    let mut i = 0;
    while i < source.len() {
        if source[i] == u16::from(b'<')
            && let Some(tag) = match_tag(source, i)
        {
            let mark = tag_mark(&tag.name);
            if let Some(mark) = mark {
                flush(line, &mut text, open);
                let set = |on: &mut bool, base: bool| *on = if tag.closing { base } else { true };
                match mark {
                    Mark::Bold => set(&mut open.style.bold, base.bold),
                    Mark::Italic => set(&mut open.style.italic, base.italic),
                    Mark::Mono => set(&mut open.style.mono, base.mono),
                    Mark::Underline => set(&mut open.style.underline, base.underline),
                    Mark::Shift(script) if !tag.closing => open.shifts.push(script),
                    Mark::Shift(script) => {
                        if let Some(at) = open.shifts.iter().rposition(|&shift| shift == script) {
                            open.shifts.remove(at);
                        }
                    }
                }
            }
            // Other HTML tags are dropped and their text kept. Anything else shaped like a tag,
            // such as the type in List<T>, is text.
            if mark.is_some() || HTML_TAGS.split(' ').any(|known| known == tag.name) {
                i = tag.end;
                continue;
            }
        }
        text.push(source[i]);
        i += 1;
    }
    flush(line, &mut text, open);
}

/// Ends the line being filled, as `commit` does inside Pele's `wrap`.
fn commit(
    current: &mut Vec<LabelRun>,
    total: &mut f64,
    hyphen: bool,
    measurer: &dyn TextMeasurer,
    size: f64,
    out: &mut Vec<LabelLine>,
) {
    if let Some(last) = current.last_mut() {
        let mut trimmed: Vec<u16> = last.text.trim_end_js().to_vec();
        if hyphen && last.text.last() == Some(&SOFT_HYPHEN) {
            trimmed.pop();
            trimmed.push(u16::from(b'-'));
        }
        if trimmed != *last.text {
            last.text = trimmed.into();
            let w = width(measurer, &last.text, size, last.style);
            *total += w - last.width;
            last.width = w;
        }
    }
    out.push(LabelLine { runs: std::mem::take(current), width: *total });
    *total = 0.0;
}

/// Pele's `wrap`: breaks one line of runs into lines no wider than `max`, where it can.
fn wrap(line: &[LabelRun], measurer: &dyn TextMeasurer, size: f64, max: f64, out: &mut Vec<LabelLine>) {
    let mut current: Vec<LabelRun> = Vec::new();
    let mut total = 0.0;
    for (index, span) in line.iter().enumerate() {
        let pieces = pieces(&span.text);
        // A piece ending in a soft hyphen fits if its '-' does, or if the rest of the word does,
        // which hides the hyphen. The line only grows along a word, so no later hyphen in it fits
        // either, and the remaining width of each word makes this one check.
        let soft = span.text.contains(&SOFT_HYPHEN);
        let n = if soft { pieces.len() } else { 0 };
        let mut own = vec![0.0; n];
        let mut rest = vec![0.0; n + 1];
        for q in (0..n).rev() {
            // A soft hyphen before a space is no break of its own: the space is.
            let shy = pieces[q].last() == Some(&SOFT_HYPHEN);
            let end = pieces[q].trim_end_js();
            own[q] = width(measurer, if shy { &end[..end.len() - 1] } else { end }, size, span.style);
            // Word width beyond this span is unknown.
            rest[q] = if !shy {
                own[q]
            } else if q == n - 1 {
                if index == line.len() - 1 { own[q] } else { f64::INFINITY }
            } else {
                own[q] + rest[q + 1]
            };
        }
        for (p, &piece) in pieces.iter().enumerate() {
            let mut part = piece;
            let part_width = if soft { own[p] } else { width(measurer, part.trim_end_js(), size, span.style) };
            let fits = if !soft || part.last() != Some(&SOFT_HYPHEN) {
                total + part_width <= max
            } else {
                let mut hyphenated = part[..part.len() - 1].to_vec();
                hyphenated.push(u16::from(b'-'));
                total + width(measurer, &hyphenated, size, span.style) <= max || total + rest[p] <= max
            };
            if total > 0.0 && !fits {
                // Breaking at whitespace leaves a preceding soft hyphen hidden.
                let unbroken = part.trim_start_js().len() == part.len();
                commit(&mut current, &mut total, unbroken, measurer, size, out);
                part = part.trim_start_js();
            }
            match current.last_mut() {
                Some(last) if last.style == span.style => {
                    last.text.push_units(part);
                    // Remeasuring for each zero-width piece would take quadratic time.
                    if part_width > 0.0 || part.trim_end_js().len() != part.len() {
                        let before = last.width;
                        last.width = width(measurer, &last.text, size, last.style);
                        total += last.width - before;
                    }
                }
                _ => {
                    let w = width(measurer, part, size, span.style);
                    current.push(LabelRun { text: part.into(), style: span.style, width: w });
                    total += w;
                }
            }
        }
    }
    if !current.is_empty() || out.is_empty() {
        commit(&mut current, &mut total, false, measurer, size, out);
    }
}

/// `source.split(RE_BREAK)`, where `RE_BREAK` is `/<br\s*\/?>|\\n|\n/gi`.
fn split_breaks(source: &[u16]) -> Vec<&[u16]> {
    let letter =
        |at: usize, lower: u8| source.at(at).is_some_and(|c| c == u16::from(lower) || c == u16::from(lower - 32));
    let break_end = |at: usize| -> Option<usize> {
        if source[at] == u16::from(b'<') && letter(at + 1, b'b') && letter(at + 2, b'r') {
            let spaces = source.run(at + 3, is_space);
            let slash = spaces + usize::from(source.at(spaces) == Some(u16::from(b'/')));
            if source.at(slash) == Some(u16::from(b'>')) {
                return Some(slash + 1);
            }
        }
        if source[at] == u16::from(b'\\') && letter(at + 1, b'n') {
            return Some(at + 2);
        }
        (source[at] == 10).then_some(at + 1)
    };
    let mut parts = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < source.len() {
        match break_end(at) {
            Some(end) => {
                parts.push(&source[start..at]);
                start = end;
                at = end;
            }
            None => at += 1,
        }
    }
    parts.push(&source[start..]);
    parts
}

fn finish(lines: Vec<LabelLine>, size: f64, line_height: f64) -> Label {
    let mut widest = 0.0;
    for line in &lines {
        if line.width > widest {
            widest = line.width;
        }
    }
    let snug = if lines.len() > 1 { js::round(size * WRAPPED_LINE_HEIGHT) } else { line_height };
    Label { height: lines.len() as f64 * snug, lines, width: widest, size, line_height: snug }
}

/// Pele's `layoutLabel` for a label that is not Markdown, on a host with no icons and no math.
/// `raw` is empty where Pele's is `undefined`.
pub(crate) fn layout_label(
    raw: &[u16],
    measurer: &dyn TextMeasurer,
    size: f64,
    max_width: f64,
    base: Emphasis,
) -> Label {
    if raw.is_empty() {
        return Label::default();
    }
    let line_height = js::round(size * LINE_HEIGHT);

    if !has_markup(raw) && !raw.contains(&u16::from(b'&')) && !raw.contains(&0xfb02) {
        let text = raw.trim_js();
        let w = width(measurer, text, size, base);
        if w <= max_width || !text.iter().any(|&c| c == 32 || is_cjk(c) || is_hint(c)) {
            return Label::single(text, base, w, size, line_height);
        }
        let mut lines = Vec::new();
        wrap(&[LabelRun { text: text.into(), style: base, width: w }], measurer, size, max_width, &mut lines);
        return finish(lines, size, line_height);
    }

    let mut lines = Vec::new();
    // HTML tags can span explicit breaks.
    let mut open = Open { style: base, shifts: Vec::new() };
    for part in split_breaks(raw.trim_js()) {
        let mut line = Vec::new();
        inline(&mut line, part.trim_js(), base, &mut open);
        let mut w = 0.0;
        for run in &mut line {
            run.width = width(measurer, &run.text, size, run.style);
            w += run.width;
        }
        if w > max_width {
            wrap(&line, measurer, size, max_width, &mut lines);
        } else {
            lines.push(LabelLine { runs: line, width: w });
        }
    }
    finish(lines, size, line_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{JsValue, parse_json};

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn json(text: &[u16]) -> String {
        let escaped: String = text.iter().map(|unit| format!("\\u{unit:04x}")).collect();
        format!("\"{escaped}\"")
    }

    fn bits(value: f64) -> String {
        format!("{:016x}", value.to_bits())
    }

    /// Pele's `Style` bit set for an emphasis.
    fn style_bits(style: Emphasis) -> u8 {
        let script = match style.script {
            Script::Normal => 0,
            Script::Sub => 8,
            Script::Super => 16,
        };
        u8::from(style.bold)
            | u8::from(style.italic) << 1
            | u8::from(style.mono) << 2
            | script
            | u8::from(style.underline) << 5
    }

    /// Our label in the JSON shape the oracle's `label` op answers with.
    fn shape(label: &Label) -> String {
        let run = |run: &LabelRun| {
            format!(
                "{{\"text\":{},\"style\":{},\"width\":\"{}\"}}",
                json(&run.text),
                style_bits(run.style),
                bits(run.width)
            )
        };
        let lines: Vec<String> = label
            .lines
            .iter()
            .map(|line| format!("[{}]", line.runs.iter().map(run).collect::<Vec<_>>().join(",")))
            .collect();
        let widths: Vec<String> = label.lines.iter().map(|line| format!("\"{}\"", bits(line.width))).collect();
        format!(
            "{{\"lines\":[{}],\"widths\":[{}],\"width\":\"{}\",\"height\":\"{}\",\"lineHeight\":\"{}\"}}",
            lines.join(","),
            widths.join(","),
            bits(label.width),
            bits(label.height),
            bits(label.line_height)
        )
    }

    fn labels() -> Vec<Vec<u16>> {
        let mut out: Vec<Vec<u16>> = [
            "",
            "Dogs",
            "  Pets adopted by volunteers  ",
            "Calcium [42.96]",
            "a <b>bold</b> and <i>italic <code>mono</code></i> <u>under</u> x<sub>2</sub> y<sup>3</sup>",
            "<b>all bold</b>",
            "<strong>open only",
            "</b> closes nothing <EM>upper</EM> <i class='x'>attr</i> <i/>self<b >space</b >",
            "List<T> and <unknown> but <span>span</span> <div x=1>div</div> a < b > c <1> </> < b>",
            "<sub>a<sup>b</sup>c</sub>d</sub>e<sup><sub>f</sup>g",
            "line one<br>line two<br/>three<BR />four\\nfive\\Nsix\nseven<br\n>eight<br x>nine",
            "first<br><br>third",
            "<b>bold<br>still bold</b> plain",
            "fa:fa-user Name and fab:fa-github, far:fa-x fa:fa- fax:fa-no trailing fa:fa-end",
            "<b>x </b>fa:fa-user",
            "a_b *c* \\ back",
            "B &amp; C &lt; D &#35; &#x41; &nope; &#0; &quot;q&quot; &nbsp;| &shy;",
            "issue \u{fb02}\u{b0}\u{b0}35\u{b6}\u{df} and \u{fb02}\u{b0}amp\u{b6}\u{df} and \u{fb02}\u{b0}nope\u{b6}\u{df} \u{fb02}\u{b0}\u{b0}38\u{b6}\u{df}lt;",
            "The quick brown fox jumps over the lazy dog, again and again, until it wraps",
            "word",
            "two words",
            "Supercalifragilisticexpialidocious pneumonoultramicroscopicsilicovolcanoconiosis",
            "soft\u{ad}hyphen\u{ad}ated\u{ad}word that\u{ad} is long\u{ad}",
            "zero\u{200b}width\u{200b}space\u{200b}breaks\u{200b}here",
            "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}\u{3001}\u{300c}\u{62ec}\u{5f27}\u{300d}\u{3068}\u{53e5}\u{8aad}\u{70b9}\u{3002}mixed English (\u{4e2d}\u{6587}) end",
            "a\u{3000}b \u{3000} c\u{3000}",
            "\u{ff08}\u{ff3b}open run a (b) [c] {d}, e. f; g! h?",
            "emoji \u{1f600} and \u{1f469}\u{200d}\u{1f4bb} wide \u{ac00}\u{d7a3} combining e\u{301} \u{feff}bom",
            "tabs\tand  double  spaces \u{a0}nbsp\u{a0} trailing   ",
            "<i>italic words that wrap</i> then <b>bold words that wrap too</b> and plain ones at the end",
            "<code>mono text that is long enough to wrap around</code>",
            "<b>a</b><b>b</b> c<i></i>d",
            "*",
            "\\n",
            "<br>",
            " <br> ",
        ]
        .iter()
        .map(|text| units(text))
        .collect();
        out.push(vec![0x61, 0xd800, 0x62, 0xdc00, 0x20, 0xd83d]);
        out.push(units(&"long ".repeat(200)));
        out
    }

    #[test]
    fn layout_label_matches_pele() {
        let settings = [(14.0, 4000.0, 0), (16.0, 4000.0, 1), (16.0, 80.0, 1), (14.0, 40.0, 0), (13.0, 1.0, 0)];
        let cases: Vec<(Vec<u16>, f64, f64, u8)> = labels()
            .into_iter()
            .flat_map(|raw| settings.map(|(size, max_width, base)| (raw.clone(), size, max_width, base)))
            .collect();
        let requests: Vec<String> = cases
            .iter()
            .map(|(raw, size, max_width, base)| {
                format!("label\t{}\t{}\t{}\t{base}", json(raw), bits(*size), bits(*max_width))
            })
            .collect();
        let theirs = pele_oracle::ask(&requests);
        let parse = |text: &str| parse_json(&units(text)).ok().expect("a label is JSON");
        for ((raw, size, max_width, base), theirs) in cases.iter().zip(&theirs) {
            let base = if *base == 1 { Emphasis::BOLD } else { Emphasis::default() };
            let ours = shape(&layout_label(raw, &ArialMetrics, *size, *max_width, base));
            assert!(matches!(parse(theirs), JsValue::Object(_)), "the oracle answers a label");
            assert!(
                parse(&ours) == parse(theirs),
                "label {:?} at {size}px in {max_width}px:\n ours {ours}\n pele {theirs}",
                String::from_utf16_lossy(raw)
            );
        }
    }
}
