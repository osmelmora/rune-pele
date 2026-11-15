//! `railroad-beta`: Mermaid's own notation, one constructor call per node. Ported from Pele's
//! `railroad/ir.ts`.

use super::lexer::{
    ACC_DESCR, ACC_TITLE, TITLE, block_comment_named, hidden_tokens, identifier_named, literal, parse_grammar, quoted,
    unescape,
};
use super::{Grammar, Header, NodeId, Rule};
use crate::diagrams::tokens::{Reader, SyntaxError, TokenType, keyword};

const NONTERMINAL: usize = 1;
const ZERO_OR_MORE: usize = 2;
const ONE_OR_MORE: usize = 3;
const SEQUENCE: usize = 4;
const OPTIONAL: usize = 5;
const TERMINAL: usize = 6;
const SPECIAL: usize = 7;
const CHOICE: usize = 8;
const EQUALS: usize = 9;
const SEMICOLON: usize = 10;
const OPEN: usize = 11;
const COMMA: usize = 12;
const CLOSE: usize = 13;
const TITLE_KIND: usize = 14;
const ID: usize = 17;
const STRING: usize = 18;

/// A constructor's name. A name that starts with it and goes on is a name.
const fn word(name: &'static str) -> TokenType {
    literal(name).longer(&[ID])
}

const HIDDEN: [TokenType; 4] = hidden_tokens(["RR_WHITESPACE", "RR_YAML", "RR_DIRECTIVE", "RR_SINGLE_LINE_COMMENT"]);

pub(crate) static TOKENS: [TokenType; 24] = {
    let [whitespace, yaml, directive, comment] = HIDDEN;
    [
        keyword("railroad-beta").longer(&[ID]),
        word("nonterminal"),
        word("zeroOrMore"),
        word("oneOrMore"),
        word("sequence"),
        word("optional"),
        word("terminal"),
        word("special"),
        word("choice"),
        literal("="),
        literal(";"),
        literal("("),
        literal(","),
        literal(")"),
        TITLE,
        ACC_TITLE,
        ACC_DESCR,
        identifier_named("RR_ID"),
        quoted("RR_STRING"),
        whitespace,
        yaml,
        directive,
        comment,
        block_comment_named("RR_BLOCK_COMMENT"),
    ]
};

/// A constructor call whose closing parenthesis is still to come.
enum Open {
    Sequence(Vec<NodeId>),
    Choice(Vec<NodeId>),
    Optional,
    OneOrMore,
    ZeroOrMore,
}

/// `terminal("…")` and the like: the string, with its escapes read.
fn argument(r: &mut Reader<'_>) -> Result<Vec<u16>, SyntaxError> {
    r.skip();
    r.expect(OPEN)?;
    let value = unescape(r.expect(STRING)?);
    r.expect(CLOSE)?;
    Ok(value)
}

/// One expression, read with a stack of open calls rather than by recursion.
fn expression(r: &mut Reader<'_>, grammar: &mut Grammar) -> Result<NodeId, SyntaxError> {
    let mut open: Vec<Open> = Vec::new();
    loop {
        let mut value = match r.kind() {
            Some(TERMINAL) => {
                let value = argument(r)?;
                grammar.terminal(value)
            }
            Some(NONTERMINAL) => {
                let name = argument(r)?;
                grammar.nonterminal(name)
            }
            Some(SPECIAL) => {
                let text = argument(r)?;
                grammar.special(text)
            }
            Some(kind @ (SEQUENCE | CHOICE | OPTIONAL | ONE_OR_MORE | ZERO_OR_MORE)) => {
                r.skip();
                r.expect(OPEN)?;
                open.push(match kind {
                    SEQUENCE => Open::Sequence(Vec::new()),
                    CHOICE => Open::Choice(Vec::new()),
                    OPTIONAL => Open::Optional,
                    ONE_OR_MORE => Open::OneOrMore,
                    _ => Open::ZeroOrMore,
                });
                continue;
            }
            _ => return Err(r.fail("an expression")),
        };
        loop {
            let Some(parent) = open.last_mut() else {
                return Ok(value);
            };
            if let Open::Sequence(parts) | Open::Choice(parts) = parent {
                parts.push(value);
                if r.accept(COMMA) {
                    break;
                }
            }
            r.expect(CLOSE)?;
            value = match open.pop().expect("a call is open") {
                Open::Sequence(parts) => grammar.sequence(parts),
                Open::Choice(parts) => grammar.choice(parts),
                Open::Optional => grammar.optional(value),
                Open::OneOrMore => grammar.repetition(value, 1.0, f64::INFINITY),
                Open::ZeroOrMore => grammar.repetition(value, 0.0, f64::INFINITY),
            };
        }
    }
}

/// Pele's `parseIr` and `irRules`.
pub(super) fn parse(src: &[u16], grammar: &mut Grammar) -> Result<Header, SyntaxError> {
    parse_grammar(src, &TOKENS, TITLE_KIND, unescape, |r| {
        let name = r.expect(ID)?.into();
        r.expect(EQUALS)?;
        let definition = expression(r, grammar)?;
        r.expect(SEMICOLON)?;
        grammar.rules.push(Rule { name, definition });
        Ok(())
    })
}
