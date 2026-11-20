//! Railroad diagrams, ported from Pele's `src/diagrams/railroad`. Four notations (Mermaid's own
//! constructor calls, EBNF, ABNF and PEG) each have a front end of their own, and all four lower
//! to one rule model, which is laid out and drawn.

mod abnf;
mod ebnf;
mod ir;
mod lexer;
mod peg;
/// Not part of the API: public for the workspace's tests, through the crate root.
#[doc(hidden)]
pub mod probe;
mod render;

use super::tokens::SyntaxError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::{JsText, Units, is_space};
use crate::scene::{Accessible, Scene};
use crate::{Diagnostic, ErrorCode, RenderError, fit};

/// Which of Mermaid's four grammars the text was written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Notation {
    Railroad,
    Ebnf,
    Abnf,
    Peg,
}

impl Notation {
    /// Pele's `notationOf`: `^\s*railroad-(ebnf-|abnf-|peg-)?beta`, in any case.
    fn of(text: &[u16]) -> Option<Notation> {
        let caseless = |at: usize, word: &str| {
            word.bytes().enumerate().all(|(offset, byte)| {
                text.at(at + offset)
                    .and_then(|unit| u8::try_from(unit).ok())
                    .is_some_and(|unit| unit.eq_ignore_ascii_case(&byte))
            })
        };
        let start = text.run(0, is_space);
        if !caseless(start, "railroad-") {
            return None;
        }
        let infix = start + 9;
        let (notation, length) = [("ebnf-", Notation::Ebnf), ("abnf-", Notation::Abnf), ("peg-", Notation::Peg)]
            .into_iter()
            .find(|&(word, _)| caseless(infix, word))
            .map_or((Notation::Railroad, 0), |(word, notation)| (notation, word.len()));
        caseless(infix + length, "beta").then_some(notation)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Notation::Railroad => "railroad",
            Notation::Ebnf => "ebnf",
            Notation::Abnf => "abnf",
            Notation::Peg => "peg",
        }
    }
}

/// A node of the rule model, by its place in [`Grammar::nodes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NodeId(usize);

/// Pele's `RailroadNode`. A node's parts are made before it, so the model holds no cycle.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    Terminal(JsText),
    NonTerminal(JsText),
    Special(JsText),
    Sequence(Vec<NodeId>),
    Choice(Vec<NodeId>),
    Optional(NodeId),
    /// `max` is infinite when the count has no upper bound.
    Repetition {
        element: NodeId,
        min: f64,
        max: f64,
    },
}

pub(crate) struct Rule {
    pub(crate) name: JsText,
    pub(crate) definition: NodeId,
}

/// Every rule in source order, with the nodes their definitions are made of. A name defined
/// twice is drawn twice. Nesting depth is bounded by memory, as in Pele, so nothing that reads
/// the model recurses.
#[derive(Default)]
pub(crate) struct Grammar {
    nodes: Vec<Node>,
    pub(crate) rules: Vec<Rule>,
}

impl Grammar {
    fn add(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        NodeId(self.nodes.len() - 1)
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    fn terminal(&mut self, value: impl Into<JsText>) -> NodeId {
        self.add(Node::Terminal(value.into()))
    }

    fn nonterminal(&mut self, name: impl Into<JsText>) -> NodeId {
        self.add(Node::NonTerminal(name.into()))
    }

    fn special(&mut self, text: impl Into<JsText>) -> NodeId {
        self.add(Node::Special(text.into()))
    }

    fn optional(&mut self, element: NodeId) -> NodeId {
        self.add(Node::Optional(element))
    }

    fn repetition(&mut self, element: NodeId, min: f64, max: f64) -> NodeId {
        self.add(Node::Repetition { element, min, max })
    }

    /// A sequence of one is that one, in every notation.
    fn sequence(&mut self, elements: Vec<NodeId>) -> NodeId {
        match elements.as_slice() {
            [only] => *only,
            _ => self.add(Node::Sequence(elements)),
        }
    }

    /// A choice of one is that one, in every notation.
    fn choice(&mut self, alternatives: Vec<NodeId>) -> NodeId {
        match alternatives.as_slice() {
            [only] => *only,
            _ => self.add(Node::Choice(alternatives)),
        }
    }

    /// Pele's `parts`: the nodes a node is made of, in order.
    pub(crate) fn parts(&self, id: NodeId) -> &[NodeId] {
        match self.node(id) {
            Node::Sequence(parts) | Node::Choice(parts) => parts,
            Node::Optional(element) | Node::Repetition { element, .. } => std::slice::from_ref(element),
            Node::Terminal(_) | Node::NonTerminal(_) | Node::Special(_) => &[],
        }
    }
}

/// The title and accessibility statements before the first rule.
#[derive(Default)]
pub(super) struct Header {
    title: Option<Vec<u16>>,
    acc_title: Option<Vec<u16>>,
    acc_descr: Option<Vec<u16>>,
}

pub(crate) struct RailroadModel {
    notation: Notation,
    title: Option<JsText>,
    accessible: Accessible,
    grammar: Grammar,
}

/// `text.replace(/\n\s+/g, '\n')`
pub(super) fn close_indents(text: &[u16]) -> JsText {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        let spaces = text.run(at + 1, is_space);
        at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
    }
    out.into()
}

/// Pele's `parseRailroad`: the notation the keyword names, its front end, and the model Mermaid's
/// parsers load from it.
fn parse_railroad(text: &[u16], front_title: Option<&JsText>) -> Result<RailroadModel, ParseFailure> {
    let notation = Notation::of(text).ok_or(ParseFailure::NoKeyword)?;
    let mut grammar = Grammar::default();
    let header = match notation {
        Notation::Railroad => ir::parse(text, &mut grammar),
        Notation::Ebnf => ebnf::parse(text, &mut grammar),
        Notation::Abnf => abnf::parse(text, &mut grammar),
        Notation::Peg => peg::parse(text, &mut grammar),
    }
    .map_err(ParseFailure::Syntax)?;
    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    Ok(RailroadModel {
        notation,
        title: filled(header.title)
            .map(JsText::from)
            .or_else(|| front_title.filter(|title| !title.is_empty()).cloned()),
        accessible: Accessible {
            title: filled(header.acc_title).map(|text| text.trim_start_js().into()),
            description: filled(header.acc_descr).map(|text| close_indents(&text)),
        },
        grammar,
    })
}

enum ParseFailure {
    NoKeyword,
    Syntax(SyntaxError),
}

pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<RailroadModel, RenderError> {
    parse_railroad(cx.text, cx.title).map_err(|failure| match failure {
        ParseFailure::NoKeyword => {
            RenderError::Diagnostic(Diagnostic::new(ErrorCode::Syntax, "No railroad diagram keyword found.".to_owned()))
        }
        ParseFailure::Syntax(error) => cx.syntax(error),
    })
}

pub(crate) struct Railroad;

impl Diagram for Railroad {
    type Model = RailroadModel;

    fn parse(cx: &ParseContext<'_>) -> Result<RailroadModel, RenderError> {
        parse(cx)
    }

    /// Pele's `renderRailroad`: with room to spare, then tightly when that does not fit.
    fn scene(model: &RailroadModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        fit::tighten(cx.options, |tight| render::draw(model, cx, tight))
    }
}
