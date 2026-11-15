//! The quadrant chart lexer, ported from Pele's `src/diagrams/quadrant/lexer.ts`: a state machine
//! over UTF-16 code units that gives the tokens Mermaid's `quadrant.jison` lexer gives.
//!
//! A code unit past the end of the text is [`PAST`] here, where JavaScript reads NaN. Both match
//! no test the lexer makes.

use crate::diagrams::scan::word_at;
use crate::input::{Units, is_digit, is_letter, is_line_end, is_space, is_word};

/// Pele's token types (`tokens.ts`), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum T {
    End,
    Error,
    Newline,
    Title,
    TitleValue,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    XAxis,
    YAxis,
    Delimiter,
    Quadrant1,
    Quadrant2,
    Quadrant3,
    Quadrant4,
    ClassDef,
    MdStr,
    Str,
    ClassName,
    PointStart,
    PointX,
    PointY,
    Quadrant,
    Alpha,
    UnicodeText,
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
    Space,
    Semi,
    Punctuation,
    Eof,
}

impl T {
    /// Pele's `TOKEN_NAMES`.
    pub(super) fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Error => "INVALID",
            T::Newline => "NEWLINE",
            T::Title => "title",
            T::TitleValue => "title_value",
            T::AccTitle => "acc_title",
            T::AccTitleValue => "acc_title_value",
            T::AccDescr => "acc_descr",
            T::AccDescrValue => "acc_descr_value",
            T::AccDescrMultilineValue => "acc_descr_multiline_value",
            T::XAxis => "X-AXIS",
            T::YAxis => "Y-AXIS",
            T::Delimiter => "AXIS-TEXT-DELIMITER",
            T::Quadrant1 => "QUADRANT_1",
            T::Quadrant2 => "QUADRANT_2",
            T::Quadrant3 => "QUADRANT_3",
            T::Quadrant4 => "QUADRANT_4",
            T::ClassDef => "CLASSDEF",
            T::MdStr => "MD_STR",
            T::Str => "STR",
            T::ClassName => "class_name",
            T::PointStart => "point_start",
            T::PointX => "point_x",
            T::PointY => "point_y",
            T::Quadrant => "QUADRANT",
            T::Alpha => "ALPHA",
            T::UnicodeText => "UNICODE_TEXT",
            T::Colon => "COLON",
            T::Plus => "PLUS",
            T::Comma => "COMMA",
            T::Equals => "EQUALS",
            T::Mult => "MULT",
            T::Brkt => "BRKT",
            T::Underscore => "UNDERSCORE",
            T::Dot => "DOT",
            T::Amp => "AMP",
            T::Minus => "MINUS",
            T::Num => "NUM",
            T::Space => "SPACE",
            T::Semi => "SEMI",
            T::Punctuation => "PUNCTUATION",
            T::Eof => "EOF",
        }
    }
}

/// Pele's lexer states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    Initial,
    Title,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    String,
    MdString,
    ClassName,
    PointStart,
    PointX,
    PointY,
}

impl S {
    /// The token the state gives for the empty rest of the input, where it gives one.
    fn at_end(self) -> Option<T> {
        match self {
            S::Initial => Some(T::Eof),
            S::Title => Some(T::TitleValue),
            S::AccTitle => Some(T::AccTitleValue),
            S::AccDescr => Some(T::AccDescrValue),
            S::AccDescrMultiline => Some(T::AccDescrMultilineValue),
            S::String => Some(T::Str),
            _ => None,
        }
    }
}

/// The token of a code unit that is a token on its own.
fn single(c: u32) -> Option<T> {
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
        b' ' | b'\t' | 0x0b | 0x0c => T::Space,
        b';' => T::Semi,
        b'!' | b'$' | b'%' | b'\'' | b'`' | b'?' | b'\\' | b'/' => T::Punctuation,
        _ => return None,
    })
}

/// A token: its type, where it starts and where it ends.
#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) kind: T,
    pub(super) start: usize,
    pub(super) end: usize,
}

const PAST: u32 = 0x1_0000;

/// A run of one kind of code unit that was scanned last, so that a long run is not scanned again
/// from each of its characters.
#[derive(Default)]
struct Run {
    from: usize,
    to: usize,
}

struct Lexer<'a> {
    src: &'a [u16],
    tokens: Vec<Token>,
    stack: Vec<S>,
    state: S,
    p: usize,
}

impl Lexer<'_> {
    fn c(&self, at: usize) -> u32 {
        self.src.get(at).map_or(PAST, |&c| u32::from(c))
    }

    fn is(&self, at: usize, class: fn(u16) -> bool) -> bool {
        self.src.get(at).is_some_and(|&c| class(c))
    }

    fn emit(&mut self, kind: T, start: usize, end: usize) {
        self.tokens.push(Token { kind, start, end });
        self.p = end;
    }

    fn push(&mut self, state: S) {
        self.stack.push(self.state);
        self.state = state;
    }

    fn pop(&mut self) {
        if let Some(state) = self.stack.pop() {
            self.state = state;
        }
    }

    fn line_end(&self, q: usize) -> usize {
        self.src.find_unit(10, q).unwrap_or(self.src.len())
    }

    fn spaces(&self, q: usize) -> usize {
        self.src.run(q, |c| c == 32)
    }

    fn run(&self, q: usize) -> usize {
        self.src.run(q, is_space)
    }

    /// Mermaid's `(1)|(0(.\d+)?)`, where the dot is any character within a line.
    fn coordinate(&self, q: usize) -> Option<usize> {
        let c = self.c(q);
        if c == 49 {
            return Some(q + 1);
        }
        if c != 48 {
            return None;
        }
        let mut e = q + 1;
        if e < self.src.len() && !self.is(e, is_line_end) && self.is(e + 1, is_digit) {
            e = self.src.run(e + 2, is_digit);
        }
        Some(e)
    }
}

/// Pele's `tokenize`. The list ends in an `END`, or in an `ERROR` where no rule matches.
pub(super) fn tokenize(src: &[u16]) -> Vec<Token> {
    let n = src.len();
    let mut lx = Lexer { src, tokens: Vec::new(), stack: Vec::new(), state: S::Initial, p: 0 };
    let (mut space, mut dash, mut ws, mut gap) = (Run::default(), Run::default(), Run::default(), Run::default());

    'lexing: while lx.p < n {
        let p = lx.p;
        let c = lx.c(p);
        let next = lx.c(p + 1);

        match lx.state {
            S::Initial => {}
            S::Title | S::AccTitle | S::AccDescr => {
                let kind = lx.state.at_end().expect("these states give a value");
                lx.emit(kind, p, lx.line_end(p));
                lx.pop();
                continue;
            }
            S::AccDescrMultiline => {
                if c == 125 {
                    lx.p += 1;
                    lx.pop();
                } else {
                    let e = src.find_unit(125, p).unwrap_or(n);
                    lx.emit(T::AccDescrMultilineValue, p, e);
                }
                continue;
            }
            S::String => {
                if c == 34 {
                    lx.p += 1;
                    lx.pop();
                } else {
                    let e = src.find_unit(34, p).unwrap_or(n);
                    lx.emit(T::Str, p, e);
                }
                continue;
            }
            S::MdString => {
                if c == 96 && next == 34 {
                    lx.p += 2;
                    lx.pop();
                    continue;
                }
                if c != 96 && c != 34 {
                    let e = src.run(p + 1, |d| d != 96 && d != 34);
                    lx.emit(T::MdStr, p, e);
                    continue;
                }
                break 'lexing;
            }
            S::ClassName => {
                if lx.is(p, is_word) {
                    let e = src.run(p + 1, is_word);
                    lx.emit(T::ClassName, p, e);
                    lx.pop();
                    continue;
                }
                break 'lexing;
            }
            S::PointStart => {
                if let Some(e) = lx.coordinate(p) {
                    lx.emit(T::PointX, p, e);
                    lx.push(S::PointX);
                    continue;
                }
                let k = lx.run(p);
                if lx.c(k) == 93 {
                    lx.p = lx.spaces(k + 1);
                    lx.pop();
                    continue;
                }
                break 'lexing;
            }
            S::PointX => {
                let k = lx.run(p);
                if lx.c(k) == 44 {
                    lx.p = lx.run(k + 1);
                    lx.state = S::PointY;
                    continue;
                }
                break 'lexing;
            }
            S::PointY => {
                if let Some(e) = lx.coordinate(p) {
                    lx.emit(T::PointY, p, e);
                    lx.pop();
                    continue;
                }
                break 'lexing;
            }
        }

        if c == 37 && next == 37 {
            lx.p = lx.line_end(p + 2);
            continue;
        }
        if c == 10 || c == 13 {
            let e = src.run(p + 1, |d| d == 10 || d == 13);
            lx.emit(T::Newline, p, e);
            continue;
        }

        let lower = c | 32;
        if lower == 116 && word_at(src, p, "title") && !lx.is(p + 5, is_word) {
            lx.emit(T::Title, p, p + 5);
            lx.push(S::Title);
            continue;
        }
        if lower == 97 && word_at(src, p, "acc") {
            let descr = word_at(src, p + 3, "descr");
            if descr || word_at(src, p + 3, "title") {
                let k = lx.run(p + 8);
                let d = lx.c(k);
                if d == 58 {
                    let end = lx.run(k + 1);
                    lx.emit(if descr { T::AccDescr } else { T::AccTitle }, p, end);
                    lx.push(if descr { S::AccDescr } else { S::AccTitle });
                    continue;
                }
                if d == 123 && descr {
                    lx.p = lx.run(k + 1);
                    lx.push(S::AccDescrMultiline);
                    continue;
                }
            }
        }

        // Axis, quadrant and chart keywords and the axis arrow take the spaces around them.
        let mut j = p;
        if c == 32 {
            if p < space.from || p >= space.to {
                space = Run { from: p, to: lx.spaces(p) };
            }
            j = space.to;
        }
        let d = lx.c(j);
        let dl = d | 32;
        let mut found = None;
        if dl == 120 && word_at(src, j, "x-axis") {
            found = Some((T::XAxis, j + 6));
        } else if dl == 121 && word_at(src, j, "y-axis") {
            found = Some((T::YAxis, j + 6));
        } else if d == 45 {
            if j < dash.from || j >= dash.to {
                dash = Run { from: j, to: src.run(j, |c| c == 45) };
            }
            if dash.to - j >= 2 && lx.c(dash.to) == 62 {
                found = Some((T::Delimiter, dash.to + 1));
            }
        } else if dl == 113 && word_at(src, j, "quadrant") {
            let q = lx.c(j + 9);
            if lx.c(j + 8) == 45 && (49..=52).contains(&q) {
                let kind = [T::Quadrant1, T::Quadrant2, T::Quadrant3, T::Quadrant4][(q - 49) as usize];
                found = Some((kind, j + 10));
            } else if word_at(src, j + 8, "chart") {
                found = Some((T::Quadrant, j + 13));
            }
        }
        if let Some((kind, end)) = found {
            let end = lx.spaces(end);
            lx.emit(kind, p, end);
            continue;
        }

        if lower == 99 && word_at(src, p, "classdef") && !lx.is(p + 8, is_word) {
            lx.emit(T::ClassDef, p, p + 8);
            continue;
        }
        if c == 34 {
            if next == 96 {
                lx.p += 2;
                lx.push(S::MdString);
            } else {
                lx.p += 1;
                lx.push(S::String);
            }
            continue;
        }
        if c == 58 && next == 58 && lx.c(p + 2) == 58 {
            lx.p += 3;
            lx.push(S::ClassName);
            continue;
        }
        // A point's `: [`, with any whitespace before, between and after, line breaks included.
        if c == 58 || lx.is(p, is_space) {
            let mut k = p;
            if c != 58 {
                if p < ws.from || p >= ws.to {
                    ws = Run { from: p, to: lx.run(p) };
                }
                k = ws.to;
            }
            if lx.c(k) == 58 {
                if k + 1 < gap.from || k + 1 >= gap.to {
                    gap = Run { from: k + 1, to: lx.run(k + 1) };
                }
                if lx.c(gap.to) == 91 {
                    let end = lx.run(gap.to + 1);
                    lx.emit(T::PointStart, p, end);
                    lx.push(S::PointStart);
                    continue;
                }
            }
        }

        if lx.is(p, is_letter) {
            let e = src.run(p + 1, is_letter);
            lx.emit(T::Alpha, p, e);
            continue;
        }
        if c >= 128 {
            let e = src.run(p + 1, |c| c >= 128);
            lx.emit(T::UnicodeText, p, e);
            continue;
        }
        if lx.is(p, is_digit) {
            let e = src.run(p + 1, is_digit);
            lx.emit(T::Num, p, e);
            continue;
        }
        if let Some(kind) = single(c) {
            lx.emit(kind, p, p + 1);
            continue;
        }
        break;
    }

    let mut tokens = lx.tokens;
    if lx.p < n {
        tokens.push(Token { kind: T::Error, start: lx.p, end: lx.p });
        return tokens;
    }
    // Jison tries the rules of the current state once more against the empty rest of the input.
    if let Some(kind) = lx.state.at_end() {
        tokens.push(Token { kind, start: n, end: n });
    }
    tokens.push(Token { kind: T::End, start: n, end: n });
    tokens
}
