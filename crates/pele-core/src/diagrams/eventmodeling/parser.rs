//! The event modeling tokenizer and parser, ported from Pele's `eventmodeling/parser.ts`. The
//! token table is Mermaid's Langium grammar in its order: every keyword that is also an
//! identifier names the identifier as a longer alternative.

use crate::diagrams::tokens::{
    ACC_TITLE, DIRECTIVE, Matcher, Reader, SINGLE_LINE_COMMENT, SyntaxError, TokenType, YAML, acc_descr_named, hidden,
    keyword, line_break_or_space, newline, title_before, token, tokenize, word_unit,
};
use crate::input::{Units, is_blank, is_digit, is_letter, is_line_end, is_space, is_word};

const fn unit(c: u8) -> u16 {
    c as u16
}

/// `[_a-zA-Z][\w_]*`
fn em_id(src: &[u16], at: usize) -> Option<usize> {
    let first = src.at(at)?;
    (first == unit(b'_') || is_letter(first)).then(|| src.run(at + 1, is_word))
}

/// `\d{1,3}`
fn frame_id(src: &[u16], at: usize) -> Option<usize> {
    let mut end = at;
    while end < at + 3 && src.at(end).is_some_and(is_digit) {
        end += 1;
    }
    (end > at).then_some(end)
}

/// `\{(.*)\}|"(.*)"|'(.*)'`: the last closer on the opener's line.
fn data_inline(src: &[u16], at: usize) -> Option<usize> {
    let close = match u8::try_from(src.at(at)?) {
        Ok(b'{') => unit(b'}'),
        Ok(b'"') => unit(b'"'),
        Ok(b'\'') => unit(b'\''),
        _ => return None,
    };
    let line_end = src.run(at + 1, |c| !is_line_end(c));
    (at + 1..line_end).rev().find(|&i| src[i] == close).map(|i| i + 1)
}

/// `\{[\t ]*\r?\n(?:[\S\s]*?\r?\n)?\}(?:\r?\n|(?!\S))`: the first later line that starts with a
/// closing brace that ends its line or is followed by a space. The optional group is greedy, so
/// a brace right after the opening line counts only when no later line closes.
fn data_block(src: &[u16], at: usize) -> Option<usize> {
    if src.at(at) != Some(unit(b'{')) {
        return None;
    }
    let body = newline(src, src.run(at + 1, is_blank))?;
    let closes = |line: usize| {
        if src.at(line) == Some(unit(b'}')) { line_break_or_space(src, line + 1) } else { None }
    };
    let mut from = body;
    while let Some(line_feed) = src.find_unit(10, from) {
        if let Some(end) = closes(line_feed + 1) {
            return Some(end);
        }
        from = line_feed + 1;
    }
    closes(body)
}

/// `\s+`
fn spaces(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, is_space))
}

/// `\/\*[\s\S]*?\*\/`
fn block_comment(src: &[u16], at: usize) -> Option<usize> {
    if !src.starts_at(at, "/*") {
        return None;
    }
    src.find_str("*/", at + 2).map(|close| close + 2)
}

/// `\/\/[^\n\r]*`
fn line_comment(src: &[u16], at: usize) -> Option<usize> {
    src.starts_at(at, "//").then(|| src.run(at + 2, |c| c != 10 && c != 13))
}

const ID: usize = 34;
const FRAME_ID: usize = 35;
const INLINE: usize = 36;
const BLOCK: usize = 37;
const ACC_DESCR: usize = 38;
const ACC_TITLE_KIND: usize = 39;
const TITLE: usize = 40;

const KEYWORD: usize = 0;
const RESET_FRAME: usize = 1;
const READ_MODEL: usize = 2;
const PROCESSOR: usize = 3;
const TIME_FRAME: usize = 4;
const COMMAND: usize = 5;
const ENTITY: usize = 6;
const EVENT: usize = 7;
const JSOBJ: usize = 8;
const FIGMA: usize = 9;
const GIVEN: usize = 10;
const JSON: usize = 11;
const SALT: usize = 12;
const HTML: usize = 13;
const TEXT: usize = 14;
const DATA: usize = 15;
const NOTE: usize = 16;
const WHEN: usize = 17;
const THEN: usize = 18;
const RMO: usize = 19;
const CMD: usize = 20;
const EVT: usize = 21;
const PCR: usize = 22;
const URI: usize = 23;
const ARROW: usize = 24;
const GWT: usize = 25;
const UI: usize = 26;
const MD: usize = 27;
const TF: usize = 28;
const OPEN: usize = 29;
const CLOSE: usize = 30;
const RF: usize = 31;
const TICK: usize = 32;
const DOT: usize = 33;

const fn word(text: &'static str) -> TokenType {
    token(text, Matcher::Literal(text)).longer(&[ID])
}

const fn sign(text: &'static str) -> TokenType {
    token(text, Matcher::Literal(text))
}

static TOKENS: [TokenType; 47] = [
    keyword("eventmodeling").longer(&[ID]),
    word("resetframe"),
    word("readmodel"),
    word("processor"),
    word("timeframe"),
    word("command"),
    word("entity"),
    word("event"),
    word("jsobj"),
    word("figma"),
    word("given"),
    word("json"),
    word("salt"),
    word("html"),
    word("text"),
    word("data"),
    word("note"),
    word("when"),
    word("then"),
    word("rmo"),
    word("cmd"),
    word("evt"),
    word("pcr"),
    word("uri"),
    sign("->>"),
    word("gwt"),
    word("ui"),
    word("md"),
    word("tf"),
    sign("[["),
    sign("]]"),
    word("rf"),
    sign("`"),
    sign("."),
    token("EM_ID", Matcher::Pattern(em_id)),
    token("EM_FID", Matcher::Pattern(frame_id)),
    token("EM_DATA_INLINE", Matcher::Pattern(data_inline)),
    token("EM_DATA_BLOCK", Matcher::Pattern(data_block)),
    acc_descr_named("EM_ACC_DESCR"),
    TokenType { name: "EM_ACC_TITLE", ..ACC_TITLE },
    TokenType { name: "EM_TITLE", ..title_before(word_unit) },
    hidden("EM_WS", Matcher::Pattern(spaces)),
    TokenType { name: "EM_YAML", ..YAML },
    TokenType { name: "EM_DIRECTIVE", ..DIRECTIVE },
    TokenType { name: "EM_SINGLE_LINE_COMMENT", ..SINGLE_LINE_COMMENT },
    hidden("EM_ML_COMMENT", Matcher::Pattern(block_comment)),
    hidden("EM_SL_COMMENT", Matcher::Pattern(line_comment)),
];

const ENTITY_TYPES: [usize; 9] = [RMO, READ_MODEL, UI, CMD, COMMAND, EVT, EVENT, PCR, PROCESSOR];
const DATA_TYPES: [usize; 8] = [JSON, JSOBJ, FIGMA, SALT, URI, MD, HTML, TEXT];

/// A frame of the timeline as written: `tf` or `rf`, its number, what it holds and where its
/// data comes from.
pub(crate) struct AstFrame {
    pub(crate) reset: bool,
    pub(crate) name: Vec<u16>,
    pub(crate) entity_type: Vec<u16>,
    pub(crate) entity: Vec<u16>,
    pub(crate) sources: Vec<Vec<u16>>,
    pub(crate) data_reference: Option<Vec<u16>>,
    pub(crate) data_type: Option<Vec<u16>>,
    pub(crate) inline: Option<Vec<u16>>,
    /// Where the frame starts in the parsed text, for error messages.
    pub(crate) offset: usize,
}

pub(crate) struct AstData {
    pub(crate) name: Vec<u16>,
    pub(crate) block: Vec<u16>,
}

pub(crate) struct AstNote {
    pub(crate) frame: Vec<u16>,
    pub(crate) data_type: Option<Vec<u16>>,
    pub(crate) block: Vec<u16>,
}

/// A given/when/then specification: the entity named by each statement of each clause.
pub(crate) struct AstSpecification {
    pub(crate) frame: Vec<u16>,
    pub(crate) given: Vec<Vec<u16>>,
    pub(crate) when: Vec<Vec<u16>>,
    pub(crate) then: Vec<Vec<u16>>,
}

/// Pele's `EventModelAst`. The title tokens are kept whole, keyword included.
#[derive(Default)]
pub(crate) struct EventModelAst {
    pub(crate) title: Option<Vec<u16>>,
    pub(crate) acc_title: Option<Vec<u16>>,
    pub(crate) acc_descr: Option<Vec<u16>>,
    pub(crate) entities: Vec<Vec<u16>>,
    pub(crate) frames: Vec<AstFrame>,
    pub(crate) data: Vec<AstData>,
    pub(crate) notes: Vec<AstNote>,
    pub(crate) specifications: Vec<AstSpecification>,
}

struct Parser<'a> {
    r: Reader<'a>,
}

impl<'a> Parser<'a> {
    fn at(&self, kind: usize) -> bool {
        self.r.kind() == Some(kind)
    }

    fn one(&mut self, set: &[usize], expected: &str) -> Result<&'a [u16], SyntaxError> {
        match self.r.kind() {
            Some(kind) if set.contains(&kind) => Ok(self.r.take()),
            _ => Err(self.r.fail(expected)),
        }
    }

    fn qualified_name(&mut self) -> Result<Vec<u16>, SyntaxError> {
        let mut name = self.r.expect(ID)?.to_vec();
        while self.r.accept(DOT) {
            name.push(unit(b'.'));
            name.extend_from_slice(self.r.expect(ID)?);
        }
        Ok(name)
    }

    fn data_type(&mut self) -> Result<Option<Vec<u16>>, SyntaxError> {
        if !self.r.accept(TICK) {
            return Ok(None);
        }
        let kind = self.one(&DATA_TYPES, "a data type")?.to_vec();
        self.r.expect(TICK)?;
        Ok(Some(kind))
    }

    fn statements(&mut self) -> Result<Vec<Vec<u16>>, SyntaxError> {
        let mut out = Vec::new();
        loop {
            self.one(&ENTITY_TYPES, "an entity type")?;
            out.push(self.r.expect(ID)?.to_vec());
            if !self.r.kind().is_some_and(|kind| ENTITY_TYPES.contains(&kind)) {
                return Ok(out);
            }
        }
    }

    fn frame(&mut self, reset: bool) -> Result<AstFrame, SyntaxError> {
        let offset = self.r.start();
        self.r.skip();
        let name = self.r.expect(FRAME_ID)?.to_vec();
        let entity_type = self.one(&ENTITY_TYPES, "an entity type")?.to_vec();
        let entity = self.qualified_name()?;
        let mut sources = Vec::new();
        while self.r.accept(ARROW) {
            sources.push(self.r.expect(FRAME_ID)?.to_vec());
        }
        let mut data_reference = None;
        if self.r.accept(OPEN) {
            data_reference = Some(self.r.expect(ID)?.to_vec());
            self.r.expect(CLOSE)?;
        }
        let (mut data_type, mut inline) = (None, None);
        if self.at(TICK) || self.at(INLINE) {
            data_type = self.data_type()?;
            inline = Some(self.r.expect(INLINE)?.to_vec());
        }
        Ok(AstFrame { reset, name, entity_type, entity, sources, data_reference, data_type, inline, offset })
    }

    fn statement(&mut self, ast: &mut EventModelAst) -> Result<(), SyntaxError> {
        match self.r.kind() {
            Some(ACC_DESCR) => ast.acc_descr = Some(self.r.take().to_vec()),
            Some(ACC_TITLE_KIND) => ast.acc_title = Some(self.r.take().to_vec()),
            Some(TITLE) => ast.title = Some(self.r.take().to_vec()),
            Some(ENTITY) => {
                self.r.skip();
                let name = self.qualified_name()?;
                ast.entities.push(name);
            }
            Some(kind @ (TF | TIME_FRAME | RF | RESET_FRAME)) => {
                let frame = self.frame(matches!(kind, RF | RESET_FRAME))?;
                ast.frames.push(frame);
            }
            Some(DATA) => {
                self.r.skip();
                let name = self.r.expect(ID)?.to_vec();
                self.data_type()?;
                let block = self.r.expect(BLOCK)?.to_vec();
                ast.data.push(AstData { name, block });
            }
            Some(NOTE) => {
                self.r.skip();
                let frame = self.r.expect(FRAME_ID)?.to_vec();
                let data_type = self.data_type()?;
                let block = self.r.expect(BLOCK)?.to_vec();
                ast.notes.push(AstNote { frame, data_type, block });
            }
            Some(GWT) => {
                self.r.skip();
                let frame = self.r.expect(FRAME_ID)?.to_vec();
                self.r.expect(GIVEN)?;
                let given = self.statements()?;
                let when = if self.r.accept(WHEN) { self.statements()? } else { Vec::new() };
                self.r.expect(THEN)?;
                let then = self.statements()?;
                ast.specifications.push(AstSpecification { frame, given, when, then });
            }
            _ => return Err(self.r.fail("a frame, an entity, a data block, a note, or a specification")),
        }
        Ok(())
    }
}

/// Pele's `parseEventModel`.
pub(crate) fn parse_event_model(src: &[u16]) -> Result<EventModelAst, SyntaxError> {
    let mut parser = Parser { r: Reader::new(src, tokenize(src, &TOKENS)?, &TOKENS) };
    let mut ast = EventModelAst::default();
    parser.r.expect(KEYWORD)?;
    while parser.r.kind().is_some() {
        parser.statement(&mut ast)?;
    }
    Ok(ast)
}
