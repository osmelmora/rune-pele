//! The sequence lexer, ported from Pele's `src/diagrams/sequence/lexer.ts`: a state machine over
//! UTF-16 code units that gives the tokens Mermaid's `sequenceDiagram.jison` lexer gives.
//!
//! A code unit past the end of the text is [`PAST`] here, where JavaScript reads NaN. Both match
//! no test the lexer makes.

use crate::input::{JsText, Units, is_digit, is_space, is_word};

/// Pele's token types (`tokens.ts`), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum T {
    End,
    Error,
    Invalid,
    Newline,
    Sd,
    Create,
    Box,
    RestOfLine,
    EndKw,
    Autonumber,
    Num,
    Off,
    Activate,
    Deactivate,
    Title,
    LegacyTitle,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Loop,
    Rect,
    Opt,
    Alt,
    Par,
    ParOver,
    Critical,
    Break,
    Option,
    And,
    Else,
    Participant,
    As,
    ParticipantActor,
    Destroy,
    Note,
    Over,
    Links,
    Link,
    Properties,
    Details,
    Comma,
    LeftOf,
    RightOf,
    Plus,
    Minus,
    Central,
    Actor,
    ConfigStart,
    ConfigContent,
    ConfigEnd,
    Txt,
    SolidOpenArrow,
    DottedOpenArrow,
    SolidArrow,
    BidirectionalSolidArrow,
    DottedArrow,
    BidirectionalDottedArrow,
    SolidCross,
    DottedCross,
    SolidPoint,
    DottedPoint,
    SolidArrowTop,
    SolidArrowBottom,
    StickArrowTop,
    StickArrowBottom,
    SolidArrowTopDotted,
    SolidArrowBottomDotted,
    StickArrowTopDotted,
    StickArrowBottomDotted,
    SolidArrowTopReverse,
    SolidArrowBottomReverse,
    StickArrowTopReverse,
    StickArrowBottomReverse,
    SolidArrowTopReverseDotted,
    SolidArrowBottomReverseDotted,
    StickArrowTopReverseDotted,
    StickArrowBottomReverseDotted,
}

/// Names as Mermaid's grammar spells them, for error messages.
const TOKEN_NAMES: [&str; 79] = [
    "$end",
    "INVALID",
    "INVALID",
    "NEWLINE",
    "SD",
    "create",
    "box",
    "restOfLine",
    "end",
    "autonumber",
    "NUM",
    "off",
    "activate",
    "deactivate",
    "title",
    "legacy_title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "loop",
    "rect",
    "opt",
    "alt",
    "par",
    "par_over",
    "critical",
    "break",
    "option",
    "and",
    "else",
    "participant",
    "AS",
    "participant_actor",
    "destroy",
    "note",
    "over",
    "links",
    "link",
    "properties",
    "details",
    ",",
    "left_of",
    "right_of",
    "+",
    "-",
    "()",
    "ACTOR",
    "CONFIG_START",
    "CONFIG_CONTENT",
    "CONFIG_END",
    "TXT",
    "SOLID_OPEN_ARROW",
    "DOTTED_OPEN_ARROW",
    "SOLID_ARROW",
    "BIDIRECTIONAL_SOLID_ARROW",
    "DOTTED_ARROW",
    "BIDIRECTIONAL_DOTTED_ARROW",
    "SOLID_CROSS",
    "DOTTED_CROSS",
    "SOLID_POINT",
    "DOTTED_POINT",
    "SOLID_ARROW_TOP",
    "SOLID_ARROW_BOTTOM",
    "STICK_ARROW_TOP",
    "STICK_ARROW_BOTTOM",
    "SOLID_ARROW_TOP_DOTTED",
    "SOLID_ARROW_BOTTOM_DOTTED",
    "STICK_ARROW_TOP_DOTTED",
    "STICK_ARROW_BOTTOM_DOTTED",
    "SOLID_ARROW_TOP_REVERSE",
    "SOLID_ARROW_BOTTOM_REVERSE",
    "STICK_ARROW_TOP_REVERSE",
    "STICK_ARROW_BOTTOM_REVERSE",
    "SOLID_ARROW_TOP_REVERSE_DOTTED",
    "SOLID_ARROW_BOTTOM_REVERSE_DOTTED",
    "STICK_ARROW_TOP_REVERSE_DOTTED",
    "STICK_ARROW_BOTTOM_REVERSE_DOTTED",
];

impl T {
    pub(super) fn name(self) -> &'static str {
        TOKEN_NAMES[self as usize]
    }
}

pub(super) struct Token {
    pub(super) kind: T,
    pub(super) text: JsText,
    pub(super) start: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Id,
    Alias,
    Line,
    Config,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

/// A character class of Mermaid's lexer rules.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    ActorStart,
    ActorRest,
    Id,
    IdWide,
    Line,
    Menu,
}

const PAST: u32 = 0x1_0000;

fn code(c: u8) -> u32 {
    u32::from(c)
}

fn space(c: u32) -> bool {
    u16::try_from(c).is_ok_and(is_space)
}

fn digit(c: u32) -> bool {
    u16::try_from(c).is_ok_and(is_digit)
}

struct Lexer<'a> {
    src: &'a [u16],
    n: usize,
    tokens: Vec<Token>,
    stack: Vec<S>,
    state: S,
    p: usize,
    /// The line break at or after the place last asked about, or the end of the text.
    next_nl: Option<usize>,
}

impl Lexer<'_> {
    fn at(&self, q: usize) -> u32 {
        self.src.get(q).map_or(PAST, |&c| u32::from(c))
    }

    fn emit(&mut self, kind: T, s: usize, e: usize) {
        let e = e.min(self.n);
        self.emit_text(kind, s, e, self.src[s.min(e)..e].into());
    }

    fn emit_text(&mut self, kind: T, s: usize, e: usize, text: JsText) {
        self.tokens.push(Token { kind, text, start: s });
        self.p = e;
    }

    fn push(&mut self, s: S) {
        self.stack.push(self.state);
        self.state = s;
    }

    fn pop(&mut self) {
        if let Some(state) = self.stack.pop() {
            self.state = state;
        }
    }

    fn eol(&mut self, q: usize) -> usize {
        match self.next_nl {
            Some(known) if known >= q => known,
            _ => {
                let found = self.src.find_unit(10, q).unwrap_or(self.n);
                self.next_nl = Some(found);
                found
            }
        }
    }

    fn has(&self, q: usize, class: Class) -> bool {
        let c = self.at(q);
        if c >= 128 {
            return !matches!(class, Class::Id | Class::Menu) || !space(c);
        }
        let c = char::from(c as u8);
        let ws = c == ' ' || ('\t'..='\r').contains(&c);
        match class {
            Class::ActorStart => !"/\\+()<->:\n,;".contains(c),
            Class::ActorRest => !"+<->:\n,;".contains(c),
            Class::Id => !"<>:\n,;@".contains(c) && !ws,
            Class::IdWide => !"<>:\n,;@".contains(c),
            Class::Line => !"#\n;".contains(c),
            Class::Menu => !"/\\+()<>:\n,;-".contains(c) && !ws,
        }
    }

    /// Matches a lowercase literal without regard to the case of the source.
    fn ci(&self, q: usize, word: &str) -> bool {
        word.bytes().enumerate().all(|(k, w)| {
            let c = self.at(q + k);
            if w.is_ascii_lowercase() { (c | 32) == code(w) } else { c == code(w) }
        })
    }

    fn word(&self, lit: &str) -> bool {
        self.ci(self.p, lit) && !self.src.get(self.p + lit.len()).is_some_and(|&c| is_word(c))
    }

    fn skip_ws(&self, q: usize) -> usize {
        self.src.run(q.min(self.n), is_space)
    }

    fn skip_same_line(&mut self) -> bool {
        let q = self.src.run(self.p, |c| c != 10 && is_space(c));
        if q == self.p {
            return false;
        }
        self.p = q;
        true
    }

    fn line_run(&self, q: usize) -> usize {
        let mut q = q;
        while q < self.n && self.has(q, Class::Line) {
            q += 1;
        }
        q
    }

    fn begin(&mut self, kind: T, len: usize, next: S) {
        self.emit(kind, self.p, self.p + len);
        self.push(next);
    }

    fn keyword(&mut self, lit: &str, kind: T, next: Option<S>) -> bool {
        if !self.word(lit) {
            return false;
        }
        self.emit(kind, self.p, self.p + lit.len());
        if let Some(next) = next {
            self.push(next);
        }
        true
    }

    /// The actor menu keywords count only when spaces and the start of a name follow.
    fn menu(&mut self, lit: &str, kind: T) -> bool {
        if !self.ci(self.p, lit) {
            return false;
        }
        let mut q = self.p + lit.len();
        if !matches!(self.at(q), 32 | 9) {
            return false;
        }
        while matches!(self.at(q), 32 | 9) {
            q += 1;
        }
        if q >= self.n || !self.has(q, Class::Menu) {
            return false;
        }
        self.emit(kind, self.p, self.p + lit.len());
        true
    }

    fn arrow_ahead(&self, q: usize) -> bool {
        let c = self.at(q);
        let d = self.at(q + 1);
        if c == 45 {
            if matches!(d, 120 | 88 | 41 | 92 | 47 | 45) {
                return true;
            }
            let e = self.at(q + 2);
            return d == 124 && (e == 92 || e == 47);
        }
        if c == 47 || c == 92 {
            return (d == 124 || d == c) && self.at(q + 2) == 45;
        }
        c == 40 && d == 41
    }

    fn actor(&mut self) {
        let mut q = self.p + 1;
        while q < self.n && self.has(q, Class::ActorStart) {
            q += 1;
        }
        while q < self.n && !self.arrow_ahead(q) {
            let mut r = q;
            while self.at(r) == 45 {
                r += 1;
            }
            if r >= self.n || !self.has(r, Class::ActorRest) {
                break;
            }
            r += 1;
            while r < self.n && self.has(r, Class::ActorRest) {
                r += 1;
            }
            q = r;
        }
        self.emit_text(T::Actor, self.p, q, self.src[self.p..q].trim_js().into());
    }

    fn number(&mut self) -> bool {
        let p = self.p;
        let mut q = p;
        while digit(self.at(q)) {
            q += 1;
        }
        let whole = q > p;
        if self.at(q) == 46 && digit(self.at(q + 1)) {
            let r = if digit(self.at(q + 2)) { q + 3 } else { q + 2 };
            if matches!(self.at(r), 32 | 10) {
                self.emit(T::Num, p, r);
                return true;
            }
        }
        if whole && matches!(self.at(q), 32 | 10) {
            self.emit(T::Num, p, q);
            return true;
        }
        false
    }

    fn dash(&mut self) {
        let p = self.p;
        let d = self.at(p + 1);
        let e = self.at(p + 2);
        match (d, e) {
            (62, 62) => self.emit(T::SolidArrow, p, p + 3),
            (62, _) => self.emit(T::SolidOpenArrow, p, p + 2),
            (120 | 88, _) => self.emit(T::SolidCross, p, p + 2),
            (41, _) => self.emit(T::SolidPoint, p, p + 2),
            (124, 92) => self.emit(T::SolidArrowTop, p, p + 3),
            (124, 47) => self.emit(T::SolidArrowBottom, p, p + 3),
            (92, 92) => self.emit(T::StickArrowTop, p, p + 3),
            (47, 47) => self.emit(T::StickArrowBottom, p, p + 3),
            (45, _) => {
                let f = self.at(p + 3);
                match (e, f) {
                    (62, 62) => self.emit(T::DottedArrow, p, p + 4),
                    (62, _) => self.emit(T::DottedOpenArrow, p, p + 3),
                    (120 | 88, _) => self.emit(T::DottedCross, p, p + 3),
                    (41, _) => self.emit(T::DottedPoint, p, p + 3),
                    (124, 92) => self.emit(T::SolidArrowTopDotted, p, p + 4),
                    (124, 47) => self.emit(T::SolidArrowBottomDotted, p, p + 4),
                    (92, 92) => self.emit(T::StickArrowTopDotted, p, p + 4),
                    (47, 47) => self.emit(T::StickArrowBottomDotted, p, p + 4),
                    _ => self.emit(T::Minus, p, p + 1),
                }
            }
            _ => self.emit(T::Minus, p, p + 1),
        }
    }

    fn reverse(&mut self, c: u32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        if (d != 124 && d != c) || self.at(p + 2) != 45 {
            return false;
        }
        let dotted = self.at(p + 3) == 45;
        let top = c == 47;
        let kind = match (d == 124, dotted, top) {
            (true, true, true) => T::SolidArrowTopReverseDotted,
            (true, true, false) => T::SolidArrowBottomReverseDotted,
            (true, false, true) => T::SolidArrowTopReverse,
            (true, false, false) => T::SolidArrowBottomReverse,
            (false, true, true) => T::StickArrowTopReverseDotted,
            (false, true, false) => T::StickArrowBottomReverseDotted,
            (false, false, true) => T::StickArrowTopReverse,
            (false, false, false) => T::StickArrowBottomReverse,
        };
        self.emit(kind, p, p + if dotted { 4 } else { 3 });
        true
    }

    fn title(&mut self) -> bool {
        if !self.ci(self.p, "title") {
            return false;
        }
        let mut q = self.p + 5;
        let legacy = self.at(q) == 58;
        if legacy {
            q += 1;
        }
        let e = self.src.run(q.min(self.n), |c| c != 10 && is_space(c));
        if e == self.n || self.at(e) == 10 {
            self.p = e;
            return true;
        }
        let sep = self.at(q);
        if !space(sep) || sep == 13 || !self.has(q + 1, Class::Line) {
            return false;
        }
        let end = self.line_run(q + 1);
        self.emit(if legacy { T::LegacyTitle } else { T::Title }, self.p, end);
        true
    }

    fn acc(&mut self) -> bool {
        let descr = self.ci(self.p, "accdescr");
        if !descr && !self.ci(self.p, "acctitle") {
            return false;
        }
        let q = self.skip_ws(self.p + 8);
        let c = self.at(q);
        if c == 58 {
            let end = self.skip_ws(q + 1);
            self.emit(if descr { T::AccDescr } else { T::AccTitle }, self.p, end);
            self.push(if descr { S::AccDescr } else { S::AccTitle });
            return true;
        }
        if descr && c == 123 {
            self.p = self.skip_ws(q + 1);
            self.push(S::AccDescrMultiline);
            return true;
        }
        false
    }

    fn keywords(&mut self, c: u32) -> bool {
        let (id, line) = (Some(S::Id), Some(S::Line));
        match u8::try_from(c | 32).unwrap_or(0) {
            b'a' => {
                self.keyword("actor", T::ParticipantActor, id)
                    || self.keyword("alt", T::Alt, line)
                    || self.keyword("and", T::And, line)
                    || self.keyword("activate", T::Activate, id)
                    || self.acc()
                    || self.keyword("autonumber", T::Autonumber, None)
            }
            b'b' => self.keyword("box", T::Box, line) || self.keyword("break", T::Break, line),
            b'c' => self.keyword("create", T::Create, None) || self.keyword("critical", T::Critical, line),
            b'd' => {
                self.keyword("destroy", T::Destroy, id)
                    || self.menu("details", T::Details)
                    || self.keyword("deactivate", T::Deactivate, id)
            }
            b'e' => self.keyword("else", T::Else, line) || self.keyword("end", T::EndKw, None),
            b'l' => {
                self.keyword("loop", T::Loop, line)
                    || self.keyword("left of", T::LeftOf, None)
                    || self.menu("links", T::Links)
                    || self.menu("link", T::Link)
            }
            b'n' => self.keyword("note", T::Note, None),
            b'o' => {
                self.keyword("opt", T::Opt, line)
                    || self.keyword("option", T::Option, line)
                    || self.keyword("over", T::Over, None)
                    || self.keyword("off", T::Off, None)
            }
            b'p' => {
                self.keyword("participant", T::Participant, id)
                    || self.keyword("par", T::Par, line)
                    || self.keyword("par_over", T::ParOver, line)
                    || self.menu("properties", T::Properties)
            }
            b'r' => self.keyword("rect", T::Rect, line) || self.keyword("right of", T::RightOf, None),
            b's' => self.keyword("sequencediagram", T::Sd, None),
            b't' => self.title(),
            _ => false,
        }
    }

    fn initial(&mut self) {
        let p = self.p;
        let c = self.at(p);
        if c == 10 {
            let q = self.src.run(p + 1, |c| c == 10);
            self.emit(T::Newline, p, q);
            return;
        }
        if space(c) {
            self.p = self.skip_ws(p + 1);
            return;
        }
        if c == 35 || (c == 37 && self.at(p + 1) != 123) {
            self.p = self.eol(p);
            return;
        }
        if (digit(c) || c == 46) && self.number() {
            return;
        }
        if c < 128 && (97..=122).contains(&(c | 32)) && self.keywords(c) {
            return;
        }
        if c == 44 {
            self.emit(T::Comma, p, p + 1);
        } else if c == 59 {
            self.emit(T::Newline, p, p + 1);
        } else if self.has(p, Class::ActorStart) {
            self.actor();
        } else if c == 45 {
            self.dash();
        } else if c == 60 && self.src.starts_at(p, "<<->>") {
            self.emit(T::BidirectionalSolidArrow, p, p + 5);
        } else if c == 60 && self.src.starts_at(p, "<<-->>") {
            self.emit(T::BidirectionalDottedArrow, p, p + 6);
        } else if (c == 47 || c == 92) && self.reverse(c) {
            // The reversed arrow is the token.
        } else if c == 58 {
            let end = self.line_run(p + 1);
            self.emit(T::Txt, p, end);
        } else if c == 43 {
            self.emit(T::Plus, p, p + 1);
        } else if c == 40 && self.at(p + 1) == 41 {
            self.emit(T::Central, p, p + 2);
        } else {
            self.emit(T::Invalid, p, p + 1);
        }
    }

    /// A participant name, up to an alias, a config object, or the end of the statement.
    fn id(&mut self) -> bool {
        if self.skip_same_line() {
            return true;
        }
        let p = self.p;
        let c = self.at(p);
        if c == 35 {
            self.p = self.eol(p);
            return true;
        }
        if c == 64 && self.at(p + 1) == 123 {
            self.begin(T::ConfigStart, 2, S::Config);
            return true;
        }
        let mut a = p;
        while a < self.n && self.has(a, Class::Id) {
            a += 1;
        }
        if a > p {
            let w = self.skip_ws(a);
            if self.at(w) == 64 && self.at(w + 1) == 123 {
                self.emit(T::Actor, p, a);
                return true;
            }
            if w > a && self.ci(w, "as") && space(self.at(w + 2)) {
                self.emit(T::Actor, p, a);
                self.push(S::Alias);
                return true;
            }
        }
        let mut b = a;
        while b < self.n && self.has(b, Class::IdWide) {
            b += 1;
        }
        if b > p {
            let end = self.at(b);
            // Before any other delimiter the name must be followed by a comment, so it ends at the last one.
            let mut cut = b;
            if b < self.n && end != 10 && end != 59 {
                cut -= 1;
                while cut > p && self.at(cut) != 35 {
                    cut -= 1;
                }
            }
            if cut > p {
                self.emit_text(T::Actor, p, cut, self.src[p..cut].trim_js().into());
                self.pop();
                return true;
            }
        }
        if self.at(b) == 60 {
            let end = self.eol(p);
            self.emit(T::Invalid, p, end);
            self.pop();
            return true;
        }
        if c != 10 {
            let e = self.eol(p);
            self.emit_text(T::Invalid, p, e, self.src[p..e].trim_js().into());
            self.pop();
            return true;
        }
        false
    }

    fn step(&mut self) -> bool {
        let p = self.p;
        match self.state {
            S::Initial => {
                self.initial();
                true
            }
            S::Id => self.id(),
            S::Alias => {
                if self.skip_same_line() {
                    return true;
                }
                if self.at(self.p) == 35 {
                    self.p = self.eol(self.p);
                    return true;
                }
                self.pop();
                self.pop();
                if self.word("as") {
                    self.begin(T::As, 2, S::Line);
                } else {
                    self.emit(T::Newline, self.p, self.p);
                }
                true
            }
            S::Line => {
                if self.skip_same_line() {
                    return true;
                }
                if self.at(self.p) == 35 {
                    self.p = self.eol(self.p);
                    return true;
                }
                let end = self.line_run(self.p);
                self.emit(T::RestOfLine, self.p, end);
                self.pop();
                true
            }
            S::Config => {
                if self.at(p) != 125 {
                    let q = self.src.find_unit(125, p).unwrap_or(self.n);
                    self.emit(T::ConfigContent, p, q);
                } else {
                    let w = self.skip_ws(p + 1);
                    let alias = w > p + 1 && self.ci(w, "as") && space(self.at(w + 2));
                    self.emit(T::ConfigEnd, p, p + 1);
                    self.pop();
                    if alias {
                        self.push(S::Alias);
                    } else {
                        self.pop();
                    }
                }
                true
            }
            S::AccTitle | S::AccDescr => {
                let end = self.eol(p);
                self.emit(if self.state == S::AccTitle { T::AccTitleValue } else { T::AccDescrValue }, p, end);
                self.pop();
                true
            }
            S::AccDescrMultiline => {
                if self.at(p) == 125 {
                    self.p += 1;
                    self.pop();
                } else {
                    let q = self.src.find_unit(125, p).unwrap_or(self.n);
                    self.emit(T::AccDescrMultilineValue, p, q);
                }
                true
            }
        }
    }
}

pub(super) fn tokenize(src: &[u16]) -> Vec<Token> {
    let n = src.len();
    let mut lexer = Lexer { src, n, tokens: Vec::new(), stack: Vec::new(), state: S::Initial, p: 0, next_nl: None };
    while lexer.p < n {
        if !lexer.step() {
            lexer.tokens.push(Token { kind: T::Error, text: JsText::default(), start: lexer.p });
            return lexer.tokens;
        }
    }
    // Mermaid's lexer tries its rules once more on the empty remainder before reporting the end.
    let last = match lexer.state {
        S::Initial | S::Alias => Some(T::Newline),
        S::Line => Some(T::RestOfLine),
        S::AccTitle => Some(T::AccTitleValue),
        S::AccDescr => Some(T::AccDescrValue),
        S::AccDescrMultiline => Some(T::AccDescrMultilineValue),
        S::Id | S::Config => None,
    };
    if let Some(kind) = last {
        lexer.emit(kind, n, n);
    }
    lexer.tokens.push(Token { kind: T::End, text: JsText::default(), start: n });
    lexer.tokens
}
