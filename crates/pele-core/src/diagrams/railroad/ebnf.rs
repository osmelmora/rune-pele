//! `railroad-ebnf-beta`: EBNF in the W3C and ISO 14977 styles. Ported from Pele's
//! `railroad/ebnf.ts`.

use super::lexer::{
    ACC_DESCR, ACC_TITLE, TITLE, block_comment_named, hidden_tokens, identifier_named, literal, parse_grammar, quoted,
    unescape,
};
use super::{Grammar, Header, NodeId, Rule};
use crate::diagrams::tokens::{Matcher, Reader, SyntaxError, TokenType, hidden, keyword, token};
use crate::input::{Units, is_space};

const DEFINES: usize = 1;
const EQUALS: usize = 2;
const SEMICOLON: usize = 3;
const BAR: usize = 4;
const COMMA: usize = 5;
const GROUP: usize = 6;
const OPTION: usize = 8;
const REPEAT: usize = 10;
const QUESTION: usize = 12;
const STAR: usize = 13;
const PLUS: usize = 14;
const MINUS: usize = 15;
const TITLE_KIND: usize = 16;
const ID: usize = 19;
const STRING: usize = 20;
const SPECIAL: usize = 21;
const YAML_KIND: usize = 23;
const ISO_COMMENT_KIND: usize = 27;

/// A question mark, text with something other than blanks in it and no `?` or `;`, and a
/// question mark: `\?(?=[^?;]*[^?\s;][^?;]*\?)[^?;]*\?`, read once.
fn special_sequence(src: &[u16], at: usize) -> Option<usize> {
    if src.at(at) != Some(u16::from(b'?')) {
        return None;
    }
    let mut solid = false;
    for (i, &c) in src.iter().enumerate().skip(at + 1) {
        if c == u16::from(b'?') {
            return solid.then_some(i + 1);
        }
        if c == u16::from(b';') {
            return None;
        }
        solid |= !is_space(c);
    }
    None
}

/// `\(\*[\s\S]*?\*\)`
fn iso_comment(src: &[u16], at: usize) -> Option<usize> {
    iso_opener(src, at)?;
    src.find_str("*)", at + 2).map(|close| close + 2)
}

/// `\(\*`
fn iso_opener(src: &[u16], at: usize) -> Option<usize> {
    src.starts_at(at, "(*").then_some(at + 2)
}

const HIDDEN: [TokenType; 4] =
    hidden_tokens(["EBNF_WHITESPACE", "EBNF_YAML", "EBNF_DIRECTIVE", "EBNF_SINGLE_LINE_COMMENT"]);

pub(crate) static TOKENS: [TokenType; 28] = {
    let [whitespace, yaml, directive, comment] = HIDDEN;
    [
        keyword("railroad-ebnf-beta").longer(&[ID]),
        literal("::="),
        literal("="),
        literal(";"),
        literal("|"),
        literal(","),
        literal("(").longer(&[ISO_COMMENT_KIND]),
        literal(")"),
        literal("["),
        literal("]"),
        literal("{"),
        literal("}"),
        literal("?").longer(&[SPECIAL]),
        literal("*"),
        literal("+"),
        literal("-").longer(&[YAML_KIND]),
        TITLE,
        ACC_TITLE,
        ACC_DESCR,
        identifier_named("EBNF_ID"),
        quoted("EBNF_STRING"),
        token("EBNF_SPECIAL_SEQUENCE", Matcher::Pattern(special_sequence)),
        whitespace,
        yaml,
        directive,
        comment,
        block_comment_named("EBNF_BLOCK_COMMENT"),
        hidden("EBNF_ISO_COMMENT", Matcher::Closed { pattern: iso_comment, opener: iso_opener }),
    ]
};

/// An operator written after a term's base. Each applies to everything before it in the term.
enum Postfix {
    Optional,
    ZeroOrMore,
    OneOrMore,
    /// `- except`, with the model of what is excepted.
    Except(NodeId),
}

struct Term {
    base: NodeId,
    postfixes: Vec<Postfix>,
}

/// What a bracket makes of the choice inside it.
#[derive(Clone, Copy)]
enum Bracket {
    Group,
    Optional,
    Repetition,
}

/// One open bracket, or the rule itself at the bottom of the stack: the sequences of its choice
/// so far, the bracket, its closing token, and whether it is the right side of an exception.
struct Frame {
    alternatives: Vec<Vec<Term>>,
    bracket: Option<Bracket>,
    close: Option<usize>,
    except: bool,
}

impl Frame {
    fn new(bracket: Option<Bracket>, close: Option<usize>, except: bool) -> Self {
        Frame { alternatives: vec![Vec::new()], bracket, close, except }
    }

    fn term(&mut self) -> &mut Term {
        self.alternatives.last_mut().and_then(|sequence| sequence.last_mut()).expect("a term has begun")
    }

    /// The model of the choice the frame holds: Pele's `node` for each term, sequence and choice.
    fn build(self, grammar: &mut Grammar) -> NodeId {
        let alternatives = self
            .alternatives
            .into_iter()
            .map(|sequence| {
                let elements = sequence
                    .into_iter()
                    .map(|term| {
                        term.postfixes.into_iter().fold(term.base, |result, postfix| match postfix {
                            Postfix::Optional => grammar.optional(result),
                            Postfix::ZeroOrMore => grammar.repetition(result, 0.0, f64::INFINITY),
                            Postfix::OneOrMore => grammar.repetition(result, 1.0, f64::INFINITY),
                            Postfix::Except(except) => {
                                let minus = grammar.terminal("-");
                                grammar.add(super::Node::Sequence(vec![result, minus, except]))
                            }
                        })
                    })
                    .collect();
                grammar.sequence(elements)
            })
            .collect();
        grammar.choice(alternatives)
    }
}

/// One definition, read with a stack of open brackets rather than by recursion.
fn definition(r: &mut Reader<'_>, grammar: &mut Grammar) -> Result<NodeId, SyntaxError> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut top = Frame::new(None, None, false);
    let mut except = false;
    loop {
        let mut primary = match r.kind() {
            Some(STRING) => {
                let value = unescape(r.take());
                grammar.terminal(value)
            }
            Some(ID) => {
                let name = r.take();
                grammar.nonterminal(name)
            }
            Some(SPECIAL) => {
                let text = r.take();
                grammar.special(text[1..text.len() - 1].trim_js())
            }
            Some(kind @ (GROUP | OPTION | REPEAT)) => {
                r.skip();
                let bracket = match kind {
                    GROUP => Bracket::Group,
                    OPTION => Bracket::Optional,
                    _ => Bracket::Repetition,
                };
                stack.push(std::mem::replace(&mut top, Frame::new(Some(bracket), Some(kind + 1), except)));
                except = false;
                continue;
            }
            _ => return Err(r.fail("a terminal, a name, or an opening bracket")),
        };
        loop {
            if except {
                top.term().postfixes.push(Postfix::Except(primary));
                except = false;
            } else {
                let sequence = top.alternatives.last_mut().expect("a choice has a sequence");
                sequence.push(Term { base: primary, postfixes: Vec::new() });
            }
            loop {
                let postfix = match r.kind() {
                    Some(QUESTION) => Postfix::Optional,
                    Some(STAR) => Postfix::ZeroOrMore,
                    Some(PLUS) => Postfix::OneOrMore,
                    _ => break,
                };
                r.skip();
                top.term().postfixes.push(postfix);
            }
            let kind = r.kind();
            if kind.is_some() && kind == top.close {
                r.skip();
                let closed = std::mem::replace(&mut top, stack.pop().expect("a bracket is open"));
                except = closed.except;
                let bracket = closed.bracket.expect("a bracket frame");
                let inside = closed.build(grammar);
                primary = match bracket {
                    Bracket::Group => inside,
                    Bracket::Optional => grammar.optional(inside),
                    Bracket::Repetition => grammar.repetition(inside, 0.0, f64::INFINITY),
                };
                continue;
            }
            match kind {
                Some(MINUS) => {
                    r.skip();
                    except = true;
                }
                Some(COMMA) => r.skip(),
                Some(BAR) => {
                    r.skip();
                    top.alternatives.push(Vec::new());
                }
                Some(STRING | ID | SPECIAL | GROUP | OPTION | REPEAT) => {}
                _ => match top.close {
                    None => return Ok(top.build(grammar)),
                    Some(close) => return Err(r.fail(&format!("token of type '{}'", TOKENS[close].name))),
                },
            }
            break;
        }
    }
}

/// Pele's `parseEbnf` and `ebnfRules`.
pub(super) fn parse(src: &[u16], grammar: &mut Grammar) -> Result<Header, SyntaxError> {
    parse_grammar(src, &TOKENS, TITLE_KIND, unescape, |r| {
        let name = r.expect(ID)?.into();
        if !r.accept(EQUALS) {
            r.expect(DEFINES)?;
        }
        let definition = definition(r, grammar)?;
        r.expect(SEMICOLON)?;
        grammar.rules.push(Rule { name, definition });
        Ok(())
    })
}
