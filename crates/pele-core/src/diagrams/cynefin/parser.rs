//! Pele's `parseCynefin` and its token table, `CYNEFIN_TOKENS`.

use super::Domain;
use crate::diagrams::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, SyntaxError, TITLE,
    TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword, string_value, title_value, token, tokenize,
};
use crate::input::Units;

const KEYWORD_COLON: usize = 0;
const KEYWORD: usize = 1;
const ARROW: usize = 2;
const COLON: usize = 3;
const DOMAIN: usize = 4;
const ACC_DESCR_KIND: usize = 5;
const ACC_TITLE_KIND: usize = 6;
const TITLE_KIND: usize = 7;
const STRING_KIND: usize = 8;
const NEWLINE_KIND: usize = 9;

const DOMAINS: [Domain; 5] = [Domain::Complex, Domain::Complicated, Domain::Clear, Domain::Chaotic, Domain::Confusion];

/// `complex|complicated|clear|chaotic|confusion`, tried in that order.
fn domain(src: &[u16], at: usize) -> Option<usize> {
    DOMAINS.iter().find(|domain| src.starts_at(at, domain.name())).map(|domain| at + domain.name().len())
}

fn domain_of(text: &[u16]) -> Domain {
    *DOMAINS.iter().find(|domain| text.is_str(domain.name())).expect("a DOMAIN_NAME token names a domain")
}

static CYNEFIN_TOKENS: [TokenType; 14] = [
    token("cynefin-beta:", Matcher::Literal("cynefin-beta:")),
    keyword("cynefin-beta"),
    token("-->", Matcher::Literal("-->")),
    token(":", Matcher::Literal(":")),
    token("DOMAIN_NAME", Matcher::Pattern(domain)),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Default)]
pub(super) struct CynefinAst {
    pub(super) title: Option<Vec<u16>>,
    pub(super) acc_title: Option<Vec<u16>>,
    pub(super) acc_descr: Option<Vec<u16>>,
    /// Each domain block with its items.
    pub(super) domains: Vec<(Domain, Vec<Vec<u16>>)>,
    /// Each transition: from, to and its label.
    pub(super) transitions: Vec<(Domain, Domain, Option<Vec<u16>>)>,
}

fn skip_lines(r: &mut Reader<'_>) {
    while r.accept(NEWLINE_KIND) {}
}

fn end_of_line(r: &mut Reader<'_>) -> Result<(), SyntaxError> {
    match r.kind() {
        None => Ok(()),
        Some(NEWLINE_KIND) => {
            skip_lines(r);
            Ok(())
        }
        Some(_) => Err(r.fail("a line break or the end of input")),
    }
}

/// Pele's `parseCynefin`.
pub(super) fn parse(src: &[u16]) -> Result<CynefinAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &CYNEFIN_TOKENS)?, &CYNEFIN_TOKENS);
    let mut ast = CynefinAst::default();

    skip_lines(&mut r);
    if !matches!(r.kind(), Some(KEYWORD | KEYWORD_COLON)) {
        return Err(r.fail("'cynefin-beta'"));
    }
    r.skip();
    while let Some(kind) = r.kind() {
        match kind {
            NEWLINE_KIND => r.skip(),
            ACC_DESCR_KIND => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                end_of_line(&mut r)?;
            }
            ACC_TITLE_KIND => {
                ast.acc_title = Some(acc_title_value(r.take()));
                end_of_line(&mut r)?;
            }
            TITLE_KIND => {
                ast.title = Some(title_value(r.take()));
                end_of_line(&mut r)?;
            }
            DOMAIN => {
                let name = domain_of(r.take());
                if r.accept(ARROW) {
                    let to = domain_of(r.expect(DOMAIN)?);
                    let label = if r.accept(COLON) { Some(string_value(r.expect(STRING_KIND)?)) } else { None };
                    end_of_line(&mut r)?;
                    ast.transitions.push((name, to, label));
                    continue;
                }
                let mut items = Vec::new();
                skip_lines(&mut r);
                while r.kind() == Some(STRING_KIND) {
                    items.push(string_value(r.take()));
                    skip_lines(&mut r);
                }
                ast.domains.push((name, items));
            }
            _ => return Err(r.fail("a domain, a transition, a title, or a line break")),
        }
    }
    Ok(ast)
}
