//! Pele's xychart lexer and parser (`lexer.ts`, `parser.ts`, `tokens.ts`): Mermaid's Jison grammar
//! as a state machine that cuts the text into tokens, and a parser that fills the db from them.

use super::db::{DataPoint, XyChartDb};
use crate::diagrams::scan::word_at;
use crate::diagrams::tokens::SyntaxError;
use crate::input::{Units, is_digit, is_letter, is_space, is_word, string_to_number};

/// Pele's token types, in the order of `TOKEN_NAMES`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum T {
    End,
    Error,
    Newline,
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Xychart,
    ChartOrientation,
    XAxis,
    YAxis,
    ArrowDelimiter,
    Line,
    Bar,
    NumberWithDecimal,
    SquareBracesStart,
    SquareBracesEnd,
    Str,
    Alpha,
    Colon,
    Plus,
    Comma,
    Equals,
    Mult,
    Brkt,
    Underscore,
    Dot,
    Amp,
    Minus,
    Num,
    Semi,
    Eof,
}

const TOKEN_NAMES: [&str; 34] = [
    "$end",
    "INVALID",
    "NEWLINE",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "XYCHART",
    "CHART_ORIENTATION",
    "X_AXIS",
    "Y_AXIS",
    "ARROW_DELIMITER",
    "LINE",
    "BAR",
    "NUMBER_WITH_DECIMAL",
    "SQUARE_BRACES_START",
    "SQUARE_BRACES_END",
    "STR",
    "ALPHA",
    "COLON",
    "PLUS",
    "COMMA",
    "EQUALS",
    "MULT",
    "BRKT",
    "UNDERSCORE",
    "DOT",
    "AMP",
    "MINUS",
    "NUM",
    "SEMI",
    "EOF",
];

/// The lexer's start conditions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    AxisData,
    AxisBandData,
    Data,
    DataInner,
    String,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

impl S {
    /// The token each state gives for the empty rest of the input.
    fn at_end(self) -> T {
        match self {
            S::String => T::Str,
            S::AccTitle => T::AccTitleValue,
            S::AccDescr => T::AccDescrValue,
            S::AccDescrMultiline => T::AccDescrMultilineValue,
            _ => T::Eof,
        }
    }
}

/// Mermaid's grammar means to open a markdown string at `"` followed by a backtick, but its
/// quoting turns the rule into one that matches this text and leaves the current state. Markdown
/// strings are never read.
fn broken_rule() -> String {
    format!(
        "`){}{{ this.pushstate(md_string); }}\n<md_string>(?:(?!`\").)+{}{{ return md_str; }}\n<md_string>(?:`",
        " ".repeat(36),
        " ".repeat(18)
    )
}

fn single(c: u16) -> Option<T> {
    Some(match u8::try_from(c).ok()? {
        b':' => T::Colon,
        b'+' => T::Plus,
        b',' => T::Comma,
        b'=' => T::Equals,
        b'*' => T::Mult,
        b'#' => T::Brkt,
        b'_' => T::Underscore,
        b'.' => T::Dot,
        b'&' => T::Amp,
        b'-' => T::Minus,
        b';' => T::Semi,
        _ => return None,
    })
}

struct Token {
    kind: T,
    start: usize,
    end: usize,
}

/// Pele's `tokenize`.
fn tokenize(src: &[u16]) -> Vec<Token> {
    let n = src.len();
    let mut tokens = Vec::new();
    let mut stack: Vec<S> = Vec::new();
    let mut state = S::Initial;
    let mut p = 0;
    let broken: Vec<u16> = broken_rule().encode_utf16().collect();
    let line_end = |q: usize| src.find_unit(10, q).unwrap_or(n);
    let run = |q: usize| src.run(q, is_space);
    let digits = |q: usize| src.run(q, is_digit);
    let keyword = |p: usize, word: &str| word_at(src, p, word) && !src.at(p + word.len()).is_some_and(is_word);

    macro_rules! emit {
        ($kind:expr, $start:expr, $end:expr) => {{
            let (start, end) = ($start, $end);
            tokens.push(Token { kind: $kind, start, end });
            p = end;
        }};
    }
    macro_rules! push {
        ($next:expr) => {{
            stack.push(state);
            state = $next;
        }};
    }
    macro_rules! pop {
        () => {{
            if let Some(previous) = stack.pop() {
                state = previous;
            }
        }};
    }

    while p < n {
        let c = src[p];
        if state == S::String {
            if c == 34 {
                p += 1;
                pop!();
            } else {
                emit!(T::Str, p, src.find_unit(34, p).unwrap_or(n));
            }
            continue;
        }
        if state == S::AccTitle || state == S::AccDescr {
            emit!(state.at_end(), p, line_end(p));
            pop!();
            continue;
        }
        if state == S::AccDescrMultiline {
            if c == 125 {
                p += 1;
                pop!();
            } else {
                emit!(T::AccDescrMultilineValue, p, src.find_unit(125, p).unwrap_or(n));
            }
            continue;
        }

        let next = src.at(p + 1);
        if c == 37 && next == Some(37) {
            p = line_end(p + 2);
            continue;
        }
        if c == 10 || c == 13 {
            // Only a line break that directly follows an axis or a series ends it; one after
            // spaces is skipped with them, and the state carries on to the next line.
            if (state == S::AxisData || state == S::Data) && (c == 10 || next == Some(10)) {
                emit!(T::Newline, p, if c == 10 { p + 1 } else { p + 2 });
                pop!();
                continue;
            }
            let end = src.run(p + 1, |d| d == 10 || d == 13);
            emit!(T::Newline, p, end);
            continue;
        }

        match c | 32 {
            116 if keyword(p, "title") => {
                emit!(T::Title, p, p + 5);
                continue;
            }
            97 if word_at(src, p, "acc") => {
                let descr = word_at(src, p + 3, "descr");
                if descr || word_at(src, p + 3, "title") {
                    let k = run(p + 8);
                    let d = src.at(k);
                    if d == Some(58) {
                        emit!(if descr { T::AccDescr } else { T::AccTitle }, p, run(k + 1));
                        push!(if descr { S::AccDescr } else { S::AccTitle });
                        continue;
                    }
                    if d == Some(123) && descr {
                        p = run(k + 1);
                        push!(S::AccDescrMultiline);
                        continue;
                    }
                }
            }
            120 => {
                if keyword(p, "xychart-beta") {
                    emit!(T::Xychart, p, p + 12);
                    continue;
                }
                if keyword(p, "xychart") {
                    emit!(T::Xychart, p, p + 7);
                    continue;
                }
                if keyword(p, "x-axis") {
                    emit!(T::XAxis, p, p + 6);
                    push!(S::AxisData);
                    continue;
                }
            }
            121 if keyword(p, "y-axis") => {
                emit!(T::YAxis, p, p + 6);
                push!(S::AxisData);
                continue;
            }
            118 if word_at(src, p, "vertical") => {
                emit!(T::ChartOrientation, p, p + 8);
                continue;
            }
            104 if word_at(src, p, "horizontal") => {
                emit!(T::ChartOrientation, p, p + 10);
                continue;
            }
            108 if keyword(p, "line") => {
                emit!(T::Line, p, p + 4);
                push!(S::Data);
                continue;
            }
            98 if keyword(p, "bar") => {
                emit!(T::Bar, p, p + 3);
                push!(S::Data);
                continue;
            }
            _ => {}
        }

        if c == 91 {
            if state == S::AxisData {
                push!(S::AxisBandData);
            } else if state == S::Data {
                push!(S::DataInner);
            }
            emit!(T::SquareBracesStart, p, p + 1);
            continue;
        }
        if c == 93 {
            if state == S::DataInner || state == S::AxisBandData {
                pop!();
            }
            emit!(T::SquareBracesEnd, p, p + 1);
            continue;
        }
        if state == S::AxisData && c == 45 && next == Some(45) && src.at(p + 2) == Some(62) {
            emit!(T::ArrowDelimiter, p, p + 3);
            continue;
        }
        if state == S::AxisData || state == S::DataInner {
            let q = if c == 43 || c == 45 { p + 1 } else { p };
            let d = src.at(q);
            let mut end = None;
            if d.is_some_and(is_digit) {
                let mut e = digits(q + 1);
                if src.at(e) == Some(46) && src.at(e + 1).is_some_and(is_digit) {
                    e = digits(e + 2);
                }
                end = Some(e);
            } else if d == Some(46) && src.at(q + 1).is_some_and(is_digit) {
                end = Some(digits(q + 2));
            }
            if let Some(end) = end {
                emit!(T::NumberWithDecimal, p, end);
                continue;
            }
        }
        if c == 96 && word_at(src, p, &String::from_utf16_lossy(&broken)) {
            p += broken.len();
            pop!();
            continue;
        }
        if c == 34 {
            p += 1;
            push!(S::String);
            continue;
        }
        if is_letter(c) {
            emit!(T::Alpha, p, src.run(p + 1, is_letter));
            continue;
        }
        if is_digit(c) {
            emit!(T::Num, p, digits(p + 1));
            continue;
        }
        if is_space(c) {
            p = run(p + 1);
            continue;
        }
        if let Some(kind) = single(c) {
            emit!(kind, p, p + 1);
            continue;
        }
        break;
    }

    if p < n {
        tokens.push(Token { kind: T::Error, start: p, end: p });
        return tokens;
    }
    // Jison tries the rules of the current state once more against the empty rest of the input.
    tokens.push(Token { kind: state.at_end(), start: n, end: n });
    tokens.push(Token { kind: T::End, start: n, end: n });
    tokens
}

/// `AMP NUM ALPHA PLUS EQUALS MULT DOT BRKT MINUS UNDERSCORE`
fn is_word_token(kind: T) -> bool {
    matches!(
        kind,
        T::Amp | T::Num | T::Alpha | T::Plus | T::Equals | T::Mult | T::Dot | T::Brkt | T::Minus | T::Underscore
    )
}

fn ends_line(kind: T) -> bool {
    matches!(kind, T::Newline | T::Semi | T::Eof)
}

struct Parser<'a> {
    src: &'a [u16],
    tokens: Vec<Token>,
    i: usize,
}

impl Parser<'_> {
    fn kind(&self, i: usize) -> T {
        self.tokens.get(i).map_or(T::End, |token| token.kind)
    }

    fn text(&self, i: usize) -> &[u16] {
        let token = &self.tokens[i];
        &self.src[token.start..token.end]
    }

    fn fail(&self, expected: &str) -> SyntaxError {
        let kind = self.kind(self.i);
        let at = self.tokens.get(self.i).map_or(self.src.len(), |token| token.start);
        if kind == T::Error {
            return SyntaxError { at, detail: String::new(), lexical: true };
        }
        let detail = format!("Expecting {expected}, got '{}'", TOKEN_NAMES[kind as usize]);
        SyntaxError { at, detail, lexical: false }
    }

    fn expect(&mut self, kind: T) -> Result<Vec<u16>, SyntaxError> {
        if self.kind(self.i) != kind {
            return Err(self.fail(&format!("'{}'", TOKEN_NAMES[kind as usize])));
        }
        self.i += 1;
        Ok(self.text(self.i - 1).to_vec())
    }

    fn is_text(kind: T) -> bool {
        kind == T::Str || is_word_token(kind)
    }

    fn text_value(&mut self) -> Result<Vec<u16>, SyntaxError> {
        if !Self::is_text(self.kind(self.i)) {
            return Err(self.fail("'STR', 'ALPHA', 'NUM'"));
        }
        let mut text = self.text(self.i).to_vec();
        self.i += 1;
        if self.kind(self.i - 1) != T::Str {
            while is_word_token(self.kind(self.i)) {
                text.extend_from_slice(self.text(self.i));
                self.i += 1;
            }
        }
        Ok(text)
    }

    fn number(&mut self) -> Result<f64, SyntaxError> {
        Ok(string_to_number(&self.expect(T::NumberWithDecimal)?))
    }

    fn range(&mut self) -> Result<(f64, f64), SyntaxError> {
        let min = self.number()?;
        self.expect(T::ArrowDelimiter)?;
        Ok((min, self.number()?))
    }

    fn points(&mut self) -> Result<Vec<DataPoint>, SyntaxError> {
        let mut out = Vec::new();
        self.expect(T::SquareBracesStart)?;
        loop {
            let value = self.number()?;
            let label = if self.kind(self.i) == T::Str {
                self.i += 1;
                self.text(self.i - 1).to_vec()
            } else {
                Vec::new()
            };
            out.push(DataPoint { value, label });
            if self.kind(self.i) != T::Comma {
                break;
            }
            self.i += 1;
        }
        self.expect(T::SquareBracesEnd)?;
        Ok(out)
    }
}

/// Pele's `parseXyChart`.
pub(crate) fn parse_xychart(src: &[u16], db: &mut XyChartDb) -> Result<(), SyntaxError> {
    let mut p = Parser { src, tokens: tokenize(src), i: 0 };

    loop {
        let kind = p.kind(p.i);
        if ends_line(kind) {
            p.i += 1;
        } else if kind != T::Xychart {
            break;
        } else {
            p.i += 1;
            if p.kind(p.i) == T::ChartOrientation {
                db.set_orientation(p.text(p.i));
                p.i += 1;
            }
        }
    }

    while p.kind(p.i) != T::End {
        let t = p.kind(p.i);
        p.i += 1;
        match t {
            T::Title => {
                let title = p.text_value()?;
                db.title = Some(title.trim_js().into());
            }
            T::XAxis | T::YAxis => {
                let title = if Parser::is_text(p.kind(p.i)) { Some(p.text_value()?) } else { None };
                if t == T::XAxis && p.kind(p.i) == T::SquareBracesStart {
                    let mut categories = Vec::new();
                    loop {
                        p.i += 1;
                        categories.push(p.text_value()?);
                        if p.kind(p.i) != T::Comma {
                            break;
                        }
                    }
                    p.expect(T::SquareBracesEnd)?;
                    db.set_x_axis_band(&categories);
                } else if p.kind(p.i) == T::NumberWithDecimal || title.is_none() {
                    let (min, max) = p.range()?;
                    if t == T::XAxis {
                        db.set_x_axis_range(min, max);
                    } else {
                        db.set_y_axis_range(min, max);
                    }
                }
                let title = title.unwrap_or_default();
                if t == T::XAxis {
                    db.set_x_axis_title(&title);
                } else {
                    db.set_y_axis_title(&title);
                }
            }
            T::Line | T::Bar => {
                let title = if p.kind(p.i) == T::SquareBracesStart { Vec::new() } else { p.text_value()? };
                let data = p.points()?;
                db.add_plot(t == T::Line, &title, &data);
            }
            T::AccTitle => {
                let value = p.expect(T::AccTitleValue)?;
                db.set_acc_title(value.trim_js());
            }
            T::AccDescr => {
                let value = p.expect(T::AccDescrValue)?;
                db.set_acc_description(value.trim_js());
            }
            T::AccDescrMultilineValue => {
                let value = p.text(p.i - 1).to_vec();
                db.set_acc_description(value.trim_js());
            }
            _ => {
                p.i -= 1;
                return Err(p.fail(
                    "'title', 'X_AXIS', 'Y_AXIS', 'LINE', 'BAR', 'acc_title', 'acc_descr', 'acc_descr_multiline_value'",
                ));
            }
        }
        while ends_line(p.kind(p.i)) {
            p.i += 1;
        }
    }
    Ok(())
}
