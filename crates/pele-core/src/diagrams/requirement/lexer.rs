//! The requirement lexer, ported from Pele's `src/diagrams/requirement/lexer.ts`, which writes out
//! the token stream of Mermaid's Jison lexer for `requirementDiagram`. Its rules are tried in order
//! and the first match wins, so a line that holds a direction statement anywhere is one token.

use crate::input::{Units, is_line_end, is_space, is_word, to_lowercase};

/// Pele's token types, in the order of its `T` enum, which the parser compares as ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub(super) enum T {
    End,
    Error,
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    Newline,
    Eof,
    Rd,
    StructStart,
    StructStop,
    StyleSeparator,
    Colonsep,
    Id,
    Text,
    Risk,
    Verifymthd,
    Requirement,
    FunctionalRequirement,
    InterfaceRequirement,
    PerformanceRequirement,
    PhysicalRequirement,
    DesignConstraint,
    LowRisk,
    MedRisk,
    HighRisk,
    VerifyAnalysis,
    VerifyDemonstration,
    VerifyInspection,
    VerifyTest,
    Element,
    Contains,
    Copies,
    Derives,
    Satisfies,
    Verifies,
    Refines,
    Traces,
    Type,
    Docref,
    Style,
    Classdef,
    Class,
    Alpha,
    Colon,
    Semicolon,
    Percent,
    Minus,
    Brkt,
    EndArrowL,
    EndArrowR,
    Line,
    QString,
    UnqString,
    Comma,
}

/// Every token type in order, so that a range of them can be named.
const ALL: [T; 61] = {
    use T::*;
    [
        End,
        Error,
        Title,
        AccTitle,
        AccTitleValue,
        AccDescr,
        AccDescrValue,
        AccDescrMultilineValue,
        DirectionTb,
        DirectionBt,
        DirectionRl,
        DirectionLr,
        Newline,
        Eof,
        Rd,
        StructStart,
        StructStop,
        StyleSeparator,
        Colonsep,
        Id,
        Text,
        Risk,
        Verifymthd,
        Requirement,
        FunctionalRequirement,
        InterfaceRequirement,
        PerformanceRequirement,
        PhysicalRequirement,
        DesignConstraint,
        LowRisk,
        MedRisk,
        HighRisk,
        VerifyAnalysis,
        VerifyDemonstration,
        VerifyInspection,
        VerifyTest,
        Element,
        Contains,
        Copies,
        Derives,
        Satisfies,
        Verifies,
        Refines,
        Traces,
        Type,
        Docref,
        Style,
        Classdef,
        Class,
        Alpha,
        Colon,
        Semicolon,
        Percent,
        Minus,
        Brkt,
        EndArrowL,
        EndArrowR,
        Line,
        QString,
        UnqString,
        Comma,
    ]
};

/// Pele's `TOKEN_NAMES`, as Jison's messages spell them.
const NAMES: [&str; 61] = [
    "$end",
    "INVALID",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "NEWLINE",
    "EOF",
    "RD",
    "STRUCT_START",
    "STRUCT_STOP",
    "STYLE_SEPARATOR",
    "COLONSEP",
    "ID",
    "TEXT",
    "RISK",
    "VERIFYMTHD",
    "REQUIREMENT",
    "FUNCTIONAL_REQUIREMENT",
    "INTERFACE_REQUIREMENT",
    "PERFORMANCE_REQUIREMENT",
    "PHYSICAL_REQUIREMENT",
    "DESIGN_CONSTRAINT",
    "LOW_RISK",
    "MED_RISK",
    "HIGH_RISK",
    "VERIFY_ANALYSIS",
    "VERIFY_DEMONSTRATION",
    "VERIFY_INSPECTION",
    "VERIFY_TEST",
    "ELEMENT",
    "CONTAINS",
    "COPIES",
    "DERIVES",
    "SATISFIES",
    "VERIFIES",
    "REFINES",
    "TRACES",
    "TYPE",
    "DOCREF",
    "STYLE",
    "CLASSDEF",
    "CLASS",
    "ALPHA",
    "COLON",
    "SEMICOLON",
    "PERCENT",
    "MINUS",
    "BRKT",
    "END_ARROW_L",
    "END_ARROW_R",
    "LINE",
    "qString",
    "unqString",
    "COMMA",
];

impl T {
    pub(super) fn name(self) -> &'static str {
        NAMES[self as usize]
    }

    /// The token types from `self` to `to`, both included.
    pub(super) fn through(self, to: T) -> &'static [T] {
        &ALL[self as usize..=to as usize]
    }
}

/// Every keyword is matched without regard to case and must end where a word does.
fn keyword(word: &[u16]) -> Option<T> {
    // A word is ASCII, so lowercasing it byte by byte is JavaScript's `toLowerCase`.
    let lower: String = word.iter().map(|&c| char::from(c as u8).to_ascii_lowercase()).collect();
    Some(match lower.as_str() {
        "requirementdiagram" => T::Rd,
        "id" => T::Id,
        "text" => T::Text,
        "risk" => T::Risk,
        "verifymethod" => T::Verifymthd,
        "requirement" => T::Requirement,
        "functionalrequirement" => T::FunctionalRequirement,
        "interfacerequirement" => T::InterfaceRequirement,
        "performancerequirement" => T::PerformanceRequirement,
        "physicalrequirement" => T::PhysicalRequirement,
        "designconstraint" => T::DesignConstraint,
        "low" => T::LowRisk,
        "medium" => T::MedRisk,
        "high" => T::HighRisk,
        "analysis" => T::VerifyAnalysis,
        "demonstration" => T::VerifyDemonstration,
        "inspection" => T::VerifyInspection,
        "test" => T::VerifyTest,
        "element" => T::Element,
        "contains" => T::Contains,
        "copies" => T::Copies,
        "derives" => T::Derives,
        "satisfies" => T::Satisfies,
        "verifies" => T::Verifies,
        "refines" => T::Refines,
        "traces" => T::Traces,
        "type" => T::Type,
        "docref" => T::Docref,
        "style" => T::Style,
        "classdef" => T::Classdef,
        "class" => T::Class,
        _ => return None,
    })
}

const LONGEST_KEYWORD: usize = 22;

/// What ends an unquoted string: `:,{<>-=` and line breaks.
fn stops(c: u16) -> bool {
    matches!(u8::try_from(c), Ok(b':' | b',' | b'\r' | b'\n' | b'{' | b'<' | b'>' | b'-' | b'='))
}

/// The token stream. Each token's text is a range of the source.
pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    texts: Vec<(usize, usize)>,
    pub(super) starts: Vec<usize>,
}

impl Tokens {
    pub(super) fn text<'a>(&self, src: &'a [u16], index: usize) -> &'a [u16] {
        let (from, to) = self.texts[index];
        &src[from..to]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    String,
    Style,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

/// Whether `word` starts at `at`, by ASCII letters without regard to case, as the `i` flag
/// compares them.
fn caseless(src: &[u16], at: usize, word: &str) -> bool {
    word.bytes().enumerate().all(|(offset, byte)| {
        src.at(at + offset).and_then(|c| u8::try_from(c).ok()).is_some_and(|c| c.eq_ignore_ascii_case(&byte))
    })
}

/// `/word\s*<punctuation>\s*/iy` at `at`: where the match ends.
fn word_then(src: &[u16], at: usize, word: &str, punctuation: u8) -> Option<usize> {
    if !caseless(src, at, word) {
        return None;
    }
    let mark = src.run(at + word.len(), is_space);
    (src.at(mark) == Some(u16::from(punctuation))).then(|| src.run(mark + 1, is_space))
}

/// Each match of `/direction\s+(?:(TB)|(BT)|(RL)|LR)/gi`, by kind: where it starts and ends.
fn direction_matches(src: &[u16]) -> [(Vec<usize>, Vec<usize>); 4] {
    let mut found: [(Vec<usize>, Vec<usize>); 4] = Default::default();
    let mut at = 0;
    while at + 9 <= src.len() {
        if caseless(src, at, "direction") && src.at(at + 9).is_some_and(is_space) {
            let word = src.run(at + 9, is_space);
            let kind = ["TB", "BT", "RL", "LR"].iter().position(|name| caseless(src, word, name));
            if let Some(kind) = kind {
                found[kind].0.push(at);
                found[kind].1.push(word + 2);
                at = word + 2;
                continue;
            }
        }
        at += 1;
    }
    found
}

struct Lexer<'a> {
    src: &'a [u16],
    tokens: Tokens,
    stack: Vec<S>,
    state: S,
    p: usize,
    directions: [(Vec<usize>, Vec<usize>); 4],
    seen: [usize; 4],
    any_direction: bool,
    /// The end of the line the direction statements were last looked for on.
    line_end: Option<usize>,
}

impl Lexer<'_> {
    fn emit(&mut self, kind: T, from: usize, to: usize) {
        self.tokens.types.push(kind);
        self.tokens.texts.push((from, to));
        self.tokens.starts.push(from);
        self.p = to;
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

    /// `indexOf('\n', at)`, or the end.
    fn line_break(&self, at: usize) -> usize {
        self.src.find_unit(10, at).unwrap_or(self.src.len())
    }

    /// `.*direction\s+XX[^\n]*`: the last such statement that starts on this line decides the token.
    fn direction(&mut self) -> bool {
        let p = self.p;
        if self.line_end.is_none_or(|end| end < p) {
            let end = (p..self.src.len()).find(|&at| is_line_end(self.src[at])).unwrap_or(self.src.len());
            self.line_end = Some(end);
            for (kind, (starts, _)) in self.directions.iter().enumerate() {
                let mut j = self.seen[kind];
                while j < starts.len() && starts[j] <= end {
                    j += 1;
                }
                self.seen[kind] = j;
            }
        }
        for kind in 0..4 {
            let Some(j) = self.seen[kind].checked_sub(1) else {
                continue;
            };
            if self.directions[kind].0[j] >= p {
                let end = self.line_break(self.directions[kind].1[j]);
                self.emit(T::ALL_DIRECTIONS[kind], p, end);
                return true;
            }
        }
        false
    }

    fn initial(&mut self, c: u16) -> bool {
        let (src, p, n) = (self.src, self.p, self.src.len());
        let lower = c | 32;
        if lower == 116
            && to_lowercase(&src[(p + 1).min(n)..(p + 5).min(n)]).is_str("itle")
            && src.at(p + 5).is_some_and(is_space)
        {
            let q = (p + 6..n).find(|&q| matches!(src[q], 35 | 10 | 59)).unwrap_or(n.max(p + 6));
            if q > p + 6 {
                self.emit(T::Title, p, q);
                return true;
            }
        }
        let folds_to = |at: usize, letter: u16| src.at(at).is_some_and(|d| d | 32 == letter);
        if lower == 97 && folds_to(p + 1, 99) && folds_to(p + 2, 99) {
            if let Some(end) = word_then(src, p, "accTitle", b':') {
                self.push(S::AccTitle);
                self.emit(T::AccTitle, p, end);
                return true;
            }
            if let Some(end) = word_then(src, p, "accDescr", b':') {
                self.push(S::AccDescr);
                self.emit(T::AccDescr, p, end);
                return true;
            }
            if let Some(end) = word_then(src, p, "accDescr", b'{') {
                self.push(S::AccDescrMultiline);
                self.p = end;
                return true;
            }
        }
        if self.any_direction && !is_line_end(c) && self.direction() {
            return true;
        }

        if c == 10 || (c == 13 && src.at(p + 1) == Some(10)) {
            let mut q = p;
            while q < n {
                if src[q] == 10 {
                    q += 1;
                } else if src[q] == 13 && src.at(q + 1) == Some(10) {
                    q += 2;
                } else {
                    break;
                }
            }
            self.emit(T::Newline, p, q);
            return true;
        }
        if is_space(c) {
            self.p = src.run(p + 1, is_space);
            return true;
        }
        if c == 35 || c == 37 {
            self.p = self.line_break(p);
            return true;
        }

        if is_word(c) {
            let mut q = src.run(p + 1, is_word);
            if q - p <= LONGEST_KEYWORD
                && let Some(kind) = keyword(&src[p..q])
            {
                if matches!(kind, T::Style | T::Classdef | T::Class) {
                    self.push(S::Style);
                }
                self.emit(kind, p, q);
                return true;
            }
            q = src.run(q, |d| !stops(d));
            let text = &src[p..q];
            let lead = text.len() - text.trim_start_js().len();
            let trimmed = text.trim_js().len();
            self.tokens.types.push(T::UnqString);
            self.tokens.texts.push((p + lead, p + lead + trimmed));
            self.tokens.starts.push(p);
            self.p = q;
            return true;
        }

        match u8::try_from(c) {
            Ok(b'{') => self.emit(T::StructStart, p, p + 1),
            Ok(b'}') => self.emit(T::StructStop, p, p + 1),
            Ok(b':') => {
                if src.at(p + 1) == Some(58) && src.at(p + 2) == Some(58) {
                    self.emit(T::StyleSeparator, p, p + 3);
                } else {
                    self.emit(T::Colonsep, p, p + 1);
                }
            }
            Ok(b'<') => {
                if src.at(p + 1) != Some(45) {
                    return false;
                }
                self.emit(T::EndArrowL, p, p + 2);
            }
            Ok(b'-') => {
                if src.at(p + 1) == Some(62) {
                    self.emit(T::EndArrowR, p, p + 2);
                } else {
                    self.emit(T::Line, p, p + 1);
                }
            }
            Ok(b'"') => {
                self.push(S::String);
                self.p += 1;
            }
            Ok(b',') => self.emit(T::Comma, p, p + 1),
            _ => return false,
        }
        true
    }

    fn style(&mut self, c: u16) -> bool {
        let p = self.p;
        if is_word(c) {
            let q = self.src.run(p + 1, is_word);
            self.emit(T::Alpha, p, q);
            return true;
        }
        match u8::try_from(c) {
            Ok(b':') => self.emit(T::Colon, p, p + 1),
            Ok(b';') => self.emit(T::Semicolon, p, p + 1),
            Ok(b'%') => self.emit(T::Percent, p, p + 1),
            Ok(b'-') => self.emit(T::Minus, p, p + 1),
            Ok(b'#') => self.emit(T::Brkt, p, p + 1),
            Ok(b' ') => self.p += 1,
            Ok(b'"') => {
                self.push(S::String);
                self.p += 1;
            }
            Ok(b'\n') => {
                self.pop();
                self.p += 1;
            }
            Ok(b',') => self.emit(T::Comma, p, p + 1),
            _ => return false,
        }
        true
    }

    /// The rest of the text up to `unit`, or to the end, as one token of `kind`.
    fn until(&mut self, unit: u8, kind: T) {
        let end = self.src.find_unit(u16::from(unit), self.p).unwrap_or(self.src.len());
        self.emit(kind, self.p, end);
    }
}

impl T {
    const ALL_DIRECTIONS: [T; 4] = [T::DirectionTb, T::DirectionBt, T::DirectionRl, T::DirectionLr];
}

/// Pele's `tokenize`. A text no rule matches ends in an `Error` token at the place.
pub(super) fn tokenize(src: &[u16]) -> Tokens {
    let directions = direction_matches(src);
    let any_direction = directions.iter().any(|(starts, _)| !starts.is_empty());
    let mut lexer = Lexer {
        src,
        tokens: Tokens { types: Vec::new(), texts: Vec::new(), starts: Vec::new() },
        stack: Vec::new(),
        state: S::Initial,
        p: 0,
        directions,
        seen: [0; 4],
        any_direction,
        line_end: None,
    };
    let n = src.len();
    while lexer.p < n {
        let c = src[lexer.p];
        let ok = match lexer.state {
            S::Initial => lexer.initial(c),
            S::Style => lexer.style(c),
            S::String => {
                if c == 34 {
                    lexer.pop();
                    lexer.p += 1;
                } else {
                    lexer.until(b'"', T::QString);
                }
                true
            }
            S::AccTitle | S::AccDescr => {
                let kind = if lexer.state == S::AccTitle { T::AccTitleValue } else { T::AccDescrValue };
                lexer.pop();
                let end = lexer.line_break(lexer.p);
                lexer.emit(kind, lexer.p, end);
                true
            }
            S::AccDescrMultiline => {
                if c == 125 {
                    lexer.pop();
                    lexer.p += 1;
                } else {
                    lexer.until(b'}', T::AccDescrMultilineValue);
                }
                true
            }
        };
        if !ok {
            let p = lexer.p;
            lexer.tokens.types.push(T::Error);
            lexer.tokens.texts.push((p, p));
            lexer.tokens.starts.push(p);
            return lexer.tokens;
        }
    }

    // At the end of input Jison still tries the rules that can match nothing, once.
    let last = match lexer.state {
        S::Initial => Some(T::Eof),
        S::String => Some(T::QString),
        S::AccTitle => Some(T::AccTitleValue),
        S::AccDescr => Some(T::AccDescrValue),
        S::AccDescrMultiline => Some(T::AccDescrMultilineValue),
        S::Style => None,
    };
    if let Some(last) = last {
        lexer.emit(last, n, n);
    }
    lexer.emit(T::End, n, n);
    lexer.tokens
}
