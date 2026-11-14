//! The flowchart lexer (Pele's `src/diagrams/flowchart/lexer.ts`), which reproduces the token
//! stream of Mermaid's `flow.jison` lexer. Each scanner names the sticky regular expression it
//! answers for, and the lexer states and caches follow Pele's one for one.

use crate::input::{Units, is_space as space_unit, is_word as word_unit};
use std::sync::LazyLock;

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
    Subgraph => "subgraph",
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

pub(super) struct Token {
    pub(super) kind: T,
    pub(super) text: Vec<u16>,
    /// Where the token starts in the text the lexer read.
    pub(super) start: usize,
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
    ThickEdgeText,
    DottedEdgeText,
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

/// Pele's `isUnicodeLetter` (`src/util/unicode.ts`): the code units of Mermaid's
/// `UNICODE_TEXT` pattern, as ranges packed in base 36.
fn is_unicode_letter(c: Code) -> bool {
    static TABLE: LazyLock<Vec<(Code, Code)>> = LazyLock::new(|| {
        let parts: Vec<Code> =
            PACKED.split(' ').map(|part| Code::from_str_radix(part, 36).expect("the table is base 36")).collect();
        let mut previous = 0;
        parts
            .chunks(2)
            .map(|pair| {
                let start = previous + pair[0];
                previous = start + pair[1];
                (start, previous)
            })
            .collect()
    });
    if !(0xaa..=0xffdc).contains(&c) {
        return false;
    }
    TABLE
        .binary_search_by(|&(start, end)| {
            if c < start {
                std::cmp::Ordering::Greater
            } else if c > end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

const PACKED: &str = "4q 0 b 0 5 0 6 m 2 u 2 cp 5 b f 4 8 0 2 0 3m 4 2 1 3 3 9 0 2 2 2 0 2 j 2 2a 2 3u 9 4d a 11 3 0 8 12 21 q 6 2 1a 16 10 1 2 2q 2 0 g 1 8 1 b 2 3 0 h 0 2 t u 2g c 0 p w a 1 5 0 6 l 5 0 a 0 4 0 o o 20 0 2 a 2g 1h 4 0 j 0 8 9 g 6 2 6 6 7 3 1 3 l 2 6 2 0 4 3 4 0 h 0 e 1 2 2 f 1 k 5 5 1 3 l 2 6 2 1 2 1 2 1 w 3 2 0 k 2 h 8 2 2 2 l 2 6 2 1 2 4 4 0 j 0 g 1 10 7 3 1 3 l 2 6 2 1 2 4 4 0 v 1 2 2 g 0 i 0 2 5 4 2 2 3 4 1 2 0 2 1 4 1 4 2 4 b n 0 1h 7 2 2 2 m 2 9 2 4 4 0 r 1 7 1 10 7 2 2 2 m 2 9 2 4 4 0 x 0 2 1 g 1 j 7 2 2 2 14 3 0 h 0 i 1 p 5 6 h 4 n 2 8 2 0 3 6 1n 1b 2 1 d 6 1n 1 2 0 3 1 2 0 3 0 7 3 2 6 2 2 2 0 2 0 3 1 2 3 2 1 a 0 3 4 2 0 m 3 x 0 1s 7 2 z s 4 38 16 l 0 h 5 5 3 4 0 4 1 8 2 5 c d 0 i 11 2 0 6 0 3 16 2 98 2 3 3 6 2 0 2 3 3 14 2 3 3 w 2 3 3 6 2 0 2 3 3 e 2 1k 2 3 3 1u 12 f h 2c d h7 3 g 2 p 6 22 m c 2 3 f h f h f c 2 2 g 1f 10 0 5 0 1w 2f 9 14 2 0 6 1x b s 1g t 3 4 c 17 m 6 1l m a 1g 2b 0 2m 1a i 6 1k t e 1 b 17 r z 16 2 b z 30 3 2 3 4 1 a 5b 1t 7p 3 5 3 11 3 5 3 7 2 0 2 0 2 0 2 u 3 1g 2 6 2 0 4 2 2 6 4 3 3 5 5 c 6 2 2 6 39 0 e 0 h c 2u 0 5 0 3 9 2 0 4 4 7 0 2 0 2 0 2 3 2 a 3 3 6 4 5 0 1h 1 22k 1a 2 1a 2 3o 7 3 4 1 d 11 2 0 6 0 3 1j 8 0 h m a 6 2 6 2 6 2 6 2 6 2 6 2 6 2 6 29 0 d2 1 17 4 6 1 5 2d 7 2 2 2h 2 3 6 14 4 2l i q 1i f e9 52t 23 g5o 1g wc 1w 19 3 7g 4 f b 1 l 1a h o 9 1x 1e 8 3 2u 3 3 2 3 d a 26 9 2 2 2 3 2 m u 1f f 1d 1r 5 4 0 f r b m q s 8 1a t 0 1d 14 o 2 2 7 l m 4 0 6 1b 2 0 4 1 3 4 3 0 2 0 p 2 3 a 8 2 d 5 3 5 3 5 a 6 2 6 42 y u 8mb d m 5 1c 6it a5 3 2x 13 6 d 4 6 0 2 9 2 c 2 4 2 0 2 1 2 1 2 2z y a2 j 1r 3 1h 15 b 39 4 2 3q 11 p 7 p c 2g 4 5 3 5 3 5 3 2";

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
}

/// `\s*` from `q`.
fn spaces(src: &[u16], q: usize) -> usize {
    src.run(q, space_unit)
}

fn at(src: &[u16], q: usize) -> Code {
    src.get(q).map_or(NAN, |&c| Code::from(c))
}

/// `[xo<]?` at `q`.
fn arrow_tail(src: &[u16], q: usize) -> usize {
    q + usize::from(matches!(at(src, q), 120 | 111 | 60))
}

/// `\s*[xo<]?CC+[Cxo>]\s*` for the line character `C`: `--+[-xo>]` or `==+[=xo>]`.
fn full_link(src: &[u16], p: usize, line: u16) -> Option<usize> {
    let q = arrow_tail(src, spaces(src, p));
    let after = src.run(q, |c| c == line);
    let count = after - q;
    if count < 2 {
        return None;
    }
    let end = if matches!(at(src, after), 120 | 111 | 62) {
        after + 1
    } else if count >= 3 {
        after
    } else {
        return None;
    };
    Some(spaces(src, end))
}

/// `\s*[xo<]?CC\s*`: `--` or `==` that opens a link with text.
fn start_link(src: &[u16], p: usize, line: u16) -> Option<usize> {
    let q = arrow_tail(src, spaces(src, p));
    (src.at(q) == Some(line) && src.at(q + 1) == Some(line)).then(|| spaces(src, q + 2))
}

/// `\s*[xo<]?-?\.+-[xo>]?\s*`
fn dotted_link(src: &[u16], p: usize) -> Option<usize> {
    let mut q = arrow_tail(src, spaces(src, p));
    if at(src, q) == code(b'-') {
        q += 1;
    }
    let dots = src.run(q, |c| c == u16::from(b'.'));
    if dots == q || at(src, dots) != code(b'-') {
        return None;
    }
    let end = dots + 1;
    Some(spaces(src, end + usize::from(matches!(at(src, end), 120 | 111 | 62))))
}

/// `\s*[xo<]?-\.\s*`
fn dotted_start(src: &[u16], p: usize) -> Option<usize> {
    let q = arrow_tail(src, spaces(src, p));
    (at(src, q) == code(b'-') && at(src, q + 1) == code(b'.')).then(|| spaces(src, q + 2))
}

/// `\s*~~~+\s*`
fn invisible_link(src: &[u16], p: usize) -> Option<usize> {
    let q = spaces(src, p);
    let end = src.run(q, |c| c == u16::from(b'~'));
    (end - q >= 3).then(|| spaces(src, end))
}

/// `(?:\r?\n)+`
fn newlines(src: &[u16], p: usize) -> Option<usize> {
    let mut q = p;
    loop {
        if at(src, q) == 13 && at(src, q + 1) == 10 {
            q += 2;
        } else if at(src, q) == 10 {
            q += 1;
        } else {
            break;
        }
    }
    (q > p).then_some(q)
}

/// `[^\S\n\r]+`
fn blank_run(src: &[u16], p: usize) -> Option<usize> {
    let end = src.run(p, |c| space_unit(c) && c != 10 && c != 13);
    (end > p).then_some(end)
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

    fn emit(&mut self, kind: T, s: usize, e: usize) {
        self.tokens.push(Token { kind, text: self.src[s..e].to_vec(), start: s });
        self.p = e;
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

    /// The end of the whitespace run at `q`, remembered for the next question about the same run.
    fn past_spaces(&mut self, q: usize) -> usize {
        if let Some((from, to)) = self.ws
            && q >= from
            && q <= to
        {
            return to;
        }
        if is_space(self.c(q)) {
            let to = spaces(self.src, q);
            self.ws = Some((q, to));
            return to;
        }
        q
    }

    /// The first character after the whitespace at `q`, skipping one optional `x`, `o` or `<`.
    fn link_char(&mut self, q: usize) -> Code {
        let j = self.past_spaces(q);
        let d = self.c(j);
        if matches!(d, 120 | 111 | 60) { self.c(j + 1) } else { d }
    }

    fn invisible_ahead(&mut self, q: usize) -> bool {
        let j = self.past_spaces(q);
        self.c(j) == code(b'~')
    }

    fn quote(&mut self) {
        if self.c(self.p + 1) == code(b'`') {
            self.push(S::MdString);
            self.p += 2;
        } else {
            self.push(S::String);
            self.p += 1;
        }
    }

    fn invisible(&mut self) -> bool {
        if !self.invisible_ahead(self.p) {
            return false;
        }
        match invisible_link(self.src, self.p) {
            Some(e) => {
                self.emit(T::Link, self.p, e);
                true
            }
            None => false,
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
        self.emit(kind, p, p + length);
        true
    }

    fn graph(&mut self, length: usize) {
        if self.first_graph {
            self.first_graph = false;
            self.push(S::Dir);
        }
        self.emit(T::Graph, self.p, self.p + length);
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
        self.emit(T::NodeString, self.p, q);
        true
    }

    fn try_emit(&mut self, kind: T, end: Option<usize>) -> bool {
        match end {
            Some(e) => {
                self.emit(kind, self.p, e);
                true
            }
            None => false,
        }
    }

    fn try_open(&mut self, state: S, end: Option<usize>) -> bool {
        match end {
            Some(e) => {
                self.push(state);
                self.emit(T::StartLink, self.p, e);
                true
            }
            None => false,
        }
    }

    fn link(&mut self, c: Code) -> bool {
        let (src, p) = (self.src, self.p);
        match self.link_char(p) {
            45 => {
                self.try_emit(T::Link, full_link(src, p, u16::from(b'-')))
                    || self.try_open(S::EdgeText, start_link(src, p, u16::from(b'-')))
                    || self.try_emit(T::Link, dotted_link(src, p))
                    || self.try_open(S::DottedEdgeText, dotted_start(src, p))
                    || (!matches!(c, 45 | 61 | 46) && self.invisible())
            }
            61 => {
                self.try_emit(T::Link, full_link(src, p, u16::from(b'=')))
                    || self.try_open(S::ThickEdgeText, start_link(src, p, u16::from(b'=')))
                    || (!matches!(c, 45 | 61 | 46) && self.invisible())
            }
            46 => self.try_emit(T::Link, dotted_link(src, p)) || (!matches!(c, 45 | 61 | 46) && self.invisible()),
            _ => !matches!(c, 45 | 61 | 46) && self.invisible(),
        }
    }

    fn keyword(&mut self, c: Code) -> bool {
        let (src, p) = (self.src, self.p);
        match c {
            97 if src.starts_at(p, "acc") => {
                if let Some(e) = word_then(src, p, "accTitle", b':') {
                    self.push(S::AccTitle);
                    self.emit(T::AccTitle, p, e);
                } else if let Some(e) = word_then(src, p, "accDescr", b':') {
                    self.push(S::AccDescr);
                    self.emit(T::AccDescr, p, e);
                } else if let Some(e) = word_then(src, p, "accDescr", b'{') {
                    self.push(S::AccDescrMultiline);
                    self.p = e;
                } else {
                    return false;
                }
                true
            }
            64 if self.c(p + 1) == code(b'{') => {
                self.push(S::ShapeData);
                self.tokens.push(Token { kind: T::ShapeData, text: Vec::new(), start: p });
                self.p += 2;
                true
            }
            99 => {
                if let Some(e) = word_spaced(src, p, "call") {
                    self.push(S::CallbackName);
                    self.p = e;
                } else if self.kw("classDef") {
                    self.emit(T::ClassDef, p, p + 8);
                } else if self.kw("class") {
                    self.emit(T::Class, p, p + 5);
                } else if let Some(e) = word_spaced(src, p, "click") {
                    self.push(S::Click);
                    self.p = e;
                } else {
                    return false;
                }
                true
            }
            34 => {
                self.quote();
                true
            }
            115 => {
                if self.kw("style") {
                    self.emit(T::Style, p, p + 5);
                } else if self.kw("swimlane-beta") {
                    self.graph(13);
                } else if self.kw("subgraph") {
                    self.emit(T::Subgraph, p, p + 8);
                } else {
                    return false;
                }
                true
            }
            100 if self.kw("default") => {
                self.emit(T::Default, p, p + 7);
                true
            }
            108 if self.kw("linkStyle") => {
                self.emit(T::LinkStyle, p, p + 9);
                true
            }
            105 if self.kw("interpolate") => {
                self.emit(T::Interpolate, p, p + 11);
                true
            }
            104 if src.starts_at(p, "href") && is_space(self.c(p + 4)) => {
                self.emit(T::Href, p, p + 5);
                true
            }
            102 => {
                if self.kw("flowchart-elk") {
                    self.graph(13);
                } else if self.kw("flowchart") {
                    self.graph(9);
                } else {
                    return false;
                }
                true
            }
            103 if self.kw("graph") => {
                self.graph(5);
                true
            }
            101 if self.kw("end") => {
                let e = spaces(src, p + 3);
                self.emit(T::EndKeyword, p, e);
                true
            }
            95 => {
                let Some(word) = ["_self", "_blank", "_parent", "_top"].into_iter().find(|word| self.kw(word)) else {
                    return false;
                };
                self.emit(T::LinkTarget, p, p + word.len());
                true
            }
            _ => false,
        }
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
                self.emit(kind, p, e);
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
                self.emit(T::LinkId, p, k + 1);
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
            self.emit(T::Num, p, q);
            return true;
        }
        match c {
            35 => self.emit(T::Brkt, p, p + 1),
            58 if self.c(p + 1) == code(b':') && self.c(p + 2) == code(b':') => self.emit(T::StyleSeparator, p, p + 3),
            58 => self.emit(T::Colon, p, p + 1),
            38 => self.emit(T::Amp, p, p + 1),
            59 => self.emit(T::Semi, p, p + 1),
            44 => self.emit(T::Comma, p, p + 1),
            42 => self.emit(T::Mult, p, p + 1),
            120 | 111 => return self.link(c) || self.node_string(),
            60 => {
                if !self.link(c) {
                    self.emit(T::TagStart, p, p + 1);
                }
            }
            45 => {
                if !self.link(c) && !self.node_string() {
                    self.emit(T::Minus, p, p + 1);
                }
            }
            61 => return self.link(c),
            46 => return self.link(c) || self.node_string(),
            126 => return self.invisible(),
            40 => return self.opener(c),
            91 if self.c(p + 1) == code(b'|') => self.emit(T::VertexWithPropsStart, p, p + 2),
            91 | 124 | 123 => return self.opener(c),
            62 => {
                self.push(S::Text);
                self.emit(T::TagEnd, p, p + 1);
            }
            94 => self.emit(T::Up, p, p + 1),
            92 if self.c(p + 1) == code(b'|') => self.emit(T::Sep, p, p + 2),
            92 => return self.node_string(),
            118 if !is_word(self.c(p + 1)) => self.emit(T::Down, p, p + 1),
            118 => return self.node_string(),
            _ if is_space(c) => {
                return self.link(c)
                    || self.try_emit(T::Newline, newlines(self.src, p))
                    || self.try_emit(T::Space, blank_run(self.src, p));
            }
            _ if c < 128 => return self.node_string(),
            _ if is_unicode_letter(c) => self.emit(T::UnicodeText, p, p + 1),
            _ => return false,
        }
        true
    }

    /// Edge text up to the link that closes it. `a` is the line character and `b` the other one a
    /// dotted link may end in.
    fn edge_text(&mut self, c: Code, close: fn(&[u16], usize) -> Option<usize>, a: Code, b: Code) -> bool {
        if c == code(b'"') {
            self.quote();
            return true;
        }
        let p = self.p;
        let d = self.link_char(p);
        if (d == a || d == b)
            && let Some(e) = close(self.src, p)
        {
            self.pop();
            self.emit(T::Link, p, e);
            return true;
        }
        if c == a && (a != code(b'-') || self.c(p + 1) == code(b'-')) {
            return false;
        }
        let mut q = p + 1;
        while q < self.n {
            let ch = self.c(q);
            if ch == code(b'"') {
                break;
            }
            if ch == a {
                if a != code(b'-') || self.c(q + 1) == code(b'-') {
                    break;
                }
            } else if matches!(ch, 120 | 111 | 60) || ch == b || is_space(ch) {
                let dd = self.link_char(q);
                if (dd == a || dd == b) && close(self.src, q).is_some() {
                    break;
                }
            }
            q += 1;
        }
        self.emit(T::EdgeText, p, q);
        true
    }

    /// Text up to the first of `stops`.
    fn text_until(&mut self, kind: T, stops: &[u8]) {
        let p = self.p;
        let q =
            (p + 1..self.n).find(|&q| u8::try_from(self.src[q]).is_ok_and(|ch| stops.contains(&ch))).unwrap_or(self.n);
        self.emit(kind, p, q);
    }

    fn text(&mut self, c: Code) {
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
                    41 => self.emit(T::StadiumEnd, p, p + 2),
                    93 => self.emit(T::SubroutineEnd, p, p + 2),
                    _ => self.emit(T::Sqe, p, p + 1),
                }
            }
            41 => {
                self.pop();
                if d == code(b']') {
                    self.emit(T::CylinderEnd, p, p + 2);
                } else if d == code(b')') && self.c(p + 2) == code(b')') {
                    self.emit(T::DoubleCircleEnd, p, p + 3);
                } else {
                    self.emit(T::Pe, p, p + 1);
                }
            }
            124 => {
                self.pop();
                self.emit(T::Pipe, p, p + 1);
            }
            125 => {
                self.pop();
                self.emit(T::DiamondStop, p, p + 1);
            }
            _ => {
                if (c == code(b'~') || is_space(c)) && self.invisible() {
                    return;
                }
                self.text_until(T::Text, b"[]()|{}\"");
            }
        }
    }

    fn ellipse_text(&mut self, c: Code) -> bool {
        let p = self.p;
        let d = self.c(p + 1);
        if c == code(b'"') {
            self.quote();
        } else if (c == code(b'~') || is_space(c)) && self.invisible() {
        } else if matches!(c, 45 | 47 | 41) && d == code(b')') {
            self.pop();
            self.emit(T::EllipseEnd, p, p + 2);
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
                if (ch == code(b'~') || is_space(ch))
                    && self.invisible_ahead(q)
                    && invisible_link(self.src, q).is_some()
                {
                    break;
                }
                q += 1;
            }
            self.emit(T::Text, p, q);
        }
        true
    }

    fn trap_text(&mut self, c: Code) -> bool {
        let p = self.p;
        let d = self.c(p + 1);
        if c == code(b'"') {
            self.quote();
            return true;
        }
        if (c == code(b'~') || is_space(c)) && self.invisible() {
            return true;
        }
        match c {
            40 if d == code(b']') => {
                self.pop();
                self.emit(T::TrapEnd, p, p + 2);
            }
            40 | 91 | 123 => {
                self.opener(c);
            }
            92 | 93 | 41 if d == code(b']') => {
                self.pop();
                self.emit(T::TrapEnd, p, p + 2);
            }
            92 => self.emit(T::Text, p, p + 1),
            47 if d == code(b']') => {
                self.pop();
                self.emit(T::InvTrapEnd, p, p + 2);
            }
            47 => self.emit(T::Text, p, p + 1),
            93 | 41 | 125 => return false,
            63 | 61 if d == code(b']') => {
                self.pop();
                self.emit(T::TrapEnd, p, p + 2);
            }
            _ => self.text_until(T::Text, b"\\[](){}/"),
        }
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
                        self.p += 2;
                    } else {
                        self.pop();
                        self.p += 1;
                    }
                } else {
                    let e = self.src.find_unit(u16::from(b'"'), p).unwrap_or(self.n);
                    self.emit(T::Str, p, e);
                }
            }
            S::MdString => {
                if c == code(b'`') {
                    if self.c(p + 1) != code(b'"') {
                        return false;
                    }
                    self.pop();
                    self.p += 2;
                } else if c == code(b'"') {
                    self.quote();
                } else {
                    self.text_until(T::MdStr, b"`\"");
                }
            }
            S::AccTitle | S::AccDescr => {
                let e = self.src.find_unit(10, p).unwrap_or(self.n);
                let kind = if self.state == S::AccTitle { T::AccTitleValue } else { T::AccDescrValue };
                self.pop();
                self.emit(kind, p, e);
            }
            S::AccDescrMultiline => {
                if c == code(b'}') {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.src.find_unit(u16::from(b'}'), p).unwrap_or(self.n);
                    self.emit(T::AccDescrMultilineValue, p, e);
                }
            }
            S::Dir => {
                if c == code(b'"') {
                    self.quote();
                } else if let Some(e) = no_direction(self.src, p) {
                    self.pop();
                    self.emit(T::NoDir, p, e);
                } else if let Some(e) = direction_word(self.src, p)
                    .or_else(|| direction_symbol(self.src, p))
                    .or_else(|| direction_v(self.src, p))
                {
                    self.pop();
                    self.emit(T::Dir, p, e);
                } else {
                    return self.invisible() || self.opener(c);
                }
            }
            S::Text => self.text(c),
            S::EllipseText => return self.ellipse_text(c),
            S::TrapText => return self.trap_text(c),
            S::EdgeText => return self.edge_text(c, |src, q| full_link(src, q, u16::from(b'-')), 45, NAN - 1),
            S::ThickEdgeText => return self.edge_text(c, |src, q| full_link(src, q, u16::from(b'=')), 61, NAN - 1),
            S::DottedEdgeText => return self.edge_text(c, dotted_link, 46, 45),
            S::Click => {
                if c == code(b'"') {
                    self.quote();
                } else if is_space(c) {
                    self.pop();
                    self.p += 1;
                } else {
                    let q = self.src.run(p + 1, |ch| !space_unit(ch));
                    self.emit(T::Click, p, q);
                }
            }
            S::CallbackName => {
                if c == code(b'(') {
                    let e = empty_call(self.src, p);
                    self.pop();
                    match e {
                        Some(e) => self.p = e,
                        None => {
                            self.push(S::CallbackArgs);
                            self.p += 1;
                        }
                    }
                } else {
                    let e = self.src.find_unit(u16::from(b'('), p).unwrap_or(self.n);
                    self.emit(T::CallbackName, p, e);
                }
            }
            S::CallbackArgs => {
                if c == code(b')') {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.src.find_unit(u16::from(b')'), p).unwrap_or(self.n);
                    self.emit(T::CallbackArgs, p, e);
                }
            }
            S::ShapeData => {
                if c == code(b'"') {
                    self.push(S::ShapeDataStr);
                    self.emit(T::ShapeData, p, p + 1);
                } else if c == code(b'}') {
                    self.pop();
                    self.p += 1;
                } else if c == code(b'^') {
                    return false;
                } else {
                    self.text_until(T::ShapeData, b"}^\"");
                }
            }
            S::ShapeDataStr => {
                if c == code(b'"') {
                    self.pop();
                    self.emit(T::ShapeData, p, p + 1);
                } else {
                    let e = self.src.find_unit(u16::from(b'"'), p).unwrap_or(self.n);
                    self.tokens.push(Token { kind: T::ShapeData, text: line_breaks(&self.src[p..e]), start: p });
                    self.p = e;
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
    };
    loop {
        if lexer.p >= lexer.n {
            if let Some(kind) = lexer.end_token() {
                lexer.emit(kind, lexer.n, lexer.n);
            }
            break;
        }
        if !lexer.step() {
            lexer.tokens.push(Token { kind: T::Error, text: Vec::new(), start: lexer.p });
            return lexer.tokens;
        }
    }
    lexer.tokens.push(Token { kind: T::End, text: Vec::new(), start: lexer.n });
    lexer.tokens
}
