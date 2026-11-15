//! `railroad-abnf-beta`: ABNF as in RFC 5234, with each rule closed by a semicolon. Ported from
//! Pele's `railroad/abnf.ts`.

use super::lexer::{ABNF_TITLE, ACC_DESCR, ACC_TITLE, hidden_tokens, literal, parse_grammar, unquote};
use super::{Grammar, Header, NodeId, Rule};
use crate::diagrams::tokens::{Matcher, Reader, SyntaxError, TokenType, hidden, keyword, token};
use crate::input::{Units, decimal, is_digit, is_hex, is_letter};

const EQUALS: usize = 1;
const SEMICOLON: usize = 2;
const SLASH: usize = 3;
const GROUP: usize = 4;
const OPTION: usize = 6;
const TITLE_KIND: usize = 8;
const NAME: usize = 11;
const STRING: usize = 12;
const NUMBER: usize = 13;
const REPEAT: usize = 14;
const EXACT: usize = 15;
const COMMENT_KIND: usize = 20;

/// `[A-Za-z][A-Za-z0-9-]*`
fn rule_name(src: &[u16], at: usize) -> Option<usize> {
    src.at(at).filter(|&c| is_letter(c))?;
    Some(src.run(at + 1, |c| is_letter(c) || is_digit(c) || c == u16::from(b'-')))
}

/// `;[^\n\r]*`
fn comment(src: &[u16], at: usize) -> Option<usize> {
    (src.at(at) == Some(u16::from(b';'))).then(|| src.run(at + 1, |c| c != 10 && c != 13))
}

/// `"[^"]*"`
fn string(src: &[u16], at: usize) -> Option<usize> {
    (src.at(at) == Some(u16::from(b'"'))).then_some(())?;
    src.find_unit(u16::from(b'"'), at + 1).map(|close| close + 1)
}

/// `%[xXdDbB][0-9A-Fa-f]+(?:-[0-9A-Fa-f]+|\.[0-9A-Fa-f]+)*`
fn number(src: &[u16], at: usize) -> Option<usize> {
    if src.at(at) != Some(u16::from(b'%')) || !src.at(at + 1).is_some_and(|c| c < 128 && b"xXdDbB".contains(&(c as u8)))
    {
        return None;
    }
    let mut end = src.run(at + 2, is_hex);
    if end == at + 2 {
        return None;
    }
    while matches!(src.at(end), Some(c) if c == u16::from(b'-') || c == u16::from(b'.')) {
        let next = src.run(end + 1, is_hex);
        if next == end + 1 {
            break;
        }
        end = next;
    }
    Some(end)
}

/// `[0-9]*\*[0-9]*`
fn repeat(src: &[u16], at: usize) -> Option<usize> {
    let star = src.run(at, is_digit);
    (src.at(star) == Some(u16::from(b'*'))).then(|| src.run(star + 1, is_digit))
}

/// `[0-9]+`
fn exact(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, is_digit))
}

const COMMENT: TokenType = hidden("ABNF_COMMENT", Matcher::Pattern(comment));
const HIDDEN: [TokenType; 4] =
    hidden_tokens(["ABNF_WHITESPACE", "ABNF_YAML", "ABNF_DIRECTIVE", "ABNF_SINGLE_LINE_COMMENT"]);

pub(crate) static TOKENS: [TokenType; 21] = {
    let [whitespace, yaml, directive, single_line_comment] = HIDDEN;
    [
        keyword("railroad-abnf-beta").longer(&[NAME]),
        literal("="),
        // A semicolon closes a rule only at the end of its line; with anything after it, it
        // starts a comment.
        literal(";").longer(&[COMMENT_KIND]),
        literal("/"),
        literal("("),
        literal(")"),
        literal("["),
        literal("]"),
        ABNF_TITLE,
        ACC_TITLE,
        ACC_DESCR,
        token("ABNF_RULENAME", Matcher::Pattern(rule_name)),
        token("ABNF_STRING", Matcher::Pattern(string)),
        token("ABNF_NUMVAL", Matcher::Pattern(number)),
        token("ABNF_REPEAT", Matcher::Pattern(repeat)),
        token("ABNF_EXACT_REPEAT", Matcher::Pattern(exact)),
        whitespace,
        yaml,
        directive,
        single_line_comment,
        COMMENT,
    ]
};

/// An element's repeat as the model has it: Pele's `node` for an `AbnfElement`.
fn repeated(grammar: &mut Grammar, element: NodeId, repeat: Option<&[u16]>) -> NodeId {
    let Some(repeat) = repeat else {
        return element;
    };
    let star = repeat.find_unit(u16::from(b'*'), 0);
    // `parseInt(text, 10)` of text that starts with a run of digits.
    let leading = |text: &[u16]| decimal(&text[..text.run(0, is_digit)]);
    let (min, max) = match star {
        None => {
            let count = leading(repeat);
            (count, count)
        }
        Some(star) => {
            let min = if star > 0 { leading(repeat) } else { 0.0 };
            let max = if star < repeat.len() - 1 { leading(&repeat[star + 1..]) } else { f64::INFINITY };
            (min, max)
        }
    };
    if min == 0.0 && max == 1.0 { grammar.optional(element) } else { grammar.repetition(element, min, max) }
}

#[derive(Clone, Copy)]
enum Bracket {
    Group,
    Optional,
}

/// One open bracket, or the rule itself at the bottom of the stack: the concatenations of its
/// alternation so far, the bracket, its closing token, and the repeat written before it.
struct Frame {
    alternatives: Vec<Vec<NodeId>>,
    bracket: Option<Bracket>,
    close: Option<usize>,
    repeat: Option<Vec<u16>>,
}

impl Frame {
    fn new(bracket: Option<Bracket>, close: Option<usize>, repeat: Option<Vec<u16>>) -> Self {
        Frame { alternatives: vec![Vec::new()], bracket, close, repeat }
    }
}

/// One alternation, read with a stack of open brackets rather than by recursion.
fn alternation(r: &mut Reader<'_>, grammar: &mut Grammar) -> Result<NodeId, SyntaxError> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut top = Frame::new(None, None, None);
    loop {
        let mut repeat = match r.kind() {
            Some(REPEAT | EXACT) => Some(r.take().to_vec()),
            _ => None,
        };
        let mut primary = match r.kind() {
            Some(STRING) => {
                let text = r.take();
                grammar.terminal(&text[1..text.len() - 1])
            }
            Some(NUMBER) => {
                let text = r.take();
                grammar.terminal(text)
            }
            Some(NAME) => {
                let name = r.take();
                grammar.nonterminal(name)
            }
            Some(kind @ (GROUP | OPTION)) => {
                r.skip();
                let bracket = if kind == GROUP { Bracket::Group } else { Bracket::Optional };
                stack.push(std::mem::replace(&mut top, Frame::new(Some(bracket), Some(kind + 1), repeat)));
                continue;
            }
            _ => return Err(r.fail("a string, a number, a name, or an opening bracket")),
        };
        loop {
            let element = repeated(grammar, primary, repeat.as_deref());
            top.alternatives.last_mut().expect("an alternation has a concatenation").push(element);
            let kind = r.kind();
            if kind.is_some() && kind == top.close {
                r.skip();
                let closed = std::mem::replace(&mut top, stack.pop().expect("a bracket is open"));
                let alternatives = closed.alternatives.into_iter().map(|elements| grammar.sequence(elements)).collect();
                let inside = grammar.choice(alternatives);
                primary = match closed.bracket.expect("a bracket frame") {
                    Bracket::Group => inside,
                    Bracket::Optional => grammar.optional(inside),
                };
                repeat = closed.repeat;
                continue;
            }
            match kind {
                Some(SLASH) => {
                    r.skip();
                    top.alternatives.push(Vec::new());
                }
                Some(STRING | NUMBER | NAME | GROUP | OPTION | REPEAT | EXACT) => {}
                _ => match top.close {
                    None => {
                        let alternatives =
                            top.alternatives.into_iter().map(|elements| grammar.sequence(elements)).collect();
                        return Ok(grammar.choice(alternatives));
                    }
                    Some(close) => return Err(r.fail(&format!("token of type '{}'", TOKENS[close].name))),
                },
            }
            break;
        }
    }
}

/// Pele's `parseAbnf` and `abnfRules`.
pub(super) fn parse(src: &[u16], grammar: &mut Grammar) -> Result<Header, SyntaxError> {
    parse_grammar(src, &TOKENS, TITLE_KIND, unquote, |r| {
        let name = r.expect(NAME)?.into();
        r.expect(EQUALS)?;
        let definition = alternation(r, grammar)?;
        r.expect(SEMICOLON)?;
        grammar.rules.push(Rule { name, definition });
        Ok(())
    })
}
