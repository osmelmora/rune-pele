//! The agentflow parser (Pele's `src/diagrams/agentflow/parser.ts`): a hand-written parser for
//! Mermaid's `agentflow.jison` grammar that reports errors in jison's words, and tells the db
//! where each statement sits.

use super::db::{AgentflowDb, DocItem, LinkInfo, LinkPositions, Loc, VertexUpdate, destruct_link};
use super::lexer::{DIRECTION_STATEMENTS, T, Token, tokenize};
use super::shapes::NodeShape;
use super::{Link, StatementType};
use crate::RenderError;
use crate::catalog::ParseContext;
use crate::diagrams::flowchart::{Direction, FlowText, LabelType, LinkTarget, ShapeName};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{Edit, JsObject, JsText, JsValue, Units, input_offset, is_space};

/// Tokens that may make up an id.
fn is_id(t: T) -> bool {
    matches!(
        t,
        T::Num
            | T::NodeString
            | T::Down
            | T::Minus
            | T::Default
            | T::Comma
            | T::Colon
            | T::Amp
            | T::Brkt
            | T::Mult
            | T::UnicodeText
    )
}

/// Tokens that may make up a callback name or a curve.
fn is_alpha_num(t: T) -> bool {
    matches!(
        t,
        T::Num
            | T::UnicodeText
            | T::NodeString
            | T::Dir
            | T::Down
            | T::Minus
            | T::Comma
            | T::Colon
            | T::Amp
            | T::Brkt
            | T::Mult
    )
}

/// Tokens that may make up the name of a flow or a connector.
fn is_no_tags(t: T) -> bool {
    matches!(
        t,
        T::Num
            | T::NodeString
            | T::Space
            | T::Minus
            | T::Amp
            | T::UnicodeText
            | T::Colon
            | T::Mult
            | T::Brkt
            | T::Style
            | T::LinkStyle
            | T::ClassDef
            | T::Class
            | T::Click
            | T::Graph
            | T::Dir
            | T::Flow
            | T::Connector
            | T::Global
            | T::EndKeyword
            | T::Down
            | T::Up
            | T::StartLink
    )
}

fn is_style_part(t: T) -> bool {
    matches!(t, T::Num | T::NodeString | T::Colon | T::Space | T::Brkt | T::Style)
}

fn is_text_part(t: T) -> bool {
    matches!(t, T::Text | T::TagStart | T::TagEnd | T::UnicodeText)
}

fn is_separator(t: T) -> bool {
    matches!(t, T::Newline | T::Semi | T::Eof | T::Comment)
}

const TEXT_TOKENS: &str = "'UNICODE_TEXT', 'TEXT', 'TAGSTART'";
const SEPARATORS: &str = "'COMMENT', 'SEMI', 'NEWLINE', 'EOF'";
const STATEMENT_START: &str = "'COMMENT', 'SEMI', 'NEWLINE', 'SPACE', 'EOF', 'flow', 'global', 'connector', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'STYLE', 'LINKSTYLE', 'CLASSDEF', 'CLASS', 'CLICK', 'NODE_STRING'";

/// `/}[^\S\n]*\n/g`, which Mermaid replaces with `}\n` before parsing: each match, as an edit.
fn brace_trails(source: &[u16]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut at = 0;
    while at < source.len() {
        if source[at] == u16::from(b'}') {
            let end = source.run(at + 1, |c| is_space(c) && c != 10);
            if source.at(end) == Some(10) {
                edits.push((at, end + 1, 2));
                at = end + 1;
                continue;
            }
        }
        at += 1;
    }
    edits
}

/// What a block is opened as.
enum Block {
    Global,
    /// A flow: its id and bracketed title, where the header gives them, and its metadata.
    Flow {
        id: Option<FlowText>,
        title: Option<FlowText>,
        data: Option<(Vec<u16>, Loc)>,
    },
}

/// One open block, with the list and collect flag of the document around it.
struct Frame {
    block: Block,
    /// Token index of the keyword that opens the block.
    at: usize,
    list: Vec<DocItem>,
    collect: bool,
}

/// What the header of a flow or a connector holds, up to its separator.
struct Header {
    id: FlowText,
    title: FlowText,
    data: Option<(Vec<u16>, Loc)>,
}

struct Parser<'a, 'd, 'c> {
    cx: &'a ParseContext<'c>,
    db: &'a mut AgentflowDb<'d, 'c>,
    /// The text the lexer read: the source with the blanks after each closing brace removed.
    src: Vec<u16>,
    tokens: Vec<Token>,
    i: usize,
    edits: Vec<Edit>,
    pending_data: Option<(Vec<u16>, Loc)>,
}

type Parsed<T> = Result<T, RenderError>;

impl Parser<'_, '_, '_> {
    fn kind_at(&self, i: usize) -> T {
        self.tokens.get(i).map_or(T::End, |token| token.kind)
    }

    fn kind(&self) -> T {
        self.kind_at(self.i)
    }

    fn take(&mut self) -> Vec<u16> {
        self.i += 1;
        self.tokens[self.i - 1].text.clone()
    }

    fn fail<X>(&self, expected: &str) -> Parsed<X> {
        let i = self.i.min(self.tokens.len() - 1);
        let token = &self.tokens[i];
        let mut from = token.start;
        if matches!(token.kind, T::Eof | T::End) {
            // Comments are tokens here, so skip trailing ones.
            let mut k = i;
            while k > 0 && matches!(self.tokens[k - 1].kind, T::Comment | T::Newline | T::Space | T::Eof) {
                k -= 1;
            }
            from = self.src[..self.tokens[k].start].trim_end_js().len();
        }
        let at = input_offset(&vec![self.edits.clone()], from);
        let lexical = token.kind == T::Error;
        let detail = if lexical { String::new() } else { format!("Expecting {expected}, got '{}'", token.kind.name()) };
        Err(self.cx.syntax(SyntaxError { at, detail, lexical }))
    }

    /// The span from the start of token `a` to the end of token `b`.
    fn span(&self, a: usize, b: usize) -> Loc {
        Loc { first: self.tokens[a].first, last: self.tokens[b].last }
    }

    fn expect(&mut self, kind: T, also: &str) -> Parsed<Vec<u16>> {
        if self.kind() != kind {
            let also = if also.is_empty() { String::new() } else { format!(", {also}") };
            return self.fail(&format!("'{}'{also}", kind.name()));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Parsed<()> {
        if !is_separator(self.kind()) {
            return self.fail(SEPARATORS);
        }
        self.i += 1;
        Ok(())
    }

    /// One or more tokens of a class, joined.
    fn run_of(&mut self, part: fn(T) -> bool, expected: &str) -> Parsed<Vec<u16>> {
        if !part(self.kind()) {
            return self.fail(expected);
        }
        let mut s = self.take();
        while part(self.kind()) {
            s.extend(self.take());
        }
        Ok(s)
    }

    fn id_string(&mut self) -> Parsed<Vec<u16>> {
        self.run_of(is_id, "'NODE_STRING'")
    }

    fn alpha_num(&mut self) -> Parsed<Vec<u16>> {
        self.run_of(is_alpha_num, "'NODE_STRING'")
    }

    fn text(&mut self) -> Parsed<FlowText> {
        let kind = match self.kind() {
            T::Str => LabelType::String,
            T::MdStr => LabelType::Markdown,
            t if is_text_part(t) => LabelType::Text,
            _ => return self.fail(&format!("{TEXT_TOKENS}, 'TAGEND', 'STR', 'MD_STR'")),
        };
        let mut s = self.take();
        while is_text_part(self.kind()) {
            s.extend(self.take());
        }
        Ok(FlowText { text: s.into(), kind })
    }

    fn closed(&mut self, kind: T) -> Parsed<()> {
        self.expect(kind, TEXT_TOKENS).map(drop)
    }

    fn edge_text(&mut self) -> Parsed<FlowText> {
        let kind = match self.kind() {
            T::Str => LabelType::String,
            T::MdStr => LabelType::Markdown,
            T::EdgeText | T::UnicodeText => LabelType::Text,
            _ => return self.fail("'EDGE_TEXT', 'STR', 'MD_STR'"),
        };
        let mut s = self.take();
        while matches!(self.kind(), T::EdgeText | T::UnicodeText) {
            s.extend(self.take());
        }
        Ok(FlowText { text: s.into(), kind })
    }

    fn text_no_tags(&mut self) -> Parsed<FlowText> {
        let kind = match self.kind() {
            T::Str => LabelType::Text,
            T::MdStr => LabelType::Markdown,
            t if is_no_tags(t) => LabelType::Text,
            _ => return self.fail("'NODE_STRING', 'STR', 'MD_STR'"),
        };
        let mut s = self.take();
        while is_no_tags(self.kind()) {
            s.extend(self.take());
        }
        Ok(FlowText { text: s.into(), kind })
    }

    /// The body of a metadata block, and where the block sits.
    fn shape_data(&mut self) -> (Vec<u16>, Loc) {
        let from = self.i;
        let mut s = self.take();
        while self.kind() == T::ShapeData {
            s.extend(self.take());
        }
        (s, self.span(from, self.i - 1))
    }

    /// Gives a node the metadata block that follows it, and widens the node's span over it.
    fn attach(&mut self, id: &[u16], data: (Vec<u16>, Loc)) -> Parsed<()> {
        let loc = data.1;
        self.db.add_vertex(id, VertexUpdate { metadata: Some(data), ..VertexUpdate::default() })?;
        self.db.extend_vertex_mapping(id, loc);
        Ok(())
    }

    /// Gives a flow or a connector the metadata its header holds.
    fn attach_data(&mut self, id: &[u16], data: Option<(Vec<u16>, Loc)>) -> Parsed<()> {
        match data {
            Some(data) => self.db.add_vertex(id, VertexUpdate { metadata: Some(data), ..VertexUpdate::default() }),
            None => Ok(()),
        }
    }

    /// A label closed by `close`, for a vertex with `shape`.
    fn shaped(&mut self, close: T, shape: ShapeName) -> Parsed<VertexUpdate> {
        self.i += 1;
        let label = self.text()?;
        self.closed(close)?;
        Ok(labelled(label, shape))
    }

    fn vertex(&mut self) -> Parsed<JsText> {
        let from = self.i;
        let id = self.id_string()?;
        let update = match self.kind() {
            T::Sqs => self.shaped(T::Sqe, ShapeName::SQUARE)?,
            T::DoubleCircleStart => self.shaped(T::DoubleCircleEnd, ShapeName::DOUBLECIRCLE)?,
            T::Ps => {
                self.i += 1;
                if self.kind() == T::Ps {
                    self.i += 1;
                    let label = self.text()?;
                    self.closed(T::Pe)?;
                    self.expect(T::Pe, "")?;
                    labelled(label, ShapeName::CIRCLE)
                } else {
                    let label = self.text()?;
                    self.closed(T::Pe)?;
                    labelled(label, ShapeName::ROUND)
                }
            }
            T::EllipseStart => self.shaped(T::EllipseEnd, ShapeName::ELLIPSE)?,
            T::StadiumStart => self.shaped(T::StadiumEnd, ShapeName::STADIUM)?,
            T::SubroutineStart => self.shaped(T::SubroutineEnd, ShapeName::SUBROUTINE)?,
            T::VertexWithPropsStart => {
                self.i += 1;
                let field = self.expect(T::NodeString, "")?;
                self.expect(T::Colon, "")?;
                let value = self.expect(T::NodeString, "")?;
                self.expect(T::Pipe, "")?;
                let label = self.text()?;
                self.closed(T::Sqe)?;
                let mut props = JsObject::default();
                props.set(field.into(), JsValue::String(value.into()));
                VertexUpdate { props: Some(props), ..labelled(label, ShapeName::RECT) }
            }
            T::CylinderStart => self.shaped(T::CylinderEnd, ShapeName::CYLINDER)?,
            T::DiamondStart => {
                self.i += 1;
                if self.kind() == T::DiamondStart {
                    self.i += 1;
                    let label = self.text()?;
                    self.closed(T::DiamondStop)?;
                    self.expect(T::DiamondStop, "")?;
                    labelled(label, ShapeName::HEXAGON)
                } else {
                    let label = self.text()?;
                    self.closed(T::DiamondStop)?;
                    labelled(label, ShapeName::DIAMOND)
                }
            }
            T::TagEnd => self.shaped(T::Sqe, ShapeName::ODD)?,
            T::TrapStart => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::InvTrapEnd {
                    self.i += 1;
                    labelled(label, ShapeName::LEAN_RIGHT)
                } else {
                    self.closed(T::TrapEnd)?;
                    labelled(label, ShapeName::TRAPEZOID)
                }
            }
            T::InvTrapStart => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::TrapEnd {
                    self.i += 1;
                    labelled(label, ShapeName::LEAN_LEFT)
                } else {
                    self.closed(T::InvTrapEnd)?;
                    labelled(label, ShapeName::INV_TRAPEZOID)
                }
            }
            _ => VertexUpdate::default(),
        };
        let shaped = update.shape.is_some();
        self.db.add_vertex(&id, update)?;
        self.db.add_vertex_mapping(&id, shaped, self.span(from, self.i - 1));
        Ok(id.into())
    }

    fn styled_vertex(&mut self) -> Parsed<JsText> {
        let id = self.vertex()?;
        if self.kind() == T::StyleSeparator {
            self.i += 1;
            let class = self.id_string()?;
            self.db.set_class(&id, &class);
        }
        Ok(id)
    }

    /// A node, or several joined by `&`. Metadata after the last one is left in `pending_data`
    /// for the caller.
    fn node(&mut self) -> Parsed<Vec<JsText>> {
        let mut ids = vec![self.styled_vertex()?];
        self.pending_data = None;
        loop {
            let data = (self.kind() == T::ShapeData).then(|| self.shape_data());
            if self.kind() != T::Space {
                self.pending_data = data;
                return Ok(ids);
            }
            while self.kind() == T::Space {
                self.i += 1;
            }
            if self.kind() != T::Amp {
                if data.is_some() {
                    return self.fail("'AMP'");
                }
                return Ok(ids);
            }
            self.i += 1;
            if self.kind() != T::Space {
                return self.fail("'SPACE'");
            }
            while self.kind() == T::Space {
                self.i += 1;
            }
            let next = self.styled_vertex()?;
            if let Some(data) = data {
                let last = ids.last().expect("a node list is never empty").clone();
                self.attach(&last, data)?;
            }
            ids.push(next);
        }
    }

    fn link(&mut self) -> Parsed<LinkInfo> {
        let id = (self.kind() == T::LinkId).then(|| self.take());
        if self.kind() == T::Link {
            let end = self.take();
            let mut info = LinkInfo { link: destruct_link(&end, None), text: None, id: id.map(JsText::from) };
            if self.kind() == T::Pipe {
                self.i += 1;
                info.text = Some(self.text()?);
                self.closed(T::Pipe)?;
                if self.kind() == T::Space {
                    self.i += 1;
                }
            }
            return Ok(info);
        }
        if self.kind() == T::StartLink {
            let start = self.take();
            let label = self.edge_text()?;
            let end = self.expect(T::Link, "'EDGE_TEXT'")?;
            let link: Link = destruct_link(&end, Some(&start));
            return Ok(LinkInfo { link, text: Some(label), id: id.map(JsText::from) });
        }
        self.fail("'LINK', 'START_LINK'")
    }

    fn flush_pending(&mut self, ids: &[JsText]) -> Parsed<()> {
        match self.pending_data.take() {
            Some(data) => self.attach(ids.last().expect("a node list is never empty"), data),
            None => Ok(()),
        }
    }

    fn vertex_statement(&mut self) -> Parsed<Vec<JsText>> {
        let from = self.i;
        let mut stmt = self.node()?;
        self.flush_pending(&stmt)?;
        let mut nodes = stmt.clone();
        loop {
            if !matches!(self.kind(), T::Link | T::StartLink | T::LinkId) {
                return Ok(nodes);
            }
            let info = self.link()?;
            let next = self.node()?;
            self.flush_pending(&next)?;
            self.db.add_link(&stmt, &next, &info)?;
            self.db.add_edge_mapping(&next, self.span(from, self.i - 1));
            nodes.splice(0..0, next.iter().cloned());
            stmt = next;
        }
    }

    fn style(&mut self) -> Parsed<JsText> {
        self.run_of(is_style_part, "'NODE_STRING', 'NUM', 'COLON', 'BRKT'").map(JsText::from)
    }

    fn styles(&mut self) -> Parsed<Vec<JsText>> {
        let mut out = vec![self.style()?];
        while self.kind() == T::Comma {
            self.i += 1;
            out.push(self.style()?);
        }
        Ok(out)
    }

    fn link_style_statement(&mut self) -> Parsed<()> {
        self.i += 1;
        self.expect(T::Space, "")?;
        let positions = if self.kind() == T::Default {
            self.i += 1;
            LinkPositions::Default
        } else {
            let mut indexes = vec![JsText::from(self.expect(T::Num, "'DEFAULT'")?)];
            while self.kind() == T::Comma {
                self.i += 1;
                indexes.push(self.expect(T::Num, "")?.into());
            }
            LinkPositions::Indexes(indexes)
        };
        self.expect(T::Space, "")?;
        if self.kind() != T::Interpolate {
            let styles = self.styles()?;
            return self.db.update_link(&positions, styles);
        }
        self.i += 1;
        self.expect(T::Space, "")?;
        let curve = self.alpha_num()?;
        if self.kind() == T::Space {
            self.i += 1;
            let styles = self.styles()?;
            self.db.update_link_interpolate(&positions, &curve)?;
            self.db.update_link(&positions, styles)
        } else {
            self.db.update_link_interpolate(&positions, &curve)
        }
    }

    fn link_target(text: &[u16]) -> LinkTarget {
        match () {
            _ if text.is_str("_self") => LinkTarget::SelfFrame,
            _ if text.is_str("_blank") => LinkTarget::Blank,
            _ if text.is_str("_parent") => LinkTarget::Parent,
            _ => LinkTarget::Top,
        }
    }

    fn link_tail(&mut self, id: &[u16], url: &[u16]) -> Parsed<()> {
        if self.kind() != T::Space {
            self.db.set_link(id, url, None);
            return Ok(());
        }
        self.i += 1;
        if self.kind() == T::LinkTarget {
            let target = Self::link_target(&self.take());
            self.db.set_link(id, url, Some(target));
            return Ok(());
        }
        let tooltip = self.expect(T::Str, "'LINK_TARGET'")?;
        if self.kind() == T::Space {
            self.i += 1;
            let target = Self::link_target(&self.expect(T::LinkTarget, "")?);
            self.db.set_link(id, url, Some(target));
        } else {
            self.db.set_link(id, url, None);
        }
        self.db.set_tooltip(id, &tooltip);
        Ok(())
    }

    fn click_statement(&mut self) -> Parsed<()> {
        let id = self.take();
        match self.kind() {
            T::CallbackName => {
                self.i += 1;
                if self.kind() == T::CallbackArgs {
                    self.i += 1;
                }
                let tooltip = if self.kind() == T::Space {
                    self.i += 1;
                    Some(self.expect(T::Str, "")?)
                } else {
                    None
                };
                self.db.set_click_event(&id);
                if let Some(tooltip) = tooltip {
                    self.db.set_tooltip(&id, &tooltip);
                }
            }
            T::Href => {
                self.i += 1;
                let url = self.expect(T::Str, "")?;
                self.link_tail(&id, &url)?;
            }
            T::Str => {
                let url = self.take();
                self.link_tail(&id, &url)?;
            }
            _ => {
                self.alpha_num()?;
                if self.kind() == T::Space {
                    self.i += 1;
                    let tooltip = self.expect(T::Str, "")?;
                    self.db.set_click_event(&id);
                    self.db.set_tooltip(&id, &tooltip);
                } else {
                    self.db.set_click_event(&id);
                }
            }
        }
        Ok(())
    }

    /// Reads `flow` or `connector` up to the separator: an id, an optional bracketed title and
    /// optional metadata.
    fn header(&mut self) -> Parsed<Header> {
        self.expect(T::Space, SEPARATORS)?;
        let id = self.text_no_tags()?;
        let mut title = FlowText { text: JsText::default(), kind: LabelType::Text };
        if self.kind() == T::Sqs {
            self.i += 1;
            title = self.text()?;
            self.closed(T::Sqe)?;
        }
        let data = (self.kind() == T::ShapeData).then(|| self.shape_data());
        if !is_separator(self.kind()) {
            return self.fail(&format!("{SEPARATORS}, 'SHAPE_DATA'"));
        }
        Ok(Header { id, title, data })
    }

    /// One statement. What it adds to an enclosing block's members comes back.
    fn statement(&mut self) -> Parsed<Option<DocItem>> {
        let t = self.kind();
        match t {
            T::Style => {
                self.i += 1;
                self.expect(T::Space, "")?;
                let id = self.id_string()?;
                self.expect(T::Space, "")?;
                let styles = self.styles()?;
                self.db.add_vertex(&id, VertexUpdate { styles: Some(styles), ..VertexUpdate::default() })?;
                self.separator()?;
                Ok(None)
            }
            T::LinkStyle => {
                self.link_style_statement()?;
                self.separator()?;
                Ok(None)
            }
            T::ClassDef => {
                self.i += 1;
                self.expect(T::Space, "")?;
                let id = self.id_string()?;
                self.expect(T::Space, "")?;
                let styles = self.styles()?;
                self.db.add_class(&id, &styles);
                self.separator()?;
                Ok(None)
            }
            T::Class => {
                self.i += 1;
                self.expect(T::Space, "")?;
                let ids = self.id_string()?;
                self.expect(T::Space, "")?;
                let class = self.id_string()?;
                self.db.set_class(&ids, &class);
                self.separator()?;
                Ok(None)
            }
            T::Click => {
                self.click_statement()?;
                self.separator()?;
                Ok(None)
            }
            T::Connector => {
                let at = self.i;
                self.i += 1;
                let header = self.header()?;
                let sep = self.i;
                self.i += 1;
                let id = self.db.add_connector(&header.id, Some(&header.title));
                self.attach_data(&id, header.data)?;
                let name = header.id.text.trim_js();
                self.db.add_block_mapping(name, StatementType::Connector, self.span(at, at), self.span(sep, sep));
                Ok(Some(DocItem::Text(id)))
            }
            _ if DIRECTION_STATEMENTS.iter().any(|&(kind, _)| kind == t) => {
                self.i += 1;
                let direction = match t {
                    T::DirectionTb => Direction::TB,
                    T::DirectionBt => Direction::BT,
                    T::DirectionRl => Direction::RL,
                    T::DirectionLr => Direction::LR,
                    _ => Direction::TD,
                };
                Ok(Some(DocItem::Dir(direction)))
            }
            T::AccTitle => {
                self.i += 1;
                let value: JsText = self.expect(T::AccTitleValue, "")?.trim_js().into();
                self.db.set_acc_title(&value);
                Ok(Some(DocItem::Text(value)))
            }
            T::AccDescr => {
                self.i += 1;
                let value: JsText = self.expect(T::AccDescrValue, "")?.trim_js().into();
                self.db.set_acc_description(&value);
                Ok(Some(DocItem::Text(value)))
            }
            T::AccDescrMultilineValue => {
                let value: JsText = self.take().trim_js().into();
                self.db.set_acc_description(&value);
                Ok(Some(DocItem::Text(value)))
            }
            _ => {
                if !is_id(t) {
                    return self.fail(STATEMENT_START);
                }
                let nodes = self.vertex_statement()?;
                self.separator()?;
                Ok((!nodes.is_empty()).then_some(DocItem::Ids(nodes)))
            }
        }
    }

    /// Blocks nest without recursion: each open one keeps the list and collect flag of its
    /// parent.
    fn document(&mut self) -> Parsed<()> {
        let mut open: Vec<Frame> = Vec::new();
        let mut list: Vec<DocItem> = Vec::new();
        let mut collect = false;
        loop {
            let t = self.kind();
            match t {
                T::End => break,
                T::EndKeyword => {
                    let Some(frame) = open.pop() else {
                        break;
                    };
                    let at = self.i;
                    self.i += 1;
                    let members = std::mem::replace(&mut list, frame.list);
                    collect = frame.collect;
                    match frame.block {
                        Block::Global => self.db.add_global(members),
                        Block::Flow { id, title, data } => {
                            let name = self.db.add_subgraph(id.as_ref(), members, title.as_ref());
                            self.attach_data(&name, data)?;
                            // The mapping takes the id as it is written, which may hold spaces.
                            let written = id.as_ref().map_or(&[][..], |id| &id.text);
                            let (start, end) = (self.span(frame.at, frame.at), self.span(at, at));
                            self.db.add_block_mapping(written, StatementType::Subgraph, start, end);
                            if collect {
                                list.push(DocItem::Text(name));
                            }
                        }
                    }
                }
                T::Semi | T::Newline | T::Space | T::Eof => {
                    if collect {
                        list.push(DocItem::Text(self.tokens[self.i].text.clone().into()));
                    }
                    self.i += 1;
                }
                T::Comment => self.i += 1,
                T::Flow | T::Global => {
                    let at = self.i;
                    self.i += 1;
                    let block = if t == T::Global || is_separator(self.kind()) {
                        self.separator()?;
                        if t == T::Global { Block::Global } else { Block::Flow { id: None, title: None, data: None } }
                    } else {
                        let header = self.header()?;
                        self.i += 1;
                        Block::Flow { id: Some(header.id), title: Some(header.title), data: header.data }
                    };
                    open.push(Frame { block, at, list: std::mem::take(&mut list), collect });
                    collect = true;
                }
                _ => {
                    if let Some(item) = self.statement()?
                        && collect
                    {
                        list.push(item);
                    }
                }
            }
        }
        if !open.is_empty() {
            return self.fail("'end'");
        }
        Ok(())
    }

    fn agentflow(&mut self) -> Parsed<()> {
        while matches!(self.kind(), T::Space | T::Newline | T::Comment) {
            self.i += 1;
        }
        self.expect(T::Graph, "'SPACE', 'NEWLINE', 'COMMENT'")?;
        if self.kind() == T::NoDir {
            self.i += 1;
            self.db.set_direction(&JsText::from("TB"));
        } else {
            let dir = self.expect(T::Dir, "'NODIR'")?;
            let mut t = self.kind();
            if t == T::Space {
                while self.kind() == T::Space {
                    self.i += 1;
                }
                t = self.kind();
                if t != T::Newline {
                    return self.fail("'NEWLINE', 'SPACE'");
                }
            } else if t != T::Semi && t != T::Newline && t != T::Comment {
                return self.fail("'COMMENT', 'SEMI', 'NEWLINE', 'SPACE'");
            }
            self.i += 1;
            self.db.set_direction(&dir);
        }
        self.document()?;
        if self.kind() != T::End {
            return self.fail(STATEMENT_START);
        }
        Ok(())
    }
}

fn labelled(label: FlowText, shape: ShapeName) -> VertexUpdate {
    VertexUpdate { text: Some(label), shape: Some(NodeShape::Named(shape)), ..VertexUpdate::default() }
}

/// Pele's `parseAgentflow`, into `db`.
pub(super) fn parse<'c>(cx: &ParseContext<'c>, db: &mut AgentflowDb<'_, 'c>) -> Result<(), RenderError> {
    let source = cx.text;
    let edits = if source.contains(&u16::from(b'}')) { brace_trails(source) } else { Vec::new() };
    let mut src = Vec::with_capacity(source.len());
    let mut at = 0;
    for &(from, to, _) in &edits {
        src.extend_from_slice(&source[at..from]);
        src.extend([u16::from(b'}'), 10]);
        at = to;
    }
    src.extend_from_slice(&source[at..]);
    let tokens = tokenize(&src);
    let mut parser = Parser { cx, db, src, tokens, i: 0, edits, pending_data: None };
    parser.agentflow()
}
