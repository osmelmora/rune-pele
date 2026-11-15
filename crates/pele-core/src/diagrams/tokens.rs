//! The tokenizer for the diagram types Mermaid defines with Langium grammars, ported from Pele's
//! `src/diagrams/common/tokens.ts` and `accDescr.ts`. Token types are tried in order and the first
//! match wins, unless the winner names longer alternatives.
//!
//! Pele also tries a type only at the characters its `first` names. A table whose `first` is
//! exact matches the same tokens without it, so this port has none.
//!
//! Each pattern function answers for the sticky regular expression its comment quotes: given a
//! position, it returns where the match ends.

use crate::input::{Units, is_blank, is_line_end, is_space, is_word};

/// Where parsing stopped, in the text the parser was given.
#[derive(Debug)]
pub(crate) struct SyntaxError {
    pub(crate) at: usize,
    pub(crate) detail: String,
    /// No token type matches here, as opposed to a token the grammar does not expect.
    pub(crate) lexical: bool,
}

type Scan = fn(&[u16], usize) -> Option<usize>;
pub(crate) type Scanner = Box<dyn FnMut(&[u16], usize) -> Option<usize>>;

pub(crate) enum Matcher {
    Literal(&'static str),
    /// `word(?:(?=%%)|(?!\S))`: Mermaid ends a keyword only at whitespace, a comment, or the end.
    Keyword(&'static str),
    Pattern(Scan),
    /// A construct that needs a closer. When `opener` matches but `pattern` does not, no closer
    /// exists in the rest of the text, so the pattern is not tried again.
    Closed {
        pattern: Scan,
        opener: Scan,
    },
    /// A matcher that remembers what it has scanned. It is made once per text.
    Scanner(fn() -> Scanner),
    /// Pele's `titleBefore(rest)`: `TITLE`, except that the bare word does not match where
    /// `rest`, the suffix of a longer name, matches right after it.
    TitleBefore(fn(&[u16], usize) -> bool),
    /// Pele's `match` that reads the token before it: the type of the last visible token, or
    /// `None` at the start, is passed after the position.
    After(fn(&[u16], usize, Option<usize>) -> Option<usize>),
}

pub(crate) struct TokenType {
    pub(crate) name: &'static str,
    pub(crate) matcher: Matcher,
    pub(crate) hidden: bool,
    /// Chevrotain's `LONGER_ALT`, as places in the same table. When this type matches, these are
    /// tried at the same place, and the first that matches more text becomes the token.
    pub(crate) longer: &'static [usize],
}

impl TokenType {
    pub(crate) const fn longer(self, alternatives: &'static [usize]) -> Self {
        TokenType { longer: alternatives, ..self }
    }
}

pub(crate) const fn token(name: &'static str, matcher: Matcher) -> TokenType {
    TokenType { name, matcher, hidden: false, longer: &[] }
}

pub(crate) const fn hidden(name: &'static str, matcher: Matcher) -> TokenType {
    TokenType { name, matcher, hidden: true, longer: &[] }
}

pub(crate) const fn keyword(word: &'static str) -> TokenType {
    token(word, Matcher::Keyword(word))
}

const fn unit(c: u8) -> u16 {
    c as u16
}

fn not_line_break(c: u16) -> bool {
    c != 10 && c != 13
}

/// `\r?\n`
pub(crate) fn newline(src: &[u16], at: usize) -> Option<usize> {
    let line_feed = at + usize::from(src.at(at) == Some(13));
    (src.at(line_feed) == Some(10)).then_some(line_feed + 1)
}

/// `(?:\r?\n|(?!\S))`
pub(crate) fn line_break_or_space(src: &[u16], at: usize) -> Option<usize> {
    newline(src, at).or_else(|| src.at(at).is_none_or(is_space).then_some(at))
}

/// `(?:[^\n\r]*?(?=%%)|[^\n\r]*)`: the rest of the line, or what comes before a comment on it.
fn line_value(src: &[u16], from: usize) -> usize {
    let line_end = src.run(from, not_line_break);
    (from..line_end).find(|&at| src.starts_at(at, "%%")).unwrap_or(line_end)
}

/// `[\t ]*accDescr(?:[\t ]*:([^\n\r]*?(?=%%)|[^\n\r]*)|\s*{([^}]*)})`
pub(crate) fn acc_descr(src: &[u16], at: usize) -> Option<usize> {
    let word = src.run(at, is_blank);
    if !src.starts_at(word, "accDescr") {
        return None;
    }
    let after = word + 8;
    let colon = src.run(after, is_blank);
    if src.at(colon) == Some(unit(b':')) {
        return Some(line_value(src, colon + 1));
    }
    let brace = src.run(after, is_space);
    if src.at(brace) != Some(unit(b'{')) {
        return None;
    }
    src.find_unit(unit(b'}'), brace + 1).map(|close| close + 1)
}

/// `[\t ]*accTitle[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)`
fn acc_title(src: &[u16], at: usize) -> Option<usize> {
    let word = src.run(at, is_blank);
    if !src.starts_at(word, "accTitle") {
        return None;
    }
    let colon = src.run(word + 8, is_blank);
    (src.at(colon) == Some(unit(b':'))).then(|| line_value(src, colon + 1))
}

/// `[\t ]*title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|)`
fn title(src: &[u16], at: usize) -> Option<usize> {
    title_ending(src, at, |_, _| false)
}

/// `[\t ]*title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|(?!rest))`
pub(crate) fn title_ending(src: &[u16], at: usize, rest: fn(&[u16], usize) -> bool) -> Option<usize> {
    let word = src.run(at, is_blank);
    if !src.starts_at(word, "title") {
        return None;
    }
    let after = word + 5;
    if src.at(after).is_some_and(is_blank) {
        return Some(line_value(src, after + 1));
    }
    (!rest(src, after)).then_some(after)
}

/// Pele's `titleBefore(rest)`, the `TITLE` terminal of a grammar where a name can start with the
/// word `title`. `rest` answers whether the suffix of such a name matches at a position.
pub(crate) const fn title_before(rest: fn(&[u16], usize) -> bool) -> TokenType {
    token("TITLE", Matcher::TitleBefore(rest))
}

fn is_dash_or_word(c: u16) -> bool {
    c == unit(b'-') || is_word(c)
}

/// The `rest` `[-\w]*\w`, of architecture and radar.
pub(crate) fn dashed_name(src: &[u16], at: usize) -> bool {
    (at..src.run(at, is_dash_or_word)).any(|i| is_word(src[i]))
}

/// The `rest` `\w`, of eventmodeling and treemap.
pub(crate) fn word_unit(src: &[u16], at: usize) -> bool {
    src.at(at).is_some_and(is_word)
}

/// The `rest` `[-./\w]*[-\w]`, of git.
pub(crate) fn ref_name(src: &[u16], at: usize) -> bool {
    let end = src.run(at, |c| c == unit(b'.') || c == unit(b'/') || is_dash_or_word(c));
    (at..end).any(|i| is_dash_or_word(src[i]))
}

/// The `rest` `(?!%%)[^\n\r]`, of treeview.
pub(crate) fn line_text(src: &[u16], at: usize) -> bool {
    !src.starts_at(at, "%%") && src.at(at).is_some_and(not_line_break)
}

/// `"([^"\\]|\\.)*"|'([^'\\]|\\.)*'`
fn string(src: &[u16], at: usize) -> Option<usize> {
    let quote = src.at(at).filter(|&c| c == unit(b'"') || c == unit(b'\''))?;
    let mut i = at + 1;
    loop {
        let c = src.at(i)?;
        if c == quote {
            return Some(i + 1);
        }
        if c == unit(b'\\') {
            if is_line_end(src.at(i + 1)?) {
                return None;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
}

/// `[\t ]+`
fn whitespace(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, is_blank))
}

/// `---[\t ]*\r?\n`
fn yaml_opener(src: &[u16], at: usize) -> Option<usize> {
    if !src.starts_at(at, "---") {
        return None;
    }
    newline(src, src.run(at + 3, is_blank))
}

/// `---(?:\r?\n|(?!\S))`
fn yaml_fence(src: &[u16], at: usize) -> Option<usize> {
    if !src.starts_at(at, "---") {
        return None;
    }
    line_break_or_space(src, at + 3)
}

/// `---[\t ]*\r?\n(?:[\S\s]*?\r?\n)?---(?:\r?\n|(?!\S))`
fn yaml(src: &[u16], at: usize) -> Option<usize> {
    let body = yaml_opener(src, at)?;
    let mut from = body;
    while let Some(line_feed) = src.find_unit(10, from) {
        if let Some(end) = yaml_fence(src, line_feed + 1) {
            return Some(end);
        }
        from = line_feed + 1;
    }
    yaml_fence(src, body)
}

/// `[\t ]*%%{`
fn directive_opener(src: &[u16], at: usize) -> Option<usize> {
    let marker = src.run(at, is_blank);
    src.starts_at(marker, "%%{").then_some(marker + 3)
}

/// `[\t ]*%%{[\S\s]*?}%%(?:\r?\n|(?!\S))`
fn directive(src: &[u16], at: usize) -> Option<usize> {
    let mut from = directive_opener(src, at)?;
    while let Some(close) = src.find_str("}%%", from) {
        if let Some(end) = line_break_or_space(src, close + 3) {
            return Some(end);
        }
        from = close + 1;
    }
    None
}

/// `[\t ]*%%[^\n\r]*`
fn single_line_comment(src: &[u16], at: usize) -> Option<usize> {
    let marker = src.run(at, is_blank);
    src.starts_at(marker, "%%").then(|| src.run(marker + 2, not_line_break))
}

pub(crate) const ACC_DESCR: TokenType = token("ACC_DESCR", Matcher::Pattern(acc_descr));

/// Pele's `accDescr(name)` (`common/accDescr.ts`): `ACC_DESCR` for a grammar that goes on lexing
/// after an `accDescr {` that never closes, because `accDescr` is also a name there. Once a braced
/// description finds no closing brace there is none further on, so a later one fails without
/// reading the rest of the text again.
pub(crate) const fn acc_descr_named(name: &'static str) -> TokenType {
    token(name, Matcher::Scanner(acc_descr_scanner))
}

/// `[\t ]*accDescr(?:[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)|\s*({))`, then for a brace the text up to
/// the next closing one.
fn acc_descr_scanner() -> Scanner {
    let mut unclosed = false;
    Box::new(move |src, at| {
        let word = src.run(at, is_blank);
        if !src.starts_at(word, "accDescr") {
            return None;
        }
        let after = word + 8;
        let colon = src.run(after, is_blank);
        if src.at(colon) == Some(unit(b':')) {
            return Some(line_value(src, colon + 1));
        }
        let brace = src.run(after, is_space);
        if src.at(brace) != Some(unit(b'{')) || unclosed {
            return None;
        }
        let close = src.find_unit(unit(b'}'), brace + 1);
        unclosed = close.is_none();
        close.map(|close| close + 1)
    })
}
pub(crate) const ACC_TITLE: TokenType = token("ACC_TITLE", Matcher::Pattern(acc_title));
pub(crate) const TITLE: TokenType = token("TITLE", Matcher::Pattern(title));
pub(crate) const STRING: TokenType = token("STRING", Matcher::Pattern(string));
pub(crate) const NEWLINE: TokenType = token("NEWLINE", Matcher::Pattern(newline));
pub(crate) const WHITESPACE: TokenType = hidden("WHITESPACE", Matcher::Pattern(whitespace));
pub(crate) const YAML: TokenType = hidden("YAML", Matcher::Closed { pattern: yaml, opener: yaml_opener });
pub(crate) const DIRECTIVE: TokenType =
    hidden("DIRECTIVE", Matcher::Closed { pattern: directive, opener: directive_opener });
pub(crate) const SINGLE_LINE_COMMENT: TokenType = hidden("SINGLE_LINE_COMMENT", Matcher::Pattern(single_line_comment));

/// The visible tokens of a text. The last one is the end of input, whose kind is `None`.
pub(crate) struct Tokens {
    pub(crate) kinds: Vec<Option<usize>>,
    pub(crate) starts: Vec<usize>,
    pub(crate) ends: Vec<usize>,
}

pub(crate) fn tokenize(src: &[u16], types: &[TokenType]) -> Result<Tokens, SyntaxError> {
    let mut tokens = Tokens { kinds: Vec::new(), starts: Vec::new(), ends: Vec::new() };
    let mut dead = vec![false; types.len()];
    let mut scanners: Vec<Option<Scanner>> = types.iter().map(|_| None).collect();
    let mut matched = |kind: usize, at: usize, previous: Option<usize>| match &types[kind].matcher {
        Matcher::Literal(text) => src.starts_at(at, text).then_some(at + text.len()),
        Matcher::Keyword(word) => {
            let end = at + word.len();
            let ends = src.starts_at(end, "%%") || src.at(end).is_none_or(is_space);
            (src.starts_at(at, word) && ends).then_some(end)
        }
        Matcher::Pattern(scan) => scan(src, at),
        Matcher::Closed { .. } if dead[kind] => None,
        Matcher::Closed { pattern, opener } => {
            let end = pattern(src, at);
            dead[kind] = end.is_none() && opener(src, at).is_some();
            end
        }
        Matcher::Scanner(make) => scanners[kind].get_or_insert_with(make)(src, at),
        Matcher::TitleBefore(rest) => title_ending(src, at, *rest),
        Matcher::After(scan) => scan(src, at, previous),
    };
    let mut at = 0;
    'scan: while at < src.len() {
        let previous = tokens.kinds.last().copied().flatten();
        for kind in 0..types.len() {
            let Some(mut end) = matched(kind, at, previous).filter(|&end| end > at) else {
                continue;
            };
            let mut found = kind;
            for &alternative in types[kind].longer {
                if let Some(longer) = matched(alternative, at, previous).filter(|&longer| longer > end) {
                    (found, end) = (alternative, longer);
                    break;
                }
            }
            if !types[found].hidden {
                tokens.kinds.push(Some(found));
                tokens.starts.push(at);
                tokens.ends.push(end);
            }
            at = end;
            continue 'scan;
        }
        return Err(SyntaxError { at, detail: String::new(), lexical: true });
    }
    tokens.kinds.push(None);
    tokens.starts.push(src.len());
    tokens.ends.push(src.len());
    Ok(tokens)
}

/// `text.trim().replace(/[\t ]{2,}/gm, ' ')`
fn single(text: &[u16]) -> Vec<u16> {
    collapse(text.trim_js(), is_blank, 32)
}

/// Replaces each run of two or more `in_run` units with one `with`.
fn collapse(text: &[u16], in_run: fn(u16) -> bool, with: u16) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let end = text.run(at, in_run);
        if end >= at + 2 {
            out.push(with);
            at = end;
        } else {
            out.push(text[at]);
            at += 1;
        }
    }
    out
}

/// The title a `TITLE` token holds: what `/title([\t ][^\n\r]*|)/` captures, on one line.
pub(crate) fn title_value(token: &[u16]) -> Vec<u16> {
    let word = token.find_str("title", 0).expect("a TITLE token holds the word");
    single(&token[word + 5..])
}

/// The value an `ACC_TITLE` token holds: what `/accTitle[\t ]*:([^\n\r]*)/` captures.
pub(crate) fn acc_title_value(token: &[u16]) -> Vec<u16> {
    let colon = token.find_unit(unit(b':'), 0).expect("an ACC_TITLE token holds a colon");
    single(&token[colon + 1..])
}

/// The description an `ACC_DESCR` token holds, from `/accDescr(?:[\t ]*:([^\n\r]*)|\s*{([^}]*)})/`.
pub(crate) fn acc_descr_value(token: &[u16]) -> Vec<u16> {
    let after = token.find_str("accDescr", 0).expect("an ACC_DESCR token holds the word") + 8;
    let colon = token.run(after, is_blank);
    if token.at(colon) == Some(unit(b':')) {
        return single(&token[colon + 1..]);
    }
    let brace = token.run(after, is_space);
    let block = &token[brace + 1..token.len() - 1];
    let text = strip_line_ends(&strip_line_starts(block));
    collapse(&collapse(&text, is_blank, 32), |c| c == 10 || c == 13, 10)
}

/// `text.replace(/^\s*/gm, '')`
fn strip_line_starts(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let spaces = if at == 0 || is_line_end(text[at - 1]) { text.run(at, is_space) } else { at };
        if spaces > at {
            at = spaces;
        } else {
            out.push(text[at]);
            at += 1;
        }
    }
    out
}

/// `text.replace(/\s+$/gm, '')`: of each whitespace run, what comes before its last line end goes.
fn strip_line_ends(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let end = text.run(at, is_space);
        if end == at {
            out.push(text[at]);
            at += 1;
        } else if end == text.len() {
            break;
        } else {
            let kept = (at + 1..end).rev().find(|&k| is_line_end(text[k])).unwrap_or(at);
            out.extend_from_slice(&text[kept..end]);
            at = end;
        }
    }
    out
}

/// What a `STRING` token holds, without its quotes and with its escapes read.
pub(crate) fn string_value(token: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(token.len());
    let mut i = 1;
    while i < token.len() - 1 {
        let mut c = token[i];
        if c == unit(b'\\') {
            i += 1;
            c = match u8::try_from(token[i]) {
                Ok(b'b') => 8,
                Ok(b'f') => 12,
                Ok(b'n') => 10,
                Ok(b'r') => 13,
                Ok(b't') => 9,
                Ok(b'v') => 11,
                Ok(b'0') => 0,
                _ => token[i],
            };
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Reads tokens for a hand-written parser, with Chevrotain-style messages on failure.
pub(crate) struct Reader<'a> {
    src: &'a [u16],
    tokens: Tokens,
    types: &'a [TokenType],
    i: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(src: &'a [u16], tokens: Tokens, types: &'a [TokenType]) -> Self {
        Reader { src, tokens, types, i: 0 }
    }

    pub(crate) fn kind(&self) -> Option<usize> {
        self.tokens.kinds[self.i]
    }

    fn text(&self, index: usize) -> &'a [u16] {
        &self.src[self.tokens.starts[index]..self.tokens.ends[index]]
    }

    pub(crate) fn skip(&mut self) {
        self.i += 1;
    }

    /// Where the current token starts in the text.
    pub(crate) fn start(&self) -> usize {
        self.tokens.starts[self.i]
    }

    pub(crate) fn take(&mut self) -> &'a [u16] {
        self.i += 1;
        self.text(self.i - 1)
    }

    pub(crate) fn accept(&mut self, kind: usize) -> bool {
        let found = self.kind() == Some(kind);
        self.i += usize::from(found);
        found
    }

    pub(crate) fn expect(&mut self, kind: usize) -> Result<&'a [u16], SyntaxError> {
        if self.kind() != Some(kind) {
            return Err(self.fail(&format!("token of type '{}'", self.types[kind].name)));
        }
        Ok(self.take())
    }

    pub(crate) fn fail(&self, expected: &str) -> SyntaxError {
        let mut at = self.tokens.starts[self.i];
        let found = if self.kind().is_none() {
            // Locate EOF errors after the last token, before trailing line breaks and hidden comments.
            let mut k = self.i;
            while k > 0 && self.text(k - 1).trim_js().is_empty() {
                k -= 1;
            }
            if k > 0 {
                at = self.tokens.ends[k - 1];
            }
            "end of input".to_owned()
        } else {
            format!("`{}`", String::from_utf16_lossy(self.text(self.i)))
        };
        SyntaxError { at, detail: format!("Expecting {expected} but found {found}."), lexical: false }
    }
}
