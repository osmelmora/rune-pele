//! Pele's `parseArchitecture` and its token table, `ARCHITECTURE_TOKENS`.

use super::{Alignment, Side};
use crate::diagrams::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, SyntaxError, TokenType,
    WHITESPACE, YAML, acc_descr_value, acc_title_value, dashed_name, string_value, title_before, title_value, token,
    tokenize,
};
use crate::input::{Units, is_line_end, is_word};

const HEADER: usize = 0;
const JUNCTION: usize = 1;
const SERVICE: usize = 2;
const COLUMN: usize = 3;
const GROUP: usize = 4;
const ALIGN: usize = 5;
const ROW: usize = 6;
const DASHES: usize = 7;
const IN: usize = 8;
const COLON: usize = 9;
const DASH: usize = 10;
const DIRECTION: usize = 11;
const ARROW_GROUP: usize = 12;
const INTO: usize = 13;
const ACC_DESCR_KIND: usize = 14;
const ACC_TITLE_KIND: usize = 15;
const TITLE_KIND: usize = 16;
const STRING_KIND: usize = 17;
const ID: usize = 18;
const NEWLINE_KIND: usize = 19;
const YAML_KIND: usize = 21;
const ICON: usize = 24;
const LABEL: usize = 25;

fn is_dash_or_word(c: u16) -> bool {
    c == u16::from(b'-') || is_word(c)
}

/// `[\w]([-\w]*\w)?`
fn id(src: &[u16], at: usize) -> Option<usize> {
    if !src.at(at).is_some_and(is_word) {
        return None;
    }
    let run = src.run(at + 1, is_dash_or_word);
    (at..run).rev().find(|&i| is_word(src[i])).map(|last| last + 1)
}

/// `L|R|T|B`
fn direction(src: &[u16], at: usize) -> Option<usize> {
    src.at(at).filter(|&c| b"LRTB".map(u16::from).contains(&c)).map(|_| at + 1)
}

/// `<|>`
fn into(src: &[u16], at: usize) -> Option<usize> {
    src.at(at).filter(|&c| c == u16::from(b'<') || c == u16::from(b'>')).map(|_| at + 1)
}

/// `\([\w-:]+\)`
fn icon(src: &[u16], at: usize) -> Option<usize> {
    if src.at(at) != Some(u16::from(b'(')) {
        return None;
    }
    let end = src.run(at + 1, |c| is_dash_or_word(c) || c == u16::from(b':'));
    (end > at + 1 && src.at(end) == Some(u16::from(b')'))).then_some(end + 1)
}

/// The end of a quoted string that starts at `at` with `quote`, as `"([^"\\]|\\.)*"` reads it.
fn quoted(src: &[u16], at: usize, quote: u16) -> Option<usize> {
    let mut i = at + 1;
    loop {
        let c = src.at(i)?;
        if c == quote {
            return Some(i + 1);
        }
        if c == u16::from(b'\\') {
            if is_line_end(src.at(i + 1)?) {
                return None;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
}

/// `\[(?:"([^"\\]|\\.)*"|'([^'\\]|\\.)*'|[^\[\]\r\n]+)\]`
fn label(src: &[u16], at: usize) -> Option<usize> {
    let close = u16::from(b']');
    if src.at(at) != Some(u16::from(b'[')) {
        return None;
    }
    let first = src.at(at + 1)?;
    if (first == u16::from(b'"') || first == u16::from(b'\''))
        && let Some(end) = quoted(src, at + 1, first).filter(|&end| src.at(end) == Some(close))
    {
        return Some(end + 1);
    }
    let end = src.run(at + 1, |c| ![u16::from(b'['), close, 10, 13].contains(&c));
    (end > at + 1 && src.at(end) == Some(close)).then_some(end + 1)
}

const fn word(text: &'static str) -> TokenType {
    token(text, Matcher::Literal(text)).longer(&[ID])
}

// The direction letters come before identifiers and have no longer alternative, so Mermaid reads
// an id that starts with L, R, T or B as a direction followed by the rest.
pub(super) static ARCHITECTURE_TOKENS: [TokenType; 26] = [
    word("architecture-beta"),
    word("junction"),
    word("service"),
    word("column"),
    word("group"),
    word("align"),
    word("row"),
    token("--", Matcher::Literal("--")).longer(&[YAML_KIND]),
    word("in"),
    token(":", Matcher::Literal(":")),
    token("-", Matcher::Literal("-")).longer(&[YAML_KIND]),
    token("ARROW_DIRECTION", Matcher::Pattern(direction)),
    token("ARROW_GROUP", Matcher::Literal("{group}")),
    token("ARROW_INTO", Matcher::Pattern(into)),
    ACC_DESCR,
    ACC_TITLE,
    title_before(dashed_name),
    STRING,
    token("ID", Matcher::Pattern(id)),
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
    token("ARCH_ICON", Matcher::Pattern(icon)),
    token("ARCH_TITLE", Matcher::Pattern(label)),
];

#[derive(Default)]
pub(super) struct GroupAst {
    pub(super) id: Vec<u16>,
    pub(super) icon: Option<Vec<u16>>,
    pub(super) title: Option<Vec<u16>>,
    pub(super) parent: Option<Vec<u16>>,
}

#[derive(Default)]
pub(super) struct ServiceAst {
    pub(super) id: Vec<u16>,
    pub(super) icon: Option<Vec<u16>>,
    pub(super) icon_text: Option<Vec<u16>>,
    pub(super) title: Option<Vec<u16>>,
    pub(super) parent: Option<Vec<u16>>,
}

pub(super) struct JunctionAst {
    pub(super) id: Vec<u16>,
    pub(super) parent: Option<Vec<u16>>,
}

pub(super) struct EndAst {
    pub(super) id: Vec<u16>,
    pub(super) via_group: bool,
    pub(super) side: Side,
    pub(super) into: bool,
}

pub(super) struct EdgeAst {
    pub(super) lhs: EndAst,
    pub(super) rhs: EndAst,
    pub(super) title: Option<Vec<u16>>,
}

pub(super) struct AlignmentAst {
    pub(super) direction: Alignment,
    pub(super) members: Vec<Vec<u16>>,
}

#[derive(Default)]
pub(super) struct ArchitectureAst {
    pub(super) title: Option<Vec<u16>>,
    pub(super) acc_title: Option<Vec<u16>>,
    pub(super) acc_descr: Option<Vec<u16>>,
    pub(super) groups: Vec<GroupAst>,
    pub(super) services: Vec<ServiceAst>,
    pub(super) junctions: Vec<JunctionAst>,
    pub(super) edges: Vec<EdgeAst>,
    pub(super) alignments: Vec<AlignmentAst>,
}

/// Mermaid's `ArchitectureValueConverter` for `ARCH_TITLE`.
fn label_value(text: &[u16]) -> Vec<u16> {
    let result = text[1..text.len() - 1].trim_js();
    let quoted = |quote: u8| {
        let quote = u16::from(quote);
        result.first() == Some(&quote) && result.last() == Some(&quote)
    };
    if !(quoted(b'"') || quoted(b'\'')) {
        return result.to_vec();
    }
    let inner = if result.len() >= 2 { &result[1..result.len() - 1] } else { &[] };
    let unescaped = unescape(&unescape(inner, b'"'), b'\'');
    unescaped.trim_js().to_vec()
}

/// `text.replace(/\\<quote>/g, '<quote>')`
fn unescape(text: &[u16], quote: u8) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == u16::from(b'\\') && text.get(i + 1) == Some(&u16::from(quote)) {
            out.push(u16::from(quote));
            i += 2;
        } else {
            out.push(text[i]);
            i += 1;
        }
    }
    out
}

fn side(r: &mut Reader<'_>) -> Result<Side, SyntaxError> {
    Ok(Side::of_letter(r.expect(DIRECTION)?[0]))
}

/// Pele's `parseArchitecture`.
pub(super) fn parse(src: &[u16]) -> Result<ArchitectureAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &ARCHITECTURE_TOKENS)?, &ARCHITECTURE_TOKENS);
    let mut ast = ArchitectureAst::default();
    let owned = |text: &[u16]| text.to_vec();

    while r.accept(NEWLINE_KIND) {}
    r.expect(HEADER)?;
    while let Some(kind) = r.kind() {
        match kind {
            NEWLINE_KIND => {
                r.skip();
                continue;
            }
            ACC_DESCR_KIND => ast.acc_descr = Some(acc_descr_value(r.take())),
            ACC_TITLE_KIND => ast.acc_title = Some(acc_title_value(r.take())),
            TITLE_KIND => ast.title = Some(title_value(r.take())),
            GROUP => {
                r.skip();
                let mut group = GroupAst { id: owned(r.expect(ID)?), ..GroupAst::default() };
                if r.kind() == Some(ICON) {
                    let text = r.take();
                    group.icon = Some(owned(&text[1..text.len() - 1]));
                }
                if r.kind() == Some(LABEL) {
                    group.title = Some(label_value(r.take()));
                }
                if r.accept(IN) {
                    group.parent = Some(owned(r.expect(ID)?));
                }
                ast.groups.push(group);
            }
            SERVICE => {
                r.skip();
                let mut service = ServiceAst { id: owned(r.expect(ID)?), ..ServiceAst::default() };
                if r.kind() == Some(STRING_KIND) {
                    service.icon_text = Some(string_value(r.take()));
                } else if r.kind() == Some(ICON) {
                    let text = r.take();
                    service.icon = Some(owned(&text[1..text.len() - 1]));
                }
                if r.kind() == Some(LABEL) {
                    service.title = Some(label_value(r.take()));
                }
                if r.accept(IN) {
                    service.parent = Some(owned(r.expect(ID)?));
                }
                ast.services.push(service);
            }
            JUNCTION => {
                r.skip();
                let id = owned(r.expect(ID)?);
                let parent = if r.accept(IN) { Some(owned(r.expect(ID)?)) } else { None };
                ast.junctions.push(JunctionAst { id, parent });
            }
            ID => {
                let lhs_id = owned(r.take());
                let lhs_group = r.accept(ARROW_GROUP);
                r.expect(COLON)?;
                let lhs_side = side(&mut r)?;
                let lhs_into = r.accept(INTO);
                let mut title = None;
                if !r.accept(DASHES) {
                    if r.kind() != Some(DASH) {
                        return Err(r.fail("'--' or '-'"));
                    }
                    r.skip();
                    title = Some(label_value(r.expect(LABEL)?));
                    r.expect(DASH)?;
                }
                let rhs_into = r.accept(INTO);
                let rhs_side = side(&mut r)?;
                r.expect(COLON)?;
                let rhs_id = owned(r.expect(ID)?);
                let rhs_group = r.accept(ARROW_GROUP);
                ast.edges.push(EdgeAst {
                    lhs: EndAst { id: lhs_id, via_group: lhs_group, side: lhs_side, into: lhs_into },
                    rhs: EndAst { id: rhs_id, via_group: rhs_group, side: rhs_side, into: rhs_into },
                    title,
                });
            }
            ALIGN => {
                r.skip();
                let direction = match r.kind() {
                    Some(ROW) => Alignment::Row,
                    Some(COLUMN) => Alignment::Column,
                    _ => return Err(r.fail("'row' or 'column'")),
                };
                r.skip();
                let mut members = vec![owned(r.expect(ID)?), owned(r.expect(ID)?)];
                while r.kind() == Some(ID) {
                    members.push(owned(r.take()));
                }
                ast.alignments.push(AlignmentAst { direction, members });
            }
            _ => return Err(r.fail("a group, a service, a junction, an edge, an alignment, a title, or a line break")),
        }
        match r.kind() {
            None => {}
            Some(NEWLINE_KIND) => while r.accept(NEWLINE_KIND) {},
            Some(_) => return Err(r.fail("a line break or the end of input")),
        }
    }
    Ok(ast)
}
