//! The block diagram lexer, ported from Pele's `src/diagrams/block/lexer.ts`: one pass over the
//! source with a stack of lexer states, as Mermaid's jison lexer has.

use crate::input::{JsText, Units, is_space, is_word};

/// Pele's token types, in the order of its `T` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum T {
    End,
    Error,
    Key,
    IdBlock,
    Columns,
    MdStr,
    Str,
    SpaceBlock,
    Default,
    LinkStyle,
    Interpolate,
    ClassDef,
    DefaultClassDefId,
    ClassDefId,
    ClassDefStyleOpts,
    Class,
    ClassEntityIds,
    StyleClass,
    Style,
    StyleEntityIds,
    StyleDefinitionData,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    EndKeyword,
    NodeDStart,
    NodeDEnd,
    BlockArrowStart,
    BlockArrowEnd,
    Dir,
    NodeId,
    Link,
    StartLink,
    LinkLabel,
    Size,
    Eof,
}

impl T {
    /// Pele's `TOKEN_NAMES`, which its error messages quote.
    pub(super) fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Error => "INVALID",
            T::Key => "BLOCK_DIAGRAM_KEY",
            T::IdBlock => "id-block",
            T::Columns => "COLUMNS",
            T::MdStr => "MD_STR",
            T::Str => "STR",
            T::SpaceBlock => "SPACE_BLOCK",
            T::Default => "DEFAULT",
            T::LinkStyle => "LINKSTYLE",
            T::Interpolate => "INTERPOLATE",
            T::ClassDef => "classDef",
            T::DefaultClassDefId => "DEFAULT_CLASSDEF_ID",
            T::ClassDefId => "CLASSDEF_ID",
            T::ClassDefStyleOpts => "CLASSDEF_STYLEOPTS",
            T::Class => "class",
            T::ClassEntityIds => "CLASSENTITY_IDS",
            T::StyleClass => "STYLECLASS",
            T::Style => "style",
            T::StyleEntityIds => "STYLE_ENTITY_IDS",
            T::StyleDefinitionData => "STYLE_DEFINITION_DATA",
            T::AccTitle => "acc_title",
            T::AccTitleValue => "acc_title_value",
            T::AccDescr => "acc_descr",
            T::AccDescrValue => "acc_descr_value",
            T::AccDescrMultilineValue => "acc_descr_multiline_value",
            T::EndKeyword => "end",
            T::NodeDStart => "NODE_DSTART",
            T::NodeDEnd => "NODE_DEND",
            T::BlockArrowStart => "BLOCK_ARROW_START",
            T::BlockArrowEnd => "BLOCK_ARROW_END",
            T::Dir => "DIR",
            T::NodeId => "NODE_ID",
            T::Link => "LINK",
            T::StartLink => "START_LINK",
            T::LinkLabel => "LINK_LABEL",
            T::Size => "SIZE",
            T::Eof => "EOF",
        }
    }
}

pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    pub(super) texts: Vec<JsText>,
    pub(super) starts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    String,
    MdString,
    Node,
    BlockArrow,
    ArrowDir,
    LLabel,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    StyleStmnt,
    StyleDefinition,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

const DIRECTIONS: [&str; 6] = ["right", "left", "x", "y", "up", "down"];

/// What `charCodeAt` answers past the end: NaN, which equals no character.
const NONE: u32 = u32::MAX;

/// Whether `c` ends a node id: `( ) [ { } < > - : =` and whitespace.
fn id_stop(c: u16) -> bool {
    if c < 128 { matches!(c, 9..=13 | 32 | 40 | 41 | 45 | 58 | 60 | 61 | 62 | 91 | 123 | 125) } else { is_space(c) }
}

fn wordy(c: u32) -> bool {
    u16::try_from(c).is_ok_and(is_word)
}

struct Lexer<'a> {
    src: &'a [u16],
    n: usize,
    out: Tokens,
    stack: Vec<S>,
    state: S,
    p: usize,
}

impl Lexer<'_> {
    fn c(&self, at: usize) -> u32 {
        self.src.get(at).map_or(NONE, |&c| u32::from(c))
    }

    fn put(&mut self, kind: T, text: JsText, end: usize) -> bool {
        self.out.types.push(kind);
        self.out.texts.push(text);
        self.out.starts.push(self.p);
        self.p = end;
        true
    }

    fn emit(&mut self, kind: T, end: usize) -> bool {
        let text = self.src[self.p.min(self.n)..end.min(self.n)].into();
        self.put(kind, text, end)
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

    fn ws(&self, mut q: usize) -> usize {
        while q < self.n && is_space(self.src[q]) {
            q += 1;
        }
        q
    }

    fn digits(&self, mut q: usize) -> usize {
        while q < self.n && (48..=57).contains(&self.src[q]) {
            q += 1;
        }
        q
    }

    fn word(&self, mut q: usize) -> usize {
        while q < self.n && is_word(self.src[q]) {
            q += 1;
        }
        q
    }

    fn starts(&self, text: &str, at: usize) -> bool {
        self.src.starts_at(at, text)
    }

    fn keyword(&self, text: &str) -> bool {
        self.starts(text, self.p) && !wordy(self.c(self.p + text.len()))
    }

    fn quote(&mut self) -> bool {
        if self.c(self.p + 1) == 96 {
            self.push(S::MdString);
            self.p += 2;
        } else {
            self.push(S::String);
            self.p += 1;
        }
        true
    }

    /// The end of `.+-[xo>]?` at q.
    fn dotted(&self, mut q: usize) -> Option<usize> {
        if self.c(q) != 46 {
            return None;
        }
        q += 1;
        while self.c(q) == 46 {
            q += 1;
        }
        if self.c(q) != 45 {
            return None;
        }
        Some(if matches!(self.c(q + 1), 120 | 111 | 62) { q + 2 } else { q + 1 })
    }

    /// The end of a complete link at q, before its trailing whitespace.
    fn link_end(&self, mut q: usize) -> Option<usize> {
        if matches!(self.c(q), 120 | 111 | 60) {
            q += 1;
        }
        let line = self.c(q);
        if line == 46 {
            return self.dotted(q);
        }
        if line != 45 && line != 61 {
            return None;
        }
        let mut k = q + 1;
        while self.c(k) == line {
            k += 1;
        }
        if k - q == 1 {
            return if line == 45 { self.dotted(k) } else { None };
        }
        if matches!(self.c(k), 120 | 111 | 62) {
            return Some(k + 1);
        }
        (k - q >= 3).then_some(k)
    }

    fn link(&mut self) -> bool {
        if let Some(end) = self.link_end(self.p) {
            return self.emit(T::Link, self.ws(end));
        }
        let q = if self.c(self.p) == 60 { self.p + 1 } else { self.p };
        let (a, b) = (self.c(q), self.c(q + 1));
        if (a == 45 && (b == 45 || b == 46)) || (a == 61 && b == 61) {
            self.push(S::LLabel);
            return self.emit(T::StartLink, self.ws(q + 2));
        }
        false
    }

    fn line_state(&mut self, kind: T) -> bool {
        let end = self.src.find_unit(10, self.p).unwrap_or(self.n);
        self.pop();
        self.emit(kind, end)
    }

    /// `\w+(,\s*\w+)*`
    fn id_list(&mut self, kind: T, next: S) -> bool {
        let mut k = self.word(self.p);
        if k == self.p {
            return false;
        }
        while self.c(k) == 44 {
            let from = self.ws(k + 1);
            let to = self.word(from);
            if to == from {
                break;
            }
            k = to;
        }
        self.state = next;
        self.emit(kind, k)
    }

    fn initial(&mut self, c: u16) -> bool {
        let p = self.p;
        if is_space(c) {
            self.p = self.ws(p + 1);
            return true;
        }
        match c {
            98 if self.starts("block", p) => {
                if self.keyword("block-beta") {
                    return self.emit(T::Key, p + 10);
                }
                if self.c(p + 5) == 58 {
                    return self.emit(T::IdBlock, p + 6);
                }
                if !wordy(self.c(p + 5)) {
                    return self.emit(T::Key, p + 5);
                }
            }
            99 => {
                if self.starts("columns", p) {
                    let q = self.ws(p + 7);
                    if q > p + 7 {
                        if self.starts("auto", q) && !wordy(self.c(q + 4)) {
                            return self.put(T::Columns, "-1".into(), q + 4);
                        }
                        let e = self.digits(q);
                        if e > q {
                            return self.put(T::Columns, self.src[q..e].into(), e);
                        }
                    }
                } else if self.starts("class", p) {
                    let def = self.starts("Def", p + 5);
                    let from = if def { p + 8 } else { p + 5 };
                    let q = self.ws(from);
                    if q > from {
                        self.push(if def { S::ClassDef } else { S::Class });
                        return self.emit(if def { T::ClassDef } else { T::Class }, q);
                    }
                }
            }
            34 => return self.quote(),
            115 => {
                if self.starts("space", p) {
                    if self.c(p + 5) == 58 {
                        let e = self.digits(p + 6);
                        if e > p + 6 {
                            return self.put(T::SpaceBlock, self.src[p + 6..e].into(), e);
                        }
                    }
                    if !wordy(self.c(p + 5)) {
                        return self.put(T::SpaceBlock, "1".into(), p + 5);
                    }
                } else if self.starts("style", p) {
                    let q = self.ws(p + 5);
                    if q > p + 5 {
                        self.push(S::StyleStmnt);
                        return self.emit(T::Style, q);
                    }
                }
            }
            100 if self.keyword("default") => return self.emit(T::Default, p + 7),
            108 if self.keyword("linkStyle") => return self.emit(T::LinkStyle, p + 9),
            105 if self.keyword("interpolate") => return self.emit(T::Interpolate, p + 11),
            97 => {
                if self.starts("accTitle", p) {
                    let q = self.ws(p + 8);
                    if self.c(q) == 58 {
                        self.push(S::AccTitle);
                        return self.emit(T::AccTitle, self.ws(q + 1));
                    }
                } else if self.starts("accDescr", p) {
                    let q = self.ws(p + 8);
                    let d = self.c(q);
                    if d == 58 {
                        self.push(S::AccDescr);
                        return self.emit(T::AccDescr, self.ws(q + 1));
                    }
                    if d == 123 {
                        self.push(S::AccDescrMultiline);
                        self.p = self.ws(q + 1);
                        return true;
                    }
                }
            }
            101 if self.keyword("end") => return self.emit(T::EndKeyword, self.ws(p + 3)),
            45 => {
                if self.c(p + 1) == 41 {
                    self.push(S::Node);
                    return self.emit(T::NodeDStart, p + 2);
                }
                return self.link();
            }
            61 => return self.link(),
            60 => {
                if self.c(p + 1) == 91 {
                    self.push(S::BlockArrow);
                    return self.emit(T::BlockArrowStart, p + 2);
                }
                return self.link();
            }
            40 => {
                let d = self.c(p + 1);
                let len = match d {
                    45 | 91 => 2,
                    40 if self.c(p + 2) == 40 => 3,
                    40 => 2,
                    _ => 1,
                };
                self.push(S::Node);
                return self.emit(T::NodeDStart, p + len);
            }
            41 | 123 => {
                self.push(S::Node);
                let end = if self.c(p + 1) == u32::from(c) { p + 2 } else { p + 1 };
                return self.emit(T::NodeDStart, end);
            }
            62 => {
                self.push(S::Node);
                return self.emit(T::NodeDStart, p + 1);
            }
            91 => {
                let d = self.c(p + 1);
                self.push(S::Node);
                let end = if matches!(d, 91 | 124 | 40 | 92 | 47) { p + 2 } else { p + 1 };
                return self.emit(T::NodeDStart, end);
            }
            58 => {
                let e = self.digits(p + 1);
                if e > p + 1 {
                    return self.put(T::Size, self.src[p + 1..e].into(), e);
                }
                return false;
            }
            _ => {}
        }
        let q = self.src.run(p, |ch| !id_stop(ch));
        q > p && self.emit(T::NodeId, q)
    }

    fn node_end(&mut self, c: u16) -> bool {
        let d = self.c(self.p + 1);
        let len = match c {
            34 => return self.quote(),
            40 => match d {
                45 => 2,
                40 if self.c(self.p + 2) == 40 => 3,
                40 => 2,
                _ => 1,
            },
            41 => match d {
                41 if self.c(self.p + 2) == 41 => 3,
                41 | 93 => 2,
                _ => 1,
            },
            125 if d == 125 => 2,
            125 => 1,
            45 if d == 41 => 2,
            93 if matches!(d, 93 | 41 | 62) => 2,
            93 => 1,
            92 | 47 if d == 93 => 2,
            _ => 0,
        };
        if len == 0 {
            return false;
        }
        self.pop();
        self.emit(T::NodeDEnd, self.p + len)
    }

    fn arrow_dir(&mut self, c: u16) -> bool {
        if c == 41 {
            self.pop();
            self.pop();
            let end = self.ws(self.p + 1);
            return self.put(T::BlockArrowEnd, "]>".into(), end);
        }
        let comma = c == 44;
        let from = self.ws(if comma { self.p + 1 } else { self.p });
        for dir in DIRECTIONS {
            if self.starts(dir, from) {
                let e = self.ws(from + dir.len());
                let text = self.src[if comma { from } else { self.p }..e].into();
                return self.put(T::Dir, text, e);
            }
        }
        false
    }

    fn at_end(&mut self) {
        let kind = match self.state {
            S::Initial => Some(T::Eof),
            S::String => Some(T::Str),
            S::ClassDefId => Some(T::ClassDefStyleOpts),
            S::ClassStyle => Some(T::StyleClass),
            S::StyleDefinition => Some(T::StyleDefinitionData),
            S::AccTitle => Some(T::AccTitleValue),
            S::AccDescr => Some(T::AccDescrValue),
            S::AccDescrMultiline => Some(T::AccDescrMultilineValue),
            _ => None,
        };
        self.p = self.n;
        if let Some(kind) = kind {
            self.put(kind, JsText::default(), self.n);
        }
    }

    fn step(&mut self) -> bool {
        let p = self.p;
        let c = self.src[p];
        match self.state {
            S::Initial => self.initial(c),
            S::String => {
                if c == 34 {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.src.find_unit(34, p).unwrap_or(self.n);
                    self.emit(T::Str, end);
                }
                true
            }
            S::MdString => {
                if c == 96 {
                    let ok = self.c(p + 1) == 34;
                    if ok {
                        self.pop();
                        self.p += 2;
                    }
                    ok
                } else if c == 34 {
                    false
                } else {
                    let end = self.src.run(p + 1, |ch| ch != 96 && ch != 34);
                    self.emit(T::MdStr, end)
                }
            }
            S::Node => self.node_end(c),
            S::BlockArrow => {
                if c == 34 {
                    self.quote()
                } else if c == 93 && self.c(p + 1) == 62 {
                    let q = self.ws(p + 2);
                    let ok = self.c(q) == 40;
                    if ok {
                        self.push(S::ArrowDir);
                        self.p = q + 1;
                    }
                    ok
                } else {
                    false
                }
            }
            S::ArrowDir => self.arrow_dir(c),
            S::LLabel => {
                if c == 34 {
                    if self.c(p + 1) == 96 {
                        self.quote();
                    } else {
                        self.emit(T::LinkLabel, p + 1);
                        self.push(S::String);
                    }
                    true
                } else {
                    match self.link_end(self.ws(p)) {
                        Some(end) => {
                            self.pop();
                            self.emit(T::Link, self.ws(end))
                        }
                        None => false,
                    }
                }
            }
            S::ClassDef => {
                let mut kind = T::DefaultClassDefId;
                let mut k = if self.starts("DEFAULT", p) { p + 7 } else { p };
                let mut q = self.ws(k);
                if k == p || q == k {
                    kind = T::ClassDefId;
                    k = self.word(p);
                    q = self.ws(k);
                }
                let ok = k > p && q > k;
                if ok {
                    self.state = S::ClassDefId;
                    self.emit(kind, q);
                }
                ok
            }
            S::ClassDefId => self.line_state(T::ClassDefStyleOpts),
            S::Class => self.id_list(T::ClassEntityIds, S::ClassStyle),
            S::ClassStyle => self.line_state(T::StyleClass),
            S::StyleStmnt => self.id_list(T::StyleEntityIds, S::StyleDefinition),
            S::StyleDefinition => self.line_state(T::StyleDefinitionData),
            S::AccTitle => self.line_state(T::AccTitleValue),
            S::AccDescr => self.line_state(T::AccDescrValue),
            S::AccDescrMultiline => {
                if c == 125 {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.src.find_unit(125, p).unwrap_or(self.n);
                    self.emit(T::AccDescrMultilineValue, end);
                }
                true
            }
        }
    }
}

/// Pele's `tokenize`. The list ends in `End`, or in `Error` where no token matched.
pub(super) fn tokenize(src: &[u16]) -> Tokens {
    let empty = Tokens { types: Vec::new(), texts: Vec::new(), starts: Vec::new() };
    let mut lexer = Lexer { src, n: src.len(), out: empty, stack: Vec::new(), state: S::Initial, p: 0 };
    loop {
        if lexer.p >= lexer.n {
            lexer.at_end();
            break;
        }
        if !lexer.step() {
            let at = lexer.p;
            lexer.out.types.push(T::Error);
            lexer.out.texts.push(JsText::default());
            lexer.out.starts.push(at);
            return lexer.out;
        }
    }
    lexer.out.types.push(T::End);
    lexer.out.texts.push(JsText::default());
    lexer.out.starts.push(lexer.n);
    lexer.out
}
