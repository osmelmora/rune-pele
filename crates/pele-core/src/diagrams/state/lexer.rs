//! The state diagram lexer (Pele's `src/diagrams/state/lexer.ts`), which reproduces the token
//! stream of Mermaid's `stateDiagram.jison` lexer. The start conditions, the look-ahead caches
//! and the order rules are tried in follow Pele's one for one.

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
    Nl => "NL",
    Sd => "SD",
    Descr => "DESCR",
    Arrow => "-->",
    HideEmpty => "HIDE_EMPTY",
    Scale => "scale",
    Width => "WIDTH",
    CompositState => "COMPOSIT_STATE",
    StructStart => "STRUCT_START",
    StructStop => "STRUCT_STOP",
    StateDescr => "STATE_DESCR",
    As => "AS",
    Id => "ID",
    Fork => "FORK",
    Join => "JOIN",
    Choice => "CHOICE",
    Concurrent => "CONCURRENT",
    Note => "note",
    NoteText => "NOTE_TEXT",
    AccTitle => "acc_title",
    AccTitleValue => "acc_title_value",
    AccDescr => "acc_descr",
    AccDescrValue => "acc_descr_value",
    AccDescrMultilineValue => "acc_descr_multiline_value",
    Click => "CLICK",
    Str => "STRING",
    Href => "HREF",
    ClassDef => "classDef",
    ClassDefId => "CLASSDEF_ID",
    DefaultClassDefId => "DEFAULT_CLASSDEF_ID",
    ClassDefStyleOpts => "CLASSDEF_STYLEOPTS",
    Default => "DEFAULT",
    Style => "style",
    StyleIds => "STYLE_IDS",
    StyleDefStyleOpts => "STYLEDEF_STYLEOPTS",
    Class => "class",
    ClassEntityIds => "CLASSENTITY_IDS",
    StyleClass => "STYLECLASS",
    DirectionTb => "direction_tb",
    DirectionBt => "direction_bt",
    DirectionRl => "direction_rl",
    DirectionLr => "direction_lr",
    EdgeState => "EDGE_STATE",
    StyleSeparator => "STYLE_SEPARATOR",
    LeftOf => "left_of",
    RightOf => "right_of",
    Invalid => "INVALID",
}

/// The four direction tokens, in the order the grammar tries their rules.
pub(super) const DIRECTIONS: [T; 4] = [T::DirectionTb, T::DirectionBt, T::DirectionRl, T::DirectionLr];

pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    pub(super) texts: Vec<Vec<u16>>,
    pub(super) starts: Vec<usize>,
    /// Set when lexing stopped on the grammar's own error rather than on unrecognized text.
    pub(super) error: Option<String>,
}

/// The lexer's start conditions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Struct,
    State,
    StateString,
    StateId,
    Scale,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    Style,
    StyleDefStyles,
    Note,
    NoteId,
    NoteText,
    FloatingNote,
    FloatingNoteId,
}

/// A code unit as `charCodeAt` reads it. Past the end it is `NaN`, which equals nothing.
type Code = u32;
const NAN: Code = u32::MAX;

fn is_space(c: Code) -> bool {
    u16::try_from(c).is_ok_and(space_unit)
}

fn is_word(c: Code) -> bool {
    u16::try_from(c).is_ok_and(word_unit)
}

/// The next match at or after a position of a pattern that `scan` finds, remembering the answer
/// so that asking at each token costs one scan of the text in total. Pele's `finder`.
struct Finder {
    scan: fn(&[u16], usize) -> Option<usize>,
    from: usize,
    /// The answer for `from`, once one has been asked.
    next: Option<Option<usize>>,
}

impl Finder {
    fn new(scan: fn(&[u16], usize) -> Option<usize>) -> Self {
        Finder { scan, from: 0, next: None }
    }

    fn find(&mut self, src: &[u16], q: usize) -> Option<usize> {
        let stale = match self.next {
            None => true,
            Some(next) => q < self.from || next.is_some_and(|next| q > next),
        };
        if stale {
            self.next = Some((self.scan)(src, q));
            self.from = q;
        }
        self.next.flatten()
    }
}

/// Whether `word`, a lowercase ASCII word, is at `q`, ignoring the case of its letters.
fn caseless(src: &[u16], q: usize, word: &str) -> bool {
    word.bytes().enumerate().all(|(i, a)| {
        let c = src.get(q + i).map_or(NAN, |&c| Code::from(c));
        let a = Code::from(a);
        c == a || ((97..=122).contains(&a) && c == a - 32)
    })
}

/// `/direction/gi`
fn scan_direction(src: &[u16], q: usize) -> Option<usize> {
    (q..src.len()).find(|&at| caseless(src, at, "direction"))
}

/// `/<<(?:fork|join|choice)>>|\[\[(?:fork|join|choice)\]\]/gi`
fn scan_stereotype(src: &[u16], q: usize) -> Option<usize> {
    (q..src.len()).find(|&at| {
        let (open, close) = match src[at] {
            60 => ("<<", ">>"),
            91 => ("[[", "]]"),
            _ => return false,
        };
        src.starts_at(at, open)
            && ["fork", "join", "choice"]
                .iter()
                .any(|word| caseless(src, at + 2, word) && src.starts_at(at + 2 + word.len(), close))
    })
}

/// `/[\n\r  ]/g`
fn scan_line_end(src: &[u16], q: usize) -> Option<usize> {
    (q..src.len()).find(|&at| crate::input::is_line_end(src[at]))
}

/// `/%%/g`
fn scan_percent(src: &[u16], q: usize) -> Option<usize> {
    (q..src.len()).find(|&at| src.starts_at(at, "%%"))
}

/// `/\{/g`
fn scan_brace(src: &[u16], q: usize) -> Option<usize> {
    src.find_unit(123, q)
}

struct Lexer<'a> {
    src: &'a [u16],
    n: usize,
    out: Tokens,
    stack: Vec<S>,
    state: S,
    p: usize,
    next_direction: Finder,
    next_stereotype: Finder,
    next_line_end: Finder,
    next_percent: Finder,
    next_brace: Finder,
    /// The last `direction <dir>` of each kind on the line being lexed, and where its keyword ends.
    dir_from: Option<usize>,
    dir_to: usize,
    dir_at: [Option<usize>; 4],
    dir_end: [usize; 4],
    /// The same for `<<fork>>`, `<<join>>`, `<<choice>>` and their `[[ ]]` spellings.
    stereo_from: Option<usize>,
    stereo_to: usize,
    stereo_at: [Option<usize>; 6],
}

impl<'a> Lexer<'a> {
    fn c(&self, q: usize) -> Code {
        self.src.get(q).map_or(NAN, |&c| Code::from(c))
    }

    fn emit(&mut self, kind: T, s: usize, e: usize) {
        let text = self.src[s..e].to_vec();
        self.emit_text(kind, text, s, e);
    }

    fn emit_text(&mut self, kind: T, text: Vec<u16>, s: usize, e: usize) {
        self.out.types.push(kind);
        self.out.texts.push(text);
        self.out.starts.push(s);
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

    fn word(&self, word: &str, q: usize) -> bool {
        caseless(self.src, q, word)
    }

    fn ws_end(&self, q: usize) -> usize {
        self.src.run(q, space_unit)
    }

    fn word_end(&self, q: usize) -> usize {
        self.src.run(q, word_unit)
    }

    fn eol(&self, q: usize) -> usize {
        self.src.find_unit(10, q).unwrap_or(self.n)
    }

    fn line_end(&mut self, q: usize) -> usize {
        self.next_line_end.find(self.src, q).unwrap_or(self.n)
    }

    /// Rules `.*direction\s+TB[^\n]*` and its three siblings: the first kind, in grammar order,
    /// found between `q` and the end of its line.
    fn direction(&mut self, q: usize) -> Option<usize> {
        if self.dir_from.is_none_or(|from| q < from || q > self.dir_to) {
            let first = self.next_direction.find(self.src, q);
            let end = if first.is_none() { self.n } else { self.line_end(q) };
            self.dir_from = Some(q);
            self.dir_to = end;
            self.dir_at = [None; 4];
            let mut d = first;
            while let Some(at) = d.filter(|&at| at < end) {
                let w = self.ws_end(at + 9);
                if w != at + 9 {
                    let kind = ["tb", "bt", "rl", "lr"].iter().position(|word| self.word(word, w));
                    if let Some(kind) = kind {
                        self.dir_at[kind] = Some(at);
                        self.dir_end[kind] = w + 2;
                    }
                }
                d = self.next_direction.find(self.src, at + 1);
            }
        }
        (0..4).find(|&kind| self.dir_at[kind].is_some_and(|at| at >= q))
    }

    fn stereotype(&mut self, q: usize) -> Option<usize> {
        if self.stereo_from.is_none_or(|from| q < from || q > self.stereo_to) {
            let first = self.next_stereotype.find(self.src, q);
            let end = if first.is_none() { self.n } else { self.line_end(q) };
            self.stereo_from = Some(q);
            self.stereo_to = end;
            self.stereo_at = [None; 6];
            let mut d = first;
            while let Some(at) = d.filter(|&at| at < end) {
                let letter = self.c(at + 2) | 32;
                let kind = match letter {
                    102 => 0,
                    106 => 1,
                    _ => 2,
                };
                self.stereo_at[if self.c(at) == 60 { 0 } else { 3 } + kind] = Some(at);
                d = self.next_stereotype.find(self.src, at + 1);
            }
        }
        (0..6).find(|&kind| self.stereo_at[kind].is_some_and(|at| at >= q))
    }

    /// The grammar's `processId()`: an id ends at an inline `%%`, and a text that starts with one
    /// is dropped.
    fn emit_id(&mut self, s: usize, e: usize) -> bool {
        match self.next_percent.find(self.src, s) {
            Some(at) if at + 2 <= e => {
                if at == s {
                    self.p = e;
                    return false;
                }
                self.emit(T::Id, s, at);
            }
            _ => self.emit(T::Id, s, e),
        }
        true
    }

    /// A comment: `#` or a `%%` that does not open a directive, to the end of the line.
    fn comment(&self, c: Code) -> bool {
        c == 35 || (c == 37 && self.c(self.p + 1) == 37 && self.c(self.p + 2) != 123)
    }

    /// Spaces and tabs, but not a line feed.
    fn skip_blanks(&mut self) {
        let mut j = self.p + 1;
        while j < self.n && self.src[j] != 10 && space_unit(self.src[j]) {
            j += 1;
        }
        self.p = j;
    }

    fn run(&mut self) -> Tokens {
        loop {
            if self.p >= self.n {
                let n = self.n;
                // Jison tries the current state's rules once more on the empty remainder.
                match self.state {
                    S::Initial => self.emit(T::Nl, n, n),
                    S::AccTitle => self.emit(T::AccTitleValue, n, n),
                    S::AccDescr => self.emit(T::AccDescrValue, n, n),
                    S::AccDescrMultiline => self.emit(T::AccDescrMultilineValue, n, n),
                    S::ClassDefId => self.emit(T::ClassDefStyleOpts, n, n),
                    S::ClassStyle => self.emit(T::StyleClass, n, n),
                    S::StyleDefStyles => self.emit(T::StyleDefStyleOpts, n, n),
                    S::StateString => self.emit(T::StateDescr, n, n),
                    S::FloatingNote => self.emit(T::NoteText, n, n),
                    S::StateId | S::FloatingNoteId => self.emit(T::Id, n, n),
                    _ => {}
                }
                return self.stop(T::End);
            }
            let p = self.p;
            let c = self.c(p);
            match self.state {
                S::Initial | S::Struct => {
                    if let Some(end) = self.document(p, c) {
                        return end;
                    }
                }
                S::State => {
                    if let Some(end) = self.state_name(p, c) {
                        return end;
                    }
                }
                S::StateString => {
                    if c == 34 {
                        self.p += 1;
                        self.pop();
                        continue;
                    }
                    let close = self.src.find_unit(34, p).unwrap_or(self.n);
                    self.emit(T::StateDescr, p, close);
                }
                S::StateId => {
                    let j = self.src.run(p, |d| d != 10 && d != 123);
                    if self.emit_id(p, j) {
                        self.pop();
                    }
                }
                S::Scale => {
                    if (48..=57).contains(&c) {
                        let j = self.src.run(p + 1, |d| (48..=57).contains(&d));
                        self.emit(T::Width, p, j);
                        continue;
                    }
                    let q = self.ws_end(p);
                    if q > p && self.word("width", q) && !is_word(self.c(q + 5)) {
                        self.p = q + 5;
                        self.pop();
                        continue;
                    }
                    return self.stop(T::Error);
                }
                S::AccTitle => {
                    let end = self.eol(p);
                    self.emit(T::AccTitleValue, p, end);
                    self.pop();
                }
                S::AccDescr => {
                    let end = self.eol(p);
                    self.emit(T::AccDescrValue, p, end);
                    self.pop();
                }
                S::AccDescrMultiline => {
                    if c == 125 {
                        self.p += 1;
                        self.pop();
                        continue;
                    }
                    let close = self.src.find_unit(125, p).unwrap_or(self.n);
                    self.emit(T::AccDescrMultilineValue, p, close);
                }
                S::ClassDef => {
                    let is_default = self.word("default", p) && self.ws_end(p + 7) > p + 7;
                    let j = if is_default { p + 7 } else { self.word_end(p) };
                    let e = self.ws_end(j);
                    if j == p || e == j {
                        return self.stop(T::Error);
                    }
                    self.emit(if is_default { T::DefaultClassDefId } else { T::ClassDefId }, p, e);
                    self.pop();
                    self.push(S::ClassDefId);
                }
                S::ClassDefId => {
                    let end = self.eol(p);
                    self.emit(T::ClassDefStyleOpts, p, end);
                    self.pop();
                }
                S::Class => {
                    let mut j = self.word_end(p);
                    if j == p {
                        return self.stop(T::Error);
                    }
                    while self.c(j) == 44 {
                        let k = self.ws_end(j + 1);
                        let e = self.word_end(k);
                        if e == k {
                            break;
                        }
                        j = e;
                    }
                    self.emit(T::ClassEntityIds, p, j);
                    self.pop();
                    self.push(S::ClassStyle);
                }
                S::ClassStyle => {
                    let end = self.eol(p);
                    self.emit(T::StyleClass, p, end);
                    self.pop();
                }
                S::Style => {
                    let j = self.src.run(p, |d| word_unit(d) || d == 44);
                    let e = self.ws_end(j);
                    if j == p || e == j {
                        return self.stop(T::Error);
                    }
                    self.emit(T::StyleIds, p, e);
                    self.pop();
                    self.push(S::StyleDefStyles);
                }
                S::StyleDefStyles => {
                    let end = self.eol(p);
                    self.emit(T::StyleDefStyleOpts, p, end);
                    self.pop();
                }
                S::Note => {
                    if self.word("left of", p) && !is_word(self.c(p + 7)) {
                        self.emit(T::LeftOf, p, p + 7);
                        self.pop();
                        self.push(S::NoteId);
                        continue;
                    }
                    if self.word("right of", p) && !is_word(self.c(p + 8)) {
                        self.emit(T::RightOf, p, p + 8);
                        self.pop();
                        self.push(S::NoteId);
                        continue;
                    }
                    if c == 34 {
                        self.p += 1;
                        self.pop();
                        self.push(S::FloatingNote);
                        continue;
                    }
                    return self.stop(T::Error);
                }
                S::NoteId => {
                    let q = self.ws_end(p);
                    let j = self.src.run(q, |d| d != 58 && d != 45 && !space_unit(d));
                    if j == q {
                        return self.stop(T::Error);
                    }
                    if self.emit_id(p, j) {
                        self.pop();
                        self.push(S::NoteText);
                    }
                }
                S::NoteText => {
                    if let Some(end) = self.note_text(p) {
                        return end;
                    }
                }
                S::FloatingNote => {
                    let q = self.ws_end(p);
                    if self.word("as", q) {
                        let end = self.ws_end(q + 2);
                        self.emit(T::As, p, end);
                        self.pop();
                        self.push(S::FloatingNoteId);
                        continue;
                    }
                    if c == 34 {
                        self.p += 1;
                        continue;
                    }
                    let close = self.src.find_unit(34, p).unwrap_or(self.n);
                    self.emit(T::NoteText, p, close);
                }
                S::FloatingNoteId => {
                    let end = self.eol(p);
                    if self.emit_id(p, end) {
                        self.pop();
                    }
                }
            }
        }
    }

    /// One step in a document, at the root or inside braces. Answers the tokens when lexing
    /// stops here.
    fn document(&mut self, p: usize, c: Code) -> Option<Tokens> {
        let root = self.state == S::Initial;
        let lc = c | 32;
        if root {
            if lc == 99 && self.word("click", p) && !is_word(self.c(p + 5)) {
                self.emit(T::Click, p, p + 5);
                return None;
            }
            if lc == 104 && self.word("href", p) && !is_word(self.c(p + 4)) {
                self.emit(T::Href, p, p + 4);
                return None;
            }
            if c == 34
                && let Some(close) = self.src.find_unit(34, p + 1)
            {
                self.emit(T::Str, p, close + 1);
                return None;
            }
            if lc == 100 && self.word("default", p) && !is_word(self.c(p + 7)) {
                self.emit(T::Default, p, p + 7);
                return None;
            }
            if let Some(kind) = self.direction(p) {
                let end = self.eol(self.dir_end[kind]);
                self.emit(DIRECTIONS[kind], p, end);
                return None;
            }
            if c == 10 {
                let j = self.src.run(p + 1, |d| d == 10);
                self.emit(T::Nl, p, j);
                return None;
            }
            if is_space(c) {
                self.p = self.ws_end(p);
                return None;
            }
        } else if c != 10 && is_space(c) {
            self.skip_blanks();
            return None;
        }
        if self.comment(c) {
            self.p = self.eol(p);
            return None;
        }
        if lc == 115 {
            if root && self.word("scale", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.emit(T::Scale, p, e);
                    self.push(S::Scale);
                    return None;
                }
            }
            if self.word("style", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.emit(T::Style, p, e);
                    self.push(S::Style);
                    return None;
                }
            }
            if self.word("state", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.p = e;
                    self.push(S::State);
                    return None;
                }
                if root && self.word("diagram", p + 5) {
                    let from = if self.word("-v2", p + 12) { p + 15 } else { p + 12 };
                    let e = self.ws_end(from);
                    if e > from {
                        self.emit(T::Sd, p, e);
                        return None;
                    }
                }
            }
        } else if lc == 99 {
            if self.word("class", p) {
                let def = self.word("def", p + 5);
                let from = if def { p + 8 } else { p + 5 };
                let e = self.ws_end(from);
                if e > from {
                    self.emit(if def { T::ClassDef } else { T::Class }, p, e);
                    self.push(if def { S::ClassDef } else { S::Class });
                    return None;
                }
            }
        } else if lc == 97 && root && (self.word("acctitle", p) || self.word("accdescr", p)) {
            let title = (self.c(p + 3) | 32) == 116;
            let q = self.ws_end(p + 8);
            let d = self.c(q);
            if d == 58 {
                let end = self.ws_end(q + 1);
                self.emit(if title { T::AccTitle } else { T::AccDescr }, p, end);
                self.push(if title { S::AccTitle } else { S::AccDescr });
                return None;
            }
            if d == 123 && !title {
                self.p = self.ws_end(q + 1);
                self.push(S::AccDescrMultiline);
                return None;
            }
        }
        if root {
            if c == 123 {
                self.emit(T::StructStart, p, p + 1);
                self.push(S::Struct);
                return None;
            }
        } else {
            if let Some(kind) = self.direction(p) {
                let end = self.eol(self.dir_end[kind]);
                self.emit(DIRECTIONS[kind], p, end);
                return None;
            }
            if c == 125 {
                self.emit(T::StructStop, p, p + 1);
                self.pop();
                return None;
            }
            if c == 10 {
                self.p += 1;
                return None;
            }
        }
        if lc == 110 && self.word("note", p) {
            let e = self.ws_end(p + 4);
            if e > p + 4 {
                self.emit(T::Note, p, e);
                self.push(S::Note);
                return None;
            }
        }
        if root && lc == 104 && self.word("hide empty description", p) && !is_word(self.c(p + 22)) {
            self.emit(T::HideEmpty, p, p + 22);
            return None;
        }
        if c == 91 && self.c(p + 1) == 42 && self.c(p + 2) == 93 {
            self.emit(T::EdgeState, p, p + 3);
            return None;
        }
        let j = self.src.run(p, |d| d != 58 && d != 45 && d != 123 && !space_unit(d));
        if j > p {
            self.emit_id(p, j);
            return None;
        }
        if c == 58 {
            let mut j = p + 1;
            while j < self.n {
                let d = self.src[j];
                if d == 10 || d == 59 {
                    break;
                }
                if d == 58 {
                    let after = self.c(j + 1);
                    if j + 1 >= self.n || after == 58 || after == 10 || after == 59 {
                        break;
                    }
                    j += 1;
                }
                j += 1;
            }
            if j > p + 1 {
                let text = self.src[p..j].trim_js().to_vec();
                self.emit_text(T::Descr, text, p, j);
                return None;
            }
            if self.c(p + 1) == 58 && self.c(p + 2) == 58 {
                self.emit(T::StyleSeparator, p, p + 3);
                return None;
            }
        } else if c == 45 && self.c(p + 1) == 45 {
            if self.c(p + 2) == 62 {
                self.emit(T::Arrow, p, p + 3);
                return None;
            }
            if !root {
                self.emit(T::Concurrent, p, p + 2);
                return None;
            }
        }
        if !root {
            return Some(self.stop(T::Error));
        }
        self.emit(T::Invalid, p, p + 1);
        None
    }

    /// One step after `state`. Answers the tokens when lexing stops here.
    fn state_name(&mut self, p: usize, c: Code) -> Option<Tokens> {
        if c != 10 && is_space(c) {
            self.skip_blanks();
            return None;
        }
        if self.comment(c) {
            self.p = self.eol(p);
            return None;
        }
        if let Some(kind) = self.stereotype(p) {
            let at = self.stereo_at[kind].expect("a kind that was found has a place");
            let choice = kind == 2 || kind == 5;
            let token = if choice {
                T::Choice
            } else if kind % 3 == 0 {
                T::Fork
            } else {
                T::Join
            };
            let text = self.src[p..at].trim_js().to_vec();
            self.emit_text(token, text, p, at + if choice { 10 } else { 8 });
            self.pop();
            return None;
        }
        if c == 34 {
            self.p += 1;
            self.push(S::StateString);
            return None;
        }
        let q = if c == 10 { self.ws_end(p) } else { p };
        if self.word("as", q) {
            let e = self.ws_end(q + 2);
            if e > q + 2 {
                self.emit(T::As, p, e);
                self.push(S::StateId);
                return None;
            }
        }
        if c == 10 {
            self.p += 1;
            self.pop();
            return None;
        }
        if c == 123 {
            self.emit(T::StructStart, p, p + 1);
            self.pop();
            self.push(S::Struct);
            return None;
        }
        if is_word(c) {
            let first = self.word_end(p);
            let second = self.ws_end(first);
            if second > first && is_word(self.c(second)) {
                let brace = self.next_brace.find(self.src, second + 1);
                if let Some(brace) = brace.filter(|&brace| brace < self.line_end(second)) {
                    let found = String::from_utf16_lossy(self.src[p..brace + 1].trim_js());
                    self.out.error = Some(format!("Error: State name must be a single word. Found: \"{found}\""));
                    return Some(self.stop(T::Error));
                }
            }
        }
        let j = self.src.run(p + 1, |d| d != 123 && !space_unit(d));
        self.emit(T::CompositState, p, j);
        None
    }

    /// One step in a note's text. Answers the tokens when lexing stops here.
    fn note_text(&mut self, p: usize) -> Option<Tokens> {
        let q = self.ws_end(p);
        if self.c(q) == 58 {
            let j = self.src.run(q + 1, |d| d != 58 && d != 10 && d != 59);
            if j > q + 1 {
                // Mermaid drops two characters here, expecting " :"; with no space before the
                // colon the first character of the note goes too.
                let text = self.src[p + 2..j].trim_js().to_vec();
                self.emit_text(T::NoteText, text, p, j);
                self.pop();
                return None;
            }
        }
        let mut k = self.src.find_unit(10, p);
        let mut e = 0;
        while let Some(line) = k {
            e = self.ws_end(line);
            if self.word("end note", e) && !is_word(self.c(e + 8)) {
                break;
            }
            k = self.src.find_unit(10, e);
        }
        if k.is_none() {
            return Some(self.stop(T::Error));
        }
        let text = self.src[p..e].trim_js().to_vec();
        self.emit_text(T::NoteText, text, p, e + 8);
        self.pop();
        None
    }

    /// Ends the stream with `kind`, leaving the lexer empty.
    fn stop(&mut self, kind: T) -> Tokens {
        let mut out = std::mem::replace(
            &mut self.out,
            Tokens { types: Vec::new(), texts: Vec::new(), starts: Vec::new(), error: None },
        );
        out.types.push(kind);
        out.texts.push(Vec::new());
        out.starts.push(self.p);
        out
    }
}

/// Pele's `tokenize`.
pub(super) fn tokenize(src: &[u16]) -> Tokens {
    let mut lexer = Lexer {
        src,
        n: src.len(),
        out: Tokens { types: Vec::new(), texts: Vec::new(), starts: Vec::new(), error: None },
        stack: Vec::new(),
        state: S::Initial,
        p: 0,
        next_direction: Finder::new(scan_direction),
        next_stereotype: Finder::new(scan_stereotype),
        next_line_end: Finder::new(scan_line_end),
        next_percent: Finder::new(scan_percent),
        next_brace: Finder::new(scan_brace),
        dir_from: None,
        dir_to: 0,
        dir_at: [None; 4],
        dir_end: [0; 4],
        stereo_from: None,
        stereo_to: 0,
        stereo_at: [None; 6],
    };
    lexer.run()
}
