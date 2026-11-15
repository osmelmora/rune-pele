//! The tree view tokenizer and parser, ported from Pele's `treeview/parser.ts`: one entry per
//! line, its indentation, its name and its annotations.

use crate::diagrams::tokens::{
    ACC_TITLE, Matcher, Reader, SyntaxError, TokenType, acc_descr_named, acc_descr_value, acc_title_value, hidden,
    keyword, line_text, title_before, title_value, token, tokenize,
};
use crate::input::{Units, is_blank, is_letter, is_word};

const fn unit(c: u8) -> u16 {
    c as u16
}

const KEYWORD: usize = 0;
const ACC_DESCR: usize = 1;
const ACC_TITLE_KIND: usize = 2;
const TITLE: usize = 3;
const CLASS_ANNOTATION: usize = 4;
const ICON_ANNOTATION: usize = 5;
const DESC_ANNOTATION: usize = 6;
const INDENTATION: usize = 7;
const QUOTED_NAME: usize = 8;
const BARE_NAME: usize = 12;

fn blanks(src: &[u16], at: usize) -> Option<usize> {
    let end = src.run(at, is_blank);
    (end > at).then_some(end)
}

/// `[ \t]+:::[ \t]*[A-Za-z_][\w-]*`
fn class_annotation(src: &[u16], at: usize) -> Option<usize> {
    let marker = blanks(src, at)?;
    if !src.starts_at(marker, ":::") {
        return None;
    }
    let name = src.run(marker + 3, is_blank);
    let first = src.at(name)?;
    (is_letter(first) || first == unit(b'_')).then(|| src.run(name + 1, |c| is_word(c) || c == unit(b'-')))
}

/// `[ \t]+icon\([\w-]*(?::[\w-]+)?\)`
fn icon_annotation(src: &[u16], at: usize) -> Option<usize> {
    let word = |c: u16| is_word(c) || c == unit(b'-');
    let marker = blanks(src, at)?;
    if !src.starts_at(marker, "icon(") {
        return None;
    }
    let mut end = src.run(marker + 5, word);
    if src.at(end) == Some(unit(b':')) {
        let pack = src.run(end + 1, word);
        if pack > end + 1 {
            end = pack;
        }
    }
    (src.at(end) == Some(unit(b')'))).then_some(end + 1)
}

/// `[ \t]+##[^\n\r]*`
fn desc_annotation(src: &[u16], at: usize) -> Option<usize> {
    let marker = blanks(src, at)?;
    src.starts_at(marker, "##").then(|| src.run(marker + 2, |c| c != 10 && c != 13))
}

/// `"[^"]*"|'[^']*'`
fn quoted_name(src: &[u16], at: usize) -> Option<usize> {
    let quote = src.at(at).filter(|&c| c == unit(b'"') || c == unit(b'\''))?;
    src.find_unit(quote, at + 1).map(|close| close + 1)
}

/// `%%[^\n]*`
fn comment(src: &[u16], at: usize) -> Option<usize> {
    src.starts_at(at, "%%").then(|| src.run(at + 2, |c| c != 10))
}

/// `\r?\n`
fn line_break(src: &[u16], at: usize) -> Option<usize> {
    let feed = at + usize::from(src.at(at) == Some(13));
    (src.at(feed) == Some(10)).then_some(feed + 1)
}

/// Whether an annotation starts here, as the bare name's lookahead asks.
fn annotation_at(src: &[u16], at: usize) -> bool {
    if src.starts_at(at, "##") || src.starts_at(at, "icon(") {
        return true;
    }
    if !src.starts_at(at, ":::") {
        return false;
    }
    let name = src.run(at + 3, is_blank);
    src.at(name).is_some_and(|c| is_letter(c) || c == unit(b'_'))
}

/// Pele's `matchBareName`, for `(?!:::|icon\(|##)[^ \t\n\r"'](?:(?![ \t]+:::[ \t]*[A-Za-z_]|[ \t]+icon\(|[ \t]+##)[^\n\r])*`:
/// the rest of the line, up to blanks that an annotation follows. Each run of blanks is looked
/// past once.
fn bare_name(src: &[u16], at: usize) -> Option<usize> {
    let first = src.at(at)?;
    if is_blank(first) || first == 10 || first == 13 || first == unit(b'"') || first == unit(b'\'') {
        return None;
    }
    if src.starts_at(at, ":::") || src.starts_at(at, "icon(") || src.starts_at(at, "##") {
        return None;
    }
    let mut i = at + 1;
    while let Some(c) = src.at(i) {
        if c == 10 || c == 13 {
            break;
        }
        if is_blank(c) {
            let after = src.run(i + 1, is_blank);
            if annotation_at(src, after) {
                break;
            }
            i = after;
        } else {
            i += 1;
        }
    }
    Some(i)
}

static TOKENS: [TokenType; 13] = [
    keyword("treeView-beta").longer(&[BARE_NAME]),
    acc_descr_named("ACC_DESCR"),
    ACC_TITLE,
    title_before(line_text),
    token("CLASS_ANNOTATION", Matcher::Pattern(class_annotation)),
    token("ICON_ANNOTATION", Matcher::Pattern(icon_annotation)),
    token("DESC_ANNOTATION", Matcher::Pattern(desc_annotation)),
    token("INDENTATION", Matcher::Pattern(blanks)),
    token("QUOTED_NAME", Matcher::Pattern(quoted_name)),
    hidden("WS", Matcher::Pattern(blanks)),
    hidden("ML_COMMENT", Matcher::Pattern(comment)),
    hidden("NL", Matcher::Pattern(line_break)),
    token("BARE_NAME", Matcher::Pattern(bare_name)),
];

/// One entry as written. Each annotation keeps the value Mermaid's converter gives it.
#[derive(Default)]
pub(crate) struct AstNode {
    pub(crate) indent: Option<usize>,
    pub(crate) name: Vec<u16>,
    pub(crate) class: Option<Vec<u16>>,
    pub(crate) icon: Option<Vec<u16>>,
    pub(crate) description: Option<Vec<u16>>,
}

#[derive(Default)]
pub(crate) struct TreeViewAst {
    pub(crate) title: Option<Vec<u16>>,
    pub(crate) acc_title: Option<Vec<u16>>,
    pub(crate) acc_descr: Option<Vec<u16>>,
    pub(crate) nodes: Vec<AstNode>,
}

fn trim_blanks_end(text: &[u16]) -> &[u16] {
    let end = text.iter().rposition(|&c| !is_blank(c)).map_or(0, |last| last + 1);
    &text[..end]
}

/// Pele's `parseTreeView`.
pub(crate) fn parse_tree_view(src: &[u16]) -> Result<TreeViewAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &TOKENS)?, &TOKENS);
    let mut ast = TreeViewAst::default();
    r.expect(KEYWORD)?;
    loop {
        match r.kind() {
            Some(ACC_DESCR) => ast.acc_descr = Some(acc_descr_value(r.take())),
            Some(ACC_TITLE_KIND) => ast.acc_title = Some(acc_title_value(r.take())),
            Some(TITLE) => ast.title = Some(title_value(r.take())),
            _ => break,
        }
    }
    while r.kind().is_some() {
        let mut node = AstNode::default();
        if r.kind() == Some(INDENTATION) {
            node.indent = Some(r.take().len());
        }
        match r.kind() {
            Some(QUOTED_NAME) => {
                let text = r.take();
                node.name = text[1..text.len() - 1].to_vec();
            }
            Some(BARE_NAME) => node.name = trim_blanks_end(r.take()).to_vec(),
            _ => return Err(r.fail("a name")),
        }
        loop {
            match r.kind() {
                Some(CLASS_ANNOTATION) => node.class = Some(r.take().trim_js()[3..].trim_js().to_vec()),
                Some(ICON_ANNOTATION) => {
                    let trimmed = r.take().trim_js();
                    node.icon = Some(trimmed[5..trimmed.len() - 1].to_vec());
                }
                Some(DESC_ANNOTATION) => node.description = Some(r.take().trim_js()[2..].trim_js().to_vec()),
                _ => break,
            }
        }
        ast.nodes.push(node);
    }
    Ok(ast)
}
