//! The body of a `json` node, ported from Pele's `src/diagrams/usecase/json.ts`.
//!
//! Pele reads it with `JSON.parse` and puts the engine's own message into its error. That
//! message is part of Pele's output, so [`parse`] follows V8's `JsonParser` (`src/json/
//! json-parser.cc` at V8 14.6, the engine of the pinned Node): the same value, and for text it
//! rejects, the same message. [`walk`] is Pele's own second reading, which finds where the text
//! is at fault when the engine's message does not say.

use crate::input::{JsText, Units, is_digit, written_codes};
use crate::js;
use indexmap::IndexMap;

/// One value of a JSON text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    Null,
    Bool(bool),
    Number(f64),
    String(JsText),
    /// The places of the items in [`Json::nodes`].
    Array(Vec<usize>),
    /// Each key with the place of its value. A key written twice keeps its first position and
    /// takes its last value, which is what `JSON.parse` and Pele's `propertyOrder` together say.
    Object(IndexMap<JsText, usize>),
}

/// A parsed JSON text. Values sit in one list and refer to each other by position, so a text
/// nested tens of thousands deep is neither read nor dropped by recursion.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Json {
    nodes: Vec<Node>,
    root: usize,
}

impl Json {
    pub(crate) fn root(&self) -> usize {
        self.root
    }

    pub(crate) fn node(&self, at: usize) -> &Node {
        &self.nodes[at]
    }
}

/// What is open while a value inside it is read.
enum Frame {
    Array(Vec<usize>),
    Object { entries: IndexMap<JsText, usize>, key: JsText },
}

struct Parser<'a> {
    text: &'a [u16],
    cursor: usize,
    nodes: Vec<Node>,
}

type Parsed<T> = Result<T, JsText>;

fn message(text: &str) -> JsText {
    text.into()
}

impl Parser<'_> {
    fn current(&self) -> Option<u16> {
        self.text.at(self.cursor)
    }

    fn is(&self, byte: u8) -> bool {
        self.current() == Some(u16::from(byte))
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.current(), Some(32 | 9 | 13 | 10)) {
            self.cursor += 1;
        }
    }

    /// V8's `Check`: takes `byte` where it is the next character that is not whitespace.
    fn check(&mut self, byte: u8) -> bool {
        self.skip_whitespace();
        if self.is(byte) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// V8's `CalculateFileLocation`, for a message that names the place of the fault.
    fn positioned(&self, what: &str) -> JsText {
        let mut line = 1;
        let mut last_break = 0;
        let end = self.cursor.min(self.text.len());
        let mut at = 0;
        while at < end {
            if self.text[at] == 13 && at + 1 < end && self.text[at + 1] == 10 {
                at += 1;
            }
            if self.text[at] == 13 || self.text[at] == 10 {
                line += 1;
                last_break = at + 1;
            }
            at += 1;
        }
        let column = 1 + end - last_break;
        message(&format!("{what} in JSON at position {} (line {line} column {column})", self.cursor))
    }

    /// V8's `ReportUnexpectedCharacter`: the message for a character that fits nowhere, or for
    /// the end of the text.
    fn unexpected(&self) -> JsText {
        let Some(c) = self.current() else {
            return message("Unexpected end of JSON input");
        };
        if c == u16::from(b'"') {
            return self.positioned("Unexpected string");
        }
        if is_digit(c) || c == u16::from(b'-') {
            return self.positioned("Unexpected number");
        }
        // V8 shows the text around the character once the text is long enough to need cutting.
        const CONTEXT: usize = 10;
        let (pos, length) = (self.cursor, self.text.len());
        let (lead, from, to, trail) = if length < 2 * CONTEXT + 1 {
            ("", 0, length, "")
        } else if pos < CONTEXT {
            ("", 0, pos + CONTEXT, "...")
        } else if pos < length - CONTEXT {
            ("...", pos - CONTEXT, pos + CONTEXT, "...")
        } else {
            ("...", pos - CONTEXT, length, "")
        };
        let mut out = message("Unexpected token '");
        out.push(c);
        out.push_str(&format!("', {lead}\""));
        out.push_units(&self.text[from..to]);
        out.push_str(&format!("\"{trail} is not valid JSON"));
        out
    }

    fn add(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// V8's `ScanLiteral`, on the first letter of `word`.
    fn literal(&mut self, word: &str) -> Parsed<()> {
        for expected in word.encode_utf16().skip(1) {
            self.cursor += 1;
            if self.current() != Some(expected) {
                return Err(self.unexpected());
            }
        }
        self.cursor += 1;
        Ok(())
    }

    /// V8's `ParseJsonNumberAsDoubleOrSmi`.
    fn number(&mut self) -> Parsed<f64> {
        let start = self.cursor;
        let digits = |parser: &mut Self| {
            while parser.current().is_some_and(is_digit) {
                parser.cursor += 1;
            }
        };
        if self.is(b'-') {
            self.cursor += 1;
        }
        if self.is(b'0') {
            self.cursor += 1;
            if self.current().is_some_and(is_digit) {
                return Err(self.positioned("Unexpected number"));
            }
        } else {
            let from = self.cursor;
            digits(self);
            if self.cursor == from {
                return Err(self.positioned("No number after minus sign"));
            }
        }
        if self.is(b'.') {
            self.cursor += 1;
            if !self.current().is_some_and(is_digit) {
                return Err(self.positioned("Unterminated fractional number"));
            }
            digits(self);
        }
        if self.is(b'e') || self.is(b'E') {
            self.cursor += 1;
            if self.is(b'-') || self.is(b'+') {
                self.cursor += 1;
            }
            if !self.current().is_some_and(is_digit) {
                return Err(self.positioned("Exponent part is missing a number"));
            }
            digits(self);
        }
        let spelled: String = self.text[start..self.cursor].iter().map(|&c| char::from(c as u8)).collect();
        Ok(spelled.parse().expect("a JSON number is a decimal number"))
    }

    /// V8's `ScanJsonString`, after the opening quote.
    fn string(&mut self) -> Parsed<JsText> {
        let mut out = JsText::default();
        loop {
            let Some(c) = self.current() else {
                return Err(self.positioned("Unterminated string"));
            };
            if c == u16::from(b'"') {
                self.cursor += 1;
                return Ok(out);
            }
            if c < 0x20 {
                return Err(self.positioned("Bad control character in string literal"));
            }
            self.cursor += 1;
            if c != u16::from(b'\\') {
                out.push(c);
                continue;
            }
            let decoded = match self.current().and_then(|c| u8::try_from(c).ok()) {
                Some(c @ (b'"' | b'\\' | b'/')) => u16::from(c),
                Some(b'b') => 8,
                Some(b't') => 9,
                Some(b'n') => 10,
                Some(b'f') => 12,
                Some(b'r') => 13,
                Some(b'u') => {
                    let mut value = 0u16;
                    for _ in 0..4 {
                        self.cursor += 1;
                        let Some(digit) = self.current().and_then(crate::input::hex_digit) else {
                            return Err(self.positioned("Bad Unicode escape"));
                        };
                        value = value * 16 + digit as u16;
                    }
                    value
                }
                Some(_) => return Err(self.positioned("Bad escaped character")),
                // The end of the text, or a character past Latin-1.
                None => return Err(self.unexpected()),
            };
            out.push(decoded);
            self.cursor += 1;
        }
    }

    /// Takes the quote that opens a property name, or fails with `what`.
    fn key(&mut self, what: &str) -> Parsed<JsText> {
        if !self.check(b'"') {
            return Err(self.positioned(what));
        }
        let key = self.string()?;
        if !self.check(b':') {
            return Err(self.positioned("Expected ':' after property name"));
        }
        Ok(key)
    }

    /// V8's `ParseJsonValue`, with what is open kept on a list instead of the call stack.
    fn value(&mut self) -> Parsed<usize> {
        let mut open: Vec<Frame> = Vec::new();
        loop {
            self.skip_whitespace();
            let node = match self.current().and_then(|c| u8::try_from(c).ok()) {
                Some(b'-' | b'0'..=b'9') => Node::Number(self.number()?),
                Some(b'"') => {
                    self.cursor += 1;
                    Node::String(self.string()?)
                }
                Some(b't') => {
                    self.literal("true")?;
                    Node::Bool(true)
                }
                Some(b'f') => {
                    self.literal("false")?;
                    Node::Bool(false)
                }
                Some(b'n') => {
                    self.literal("null")?;
                    Node::Null
                }
                Some(b'{') => {
                    self.cursor += 1;
                    if self.check(b'}') {
                        Node::Object(IndexMap::new())
                    } else {
                        let key = self.key("Expected property name or '}'")?;
                        open.push(Frame::Object { entries: IndexMap::new(), key });
                        continue;
                    }
                }
                Some(b'[') => {
                    self.cursor += 1;
                    if self.check(b']') {
                        Node::Array(Vec::new())
                    } else {
                        open.push(Frame::Array(Vec::new()));
                        continue;
                    }
                }
                _ => return Err(self.unexpected()),
            };
            let mut done = self.add(node);
            // Hands the finished value to what holds it, and closes each holder that ends here.
            loop {
                let Some(frame) = open.last_mut() else {
                    return Ok(done);
                };
                match frame {
                    Frame::Array(items) => {
                        items.push(done);
                        if self.check(b',') {
                            break;
                        }
                        if !self.is(b']') {
                            return Err(self.positioned("Expected ',' or ']' after array element"));
                        }
                    }
                    Frame::Object { entries, key } => {
                        entries.insert(std::mem::take(key), done);
                        if self.check(b',') {
                            let next = self.key("Expected double-quoted property name")?;
                            let Some(Frame::Object { key, .. }) = open.last_mut() else {
                                unreachable!("the frame is the object just matched");
                            };
                            *key = next;
                            break;
                        }
                        if !self.is(b'}') {
                            return Err(self.positioned("Expected ',' or '}' after property value"));
                        }
                    }
                }
                self.cursor += 1;
                done = match open.pop().expect("a frame is open") {
                    Frame::Array(items) => self.add(Node::Array(items)),
                    Frame::Object { entries, .. } => self.add(Node::Object(entries)),
                };
            }
        }
    }
}

/// `JSON.parse` in V8: the value, or the message of the `SyntaxError` it throws.
pub(crate) fn parse(text: &[u16]) -> Result<Json, JsText> {
    let mut parser = Parser { text, cursor: 0, nodes: Vec::new() };
    let root = parser.value()?;
    parser.skip_whitespace();
    if parser.cursor < text.len() {
        return Err(parser.positioned("Unexpected non-whitespace character after JSON"));
    }
    Ok(Json { nodes: parser.nodes, root })
}

/// Pele's `propertyOrder`, as far as it finds fault: the offset of the first character that is
/// not valid JSON, or nothing for text that is.
fn walk(text: &[u16]) -> Option<usize> {
    #[derive(PartialEq)]
    enum State {
        Value,
        Key,
        After,
    }
    let n = text.len();
    let is = |at: usize, byte: u8| text.at(at) == Some(u16::from(byte));
    let digit = |at: usize| text.at(at).is_some_and(is_digit);
    let skip = |at: &mut usize| {
        while matches!(text.at(*at), Some(32 | 9 | 10 | 13)) {
            *at += 1;
        }
    };
    let read_string = |at: &mut usize| -> Result<(), usize> {
        *at += 1;
        while *at < n {
            let start = *at;
            let ch = text[*at];
            *at += 1;
            if ch == 34 {
                return Ok(());
            }
            if ch < 0x20 {
                return Err(start);
            }
            if ch != 92 {
                continue;
            }
            let escape_at = *at;
            let escape = text.at(*at).and_then(|c| u8::try_from(c).ok());
            *at += 1;
            match escape {
                Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {}
                Some(b'u') => {
                    let hex = (0..4).all(|k| text.at(*at + k).is_some_and(|c| crate::input::hex_digit(c).is_some()));
                    if !hex {
                        return Err(*at);
                    }
                    *at += 4;
                }
                _ => return Err(escape_at),
            }
        }
        Err(*at)
    };
    let literal = |at: &mut usize, word: &str| -> Result<(), usize> {
        for (k, expected) in word.encode_utf16().enumerate() {
            if text.at(*at + k) != Some(expected) {
                return Err(*at + k);
            }
        }
        *at += word.len();
        Ok(())
    };
    let number = |at: &mut usize| -> Result<(), usize> {
        let digits = |at: &mut usize| {
            while digit(*at) {
                *at += 1;
            }
        };
        if is(*at, b'-') {
            *at += 1;
        }
        if is(*at, b'0') {
            *at += 1;
        } else if digit(*at) {
            digits(at);
        } else {
            return Err(*at);
        }
        if is(*at, b'.') {
            *at += 1;
            if !digit(*at) {
                return Err(*at);
            }
            digits(at);
        }
        if is(*at, b'e') || is(*at, b'E') {
            *at += 1;
            if is(*at, b'+') || is(*at, b'-') {
                *at += 1;
            }
            if !digit(*at) {
                return Err(*at);
            }
            digits(at);
        }
        Ok(())
    };

    let run = || -> Result<(), usize> {
        // Whether each open container is an object.
        let mut stack: Vec<bool> = Vec::new();
        let mut at = 0;
        let mut state = State::Value;
        skip(&mut at);
        loop {
            match state {
                State::Value => {
                    skip(&mut at);
                    state = State::After;
                    match text.at(at).and_then(|c| u8::try_from(c).ok()) {
                        Some(b'{') => {
                            at += 1;
                            skip(&mut at);
                            if is(at, b'}') {
                                at += 1;
                            } else {
                                stack.push(true);
                                state = State::Key;
                            }
                        }
                        Some(b'[') => {
                            at += 1;
                            skip(&mut at);
                            if is(at, b']') {
                                at += 1;
                            } else {
                                stack.push(false);
                                state = State::Value;
                            }
                        }
                        Some(b'"') => read_string(&mut at)?,
                        Some(b't') => literal(&mut at, "true")?,
                        Some(b'f') => literal(&mut at, "false")?,
                        Some(b'n') => literal(&mut at, "null")?,
                        Some(b'-' | b'0'..=b'9') => number(&mut at)?,
                        _ => return Err(at),
                    }
                }
                State::Key => {
                    if !is(at, b'"') {
                        return Err(at);
                    }
                    read_string(&mut at)?;
                    skip(&mut at);
                    if !is(at, b':') {
                        return Err(at);
                    }
                    at += 1;
                    state = State::Value;
                }
                State::After => {
                    let Some(&object) = stack.last() else {
                        break;
                    };
                    skip(&mut at);
                    if is(at, if object { b'}' } else { b']' }) {
                        at += 1;
                        stack.pop();
                    } else if !is(at, b',') {
                        return Err(at);
                    } else {
                        at += 1;
                        skip(&mut at);
                        state = if object { State::Key } else { State::Value };
                    }
                }
            }
        }
        skip(&mut at);
        if at == n { Ok(()) } else { Err(at) }
    };
    run().err()
}

/// Pele's `locationAt`: the line and column `offset` characters into a text that starts at
/// `line` and `column`.
fn location_at(text: &[u16], offset: usize, mut line: f64, mut column: f64) -> (f64, f64) {
    for i in 0..offset.min(text.len()) {
        match text[i] {
            13 => {
                line += 1.0;
                column = 1.0;
            }
            10 => {
                if i == 0 || text[i - 1] != 13 {
                    line += 1.0;
                    column = 1.0;
                }
            }
            _ => column += 1.0,
        }
    }
    (line, column)
}

fn word_at(text: &[u16], at: usize) -> bool {
    text.at(at).is_some_and(crate::input::is_word)
}

/// A run of digits at `at` as `parseInt` reads it, and where it ends.
fn digits_at(text: &[u16], at: usize) -> Option<(f64, usize)> {
    let end = text.run(at, is_digit);
    let spelled: String = text[at..end].iter().map(|&c| char::from(c as u8)).collect();
    (end > at).then(|| (spelled.parse().expect("digits are a number"), end))
}

/// Every place `word` starts in `text` at a word boundary.
fn bounded<'a>(text: &'a [u16], word: &'a str) -> impl Iterator<Item = usize> + 'a {
    (0..text.len())
        .filter(move |&at| text.starts_at(at, word) && !(at > 0 && word_at(text, at - 1)))
        .map(move |at| at + word.len())
}

/// Pele's `locationFromMessage`. Engines word `JSON.parse` errors differently: some give an
/// offset, some a line and column. The message can quote the text itself, and what it quotes is
/// read the same way.
fn location_from_message(message: &[u16], text: &[u16], line: f64, column: f64) -> Option<(f64, f64)> {
    // /\bposition (\d+)\b/
    for after in bounded(message, "position ") {
        if let Some((position, end)) = digits_at(message, after)
            && !word_at(message, end)
        {
            // A position past what an offset can hold is past the end of the text as well.
            return Some(location_at(text, position as usize, line, column));
        }
    }
    // /\bline (\d+) column (\d+)\b/
    for after in bounded(message, "line ") {
        let Some((local_line, end)) = digits_at(message, after) else {
            continue;
        };
        if !message.starts_at(end, " column ") {
            continue;
        }
        if let Some((local_column, end)) = digits_at(message, end + " column ".len())
            && !word_at(message, end)
        {
            let column = if local_line == 1.0 { column + local_column - 1.0 } else { local_column };
            return Some((line + local_line - 1.0, column));
        }
    }
    // /unexpected end|end of json input/i
    let lower: Vec<u16> = message.iter().map(|&c| if (65..=90).contains(&c) { c + 32 } else { c }).collect();
    if lower.has("unexpected end") || lower.has("end of json input") {
        return Some(location_at(text, text.len(), line, column));
    }
    None
}

/// Why a `json` node's body was rejected: Pele's message without its position, and the line and
/// column of the fault as JavaScript numbers.
#[derive(Debug, PartialEq)]
pub(crate) struct JsonFault {
    pub(crate) message: JsText,
    pub(crate) line: f64,
    pub(crate) column: f64,
}

impl JsonFault {
    /// Pele's `jsonError` message.
    pub(crate) fn worded(&self) -> JsText {
        let mut out = self.message.clone();
        out.push_str(&format!(" (line {}, column {})", js::to_string(self.line), js::to_string(self.column)));
        out
    }
}

/// Pele's `parseOrderedJson`: the body of a `json` node. `origin` gives the line and column the
/// text starts at in the diagram. The text is one the lexer took from `{` to its matching `}`,
/// so a value that parses is an object, and Pele's "must have an object root" error cannot
/// arise.
pub(crate) fn parse_ordered_json(text: &[u16], origin: impl FnOnce() -> (usize, usize)) -> Result<Json, JsonFault> {
    let first = match parse(text) {
        Ok(value) => return Ok(value),
        Err(message) => message,
    };
    // Entity codes are shown as written, so that the message and the position match the source.
    let shown = written_codes(text);
    let failure = if shown == text { first } else { parse(&shown).err().unwrap_or(first) };
    let (line, column) = origin();
    let (line, column) = (line as f64, column as f64);
    let (line, column) = location_from_message(&failure, &shown, line, column)
        .unwrap_or_else(|| location_at(&shown, walk(&shown).unwrap_or(0), line, column));
    let mut message = message("Invalid JSON: ");
    message.push_units(&failure);
    Err(JsonFault { message, line, column })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn rejected(text: &str) -> String {
        parse(&units(text)).expect_err("the text is not JSON").to_display()
    }

    #[test]
    fn a_repeated_key_keeps_its_first_place_and_its_last_value() {
        let json = parse(&units(r#"{"b":1,"a":{"x":[]},"b":"two"}"#)).expect("JSON");
        let Node::Object(entries) = json.node(json.root()) else {
            panic!("the root is an object");
        };
        let keys: Vec<String> = entries.keys().map(|key| key.to_display()).collect();
        assert_eq!(keys, ["b", "a"]);
        assert_eq!(json.node(entries[0]), &Node::String("two".into()));
    }

    #[test]
    fn deep_nesting_needs_no_stack() {
        let deep = format!("{{\"a\":{}{}}}", "[".repeat(20_000), "]".repeat(20_000));
        assert!(parse(&units(&deep)).is_ok());
        assert_eq!(walk(&units(&deep)), None);
        let broken = format!("{{\"a\":{}", "[".repeat(20_000));
        assert_eq!(rejected(&broken), "Unexpected end of JSON input");
    }

    #[test]
    fn a_fault_the_engine_does_not_place_is_found_by_the_walk() {
        let fault = parse_ordered_json(&units("{\"a\":\n tru }"), || (3, 9)).expect_err("not JSON");
        assert_eq!(
            fault.worded().to_display(),
            "Invalid JSON: Unexpected token ' ', \"{\"a\":\n tru }\" is not valid JSON (line 4, column 5)"
        );
    }
}
