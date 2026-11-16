//! Pele's Ishikawa lexer and parser (`ishikawa/lexer.ts`, `ishikawa/parser.ts`), which follow the
//! lexer and parser Jison builds from Mermaid's grammar.

use crate::diagrams::tokens::SyntaxError;
use crate::input::{Units, is_space, is_word};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum T {
    End,
    Eof,
    Spaceline,
    Nl,
    Ishikawa,
    Spacelist,
    Text,
}

impl T {
    fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Eof => "EOF",
            T::Spaceline => "SPACELINE",
            T::Nl => "NL",
            T::Ishikawa => "ISHIKAWA",
            T::Spacelist => "SPACELIST",
            T::Text => "TEXT",
        }
    }

    fn stops(self) -> bool {
        matches!(self, T::Spaceline | T::Nl | T::Eof)
    }
}

struct Token {
    kind: T,
    start: usize,
    end: usize,
}

/// The length of `ishikawa-beta`, or failing that `ishikawa`, at `at`, either one ending at a word
/// boundary, or 0. Upper case ASCII letters match their lower case.
fn keyword(src: &[u16], at: usize) -> usize {
    let word = b"ishikawa-beta";
    let mut k = 0;
    while k < word.len() {
        let w = u16::from(word[k]);
        match src.at(at + k) {
            Some(c) if c == w || ((65..=90).contains(&c) && c + 32 == w) => k += 1,
            _ => break,
        }
    }
    let boundary = |length: usize| !src.at(at + length).is_some_and(is_word);
    if k == word.len() && boundary(13) {
        13
    } else if k >= 8 && boundary(8) {
        8
    } else {
        0
    }
}

/// Pele's `tokenize`, where the first rule that matches wins: white space up to a comment or up
/// to the last line break in it is a blank line, a single line break before text is a new line,
/// other white space is indentation, and the rest of a line is text.
fn tokenize(src: &[u16]) -> Vec<Token> {
    let n = src.len();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < n {
        let mut j = i;
        let mut last_break = None;
        while j < n && is_space(src[j]) {
            if src[j] == 10 {
                last_break = Some(j);
            }
            j += 1;
        }
        let (kind, end) = if src.at(j) == Some(37) && src.at(j + 1) == Some(37) {
            let end = src.run(j + 2, |c| !matches!(c, 10 | 13 | 0x2028 | 0x2029));
            (T::Spaceline, end)
        } else if j == i {
            match keyword(src, i) {
                0 => (T::Text, src.find_unit(10, i).unwrap_or(n)),
                length => (T::Ishikawa, i + length),
            }
        } else {
            match last_break {
                Some(at) if at > i => (T::Spaceline, at + 1),
                Some(_) => (T::Nl, i + 1),
                None => (T::Spacelist, j),
            }
        };
        tokens.push(Token { kind, start: i, end });
        i = end;
    }
    tokens.push(Token { kind: T::Eof, start: n, end: n });
    tokens.push(Token { kind: T::End, start: n, end: n });
    tokens
}

const STOP: &str = "'SPACELINE', 'NL', 'EOF'";

/// Pele's `parseIshikawa`: blank lines and comments may come first, then the keyword and at most
/// one line break, then statements. A statement is a line of text, with or without indentation,
/// or a blank line; each is ended by a blank line, a line break or the end of the text, and any
/// number of line breaks after that. Each line of text is handed to `add` with how far it is set
/// in.
pub(super) fn parse(src: &[u16], mut add: impl FnMut(usize, &[u16])) -> Result<(), SyntaxError> {
    let tokens = tokenize(src);
    let kind = |i: usize| tokens[i].kind;
    let fail = |i: usize, expected: &str| SyntaxError {
        at: tokens[i].start,
        detail: format!("Expecting {expected}, got '{}'", kind(i).name()),
        lexical: false,
    };
    let mut i = 0;
    if kind(i) == T::Spaceline {
        i += 1;
        while matches!(kind(i), T::Spaceline | T::Nl) {
            i += 1;
        }
        if kind(i) != T::Ishikawa {
            return Err(fail(i, "'SPACELINE', 'NL', 'ISHIKAWA'"));
        }
    } else if kind(i) != T::Ishikawa {
        return Err(fail(i, "'SPACELINE', 'ISHIKAWA'"));
    }
    i += 1;
    if kind(i) == T::Nl {
        i += 1;
    }
    loop {
        let here = kind(i);
        if here == T::Text || (here == T::Spacelist && kind(i + 1) == T::Text) {
            let text = if here == T::Text { i } else { i + 1 };
            i = text + 1;
            if !kind(i).stops() {
                return Err(fail(i, STOP));
            }
            let level = if here == T::Text { 0 } else { tokens[text - 1].end - tokens[text - 1].start };
            add(level, src[tokens[text].start..tokens[text].end].trim_js());
        } else if matches!(here, T::Spaceline | T::Spacelist) {
            i += 1;
            if !kind(i).stops() {
                return Err(fail(i, if here == T::Spacelist { "'SPACELINE', 'NL', 'EOF', 'TEXT'" } else { STOP }));
            }
        } else {
            return Err(fail(i, "'SPACELINE', 'SPACELIST', 'TEXT'"));
        }
        i += 1;
        while matches!(kind(i), T::Nl | T::Eof) {
            i += 1;
        }
        if kind(i) == T::End {
            return Ok(());
        }
    }
}
