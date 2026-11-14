//! The flowchart parser (Pele's `src/diagrams/flowchart/parser.ts`): a hand-written parser for
//! Mermaid's `flow.jison` grammar that reports errors in jison's words.

use super::db::{DocItem, FlowDb, LinkInfo, LinkPositions, SubgraphHeader, VertexUpdate, destruct_link};
use super::lexer::{DIRECTION_STATEMENTS, T, Token, tokenize};
use super::shapes::ShapeName;
use super::{Direction, FlowText, LabelType, LinkTarget};
use crate::RenderError;
use crate::catalog::ParseContext;
use crate::diagrams::tokens::SyntaxError;
use crate::input::{Edit, JsObject, JsText, Units, input_offset, is_space};

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

/// Tokens that may make up a subgraph's name.
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
            | T::Subgraph
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

const TEXT_TOKENS: &str = "'UNICODE_TEXT', 'TEXT', 'TAGSTART'";
const STATEMENT_START: &str = "'SEMI', 'NEWLINE', 'SPACE', 'EOF', 'subgraph', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'STYLE', 'LINKSTYLE', 'CLASSDEF', 'CLASS', 'CLICK', 'NODE_STRING'";

/// `/}\s*\n/g`, which Mermaid replaces with `}\n` before parsing: each match, as an edit.
fn brace_trails(source: &[u16]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut at = 0;
    while at < source.len() {
        if source[at] == u16::from(b'}') {
            let end = source.run(at + 1, is_space);
            if let Some(last) = (at + 1..end).rev().find(|&q| source[q] == 10) {
                edits.push((at, last + 1, 2));
                at = last + 1;
                continue;
            }
        }
        at += 1;
    }
    edits
}

/// One open subgraph: its header, with the list and collect flag of the document around it.
struct Frame {
    header: SubgraphHeader,
    list: Vec<DocItem>,
    collect: bool,
}

struct Parser<'a, 'c> {
    cx: &'a ParseContext<'c>,
    db: &'a mut FlowDb,
    tokens: Vec<Token>,
    i: usize,
    edits: Vec<Edit>,
    pending_data: Option<Vec<u16>>,
}

type Parsed<T> = Result<T, RenderError>;

impl Parser<'_, '_> {
    fn kind(&self) -> T {
        self.tokens.get(self.i).map_or(T::End, |token| token.kind)
    }

    fn take(&mut self) -> Vec<u16> {
        self.i += 1;
        self.tokens[self.i - 1].text.clone()
    }

    fn fail<X>(&self, expected: &str) -> Parsed<X> {
        let token = &self.tokens[self.i.min(self.tokens.len() - 1)];
        let at = input_offset(&vec![self.edits.clone()], token.start);
        let lexical = token.kind == T::Error;
        let detail = if lexical { String::new() } else { format!("Expecting {expected}, got '{}'", token.kind.name()) };
        Err(self.cx.syntax(SyntaxError { at, detail, lexical }))
    }

    fn expect(&mut self, kind: T, also: &str) -> Parsed<Vec<u16>> {
        if self.kind() != kind {
            let also = if also.is_empty() { String::new() } else { format!(", {also}") };
            return self.fail(&format!("'{}'{also}", kind.name()));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Parsed<()> {
        if !matches!(self.kind(), T::Newline | T::Semi | T::Eof) {
            return self.fail("'SEMI', 'NEWLINE', 'EOF'");
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

    fn shape_data(&mut self) -> Vec<u16> {
        let mut s = self.take();
        while self.kind() == T::ShapeData {
            s.extend(self.take());
        }
        s
    }

    fn add(&mut self, id: &[u16], label: FlowText, shape: ShapeName) -> Parsed<()> {
        self.db.add_vertex(id, VertexUpdate { text: Some(label), shape: Some(shape), ..VertexUpdate::default() })
    }

    /// A label closed by `close`, for the vertex `id` with `shape`.
    fn shaped(&mut self, id: &[u16], close: T, shape: ShapeName) -> Parsed<()> {
        self.i += 1;
        let label = self.text()?;
        self.closed(close)?;
        self.add(id, label, shape)
    }

    fn vertex(&mut self) -> Parsed<JsText> {
        let id = self.id_string()?;
        match self.kind() {
            T::Sqs => self.shaped(&id, T::Sqe, ShapeName::SQUARE)?,
            T::DoubleCircleStart => self.shaped(&id, T::DoubleCircleEnd, ShapeName::DOUBLECIRCLE)?,
            T::Ps => {
                self.i += 1;
                if self.kind() == T::Ps {
                    self.i += 1;
                    let label = self.text()?;
                    self.closed(T::Pe)?;
                    self.expect(T::Pe, "")?;
                    self.add(&id, label, ShapeName::CIRCLE)?;
                } else {
                    let label = self.text()?;
                    self.closed(T::Pe)?;
                    self.add(&id, label, ShapeName::ROUND)?;
                }
            }
            T::EllipseStart => self.shaped(&id, T::EllipseEnd, ShapeName::ELLIPSE)?,
            T::StadiumStart => self.shaped(&id, T::StadiumEnd, ShapeName::STADIUM)?,
            T::SubroutineStart => self.shaped(&id, T::SubroutineEnd, ShapeName::SUBROUTINE)?,
            T::VertexWithPropsStart => {
                self.i += 1;
                let field = self.expect(T::NodeString, "")?;
                self.expect(T::Colon, "")?;
                let value = self.expect(T::NodeString, "")?;
                self.expect(T::Pipe, "")?;
                let label = self.text()?;
                self.closed(T::Sqe)?;
                let mut props = JsObject::default();
                props.set(field.into(), crate::input::JsValue::String(value.into()));
                let update = VertexUpdate {
                    text: Some(label),
                    shape: Some(ShapeName::RECT),
                    props: Some(props),
                    ..VertexUpdate::default()
                };
                self.db.add_vertex(&id, update)?;
            }
            T::CylinderStart => self.shaped(&id, T::CylinderEnd, ShapeName::CYLINDER)?,
            T::DiamondStart => {
                self.i += 1;
                if self.kind() == T::DiamondStart {
                    self.i += 1;
                    let label = self.text()?;
                    self.closed(T::DiamondStop)?;
                    self.expect(T::DiamondStop, "")?;
                    self.add(&id, label, ShapeName::HEXAGON)?;
                } else {
                    let label = self.text()?;
                    self.closed(T::DiamondStop)?;
                    self.add(&id, label, ShapeName::DIAMOND)?;
                }
            }
            T::TagEnd => self.shaped(&id, T::Sqe, ShapeName::ODD)?,
            T::TrapStart => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::InvTrapEnd {
                    self.i += 1;
                    self.add(&id, label, ShapeName::LEAN_RIGHT)?;
                } else {
                    self.closed(T::TrapEnd)?;
                    self.add(&id, label, ShapeName::TRAPEZOID)?;
                }
            }
            T::InvTrapStart => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::TrapEnd {
                    self.i += 1;
                    self.add(&id, label, ShapeName::LEAN_LEFT)?;
                } else {
                    self.closed(T::InvTrapEnd)?;
                    self.add(&id, label, ShapeName::INV_TRAPEZOID)?;
                }
            }
            _ => self.db.add_vertex(&id, VertexUpdate::default())?,
        }
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

    fn metadata(&mut self, id: &[u16], data: Vec<u16>) -> Parsed<()> {
        self.db.add_vertex(id, VertexUpdate { metadata: Some(data), ..VertexUpdate::default() })
    }

    /// A node, or several joined by `&`. Shape data after the last one is left in
    /// `pending_data` for the caller.
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
                self.metadata(&last, data)?;
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
            return Ok(LinkInfo {
                link: destruct_link(&end, Some(&start)),
                text: Some(label),
                id: id.map(JsText::from),
            });
        }
        self.fail("'LINK', 'START_LINK'")
    }

    fn flush_pending(&mut self, ids: &[JsText]) -> Parsed<()> {
        match self.pending_data.take() {
            Some(data) => self.metadata(ids.last().expect("a node list is never empty"), data),
            None => Ok(()),
        }
    }

    fn vertex_statement(&mut self, collect: bool) -> Parsed<Vec<JsText>> {
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
            if collect {
                nodes.splice(0..0, next.iter().cloned());
            }
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

    fn subgraph_header(&mut self) -> Parsed<SubgraphHeader> {
        self.i += 1;
        if matches!(self.kind(), T::Newline | T::Semi | T::Eof) {
            self.i += 1;
            return Ok(SubgraphHeader::Unnamed);
        }
        self.expect(T::Space, "'SEMI', 'NEWLINE', 'EOF'")?;
        let id = self.text_no_tags()?;
        let mut title = None;
        if self.kind() == T::Sqs {
            self.i += 1;
            title = Some(self.text()?);
            self.closed(T::Sqe)?;
        }
        if !matches!(self.kind(), T::Newline | T::Semi | T::Eof) {
            return self.fail("'SEMI', 'NEWLINE', 'EOF', 'SQS'");
        }
        self.i += 1;
        Ok(SubgraphHeader::Named { id, title })
    }

    /// One statement. What it adds to an enclosing subgraph's members comes back.
    fn statement(&mut self, collect: bool) -> Parsed<Option<DocItem>> {
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
                let nodes = self.vertex_statement(collect)?;
                self.separator()?;
                Ok((!nodes.is_empty()).then_some(DocItem::Ids(nodes)))
            }
        }
    }

    /// Subgraphs nest without recursion: each open one keeps the list and collect flag of its
    /// parent.
    fn document(&mut self) -> Parsed<()> {
        let mut open: Vec<Frame> = Vec::new();
        let mut list: Vec<DocItem> = Vec::new();
        let mut collect = false;
        loop {
            match self.kind() {
                T::End => break,
                T::EndKeyword => {
                    let Some(frame) = open.pop() else {
                        break;
                    };
                    self.i += 1;
                    let members = std::mem::replace(&mut list, frame.list);
                    let id = self.db.add_subgraph(frame.header, members);
                    collect = frame.collect;
                    if collect {
                        list.push(DocItem::Text(id));
                    }
                }
                T::Semi | T::Newline | T::Space | T::Eof => {
                    if collect {
                        list.push(DocItem::Text(self.tokens[self.i].text.clone().into()));
                    }
                    self.i += 1;
                }
                T::Subgraph => {
                    let header = self.subgraph_header()?;
                    open.push(Frame { header, list: std::mem::take(&mut list), collect });
                    collect = true;
                }
                _ => {
                    if let Some(item) = self.statement(collect)?
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

    fn flowchart(&mut self) -> Parsed<()> {
        while matches!(self.kind(), T::Space | T::Newline) {
            self.i += 1;
        }
        self.expect(T::Graph, "'SPACE', 'NEWLINE'")?;
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
            } else if t != T::Semi && t != T::Newline {
                return self.fail("'SEMI', 'NEWLINE', 'SPACE'");
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

/// Pele's `parseFlowchart`, into `db`.
pub(super) fn parse(cx: &ParseContext<'_>, db: &mut FlowDb) -> Result<(), RenderError> {
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
    let mut parser = Parser { cx, db, tokens: tokenize(&src), i: 0, edits, pending_data: None };
    parser.flowchart()
}
