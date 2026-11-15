//! Pele's C4 lexer (`src/diagrams/c4/lexer.ts`), which follows the lexer Jison compiles from
//! Mermaid's `c4Diagram.jison`: its rule order, its start conditions, and what it returns once
//! the input runs out in each of them.

use crate::input::{Units, is_line_end, is_space, is_word};

/// Pele's token types, in its order: the parser compares them by place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub(super) enum T {
    End,
    Error,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    Title,
    AccDescription,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Newline,
    C4Context,
    C4Container,
    C4Component,
    C4Dynamic,
    C4Deployment,
    Person,
    PersonExt,
    System,
    SystemDb,
    SystemQueue,
    SystemExt,
    SystemExtDb,
    SystemExtQueue,
    Container,
    ContainerDb,
    ContainerQueue,
    ContainerExt,
    ContainerExtDb,
    ContainerExtQueue,
    Component,
    ComponentDb,
    ComponentQueue,
    ComponentExt,
    ComponentExtDb,
    ComponentExtQueue,
    EnterpriseBoundary,
    SystemBoundary,
    Boundary,
    ContainerBoundary,
    Node,
    NodeL,
    NodeR,
    Rel,
    BiRel,
    RelU,
    RelD,
    RelL,
    RelR,
    RelB,
    RelIndex,
    UpdateElStyle,
    UpdateRelStyle,
    UpdateLayoutConfig,
    EofInStruct,
    AttributeEmpty,
    Str,
    StrKey,
    StrValue,
    LBrace,
    RBrace,
    Eof,
}

/// Pele's `TOKEN_NAMES`, by token type.
const NAMES: [&str; 65] = [
    "$end",
    "INVALID",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "title",
    "accDescription",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "NEWLINE",
    "C4_CONTEXT",
    "C4_CONTAINER",
    "C4_COMPONENT",
    "C4_DYNAMIC",
    "C4_DEPLOYMENT",
    "PERSON",
    "PERSON_EXT",
    "SYSTEM",
    "SYSTEM_DB",
    "SYSTEM_QUEUE",
    "SYSTEM_EXT",
    "SYSTEM_EXT_DB",
    "SYSTEM_EXT_QUEUE",
    "CONTAINER",
    "CONTAINER_DB",
    "CONTAINER_QUEUE",
    "CONTAINER_EXT",
    "CONTAINER_EXT_DB",
    "CONTAINER_EXT_QUEUE",
    "COMPONENT",
    "COMPONENT_DB",
    "COMPONENT_QUEUE",
    "COMPONENT_EXT",
    "COMPONENT_EXT_DB",
    "COMPONENT_EXT_QUEUE",
    "ENTERPRISE_BOUNDARY",
    "SYSTEM_BOUNDARY",
    "BOUNDARY",
    "CONTAINER_BOUNDARY",
    "NODE",
    "NODE_L",
    "NODE_R",
    "REL",
    "BIREL",
    "REL_U",
    "REL_D",
    "REL_L",
    "REL_R",
    "REL_B",
    "REL_INDEX",
    "UPDATE_EL_STYLE",
    "UPDATE_REL_STYLE",
    "UPDATE_LAYOUT_CONFIG",
    "EOF_IN_STRUCT",
    "ATTRIBUTE_EMPTY",
    "STR",
    "STR_KEY",
    "STR_VALUE",
    "LBRACE",
    "RBRACE",
    "EOF",
];

impl T {
    pub(super) fn name(self) -> &'static str {
        NAMES[self as usize]
    }

    /// The token `offset` places after this one.
    pub(super) fn after(self, offset: usize) -> T {
        ALL[self as usize + offset]
    }
}

const ALL: [T; 65] = {
    use T::*;
    [
        End,
        Error,
        DirectionTb,
        DirectionBt,
        DirectionRl,
        DirectionLr,
        Title,
        AccDescription,
        AccTitle,
        AccTitleValue,
        AccDescr,
        AccDescrValue,
        AccDescrMultilineValue,
        Newline,
        C4Context,
        C4Container,
        C4Component,
        C4Dynamic,
        C4Deployment,
        Person,
        PersonExt,
        System,
        SystemDb,
        SystemQueue,
        SystemExt,
        SystemExtDb,
        SystemExtQueue,
        Container,
        ContainerDb,
        ContainerQueue,
        ContainerExt,
        ContainerExtDb,
        ContainerExtQueue,
        Component,
        ComponentDb,
        ComponentQueue,
        ComponentExt,
        ComponentExtDb,
        ComponentExtQueue,
        EnterpriseBoundary,
        SystemBoundary,
        Boundary,
        ContainerBoundary,
        Node,
        NodeL,
        NodeR,
        Rel,
        BiRel,
        RelU,
        RelD,
        RelL,
        RelR,
        RelB,
        RelIndex,
        UpdateElStyle,
        UpdateRelStyle,
        UpdateLayoutConfig,
        EofInStruct,
        AttributeEmpty,
        Str,
        StrKey,
        StrValue,
        LBrace,
        RBrace,
        Eof,
    ]
};

const KEYWORDS: [(&str, T); 48] = [
    ("C4Context", T::C4Context),
    ("C4Container", T::C4Container),
    ("C4Component", T::C4Component),
    ("C4Dynamic", T::C4Dynamic),
    ("C4Deployment", T::C4Deployment),
    ("Person", T::Person),
    ("Person_Ext", T::PersonExt),
    ("System", T::System),
    ("SystemDb", T::SystemDb),
    ("SystemQueue", T::SystemQueue),
    ("System_Ext", T::SystemExt),
    ("SystemDb_Ext", T::SystemExtDb),
    ("SystemQueue_Ext", T::SystemExtQueue),
    ("Container", T::Container),
    ("ContainerDb", T::ContainerDb),
    ("ContainerQueue", T::ContainerQueue),
    ("Container_Ext", T::ContainerExt),
    ("ContainerDb_Ext", T::ContainerExtDb),
    ("ContainerQueue_Ext", T::ContainerExtQueue),
    ("Component", T::Component),
    ("ComponentDb", T::ComponentDb),
    ("ComponentQueue", T::ComponentQueue),
    ("Component_Ext", T::ComponentExt),
    ("ComponentDb_Ext", T::ComponentExtDb),
    ("ComponentQueue_Ext", T::ComponentExtQueue),
    ("Enterprise_Boundary", T::EnterpriseBoundary),
    ("System_Boundary", T::SystemBoundary),
    ("Boundary", T::Boundary),
    ("Container_Boundary", T::ContainerBoundary),
    ("Deployment_Node", T::Node),
    ("Node", T::Node),
    ("Node_L", T::NodeL),
    ("Node_R", T::NodeR),
    ("Rel", T::Rel),
    ("BiRel", T::BiRel),
    ("Rel_Up", T::RelU),
    ("Rel_U", T::RelU),
    ("Rel_Down", T::RelD),
    ("Rel_D", T::RelD),
    ("Rel_Left", T::RelL),
    ("Rel_L", T::RelL),
    ("Rel_Right", T::RelR),
    ("Rel_R", T::RelR),
    ("Rel_Back", T::RelB),
    ("RelIndex", T::RelIndex),
    ("UpdateElementStyle", T::UpdateElStyle),
    ("UpdateRelStyle", T::UpdateRelStyle),
    ("UpdateLayoutConfig", T::UpdateLayoutConfig),
];

const DIRECTIONS: [&str; 4] = ["TB", "BT", "RL", "LR"];

/// Token `i` is `src[starts[i]..ends[i]]`.
pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    pub(super) starts: Vec<usize>,
    pub(super) ends: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Struct,
    Attribute,
    String,
    StringKv,
    StringKvKey,
    StringKvValue,
    Acc,
    AccDescrMultiline,
}

struct Lexer<'a> {
    src: &'a [u16],
    p: usize,
    tokens: Tokens,
}

impl Lexer<'_> {
    fn at(&self, q: usize) -> Option<u16> {
        self.src.get(q).copied()
    }

    fn is(&self, q: usize, c: u8) -> bool {
        self.at(q) == Some(u16::from(c))
    }

    fn emit(&mut self, kind: T, start: usize, end: usize) {
        self.tokens.types.push(kind);
        self.tokens.starts.push(start);
        self.tokens.ends.push(end);
        self.p = end;
    }

    fn skip_ws(&self, q: usize) -> usize {
        self.src.run(q, is_space)
    }

    /// `src.indexOf(ch, from)`, or the end where there is none.
    fn up_to(&self, ch: u8, from: usize) -> usize {
        self.src.find_unit(u16::from(ch), from).unwrap_or(self.src.len())
    }

    fn skip_while(&self, mut q: usize, c: u8) -> usize {
        while self.is(q, c) {
            q += 1;
        }
        q
    }

    /// `word\s[^#\n;]+`
    fn statement(&mut self, word: &str, kind: T) -> bool {
        let q = self.p + word.len();
        if !self.src.starts_at(self.p, word) || !self.at(q).is_some_and(is_space) {
            return false;
        }
        let from = q + 1;
        let end = self.src.run(from, |c| !matches!(c, 35 | 10 | 59));
        if end == from {
            return false;
        }
        self.emit(kind, self.p, end);
        true
    }

    /// `/title[^\S\r\n]*(?=[\r\n]|$)\s*/y`: a title with nothing after it, and the blank lines
    /// that follow it. Where it ends, if it matches here.
    fn bare_title(&self) -> Option<usize> {
        if !self.src.starts_at(self.p, "title") {
            return None;
        }
        let q = self.src.run(self.p + 5, |c| is_space(c) && c != 13 && c != 10);
        match self.at(q) {
            None | Some(10 | 13) => Some(self.skip_ws(q)),
            Some(_) => None,
        }
    }

    /// `/.*direction\s+NAME[^\n]*/y` at `p`, for the line that ends at `line_end`: the greedy `.*`
    /// takes the last `direction` on the line that the rest matches after.
    fn direction(&self, name: &str, line_end: usize) -> Option<usize> {
        let p = self.p;
        let last = line_end.checked_sub(9).filter(|&last| last >= p)?;
        (p..=last).rev().find_map(|k| {
            if !self.src.starts_at(k, "direction") {
                return None;
            }
            let gap = self.src.run(k + 9, is_space);
            (gap > k + 9 && self.src.starts_at(gap, name)).then(|| self.up_to(b'\n', gap + 2))
        })
    }
}

/// Pele's `tokenize`.
pub(super) fn tokenize(src: &[u16]) -> Tokens {
    let n = src.len();
    let mut lx = Lexer { src, p: 0, tokens: Tokens { types: Vec::new(), starts: Vec::new(), ends: Vec::new() } };
    let mut state = S::Initial;
    let mut acc_value = T::AccTitleValue;
    let mut done = false;
    // Where the next `direction` is, `None` once there is none; searched for only once `p` has
    // passed the last one found.
    let mut dir_next: Option<Option<usize>> = None;
    let mut line_end: Option<usize> = None;
    let mut dir_failed = 0;

    loop {
        // Jison notes the end of input one call before it reports it, so a rule that matches
        // nothing gets one last turn.
        if done {
            lx.emit(T::End, n, n);
            break;
        }
        let p = lx.p;
        if p >= n {
            done = true;
        }
        let c = lx.at(p);

        match state {
            S::Initial => {
                if p >= n {
                    lx.emit(T::Eof, n, n);
                    continue;
                }
                if dir_next != Some(None) && p >= dir_failed {
                    if dir_next.is_none_or(|next| next.is_some_and(|next| next < p)) {
                        dir_next = Some(src.find_str("direction", p));
                    }
                    if let Some(Some(next)) = dir_next {
                        if line_end.is_none_or(|end| end < p) {
                            line_end = Some(src.run(p, |c| !is_line_end(c)));
                        }
                        let end = line_end.expect("set above");
                        if next < end {
                            let found = DIRECTIONS
                                .iter()
                                .enumerate()
                                .find_map(|(i, name)| lx.direction(name, end).map(|stop| (i, stop)));
                            if let Some((i, stop)) = found {
                                lx.emit(T::DirectionTb.after(i), p, stop);
                                continue;
                            }
                            // No direction statement starts anywhere in the rest of this line either.
                            dir_failed = end;
                        }
                    }
                }
                if c == Some(116) {
                    if let Some(end) = lx.bare_title() {
                        lx.p = end;
                        continue;
                    }
                    if lx.statement("title", T::Title) {
                        continue;
                    }
                }
                if c == Some(97) {
                    if lx.statement("accDescription", T::AccDescription) {
                        continue;
                    }
                    let title = src.starts_at(p, "accTitle");
                    if title || src.starts_at(p, "accDescr") {
                        let q = lx.skip_ws(p + 8);
                        if lx.is(q, b':') {
                            let end = lx.skip_ws(q + 1);
                            lx.emit(if title { T::AccTitle } else { T::AccDescr }, p, end);
                            acc_value = if title { T::AccTitleValue } else { T::AccDescrValue };
                            state = S::Acc;
                            continue;
                        }
                        if lx.is(q, b'{') && !title {
                            lx.p = lx.skip_ws(q + 1);
                            state = S::AccDescrMultiline;
                            continue;
                        }
                    }
                }
                let c = c.expect("p is inside the text");
                if c == 37 && lx.is(p + 1, b'%') {
                    // A comment takes the line breaks after it, so no NEWLINE follows.
                    let q = lx.up_to(b'\n', p + 2);
                    lx.p = src.run(q, |c| c == 10 || c == 13);
                    continue;
                }
                if is_space(c) {
                    let end = src.run(p, is_space);
                    match src[p..end].iter().rposition(|&d| d == 10) {
                        Some(last) => lx.emit(T::Newline, p, p + last + 1),
                        None => lx.p = end,
                    }
                    continue;
                }
                if c == 123 {
                    lx.emit(T::LBrace, p, p + 1);
                    continue;
                }
                if c == 125 {
                    lx.emit(T::RBrace, p, p + 1);
                    continue;
                }
                if is_word(c) {
                    let q = src.run(p + 1, is_word);
                    let keyword = (q - p <= 19)
                        .then(|| KEYWORDS.iter().find(|(word, _)| src[p..q].iter().copied().eq(word.encode_utf16())))
                        .flatten();
                    if let Some(&(_, kind)) = keyword {
                        if kind > T::C4Deployment {
                            state = S::Struct;
                        }
                        lx.emit(kind, p, q);
                        continue;
                    }
                }
                lx.emit(T::Error, p, p);
                break;
            }
            S::Struct => match c {
                None => lx.emit(T::EofInStruct, n, n),
                Some(40) => {
                    let q = lx.skip_while(p + 1, b' ');
                    if lx.is(q, b',') {
                        lx.emit(T::AttributeEmpty, p, q + 1);
                    } else {
                        lx.p += 1;
                    }
                    state = S::Attribute;
                }
                Some(41) => {
                    lx.p += 1;
                    state = S::Initial;
                }
                Some(_) => {
                    lx.emit(T::Error, p, p);
                    break;
                }
            },
            S::Attribute => match c {
                None => {}
                Some(41) => {
                    lx.p += 1;
                    state = S::Initial;
                }
                Some(44) => {
                    if lx.is(p + 1, b',') {
                        lx.emit(T::AttributeEmpty, p, p + 2);
                    } else {
                        lx.p += 1;
                    }
                }
                Some(_) => {
                    let q = lx.skip_while(p, b' ');
                    if lx.is(q, b'"') {
                        if lx.is(q + 1, b'"') {
                            lx.emit(T::AttributeEmpty, p, q + 2);
                        } else {
                            lx.p = q + 1;
                            state = S::String;
                        }
                    } else if lx.is(q, b'$') {
                        lx.p = q + 1;
                        state = S::StringKv;
                    } else {
                        // An unquoted value runs to the next comma, through closing parentheses
                        // and line breaks.
                        let end = lx.up_to(b',', p);
                        lx.emit(T::Str, p, end);
                    }
                }
            },
            S::String => {
                if c == Some(34) {
                    lx.p += 1;
                    state = S::Attribute;
                } else {
                    let end = lx.up_to(b'"', p);
                    lx.emit(T::Str, p, end);
                }
            }
            S::StringKv => {
                let end = lx.up_to(b'=', p);
                lx.emit(T::StrKey, p, end);
                state = S::StringKvKey;
            }
            S::StringKvKey => {
                if c.is_none() {
                    continue;
                }
                let equals = c == Some(61);
                let q = if equals { lx.skip_while(p + 1, b' ') } else { p + 1 };
                if !equals || !lx.is(q, b'"') {
                    lx.emit(T::Error, p, p);
                    break;
                }
                lx.p = q + 1;
                state = S::StringKvValue;
            }
            S::StringKvValue => match c {
                None => {}
                Some(34) => {
                    lx.p += 1;
                    state = S::Attribute;
                }
                Some(_) => {
                    let end = lx.up_to(b'"', p);
                    lx.emit(T::StrValue, p, end);
                }
            },
            S::Acc => {
                let end = lx.up_to(b'\n', p);
                lx.emit(acc_value, p, end);
                state = S::Initial;
            }
            S::AccDescrMultiline => {
                if c == Some(125) {
                    lx.p += 1;
                    state = S::Initial;
                } else {
                    let end = lx.up_to(b'}', p);
                    lx.emit(T::AccDescrMultilineValue, p, end);
                }
            }
        }
    }
    lx.tokens
}
