//! The agentflow lexer (Pele's `src/diagrams/agentflow/lexer.ts`), which reproduces the token
//! stream of Mermaid's `agentflow.jison` lexer, with the line and column Jison counts for each
//! token. Each scanner names the regular expression it answers for, and the lexer states and
//! caches follow Pele's one for one.

use crate::common::is_unicode_letter as unicode_letter_unit;
use crate::input::{Units, is_space as space_unit, is_word as word_unit};

macro_rules! tokens {
    ($($kind:ident => $name:literal),+ $(,)?) => {
        /// Pele's token types (`tokens.ts`), with the names its messages print.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(super) enum T {
            $($kind),+
        }

        impl T {
            pub(super) fn name(self) -> &'static str {
                match self {
                    $(T::$kind => $name),+
                }
            }
        }
    };
}

tokens! {
    End => "$end",
    Error => "INVALID",
    AccTitle => "acc_title",
    AccTitleValue => "acc_title_value",
    AccDescr => "acc_descr",
    AccDescrValue => "acc_descr_value",
    AccDescrMultilineValue => "acc_descr_multiline_value",
    ShapeData => "SHAPE_DATA",
    CallbackName => "CALLBACKNAME",
    CallbackArgs => "CALLBACKARGS",
    MdStr => "MD_STR",
    Str => "STR",
    Style => "STYLE",
    Default => "DEFAULT",
    LinkStyle => "LINKSTYLE",
    Interpolate => "INTERPOLATE",
    ClassDef => "CLASSDEF",
    Class => "CLASS",
    Href => "HREF",
    Click => "CLICK",
    Graph => "GRAPH",
    Flow => "flow",
    Connector => "connector",
    Global => "global",
    EndKeyword => "end",
    LinkTarget => "LINK_TARGET",
    NoDir => "NODIR",
    Dir => "DIR",
    DirectionTb => "direction_tb",
    DirectionBt => "direction_bt",
    DirectionRl => "direction_rl",
    DirectionLr => "direction_lr",
    DirectionTd => "direction_td",
    LinkId => "LINK_ID",
    Num => "NUM",
    Brkt => "BRKT",
    StyleSeparator => "STYLE_SEPARATOR",
    Colon => "COLON",
    Amp => "AMP",
    Semi => "SEMI",
    Comma => "COMMA",
    Mult => "MULT",
    Link => "LINK",
    StartLink => "START_LINK",
    EdgeText => "EDGE_TEXT",
    EllipseEnd => "-)",
    Text => "TEXT",
    EllipseStart => "(-",
    StadiumEnd => "STADIUMEND",
    StadiumStart => "STADIUMSTART",
    SubroutineEnd => "SUBROUTINEEND",
    SubroutineStart => "SUBROUTINESTART",
    VertexWithPropsStart => "VERTEX_WITH_PROPS_START",
    TagEnd => "TAGEND",
    CylinderEnd => "CYLINDEREND",
    CylinderStart => "CYLINDERSTART",
    DoubleCircleEnd => "DOUBLECIRCLEEND",
    DoubleCircleStart => "DOUBLECIRCLESTART",
    TrapEnd => "TRAPEND",
    InvTrapEnd => "INVTRAPEND",
    TrapStart => "TRAPSTART",
    InvTrapStart => "INVTRAPSTART",
    TagStart => "TAGSTART",
    Up => "UP",
    Sep => "SEP",
    Down => "DOWN",
    Comment => "COMMENT",
    NodeString => "NODE_STRING",
    Minus => "MINUS",
    UnicodeText => "UNICODE_TEXT",
    Pipe => "PIPE",
    Pe => "PE",
    Ps => "PS",
    Sqe => "SQE",
    Sqs => "SQS",
    DiamondStop => "DIAMOND_STOP",
    DiamondStart => "DIAMOND_START",
    Newline => "NEWLINE",
    Space => "SPACE",
    Eof => "EOF",
}

/// The direction statements in the order the lexer tries them, with the word each one needs.
pub(super) const DIRECTION_STATEMENTS: [(T, &str); 5] = [
    (T::DirectionTb, "TB"),
    (T::DirectionBt, "BT"),
    (T::DirectionRl, "RL"),
    (T::DirectionLr, "LR"),
    (T::DirectionTd, "TD"),
];

/// A place in the text as Jison counts it: lines from 1, columns from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Place {
    pub(super) line: usize,
    pub(super) column: usize,
}

pub(super) struct Token {
    pub(super) kind: T,
    pub(super) text: Vec<u16>,
    /// Where the token starts in the text the lexer read.
    pub(super) start: usize,
    pub(super) first: Place,
    pub(super) last: Place,
}

/// The lexer's start conditions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    String,
    MdString,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Dir,
    Text,
    EllipseText,
    TrapText,
    EdgeText,
    Click,
    CallbackName,
    CallbackArgs,
    ShapeData,
    ShapeDataStr,
}

/// A code unit as `charCodeAt` reads it. Past the end it is `NaN`, which equals nothing.
type Code = u32;
const NAN: Code = u32::MAX;

const fn code(c: u8) -> Code {
    c as Code
}

fn is_space(c: Code) -> bool {
    u16::try_from(c).is_ok_and(space_unit)
}

fn is_word(c: Code) -> bool {
    u16::try_from(c).is_ok_and(word_unit)
}

/// `'A'..'Z' 'a'..'z' '0'..'9'` and ``!"#$%&'*+.`?\_/``.
fn is_node_char(c: Code) -> bool {
    u8::try_from(c).is_ok_and(|c| c.is_ascii_alphanumeric() || b"!\"#$%&'*+.`?\\_/".contains(&c))
}

fn is_unicode_letter(c: Code) -> bool {
    u16::try_from(c).is_ok_and(unicode_letter_unit)
}

struct Lexer<'a> {
    src: &'a [u16],
    n: usize,
    tokens: Vec<Token>,
    stack: Vec<S>,
    state: S,
    p: usize,
    first_graph: bool,
    dir_next: Option<usize>,
    at_next: Option<usize>,
    line_end: Option<usize>,
    dir_failed: usize,
    run_end: usize,
    run_at: Option<usize>,
    /// The last whitespace run measured: from where, to the first unit after it.
    ws: Option<(usize, usize)>,
    /// The same for the last run of spaces and tabs.
    blank: Option<(usize, usize)>,
    /// Where the last search for a link found none.
    no_link: Option<usize>,
    /// Where the last search for the arrow that ends an edge's text found none.
    no_arrow: Option<usize>,
    line: usize,
    column: usize,
}

/// `\s*` from `q`.
fn spaces(src: &[u16], q: usize) -> usize {
    src.run(q, space_unit)
}

fn at(src: &[u16], q: usize) -> Code {
    src.get(q).map_or(NAN, |&c| Code::from(c))
}

/// `(?:\r?\n)*\s*\n`: whitespace up to and including its last line feed.
fn no_direction(src: &[u16], p: usize) -> Option<usize> {
    let end = spaces(src, p);
    (p..end).rev().find(|&q| src[q] == 10).map(|q| q + 1)
}

/// `\s*(?:LR|RL|TB|BT|TD|BR)\b`
fn direction_word(src: &[u16], p: usize) -> Option<usize> {
    let q = spaces(src, p);
    let found = ["LR", "RL", "TB", "BT", "TD", "BR"].iter().any(|word| src.starts_at(q, word));
    (found && !is_word(at(src, q + 2))).then_some(q + 2)
}

/// `\s*[<>^]`
fn direction_symbol(src: &[u16], p: usize) -> Option<usize> {
    let q = spaces(src, p);
    matches!(at(src, q), 60 | 62 | 94).then_some(q + 1)
}

/// `\s*v\b`
fn direction_v(src: &[u16], p: usize) -> Option<usize> {
    let q = spaces(src, p);
    (at(src, q) == code(b'v') && !is_word(at(src, q + 1))).then_some(q + 1)
}

/// `.*direction\s+XX[^\n]*`: the last `direction` on the line from `p` that is followed by
/// whitespace and `XX`, through to the next line feed.
fn direction_statement(src: &[u16], p: usize, word: &str) -> Option<usize> {
    let line_end = src.run(p, |c| !crate::input::is_line_end(c));
    (p..line_end).rev().find_map(|k| {
        if !src.starts_at(k, "direction") {
            return None;
        }
        let gap = spaces(src, k + 9);
        (gap > k + 9 && src.starts_at(gap, word)).then(|| src.find_unit(10, gap + 2).unwrap_or(src.len()))
    })
}

/// `name\s*:\s*`, `name\s*\{\s*` and their kin: `word`, then whitespace, then `mark`, then whitespace.
fn word_then(src: &[u16], p: usize, word: &str, mark: u8) -> Option<usize> {
    if !src.starts_at(p, word) {
        return None;
    }
    let q = spaces(src, p + word.len());
    (at(src, q) == code(mark)).then(|| spaces(src, q + 1))
}

/// `word\s+`
fn word_spaced(src: &[u16], p: usize, word: &str) -> Option<usize> {
    let q = p + word.len();
    let end = spaces(src, q);
    (src.starts_at(p, word) && end > q).then_some(end)
}

/// `\(\s*\)`
fn empty_call(src: &[u16], p: usize) -> Option<usize> {
    let q = spaces(src, p + 1);
    (at(src, p) == code(b'(') && at(src, q) == code(b')')).then_some(q + 1)
}

/// `text.replace(/\n\s*/g, '<br/>')`
fn line_breaks(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut q = 0;
    while q < text.len() {
        if text[q] == 10 {
            out.extend("<br/>".encode_utf16());
            q = spaces(text, q + 1);
        } else {
            out.push(text[q]);
            q += 1;
        }
    }
    out
}

impl<'a> Lexer<'a> {
    fn c(&self, q: usize) -> Code {
        at(self.src, q)
    }

    fn place(&self) -> Place {
        Place { line: self.line, column: self.column }
    }

    /// Moves over one match. Jison counts `\r\n`, `\r` and `\n` as one line break each, and
    /// after a break its column stops at the first U+2028 or U+2029.
    fn advance(&mut self, e: usize) {
        let mut brk = None;
        let mut k = self.p;
        while k < e {
            let ch = self.src[k];
            if ch == 10 {
                self.line += 1;
                brk = Some(k);
            } else if ch == 13 {
                if k + 1 < e && self.src[k + 1] == 10 {
                    k += 1;
                }
                self.line += 1;
                brk = Some(k);
            }
            k += 1;
        }
        match brk {
            None => self.column += e - self.p,
            Some(brk) => {
                let end = self.src[..e].run(brk + 1, |ch| ch != 0x2028 && ch != 0x2029);
                self.column = end - brk - 1;
            }
        }
        self.p = e;
    }

    /// Records a token that started at `start` and `first`, and ends at the cursor.
    fn token(&mut self, kind: T, text: Vec<u16>, start: usize, first: Place) {
        let last = self.place();
        self.tokens.push(Token { kind, text, start, first, last });
    }

    /// A token that ends at `e`, with the text `text` where that is not the text it spans.
    fn emit_as(&mut self, kind: T, text: Option<Vec<u16>>, e: usize) {
        let (start, first) = (self.p, self.place());
        let text = text.unwrap_or_else(|| self.src[start..e].to_vec());
        self.advance(e);
        self.token(kind, text, start, first);
    }

    fn emit(&mut self, kind: T, e: usize) {
        self.emit_as(kind, None, e);
    }

    /// For a token that cannot hold a line break.
    fn emit_flat(&mut self, kind: T, e: usize) {
        let (start, first) = (self.p, self.place());
        let text = self.src[start..e].to_vec();
        self.column += e - start;
        self.p = e;
        self.token(kind, text, start, first);
    }

    /// For a run that Jison matches one character at a time, where `\r\n` counts as two line
    /// breaks.
    fn emit_chars(&mut self, kind: T, e: usize) {
        let (start, first) = (self.p, self.place());
        let text = self.src[start..e].to_vec();
        for &ch in &text {
            if ch == 10 || ch == 13 {
                self.line += 1;
                self.column = 0;
            } else {
                self.column += 1;
            }
        }
        self.p = e;
        self.token(kind, text, start, first);
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

    fn kw(&self, word: &str) -> bool {
        self.src.starts_at(self.p, word) && !is_word(self.c(self.p + word.len()))
    }

    /// The end of the run of whitespace at `q`. The last run is kept, so asking again from
    /// inside it costs nothing.
    fn ws_end(&mut self, q: usize) -> usize {
        if let Some((from, to)) = self.ws
            && q >= from
            && q <= to
        {
            return to;
        }
        let to = spaces(self.src, q);
        self.ws = Some((q, to));
        to
    }

    /// The same for a run of spaces and tabs.
    fn blank_end(&mut self, q: usize) -> usize {
        if let Some((from, to)) = self.blank
            && q >= from
            && q <= to
        {
            return to;
        }
        let to = self.src.run(q, |ch| ch == 32 || ch == 9);
        self.blank = Some((q, to));
        to
    }

    /// The end of `\s*--+[x>]\s*` at `q`.
    fn arrow_end(&mut self, q: usize) -> Option<usize> {
        let j = self.ws_end(q);
        if self.no_arrow == Some(j) || self.c(j) != code(b'-') || self.c(j + 1) != code(b'-') {
            return None;
        }
        let k = self.src.run(j + 2, |ch| ch == u16::from(b'-'));
        if matches!(self.c(k), 120 | 62) {
            return Some(self.ws_end(k + 1));
        }
        self.no_arrow = Some(j);
        None
    }

    /// A whole link, `\s*--+[x>]\s*` or `\s*-\.+-\s*`, or the `\s*--\s*` that opens one with text.
    fn link(&mut self) -> bool {
        let j = self.ws_end(self.p);
        if self.no_link == Some(j) || self.c(j) != code(b'-') {
            return false;
        }
        let d = self.c(j + 1);
        if d == code(b'-') {
            let k = self.src.run(j + 2, |ch| ch == u16::from(b'-'));
            if matches!(self.c(k), 120 | 62) {
                let e = self.ws_end(k + 1);
                self.emit(T::Link, e);
            } else {
                self.push(S::EdgeText);
                let e = self.ws_end(j + 2);
                self.emit(T::StartLink, e);
            }
            return true;
        }
        if d == code(b'.') {
            let k = self.src.run(j + 2, |ch| ch == u16::from(b'.'));
            if self.c(k) == code(b'-') {
                let e = self.ws_end(k + 1);
                self.emit(T::Link, e);
                return true;
            }
        }
        self.no_link = Some(j);
        false
    }

    /// `%%` not followed by `{`, through the end of the line.
    fn comment_end(&self, q: usize) -> Option<usize> {
        if self.c(q) != code(b'%') || self.c(q + 1) != code(b'%') || self.c(q + 2) == code(b'{') {
            return None;
        }
        Some(self.src.find_unit(10, q + 2).unwrap_or(self.n))
    }

    fn quote(&mut self) {
        if self.c(self.p + 1) == code(b'`') {
            self.push(S::MdString);
            self.advance(self.p + 2);
        } else {
            self.push(S::String);
            self.advance(self.p + 1);
        }
    }

    fn opener(&mut self, c: Code) -> bool {
        let p = self.p;
        let d = self.c(p + 1);
        let (state, kind, length) = match c {
            40 if d == code(b'-') => (S::EllipseText, T::EllipseStart, 2),
            40 if d == code(b'[') => (S::Text, T::StadiumStart, 2),
            40 if d == code(b'(') && self.c(p + 2) == code(b'(') => (S::Text, T::DoubleCircleStart, 3),
            40 => (S::Text, T::Ps, 1),
            91 if d == code(b'[') => (S::Text, T::SubroutineStart, 2),
            91 if d == code(b'(') => (S::Text, T::CylinderStart, 2),
            91 if d == code(b'/') => (S::TrapText, T::TrapStart, 2),
            91 if d == code(b'\\') => (S::TrapText, T::InvTrapStart, 2),
            91 => (S::Text, T::Sqs, 1),
            124 => (S::Text, T::Pipe, 1),
            123 => (S::Text, T::DiamondStart, 1),
            _ => return false,
        };
        self.push(state);
        self.emit_flat(kind, p + length);
        true
    }

    fn node_string(&mut self) -> bool {
        let mut q = self.p;
        while q < self.n {
            let ch = self.c(q);
            if is_node_char(ch) {
                q += 1;
            } else if ch == code(b'-') && q + 1 < self.n {
                if matches!(self.c(q + 1), 62 | 45 | 46) {
                    break;
                }
                q += 1;
            } else {
                break;
            }
        }
        if q == self.p {
            return false;
        }
        self.emit_flat(T::NodeString, q);
        true
    }

    /// The end of the spaces and tabs at `from`.
    fn blanks(&self, from: usize) -> usize {
        self.src.run(from, |ch| ch == 32 || ch == 9)
    }

    fn keyword(&mut self, c: Code) -> bool {
        let (src, p) = (self.src, self.p);
        match c {
            97 => {
                if src.starts_at(p, "acc") {
                    if let Some(e) = word_then(src, p, "accTitle", b':') {
                        self.push(S::AccTitle);
                        self.emit(T::AccTitle, e);
                        return true;
                    }
                    if let Some(e) = word_then(src, p, "accDescr", b':') {
                        self.push(S::AccDescr);
                        self.emit(T::AccDescr, e);
                        return true;
                    }
                    if let Some(e) = word_then(src, p, "accDescr", b'{') {
                        self.push(S::AccDescrMultiline);
                        self.advance(e);
                        return true;
                    }
                }
                if !self.kw("agentflow-beta") {
                    return false;
                }
                if self.first_graph {
                    self.first_graph = false;
                    self.push(S::Dir);
                }
                self.emit_flat(T::Graph, p + 14);
            }
            64 if self.c(p + 1) == code(b'{') => {
                self.push(S::ShapeData);
                self.emit_as(T::ShapeData, Some(Vec::new()), p + 2);
            }
            99 => {
                if let Some(e) = word_spaced(src, p, "call") {
                    self.push(S::CallbackName);
                    self.advance(e);
                } else if self.kw("classDef") {
                    self.emit_flat(T::ClassDef, p + 8);
                } else if self.kw("class") {
                    self.emit_flat(T::Class, p + 5);
                } else if let Some(e) = word_spaced(src, p, "click") {
                    self.push(S::Click);
                    self.advance(e);
                } else if self.kw("connector") {
                    self.emit_flat(T::Connector, p + 9);
                } else {
                    return false;
                }
            }
            34 => self.quote(),
            115 if self.kw("style") => self.emit_flat(T::Style, p + 5),
            100 if self.kw("default") => self.emit_flat(T::Default, p + 7),
            108 if self.kw("linkStyle") => self.emit_flat(T::LinkStyle, p + 9),
            105 if self.kw("interpolate") => self.emit_flat(T::Interpolate, p + 11),
            104 if src.starts_at(p, "href") && is_space(self.c(p + 4)) => self.emit(T::Href, p + 5),
            102 if self.kw("flow") => self.emit_flat(T::Flow, p + 4),
            103 if self.kw("global") => {
                let e = self.blanks(p + 6);
                self.emit_flat(T::Global, e);
            }
            101 if self.kw("end") => {
                let e = self.blanks(p + 3);
                self.emit_flat(T::EndKeyword, e);
            }
            95 => {
                let Some(word) = ["_self", "_blank", "_parent", "_top"].into_iter().find(|word| self.kw(word)) else {
                    return false;
                };
                self.emit_flat(T::LinkTarget, p + word.len());
            }
            _ => return false,
        }
        true
    }

    fn direction(&mut self) -> bool {
        let Some(mut next) = self.dir_next else {
            return false;
        };
        if self.p < self.dir_failed {
            return false;
        }
        let p = self.p;
        if next < p {
            self.dir_next = self.src.find_str("direction", p);
            match self.dir_next {
                Some(found) => next = found,
                None => return false,
            }
        }
        if self.line_end.is_none_or(|end| end < p) {
            self.line_end = Some(self.src.run(p, |c| !crate::input::is_line_end(c)));
        }
        let line_end = self.line_end.expect("measured above");
        if next >= line_end {
            return false;
        }
        for (kind, word) in DIRECTION_STATEMENTS {
            if let Some(e) = direction_statement(self.src, p, word) {
                self.emit(kind, e);
                return true;
            }
        }
        // No direction statement starts anywhere in the rest of this line either.
        self.dir_failed = line_end;
        false
    }

    /// A link id is a run of non-space characters up to its last `@` that is not followed by
    /// `{` or `"`.
    fn link_id(&mut self, c: Code) -> bool {
        if self.at_next.is_none() || c == code(b'"') || is_space(c) {
            return false;
        }
        let p = self.p;
        if p >= self.run_end {
            if self.at_next.is_some_and(|next| next < p) {
                self.at_next = self.src.find_unit(u16::from(b'@'), p);
            }
            let q = self.src.run(p, |ch| ch != u16::from(b'"') && !space_unit(ch));
            self.run_end = q;
            self.run_at = None;
            if let Some(next) = self.at_next.filter(|&next| next < q) {
                let last = q.min(self.n - 1);
                self.run_at =
                    (next..last).rev().find(|&k| self.src[k] == u16::from(b'@') && !matches!(self.c(k + 1), 123 | 34));
            }
        }
        match self.run_at {
            Some(k) if k > p => {
                self.emit_flat(T::LinkId, k + 1);
                true
            }
            _ => false,
        }
    }

    fn initial(&mut self, c: Code) -> bool {
        if self.keyword(c) || self.direction() || self.link_id(c) {
            return true;
        }
        let p = self.p;
        if (48..=57).contains(&c) {
            let q = self.src.run(p + 1, |ch| (48..=57).contains(&ch));
            self.emit_flat(T::Num, q);
            return true;
        }
        match c {
            35 => self.emit_flat(T::Brkt, p + 1),
            58 if self.c(p + 1) == code(b':') && self.c(p + 2) == code(b':') => {
                self.emit_flat(T::StyleSeparator, p + 3)
            }
            58 => self.emit_flat(T::Colon, p + 1),
            38 => self.emit_flat(T::Amp, p + 1),
            59 => self.emit_flat(T::Semi, p + 1),
            44 => self.emit_flat(T::Comma, p + 1),
            42 => self.emit_flat(T::Mult, p + 1),
            60 => self.emit_flat(T::TagStart, p + 1),
            45 => {
                if !self.link() && !self.node_string() {
                    self.emit_flat(T::Minus, p + 1);
                }
            }
            40 => return self.opener(c),
            91 if self.c(p + 1) == code(b'|') => self.emit_flat(T::VertexWithPropsStart, p + 2),
            91 | 124 | 123 => return self.opener(c),
            62 => {
                self.push(S::Text);
                self.emit_flat(T::TagEnd, p + 1);
            }
            94 => self.emit_flat(T::Up, p + 1),
            92 if self.c(p + 1) == code(b'|') => self.emit_flat(T::Sep, p + 2),
            92 => return self.node_string(),
            118 if !is_word(self.c(p + 1)) => self.emit_flat(T::Down, p + 1),
            118 => return self.node_string(),
            37 => match self.comment_end(p) {
                Some(e) => self.emit(T::Comment, e),
                None => return self.node_string(),
            },
            _ if is_space(c) => {
                if self.link() {
                    return true;
                }
                if c == 32 || c == 9 {
                    let blank = self.blank_end(p);
                    if let Some(e) = self.comment_end(blank) {
                        self.emit(T::Comment, e);
                        return true;
                    }
                }
                // `(?:\r?\n)+`, or one whitespace character.
                let mut e = p;
                loop {
                    if self.c(e) == 10 {
                        e += 1;
                    } else if self.c(e) == 13 && self.c(e + 1) == 10 {
                        e += 2;
                    } else {
                        break;
                    }
                }
                if e > p {
                    self.emit(T::Newline, e);
                } else {
                    self.emit(T::Space, p + 1);
                }
            }
            _ if c < 128 => return self.node_string(),
            _ if is_unicode_letter(c) => self.emit_flat(T::UnicodeText, p + 1),
            _ => return false,
        }
        true
    }

    /// The first of `stops` after the character at the cursor, or the end of the text.
    fn until(&self, stops: &[u8]) -> usize {
        (self.p + 1..self.n).find(|&q| u8::try_from(self.src[q]).is_ok_and(|ch| stops.contains(&ch))).unwrap_or(self.n)
    }

    /// A comment inside a label, which is dropped. Jison leaves the lone `%` out of Mermaid's
    /// text pattern, so only a comment may start with one.
    fn skip_comment(&mut self) -> bool {
        match self.comment_end(self.p) {
            Some(e) => {
                self.advance(e);
                true
            }
            None => false,
        }
    }

    fn text(&mut self, c: Code) -> bool {
        let p = self.p;
        let d = self.c(p + 1);
        match c {
            34 => self.quote(),
            40 | 91 | 123 => {
                self.opener(c);
            }
            93 => {
                self.pop();
                match d {
                    41 => self.emit_flat(T::StadiumEnd, p + 2),
                    93 => self.emit_flat(T::SubroutineEnd, p + 2),
                    _ => self.emit_flat(T::Sqe, p + 1),
                }
            }
            41 => {
                self.pop();
                if d == code(b']') {
                    self.emit_flat(T::CylinderEnd, p + 2);
                } else if d == code(b')') && self.c(p + 2) == code(b')') {
                    self.emit_flat(T::DoubleCircleEnd, p + 3);
                } else {
                    self.emit_flat(T::Pe, p + 1);
                }
            }
            124 => {
                self.pop();
                self.emit_flat(T::Pipe, p + 1);
            }
            125 => {
                self.pop();
                self.emit_flat(T::DiamondStop, p + 1);
            }
            37 => return self.skip_comment(),
            _ => {
                let q = self.until(b"[](){}|\"%");
                self.emit(T::Text, q);
            }
        }
        true
    }

    fn ellipse_text(&mut self, c: Code) -> bool {
        let p = self.p;
        let d = self.c(p + 1);
        if c == code(b'"') {
            self.quote();
        } else if c == code(b'%') && self.skip_comment() {
        } else if matches!(c, 45 | 47 | 41) && d == code(b')') {
            self.pop();
            self.emit_flat(T::EllipseEnd, p + 2);
        } else if matches!(c, 40 | 91 | 123) {
            self.opener(c);
        } else if matches!(c, 41 | 93 | 125) {
            return false;
        } else {
            let mut q = p + 1;
            while q < self.n {
                let ch = self.c(q);
                if matches!(ch, 34 | 40 | 41 | 91 | 93 | 123 | 125) {
                    break;
                }
                if matches!(ch, 45 | 47) && self.c(q + 1) == code(b')') {
                    break;
                }
                if ch == code(b'%') && self.c(q + 1) == code(b'%') && self.c(q + 2) != code(b'{') {
                    break;
                }
                q += 1;
            }
            self.emit_chars(T::Text, q);
        }
        true
    }

    fn trap_text(&mut self, c: Code) -> bool {
        let p = self.p;
        let d = self.c(p + 1);
        match c {
            34 => self.quote(),
            40 if d == code(b']') => {
                self.pop();
                self.emit_flat(T::TrapEnd, p + 2);
            }
            40 | 91 | 123 => {
                self.opener(c);
            }
            92 | 93 | 41 if d == code(b']') => {
                self.pop();
                self.emit_flat(T::TrapEnd, p + 2);
            }
            92 => self.emit_flat(T::Text, p + 1),
            47 if d == code(b']') => {
                self.pop();
                self.emit_flat(T::InvTrapEnd, p + 2);
            }
            47 => self.emit_flat(T::Text, p + 1),
            93 | 41 | 125 => return false,
            37 => return self.skip_comment(),
            63 | 61 if d == code(b']') => {
                self.pop();
                self.emit_flat(T::TrapEnd, p + 2);
            }
            _ => {
                let q = self.until(b"\\[](){}/%");
                self.emit(T::Text, q);
            }
        }
        true
    }

    /// Edge text up to the arrow that closes it.
    fn edge_text(&mut self, c: Code) -> bool {
        if c == code(b'"') {
            self.quote();
            return true;
        }
        let p = self.p;
        if let Some(e) = self.arrow_end(p) {
            self.pop();
            self.emit(T::Link, e);
            return true;
        }
        if c == code(b'-') && self.c(p + 1) == code(b'-') {
            return false;
        }
        let mut q = p + 1;
        while q < self.n {
            let ch = self.c(q);
            if ch == code(b'"') {
                break;
            }
            if ch == code(b'-') {
                if self.c(q + 1) == code(b'-') {
                    break;
                }
            } else if is_space(ch) && self.arrow_end(q).is_some() {
                break;
            }
            q += 1;
        }
        self.emit_chars(T::EdgeText, q);
        true
    }

    fn step(&mut self) -> bool {
        let p = self.p;
        let c = self.c(p);
        match self.state {
            S::Initial => return self.initial(c),
            S::String => {
                if c == code(b'"') {
                    if self.c(p + 1) == code(b'`') {
                        self.push(S::MdString);
                        self.advance(p + 2);
                    } else {
                        self.pop();
                        self.advance(p + 1);
                    }
                } else {
                    let e = self.src.find_unit(u16::from(b'"'), p).unwrap_or(self.n);
                    self.emit(T::Str, e);
                }
            }
            S::MdString => {
                if c == code(b'`') {
                    if self.c(p + 1) != code(b'"') {
                        return false;
                    }
                    self.pop();
                    self.advance(p + 2);
                } else if c == code(b'"') {
                    self.quote();
                } else {
                    let q = self.until(b"`\"");
                    self.emit(T::MdStr, q);
                }
            }
            S::AccTitle | S::AccDescr => {
                let e = self.src.find_unit(10, p).unwrap_or(self.n);
                let kind = if self.state == S::AccTitle { T::AccTitleValue } else { T::AccDescrValue };
                self.pop();
                self.emit(kind, e);
            }
            S::AccDescrMultiline => {
                if c == code(b'}') {
                    self.pop();
                    self.advance(p + 1);
                } else {
                    let e = self.src.find_unit(u16::from(b'}'), p).unwrap_or(self.n);
                    self.emit(T::AccDescrMultilineValue, e);
                }
            }
            S::Dir => {
                if c == code(b'"') {
                    self.quote();
                } else if let Some(e) = no_direction(self.src, p) {
                    self.pop();
                    self.emit(T::NoDir, e);
                } else if let Some(e) = direction_word(self.src, p)
                    .or_else(|| direction_symbol(self.src, p))
                    .or_else(|| direction_v(self.src, p))
                {
                    self.pop();
                    self.emit(T::Dir, e);
                } else {
                    return self.opener(c);
                }
            }
            S::Text => return self.text(c),
            S::EllipseText => return self.ellipse_text(c),
            S::TrapText => return self.trap_text(c),
            S::EdgeText => return self.edge_text(c),
            S::Click => {
                if c == code(b'"') {
                    self.quote();
                } else if is_space(c) {
                    self.pop();
                    self.advance(p + 1);
                } else {
                    let q = self.src.run(p + 1, |ch| !space_unit(ch));
                    self.emit_flat(T::Click, q);
                }
            }
            S::CallbackName => {
                if c == code(b'(') {
                    let e = empty_call(self.src, p);
                    self.pop();
                    match e {
                        Some(e) => self.advance(e),
                        None => {
                            self.push(S::CallbackArgs);
                            self.advance(p + 1);
                        }
                    }
                } else {
                    let e = self.src.find_unit(u16::from(b'('), p).unwrap_or(self.n);
                    self.emit(T::CallbackName, e);
                }
            }
            S::CallbackArgs => {
                if c == code(b')') {
                    self.pop();
                    self.advance(p + 1);
                } else {
                    let e = self.src.find_unit(u16::from(b')'), p).unwrap_or(self.n);
                    self.emit(T::CallbackArgs, e);
                }
            }
            S::ShapeData => {
                if c == code(b'"') {
                    self.push(S::ShapeDataStr);
                    self.emit_flat(T::ShapeData, p + 1);
                } else if c == code(b'}') {
                    self.pop();
                    self.advance(p + 1);
                } else if c == code(b'^') {
                    return false;
                } else {
                    let q = self.until(b"}^\"");
                    self.emit(T::ShapeData, q);
                }
            }
            S::ShapeDataStr => {
                if c == code(b'"') {
                    self.pop();
                    self.emit_flat(T::ShapeData, p + 1);
                } else {
                    let e = self.src.find_unit(u16::from(b'"'), p).unwrap_or(self.n);
                    self.emit_as(T::ShapeData, Some(line_breaks(&self.src[p..e])), e);
                }
            }
        }
        true
    }

    /// The token the input ends with in the current state, if the state has one.
    fn end_token(&self) -> Option<T> {
        match self.state {
            S::Initial => Some(T::Eof),
            S::AccTitle => Some(T::AccTitleValue),
            S::AccDescr => Some(T::AccDescrValue),
            S::AccDescrMultiline => Some(T::AccDescrMultilineValue),
            S::Click => Some(T::Click),
            S::CallbackName => Some(T::CallbackName),
            S::CallbackArgs => Some(T::CallbackArgs),
            _ => None,
        }
    }

    /// A token of no text at the cursor.
    fn mark(&mut self, kind: T) {
        let place = self.place();
        self.tokens.push(Token { kind, text: Vec::new(), start: self.p, first: place, last: place });
    }
}

/// Pele's `tokenize`. The last token is `End`, or `Error` where no rule matches.
pub(super) fn tokenize(src: &[u16]) -> Vec<Token> {
    let mut lexer = Lexer {
        src,
        n: src.len(),
        tokens: Vec::new(),
        stack: Vec::new(),
        state: S::Initial,
        p: 0,
        first_graph: true,
        dir_next: src.find_str("direction", 0),
        at_next: src.find_unit(u16::from(b'@'), 0),
        line_end: None,
        dir_failed: 0,
        run_end: 0,
        run_at: None,
        ws: None,
        blank: None,
        no_link: None,
        no_arrow: None,
        line: 1,
        column: 0,
    };
    loop {
        if lexer.p >= lexer.n {
            if let Some(kind) = lexer.end_token() {
                lexer.emit_flat(kind, lexer.n);
            }
            break;
        }
        if !lexer.step() {
            lexer.mark(T::Error);
            return lexer.tokens;
        }
    }
    lexer.mark(T::End);
    lexer.tokens
}
