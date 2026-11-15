//! Pele's sankey lexer (`src/diagrams/sankey/lexer.ts`): the states and tokens of Mermaid's
//! `sankey.jison`, with RFC 4180's text extended to Unicode.

use crate::input::{Units, is_word, to_lowercase};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum T {
    End,
    /// Not a Jison token: the reference lexer would return the same empty token forever from here.
    Stuck,
    Sankey,
    Newline,
    Comma,
    Dquote,
    NonEscapedText,
    EscapedText,
    Eof,
}

impl T {
    pub(super) fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Stuck => "STUCK",
            T::Sankey => "SANKEY",
            T::Newline => "NEWLINE",
            T::Comma => "COMMA",
            T::Dquote => "DQUOTE",
            T::NonEscapedText => "NON_ESCAPED_TEXT",
            T::EscapedText => "ESCAPED_TEXT",
            T::Eof => "EOF",
        }
    }
}

pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    /// Where each token starts and ends.
    pub(super) spans: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Initial,
    Csv,
    EscapedText,
}

/// RFC 4180 TEXTDATA extended to Unicode, excluding controls and invalid XML characters.
fn is_text_data(c: u32) -> bool {
    (32..=126).contains(&c) && c != 34 && c != 44
        || (0xa0..=0xd7ff).contains(&c)
        || (0xe000..=0xfffd).contains(&c)
        || (0x10000..=0x10ffff).contains(&c)
}

/// `codePointAt`: a surrogate pair as one code point, anything else as its unit.
fn code_point(src: &[u16], at: usize) -> u32 {
    let unit = u32::from(src[at]);
    match src.at(at + 1) {
        Some(low) if (0xd800..0xdc00).contains(&unit) && (0xdc00..0xe000).contains(&low) => {
            0x10000 + ((unit - 0xd800) << 10) + (u32::from(low) - 0xdc00)
        }
        _ => unit,
    }
}

fn width(c: u32) -> usize {
    if c > 0xffff { 2 } else { 1 }
}

pub(super) fn tokenize(src: &[u16]) -> Tokens {
    let n = src.len();
    let mut tokens = Tokens { types: Vec::new(), spans: Vec::new() };
    let mut stack = Vec::new();
    let mut state = State::Initial;
    let mut p = 0;
    let keyword = |p: usize, word: &str| {
        let end = (p + word.len()).min(n);
        to_lowercase(&src[p..end]).is_str(word) && !src.at(p + word.len()).is_some_and(is_word)
    };

    while p < n {
        let c = src[p];
        let start = p;
        let mut q = p;
        let mut emit = |kind: T, s: usize, e: usize, p: &mut usize| {
            tokens.types.push(kind);
            tokens.spans.push((s, e));
            *p = e;
        };
        if state == State::EscapedText {
            if c == 34 && src.at(p + 1) != Some(34) {
                if let Some(outer) = stack.pop() {
                    state = outer;
                }
                emit(T::Dquote, p, p + 1, &mut p);
                continue;
            }
            while q < n {
                let d = code_point(src, q);
                if is_text_data(d) || d == 44 || d == 13 || d == 10 {
                    q += width(d);
                } else if d == 34 && src.at(q + 1) == Some(34) {
                    q += 2;
                } else {
                    break;
                }
            }
            emit(T::EscapedText, p, q, &mut p);
        } else {
            if state == State::Initial && (c | 32) == 115 {
                let length = if keyword(p, "sankey-beta") {
                    11
                } else if keyword(p, "sankey") {
                    6
                } else {
                    0
                };
                if length > 0 {
                    stack.push(state);
                    state = State::Csv;
                    emit(T::Sankey, p, p + length, &mut p);
                    continue;
                }
            }
            if c == 10 || (c == 13 && src.at(p + 1) == Some(10)) {
                emit(T::Newline, p, if c == 10 { p + 1 } else { p + 2 }, &mut p);
                continue;
            }
            if c == 44 {
                emit(T::Comma, p, p + 1, &mut p);
                continue;
            }
            if c == 34 {
                stack.push(state);
                state = State::EscapedText;
                emit(T::Dquote, p, p + 1, &mut p);
                continue;
            }
            while q < n {
                let d = code_point(src, q);
                if !is_text_data(d) {
                    break;
                }
                q += width(d);
            }
            emit(T::NonEscapedText, p, q, &mut p);
        }
        // A character outside the CSV alphabet matches as empty text and is never consumed.
        if q == start {
            tokens.types.push(T::Stuck);
            tokens.spans.push((p, p));
            return tokens;
        }
    }

    tokens.types.push(if state == State::EscapedText { T::EscapedText } else { T::Eof });
    tokens.spans.push((n, n));
    tokens.types.push(T::End);
    tokens.spans.push((n, n));
    tokens
}
