//! Pele's `parsePacket` (`src/diagrams/packet/parser.ts`): Mermaid's Langium grammar for packet
//! diagrams, read by a hand-written parser over the shared tokenizer.

use crate::diagrams::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, INT, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, SyntaxError, TITLE,
    TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword, string_value, title_value, token, tokenize,
};
use crate::input::decimal;

const PACKET_BETA: usize = 0;
const PACKET: usize = 1;
const DASH: usize = 2;
const PLUS: usize = 3;
const COLON: usize = 4;
const ACC_DESCR_KIND: usize = 5;
const ACC_TITLE_KIND: usize = 6;
const TITLE_KIND: usize = 7;
const INT_KIND: usize = 8;
const STRING_KIND: usize = 9;
const NEWLINE_KIND: usize = 10;
const YAML_KIND: usize = 12;

static PACKET_TOKENS: [TokenType; 15] = [
    token("packet-beta", Matcher::Literal("packet-beta")),
    keyword("packet"),
    // A front matter block also starts with dashes, so Langium lets it win over a dash.
    token("-", Matcher::Literal("-")).longer(&[YAML_KIND]),
    token("+", Matcher::Literal("+")),
    token(":", Matcher::Literal(":")),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    INT,
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

/// A field as written: its first and last bit or its width, and its label.
#[derive(Debug, Default)]
pub(super) struct BlockAst {
    pub(super) start: Option<f64>,
    pub(super) end: Option<f64>,
    pub(super) bits: Option<f64>,
    pub(super) label: Vec<u16>,
}

#[derive(Debug, Default)]
pub(super) struct PacketAst {
    pub(super) title: Option<Vec<u16>>,
    pub(super) acc_title: Option<Vec<u16>>,
    pub(super) acc_descr: Option<Vec<u16>>,
    pub(super) blocks: Vec<BlockAst>,
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

pub(super) fn parse_packet(src: &[u16]) -> Result<PacketAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &PACKET_TOKENS)?, &PACKET_TOKENS);
    let mut ast = PacketAst::default();

    while r.accept(NEWLINE_KIND) {}
    if !r.accept(PACKET_BETA) {
        r.expect(PACKET)?;
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
            INT_KIND | PLUS => {
                let mut block = BlockAst::default();
                if r.accept(PLUS) {
                    block.bits = Some(decimal(r.expect(INT_KIND)?));
                } else {
                    block.start = Some(decimal(r.take()));
                    if r.accept(DASH) {
                        block.end = Some(decimal(r.expect(INT_KIND)?));
                    }
                }
                r.expect(COLON)?;
                block.label = string_value(r.expect(STRING_KIND)?);
                ast.blocks.push(block);
                end_of_line(&mut r)?;
            }
            _ => return Err(r.fail("a field, a title, or a line break")),
        }
    }
    Ok(ast)
}
