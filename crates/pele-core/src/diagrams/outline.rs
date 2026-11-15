//! The indented outline grammar that Mermaid's mindmap and kanban share, ported from Pele's
//! `src/diagrams/common/outline.ts`. Kanban adds `@{ … }` metadata after a node and keeps `@` out
//! of node ids.

use super::scan::keyword_at;
use super::tokens::SyntaxError;
use crate::input::{JsText, Units, is_line_end, is_space, is_word};

/// A token of the outline grammar, in the order Pele numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    End,
    Error,
    Spaceline,
    Nl,
    Eof,
    Keyword,
    Spacelist,
    Icon,
    Class,
    NodeDstart,
    NodeId,
    NodeDescr,
    NodeDend,
    ShapeData,
}

impl Kind {
    /// The name the generated parser prints for the token. It has none for the end of input and
    /// prints its number.
    fn name(self, kanban: bool) -> &'static str {
        match self {
            Kind::End => "1",
            Kind::Error => "INVALID",
            Kind::Spaceline => "SPACELINE",
            Kind::Nl => "NL",
            Kind::Eof => "EOF",
            Kind::Keyword if kanban => "KANBAN",
            Kind::Keyword => "MINDMAP",
            Kind::Spacelist => "SPACELIST",
            Kind::Icon => "ICON",
            Kind::Class => "CLASS",
            Kind::NodeDstart => "NODE_DSTART",
            Kind::NodeId => "NODE_ID",
            Kind::NodeDescr => "NODE_DESCR",
            Kind::NodeDend => "NODE_DEND",
            Kind::ShapeData => "SHAPE_DATA",
        }
    }

    fn is_stop(self) -> bool {
        matches!(self, Kind::Spaceline | Kind::Nl | Kind::Eof)
    }

    /// Whether a statement can start with the token.
    fn starts(self) -> bool {
        matches!(self, Kind::Spaceline | Kind::Spacelist | Kind::Icon | Kind::Class | Kind::NodeDstart | Kind::NodeId)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: Kind,
    pub(crate) text: JsText,
    pub(crate) start: usize,
}

/// The shape a node's delimiters give it, as Pele's `NODE_TYPE` numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeShape {
    /// Also Mermaid's `NO_BORDER`.
    Default = 0,
    RoundedRect = 1,
    Rect = 2,
    Circle = 3,
    Cloud = 4,
    Bang = 5,
    Hexagon = 6,
}

impl NodeShape {
    /// Pele's `getType`: any opening delimiter pairs with any closing one, as in Mermaid.
    pub(crate) fn of(start: &[u16], end: &[u16]) -> NodeShape {
        match String::from_utf16_lossy(start).as_str() {
            "[" => NodeShape::Rect,
            "(" if end.is_str(")") => NodeShape::RoundedRect,
            "(" | ")" => NodeShape::Cloud,
            "((" => NodeShape::Circle,
            "))" => NodeShape::Bang,
            "{{" => NodeShape::Hexagon,
            _ => NodeShape::Default,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    Class,
    Icon,
    Node,
    /// A quoted description.
    Quoted,
    /// A Markdown description, between `` "` `` and `` `" ``.
    Markdown,
    Data,
    DataString,
}

/// Where a run of the units `keep` holds, from `at`, ends, when it holds at least one: a sticky
/// regular expression of one character class and `+`.
fn run_of(src: &[u16], at: usize, keep: impl Fn(u16) -> bool) -> Option<usize> {
    Some(src.run(at, keep)).filter(|&end| end > at)
}

/// `.+`
fn line(src: &[u16], at: usize) -> Option<usize> {
    run_of(src, at, |c| !is_line_end(c))
}

fn not_in(set: &'static [u8]) -> impl Fn(u16) -> bool {
    move |c| u8::try_from(c).map_or(true, |c| !set.contains(&c))
}

/// `text.replace(/\n\s*/g, '<br/>')`
fn line_breaks(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if text[at] == 10 {
            out.extend("<br/>".encode_utf16());
            at = text.run(at + 1, is_space);
        } else {
            out.push(text[at]);
            at += 1;
        }
    }
    out
}

/// Pele's outline `tokenize`. A text the grammar cannot read ends in an `Error` token where it
/// stops; any other ends in `Eof` (outside a node) and `End`.
pub(crate) fn tokenize(src: &[u16], kanban: bool) -> Vec<Token> {
    let n = src.len();
    let keyword = if kanban { "kanban" } else { "mindmap" };
    let id = if kanban { not_in(b"([\n){}@") } else { not_in(b"([\n){}") };
    let up_to = |unit: u16, from: usize| src.find_unit(unit, from).unwrap_or(n);
    let put = |tokens: &mut Vec<Token>, p: &mut usize, kind: Kind, text: &[u16], end: usize| {
        tokens.push(Token { kind, text: text.into(), start: *p });
        *p = end;
    };
    let mut tokens = Vec::new();
    let mut state = State::Initial;
    let mut p = 0;
    // The first line feed at or after a position already passed, kept for a run of positions.
    let mut next_break: Option<usize> = None;
    while p < n {
        let c = src[p];
        let d = src.at(p + 1);
        let (kind, end, next) = match state {
            State::Initial => {
                // `\s+` from here, where `c` could start one.
                let w = src.run(p, is_space);
                if kanban && c == 64 && d == Some(123) {
                    let end = p + 2;
                    put(&mut tokens, &mut p, Kind::ShapeData, &[], end);
                    state = State::Data;
                    continue;
                } else if src.at(w) == Some(37) && src.at(w + 1) == Some(37) {
                    (Kind::Spaceline, line(src, w).expect("a comment holds its marker"), State::Initial)
                } else if w > p {
                    let first_break = match next_break {
                        Some(at) if at >= p => at,
                        _ => *next_break.insert(up_to(10, p)),
                    };
                    if first_break >= w {
                        (Kind::Spacelist, w, State::Initial)
                    } else {
                        match src[..w].iter().rposition(|&c| c == 10).filter(|&at| at > p) {
                            Some(last) => (Kind::Spaceline, last + 1, State::Initial),
                            None => (Kind::Nl, p + 1, State::Initial),
                        }
                    }
                } else if keyword_at(src, p, keyword) && !src.at(p + keyword.len()).is_some_and(is_word) {
                    (Kind::Keyword, p + keyword.len(), State::Initial)
                } else if c == 58 && d == Some(58) && src.at(p + 2) == Some(58) {
                    p += 3;
                    state = State::Class;
                    continue;
                } else if c == 58 && d == Some(58) && keyword_at(src, p + 2, "icon") && src.at(p + 6) == Some(40) {
                    p += 7;
                    state = State::Icon;
                    continue;
                } else if (c == 45 && d == Some(41))
                    || (c == 40 && (d == Some(45) || d == Some(40)))
                    || ((c == 41 || c == 123) && d == Some(c))
                {
                    (Kind::NodeDstart, p + 2, State::Node)
                } else if c == 41 || c == 40 || c == 91 {
                    (Kind::NodeDstart, p + 1, State::Node)
                } else if let Some(end) = run_of(src, p, &id) {
                    (Kind::NodeId, end, State::Initial)
                } else {
                    break;
                }
            }
            State::Node => {
                if c == 34 {
                    let markdown = d == Some(96);
                    state = if markdown { State::Markdown } else { State::Quoted };
                    p += 1 + usize::from(markdown);
                    continue;
                } else if c == 41 || c == 40 {
                    let pair = d == Some(c) || (c == 40 && d == Some(45));
                    (Kind::NodeDend, p + 1 + usize::from(pair), State::Initial)
                } else if c == 93 {
                    (Kind::NodeDend, p + 1, State::Initial)
                } else if (c == 125 && d == Some(125)) || (c == 45 && d == Some(41)) {
                    (Kind::NodeDend, p + 2, State::Initial)
                } else {
                    // A lone closing brace takes the rest of its line as text.
                    let end = if c == 125 { line(src, p) } else { run_of(src, p, not_in(b")](}")) };
                    (Kind::NodeDescr, end.expect("a description holds its first unit"), State::Node)
                }
            }
            State::Quoted => {
                if c == 34 {
                    p += 1;
                    state = State::Node;
                    continue;
                }
                (Kind::NodeDescr, up_to(34, p), State::Quoted)
            }
            State::Markdown => {
                if c == 96 && d == Some(34) {
                    p += 2;
                    state = State::Node;
                    continue;
                }
                match run_of(src, p, not_in(b"`\"")) {
                    Some(end) => (Kind::NodeDescr, end, State::Markdown),
                    None => break,
                }
            }
            State::Class => {
                if let Some(end) = line(src, p) {
                    (Kind::Class, end, State::Initial)
                } else if c == 10 {
                    p += 1;
                    state = State::Initial;
                    continue;
                } else {
                    break;
                }
            }
            State::Icon => {
                if c == 41 {
                    p += 1;
                    state = State::Initial;
                    continue;
                }
                (Kind::Icon, up_to(41, p), State::Icon)
            }
            State::Data => {
                if c == 34 {
                    (Kind::ShapeData, p + 1, State::DataString)
                } else if c == 125 {
                    p += 1;
                    state = State::Initial;
                    continue;
                } else {
                    match run_of(src, p, not_in(b"}^\"")) {
                        Some(end) => (Kind::ShapeData, end, State::Data),
                        None => break,
                    }
                }
            }
            State::DataString => {
                if c == 34 {
                    (Kind::ShapeData, p + 1, State::Data)
                } else {
                    let end = up_to(34, p);
                    let text = line_breaks(&src[p..end]);
                    put(&mut tokens, &mut p, Kind::ShapeData, &text, end);
                    continue;
                }
            }
        };
        let text = &src[p..end];
        put(&mut tokens, &mut p, kind, text, end);
        state = next;
    }
    if p < n {
        tokens.push(Token { kind: Kind::Error, text: JsText::default(), start: p });
    } else {
        if state == State::Initial {
            tokens.push(Token { kind: Kind::Eof, text: JsText::default(), start: n });
        }
        tokens.push(Token { kind: Kind::End, text: JsText::default(), start: n });
    }
    tokens
}

/// What an icon or class line adds to the node before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decoration<'a> {
    Icon(&'a [u16]),
    Class(&'a [u16]),
}

/// One node of the outline, as the grammar's action hands it to the db.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutlineNode<'a> {
    /// How far the node's line is set in, in UTF-16 code units.
    pub(crate) level: usize,
    pub(crate) id: &'a [u16],
    pub(crate) descr: &'a [u16],
    pub(crate) shape: NodeShape,
    /// Kanban's `@{ … }` metadata, its quoted line breaks as `<br/>`.
    pub(crate) shape_data: Option<&'a [u16]>,
}

/// What mindmap's and kanban's dbs do with the grammar's actions.
pub(crate) trait OutlineDb {
    /// What an action can fail with, which the parse hands back as it is.
    type Error;
    fn add_node(&mut self, node: OutlineNode<'_>) -> Result<(), Self::Error>;
    fn decorate_node(&mut self, decoration: Decoration<'_>);
}

#[derive(Debug)]
pub(crate) enum OutlineError<E> {
    Syntax(SyntaxError),
    Db(E),
}

const STOP: &str = "'SPACELINE', 'NL', 'EOF'";

struct Parser {
    tokens: Vec<Token>,
    i: usize,
    kanban: bool,
}

impl Parser {
    fn kind(&self) -> Option<Kind> {
        self.tokens.get(self.i).map(|token| token.kind)
    }

    fn is(&self, kind: Kind) -> bool {
        self.kind() == Some(kind)
    }

    fn stops(&self) -> bool {
        self.kind().is_some_and(Kind::is_stop)
    }

    fn take(&mut self) -> JsText {
        self.i += 1;
        self.tokens[self.i - 1].text.clone()
    }

    /// The error the generated parser reports in this state, at the token it is on.
    fn fail<E>(&self, expected: &str) -> OutlineError<E> {
        let token = &self.tokens[self.i];
        OutlineError::Syntax(if token.kind == Kind::Error {
            SyntaxError { at: token.start, detail: String::new(), lexical: true }
        } else {
            let detail = format!("Expecting {expected}, got '{}'", token.kind.name(self.kanban));
            SyntaxError { at: token.start, detail, lexical: false }
        })
    }
}

/// Pele's `parseOutline`: makes the calls on `db` that the grammar's actions make, in the same
/// order, and reports errors in the state where the generated parser would.
pub(crate) fn parse_outline<D: OutlineDb>(src: &[u16], db: &mut D, kanban: bool) -> Result<(), OutlineError<D::Error>> {
    let mut r = Parser { tokens: tokenize(src, kanban), i: 0, kanban };
    let data = if kanban { ", 'SHAPE_DATA'" } else { "" };
    let keyword = format!("'{}'", Kind::Keyword.name(kanban));
    if r.is(Kind::Spaceline) {
        r.i += 1;
        while r.is(Kind::Spaceline) || r.is(Kind::Nl) {
            r.i += 1;
        }
        if !r.is(Kind::Keyword) {
            return Err(r.fail(&format!("'SPACELINE', 'NL', {keyword}")));
        }
    } else if !r.is(Kind::Keyword) {
        return Err(r.fail(&format!("'SPACELINE', {keyword}")));
    }
    r.i += 1;
    let mut first = "'SPACELINE', 'NL', 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'";
    if r.is(Kind::Nl) {
        r.i += 1;
        first = "'SPACELINE', 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'";
    }
    loop {
        let indented = r.is(Kind::Spacelist);
        let level = if indented { r.take().len() } else { 0 };
        match r.kind() {
            Some(t @ (Kind::NodeId | Kind::NodeDstart)) => {
                let id = (t == Kind::NodeId).then(|| r.take());
                let (descr, shape) = if r.is(Kind::NodeDstart) {
                    let start = r.take();
                    if !r.is(Kind::NodeDescr) {
                        return Err(r.fail("'NODE_DESCR'"));
                    }
                    let descr = r.take();
                    if !r.is(Kind::NodeDend) {
                        return Err(r.fail("'NODE_DEND'"));
                    }
                    let end = r.take();
                    if !r.stops() && !r.is(Kind::ShapeData) {
                        return Err(r.fail(&format!("{STOP}{data}")));
                    }
                    (descr, NodeShape::of(&start, &end))
                } else {
                    if !r.stops() && !r.is(Kind::ShapeData) {
                        return Err(r.fail(&format!("{STOP}, 'NODE_DSTART'{data}")));
                    }
                    (id.clone().expect("a node without delimiters has an id"), NodeShape::Default)
                };
                let shape_data = if r.is(Kind::ShapeData) {
                    let mut shape_data = r.take();
                    while r.is(Kind::ShapeData) {
                        shape_data.push_units(&r.take());
                    }
                    if !r.stops() {
                        return Err(r.fail(&format!("{STOP}{data}")));
                    }
                    Some(shape_data)
                } else {
                    None
                };
                let id = id.as_deref().unwrap_or(&descr);
                let node = OutlineNode { level, id, descr: &descr, shape, shape_data: shape_data.as_deref() };
                db.add_node(node).map_err(OutlineError::Db)?;
            }
            Some(t @ (Kind::Icon | Kind::Class)) => {
                let text = r.take();
                if !r.stops() {
                    return Err(r.fail(STOP));
                }
                db.decorate_node(if t == Kind::Icon { Decoration::Icon(&text) } else { Decoration::Class(&text) });
            }
            _ if indented => {
                if !r.stops() {
                    return Err(r.fail(&format!("{STOP}, 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'")));
                }
            }
            Some(Kind::Spaceline) => {
                r.i += 1;
                if !r.stops() {
                    return Err(r.fail(STOP));
                }
            }
            _ => return Err(r.fail(first)),
        }
        r.i += 1;
        while r.is(Kind::Nl) || r.is(Kind::Eof) {
            r.i += 1;
        }
        if r.is(Kind::End) {
            return Ok(());
        }
        if !r.kind().is_some_and(Kind::starts) {
            return Err(r.fail(&format!("{STOP}, 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'")));
        }
    }
}
