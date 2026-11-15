//! The class diagram lexer, ported from Pele's `src/diagrams/class/lexer.ts`: a state machine over
//! UTF-16 code units that gives the tokens Mermaid's `classDiagram.jison` lexer gives.

use crate::common::is_unicode_letter;
use crate::input::{Units, is_space, is_word};

/// Pele's token types (`tokens.ts`), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum T {
    End,
    Error,
    ClassDiagram,
    Newline,
    Eof,
    Sqs,
    Str,
    Sqe,
    Dot,
    GenericType,
    Label,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    StructStart,
    StructStop,
    Namespace,
    StyleSeparator,
    AnnotationStart,
    AnnotationEnd,
    Class,
    Member,
    NoteFor,
    Note,
    ClassDef,
    Alpha,
    Comma,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
    Line,
    DottedLine,
    Callback,
    Link,
    LinkTarget,
    Click,
    CallbackName,
    CallbackArgs,
    Href,
    Style,
    CssClass,
    Colon,
    Brkt,
    Pct,
    Minus,
    UnicodeText,
    BquoteStr,
    EdgeState,
    EofInStruct,
    OpenInStruct,
    Plus,
    Equals,
    Punctuation,
}

impl T {
    /// Pele's `TOKEN_NAMES`, as its messages name a token.
    pub(super) fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Error => "INVALID",
            T::ClassDiagram => "CLASS_DIAGRAM",
            T::Newline => "NEWLINE",
            T::Eof => "EOF",
            T::Sqs => "SQS",
            T::Str => "STR",
            T::Sqe => "SQE",
            T::Dot => "DOT",
            T::GenericType => "GENERICTYPE",
            T::Label => "LABEL",
            T::AccTitle => "acc_title",
            T::AccTitleValue => "acc_title_value",
            T::AccDescr => "acc_descr",
            T::AccDescrValue => "acc_descr_value",
            T::AccDescrMultilineValue => "acc_descr_multiline_value",
            T::StructStart => "STRUCT_START",
            T::StructStop => "STRUCT_STOP",
            T::Namespace => "NAMESPACE",
            T::StyleSeparator => "STYLE_SEPARATOR",
            T::AnnotationStart => "ANNOTATION_START",
            T::AnnotationEnd => "ANNOTATION_END",
            T::Class => "CLASS",
            T::Member => "MEMBER",
            T::NoteFor => "NOTE_FOR",
            T::Note => "NOTE",
            T::ClassDef => "CLASSDEF",
            T::Alpha => "ALPHA",
            T::Comma => "COMMA",
            T::DirectionTb => "direction_tb",
            T::DirectionBt => "direction_bt",
            T::DirectionRl => "direction_rl",
            T::DirectionLr => "direction_lr",
            T::Aggregation => "AGGREGATION",
            T::Extension => "EXTENSION",
            T::Composition => "COMPOSITION",
            T::Dependency => "DEPENDENCY",
            T::Lollipop => "LOLLIPOP",
            T::Line => "LINE",
            T::DottedLine => "DOTTED_LINE",
            T::Callback => "CALLBACK",
            T::Link => "LINK",
            T::LinkTarget => "LINK_TARGET",
            T::Click => "CLICK",
            T::CallbackName => "CALLBACK_NAME",
            T::CallbackArgs => "CALLBACK_ARGS",
            T::Href => "HREF",
            T::Style => "STYLE",
            T::CssClass => "CSSCLASS",
            T::Colon => "COLON",
            T::Brkt => "BRKT",
            T::Pct => "PCT",
            T::Minus => "MINUS",
            T::UnicodeText => "UNICODE_TEXT",
            T::BquoteStr => "BQUOTE_STR",
            T::EdgeState => "EDGE_STATE",
            T::EofInStruct => "EOF_IN_STRUCT",
            T::OpenInStruct => "OPEN_IN_STRUCT",
            T::Plus => "PLUS",
            T::Equals => "EQUALS",
            T::Punctuation => "PUNCTUATION",
        }
    }
}

const DIRECTIONS: [(T, &str); 4] =
    [(T::DirectionTb, "TB"), (T::DirectionBt, "BT"), (T::DirectionRl, "RL"), (T::DirectionLr, "LR")];

/// The lexer's states. Each one gives a token of its own at the end of input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    Initial,
    String,
    BqString,
    Generic,
    CallbackName,
    CallbackArgs,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Cls,
    ClassBody,
    Namespace,
    NamespaceBody,
}

impl S {
    /// Pele's `AT_END`.
    fn at_end(self) -> T {
        match self {
            S::Initial | S::BqString | S::Cls | S::Namespace => T::Eof,
            S::String => T::Str,
            S::Generic => T::GenericType,
            S::CallbackName => T::CallbackName,
            S::CallbackArgs => T::CallbackArgs,
            S::AccTitle => T::AccTitleValue,
            S::AccDescr => T::AccDescrValue,
            S::AccDescrMultiline => T::AccDescrMultilineValue,
            S::ClassBody | S::NamespaceBody => T::EofInStruct,
        }
    }
}

/// The tokens of a source: each one's type and where its text starts and ends.
pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    pub(super) starts: Vec<usize>,
    pub(super) ends: Vec<usize>,
}

struct Lexer<'a> {
    src: &'a [u16],
    p: usize,
    state: S,
    stack: Vec<S>,
    out: Tokens,
    dir_next: Option<usize>,
    line_end: Option<usize>,
    dir_failed: usize,
}

const fn unit(c: u8) -> u16 {
    c as u16
}

impl Lexer<'_> {
    fn n(&self) -> usize {
        self.src.len()
    }

    fn at(&self, at: usize) -> Option<u16> {
        self.src.at(at)
    }

    fn emit(&mut self, kind: T, start: usize, end: usize) -> bool {
        self.out.types.push(kind);
        self.out.starts.push(start);
        self.out.ends.push(end);
        self.p = end;
        true
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

    /// `kw`: the word at `p`, not followed by a word character.
    fn kw(&self, word: &str) -> bool {
        self.src.starts_at(self.p, word) && !self.at(self.p + word.len()).is_some_and(is_word)
    }

    fn skip_ws(&self, from: usize) -> usize {
        self.src.run(from, is_space)
    }

    /// `until`: where the next `ch` is, or the end.
    fn until(&self, ch: u8) -> usize {
        self.src.find_unit(unit(ch), self.p).unwrap_or(self.n())
    }

    /// A run of whitespace is a NEWLINE up to its last line feed; the rest is skipped.
    fn space(&mut self, leaves: bool) {
        let end = self.src.run(self.p, is_space);
        let Some(last) = (self.p..end).rev().find(|&at| self.src[at] == 10) else {
            self.p = end;
            return;
        };
        if leaves {
            self.pop();
        }
        self.emit(T::Newline, self.p, last + 1);
    }

    /// The keywords that every state recognizes, even inside generics and backtick names.
    fn keyword(&mut self, c: u16) -> bool {
        let p = self.p;
        match u8::try_from(c) {
            Ok(b'c') => {
                if self.kw("cssClass") {
                    return self.emit(T::CssClass, p, p + 8);
                }
                if self.kw("callback") {
                    return self.emit(T::Callback, p, p + 8);
                }
                if self.kw("click") {
                    return self.emit(T::Click, p, p + 5);
                }
            }
            Ok(b'l') if self.kw("link") => return self.emit(T::Link, p, p + 4),
            Ok(b'n') => {
                if self.kw("note for") {
                    return self.emit(T::NoteFor, p, p + 8);
                }
                if self.kw("note") {
                    return self.emit(T::Note, p, p + 4);
                }
            }
            Ok(b'<') if self.at(p + 1) == Some(unit(b'<')) => return self.emit(T::AnnotationStart, p, p + 2),
            Ok(b'>') if self.at(p + 1) == Some(unit(b'>')) => return self.emit(T::AnnotationEnd, p, p + 2),
            Ok(b'h') if self.kw("href") => return self.emit(T::Href, p, p + 4),
            _ => {}
        }
        false
    }

    fn common(&mut self, c: u16) -> bool {
        if self.keyword(c) {
            return true;
        }
        let p = self.p;
        let d = self.at(p + 1);
        let is = |ch: u8| d == Some(unit(ch));
        match u8::try_from(c) {
            Ok(b'"') => {
                self.push(S::String);
                self.p += 1;
                return true;
            }
            Ok(b'~') => {
                self.push(S::Generic);
                self.p += 1;
                return true;
            }
            Ok(b'`') => {
                self.push(S::BqString);
                self.p += 1;
                return true;
            }
            Ok(b'_') => {
                for target in ["_self", "_blank", "_parent", "_top"] {
                    if self.kw(target) {
                        return self.emit(T::LinkTarget, p, p + target.len());
                    }
                }
            }
            Ok(b'<') => {
                return if is(b'|') { self.emit(T::Extension, p, p + 2) } else { self.emit(T::Dependency, p, p + 1) };
            }
            Ok(b'|') => return is(b'>') && self.emit(T::Extension, p, p + 2),
            Ok(b'>') => return self.emit(T::Dependency, p, p + 1),
            Ok(b'*') => return self.emit(T::Composition, p, p + 1),
            Ok(b'o') if !d.is_some_and(is_word) => return self.emit(T::Aggregation, p, p + 1),
            Ok(b'(') => return is(b')') && self.emit(T::Lollipop, p, p + 2),
            Ok(b'-') => {
                return if is(b'-') { self.emit(T::Line, p, p + 2) } else { self.emit(T::Minus, p, p + 1) };
            }
            Ok(b'.') => {
                return if is(b'.') { self.emit(T::DottedLine, p, p + 2) } else { self.emit(T::Dot, p, p + 1) };
            }
            Ok(b':') => {
                let end = self.src.run(p + 1, |ch| ch != unit(b':') && ch != 10 && ch != unit(b';'));
                if end > p + 1 {
                    return self.emit(T::Label, p, end);
                }
                return is(b':') && self.at(p + 2) == Some(unit(b':')) && self.emit(T::StyleSeparator, p, p + 3);
            }
            Ok(b'+') => return self.emit(T::Plus, p, p + 1),
            Ok(b'%') => return self.emit(T::Pct, p, p + 1),
            Ok(b'=') => return self.emit(T::Equals, p, p + 1),
            Ok(b'[') => return self.emit(T::Sqs, p, p + 1),
            Ok(b']') => return self.emit(T::Sqe, p, p + 1),
            Ok(b'!' | b'#' | b'$' | b'&' | b'\'' | b',' | b'?' | b'\\' | b'/') => {
                return self.emit(T::Punctuation, p, p + 1);
            }
            _ => {}
        }
        if is_word(c) {
            let end = self.src.run(p + 1, is_word);
            return self.emit(T::Alpha, p, end);
        }
        is_unicode_letter(c) && self.emit(T::UnicodeText, p, p + 1)
    }

    /// `/[\n\r  ]/g` from `p`: where the line ends.
    fn find_line_end(&self) -> usize {
        (self.p..self.n()).find(|&at| crate::input::is_line_end(self.src[at])).unwrap_or(self.n())
    }

    /// `/.*direction\s+XX[^\n]*/y`: up to the end of the line, when the line holds the statement.
    fn direction_match(&self, name: &str, line: usize) -> Option<usize> {
        // `.*` is greedy, so the last `direction` on the line that is followed by the name wins.
        let found = (self.p..line).rev().find(|&at| {
            self.src.starts_at(at, "direction") && {
                let gap = at + 9;
                let word = self.src.run(gap, is_space);
                word > gap && self.src.starts_at(word, name)
            }
        })?;
        let gap = found + 9;
        let word = self.src.run(gap, is_space);
        Some(self.src.run(word + 2, |c| c != 10))
    }

    fn direction(&mut self) -> bool {
        let Some(mut next) = self.dir_next else {
            return false;
        };
        if self.p < self.dir_failed {
            return false;
        }
        if next < self.p {
            match self.src.find_str("direction", self.p) {
                Some(found) => next = found,
                None => {
                    self.dir_next = None;
                    return false;
                }
            }
            self.dir_next = Some(next);
        }
        let line_end = match self.line_end {
            Some(end) if end >= self.p => end,
            _ => {
                let end = self.find_line_end();
                self.line_end = Some(end);
                end
            }
        };
        if next >= line_end {
            return false;
        }
        for (kind, name) in DIRECTIONS {
            if let Some(end) = self.direction_match(name, line_end) {
                return self.emit(kind, self.p, end);
            }
        }
        // No direction statement starts anywhere in the rest of this line either.
        self.dir_failed = line_end;
        false
    }

    fn initial(&mut self, c: u16) -> bool {
        if self.direction() {
            return true;
        }
        if is_space(c) {
            self.space(false);
            return true;
        }
        let p = self.p;
        match u8::try_from(c) {
            Ok(b'%') if self.at(p + 1) == Some(unit(b'%')) => {
                let mut end = self.until(b'\n');
                while end < self.n() && matches!(self.src[end], 10 | 13) {
                    end += 1;
                }
                self.p = end;
                return true;
            }
            Ok(b'a') => {
                let title = self.src.starts_at(p, "accTitle");
                if title || self.src.starts_at(p, "accDescr") {
                    let q = self.skip_ws(p + 8);
                    let d = self.at(q);
                    if d == Some(unit(b':')) {
                        let end = self.skip_ws(q + 1);
                        self.emit(if title { T::AccTitle } else { T::AccDescr }, p, end);
                        self.push(if title { S::AccTitle } else { S::AccDescr });
                        return true;
                    }
                    if d == Some(unit(b'{')) && !title {
                        self.p = self.skip_ws(q + 1);
                        self.push(S::AccDescrMultiline);
                        return true;
                    }
                }
            }
            Ok(b'c') => {
                if self.kw("classDiagram-v2") {
                    return self.emit(T::ClassDiagram, p, p + 15);
                }
                if self.kw("classDiagram") {
                    return self.emit(T::ClassDiagram, p, p + 12);
                }
                if self.src.starts_at(p, "call") && self.at(p + 4).is_some_and(is_space) {
                    self.p = self.skip_ws(p + 4);
                    self.push(S::CallbackName);
                    return true;
                }
                if self.kw("classDef") {
                    return self.emit(T::ClassDef, p, p + 8);
                }
                if self.kw("class") {
                    self.push(S::Cls);
                    return self.emit(T::Class, p, p + 5);
                }
            }
            Ok(b'[') if self.src.starts_at(p + 1, "*]") => return self.emit(T::EdgeState, p, p + 3),
            Ok(b's') if self.kw("style") => return self.emit(T::Style, p, p + 5),
            Ok(b'n') if self.kw("namespace") => {
                self.push(S::Namespace);
                return self.emit(T::Namespace, p, p + 9);
            }
            Ok(b',') => return self.emit(T::Comma, p, p + 1),
            Ok(b'#') => return self.emit(T::Brkt, p, p + 1),
            _ => {}
        }
        self.common(c) || (c == unit(b':') && self.emit(T::Colon, p, p + 1))
    }

    fn step(&mut self, c: u16) -> bool {
        let p = self.p;
        match self.state {
            S::Initial => return self.initial(c),
            S::Cls => {
                if is_space(c) {
                    self.space(true);
                } else if c == unit(b'}') {
                    self.pop();
                    self.pop();
                    self.emit(T::StructStop, p, p + 1);
                } else if c == unit(b'{') {
                    self.push(S::ClassBody);
                    self.emit(T::StructStart, p, p + 1);
                } else {
                    return self.common(c);
                }
            }
            S::ClassBody => {
                if c == unit(b'"') {
                    self.push(S::String);
                    self.p += 1;
                } else if c == unit(b'}') {
                    self.pop();
                    self.emit(T::StructStop, p, p + 1);
                } else if c == unit(b'[') && self.src.starts_at(p + 1, "*]") {
                    self.emit(T::EdgeState, p, p + 3);
                } else if c == unit(b'{') {
                    self.emit(T::OpenInStruct, p, p + 1);
                } else if c == 10 {
                    self.p += 1;
                } else {
                    let end = self.src.run(p + 1, |ch| ch != unit(b'{') && ch != unit(b'}') && ch != 10);
                    self.emit(T::Member, p, end);
                }
            }
            S::Namespace => {
                if is_space(c) {
                    self.space(true);
                } else if c == unit(b'{') {
                    self.push(S::NamespaceBody);
                    self.emit(T::StructStart, p, p + 1);
                } else if c == unit(b'}') {
                    // The brace is read again in the state the namespace was opened from.
                    self.pop();
                } else if c == unit(b'n') && self.kw("namespace") {
                    self.push(S::Namespace);
                    self.emit(T::Namespace, p, p + 9);
                } else {
                    return self.common(c);
                }
            }
            S::NamespaceBody => {
                if is_space(c) {
                    self.space(false);
                } else if c == unit(b'}') {
                    self.pop();
                    self.emit(T::StructStop, p, p + 1);
                } else if c == unit(b'n') && self.kw("namespace") {
                    self.push(S::Namespace);
                    self.emit(T::Namespace, p, p + 9);
                } else if c == unit(b'[') && self.src.starts_at(p + 1, "*]") {
                    self.emit(T::EdgeState, p, p + 3);
                } else if c == unit(b'c') && self.kw("class") {
                    self.push(S::Cls);
                    self.emit(T::Class, p, p + 5);
                } else {
                    return self.common(c);
                }
            }
            S::String => {
                if c == unit(b'"') {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(b'"');
                    self.emit(T::Str, p, end);
                }
            }
            S::Generic => {
                if c == unit(b'"') {
                    self.push(S::String);
                    self.p += 1;
                } else if self.keyword(c) {
                } else if c == unit(b'~') {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(b'~');
                    self.emit(T::GenericType, p, end);
                }
            }
            S::BqString => {
                if c == unit(b'"') {
                    self.push(S::String);
                    self.p += 1;
                } else if self.keyword(c) {
                } else if c == unit(b'~') {
                    self.push(S::Generic);
                    self.p += 1;
                } else if c == unit(b'`') {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(b'`');
                    self.emit(T::BquoteStr, p, end);
                }
            }
            S::CallbackName => {
                if c == unit(b'(') {
                    let q = self.skip_ws(p + 1);
                    self.pop();
                    if self.at(q) == Some(unit(b')')) {
                        self.p = q + 1;
                    } else {
                        self.push(S::CallbackArgs);
                        self.p += 1;
                    }
                } else {
                    let end = self.until(b'(');
                    self.emit(T::CallbackName, p, end);
                }
            }
            S::CallbackArgs => {
                if c == unit(b')') {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(b')');
                    self.emit(T::CallbackArgs, p, end);
                }
            }
            S::AccTitle | S::AccDescr => {
                let kind = if self.state == S::AccTitle { T::AccTitleValue } else { T::AccDescrValue };
                let end = self.until(b'\n');
                self.emit(kind, p, end);
                self.pop();
            }
            S::AccDescrMultiline => {
                if c == unit(b'}') {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(b'}');
                    self.emit(T::AccDescrMultilineValue, p, end);
                }
            }
        }
        true
    }
}

/// Pele's `tokenize`. A source that has text no state can read ends in an `Error` token there.
pub(super) fn tokenize(src: &[u16]) -> Tokens {
    let mut lexer = Lexer {
        src,
        p: 0,
        state: S::Initial,
        stack: Vec::new(),
        out: Tokens { types: Vec::new(), starts: Vec::new(), ends: Vec::new() },
        dir_next: src.find_str("direction", 0),
        line_end: None,
        dir_failed: 0,
    };
    let n = src.len();
    loop {
        if lexer.p >= n {
            let at_end = lexer.state.at_end();
            lexer.out.types.push(at_end);
            lexer.out.starts.push(n);
            lexer.out.ends.push(n);
            break;
        }
        let c = src[lexer.p];
        if !lexer.step(c) {
            let p = lexer.p;
            lexer.out.types.push(T::Error);
            lexer.out.starts.push(p);
            lexer.out.ends.push(p);
            return lexer.out;
        }
    }
    lexer.out.types.push(T::End);
    lexer.out.starts.push(n);
    lexer.out.ends.push(n);
    lexer.out
}
