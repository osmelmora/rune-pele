//! The ER lexer, ported from Pele's `src/diagrams/er/lexer.ts`: a state machine over UTF-16 code
//! units that gives the tokens Mermaid's `erDiagram.jison` lexer gives.
//!
//! A code unit past the end of the text is -1 here, where JavaScript reads NaN. Both match no
//! test the lexer makes.

use crate::input::{is_digit, is_line_end, is_space, is_word};

/// Pele's token types (`tokens.ts`), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum T {
    End,
    Error,
    ErDiagram,
    Eof,
    Newline,
    Colon,
    StyleSeparator,
    BlockStart,
    BlockStop,
    Sqs,
    Sqe,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    EndKw,
    Subgraph,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    ClassDef,
    UnicodeText,
    StyleText,
    Comma,
    Class,
    Style,
    Semi,
    Num,
    Brkt,
    EntityName,
    DecimalNum,
    EntityOne,
    AttributeWord,
    AttributeKey,
    Comment,
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    MdParent,
    NonIdentifying,
    Identifying,
    Word,
    /// A character no rule names. The grammar gives it meaning only as `?` and `,` in a block.
    Char,
}

impl T {
    /// Pele's `TOKEN_NAMES`, as its messages name a token.
    pub(super) fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Error => "INVALID",
            T::ErDiagram => "ER_DIAGRAM",
            T::Eof => "EOF",
            T::Newline => "NEWLINE",
            T::Colon => "COLON",
            T::StyleSeparator => "STYLE_SEPARATOR",
            T::BlockStart => "BLOCK_START",
            T::BlockStop => "BLOCK_STOP",
            T::Sqs => "SQS",
            T::Sqe => "SQE",
            T::AccTitle => "acc_title",
            T::AccTitleValue => "acc_title_value",
            T::AccDescr => "acc_descr",
            T::AccDescrValue => "acc_descr_value",
            T::AccDescrMultilineValue => "acc_descr_multiline_value",
            T::EndKw => "END",
            T::Subgraph => "SUBGRAPH",
            T::DirectionTb => "direction_tb",
            T::DirectionBt => "direction_bt",
            T::DirectionRl => "direction_rl",
            T::DirectionLr => "direction_lr",
            T::ClassDef => "CLASSDEF",
            T::UnicodeText => "UNICODE_TEXT",
            T::StyleText => "STYLE_TEXT",
            T::Comma => "COMMA",
            T::Class => "CLASS",
            T::Style => "STYLE",
            T::Semi => "SEMI",
            T::Num => "NUM",
            T::Brkt => "BRKT",
            T::EntityName => "ENTITY_NAME",
            T::DecimalNum => "DECIMAL_NUM",
            T::EntityOne => "ENTITY_ONE",
            T::AttributeWord => "ATTRIBUTE_WORD",
            T::AttributeKey => "ATTRIBUTE_KEY",
            T::Comment => "COMMENT",
            T::ZeroOrOne => "ZERO_OR_ONE",
            T::ZeroOrMore => "ZERO_OR_MORE",
            T::OneOrMore => "ONE_OR_MORE",
            T::OnlyOne => "ONLY_ONE",
            T::MdParent => "MD_PARENT",
            T::NonIdentifying => "NON_IDENTIFYING",
            T::Identifying => "IDENTIFYING",
            T::Word => "WORD",
            T::Char => "CHAR",
        }
    }
}

pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    pub(super) starts: Vec<usize>,
    pub(super) ends: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Block,
    BlockBq,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Style,
}

const TEXT: u8 = 1;
const STYLE_TEXT: u8 = 2;
const ATTR_START: u8 = 4;
const ATTR_REST: u8 = 8;

/// What each ASCII character may be part of.
const CLASS: [u8; 128] = {
    let mut class = [0u8; 128];
    let mut c = 0;
    while c < 128 {
        let digit = c >= 48 && c <= 57;
        let word = digit || (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95;
        if word {
            class[c] |= TEXT | STYLE_TEXT | ATTR_REST;
        }
        if word && !digit {
            class[c] |= ATTR_START;
        }
        c += 1;
    }
    class[b'-' as usize] |= TEXT | STYLE_TEXT | ATTR_REST;
    class[b'*' as usize] |= TEXT | STYLE_TEXT | ATTR_REST | ATTR_START;
    class[b'.' as usize] |= TEXT | ATTR_REST;
    let rest = b"[](),";
    let mut i = 0;
    while i < rest.len() {
        class[rest[i] as usize] |= ATTR_REST;
        i += 1;
    }
    class
};

fn class_has(c: i32, kind: u8) -> bool {
    (0..128).contains(&c) && CLASS[c as usize] & kind != 0
}

/// A code unit that may run on in a token of the INITIAL or style state.
fn runs(c: i32, kind: u8) -> bool {
    c >= 128 || class_has(c, kind)
}

/// Mermaid's attribute pattern is case-insensitive, which lets the micro sign into its
/// `À-￿` range.
fn is_attr(c: i32, kind: u8) -> bool {
    if c < 128 { class_has(c, kind) } else { c >= 0xc0 || c == 0xb5 }
}

fn space(c: i32) -> bool {
    c >= 0 && is_space(c as u16)
}

fn word(c: i32) -> bool {
    c >= 0 && is_word(c as u16)
}

fn digit(c: i32) -> bool {
    c >= 0 && is_digit(c as u16)
}

struct Lexer<'a> {
    src: &'a [u16],
    n: usize,
    tokens: Tokens,
    state: S,
    p: usize,
    /// Where the next `direction` is, -2 before the first look and -1 when there is none.
    dir_next: isize,
    /// The end of the last line on which no direction statement matched.
    dir_failed: usize,
    line_end: isize,
    tilde_next: isize,
    tilde_line: isize,
    tilde_last: isize,
    ws_next: isize,
}

impl<'a> Lexer<'a> {
    fn c(&self, at: usize) -> i32 {
        self.src.get(at).map_or(-1, |&c| i32::from(c))
    }

    fn emit(&mut self, kind: T, start: usize, end: usize) {
        self.tokens.types.push(kind);
        self.tokens.starts.push(start);
        self.tokens.ends.push(end);
        self.p = end;
    }

    /// Matches a lower-case keyword at p, ignoring the case of its letters.
    fn kw(&self, word: &str) -> bool {
        word.bytes().enumerate().all(|(i, k)| {
            let (k, c) = (i32::from(k), self.c(self.p + i));
            c == k || ((97..=122).contains(&k) && c >= 0 && (c | 32) == k)
        })
    }

    fn kwb(&self, keyword: &str) -> bool {
        self.kw(keyword) && !word(self.c(self.p + keyword.len()))
    }

    fn lower(&self, at: usize) -> i32 {
        let c = self.c(at);
        if (65..=90).contains(&c) { c + 32 } else { c }
    }

    /// `/[\n\r  ]/g` from q: where the line q is on ends.
    fn next_line_end(&self, q: usize) -> usize {
        (q..self.n).find(|&at| is_line_end(self.src[at])).unwrap_or(self.n)
    }

    fn digits(&self, mut q: usize) -> usize {
        while digit(self.c(q)) {
            q += 1;
        }
        q
    }

    fn caseless(&self, at: usize, word: &str) -> bool {
        word.bytes().enumerate().all(|(i, k)| {
            self.src.get(at + i).is_some_and(|&c| u8::try_from(c).is_ok_and(|c| c.eq_ignore_ascii_case(&k)))
        })
    }

    /// `/accTitle\s*:\s*/iy` and its kin: the keyword, spaces, `opener`, spaces.
    fn acc(&self, keyword: &str, opener: u16) -> Option<usize> {
        if !self.caseless(self.p, keyword) {
            return None;
        }
        let mut at = self.p + keyword.len();
        while space(self.c(at)) {
            at += 1;
        }
        if self.src.get(at) != Some(&opener) {
            return None;
        }
        at += 1;
        while space(self.c(at)) {
            at += 1;
        }
        Some(at)
    }

    /// `/.*direction\s+XX[^\n]*/iy` at p, for the two letters of one direction. The greedy `.*`
    /// takes the last `direction` on the line that is followed by them.
    fn direction_to(&self, which: &str) -> Option<usize> {
        let line_end = self.next_line_end(self.p);
        (self.p..line_end).rev().find_map(|k| {
            if !self.caseless(k, "direction") {
                return None;
            }
            let mut spaces = k + 9;
            while space(self.c(spaces)) {
                spaces += 1;
            }
            if spaces == k + 9 || !self.caseless(spaces, which) {
                return None;
            }
            Some((spaces + 2..self.n).find(|&at| self.src[at] == 10).unwrap_or(self.n))
        })
    }

    /// A direction statement swallows its whole line, from wherever the lexer stands on it.
    fn direction(&mut self) -> bool {
        if self.dir_next == -1 {
            return false;
        }
        if self.dir_next < self.p as isize {
            self.dir_next = (self.p..self.n).find(|&at| self.caseless(at, "direction")).map_or(-1, |at| at as isize);
            if self.dir_next == -1 {
                return false;
            }
        }
        if self.p < self.dir_failed {
            return false;
        }
        if self.line_end < self.p as isize {
            self.line_end = self.next_line_end(self.p) as isize;
        }
        if self.dir_next >= self.line_end {
            return false;
        }
        let kinds = [T::DirectionTb, T::DirectionBt, T::DirectionRl, T::DirectionLr];
        for (kind, which) in kinds.into_iter().zip(["TB", "BT", "RL", "LR"]) {
            if let Some(end) = self.direction_to(which) {
                self.emit(kind, self.p, end);
                return true;
            }
        }
        self.dir_failed = self.line_end as usize;
        false
    }

    fn quoted(&mut self) {
        let p = self.p;
        let Some(e) = (p + 1..self.n).find(|&at| self.src[at] == 34) else {
            self.emit(T::Char, p, p + 1);
            return;
        };
        let name = e > p + 1 && self.src[p + 1..e].iter().all(|&c| !matches!(c, 37 | 92 | 13 | 10 | 11 | 8));
        self.emit(if name { T::EntityName } else { T::Word }, p, e + 1);
    }

    fn number(&mut self, c: i32) {
        let p = self.p;
        let d = self.c(p + 1);
        if d == 43 && (c == 49 || c == 48) {
            self.emit(if c == 49 { T::OneOrMore } else { T::ZeroOrMore }, p, p + 2);
            return;
        }
        let q = self.digits(p + 1);
        if self.c(q) == 46 && digit(self.c(q + 1)) {
            let end = self.digits(q + 2);
            self.emit(T::DecimalNum, p, end);
            return;
        }
        if c == 49 {
            if space(d) {
                let mut r = p + 2;
                while space(self.c(r)) {
                    r += 1;
                }
                let f = self.c(r);
                if word(f) || f == 34 || f == 39 {
                    self.emit(T::OnlyOne, p, p + 1);
                    return;
                }
            } else if d == 45 || d == 46 {
                let f = self.c(p + 2);
                if f == 45 || f == 46 {
                    self.emit(T::OnlyOne, p, p + 1);
                    return;
                }
            }
            if !word(d) {
                self.emit(T::EntityOne, p, p + 1);
                return;
            }
        }
        self.emit(T::Num, p, q);
    }

    fn keyword(&mut self, lc: i32) -> bool {
        let p = self.p;
        let d = self.c(p + 1);
        let found = match u8::try_from(lc).unwrap_or(0) {
            b'e' => {
                if self.kwb("erdiagram") {
                    Some((T::ErDiagram, 9))
                } else if self.kwb("end") {
                    let mut e = p + 3;
                    while space(self.c(e)) {
                        e += 1;
                    }
                    Some((T::EndKw, e - p))
                } else {
                    None
                }
            }
            b's' => {
                if self.kwb("style") {
                    self.state = S::Style;
                    Some((T::Style, 5))
                } else if self.kwb("subgraph") {
                    Some((T::Subgraph, 8))
                } else {
                    None
                }
            }
            b'c' => {
                if self.kwb("classdef") {
                    self.state = S::Style;
                    Some((T::ClassDef, 8))
                } else if self.kwb("class") {
                    Some((T::Class, 5))
                } else {
                    None
                }
            }
            b'o' => {
                if self.kwb("one or zero") {
                    Some((T::ZeroOrOne, 11))
                } else if self.kwb("one or more") || self.kwb("one or many") {
                    Some((T::OneOrMore, 11))
                } else if self.kwb("one") {
                    Some((T::OnlyOne, 3))
                } else if self.kwb("only one") {
                    Some((T::OnlyOne, 8))
                } else if d == 124 {
                    Some((T::ZeroOrOne, 2))
                } else if d == 123 {
                    Some((T::ZeroOrMore, 2))
                } else if self.kwb("optionally to") {
                    Some((T::NonIdentifying, 13))
                } else {
                    None
                }
            }
            b'z' => {
                if self.kwb("zero or one") {
                    Some((T::ZeroOrOne, 11))
                } else if self.kwb("zero or more") || self.kwb("zero or many") {
                    Some((T::ZeroOrMore, 12))
                } else {
                    None
                }
            }
            b'm' => {
                if self.kw("many(0)") {
                    Some((T::ZeroOrMore, 7))
                } else if self.kw("many(1)") {
                    Some((T::OneOrMore, 7))
                } else if self.kwb("many") {
                    Some((T::ZeroOrMore, 4))
                } else {
                    None
                }
            }
            b't' => self.kwb("to").then_some((T::Identifying, 2)),
            b'u' => matches!(d, 46 | 45 | 124).then_some((T::MdParent, 1)),
            b'|' => {
                if self.lower(p + 1) == 111 && !word(self.c(p + 2)) {
                    Some((T::ZeroOrOne, 2))
                } else if d == 124 {
                    Some((T::OnlyOne, 2))
                } else if d == 123 {
                    Some((T::OneOrMore, 2))
                } else {
                    None
                }
            }
            b'}' => {
                if self.lower(p + 1) == 111 && !word(self.c(p + 2)) {
                    Some((T::ZeroOrMore, 2))
                } else if d == 124 {
                    Some((T::OneOrMore, 2))
                } else {
                    None
                }
            }
            b'.' => matches!(d, 46 | 45).then_some((T::NonIdentifying, 2)),
            b'-' => match d {
                45 => Some((T::Identifying, 2)),
                46 => Some((T::NonIdentifying, 2)),
                _ => None,
            },
            _ => None,
        };
        match found {
            Some((kind, length)) => {
                self.emit(kind, p, p + length);
                true
            }
            None => false,
        }
    }

    fn initial(&mut self, c: i32) {
        let p = self.p;
        if (c == 97 || c == 65) && self.kw("acc") {
            if let Some(e) = self.acc("acctitle", 58) {
                self.state = S::AccTitle;
                self.emit(T::AccTitle, p, e);
                return;
            }
            if let Some(e) = self.acc("accdescr", 58) {
                self.state = S::AccDescr;
                self.emit(T::AccDescr, p, e);
                return;
            }
            if let Some(e) = self.acc("accdescr", 123) {
                self.state = S::AccDescrMultiline;
                self.p = e;
                return;
            }
        }
        if self.direction() {
            return;
        }
        match c {
            32 | 9 | 13 => {
                let mut e = p + 1;
                while matches!(self.c(e), 32 | 9 | 13) {
                    e += 1;
                }
                self.p = e;
                return;
            }
            10 => {
                let mut e = p + 1;
                while self.c(e) == 10 {
                    e += 1;
                }
                self.emit(T::Newline, p, e);
                return;
            }
            34 => {
                self.quoted();
                return;
            }
            123 => {
                self.state = S::Block;
                self.emit(T::BlockStart, p, p + 1);
                return;
            }
            35 => return self.emit(T::Brkt, p, p + 1),
            44 => return self.emit(T::Comma, p, p + 1),
            58 => {
                if self.c(p + 1) == 58 && self.c(p + 2) == 58 {
                    self.emit(T::StyleSeparator, p, p + 3);
                } else {
                    self.emit(T::Colon, p, p + 1);
                }
                return;
            }
            91 => return self.emit(T::Sqs, p, p + 1),
            93 => return self.emit(T::Sqe, p, p + 1),
            _ => {}
        }
        if digit(c) {
            self.number(c);
            return;
        }
        if self.keyword(if (65..=90).contains(&c) { c + 32 } else { c }) {
            return;
        }
        if runs(c, TEXT) {
            let mut e = p + 1;
            while runs(self.c(e), TEXT) {
                e += 1;
            }
            self.emit(T::UnicodeText, p, e);
            return;
        }
        self.emit(T::Char, p, p + 1);
    }

    /// Mermaid's generic type rule: a run of non-space characters holding a `~`, through the
    /// last `~` on the line.
    fn generic(&mut self) -> bool {
        let p = self.p;
        if self.tilde_next == -1 {
            return false;
        }
        if self.tilde_next < p as isize {
            self.tilde_next = (p..self.n).find(|&at| self.src[at] == 126).map_or(-1, |at| at as isize);
            if self.tilde_next == -1 {
                return false;
            }
        }
        if self.ws_next < p as isize {
            let mut q = p;
            while q < self.n && !is_space(self.src[q]) {
                q += 1;
            }
            self.ws_next = q as isize;
        }
        if self.tilde_next >= self.ws_next {
            return false;
        }
        if self.tilde_line < p as isize {
            let line = self.next_line_end(p);
            self.tilde_line = line as isize;
            // `lastIndexOf('~', line - 1)`, where a negative start looks at index 0 alone.
            let from = line.saturating_sub(1);
            self.tilde_last = (0..=from.min(self.n.saturating_sub(1)))
                .rev()
                .find(|&at| self.src.get(at) == Some(&126))
                .map_or(-1, |at| at as isize);
        }
        if self.tilde_last <= self.tilde_next {
            return false;
        }
        let mut e = self.tilde_last as usize + 1;
        while e < self.n && !is_space(self.src[e]) {
            e += 1;
        }
        self.emit(T::AttributeWord, p, e);
        true
    }

    fn block(&mut self, c: i32) {
        let p = self.p;
        if space(c) {
            self.p += 1;
            while space(self.c(self.p)) {
                self.p += 1;
            }
            return;
        }
        let lc = c | 32;
        if (lc == 112 || lc == 102 || lc == 117) && (self.c(p + 1) | 32) == 107 && !word(self.c(p + 2)) {
            self.emit(T::AttributeKey, p, p + 2);
            return;
        }
        if self.generic() {
            return;
        }
        if is_attr(c, ATTR_START) {
            let mut e = p + 1;
            while e < self.n && is_attr(self.c(e), ATTR_REST) {
                e += 1;
            }
            self.emit(T::AttributeWord, p, e);
            return;
        }
        if c == 96 {
            self.state = S::BlockBq;
            self.p += 1;
            return;
        }
        if c == 34 {
            if let Some(e) = (p + 1..self.n).find(|&at| self.src[at] == 34) {
                self.emit(T::Comment, p, e + 1);
                return;
            }
        } else if c == 125 {
            self.state = S::Initial;
            self.emit(T::BlockStop, p, p + 1);
            return;
        }
        self.emit(T::Char, p, p + 1);
    }

    /// The rest of the text up to `unit`, or to its end.
    fn until(&self, unit: u16) -> usize {
        (self.p..self.n).find(|&at| self.src[at] == unit).unwrap_or(self.n)
    }

    /// Lexes the text. In the style state a character no rule takes is an error token, and
    /// lexing stops there.
    fn run(mut self) -> Tokens {
        while self.p < self.n {
            let p = self.p;
            let c = self.c(p);
            match self.state {
                S::Initial => self.initial(c),
                S::Block => self.block(c),
                S::BlockBq => {
                    if c == 96 {
                        self.state = S::Block;
                        self.p += 1;
                    } else {
                        let e = self.until(96);
                        self.emit(T::AttributeWord, p, e);
                    }
                }
                S::AccTitle | S::AccDescr => {
                    let e = self.until(10);
                    let kind = if self.state == S::AccTitle { T::AccTitleValue } else { T::AccDescrValue };
                    self.emit(kind, p, e);
                    self.state = S::Initial;
                }
                S::AccDescrMultiline => {
                    if c == 125 {
                        self.state = S::Initial;
                        self.p += 1;
                    } else {
                        let e = self.until(125);
                        self.emit(T::AccDescrMultilineValue, p, e);
                    }
                }
                S::Style => {
                    if c == 10 {
                        let mut e = p + 1;
                        while self.c(e) == 10 {
                            e += 1;
                        }
                        self.state = S::Initial;
                        self.emit(T::Newline, p, e);
                    } else if space(c) {
                        self.p += 1;
                        while space(self.c(self.p)) {
                            self.p += 1;
                        }
                    } else if c == 58 {
                        self.emit(T::Colon, p, p + 1);
                    } else if c == 44 {
                        self.emit(T::Comma, p, p + 1);
                    } else if c == 35 {
                        self.emit(T::Brkt, p, p + 1);
                    } else if c == 59 {
                        self.emit(T::Semi, p, p + 1);
                    } else if runs(c, STYLE_TEXT) {
                        let mut e = p + 1;
                        while runs(self.c(e), STYLE_TEXT) {
                            e += 1;
                        }
                        self.emit(T::StyleText, p, e);
                    } else {
                        self.emit(T::Error, p, p);
                        return self.tokens;
                    }
                }
            }
        }
        // At the end of input Jison's INITIAL state yields EOF, the title and description
        // states yield an empty value, and the block and style states yield nothing.
        let last = match self.state {
            S::Initial => Some(T::Eof),
            S::AccTitle => Some(T::AccTitleValue),
            S::AccDescr => Some(T::AccDescrValue),
            S::AccDescrMultiline => Some(T::AccDescrMultilineValue),
            S::Block | S::BlockBq | S::Style => None,
        };
        let n = self.n;
        if let Some(last) = last {
            self.emit(last, n, n);
        }
        self.emit(T::End, n, n);
        self.tokens
    }
}

pub(super) fn tokenize(src: &[u16]) -> Tokens {
    let lexer = Lexer {
        src,
        n: src.len(),
        tokens: Tokens { types: Vec::new(), starts: Vec::new(), ends: Vec::new() },
        state: S::Initial,
        p: 0,
        dir_next: -2,
        dir_failed: 0,
        line_end: -1,
        tilde_next: -2,
        tilde_line: -1,
        tilde_last: -1,
        ws_next: -1,
    };
    lexer.run()
}
