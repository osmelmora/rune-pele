//! `railroad-peg-beta`: parsing expression grammars. Ported from Pele's `railroad/peg.ts`.

use super::lexer::{
    ACC_DESCR, ACC_TITLE, TITLE, hidden_tokens, identifier_named, literal, parse_grammar, quoted, unescape,
};
use super::{Grammar, Header, Node, NodeId, Rule};
use crate::diagrams::tokens::{Matcher, Reader, SyntaxError, TokenType, hidden, keyword};
use crate::input::{JsText, Units};

const ARROW: usize = 1;
const SEMICOLON: usize = 2;
const SLASH: usize = 3;
const AND: usize = 4;
const NOT: usize = 5;
const QUESTION: usize = 6;
const STAR: usize = 7;
const PLUS: usize = 8;
const GROUP: usize = 9;
const GROUP_END: usize = 10;
const DOT: usize = 11;
const TITLE_KIND: usize = 12;
const ID: usize = 15;
const STRING: usize = 16;

/// `#[^\n\r]*`
fn line_comment(src: &[u16], at: usize) -> Option<usize> {
    (src.at(at) == Some(u16::from(b'#'))).then(|| src.run(at + 1, |c| c != 10 && c != 13))
}

const HIDDEN: [TokenType; 4] =
    hidden_tokens(["PEG_WHITESPACE", "PEG_YAML", "PEG_DIRECTIVE", "PEG_SINGLE_LINE_COMMENT"]);

pub(crate) static TOKENS: [TokenType; 22] = {
    let [whitespace, yaml, directive, comment] = HIDDEN;
    [
        keyword("railroad-peg-beta").longer(&[ID]),
        literal("<-"),
        literal(";"),
        literal("/"),
        literal("&"),
        literal("!"),
        literal("?"),
        literal("*"),
        literal("+"),
        literal("("),
        literal(")"),
        literal("."),
        TITLE,
        ACC_TITLE,
        ACC_DESCR,
        identifier_named("PEG_ID"),
        quoted("PEG_STRING"),
        whitespace,
        yaml,
        directive,
        comment,
        hidden("PEG_LINE_COMMENT", Matcher::Pattern(line_comment)),
    ]
};

/// What a predicate shows of the expression it looks ahead for.
fn label(grammar: &Grammar, node: NodeId) -> JsText {
    match grammar.node(node) {
        Node::Terminal(value) => {
            let mut text = JsText::from("\"");
            text.push_units(value);
            text.push(u16::from(b'"'));
            text
        }
        Node::NonTerminal(text) | Node::Special(text) => text.clone(),
        _ => "(...)".into(),
    }
}

/// A primary with the suffix written after it and the predicate written before it: Pele's `node`
/// for a `PegSuffix` and a `PegPrefix`.
fn prefixed(grammar: &mut Grammar, operator: Option<&[u16]>, primary: NodeId, suffix: Option<u16>) -> NodeId {
    let kid = match suffix.map(|unit| unit as u8) {
        None => primary,
        Some(b'?') => grammar.optional(primary),
        Some(b'+') => grammar.repetition(primary, 1.0, f64::INFINITY),
        Some(_) => grammar.repetition(primary, 0.0, f64::INFINITY),
    };
    match operator {
        None => kid,
        Some(operator) => {
            let mut text = JsText::from(operator);
            text.push_units(&label(grammar, kid));
            grammar.special(text)
        }
    }
}

/// One open group, or the rule itself at the bottom of the stack: the sequences of its choice so
/// far, and the predicate written before the group.
struct Frame {
    alternatives: Vec<Vec<NodeId>>,
    operator: Option<Vec<u16>>,
}

impl Frame {
    fn new(operator: Option<Vec<u16>>) -> Self {
        Frame { alternatives: vec![Vec::new()], operator }
    }

    fn build(self, grammar: &mut Grammar) -> NodeId {
        let alternatives = self.alternatives.into_iter().map(|elements| grammar.sequence(elements)).collect();
        grammar.choice(alternatives)
    }
}

/// One definition, read with a stack of open groups rather than by recursion.
fn definition(r: &mut Reader<'_>, grammar: &mut Grammar) -> Result<NodeId, SyntaxError> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut top = Frame::new(None);
    loop {
        let mut operator = match r.kind() {
            Some(AND | NOT) => Some(r.take().to_vec()),
            _ => None,
        };
        let mut primary = match r.kind() {
            Some(STRING) => {
                let value = unescape(r.take());
                grammar.terminal(value)
            }
            Some(ID) => {
                let name = r.take();
                grammar.nonterminal(name)
            }
            Some(DOT) => {
                let dot = r.take();
                grammar.special(dot)
            }
            Some(GROUP) => {
                r.skip();
                stack.push(std::mem::replace(&mut top, Frame::new(operator)));
                continue;
            }
            _ => return Err(r.fail("a literal, a name, a dot, or an opening parenthesis")),
        };
        loop {
            let suffix = match r.kind() {
                Some(QUESTION | STAR | PLUS) => r.take().first().copied(),
                _ => None,
            };
            let element = prefixed(grammar, operator.as_deref(), primary, suffix);
            top.alternatives.last_mut().expect("a choice has a sequence").push(element);
            let kind = r.kind();
            if kind == Some(GROUP_END) && !stack.is_empty() {
                r.skip();
                let closed = std::mem::replace(&mut top, stack.pop().expect("a group is open"));
                operator = closed.operator.clone();
                primary = closed.build(grammar);
                continue;
            }
            match kind {
                Some(SLASH) => {
                    r.skip();
                    top.alternatives.push(Vec::new());
                }
                Some(AND | NOT | STRING | ID | DOT | GROUP) => {}
                _ if stack.is_empty() => return Ok(top.build(grammar)),
                _ => return Err(r.fail("token of type ')'")),
            }
            break;
        }
    }
}

/// Pele's `parsePeg` and `pegRules`.
pub(super) fn parse(src: &[u16], grammar: &mut Grammar) -> Result<Header, SyntaxError> {
    parse_grammar(src, &TOKENS, TITLE_KIND, unescape, |r| {
        let name = r.expect(ID)?.into();
        r.expect(ARROW)?;
        let definition = definition(r, grammar)?;
        r.expect(SEMICOLON)?;
        grammar.rules.push(Rule { name, definition });
        Ok(())
    })
}
