//! Pele's YAML reader (`src/util/yaml.ts`), for front matter: block and flow collections, quoted
//! and plain scalars, and block scalars. It is ported statement by statement, so that it accepts,
//! rejects and coerces exactly what Pele does.

use super::{JsObject, JsText, JsValue, Units, decimal, hex_digit, is_digit, is_space, split_unit};
use crate::js;

const MAX_DEPTH: usize = 64;

enum YamlError {
    /// A quoted scalar or flow collection runs past the text read so far.
    Incomplete(&'static str),
    Invalid(String),
}

impl YamlError {
    fn message(self) -> String {
        match self {
            YamlError::Incomplete(message) => message.to_owned(),
            YamlError::Invalid(message) => message,
        }
    }
}

fn invalid<T>(message: &str) -> Result<T, YamlError> {
    Err(YamlError::Invalid(message.to_owned()))
}

const fn unit(c: u8) -> u16 {
    c as u16
}

fn is(c: Option<u16>, expected: u8) -> bool {
    c == Some(unit(expected))
}

fn is_blank(c: Option<u16>) -> bool {
    matches!(c, Some(32 | 9 | 10 | 13))
}

fn indent_of(line: &[u16]) -> isize {
    (line.len() - line.trim_start_js().len()) as isize
}

fn repeat(c: u16, count: usize) -> Vec<u16> {
    vec![c; count]
}

struct Reader {
    lines: Vec<Vec<u16>>,
    i: usize,
}

impl Reader {
    fn new(src: &[u16]) -> Self {
        let normalized = super::normalize_line_breaks(src);
        Reader { lines: split_unit(&normalized, 10).into_iter().map(<[u16]>::to_vec).collect(), i: 0 }
    }

    fn skip_blank(&mut self) {
        while self.i < self.lines.len() {
            let trimmed = self.lines[self.i].trim_js();
            if !trimmed.is_empty() && trimmed[0] != unit(b'#') && !trimmed.is_str("---") {
                break;
            }
            self.i += 1;
        }
    }

    fn indent(&self) -> Result<usize, YamlError> {
        let line = &self.lines[self.i];
        let k = line.run(0, |c| c == 32);
        if is(line.at(k), b'\t') {
            return invalid("YAML: tabs are not allowed for indentation");
        }
        Ok(k)
    }
}

/// `[\s#:,[\]{}&*!|>'"%@`]`
fn starts_no_key(c: u16) -> bool {
    is_space(c) || u8::try_from(c).is_ok_and(|c| b"#:,[]{}&*!|>'\"%@`".contains(&c))
}

/// Splits `key: rest` at the start of a line into the raw key and the offset where the value starts.
fn split_key(line: &[u16]) -> Option<(&[u16], usize)> {
    let first = line.at(0)?;
    let end;
    let mut colon;
    if first == unit(b'"') || first == unit(b'\'') {
        let mut i = 1;
        loop {
            let c = line.at(i)?;
            if first == unit(b'"') && c == unit(b'\\') {
                i += 2;
            } else if c == first {
                if first == unit(b'"') || !is(line.at(i + 1), b'\'') {
                    break;
                }
                i += 2;
            } else {
                i += 1;
            }
        }
        end = i + 1;
        colon = end;
        while is_blank(line.at(colon)) {
            colon += 1;
        }
        if !is(line.at(colon), b':') {
            return None;
        }
    } else {
        if starts_no_key(first) {
            return None;
        }
        if first == unit(b'-')
            && (line.len() < 2 || is_blank(line.at(1)) || is(line.at(1), b':') || is(line.at(1), b'#'))
        {
            return None;
        }
        colon = line.run(1, |c| c != unit(b':') && c != unit(b'#'));
        if !is(line.at(colon), b':') {
            return None;
        }
        let mut key_end = colon;
        while is_blank(line.at(key_end - 1)) {
            key_end -= 1;
        }
        end = key_end;
    }
    let mut length = colon + 1;
    if length < line.len() && !is_blank(line.at(length)) {
        return None;
    }
    while is_blank(line.at(length)) {
        length += 1;
    }
    Some((&line[..end], length))
}

fn all_in(text: &[u16], digits: &[u8]) -> bool {
    !text.is_empty() && text.iter().all(|&c| u8::try_from(c).is_ok_and(|c| digits.contains(&c)))
}

fn unsigned(text: &[u16]) -> &[u16] {
    match text.first() {
        Some(&c) if c == unit(b'-') || c == unit(b'+') => &text[1..],
        _ => text,
    }
}

/// `/^[-+]?(?:0|[1-9][0-9]*)$/`
fn is_int(s: &[u16]) -> bool {
    let digits = unsigned(s);
    all_in(digits, b"0123456789") && (digits.len() == 1 || digits[0] != unit(b'0'))
}

/// `/^[-+]?(?:[0-9][0-9]*(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][-+]?[0-9]+)?$/`
fn is_float(s: &[u16]) -> bool {
    let body = unsigned(s);
    let whole = body.run(0, is_digit);
    let mut end = whole;
    if whole > 0 {
        if is(body.at(end), b'.') {
            end = body.run(end + 1, is_digit);
        }
    } else {
        if !is(body.at(0), b'.') {
            return false;
        }
        end = body.run(1, is_digit);
        if end == 1 {
            return false;
        }
    }
    if end == body.len() {
        return true;
    }
    if !matches!(body.at(end), Some(69 | 101)) {
        return false;
    }
    all_in(unsigned(&body[end + 1..]), b"0123456789")
}

/// A `0x`, `0o` or `0b` integer, as `Number(text)` reads it.
fn radix_literal(s: &[u16]) -> Option<f64> {
    let body = unsigned(s);
    let (radix, digits): (u32, &[u8]) = match (body.at(0), body.at(1)) {
        (Some(48), Some(120)) => (16, b"0123456789abcdefABCDEF"),
        (Some(48), Some(111)) => (8, b"01234567"),
        (Some(48), Some(98)) => (2, b"01"),
        _ => return None,
    };
    if !all_in(&body[2..], digits) {
        return None;
    }
    let mut exact: u128 = 0;
    let mut value = 0.0;
    let mut overflowed = false;
    for &c in &body[2..] {
        let digit = hex_digit(c).expect("a checked digit");
        match exact.checked_mul(u128::from(radix)).and_then(|total| total.checked_add(u128::from(digit))) {
            Some(total) if !overflowed => exact = total,
            _ => overflowed = true,
        }
        value = value * f64::from(radix) + f64::from(digit);
    }
    let magnitude = if overflowed { value } else { exact as f64 };
    Some(if s[0] == unit(b'-') { -magnitude } else { magnitude })
}

fn scalar(raw: &[u16]) -> JsValue {
    let s = raw.trim_js();
    let any = |words: &[&str]| words.iter().any(|word| s.is_str(word));
    if s.is_empty() || any(&["~", "null", "Null", "NULL"]) {
        return JsValue::Null;
    }
    if any(&["true", "True", "TRUE"]) {
        return JsValue::Bool(true);
    }
    if any(&["false", "False", "FALSE"]) {
        return JsValue::Bool(false);
    }
    let c = s[0];
    if c != unit(b'-') && c != unit(b'+') && c != unit(b'.') && !is_digit(c) {
        return JsValue::String(s.into());
    }
    if is_int(s) || is_float(s) {
        return JsValue::Number(decimal(s));
    }
    if let Some(value) = radix_literal(s) {
        return JsValue::Number(value);
    }
    let body = unsigned(s);
    if [".inf", ".Inf", ".INF"].iter().any(|word| body.is_str(word)) {
        return JsValue::Number(if c == unit(b'-') { f64::NEG_INFINITY } else { f64::INFINITY });
    }
    if any(&[".nan", ".NaN", ".NAN"]) {
        return JsValue::Number(f64::NAN);
    }
    JsValue::String(s.into())
}

fn escape(c: u16) -> Option<u16> {
    Some(match u8::try_from(c).ok()? {
        b'0' => 0,
        b'a' => 7,
        b'b' => 8,
        b't' => 9,
        b'n' => 10,
        b'v' => 11,
        b'f' => 12,
        b'r' => 13,
        b'e' => 0x1b,
        b' ' => 32,
        b'"' => 34,
        b'/' => 47,
        b'\\' => 92,
        b'N' => 0x85,
        b'_' => 0xa0,
        b'L' => 0x2028,
        b'P' => 0x2029,
        _ => return None,
    })
}

/// Folds the line breaks of a scalar: blanks around a break go, one break becomes a space, and
/// each further break stays.
fn fold(text: &[u16]) -> Vec<u16> {
    if !text.contains(&10) {
        return text.to_vec();
    }
    let lines = split_unit(text, 10);
    let last = lines.len() - 1;
    let mut out = Vec::with_capacity(text.len());
    let mut breaks = 0;
    for (i, line) in lines.iter().enumerate() {
        let from = if i > 0 { line.run(0, |c| c == 32 || c == 9) } else { 0 };
        let mut to = line.len();
        if i < last {
            while to > from && (line[to - 1] == 32 || line[to - 1] == 9) {
                to -= 1;
            }
        }
        let line = &line[from..to];
        if i > 0 {
            breaks += 1;
        }
        if !line.is_empty() || i == last {
            match breaks {
                0 => {}
                1 => out.push(32),
                more => out.extend(repeat(10, more - 1)),
            }
            breaks = 0;
            out.extend_from_slice(line);
        }
    }
    out
}

/// `parseInt(text, 16)`, or `None` for NaN.
fn parse_int_hex(text: &[u16]) -> Option<f64> {
    let text = text.trim_start_js();
    let (negative, text) = match text.first() {
        Some(&c) if c == unit(b'-') => (true, &text[1..]),
        Some(&c) if c == unit(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let text = if is(text.at(0), b'0') && matches!(text.at(1), Some(88 | 120)) { &text[2..] } else { text };
    let end = text.run(0, |c| hex_digit(c).is_some());
    if end == 0 {
        return None;
    }
    let value = text[..end].iter().fold(0.0, |total, &c| total * 16.0 + f64::from(hex_digit(c).expect("a hex digit")));
    Some(if negative { -value } else { value })
}

fn unquote_double(body: &[u16]) -> Result<Vec<u16>, YamlError> {
    if !body.contains(&unit(b'\\')) {
        return Ok(fold(body));
    }
    // `body.replace(/\\\n[ \t]*/g, '')`
    let mut joined = Vec::with_capacity(body.len());
    let mut at = 0;
    while at < body.len() {
        if body[at] == unit(b'\\') && body.at(at + 1) == Some(10) {
            at = body.run(at + 2, |c| c == 32 || c == 9);
        } else {
            joined.push(body[at]);
            at += 1;
        }
    }
    let s = fold(&joined);
    let mut out = JsText::default();
    let mut i = 0;
    while i < s.len() {
        let ch = s[i];
        if ch != unit(b'\\') {
            out.push(ch);
            i += 1;
            continue;
        }
        i += 1;
        let e = s.at(i);
        let length = match e.and_then(|e| u8::try_from(e).ok()) {
            Some(b'x') => Some(2),
            Some(b'u') => Some(4),
            Some(b'U') => Some(8),
            _ => None,
        };
        if let Some(length) = length {
            let digits = &s[(i + 1).min(s.len())..(i + 1 + length).min(s.len())];
            match parse_int_hex(digits) {
                Some(code) if (0.0..=1_114_111.0).contains(&code) => out.push_code_point(code as u32),
                Some(code) => return Err(YamlError::Invalid(format!("Invalid code point {}", js::to_string(code)))),
                None => return invalid("Invalid code point NaN"),
            }
            i += length;
        } else if let Some(escaped) = e.and_then(escape) {
            out.push(escaped);
        } else {
            return invalid("YAML: unknown escape sequence");
        }
        i += 1;
    }
    Ok(out.to_vec())
}

fn skip(s: &[u16], mut pos: usize, commas: bool) -> usize {
    while let Some(c) = s.at(pos) {
        if matches!(c, 32 | 10 | 9 | 13) || (commas && c == unit(b',')) {
            pos += 1;
        } else if c == unit(b'#') && (pos == 0 || is_space(s[pos - 1])) {
            pos = s.find_unit(10, pos).unwrap_or(s.len());
        } else {
            break;
        }
    }
    pos
}

/// Entries of a flow collection are separated by exactly one comma, as js-yaml requires.
fn entry_start(c: u16, separated: bool) -> Result<(), YamlError> {
    if !separated {
        return invalid("YAML: missed comma between flow collection entries");
    }
    if c == unit(b',') {
        return invalid("YAML: expected the node content, but found ','");
    }
    Ok(())
}

/// Parses a flow value starting at `s[pos]`; returns the value and the index after it.
fn flow(s: &[u16], pos: usize, stops: &[u8], depth: usize) -> Result<(JsValue, usize), YamlError> {
    if depth > MAX_DEPTH {
        return invalid("YAML: nesting is too deep");
    }
    let mut pos = skip(s, pos, false);
    let ch = s.at(pos);
    if is(ch, b'{') {
        let mut out = JsObject::default();
        pos += 1;
        let mut separated = true;
        loop {
            pos = skip(s, pos, false);
            let Some(c) = s.at(pos) else {
                return Err(YamlError::Incomplete("YAML: unexpected end of the stream within a flow collection"));
            };
            if c == unit(b'}') {
                return Ok((JsValue::Object(out), pos + 1));
            }
            entry_start(c, separated)?;
            let (key, after_key) = flow(s, pos, b":,}", depth + 1)?;
            pos = skip(s, after_key, false);
            let mut value = JsValue::Null;
            if is(s.at(pos), b':') {
                (value, pos) = flow(s, pos + 1, b",}", depth + 1)?;
            }
            out.set(key.to_js_string(), value);
            pos = skip(s, pos, false);
            separated = is(s.at(pos), b',');
            if separated {
                pos += 1;
            }
        }
    }
    if is(ch, b'[') {
        let mut out = Vec::new();
        pos += 1;
        let mut separated = true;
        loop {
            pos = skip(s, pos, false);
            let Some(c) = s.at(pos) else {
                return Err(YamlError::Incomplete("YAML: unexpected end of the stream within a flow collection"));
            };
            if c == unit(b']') {
                return Ok((JsValue::Array(out), pos + 1));
            }
            entry_start(c, separated)?;
            let (value, after) = flow(s, pos, b",]", depth + 1)?;
            out.push(value);
            pos = skip(s, after, false);
            separated = is(s.at(pos), b',');
            if separated {
                pos += 1;
            }
        }
    }
    if is(ch, b'"') {
        let mut end = pos + 1;
        while end < s.len() && s[end] != unit(b'"') {
            end += if s[end] == unit(b'\\') { 2 } else { 1 };
        }
        if end >= s.len() {
            return Err(YamlError::Incomplete("YAML: unexpected end of the stream within a double quoted scalar"));
        }
        return Ok((JsValue::String(unquote_double(&s[pos + 1..end])?.into()), end + 1));
    }
    if is(ch, b'\'') {
        let mut end = pos + 1;
        while end < s.len() {
            if s[end] == unit(b'\'') {
                if !is(s.at(end + 1), b'\'') {
                    break;
                }
                end += 1;
            }
            end += 1;
        }
        if end >= s.len() {
            return Err(YamlError::Incomplete("YAML: unexpected end of the stream within a single quoted scalar"));
        }
        let folded = fold(&s[pos + 1..end]);
        let mut text = Vec::with_capacity(folded.len());
        let mut at = 0;
        while at < folded.len() {
            text.push(folded[at]);
            at += if folded[at] == unit(b'\'') && is(folded.at(at + 1), b'\'') { 2 } else { 1 };
        }
        return Ok((JsValue::String(text.into()), end + 1));
    }
    let plain = |end: usize| scalar(&fold(s[pos..end].trim_js()));
    let mut end = pos;
    while let Some(c) = s.at(end) {
        let stop = u8::try_from(c).is_ok_and(|c| stops.contains(&c));
        let closes = |next: u16| is_space(next) || u8::try_from(next).is_ok_and(|next| b",[]{}".contains(&next));
        if stop && (c != unit(b':') || s.at(end + 1).is_none_or(closes)) {
            break;
        }
        if c == unit(b'#') && end > pos && is_space(s[end - 1]) {
            return Ok((plain(end), s.find_unit(10, end).unwrap_or(s.len())));
        }
        end += 1;
    }
    Ok((plain(end), end))
}

fn strip_comment(s: &[u16]) -> &[u16] {
    let mut quote = None;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if let Some(open) = quote {
            if c == unit(b'\\') && open == unit(b'"') {
                i += 1;
            } else if c == open {
                quote = None;
            }
        } else if c == unit(b'"') || c == unit(b'\'') {
            quote = Some(c);
        } else if c == unit(b'#') && (i == 0 || s[i - 1] == 32 || s[i - 1] == 9) {
            return &s[..i];
        }
        i += 1;
    }
    s
}

fn block_scalar(r: &mut Reader, header: &[u16], parent_indent: isize) -> Vec<u16> {
    let folded = header[0] == unit(b'>');
    let chomp = if header.contains(&unit(b'-')) {
        b'-'
    } else if header.contains(&unit(b'+')) {
        b'+'
    } else {
        0
    };
    let explicit = header.iter().find(|&&c| (49..=57).contains(&c));
    let mut indent = explicit.map(|&digit| parent_indent + isize::from(digit as u8 - b'0'));
    let mut out: Vec<Vec<u16>> = Vec::new();
    while r.i < r.lines.len() {
        let line = &r.lines[r.i];
        if line.trim_js().is_empty() {
            out.push(Vec::new());
            r.i += 1;
            continue;
        }
        let k = indent_of(line);
        let indent = match indent {
            Some(indent) => indent,
            None => {
                if k <= parent_indent {
                    break;
                }
                *indent.insert(k)
            }
        };
        if k < indent {
            break;
        }
        out.push(line[(indent.max(0) as usize).min(line.len())..].to_vec());
        r.i += 1;
    }
    let mut trailing = 0;
    while out.last().is_some_and(Vec::is_empty) {
        out.pop();
        trailing += 1;
    }
    let indented = |line: &[u16]| matches!(line.first(), Some(32 | 9));
    let mut text = Vec::new();
    for (i, cur) in out.iter().enumerate() {
        if i > 0 {
            if !folded || cur.is_empty() {
                text.push(10);
            } else if !out[i - 1].is_empty() {
                text.push(if indented(&out[i - 1]) || indented(cur) { 10 } else { 32 });
            }
        }
        text.extend_from_slice(cur);
    }
    if out.is_empty() {
        return if chomp == b'+' { repeat(10, trailing) } else { Vec::new() };
    }
    match chomp {
        b'-' => {}
        b'+' => text.extend(repeat(10, trailing + 1)),
        _ => text.push(10),
    }
    text
}

/// `/^[|>][-+0-9]*\s*(?:#.*)?$/`
fn is_block_header(rest: &[u16]) -> bool {
    let flags = rest.run(1, |c| c == unit(b'-') || c == unit(b'+') || is_digit(c));
    let end = rest.run(flags, is_space);
    end == rest.len() || (rest[end] == unit(b'#') && !rest[end..].iter().any(|&c| super::is_line_end(c)))
}

fn inline_value(r: &mut Reader, rest: &[u16], indent: isize) -> Result<JsValue, YamlError> {
    let first = rest.at(0);
    if (is(first, b'|') || is(first, b'>')) && is_block_header(rest) {
        return Ok(JsValue::String(block_scalar(r, rest, indent).into()));
    }
    if is(first, b'{') || is(first, b'[') || is(first, b'"') || is(first, b'\'') {
        // The value may continue on later lines. Read them in growing batches until it closes.
        let start = r.i;
        let mut text = rest.to_vec();
        let mut taken = 0;
        let mut batch = 1;
        loop {
            match flow(&text, 0, b"", 0) {
                Ok((value, end)) => {
                    let used = text[..end].iter().filter(|&&c| c == 10).count();
                    let line_end = text.find_unit(10, end).unwrap_or(text.len());
                    if !strip_comment(&text[end..line_end]).trim_js().is_empty() {
                        return invalid("YAML: unexpected content after a value");
                    }
                    r.i = start + used;
                    return Ok(value);
                }
                Err(YamlError::Incomplete(message)) => {
                    if start + taken >= r.lines.len() {
                        return invalid(message);
                    }
                    let more = &r.lines[start + taken..(start + taken + batch).min(r.lines.len())];
                    for line in more {
                        text.push(10);
                        text.extend_from_slice(line);
                    }
                    taken += more.len();
                    batch *= 2;
                }
                Err(error) => return Err(error),
            }
        }
    }
    let mut text = strip_comment(rest).trim_js().to_vec();
    while r.i < r.lines.len() {
        let line = &r.lines[r.i];
        if line.trim_js().is_empty() {
            let mut j = r.i;
            while j < r.lines.len() && r.lines[j].trim_js().is_empty() {
                j += 1;
            }
            if j >= r.lines.len() || indent_of(&r.lines[j]) <= indent {
                break;
            }
            text.extend(repeat(10, j - r.i));
            r.i = j;
            continue;
        }
        if indent_of(line) <= indent {
            break;
        }
        if text.last() != Some(&10) {
            text.push(32);
        }
        text.extend_from_slice(strip_comment(line).trim_js());
        r.i += 1;
    }
    Ok(scalar(&text))
}

fn is_sequence_entry(line: &[u16]) -> bool {
    line.is_str("-") || line.starts_at(0, "- ")
}

fn block(r: &mut Reader, min_indent: usize, depth: usize) -> Result<JsValue, YamlError> {
    if depth > MAX_DEPTH {
        return invalid("YAML: nesting is too deep");
    }
    r.skip_blank();
    if r.i >= r.lines.len() {
        return Ok(JsValue::Null);
    }
    let indent = r.indent()?;
    if indent < min_indent {
        return Ok(JsValue::Null);
    }
    let line = r.lines[r.i][indent..].to_vec();

    if is_sequence_entry(&line) {
        let mut out = Vec::new();
        while r.i < r.lines.len() {
            r.skip_blank();
            if r.i >= r.lines.len() || r.indent()? != indent {
                break;
            }
            let cur = r.lines[r.i][indent..].to_vec();
            if !is_sequence_entry(&cur) {
                break;
            }
            let rest = &cur[1..];
            let pad = rest.len() - rest.trim_start_js().len() + 1;
            let body = strip_comment(rest).trim_js();
            if body.is_empty() {
                r.i += 1;
                out.push(block(r, indent + 1, depth + 1)?);
            } else if split_key(body).is_some() {
                let mut shifted = repeat(32, indent + pad);
                shifted.extend_from_slice(rest.trim_start_js());
                r.lines[r.i] = shifted;
                out.push(block(r, indent + pad, depth + 1)?);
            } else {
                r.i += 1;
                out.push(inline_value(r, rest.trim_js(), indent as isize)?);
            }
        }
        return Ok(JsValue::Array(out));
    }

    if split_key(&line).is_none() {
        r.i += 1;
        return inline_value(r, line.trim_js(), indent as isize - 1);
    }

    let mut out = JsObject::default();
    while r.i < r.lines.len() {
        r.skip_blank();
        if r.i >= r.lines.len() {
            break;
        }
        let k = r.indent()?;
        if k < indent {
            break;
        }
        if k > indent {
            return invalid("YAML: bad indentation of a mapping entry");
        }
        let cur = r.lines[r.i][indent..].to_vec();
        let Some((raw_key, length)) = split_key(&cur) else {
            return invalid("YAML: can not read a block mapping entry");
        };
        let key = flow(raw_key, 0, b"", 0)?.0.to_js_string();
        let rest = &cur[length.min(cur.len())..];
        r.i += 1;
        let value = if strip_comment(rest).trim_js().is_empty() {
            r.skip_blank();
            if r.i < r.lines.len() {
                let next = r.indent()?;
                let sequence = is_sequence_entry(&r.lines[r.i][next..]);
                if next > indent || (sequence && next == indent) {
                    block(r, if sequence { indent } else { indent + 1 }, depth + 1)?
                } else {
                    JsValue::Null
                }
            } else {
                JsValue::Null
            }
        } else {
            inline_value(r, rest.trim_js(), indent as isize)?
        };
        out.set(key, value);
    }
    Ok(JsValue::Object(out))
}

/// Pele's `parseYaml`. The error is the message Pele's reader throws.
pub(super) fn parse_yaml(src: &[u16]) -> Result<JsValue, String> {
    let trimmed = src.trim_js();
    if is(trimmed.at(0), b'{') || is(trimmed.at(0), b'[') {
        let (value, end) = flow(trimmed, 0, b"", 0).map_err(YamlError::message)?;
        if skip(trimmed, end, false) == trimmed.len() {
            return Ok(value);
        }
    }
    let mut r = Reader::new(src);
    let value = block(&mut r, 0, 0).map_err(YamlError::message)?;
    r.skip_blank();
    if r.i < r.lines.len() && !r.lines[r.i].trim_js().is_str("...") {
        return Err("YAML: end of the stream or a document separator is expected".to_owned());
    }
    Ok(value)
}
