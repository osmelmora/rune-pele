//! The block diagram parser, ported from Pele's `src/diagrams/block/parser.ts`. It reads the
//! tokens into statement lists, nesting composites without recursion.

use super::db::BlockDb;
use super::lexer::{T, Tokens, tokenize};
use super::{BlockEdge, Head, Kind};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units};

/// A node, a space or a composite, as a statement gives it.
pub(super) struct Parsed {
    /// Unset for a composite whose header was a link statement.
    pub(super) id: Option<JsText>,
    pub(super) kind: Kind,
    pub(super) label: Option<JsText>,
    pub(super) width_in_columns: Option<f64>,
    pub(super) width: Option<f64>,
    pub(super) directions: Option<Vec<JsText>>,
    /// A composite's statements, with each link statement spread out.
    pub(super) children: Vec<Item>,
}

/// One element of a statement list once its link statements are spread out: Pele's `Block`
/// shape in each of the roles it plays.
pub(super) enum Item {
    Block(Parsed),
    Edge(BlockEdge),
    Columns(f64),
    ClassDef { id: JsText, css: JsText },
    ApplyClass { ids: JsText, class: JsText },
    ApplyStyles { id: JsText, styles: JsText },
}

struct NodeInfo {
    id: JsText,
    label: Option<JsText>,
    brackets: Option<JsText>,
    directions: Option<Vec<JsText>>,
}

/// An open composite: the list it sits in, and the statement that heads it.
struct Frame {
    list: Vec<Item>,
    header: Option<Vec<Item>>,
}

const STATEMENT_START: &str =
    "'NODE_ID', 'COLUMNS', 'SPACE_BLOCK', 'BLOCK_DIAGRAM_KEY', 'id-block', 'classDef', 'class', 'style'";

/// `parseInt` of a token's digits, or of `-1`.
fn int(text: &[u16]) -> f64 {
    String::from_utf16_lossy(text).parse().expect("the lexer gives digits")
}

fn concat(a: &[u16], b: &[u16]) -> JsText {
    let mut out = JsText::from(a);
    out.push_units(b);
    out
}

/// Pele's `edgeStrToEdgeData`.
fn end_head(link: &[u16]) -> Head {
    match link.trim_js().last() {
        Some(120) => Head::Cross,
        Some(111) => Head::Circle,
        Some(62) => Head::Point,
        _ => Head::Unnamed,
    }
}

/// Pele's `edgeStrToEdgeStartData`.
fn start_head(link: &[u16]) -> Head {
    match link.trim_js().first() {
        Some(120) => Head::Cross,
        Some(111) => Head::Circle,
        Some(60) => Head::Point,
        _ => Head::Open,
    }
}

struct Parser {
    tokens: Tokens,
    i: usize,
}

impl Parser {
    fn kind(&self) -> T {
        self.tokens.types[self.i]
    }

    fn fail(&self, expected: &str) -> SyntaxError {
        let kind = self.kind();
        let at = self.tokens.starts[self.i];
        if kind == T::Error {
            return SyntaxError { at, detail: String::new(), lexical: true };
        }
        SyntaxError { at, detail: format!("Expecting {expected}, got '{}'", kind.name()), lexical: false }
    }

    fn expect(&mut self, kind: T) -> Result<JsText, SyntaxError> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        self.i += 1;
        Ok(self.tokens.texts[self.i - 1].clone())
    }

    fn take(&mut self) -> JsText {
        self.i += 1;
        self.tokens.texts[self.i - 1].clone()
    }

    fn node(&mut self) -> Result<NodeInfo, SyntaxError> {
        let id = self.expect(T::NodeId)?;
        if self.kind() == T::NodeDStart {
            let open = self.take();
            let label = self.expect(T::Str)?;
            let close = self.expect(T::NodeDEnd)?;
            return Ok(NodeInfo { id, label: Some(label), brackets: Some(concat(&open, &close)), directions: None });
        }
        if self.kind() == T::BlockArrowStart {
            let open = self.take();
            let label = self.expect(T::Str)?;
            let mut directions = vec![self.expect(T::Dir)?];
            while self.kind() == T::Dir {
                directions.push(self.take());
            }
            let close = self.expect(T::BlockArrowEnd)?;
            let brackets = Some(concat(&open, &close));
            return Ok(NodeInfo { id, label: Some(label), brackets, directions: Some(directions) });
        }
        Ok(NodeInfo { id, label: None, brackets: None, directions: None })
    }

    /// A node, or a link statement as the blocks and edges it holds, in order.
    fn node_statement(&mut self) -> Result<Vec<Item>, SyntaxError> {
        let first = self.node()?;
        let size = if self.kind() == T::Size { int(&self.take()) } else { 1.0 };
        let block = |node: NodeInfo, width_in_columns: Option<f64>| Parsed {
            id: Some(node.id),
            kind: Kind::of_brackets(node.brackets.as_deref()),
            label: node.label,
            width_in_columns,
            width: None,
            directions: node.directions,
            children: Vec::new(),
        };
        let mut from = block(first, Some(size));
        let mut chain: Option<Vec<Item>> = None;
        while matches!(self.kind(), T::Link | T::StartLink) {
            let mut label = JsText::default();
            let opener = self.kind();
            self.i += 1;
            if opener == T::StartLink {
                self.expect(T::LinkLabel)?;
                label = self.expect(T::Str)?;
                self.expect(T::Link)?;
            }
            let link = self.tokens.texts[self.i - 1].clone();
            let to = self.node()?;
            // Mermaid keeps only the last link of `A --> B --> C` and loses its start. Here every
            // link counts, as in Pele.
            let chain = chain.get_or_insert_with(|| {
                let first = Parsed { width_in_columns: None, ..std::mem::replace(&mut from, block_placeholder()) };
                vec![Item::Block(first)]
            });
            let last = match chain.last() {
                Some(Item::Block(last)) => last.id.clone().expect("a chain's blocks have ids"),
                _ => unreachable!("a chain ends in a block"),
            };
            let mut id = last.clone();
            id.push(45);
            id.push_units(&to.id);
            let edge = BlockEdge {
                id,
                label,
                directions: to.directions.clone(),
                start: last,
                end: to.id.clone(),
                arrow_type_start: start_head(&link),
                arrow_type_end: end_head(&link),
                thick: link.has("=="),
                dotted: link.has(".-"),
            };
            chain.push(Item::Edge(edge));
            chain.push(Item::Block(block(to, None)));
        }
        Ok(chain.unwrap_or_else(|| vec![Item::Block(from)]))
    }
}

/// What `from` holds once a chain has taken it. Nothing reads it.
fn block_placeholder() -> Parsed {
    Parsed {
        id: None,
        kind: Kind::Na,
        label: None,
        width_in_columns: None,
        width: None,
        directions: None,
        children: Vec::new(),
    }
}

/// Pele's `parseBlock`: the statements of a source, each link statement spread out.
pub(super) fn parse(src: &[u16], db: &mut BlockDb) -> Result<Vec<Item>, SyntaxError> {
    let mut parser = Parser { tokens: tokenize(src), i: 0 };
    parser.expect(T::Key)?;
    let mut open: Vec<Frame> = Vec::new();
    // A link statement spreads to several items, and every statement to at least one, so the
    // list is empty exactly when Pele's is.
    let mut list: Vec<Item> = Vec::new();
    loop {
        match parser.kind() {
            T::NodeId => {
                list.extend(parser.node_statement()?);
            }
            T::Columns => {
                let columns = int(&parser.take());
                list.push(Item::Columns(columns));
            }
            T::SpaceBlock => {
                let width = int(&parser.take());
                list.push(Item::Block(Parsed {
                    id: Some(db.generate_id()),
                    kind: Kind::Space,
                    label: Some(JsText::default()),
                    width_in_columns: None,
                    width: Some(width),
                    directions: None,
                    children: Vec::new(),
                }));
            }
            T::Key => {
                parser.i += 1;
                open.push(Frame { list: std::mem::take(&mut list), header: None });
            }
            T::IdBlock => {
                parser.i += 1;
                if parser.kind() != T::NodeId {
                    return Err(parser.fail("'NODE_ID'"));
                }
                let header = parser.node_statement()?;
                open.push(Frame { list: std::mem::take(&mut list), header: Some(header) });
            }
            T::EndKeyword => {
                let frame = open.pop();
                let Some(frame) = frame.filter(|_| !list.is_empty()) else {
                    return Err(parser.fail(STATEMENT_START));
                };
                parser.i += 1;
                let id = db.generate_id();
                let children = std::mem::replace(&mut list, frame.list);
                // A header that is a link statement leaves the composite with none of its fields.
                let composite = match frame.header {
                    None => Parsed {
                        id: Some(id),
                        kind: Kind::Composite,
                        label: Some(JsText::default()),
                        width_in_columns: None,
                        width: None,
                        directions: None,
                        children,
                    },
                    Some(mut header) if header.len() == 1 => {
                        let Some(Item::Block(header)) = header.pop() else {
                            unreachable!("a statement of one item is a block");
                        };
                        Parsed { kind: Kind::Composite, children, ..header }
                    }
                    Some(_) => Parsed { kind: Kind::Composite, children, ..block_placeholder() },
                };
                list.push(Item::Block(composite));
            }
            T::ClassDef => {
                parser.i += 1;
                let id = parser.expect(T::ClassDefId)?.trim_js().into();
                let css = parser.expect(T::ClassDefStyleOpts)?.trim_js().into();
                list.push(Item::ClassDef { id, css });
            }
            T::Class => {
                parser.i += 1;
                let ids = parser.expect(T::ClassEntityIds)?.trim_js().into();
                let class = parser.expect(T::StyleClass)?.trim_js().into();
                list.push(Item::ApplyClass { ids, class });
            }
            T::Style => {
                parser.i += 1;
                let id = parser.expect(T::StyleEntityIds)?.trim_js().into();
                let styles = parser.expect(T::StyleDefinitionData)?.trim_js().into();
                list.push(Item::ApplyStyles { id, styles });
            }
            T::Eof => {
                if !open.is_empty() {
                    return Err(parser.fail(&format!("{STATEMENT_START}, 'end'")));
                }
                if list.is_empty() {
                    return Err(parser.fail(STATEMENT_START));
                }
                return Ok(list);
            }
            _ => {
                let expected = if list.is_empty() {
                    STATEMENT_START.to_owned()
                } else if open.is_empty() {
                    format!("{STATEMENT_START}, 'EOF'")
                } else {
                    format!("{STATEMENT_START}, 'end'")
                };
                return Err(parser.fail(&expected));
            }
        }
    }
}
