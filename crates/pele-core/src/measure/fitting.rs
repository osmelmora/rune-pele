//! Labels laid out to fit a box of a set width, as the types that draw text in such boxes lay
//! theirs out. Ported from Pele's `src/diagrams/common/fit.ts` (git, journey, timeline),
//! `fit-text.ts` (packet, treemap) and `literal-label.ts` (c4, eventmodeling, ishikawa, packet,
//! sequence, treeview).
//!
//! A label here never holds an icon or math, as everywhere in this port, so Pele's branches for
//! them have nothing to do.

use super::{Emphasis, LINE_HEIGHT, Label, LabelLine, LabelRun, TextMeasurer, layout_label, width as measured};
use crate::input::{JsText, Units, is_space, is_word, split_unit};
use crate::js;

/// The code points of a text, as `Array.from` gives them: a surrogate pair as one, any other code
/// unit alone.
fn code_points(text: &[u16]) -> Vec<&[u16]> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let end = point_end(text, at);
        out.push(&text[at..end]);
        at = end;
    }
    out
}

/// Where the code point at `at` ends.
fn point_end(text: &[u16], at: usize) -> usize {
    let pair = (0xd800..0xdc00).contains(&text[at]) && text.get(at + 1).is_some_and(|c| (0xdc00..0xe000).contains(c));
    at + 1 + usize::from(pair)
}

/// `/\s/.test(char)` for one code point.
fn is_space_point(point: &[u16]) -> bool {
    matches!(point, [c] if is_space(*c))
}

/// `<br\s*\/?>` at `at`, in any case: where it ends.
fn line_break(text: &[u16], at: usize) -> Option<usize> {
    let lower = |i: usize| text.at(i).map(|c| c | 32);
    if text.at(at) != Some(u16::from(b'<'))
        || lower(at + 1) != Some(u16::from(b'b'))
        || lower(at + 2) != Some(u16::from(b'r'))
    {
        return None;
    }
    let mut end = text.run(at + 3, is_space);
    end += usize::from(text.at(end) == Some(u16::from(b'/')));
    (text.at(end) == Some(u16::from(b'>'))).then_some(end + 1)
}

/// `text.replace(/<(?!br\s*\/?>)/gi, '&lt;').replace(/:fa-/g, '&colon;fa-')`. Neither replacement
/// can make or break a match of the other, so one pass does both.
fn plain_text(raw: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(raw.len());
    for (at, &c) in raw.iter().enumerate() {
        if c == u16::from(b'<') && line_break(raw, at).is_none() {
            out.extend("&lt;".encode_utf16());
        } else if c == u16::from(b':') && raw.starts_at(at, ":fa-") {
            out.extend("&colon;".encode_utf16());
        } else {
            out.push(c);
        }
    }
    out
}

/// Pele's `plainLabel`: lays out text as Mermaid's journey and timeline draw it, as plain text in
/// which only `<br>` is markup, so that other tags and icon names show as written. `raw` is `None`
/// where Pele's is `undefined`.
pub(crate) fn plain_label(
    raw: Option<&[u16]>,
    measurer: &dyn TextMeasurer,
    size: f64,
    max: f64,
    base: Emphasis,
) -> Label {
    let text = raw.map(plain_text).unwrap_or_default();
    layout_label(&text, false, measurer, size, max, base)
}

/// Where the piece of text at `at` that a word can be cut between ends, as Pele's
/// `/<br\s*\/?>|ﬂ°°?\w+¶ß|&#?\w+;|[\ud800-\udbff][\udc00-\udfff]|[\s\S]/gi` matches it: a line
/// break, an entity the preprocessor encoded, an entity, a surrogate pair, or one code unit.
fn piece_end(text: &[u16], at: usize) -> usize {
    let words = |from: usize| Some(text.run(from, is_word)).filter(|&end| end > from);
    let encoded = || {
        if text.at(at) != Some(0xfb02) || text.at(at + 1) != Some(0xb0) {
            return None;
        }
        let name = at + 2 + usize::from(text.at(at + 2) == Some(0xb0));
        let end = words(name)?;
        (text.at(end) == Some(0xb6) && text.at(end + 1) == Some(0xdf)).then_some(end + 2)
    };
    let entity = || {
        if text.at(at) != Some(u16::from(b'&')) {
            return None;
        }
        let name = at + 1 + usize::from(text.at(at + 1) == Some(u16::from(b'#')));
        let end = words(name)?;
        (text.at(end) == Some(u16::from(b';'))).then_some(end + 1)
    };
    line_break(text, at).or_else(encoded).or_else(entity).unwrap_or_else(|| point_end(text, at))
}

/// Pele's `fitLabel`: `plain_label`, which wraps at spaces only, and then for a box of a set width
/// a cut in any word too long to fit.
pub(crate) fn fit_label(
    raw: Option<&[u16]>,
    measurer: &dyn TextMeasurer,
    size: f64,
    max: f64,
    base: Emphasis,
) -> Label {
    let label = plain_label(raw, measurer, size, max, base);
    let Some(raw) = raw else {
        return label;
    };
    if label.width <= max {
        return label;
    }
    let mut out = Vec::with_capacity(raw.len() + 8);
    let mut line = 0.0;
    let mut at = 0;
    while at < raw.len() {
        let end = piece_end(raw, at);
        let piece = &raw[at..end];
        if piece.trim_js().is_empty() || (piece.len() > 2 && piece[0] == u16::from(b'<')) {
            line = 0.0;
        } else {
            // An entity is measured as one character.
            let w = measured(measurer, if piece.len() > 2 { &[0x78] } else { piece }, size, base);
            if line > 0.0 && line + w > max {
                out.push(10);
                line = 0.0;
            }
            line += w;
        }
        out.extend_from_slice(piece);
        at = end;
    }
    plain_label(Some(&out), measurer, size, max, base)
}

/// A line of text fitted into a width.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fitted {
    pub(crate) text: JsText,
    pub(crate) size: f64,
    pub(crate) width: f64,
}

/// Pele's `fitText`: fits one line of text into `max`, first by shrinking the font down to `min`,
/// then by cutting the text and adding an ellipsis. `None` when not even one character fits.
pub(crate) fn fit_text(
    text: &[u16],
    max: f64,
    size: f64,
    min: f64,
    measurer: &dyn TextMeasurer,
    style: Emphasis,
) -> Option<Fitted> {
    if text.is_empty() || max <= 0.0 {
        return None;
    }
    let mut width = measured(measurer, text, size, style);
    if width <= max {
        return Some(Fitted { text: text.into(), size, width });
    }
    // Width grows with font size, so one division lands on the right size or just above it.
    let mut fitted = js::max(min, ((size * max) / width).floor());
    while fitted >= min {
        width = measured(measurer, text, fitted, style);
        if width <= max {
            return Some(Fitted { text: text.into(), size: fitted, width });
        }
        fitted -= 1.0;
    }
    let points = code_points(text);
    let cut = |count: usize| {
        let mut cut = points[..count].concat().trim_end_js().to_vec();
        cut.push(0x2026);
        cut
    };
    let (mut low, mut high) = (0, points.len() - 1);
    while low < high {
        let mid = (low + high + 1) >> 1;
        if measured(measurer, &cut(mid), min, style) <= max {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    if low == 0 {
        return None;
    }
    let cut = cut(low);
    width = measured(measurer, &cut, min, style);
    (width <= max).then(|| Fitted { text: cut.into(), size: min, width })
}

/// Pele's `literalLabel`: a label for literal text, such as a filename or a protocol field, which
/// keeps every character but the spaces a line breaks at. A word too long for `room` breaks
/// between code points, so measuring stays bounded by one line of text.
pub(crate) fn literal_label(text: &[u16], measurer: &dyn TextMeasurer, size: f64, room: f64, style: Emphasis) -> Label {
    let mut lines: Vec<Vec<u16>> = Vec::new();
    for raw in split_unit(text, 10) {
        let points = code_points(raw);
        if points.is_empty() {
            lines.push(Vec::new());
        }
        let mut from = 0;
        while from < points.len() {
            let left = points.len() - from;
            let fits = |length: usize| measured(measurer, &points[from..from + length].concat(), size, style) <= room;
            let mut hi = 1;
            while hi < left && fits(hi) {
                hi = left.min(hi * 2);
            }
            let mut lo = 1;
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if fits(mid) {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            let mut take = lo;
            let wraps = from + take < points.len();
            if wraps
                && !is_space_point(points[from + take])
                && let Some(space) = (1..take).rev().find(|&i| is_space_point(points[from + i]))
            {
                take = space + 1;
            }
            let line = points[from..from + take].concat();
            let trimmed = line.trim_end_js();
            lines.push(if wraps && !trimmed.is_empty() { trimmed.to_vec() } else { line });
            from += take;
            while wraps && from < points.len() && is_space_point(points[from]) {
                from += 1;
            }
        }
    }
    if text.is_empty() {
        lines.clear();
    }
    let line_height = js::round(size * LINE_HEIGHT);
    let mut widest = 0.0;
    let lines: Vec<LabelLine> = lines
        .into_iter()
        .map(|line| {
            let width = measured(measurer, &line, size, style);
            widest = js::max(widest, width);
            LabelLine { runs: vec![LabelRun { text: line.into(), style, width }], width }
        })
        .collect();
    Label { height: lines.len() as f64 * line_height, lines, width: widest, size, line_height }
}

/// Pele's `breakLabel`: a label whose lines are wider than `room` broken between characters, each
/// run keeping its emphasis.
pub(crate) fn break_label(label: &Label, measurer: &dyn TextMeasurer, room: f64) -> Label {
    if label.width <= room {
        return label.clone();
    }
    let mut lines = Vec::new();
    for line in &label.lines {
        if line.width <= room {
            lines.push(line.clone());
            continue;
        }
        let mut current: Vec<LabelRun> = Vec::new();
        let mut width = 0.0;
        let commit = |current: &mut Vec<LabelRun>, width: &mut f64, lines: &mut Vec<LabelLine>| {
            lines.push(LabelLine { runs: std::mem::take(current), width: *width });
            *width = 0.0;
        };
        for run in &line.runs {
            for point in code_points(&run.text) {
                let last = current.last().filter(|last| last.style == run.style);
                let mut added = match last {
                    Some(last) => {
                        measured(measurer, &[&last.text[..], point].concat(), label.size, run.style) - last.width
                    }
                    None => measured(measurer, point, label.size, run.style),
                };
                let mut joined = last.is_some();
                if !current.is_empty() && width + added > room {
                    commit(&mut current, &mut width, &mut lines);
                    joined = false;
                    added = measured(measurer, point, label.size, run.style);
                }
                match current.last_mut() {
                    Some(last) if joined => {
                        last.text.push_units(point);
                        last.width += added;
                    }
                    _ => current.push(LabelRun { text: point.into(), style: run.style, width: added }),
                }
                width += added;
            }
        }
        if !current.is_empty() || line.runs.is_empty() {
            commit(&mut current, &mut width, &mut lines);
        }
    }
    let mut widest = 0.0;
    for line in &lines {
        widest = js::max(widest, line.width);
    }
    Label { height: lines.len() as f64 * label.line_height, lines, width: widest, ..label.clone() }
}
