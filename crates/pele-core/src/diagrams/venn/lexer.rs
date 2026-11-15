//! The venn lexer, ported from Pele's `src/diagrams/venn/lexer.ts`: it reads one token at a time,
//! as Mermaid's Jison lexer for venn diagrams does. Every rule ignores case, the first rule that
//! matches wins, and a keyword must end at a word boundary.
//!
//! A code unit past the end of the text is [`PAST`] here, where JavaScript reads NaN. Both match
//! no test the lexer makes.

use crate::input::{Units, is_digit, is_hex, is_space, is_word};

/// Pele's token types, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum T {
    End,
    Error,
    Eof,
    Newline,
    Venn,
    Title,
    Set,
    Union,
    Text,
    IndentText,
    Style,
    BracketLabel,
    Numeric,
    HexColor,
    RgbColor,
    RgbaColor,
    Identifier,
    String,
    Comma,
    Colon,
}

impl T {
    /// Pele's `TOKEN_NAMES`.
    pub(super) fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Error => "INVALID",
            T::Eof => "EOF",
            T::Newline => "NEWLINE",
            T::Venn => "VENN",
            T::Title => "TITLE",
            T::Set => "SET",
            T::Union => "UNION",
            T::Text => "TEXT",
            T::IndentText => "INDENT_TEXT",
            T::Style => "STYLE",
            T::BracketLabel => "BRACKET_LABEL",
            T::Numeric => "NUMERIC",
            T::HexColor => "HEXCOLOR",
            T::RgbColor => "RGBCOLOR",
            T::RgbaColor => "RGBACOLOR",
            T::Identifier => "IDENTIFIER",
            T::String => "STRING",
            T::Comma => "COMMA",
            T::Colon => "COLON",
        }
    }
}

/// The part of the model the lexer talks to: an indented `text` line is a different token after
/// a set or union than anywhere else, and the parser is what knows which came before.
pub(super) trait IndentHost {
    fn indent_mode(&self) -> bool;
    fn set_indent_mode(&mut self, enabled: bool);
}

const KEYWORDS: [(&str, T); 5] =
    [("venn-beta", T::Venn), ("set", T::Set), ("union", T::Union), ("text", T::Text), ("style", T::Style)];

const PAST: u32 = 0x1_0000;

pub(super) struct Lexer<'a> {
    src: &'a [u16],
    /// The text and position of the token `next` returned last.
    pub(super) text: &'a [u16],
    pub(super) start: usize,
    at: usize,
    line_start: bool,
    ended: bool,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(src: &'a [u16]) -> Self {
        Lexer { src, text: &[], start: 0, at: 0, line_start: false, ended: false }
    }

    fn c(&self, at: usize) -> u32 {
        self.src.get(at).map_or(PAST, |&c| u32::from(c))
    }

    fn is(&self, at: usize, class: fn(u16) -> bool) -> bool {
        self.src.get(at).is_some_and(|&c| class(c))
    }

    /// Whether `word`, in lowercase, starts at `at` in any case.
    fn word(&self, at: usize, word: &str) -> bool {
        word.bytes().enumerate().all(|(k, w)| {
            let (c, w) = (self.c(at + k), u32::from(w));
            c == w || (c + 32 == w && (65..=90).contains(&c))
        })
    }

    fn keyword(&self, at: usize, word: &str) -> bool {
        self.word(at, word) && !self.is(at + word.len(), is_word)
    }

    /// The end of `rgb(1, 2, 3)` or `rgba(1, 2, 3, 0.5)` whose parenthesis opens at `at`.
    fn color(&self, at: usize, parts: usize) -> Option<usize> {
        let mut i = at + 1;
        for part in 0..parts {
            i = self.src.run(i, is_space);
            let from = i;
            i = self.src.run(i, |c| is_digit(c) || c == 46);
            if i == from {
                return None;
            }
            i = self.src.run(i, is_space);
            if self.c(i) != if part == parts - 1 { 41 } else { 44 } {
                return None;
            }
            i += 1;
        }
        Some(i)
    }

    fn after_line_feed(&self, at: usize) -> bool {
        let mut k = at;
        while k > 0 && matches!(self.src[k - 1], 32 | 9) {
            k -= 1;
        }
        k > 0 && self.src[k - 1] == 10
    }

    fn line_end(&self, from: usize) -> usize {
        self.src.find_unit(10, from).unwrap_or(self.src.len())
    }

    fn blanks(&self, from: usize) -> usize {
        self.src.run(from, |c| c == 32 || c == 9)
    }

    pub(super) fn next(&mut self, host: &mut dyn IndentHost) -> T {
        let src = self.src;
        let n = src.len();
        loop {
            let mut i = self.at;
            self.start = i;
            self.text = &[];
            if i >= n {
                if self.ended {
                    return T::End;
                }
                self.ended = true;
                return T::Eof;
            }
            let c = self.c(i);
            if c == 37 && self.c(i + 1) == 37 && self.c(i + 2) != 123 {
                self.at = self.line_end(i + 2);
                continue;
            }
            if self.line_start
                && self.word(i, "title")
                && self.after_line_feed(i)
                && src[i + 5..self.line_end(i + 5)].trim_js().is_empty()
            {
                // Skip a bare title like a comment, so it doesn't end indented text.
                let j = self.line_end(i + 5);
                self.at = src.run(j, |c| c == 10 || c == 13);
                continue;
            }
            if self.line_start {
                if c == 32 || c == 9 {
                    let j = self.blanks(i + 1);
                    self.at = j;
                    if self.keyword(j, "text") && host.indent_mode() {
                        // The word `text` itself is dropped; the indentation stands for it.
                        self.at = j + 4;
                        self.line_start = false;
                        self.text = &src[i..j];
                        return T::IndentText;
                    }
                    continue;
                }
                if c != 10 && c != 13 {
                    host.set_indent_mode(false);
                    self.line_start = false;
                }
            }
            if c == 10 || c == 13 {
                i = src.run(i + 1, |c| c == 10 || c == 13);
                self.line_start = true;
                return self.token(T::Newline, i);
            }
            if c == 37 && self.c(i + 1) == 37 {
                self.at = self.line_end(i);
                continue;
            }
            if c == 32 || c == 9 {
                self.at = self.blanks(i + 1);
                continue;
            }

            if ((c | 32) >= 97 && (c | 32) <= 122) || c == 95 {
                let sep = self.c(i + 5);
                if self.word(i, "title") && sep != PAST && is_space(sep as u16) && sep != 10 && sep != 13 {
                    let end = src.run(i + 6, |ch| ch != 35 && ch != 10 && ch != 59);
                    if end > i + 6 {
                        return self.token(T::Title, end);
                    }
                }
                for (word, kind) in KEYWORDS {
                    // A hyphen continues a name, as in `set-theory`.
                    if self.keyword(i, word) && self.c(i + word.len()) != 45 {
                        return self.token(kind, i + word.len());
                    }
                }
                if self.word(i, "rgb") {
                    let alpha = (self.c(i + 3) | 32) == 97;
                    let open = i + if alpha { 4 } else { 3 };
                    let end = if self.c(open) == 40 { self.color(open, if alpha { 4 } else { 3 }) } else { None };
                    if let Some(end) = end {
                        return self.token(if alpha { T::RgbaColor } else { T::RgbColor }, end);
                    }
                }
                let end = src.run(i + 1, |c| is_word(c) || c == 45);
                return self.token(T::Identifier, end);
            }
            if c == 91 {
                if self.c(i + 1) == 34 {
                    let quote = src.find_unit(34, i + 2);
                    let Some(quote) = quote.filter(|&quote| self.c(quote + 1) == 93) else {
                        return T::Error;
                    };
                    self.at = quote + 2;
                    self.text = &src[i + 2..quote];
                    return T::BracketLabel;
                }
                let end = src.run(i + 1, |c| c != 93 && c != 34);
                if end == i + 1 || self.c(end) != 93 {
                    return T::Error;
                }
                self.at = end + 1;
                self.text = src[i + 1..end].trim_js();
                return T::BracketLabel;
            }
            if self.is(i, is_digit) || c == 46 || c == 43 || c == 45 {
                if c == 43 || c == 45 {
                    i += 1;
                }
                let from = i;
                i = src.run(i, is_digit);
                if i > from {
                    if self.c(i) == 46 && self.is(i + 1, is_digit) {
                        i = src.run(i + 2, is_digit);
                    }
                } else if self.c(i) == 46 && self.is(i + 1, is_digit) {
                    i = src.run(i + 2, is_digit);
                } else {
                    return T::Error;
                }
                return self.token(T::Numeric, i);
            }
            if c == 35 {
                let end = src[..(i + 9).min(n)].run(i + 1, is_hex);
                return if end >= i + 4 { self.token(T::HexColor, end) } else { T::Error };
            }
            if c == 34 {
                return match src.find_unit(34, i + 1) {
                    Some(quote) => self.token(T::String, quote + 1),
                    None => T::Error,
                };
            }
            if c == 44 {
                return self.token(T::Comma, i + 1);
            }
            if c == 58 {
                return self.token(T::Colon, i + 1);
            }
            return T::Error;
        }
    }

    fn token(&mut self, kind: T, end: usize) -> T {
        self.text = &self.src[self.at..end];
        self.at = end;
        kind
    }
}
