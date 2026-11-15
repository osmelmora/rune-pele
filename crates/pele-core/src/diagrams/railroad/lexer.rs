//! What the four railroad grammars share, ported from Pele's `railroad/common.ts`: the title and
//! accessibility tokens (which, unlike the ones in Mermaid's common grammar, take no leading
//! blanks), the hidden tokens, string values, and the statements before the first rule.

use super::Header;
use crate::diagrams::tokens::{
    DIRECTIVE, Matcher, Reader, SINGLE_LINE_COMMENT, STRING, Scanner, SyntaxError, TokenType, YAML, acc_descr_value,
    acc_title_value, hidden, title_value, token, tokenize,
};
use crate::input::{Units, is_blank, is_digit, is_letter, is_space, is_word};

const fn unit(c: u8) -> u16 {
    c as u16
}

/// `(?:[^\n\r]*?(?=%%)|[^\n\r]*)`: the rest of the line, or what comes before a comment on it.
fn line_value(src: &[u16], from: usize) -> usize {
    let line_end = src.run(from, |c| c != 10 && c != 13);
    (from..line_end).find(|&at| src.starts_at(at, "%%")).unwrap_or(line_end)
}

/// `title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|(?!rest))`
fn title_ending(src: &[u16], at: usize, rest: fn(u16) -> bool) -> Option<usize> {
    if !src.starts_at(at, "title") {
        return None;
    }
    let after = at + 5;
    if src.at(after).is_some_and(is_blank) {
        return Some(line_value(src, after + 1));
    }
    (!src.at(after).is_some_and(rest)).then_some(after)
}

fn is_name_unit(c: u16) -> bool {
    is_word(c) || c == unit(b'-')
}

/// The `TITLE` of the grammars whose names are `[\w-]`.
fn title_before_name(src: &[u16], at: usize) -> Option<usize> {
    title_ending(src, at, is_name_unit)
}

/// The `TITLE` of ABNF, whose names are `[A-Za-z0-9-]`.
fn title_before_rulename(src: &[u16], at: usize) -> Option<usize> {
    title_ending(src, at, |c| is_letter(c) || is_digit(c) || c == unit(b'-'))
}

pub(super) const TITLE: TokenType = token("TITLE", Matcher::Pattern(title_before_name));
pub(super) const ABNF_TITLE: TokenType = token("TITLE", Matcher::Pattern(title_before_rulename));

/// `accTitle[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)`
fn acc_title(src: &[u16], at: usize) -> Option<usize> {
    if !src.starts_at(at, "accTitle") {
        return None;
    }
    let colon = src.run(at + 8, is_blank);
    (src.at(colon) == Some(unit(b':'))).then(|| line_value(src, colon + 1))
}

pub(super) const ACC_TITLE: TokenType = token("ACC_TITLE", Matcher::Pattern(acc_title));

/// `accDescr(?:[\t ]*:([^\n\r]*?(?=%%)|[^\n\r]*)|\s*{([^}]*)})`. The braced form scans to the next
/// closing brace. Once none is found, none exists further on either, so later braced
/// descriptions fail without scanning again.
fn acc_descr_scanner() -> Scanner {
    let mut unclosed = usize::MAX;
    Box::new(move |src, at| {
        if !src.starts_at(at, "accDescr") {
            return None;
        }
        let colon = src.run(at + 8, is_blank);
        if src.at(colon) == Some(unit(b':')) {
            return Some(line_value(src, colon + 1));
        }
        let brace = src.run(colon, is_space);
        if src.at(brace) != Some(unit(b'{')) || brace >= unclosed {
            return None;
        }
        let close = src.find_unit(unit(b'}'), brace + 1);
        if close.is_none() {
            unclosed = brace;
        }
        close.map(|close| close + 1)
    })
}

pub(super) const ACC_DESCR: TokenType = token("ACC_DESCR", Matcher::Scanner(acc_descr_scanner));

/// `[A-Z_a-z][\w-]*`
fn identifier(src: &[u16], at: usize) -> Option<usize> {
    let first = src.at(at)?;
    (is_letter(first) || first == unit(b'_')).then(|| src.run(at + 1, is_name_unit))
}

pub(super) const fn identifier_named(name: &'static str) -> TokenType {
    token(name, Matcher::Pattern(identifier))
}

/// `"([^"\\]|\\.)*"|'([^'\\]|\\.)*'`, as Mermaid's common `STRING`.
pub(super) const fn quoted(name: &'static str) -> TokenType {
    TokenType { name, ..STRING }
}

pub(super) const fn literal(text: &'static str) -> TokenType {
    token(text, Matcher::Literal(text))
}

/// `[\t \r\n]+`
fn whitespace(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, |c| c == 9 || c == 32 || c == 13 || c == 10))
}

/// Whitespace, front matter, directive, and `%%` comment, in the order every railroad grammar
/// lists them.
pub(super) const fn hidden_tokens(prefix: [&'static str; 4]) -> [TokenType; 4] {
    [
        hidden(prefix[0], Matcher::Pattern(whitespace)),
        TokenType { name: prefix[1], ..YAML },
        TokenType { name: prefix[2], ..DIRECTIVE },
        TokenType { name: prefix[3], ..SINGLE_LINE_COMMENT },
    ]
}

/// `/\*[\s\S]*?\*/`
fn block_comment(src: &[u16], at: usize) -> Option<usize> {
    block_opener(src, at)?;
    src.find_str("*/", at + 2).map(|close| close + 2)
}

/// `/\*`
fn block_opener(src: &[u16], at: usize) -> Option<usize> {
    src.starts_at(at, "/*").then_some(at + 2)
}

pub(super) const fn block_comment_named(name: &'static str) -> TokenType {
    hidden(name, Matcher::Closed { pattern: block_comment, opener: block_opener })
}

/// Mermaid's railroad value converters know three escapes; any other escaped character stands
/// for itself.
pub(super) fn unescape(text: &[u16]) -> Vec<u16> {
    if !text.contains(&unit(b'\\')) {
        return unquote(text);
    }
    let end = text.len() - 1;
    let mut out = Vec::with_capacity(end);
    let mut i = 1;
    while i < end {
        let mut c = text[i];
        if c == unit(b'\\') && i + 1 < end {
            i += 1;
            c = match u8::try_from(text[i]) {
                Ok(b'n') => 10,
                Ok(b'r') => 13,
                Ok(b't') => 9,
                _ => text[i],
            };
        }
        out.push(c);
        i += 1;
    }
    out
}

/// `text.slice(1, -1)`, which is empty for a lone quote.
pub(super) fn unquote(text: &[u16]) -> Vec<u16> {
    text.get(1..text.len().saturating_sub(1)).unwrap_or_default().to_vec()
}

/// Pele's `parseGrammar`: the keyword, then any titles and accessibility statements, then rules
/// to the end of the text. Every table starts with its keyword and has `TITLE`, `ACC_TITLE` and
/// `ACC_DESCR` in a row from `meta`.
pub(super) fn parse_grammar<'a>(
    src: &'a [u16],
    types: &'a [TokenType],
    meta: usize,
    unquote: fn(&[u16]) -> Vec<u16>,
    mut rule: impl FnMut(&mut Reader<'a>) -> Result<(), SyntaxError>,
) -> Result<Header, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, types)?, types);
    let mut header = Header::default();
    r.expect(0)?;
    loop {
        match r.kind() {
            Some(kind) if kind == meta => {
                let title = title_value(r.take());
                let quoted = matches!(title.first(), Some(&quote) if quote == unit(b'"') || quote == unit(b'\''))
                    && title.last() == title.first();
                header.title = Some(if quoted { unquote(&title) } else { title });
            }
            Some(kind) if kind == meta + 1 => header.acc_title = Some(acc_title_value(r.take())),
            Some(kind) if kind == meta + 2 => header.acc_descr = Some(acc_descr_value(r.take())),
            _ => break,
        }
    }
    while r.kind().is_some() {
        rule(&mut r)?;
    }
    Ok(header)
}
