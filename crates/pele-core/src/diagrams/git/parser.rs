//! Pele's `parseGit` (`src/diagrams/git/parser.ts`): Mermaid's Langium grammar for git graphs,
//! read by a hand-written parser over the shared tokenizer.

use super::{CommitType, Direction};
use crate::diagrams::tokens::{
    ACC_DESCR_CLOSED, ACC_TITLE, DIRECTIVE, INT, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, SyntaxError,
    TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword, ref_name, string_value, title_before,
    title_value, token, tokenize,
};
use crate::input::{Units, decimal, is_word};

const CHERRY_PICK: usize = 0;
const GIT_GRAPH_COLON: usize = 1;
const HIGHLIGHT: usize = 2;
const GIT_GRAPH: usize = 3;
const CHECKOUT: usize = 4;
const REVERSE: usize = 5;
const PARENT: usize = 6;
const COMMIT: usize = 7;
const NORMAL: usize = 8;
const BRANCH: usize = 9;
const ORDER: usize = 10;
const SWITCH: usize = 11;
const TYPE: usize = 12;
const MERGE: usize = 13;
const MSG: usize = 14;
const TAG: usize = 15;
const ID: usize = 16;
const LR: usize = 17;
const TB: usize = 18;
const BT: usize = 19;
const COLON: usize = 20;
const ACC_DESCR_KIND: usize = 21;
const ACC_TITLE_KIND: usize = 22;
const TITLE_KIND: usize = 23;
const INT_KIND: usize = 24;
const STRING_KIND: usize = 25;
const NEWLINE_KIND: usize = 26;
const REFERENCE: usize = 31;

/// A keyword that is the start of a longer name is part of that name: `commits` is a branch.
const NAME: &[usize] = &[REFERENCE];

fn is_name(c: u16) -> bool {
    c == u16::from(b'-') || is_word(c)
}

/// `\w([-\./\w]*[-\w])?`: a name may hold dots and slashes, but does not end in one.
fn reference(src: &[u16], at: usize) -> Option<usize> {
    if !is_word(src.at(at)?) {
        return None;
    }
    let end = src.run(at + 1, |c| c == u16::from(b'.') || c == u16::from(b'/') || is_name(c));
    Some((at + 1..end).rev().find(|&i| is_name(src[i])).map_or(at + 1, |last| last + 1))
}

const fn word(name: &'static str) -> TokenType {
    token(name, Matcher::Literal(name)).longer(NAME)
}

const fn literal(name: &'static str) -> TokenType {
    token(name, Matcher::Literal(name))
}

static GIT_TOKENS: [TokenType; 32] = [
    word("cherry-pick"),
    literal("gitGraph:"),
    word("HIGHLIGHT"),
    keyword("gitGraph").longer(NAME),
    word("checkout"),
    word("REVERSE"),
    literal("parent:"),
    word("commit"),
    word("NORMAL"),
    word("branch"),
    literal("order:"),
    word("switch"),
    literal("type:"),
    word("merge"),
    literal("msg:"),
    literal("tag:"),
    literal("id:"),
    word("LR"),
    word("TB"),
    word("BT"),
    literal(":"),
    // Without a closing brace the block form scans to the end of the text, and a brace is not a
    // token on its own, so the first failure is final.
    ACC_DESCR_CLOSED,
    ACC_TITLE,
    title_before(ref_name),
    INT,
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
    token("REFERENCE", Matcher::Pattern(reference)),
];

#[derive(Debug)]
pub(super) enum Statement {
    Commit { id: Option<Vec<u16>>, message: Option<Vec<u16>>, tags: Vec<Vec<u16>>, kind: Option<CommitType> },
    Branch { name: Vec<u16>, order: Option<f64> },
    Merge { branch: Vec<u16>, id: Option<Vec<u16>>, tags: Vec<Vec<u16>>, kind: Option<CommitType> },
    Checkout { branch: Vec<u16> },
    CherryPick { id: Option<Vec<u16>>, tags: Vec<Vec<u16>>, parent: Option<Vec<u16>> },
}

#[derive(Debug, Default)]
pub(super) struct GitAst {
    pub(super) direction: Option<Direction>,
    pub(super) title: Option<Vec<u16>>,
    pub(super) acc_title: Option<Vec<u16>>,
    pub(super) acc_descr: Option<Vec<u16>>,
    pub(super) statements: Vec<Statement>,
}

fn end_of_line(r: &mut Reader<'_>) -> Result<(), SyntaxError> {
    match r.kind() {
        None => Ok(()),
        Some(NEWLINE_KIND) => {
            while r.accept(NEWLINE_KIND) {}
            Ok(())
        }
        Some(_) => Err(r.fail("a line break or the end of input")),
    }
}

fn name(r: &mut Reader<'_>) -> Result<Vec<u16>, SyntaxError> {
    match r.kind() {
        Some(REFERENCE) => Ok(r.take().to_vec()),
        Some(STRING_KIND) => Ok(string_value(r.take())),
        _ => Err(r.fail("a branch name")),
    }
}

fn string(r: &mut Reader<'_>) -> Result<Vec<u16>, SyntaxError> {
    Ok(string_value(r.expect(STRING_KIND)?))
}

fn commit_type(r: &mut Reader<'_>) -> Result<CommitType, SyntaxError> {
    let kind = match r.kind() {
        Some(NORMAL) => CommitType::Normal,
        Some(REVERSE) => CommitType::Reverse,
        Some(HIGHLIGHT) => CommitType::Highlight,
        _ => return Err(r.fail("NORMAL, REVERSE, or HIGHLIGHT")),
    };
    r.skip();
    Ok(kind)
}

pub(super) fn parse_git(src: &[u16]) -> Result<GitAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &GIT_TOKENS)?, &GIT_TOKENS);
    let mut ast = GitAst::default();

    while r.accept(NEWLINE_KIND) {}
    if !r.accept(GIT_GRAPH_COLON) {
        r.expect(GIT_GRAPH)?;
        let direction = match r.kind() {
            Some(LR) => Some(Direction::LeftRight),
            Some(TB) => Some(Direction::TopBottom),
            Some(BT) => Some(Direction::BottomTop),
            _ => None,
        };
        match direction {
            Some(direction) if r.next_kind() == Some(COLON) => {
                ast.direction = Some(direction);
                r.skip();
                r.skip();
            }
            _ => {
                r.accept(COLON);
            }
        }
    }

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
            COMMIT => {
                r.skip();
                let (mut id, mut message, mut tags, mut kind) = (None, None, Vec::new(), None);
                loop {
                    if r.accept(ID) {
                        id = Some(string(&mut r)?);
                    } else if r.accept(TAG) {
                        tags.push(string(&mut r)?);
                    } else if r.accept(TYPE) {
                        kind = Some(commit_type(&mut r)?);
                    } else if r.accept(MSG) || r.kind() == Some(STRING_KIND) {
                        message = Some(string(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(Statement::Commit { id, message, tags, kind });
            }
            BRANCH => {
                r.skip();
                let name = name(&mut r)?;
                let order = if r.accept(ORDER) { Some(decimal(r.expect(INT_KIND)?)) } else { None };
                end_of_line(&mut r)?;
                ast.statements.push(Statement::Branch { name, order });
            }
            MERGE => {
                r.skip();
                let branch = name(&mut r)?;
                let (mut id, mut tags, mut kind) = (None, Vec::new(), None);
                loop {
                    if r.accept(ID) {
                        id = Some(string(&mut r)?);
                    } else if r.accept(TAG) {
                        tags.push(string(&mut r)?);
                    } else if r.accept(TYPE) {
                        kind = Some(commit_type(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(Statement::Merge { branch, id, tags, kind });
            }
            CHECKOUT | SWITCH => {
                r.skip();
                let branch = name(&mut r)?;
                end_of_line(&mut r)?;
                ast.statements.push(Statement::Checkout { branch });
            }
            CHERRY_PICK => {
                r.skip();
                let (mut id, mut tags, mut parent) = (None, Vec::new(), None);
                loop {
                    if r.accept(ID) {
                        id = Some(string(&mut r)?);
                    } else if r.accept(TAG) {
                        tags.push(string(&mut r)?);
                    } else if r.accept(PARENT) {
                        parent = Some(string(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(Statement::CherryPick { id, tags, parent });
            }
            _ => return Err(r.fail("a statement, a title, or a line break")),
        }
    }
    Ok(ast)
}
