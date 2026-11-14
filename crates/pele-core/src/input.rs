//! The input boundary. Mermaid source becomes JavaScript-shaped text and values here, so the
//! code behind it can trust what it is given.
//!
//! Pele's regular expressions are written out as scanners over UTF-16 code units. Each one names
//! the pattern it answers for.

mod yaml;

use crate::js;
use crate::{Diagnostic, ErrorCode, RenderError};
use std::collections::HashMap;
use std::sync::LazyLock;

/// A JavaScript string: UTF-16 code units, which may hold a surrogate with no partner.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub(crate) struct JsText(Vec<u16>);

impl JsText {
    pub(crate) fn push_str(&mut self, text: &str) {
        self.0.extend(text.encode_utf16());
    }

    pub(crate) fn push_units(&mut self, units: &[u16]) {
        self.0.extend_from_slice(units);
    }

    pub(crate) fn push(&mut self, unit: u16) {
        self.0.push(unit);
    }

    /// `String.fromCodePoint` appended to the text.
    pub(crate) fn push_code_point(&mut self, code_point: u32) {
        if code_point < 0x10000 {
            self.0.push(code_point as u16);
        } else {
            let offset = code_point - 0x10000;
            self.0.push(0xd800 + (offset >> 10) as u16);
            self.0.push(0xdc00 + (offset & 0x3ff) as u16);
        }
    }

    pub(crate) fn truncate(&mut self, length: usize) {
        self.0.truncate(length);
    }

    /// The text for a message, with each lone surrogate shown as U+FFFD.
    pub(crate) fn to_display(&self) -> String {
        String::from_utf16_lossy(&self.0)
    }
}

impl std::ops::Deref for JsText {
    type Target = [u16];
    fn deref(&self) -> &[u16] {
        &self.0
    }
}

impl From<&str> for JsText {
    fn from(text: &str) -> Self {
        JsText(text.encode_utf16().collect())
    }
}

impl From<&[u16]> for JsText {
    fn from(units: &[u16]) -> Self {
        JsText(units.to_vec())
    }
}

impl From<Vec<u16>> for JsText {
    fn from(units: Vec<u16>) -> Self {
        JsText(units)
    }
}

impl std::fmt::Debug for JsText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_display())
    }
}

/// `\s`, which is also what `trim` removes.
pub(crate) fn is_space(c: u16) -> bool {
    matches!(
        c,
        9..=13 | 32 | 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff
    )
}

/// `\w`, which is ASCII only in JavaScript.
pub(crate) fn is_word(c: u16) -> bool {
    matches!(c, 48..=57 | 65..=90 | 97..=122 | 95)
}

/// A line terminator: what `.` does not match and where `^` and `$` match under the `m` flag.
pub(crate) fn is_line_end(c: u16) -> bool {
    matches!(c, 10 | 13 | 0x2028 | 0x2029)
}

pub(crate) fn is_digit(c: u16) -> bool {
    matches!(c, 48..=57)
}

pub(crate) fn is_blank(c: u16) -> bool {
    c == 32 || c == 9
}

pub(crate) fn hex_digit(c: u16) -> Option<u32> {
    char::from_u32(u32::from(c)).and_then(|ch| ch.to_digit(16))
}

/// String methods of JavaScript, on code units.
pub(crate) trait Units {
    fn units(&self) -> &[u16];

    fn at(&self, index: usize) -> Option<u16> {
        self.units().get(index).copied()
    }

    fn trim_start_js(&self) -> &[u16] {
        let units = self.units();
        let start = units.iter().position(|&c| !is_space(c)).unwrap_or(units.len());
        &units[start..]
    }

    fn trim_end_js(&self) -> &[u16] {
        let units = self.units();
        let end = units.iter().rposition(|&c| !is_space(c)).map_or(0, |last| last + 1);
        &units[..end]
    }

    fn trim_js(&self) -> &[u16] {
        self.trim_start_js().trim_end_js()
    }

    /// `startsWith(needle, at)`.
    fn starts_at(&self, at: usize, needle: &str) -> bool {
        let units = self.units();
        needle.encode_utf16().enumerate().all(|(offset, unit)| units.get(at + offset) == Some(&unit))
    }

    /// `indexOf(needle, from)`.
    fn find_str(&self, needle: &str, from: usize) -> Option<usize> {
        (from..=self.units().len()).find(|&at| self.starts_at(at, needle))
    }

    fn has(&self, needle: &str) -> bool {
        self.find_str(needle, 0).is_some()
    }

    fn is_str(&self, text: &str) -> bool {
        self.units().iter().copied().eq(text.encode_utf16())
    }

    fn find_unit(&self, unit: u16, from: usize) -> Option<usize> {
        let units = self.units();
        (from.min(units.len())..units.len()).find(|&at| units[at] == unit)
    }

    /// The number of leading code units `matches` holds for.
    fn run(&self, from: usize, matches: impl Fn(u16) -> bool) -> usize {
        let units = self.units();
        let mut end = from.min(units.len());
        while end < units.len() && matches(units[end]) {
            end += 1;
        }
        end
    }
}

impl Units for [u16] {
    fn units(&self) -> &[u16] {
        self
    }
}

/// `text.split(separator)` for a one-unit separator.
pub(crate) fn split_unit(text: &[u16], separator: u16) -> Vec<&[u16]> {
    text.split(|&c| c == separator).collect()
}

/// A value as `JSON.parse` and Pele's YAML reader build it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JsValue {
    Null,
    Bool(bool),
    Number(f64),
    String(JsText),
    Array(Vec<JsValue>),
    Object(JsObject),
}

impl JsValue {
    /// `String(value)`.
    pub(crate) fn to_js_string(&self) -> JsText {
        match self {
            JsValue::Null => "null".into(),
            JsValue::Bool(value) => if *value { "true" } else { "false" }.into(),
            JsValue::Number(value) => js::to_string(*value).as_str().into(),
            JsValue::String(text) => text.clone(),
            JsValue::Array(items) => {
                let mut out = JsText::default();
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(u16::from(b','));
                    }
                    if !matches!(item, JsValue::Null) {
                        out.push_units(&item.to_js_string());
                    }
                }
                out
            }
            JsValue::Object(_) => "[object Object]".into(),
        }
    }

    /// Whether `if (value)` runs.
    pub(crate) fn truthy(&self) -> bool {
        match self {
            JsValue::Null => false,
            JsValue::Bool(value) => *value,
            JsValue::Number(value) => *value != 0.0 && !value.is_nan(),
            JsValue::String(text) => !text.is_empty(),
            JsValue::Array(_) | JsValue::Object(_) => true,
        }
    }

    /// Pele's `isRecord`: an object that is not an array.
    pub(crate) fn as_record(&self) -> Option<&JsObject> {
        match self {
            JsValue::Object(object) => Some(object),
            _ => None,
        }
    }

    pub(crate) fn as_number(&self) -> Option<f64> {
        match self {
            JsValue::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn as_text(&self) -> Option<&JsText> {
        match self {
            JsValue::String(text) => Some(text),
            _ => None,
        }
    }
}

/// An object with JavaScript's key order: array indexes ascending, then the rest as inserted.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct JsObject {
    entries: Vec<(JsText, JsValue)>,
}

const UNSAFE_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

/// The array index a property key names, if it is one in canonical form.
fn array_index(key: &[u16]) -> Option<u32> {
    if key.is_empty() || key.len() > 10 || !key.iter().all(|&c| is_digit(c)) || (key.len() > 1 && key[0] == 48) {
        return None;
    }
    let value = key.iter().fold(0u64, |total, &c| total * 10 + u64::from(c - 48));
    u32::try_from(value).ok().filter(|&index| index < u32::MAX)
}

impl JsObject {
    pub(crate) fn get(&self, key: &str) -> Option<&JsValue> {
        self.entries.iter().find(|(name, _)| name.is_str(key)).map(|(_, value)| value)
    }

    fn position(&self, key: &[u16]) -> Option<usize> {
        self.entries.iter().position(|(name, _)| **name == *key)
    }

    pub(crate) fn set(&mut self, key: JsText, value: JsValue) {
        if let Some(at) = self.position(&key) {
            self.entries[at].1 = value;
            return;
        }
        let at = match array_index(&key) {
            Some(index) => {
                self.entries.iter().take_while(|(name, _)| array_index(name).is_some_and(|other| other < index)).count()
            }
            None => self.entries.len(),
        };
        self.entries.insert(at, (key, value));
    }

    /// `{...self, ...over}`.
    pub(crate) fn spread(&self, over: &JsObject) -> JsObject {
        let mut out = self.clone();
        for (key, value) in &over.entries {
            out.set(key.clone(), value.clone());
        }
        out
    }

    /// Pele's deep `merge`, which never copies a key that could reach a prototype.
    pub(crate) fn merge(&mut self, source: &JsObject) {
        for (key, value) in &source.entries {
            if UNSAFE_KEYS.iter().any(|unsafe_key| key.is_str(unsafe_key)) {
                continue;
            }
            let Some(record) = value.as_record() else {
                self.set(key.clone(), value.clone());
                continue;
            };
            match self.position(key) {
                Some(at) if matches!(self.entries[at].1, JsValue::Object(_)) => {
                    if let JsValue::Object(current) = &mut self.entries[at].1 {
                        current.merge(record);
                    }
                }
                _ => {
                    let mut copy = JsObject::default();
                    copy.merge(record);
                    self.set(key.clone(), JsValue::Object(copy));
                }
            }
        }
    }
}

/// How deep a JSON value may nest. V8 has no fixed bound, so deeper input is a limit error here.
const MAX_JSON_DEPTH: usize = 512;

pub(crate) enum JsonError {
    Invalid,
    TooDeep,
}

/// `JSON.parse`.
pub(crate) fn parse_json(text: &[u16]) -> Result<JsValue, JsonError> {
    let mut parser = Json { text, at: 0 };
    let value = parser.value(0)?;
    parser.skip_space();
    if parser.at == text.len() { Ok(value) } else { Err(JsonError::Invalid) }
}

struct Json<'a> {
    text: &'a [u16],
    at: usize,
}

impl Json<'_> {
    fn skip_space(&mut self) {
        self.at = self.text.run(self.at, |c| matches!(c, 9 | 10 | 13 | 32));
    }

    fn eat(&mut self, literal: &str) -> bool {
        let found = self.text.starts_at(self.at, literal);
        if found {
            self.at += literal.len();
        }
        found
    }

    fn value(&mut self, depth: usize) -> Result<JsValue, JsonError> {
        if depth > MAX_JSON_DEPTH {
            return Err(JsonError::TooDeep);
        }
        self.skip_space();
        match self.text.at(self.at).and_then(|c| u8::try_from(c).ok()) {
            Some(b'{') => {
                self.at += 1;
                let mut object = JsObject::default();
                self.skip_space();
                if self.eat("}") {
                    return Ok(JsValue::Object(object));
                }
                loop {
                    self.skip_space();
                    let key = self.string()?;
                    self.skip_space();
                    if !self.eat(":") {
                        return Err(JsonError::Invalid);
                    }
                    let value = self.value(depth + 1)?;
                    object.set(key, value);
                    self.skip_space();
                    if self.eat("}") {
                        return Ok(JsValue::Object(object));
                    }
                    if !self.eat(",") {
                        return Err(JsonError::Invalid);
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                self.skip_space();
                if self.eat("]") {
                    return Ok(JsValue::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.skip_space();
                    if self.eat("]") {
                        return Ok(JsValue::Array(items));
                    }
                    if !self.eat(",") {
                        return Err(JsonError::Invalid);
                    }
                }
            }
            Some(b'"') => self.string().map(JsValue::String),
            Some(b't') if self.eat("true") => Ok(JsValue::Bool(true)),
            Some(b'f') if self.eat("false") => Ok(JsValue::Bool(false)),
            Some(b'n') if self.eat("null") => Ok(JsValue::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(JsonError::Invalid),
        }
    }

    fn string(&mut self) -> Result<JsText, JsonError> {
        if !self.eat("\"") {
            return Err(JsonError::Invalid);
        }
        let mut out = JsText::default();
        loop {
            let c = self.text.at(self.at).ok_or(JsonError::Invalid)?;
            self.at += 1;
            match c {
                34 => return Ok(out),
                92 => {
                    let escape = self.text.at(self.at).ok_or(JsonError::Invalid)?;
                    self.at += 1;
                    let unit = match u8::try_from(escape).map_err(|_| JsonError::Invalid)? {
                        b'"' => 34,
                        b'\\' => 92,
                        b'/' => 47,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'u' => {
                            let mut unit = 0u32;
                            for _ in 0..4 {
                                let digit = self.text.at(self.at).and_then(hex_digit).ok_or(JsonError::Invalid)?;
                                unit = unit * 16 + digit;
                                self.at += 1;
                            }
                            unit as u16
                        }
                        _ => return Err(JsonError::Invalid),
                    };
                    out.push(unit);
                }
                0..=31 => return Err(JsonError::Invalid),
                _ => out.push(c),
            }
        }
    }

    fn number(&mut self) -> Result<JsValue, JsonError> {
        let start = self.at;
        self.eat("-");
        let digits = self.text.run(self.at, is_digit);
        if digits == self.at || (self.text[self.at] == 48 && digits > self.at + 1) {
            return Err(JsonError::Invalid);
        }
        self.at = digits;
        if self.eat(".") {
            let fraction = self.text.run(self.at, is_digit);
            if fraction == self.at {
                return Err(JsonError::Invalid);
            }
            self.at = fraction;
        }
        if self.eat("e") || self.eat("E") {
            if !self.eat("+") {
                self.eat("-");
            }
            let exponent = self.text.run(self.at, is_digit);
            if exponent == self.at {
                return Err(JsonError::Invalid);
            }
            self.at = exponent;
        }
        Ok(JsValue::Number(decimal(&self.text[start..self.at])))
    }
}

/// `Number(text)` for text that is already known to be a decimal literal.
pub(crate) fn decimal(text: &[u16]) -> f64 {
    let ascii: String = text.iter().map(|&c| char::from(c as u8)).collect();
    let (mantissa, exponent) = match ascii.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent),
        None => (ascii.as_str(), "0"),
    };
    let (sign, digits) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let whole = if whole.is_empty() { "0" } else { whole };
    let fraction = if fraction.is_empty() { "0" } else { fraction };
    format!("{sign}{whole}.{fraction}e{exponent}").parse().expect("a decimal literal parses")
}

/// The input range `from..to` replaced by `length` output units.
pub(crate) type Edit = (usize, usize, usize);

/// The edits of each preprocessing step in order, recorded only when a source position is needed.
pub(crate) type Steps = Vec<Vec<Edit>>;

fn record(steps: &mut Option<&mut Steps>, edits: Vec<Edit>) {
    if let Some(steps) = steps {
        steps.push(edits);
    }
}

/// Pele's `inputOffset`: where the unit at `at` of the preprocessed text sits in the source.
pub(crate) fn input_offset(steps: &Steps, at: usize) -> usize {
    // An entity code grows when it is hidden, so the shift can be negative.
    let signed = |n: usize| n as isize;
    let at = steps.iter().rev().fold(signed(at), |at, edits| {
        let mut shift = 0;
        for &(from, to, length) in edits {
            let (from, to, length) = (signed(from), signed(to), signed(length));
            let start = from - shift;
            if at < start {
                break;
            }
            if at < start + length {
                return from + (at - start).min(to - from - 1);
            }
            shift += to - from - length;
        }
        at + shift
    });
    at.max(0) as usize
}

/// What preprocessing leaves of a source.
pub(crate) struct Preprocessed {
    /// The diagram text with front matter, directives and comment lines removed.
    pub(crate) text: JsText,
    pub(crate) title: Option<JsText>,
    pub(crate) config: JsObject,
}

fn syntax(message: String) -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(ErrorCode::Syntax, message))
}

pub(crate) fn nested_too_deeply() -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, "Diagram is nested too deeply.".to_owned()))
}

/// Pele's `preprocess` followed by its `cleanupComments`.
pub(crate) fn preprocess(source: &str, mut steps: Option<&mut Steps>) -> Result<Preprocessed, RenderError> {
    let mut text = single_quote_attributes(normalize_line_breaks(&source.encode_utf16().collect::<Vec<_>>()));
    let mut config = JsObject::default();
    let mut title = None;

    if let Some(front) = text.has("---").then(|| match_front_matter(&text)).flatten() {
        let parsed = yaml::parse_yaml(&front.body)
            .map_err(|message| syntax(format!("Front matter is not valid YAML. {message}")))?;
        if let Some(record) = parsed.as_record() {
            if let Some(value) = record.get("title").filter(|value| value.truthy()) {
                title = Some(value.to_js_string());
            }
            if let Some(overrides) = record.get("config").and_then(JsValue::as_record) {
                config.merge(overrides);
            }
            if let Some(mode) = record.get("displayMode").filter(|value| value.truthy()) {
                let mut gantt = config.get("gantt").and_then(JsValue::as_record).cloned().unwrap_or_default();
                gantt.set("displayMode".into(), JsValue::String(mode.to_js_string()));
                config.set("gantt".into(), JsValue::Object(gantt));
            }
        }
        text.drain(..front.length);
        record(&mut steps, vec![(0, front.length, 0)]);
    }

    let text = directives(text, &mut config, &mut steps)?;
    Ok(Preprocessed { text: cleanup_comments(text, &mut steps).into(), title, config })
}

/// `text.replace(/\r\n?/g, '\n')`.
pub(crate) fn normalize_line_breaks(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if text[at] == 13 {
            out.push(10);
            at += if text.at(at + 1) == Some(10) { 2 } else { 1 };
        } else {
            out.push(text[at]);
            at += 1;
        }
    }
    out
}

/// Rewrites `a="x"` inside `<tag ...>` as `a='x'`, up to the last `>` of the text.
/// Answers for `/<(\w+)([^>]*)>/g` and, inside it, `/="([^"]*)"/g`.
fn single_quote_attributes(text: Vec<u16>) -> Vec<u16> {
    let last_close = match text.iter().rposition(|&c| c == u16::from(b'>')) {
        Some(at) if at > 0 && text.contains(&u16::from(b'<')) => at,
        _ => return text,
    };
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at <= last_close {
        let tag_end = if text[at] == u16::from(b'<') { text.run(at + 1, is_word) } else { at + 1 };
        if tag_end == at + 1 {
            out.push(text[at]);
            at += 1;
            continue;
        }
        let close = text.find_unit(u16::from(b'>'), tag_end).expect("the text up to the last `>` ends with one");
        out.extend_from_slice(&text[at..tag_end]);
        let mut attr = tag_end;
        while attr < close {
            let quote = text.starts_at(attr, "=\"").then(|| text[..close].find_unit(34, attr + 2)).flatten();
            match quote {
                Some(end) => {
                    out.extend("='".encode_utf16());
                    out.extend_from_slice(&text[attr + 2..end]);
                    out.push(u16::from(b'\''));
                    attr = end + 1;
                }
                None => {
                    out.push(text[attr]);
                    attr += 1;
                }
            }
        }
        out.push(u16::from(b'>'));
        at = close + 1;
    }
    out.extend_from_slice(&text[last_close + 1..]);
    out
}

struct FrontMatter {
    body: Vec<u16>,
    length: usize,
}

fn is_line_break(c: Option<u16>) -> bool {
    matches!(c, Some(10 | 13))
}

/// Pele's `matchFrontMatter`: a `---` fenced YAML block at the start of the text.
fn match_front_matter(text: &[u16]) -> Option<FrontMatter> {
    let cursor = text.run(0, |c| is_space(c) && !is_line_break(Some(c)));
    let indent = &text[..cursor];
    if !text.starts_at(cursor, "---") {
        return None;
    }
    let mut opening_ends = Vec::new();
    let mut at = cursor + 3;
    while at < text.len() && is_space(text[at]) {
        if is_line_break(Some(text[at])) {
            opening_ends.push(at);
        }
        at += 1;
    }
    let first_opening = *opening_ends.first()?;

    let mut fence = indent.to_vec();
    fence.extend("---".encode_utf16());
    let fenced_at = |at: usize| text.get(at..at + fence.len()).is_some_and(|found| found == fence.as_slice());
    let closing_end = |at: usize| -> Option<usize> {
        if !is_line_break(text.at(at)) || !fenced_at(at + 1) {
            return None;
        }
        let mut last = None;
        let mut end = at + 1 + fence.len();
        while end < text.len() && is_space(text[end]) {
            if is_line_break(Some(text[end])) {
                last = Some(end);
            }
            end += 1;
        }
        last
    };
    let needle_at = |at: usize| text.at(at) == Some(10) && fenced_at(at + 1);

    let last_fence = (first_opening + 1..text.len()).rev().find(|&at| needle_at(at) && closing_end(at).is_some())?;
    let opening_end = opening_ends.iter().copied().rfind(|&end| end < last_fence).unwrap_or(first_opening);
    let body_start = opening_end + 1;
    let fence_at = (body_start..=last_fence).find(|&at| needle_at(at) && closing_end(at).is_some())?;
    let end = closing_end(fence_at)?;

    let body = &text[body_start..fence_at];
    let body = if indent.is_empty() {
        body.to_vec()
    } else {
        let lines: Vec<&[u16]> =
            split_unit(body, 10).into_iter().map(|line| line.strip_prefix(indent).unwrap_or(line)).collect();
        lines.join(&10)
    };
    Some(FrontMatter { body, length: end + 1 })
}

struct Directive {
    end: usize,
    kind: std::ops::Range<usize>,
    arguments: Option<std::ops::Range<usize>>,
}

/// Answers for Pele's `RE_DIRECTIVE` at one position:
/// `/%{2}{\s*(?:(\w+)\s*:|(\w+))\s*(?:(\w+)|((?:(?!}%{2}).|\r?\n)*))?\s*(?:}%{2})?/`.
fn match_directive(text: &[u16], at: usize) -> Option<Directive> {
    if !text.starts_at(at, "%%{") {
        return None;
    }
    let word = text.run(at + 3, is_space);
    let word_end = text.run(word, is_word);
    if word_end == word {
        return None;
    }
    let colon = text.run(word_end, is_space);
    let mut end = if text.at(colon) == Some(u16::from(b':')) { colon + 1 } else { word_end };
    end = text.run(end, is_space);

    let mut arguments = None;
    let bare = text.run(end, is_word);
    if bare > end {
        end = bare;
    } else {
        let start = end;
        loop {
            match text.at(end) {
                Some(13) if text.at(end + 1) == Some(10) => end += 2,
                Some(10) => end += 1,
                Some(c) if !is_line_end(c) && !text.starts_at(end, "}%%") => end += 1,
                _ => break,
            }
        }
        if end > start {
            arguments = Some(start..end);
        }
    }
    end = text.run(end, is_space);
    if text.starts_at(end, "}%%") {
        end += 3;
    }
    Some(Directive { end, kind: word..word_end, arguments })
}

/// Pele's `directives`: reads `%%{init: ...}%%` and `%%{wrap}%%` into the config and removes them.
fn directives(text: Vec<u16>, config: &mut JsObject, steps: &mut Option<&mut Steps>) -> Result<Vec<u16>, RenderError> {
    if !text.has("%%{") {
        return Ok(text);
    }
    let normalized: Vec<u16> =
        text.trim_js().iter().map(|&c| if c == u16::from(b'\'') { u16::from(b'"') } else { c }).collect();
    let mut at = 0;
    while at < normalized.len() {
        let Some(found) = match_directive(&normalized, at) else {
            at += 1;
            continue;
        };
        at = found.end;
        let kind = &normalized[found.kind];
        if kind.is_str("wrap") {
            config.set("wrap".into(), JsValue::Bool(true));
        } else if let Some(arguments) = found.arguments.filter(|_| kind.is_str("init") || kind.is_str("initialize")) {
            match parse_json(normalized[arguments].trim_js()) {
                Ok(JsValue::Object(overrides)) => config.merge(&overrides),
                Ok(_) | Err(JsonError::Invalid) => {}
                Err(JsonError::TooDeep) => return Err(nested_too_deeply()),
            }
        }
    }

    let mut out = Vec::with_capacity(text.len());
    let mut edits = Vec::new();
    let mut at = 0;
    while at < text.len() {
        match match_directive(&text, at) {
            Some(found) => {
                edits.push((at, found.end, 0));
                at = found.end;
            }
            None => {
                out.push(text[at]);
                at += 1;
            }
        }
    }
    record(steps, edits);
    Ok(out)
}

/// Pele's `cleanupComments`: drops `%%` comment lines, then leading whitespace.
/// The lines are those `/^\s*%%(?!{)[^\n]+\n?/gm` matches.
fn cleanup_comments(text: Vec<u16>, steps: &mut Option<&mut Steps>) -> Vec<u16> {
    let trim_start = |text: Vec<u16>, steps: &mut Option<&mut Steps>| {
        let trimmed = text.trim_start_js().to_vec();
        record(steps, vec![(0, text.len() - trimmed.len(), 0)]);
        trimmed
    };
    if !text.has("%%") {
        return trim_start(text, steps);
    }
    let mut out = Vec::with_capacity(text.len());
    let mut edits = Vec::new();
    let mut at = 0;
    // Where the whitespace run that holds `at` ends, kept so a long run is scanned once.
    let mut marker = 0;
    while at < text.len() {
        let line_start = at == 0 || is_line_end(text[at - 1]);
        if line_start && at >= marker {
            marker = text.run(at, is_space);
        }
        let commented = line_start
            && text.starts_at(marker, "%%")
            && text.at(marker + 2).is_some_and(|c| c != u16::from(b'{') && c != 10);
        if commented {
            let line_end = text.find_unit(10, marker + 2).map_or(text.len(), |newline| newline + 1);
            edits.push((at, line_end, 0));
            at = line_end;
        } else {
            out.push(text[at]);
            at += 1;
        }
    }
    record(steps, edits);
    trim_start(out, steps)
}

const PLACEHOLDER_OPEN: [u16; 2] = [0xfb02, 0xb0];
const PLACEHOLDER_CLOSE: [u16; 2] = [0xb6, 0xdf];

fn has_units(text: &[u16], needle: &[u16]) -> bool {
    text.windows(needle.len()).any(|window| window == needle)
}

/// Pele's `dropColorSemicolons`: on a line where `keyword` is followed by a colon, a `#` and
/// later a semicolon, the last semicolon goes.
fn drop_color_semicolons(text: Vec<u16>, keyword: &str, steps: &mut Option<&mut Steps>) -> Vec<u16> {
    let Some(mut from) = text.find_str(keyword, 0) else {
        record(steps, Vec::new());
        return text;
    };
    let mut edits = Vec::new();
    let mut out = Vec::new();
    let mut last = 0;
    loop {
        let line_end = text.run(from, |c| !is_line_end(c));
        let mut after_colon = false;
        let mut hash = None;
        for (at, &c) in text.iter().enumerate().take(line_end).skip(from + keyword.len()) {
            if c == u16::from(b':') {
                after_colon = true;
            } else if c == u16::from(b'#') {
                if after_colon {
                    hash = Some(at);
                    break;
                }
            } else if is_space(c) {
                after_colon = false;
            }
        }
        // Pele takes the last semicolon before the line's end and keeps it only if it comes
        // after the hash, which is the last semicolon between the two.
        if let Some(hash) = hash
            && let Some(offset) = text[hash + 1..line_end].iter().rposition(|&c| c == u16::from(b';'))
        {
            let semicolon = hash + 1 + offset;
            edits.push((semicolon, semicolon + 1, 0));
            out.extend_from_slice(&text[last..semicolon]);
            last = semicolon + 1;
        }
        match (line_end < text.len()).then(|| text.find_str(keyword, line_end + 1)).flatten() {
            Some(next) => from = next,
            None => break,
        }
    }
    record(steps, edits);
    if last == 0 {
        return text;
    }
    out.extend_from_slice(&text[last..]);
    out
}

/// Pele's `encodeEntities`: hides `#name;` codes behind placeholder characters, so that `#` and
/// `;` survive parsing.
pub(crate) fn encode_entities(text: &[u16], mut steps: Option<&mut Steps>) -> Vec<u16> {
    if !text.contains(&u16::from(b'#')) {
        return text.to_vec();
    }
    let text = drop_color_semicolons(text.to_vec(), "style", &mut steps);
    let text = drop_color_semicolons(text, "classDef", &mut steps);
    let mut out = Vec::with_capacity(text.len());
    let mut edits = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let end = if text[at] == u16::from(b'#') { text.run(at + 1, is_word) } else { at + 1 };
        if end > at + 1 && text.at(end) == Some(u16::from(b';')) {
            let inner = &text[at + 1..end];
            let start = out.len();
            out.extend_from_slice(&PLACEHOLDER_OPEN);
            if inner.iter().all(|&c| is_digit(c)) {
                out.push(0xb0);
            }
            out.extend_from_slice(inner);
            out.extend_from_slice(&PLACEHOLDER_CLOSE);
            edits.push((at, end + 1, out.len() - start));
            at = end + 1;
        } else {
            out.push(text[at]);
            at += 1;
        }
    }
    record(&mut steps, edits);
    out
}

/// Pele's `writtenCodes`: placeholders back to the `#name;` codes the author wrote.
pub(crate) fn written_codes(text: &[u16]) -> Vec<u16> {
    if !has_units(text, &PLACEHOLDER_CLOSE) {
        return text.to_vec();
    }
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if text[at..].starts_with(&PLACEHOLDER_OPEN) {
            let name = if text.at(at + 2) == Some(0xb0) { at + 3 } else { at + 2 };
            let end = text.run(name, is_word);
            if end > name && text[end..].starts_with(&PLACEHOLDER_CLOSE) {
                out.push(u16::from(b'#'));
                out.extend_from_slice(&text[name..end]);
                out.push(u16::from(b';'));
                at = end + 2;
                continue;
            }
        }
        out.push(text[at]);
        at += 1;
    }
    out
}

/// Pele's `restorePlaceholders`: placeholders back to `&name;` and `&#35;` entities.
fn restore_placeholders(text: &[u16]) -> Vec<u16> {
    let find_open = |from: usize| (from..text.len()).find(|&at| text[at..].starts_with(&PLACEHOLDER_OPEN));
    let mut out = Vec::new();
    let mut last = 0;
    let mut from = find_open(0);
    while let Some(open) = from {
        let Some(stop) = text.find_unit(0xb6, open + 2) else {
            break;
        };
        if text.at(stop + 1) == Some(0xdf) {
            let numeric = text.at(open + 2) == Some(0xb0);
            out.extend_from_slice(&text[last..open]);
            out.extend(if numeric { "&#" } else { "&" }.encode_utf16());
            out.extend_from_slice(&text[open + if numeric { 3 } else { 2 }..stop]);
            out.push(u16::from(b';'));
            last = stop + 2;
        }
        from = find_open(stop + 1);
    }
    if last == 0 {
        return text.to_vec();
    }
    out.extend_from_slice(&text[last..]);
    out
}

const NAMED: &str = "amp:38,lt:60,gt:62,quot:34,apos:39,nbsp:160,shy:173,iexcl:161,cent:162,pound:163,curren:164,yen:165,sect:167,\
copy:169,laquo:171,not:172,reg:174,deg:176,plusmn:177,sup2:178,sup3:179,micro:181,para:182,middot:183,\
sup1:185,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,times:215,divide:247,ndash:8211,mdash:8212,\
lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,\
hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,euro:8364,trade:8482,larr:8592,\
uarr:8593,rarr:8594,darr:8595,harr:8596,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,\
part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,\
radic:8730,infin:8734,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,\
asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,sube:8838,supe:8839,oplus:8853,otimes:8855,\
perp:8869,sdot:8901,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,check:10003,cross:10007,\
AMP:38,LT:60,GT:62,QUOT:34,lsqb:91,rsqb:93,lcub:123,rcub:125,verbar:124,VerticalLine:124,midast:42,\
DiacriticalGrave:96,Hat:94,UnderBar:95,\
equals:61,colon:58,semi:59,num:35,percnt:37,lpar:40,rpar:41,lbrack:91,rbrack:93,lbrace:123,rbrace:125,\
vert:124,sol:47,bsol:92,quest:63,excl:33,ast:42,plus:43,comma:44,period:46,dollar:36,commat:64,grave:96,\
hat:94,lowbar:95,tilde:126,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,\
Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,\
Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,\
eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,\
sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969";

/// The code point of each named entity. A name listed twice keeps its last code, as Pele's map does.
static NAMED_ENTITIES: LazyLock<HashMap<&str, u32>> = LazyLock::new(|| {
    let pairs = NAMED.split(',').filter_map(|pair| pair.split_once(':'));
    pairs.map(|(name, code)| (name, code.parse().expect("the entity table holds code points"))).collect()
});

fn named_entity(name: &[u16]) -> Option<u32> {
    NAMED_ENTITIES.get(String::from_utf16_lossy(name).as_str()).copied()
}

/// A code point an entity may stand for: `n > 0 && n <= 0x10ffff` and no surrogate.
fn scalar_value(digits: &[u16], radix: u32) -> Option<u32> {
    let mut value = 0u32;
    for &digit in digits {
        value = value.saturating_mul(radix).saturating_add(hex_digit(digit)?);
        if value > 0x10ffff {
            return None;
        }
    }
    (value > 0 && !(0xd800..=0xdfff).contains(&value)).then_some(value)
}

/// Pele's `decodeEntities`: Mermaid's hidden `#35;` codes and HTML entities become characters.
/// The entities are those `/&(?:#(\d+)|#[xX]([0-9a-fA-F]+)|(\w+));/g` matches.
pub(crate) fn decode_entities(text: &[u16]) -> Vec<u16> {
    let restored;
    let text = if has_units(text, &PLACEHOLDER_CLOSE) {
        restored = restore_placeholders(text);
        restored.as_slice()
    } else {
        text
    };
    if !text.contains(&u16::from(b'&')) {
        return text.to_vec();
    }
    let semicolon = |end: usize| text.at(end) == Some(u16::from(b';'));
    let mut out = JsText::default();
    let mut at = 0;
    while at < text.len() {
        if text[at] != u16::from(b'&') {
            out.push(text[at]);
            at += 1;
            continue;
        }
        let hash = text.at(at + 1) == Some(u16::from(b'#'));
        let decimal_end = text.run(at + 2, is_digit);
        let hex_end = text.run(at + 3, |c| hex_digit(c).is_some());
        let name_end = text.run(at + 1, is_word);
        let (end, code) = if hash && decimal_end > at + 2 && semicolon(decimal_end) {
            (decimal_end, scalar_value(&text[at + 2..decimal_end], 10))
        } else if hash && matches!(text.at(at + 2), Some(88 | 120)) && hex_end > at + 3 && semicolon(hex_end) {
            (hex_end, scalar_value(&text[at + 3..hex_end], 16))
        } else if name_end > at + 1 && semicolon(name_end) {
            (name_end, named_entity(&text[at + 1..name_end]))
        } else {
            out.push(text[at]);
            at += 1;
            continue;
        };
        match code {
            Some(code_point) => out.push_code_point(code_point),
            None => out.push_units(&text[at..=end]),
        }
        at = end + 1;
    }
    out.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    /// The oracle's JSON, read by this module's own parser so that a lone surrogate survives.
    fn oracle(op: &str, sources: &[String]) -> Vec<JsValue> {
        let requests: Vec<String> = sources.iter().map(|source| format!("{op}\t{}", json_string(source))).collect();
        pele_oracle::ask(&requests)
            .iter()
            .map(|answer| parse_json(&units(answer)).ok().expect("the oracle answers JSON"))
            .collect()
    }

    fn json_string(text: &str) -> String {
        let mut out = String::from("\"");
        for unit in text.encode_utf16() {
            out.push_str(&format!("\\u{unit:04x}"));
        }
        out + "\""
    }

    /// Whether our value is the oracle's, where the oracle writes a non-finite number as `{"$number": "NaN"}`.
    fn same(ours: &JsValue, theirs: &JsValue) -> bool {
        match (ours, theirs) {
            (JsValue::Number(a), JsValue::Object(wrapped)) => {
                wrapped.get("$number").and_then(JsValue::as_text).is_some_and(|text| text.is_str(&js::to_string(*a)))
            }
            (JsValue::Number(a), JsValue::Number(b)) => a == b,
            (JsValue::Array(a), JsValue::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b)),
            (JsValue::Object(a), JsValue::Object(b)) => {
                a.entries.len() == b.entries.len()
                    && a.entries.iter().zip(&b.entries).all(|((ka, va), (kb, vb))| ka == kb && same(va, vb))
            }
            _ => ours == theirs,
        }
    }

    fn sources() -> Vec<String> {
        let mut out: Vec<String> = [
            "pie title Pets\n \"Dogs\" : 386\n",
            "\r\n  pie\r\"a\": 1\r\n",
            "---\ntitle: From front matter\n---\npie\n \"A\" : 1",
            "---\ntitle: 12\nconfig:\n  pie:\n    textPosition: 0.5\n    donutHole: .2\n  themeVariables:\n    pie1: '#123456'\n    pie2: \"rgb(1, 2, 3)\"\n---\npie showData\n \"a\": 1",
            "  ---\n  title: indented\n  displayMode: compact\n  ---\n  pie\n",
            "---\ntitle: [a, b, {c: d}]\nconfig: {pie: {legendPosition: [left]}, \"2\": x, b: 1, \"1\": y}\n---\npie",
            "---\ntitle: >-\n  folded\n  text\n\n  more\nother: |+\n  kept\n\n---\npie",
            "---\ntitle: \"esc \\u00e9 \\x41 \\U0001F600 \\ud800 \\n\"\n---\npie",
            "---\ntitle: 'it''s'\nlist:\n  - a\n  - b: 1\n    c: 2\n  -\n    - nested\nnum: [1, 0x1f, 0o17, 0b101, .5, 1e3, -.inf, .nan, ~, true, False, +7, 1.]\n---\npie",
            "---\ntitle: a # comment\nplain: multi\n  line value\n\n  after blank\nkey with spaces: v\n\"quoted key\": v\n? odd\n---\npie",
            "---\ntitle: unclosed\npie",
            "---\n---\npie",
            "---\n\ttab: 1\n---\npie",
            "---\ntitle: \"bad \\q escape\"\n---\npie",
            "---\ntitle: \"bad \\uzzzz escape\"\n---\npie",
            "---\na: {b: [1, 2\n---\npie",
            "---\na: {b: 1,, c: 2}\n---\npie",
            "---\na: {b: 1 c: 2}\n---\npie",
            "---\na: 1\n  b: 2\n---\npie",
            "---\na:\n  - 1\n  b: 2\n---\npie",
            "---\n{title: flow, config: {pie: {donutHole: 0.3}}}\n---\npie",
            "---\n[1, 2]\n---\npie",
            "---\njust a scalar\n---\npie",
            "---\ntitle: x\n---\n---\ntitle: y\n---\npie",
            "---\ntitle: x\n--- \n\n\npie\n---\n",
            "---\n__proto__: {a: 1}\nconfig:\n  __proto__: {b: 1}\n  constructor: {c: 1}\n  ok: {prototype: 1, d: 1}\n---\npie",
            "%%{init: {\"pie\": {\"textPosition\": 0.2}, \"themeVariables\": {\"pie1\": \"red\"}}}%%\npie\n \"a\": 1",
            "%%{init: {'theme': 'dark', 'x': [1, {'y': null}], '10': 1, '9': 2, 'b': 1e2, 'a': -0.5}}%%\npie",
            "%%{initialize: {\"a\": \"\\ud83d\\ude00 \\ud800 \\n\"}}%%\npie",
            "%%{ init : {\"a\":1} }%%\n%%{wrap}%%\npie",
            "%%{init: {\"a\":1,}}%%\npie\n%%{init: [1]}%%\n%%{init: {\"b\": 01}}%%\n%%{init: {\"c\": 1}} trailing }%%",
            "pie %%{init: {\"a\": {\"b\": 1}}}%% title x %%{init: {\"a\": {\"c\": 2}}}%%",
            "%%{init foo}%%pie",
            "%%{init: foo bar}%%\npie",
            "%%{\n  init: {\n    \"a\": 1\n  }\n}%%\npie",
            "%%{init: {\"a\": 1}\npie title never closed",
            "%%{}%% pie",
            "%% a comment\npie\n  %% another\n\"a\": 1 %% trailing\n%%\n%%{not removed\n  \n\n  %% after blanks\n\"b\": 2",
            "\n\n  %% first\n%%second\npie",
            "pie title <span class=\"x\" id=\"y\">t</span> > <b a=\"1\"\n \"a < b\" : 1\n <i a=\"unclosed>",
            "pie title #35; #amp; #9731; #x; # ; #quot;x\n \"style: #fff; a;\" : 1\n \"classDef x: y #a;b;\": 2",
            "pie title style a: b #c; d; e\nclassDef: #x;",
            "hello there",
            "",
            "   \n  ",
            "\u{feff}pie\n\u{a0}\"a\": 1\u{2028}%% c\n\"b\":2",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        out.push(format!("%%{{init: {}1{}}}%%\npie", "[".repeat(40), "]".repeat(40)));
        out
    }

    #[test]
    fn preprocess_matches_pele() {
        let sources = sources();
        let theirs = oracle("preprocess", &sources);
        for (source, theirs) in sources.iter().zip(&theirs) {
            let JsValue::Object(theirs) = theirs else { panic!("the oracle answers an object") };
            match (preprocess(source, None), theirs.get("error")) {
                (Err(RenderError::Diagnostic(ours)), Some(JsValue::String(error))) => {
                    assert_eq!(format!("PeleError: {}", ours.message), error.to_display(), "error for {source:?}");
                }
                (Ok(ours), None) => {
                    assert_eq!(Some(&JsValue::String(ours.text.clone())), theirs.get("text"), "text of {source:?}");
                    let title = ours.title.clone().map_or(JsValue::Null, JsValue::String);
                    assert_eq!(Some(&title), theirs.get("title"), "title of {source:?}");
                    let config = JsValue::Object(ours.config.clone());
                    let wanted = theirs.get("config").expect("config");
                    assert!(same(&config, wanted), "config of {source:?}: ours {config:?}, pele {wanted:?}");
                    let parsed = JsValue::String(encode_entities(&ours.text, None).into());
                    assert_eq!(Some(&parsed), theirs.get("parsed"), "encoded entities of {source:?}");
                }
                (ours, error) => panic!("{source:?}: ours {:?}, pele error {error:?}", ours.map(|pre| pre.text)),
            }
        }
    }

    #[test]
    fn object_keys_keep_javascript_order() {
        let Ok(JsValue::Object(object)) = parse_json(&units(r#"{"b":1,"2":1,"a":1,"1":1,"01":1,"b":2}"#)) else {
            panic!("an object parses")
        };
        let keys: Vec<String> = object.entries.iter().map(|(key, _)| key.to_display()).collect();
        assert_eq!(keys, ["1", "2", "b", "a", "01"]);
        assert_eq!(object.get("b"), Some(&JsValue::Number(2.0)));
    }

    #[test]
    fn json_rejects_what_json_parse_rejects() {
        for bad in
            ["", "{", "[1,]", "{\"a\":1,}", "01", "1.", ".5", "+1", "'a'", "\"\t\"", "\"\\x41\"", "nul", "1 2", "{a:1}"]
        {
            assert!(parse_json(&units(bad)).is_err(), "{bad:?} is not JSON");
        }
        assert_eq!(
            parse_json(&units(" [1e2, -0.5, \"\\u00e9\"] ")).ok().map(|v| v.to_js_string().to_display()),
            Some("100,-0.5,é".to_owned())
        );
    }

    #[test]
    fn entities_decode_once_and_leave_unknown_codes() {
        let decoded = |text: &str| String::from_utf16_lossy(&decode_entities(&encode_entities(&units(text), None)));
        assert_eq!(decoded("B #35; c #amp; #quot;"), "B # c & \"");
        let html =
            String::from_utf16_lossy(&decode_entities(&units("&#x41;&#65;&lt;&nope;&#0;&#xd800;&#1114112;&#12abc;")));
        assert_eq!(html, "AA<&nope;&#0;&#xd800;&#1114112;&#12abc;");
        assert_eq!(decoded("#38;lt;"), "&lt;");
        assert_eq!(
            String::from_utf16_lossy(&written_codes(&encode_entities(&units("a #35; #amp; b"), None))),
            "a #35; #amp; b"
        );
    }
}
