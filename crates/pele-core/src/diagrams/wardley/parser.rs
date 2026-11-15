//! Pele's `parseWardley` (`src/diagrams/wardley/parser.ts`): the token table of Mermaid's Langium
//! grammar for wardley maps, and the statements it reads into a syntax tree.

use crate::diagrams::tokens::{
    ACC_TITLE, DIRECTIVE, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, Scanner, SyntaxError, TITLE,
    TokenType, WHITESPACE, YAML, acc_descr_named, acc_descr_value, acc_title_value, hidden, string_value, title_value,
    token, tokenize,
};
use crate::input::{Units, decimal, is_blank, is_digit, is_letter, is_space, is_word};

const LBRACKET: usize = 0;
const COMMA: usize = 1;
const RBRACKET: usize = 2;
const AT: usize = 3;
const SLASH: usize = 4;
const LPAREN: usize = 5;
const RPAREN: usize = 6;
const MINUS: usize = 7;
const LBRACE: usize = 8;
const RBRACE: usize = 9;
const NUMBER: usize = 10;
const ARROW: usize = 11;
const LINK_PORT: usize = 12;
const LINK_ARROW: usize = 13;
const LINK_LABEL: usize = 14;
const STRATEGY: usize = 15;
const WARDLEY: usize = 16;
const SIZE: usize = 17;
const EVOLUTION: usize = 18;
const ANCHOR: usize = 19;
const COMPONENT: usize = 20;
const LABEL: usize = 21;
const INERTIA: usize = 22;
const EVOLVE: usize = 23;
const PIPELINE: usize = 24;
const NOTE: usize = 25;
const ANNOTATIONS: usize = 26;
const ANNOTATION: usize = 27;
const ACCELERATOR: usize = 28;
const DEACCELERATOR: usize = 29;
const NAME: usize = 30;
const ACC_DESCR_KIND: usize = 32;
const ACC_TITLE_KIND: usize = 33;
const TITLE_KIND: usize = 34;
const INT: usize = 35;
const STRING_KIND: usize = 36;
const ID: usize = 37;
const NEWLINE_KIND: usize = 38;
const YAML_KIND: usize = 40;

fn unit(c: u8) -> u16 {
    u16::from(c)
}

/// Answers as `WARDLEY_NUMBER`'s pattern `[0-9]+\.[0-9]+` does. The pattern reads a whole run of
/// digits before it finds there is no fraction, and would do so again from every digit of a long
/// run, so the run is remembered.
fn number_scanner() -> Scanner {
    let (mut from, mut end) = (0, 0);
    Box::new(move |src, p| {
        if p < from || p >= end {
            from = p;
            end = src.run(p, is_digit);
        }
        if end == p || src.at(end) != Some(unit(b'.')) {
            return None;
        }
        let stop = src.run(end + 1, is_digit);
        (stop > end + 1).then_some(stop)
    })
}

/// `\+<>|\+>|\+<`
fn link_port(src: &[u16], at: usize) -> Option<usize> {
    if src.at(at) != Some(unit(b'+')) {
        return None;
    }
    if src.starts_at(at + 1, "<>") {
        return Some(at + 3);
    }
    matches!(src.at(at + 1), Some(62 | 60)).then_some(at + 2)
}

/// `-->|-\.->|>|\+'[^']*'<>|\+'[^']*'<|\+'[^']*'>`
fn link_arrow(src: &[u16], at: usize) -> Option<usize> {
    if src.starts_at(at, "-->") {
        return Some(at + 3);
    }
    if src.starts_at(at, "-.->") {
        return Some(at + 4);
    }
    if src.at(at) == Some(unit(b'>')) {
        return Some(at + 1);
    }
    if !src.starts_at(at, "+'") {
        return None;
    }
    let close = src.find_unit(unit(b'\''), at + 2)?;
    if src.starts_at(close + 1, "<>") {
        return Some(close + 3);
    }
    matches!(src.at(close + 1), Some(60 | 62)).then_some(close + 2)
}

/// `;[^\n\r]+`
fn link_label(src: &[u16], at: usize) -> Option<usize> {
    if src.at(at) != Some(unit(b';')) {
        return None;
    }
    let end = src.run(at + 1, |c| c != 10 && c != 13);
    (end > at + 1).then_some(end)
}

/// `build|buy|outsource|market`
fn strategy(src: &[u16], at: usize) -> Option<usize> {
    ["build", "buy", "outsource", "market"].iter().find(|word| src.starts_at(at, word)).map(|word| at + word.len())
}

/// `[A-Za-z0-9_()&]|-(?!>)`: one unit of a name, given where it is.
fn name_unit(src: &[u16], at: usize) -> bool {
    match src.at(at) {
        Some(c) if is_letter(c) || is_digit(c) || matches!(c, 95 | 40 | 41 | 38) => true,
        Some(45) => src.at(at + 1) != Some(unit(b'>')),
        _ => false,
    }
}

fn name_run(src: &[u16], mut at: usize) -> usize {
    while name_unit(src, at) {
        at += 1;
    }
    at
}

/// `(?!title\s|accTitle|accDescr)[A-Za-z](?:[A-Za-z0-9_()&]|-(?!>))*(?:[ \t]+[A-Za-z(](?:[A-Za-z0-9_()&]|-(?!>))*)*`
fn name_with_spaces(src: &[u16], at: usize) -> Option<usize> {
    let titled = src.starts_at(at, "title") && src.at(at + 5).is_some_and(is_space);
    if titled || src.starts_at(at, "accTitle") || src.starts_at(at, "accDescr") {
        return None;
    }
    if !src.at(at).is_some_and(is_letter) {
        return None;
    }
    let mut end = name_run(src, at + 1);
    loop {
        let word = src.run(end, is_blank);
        if word == end || !src.at(word).is_some_and(|c| is_letter(c) || c == unit(b'(')) {
            return Some(end);
        }
        end = name_run(src, word + 1);
    }
}

/// `[ \t]+`
fn blanks(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, is_blank))
}

/// `0|[1-9][0-9]*(?!\.)`
fn int(src: &[u16], at: usize) -> Option<usize> {
    match src.at(at)? {
        48 => Some(at + 1),
        49..=57 => {
            let end = src.run(at, is_digit);
            if src.at(end) != Some(unit(b'.')) { Some(end) } else { (end - at >= 2).then_some(end - 1) }
        }
        _ => None,
    }
}

/// `[\w]([-\w]*\w)?`
fn id(src: &[u16], at: usize) -> Option<usize> {
    if !src.at(at).is_some_and(is_word) {
        return None;
    }
    let run = src.run(at + 1, |c| c == unit(b'-') || is_word(c));
    Some((at + 1..run).rev().find(|&i| is_word(src[i])).map_or(at + 1, |last| last + 1))
}

const fn literal(name: &'static str) -> TokenType {
    token(name, Matcher::Literal(name))
}

const fn word(name: &'static str, text: &'static str) -> TokenType {
    token(name, Matcher::Literal(text))
}

/// Pele's `WARDLEY_TOKENS`, in its order.
pub(crate) static WARDLEY_TOKENS: [TokenType; 43] = [
    literal("["),
    literal(","),
    literal("]"),
    literal("@"),
    literal("/"),
    literal("("),
    literal(")"),
    literal("-").longer(&[ARROW, LINK_ARROW, YAML_KIND]),
    literal("{"),
    literal("}"),
    token("WARDLEY_NUMBER", Matcher::Scanner(number_scanner)),
    token("ARROW", Matcher::Literal("->")),
    token("LINK_PORT", Matcher::Pattern(link_port)),
    token("LINK_ARROW", Matcher::Pattern(link_arrow)),
    token("LINK_LABEL", Matcher::Pattern(link_label)),
    token("STRATEGY", Matcher::Pattern(strategy)),
    word("KW_WARDLEY", "wardley-beta"),
    word("KW_SIZE", "size"),
    word("KW_EVOLUTION", "evolution"),
    word("KW_ANCHOR", "anchor"),
    word("KW_COMPONENT", "component"),
    word("KW_LABEL", "label"),
    word("KW_INERTIA", "inertia"),
    word("KW_EVOLVE", "evolve"),
    word("KW_PIPELINE", "pipeline"),
    word("KW_NOTE", "note"),
    word("KW_ANNOTATIONS", "annotations"),
    word("KW_ANNOTATION", "annotation"),
    word("KW_ACCELERATOR", "accelerator"),
    word("KW_DEACCELERATOR", "deaccelerator"),
    token("NAME_WITH_SPACES", Matcher::Pattern(name_with_spaces)),
    hidden("WS", Matcher::Pattern(blanks)),
    acc_descr_named("ACC_DESCR"),
    ACC_TITLE,
    TITLE,
    token("INT", Matcher::Pattern(int)),
    STRING,
    token("ID", Matcher::Pattern(id)),
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct LabelAst {
    pub(crate) neg_x: bool,
    pub(crate) offset_x: f64,
    pub(crate) neg_y: bool,
    pub(crate) offset_y: f64,
}

pub(crate) struct StageAst {
    pub(crate) name: Vec<u16>,
    pub(crate) boundary: Option<f64>,
    pub(crate) second_name: Option<Vec<u16>>,
}

pub(crate) struct ComponentAst {
    pub(crate) name: Vec<u16>,
    pub(crate) visibility: f64,
    pub(crate) evolution: f64,
    pub(crate) label: Option<LabelAst>,
    pub(crate) strategy: Option<Vec<u16>>,
    pub(crate) inertia: bool,
}

pub(crate) struct PipelineComponentAst {
    pub(crate) name: Vec<u16>,
    pub(crate) evolution: f64,
    pub(crate) label: Option<LabelAst>,
}

pub(crate) struct LinkAst {
    pub(crate) from: Vec<u16>,
    pub(crate) from_port: Option<Vec<u16>>,
    pub(crate) arrow: Option<Vec<u16>>,
    pub(crate) to: Vec<u16>,
    pub(crate) to_port: Option<Vec<u16>>,
    pub(crate) link_label: Option<Vec<u16>>,
}

/// Something with a name at a place, as anchors, notes and forces are written.
pub(crate) struct PlacedAst {
    pub(crate) name: Vec<u16>,
    pub(crate) first: f64,
    pub(crate) second: f64,
}

pub(crate) struct PipelineAst {
    pub(crate) parent: Vec<u16>,
    pub(crate) components: Vec<PipelineComponentAst>,
}

pub(crate) struct AnnotationAst {
    pub(crate) number: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) text: Vec<u16>,
}

#[derive(Default)]
pub(crate) struct WardleyAst {
    pub(crate) title: Option<Vec<u16>>,
    pub(crate) acc_title: Option<Vec<u16>>,
    pub(crate) acc_descr: Option<Vec<u16>>,
    pub(crate) size: Option<(f64, f64)>,
    pub(crate) evolution: Option<Vec<StageAst>>,
    /// Name, visibility, evolution.
    pub(crate) anchors: Vec<PlacedAst>,
    pub(crate) components: Vec<ComponentAst>,
    pub(crate) links: Vec<LinkAst>,
    /// Component, target.
    pub(crate) evolves: Vec<(Vec<u16>, f64)>,
    pub(crate) pipelines: Vec<PipelineAst>,
    /// Text, visibility, evolution.
    pub(crate) notes: Vec<PlacedAst>,
    /// x, y.
    pub(crate) annotations: Vec<(f64, f64)>,
    pub(crate) annotation: Vec<AnnotationAst>,
    /// Name, x, y.
    pub(crate) accelerators: Vec<PlacedAst>,
    pub(crate) deaccelerators: Vec<PlacedAst>,
}

/// `parseInt` of an `INT` token, whose text is a decimal literal.
fn int_value(text: &[u16]) -> f64 {
    decimal(text)
}

struct Parser<'a> {
    r: Reader<'a>,
}

impl Parser<'_> {
    fn at(&self, kind: usize) -> bool {
        self.r.kind() == Some(kind)
    }

    fn is_name(&self) -> bool {
        self.at(STRING_KIND) || self.at(ID) || self.at(NAME)
    }

    fn name(&mut self) -> Result<Vec<u16>, SyntaxError> {
        if self.at(STRING_KIND) {
            return Ok(string_value(self.r.take()));
        }
        if self.at(ID) || self.at(NAME) {
            return Ok(self.r.take().to_vec());
        }
        Err(self.r.fail("a name"))
    }

    fn number(&mut self) -> Result<f64, SyntaxError> {
        Ok(decimal(self.r.expect(NUMBER)?))
    }

    fn int(&mut self) -> Result<f64, SyntaxError> {
        Ok(int_value(self.r.expect(INT)?))
    }

    fn coordinate(&mut self) -> Result<f64, SyntaxError> {
        if self.at(NUMBER) {
            return Ok(decimal(self.r.take()));
        }
        self.int()
    }

    fn end_of_line(&mut self) -> Result<(), SyntaxError> {
        if self.r.kind().is_none() {
            return Ok(());
        }
        if !self.at(NEWLINE_KIND) {
            return Err(self.r.fail("a line break or the end of input"));
        }
        while self.r.accept(NEWLINE_KIND) {}
        Ok(())
    }

    fn pair(&mut self, read: fn(&mut Self) -> Result<f64, SyntaxError>) -> Result<(f64, f64), SyntaxError> {
        self.r.expect(LBRACKET)?;
        let first = read(self)?;
        self.r.expect(COMMA)?;
        let second = read(self)?;
        self.r.expect(RBRACKET)?;
        Ok((first, second))
    }

    fn label(&mut self) -> Result<LabelAst, SyntaxError> {
        self.r.expect(LABEL)?;
        self.r.expect(LBRACKET)?;
        let neg_x = self.r.accept(MINUS);
        let offset_x = self.int()?;
        self.r.expect(COMMA)?;
        let neg_y = self.r.accept(MINUS);
        let offset_y = self.int()?;
        self.r.expect(RBRACKET)?;
        Ok(LabelAst { neg_x, offset_x, neg_y, offset_y })
    }

    fn stage(&mut self) -> Result<StageAst, SyntaxError> {
        let name = self.name()?;
        let boundary = if self.r.accept(AT) { Some(self.number()?) } else { None };
        let second_name = if self.r.accept(SLASH) { Some(self.name()?) } else { None };
        Ok(StageAst { name, boundary, second_name })
    }

    fn statement(&mut self, kind: usize, ast: &mut WardleyAst) -> Result<(), SyntaxError> {
        match kind {
            NEWLINE_KIND => self.r.skip(),
            ACC_DESCR_KIND => {
                ast.acc_descr = Some(acc_descr_value(self.r.take()));
                self.end_of_line()?;
            }
            ACC_TITLE_KIND => {
                ast.acc_title = Some(acc_title_value(self.r.take()));
                self.end_of_line()?;
            }
            TITLE_KIND => {
                ast.title = Some(title_value(self.r.take()));
                self.end_of_line()?;
            }
            SIZE => {
                self.r.skip();
                let size = self.pair(Self::int)?;
                self.end_of_line()?;
                ast.size = Some(size);
            }
            EVOLUTION => {
                self.r.skip();
                let mut stages = vec![self.stage()?];
                self.r.expect(ARROW)?;
                stages.push(self.stage()?);
                while self.r.accept(ARROW) {
                    stages.push(self.stage()?);
                }
                self.end_of_line()?;
                ast.evolution = Some(stages);
            }
            ANCHOR => {
                self.r.skip();
                let name = self.name()?;
                let (first, second) = self.pair(Self::number)?;
                self.end_of_line()?;
                ast.anchors.push(PlacedAst { name, first, second });
            }
            COMPONENT => {
                self.r.skip();
                let name = self.name()?;
                let (visibility, evolution) = self.pair(Self::number)?;
                let label = if self.at(LABEL) { Some(self.label()?) } else { None };
                let mut strategy = None;
                if self.at(LPAREN) && self.r.peek(1) == Some(STRATEGY) {
                    self.r.skip();
                    strategy = Some(self.r.take().to_vec());
                    self.r.expect(RPAREN)?;
                }
                let mut inertia = false;
                if self.r.accept(INERTIA) {
                    inertia = true;
                } else if self.r.accept(LPAREN) {
                    self.r.expect(INERTIA)?;
                    self.r.expect(RPAREN)?;
                    inertia = true;
                }
                self.end_of_line()?;
                ast.components.push(ComponentAst { name, visibility, evolution, label, strategy, inertia });
            }
            EVOLVE => {
                self.r.skip();
                let component = self.name()?;
                let target = self.number()?;
                self.end_of_line()?;
                ast.evolves.push((component, target));
            }
            PIPELINE => {
                self.r.skip();
                let parent = self.name()?;
                self.r.expect(LBRACE)?;
                self.r.expect(NEWLINE_KIND)?;
                while self.r.accept(NEWLINE_KIND) {}
                let mut components = Vec::new();
                loop {
                    self.r.expect(COMPONENT)?;
                    let name = self.name()?;
                    self.r.expect(LBRACKET)?;
                    let evolution = self.number()?;
                    self.r.expect(RBRACKET)?;
                    let label = if self.at(LABEL) { Some(self.label()?) } else { None };
                    self.end_of_line()?;
                    components.push(PipelineComponentAst { name, evolution, label });
                    if !self.at(COMPONENT) {
                        break;
                    }
                }
                self.r.expect(RBRACE)?;
                self.end_of_line()?;
                ast.pipelines.push(PipelineAst { parent, components });
            }
            NOTE => {
                self.r.skip();
                let name = string_value(self.r.expect(STRING_KIND)?);
                let (first, second) = self.pair(Self::number)?;
                self.end_of_line()?;
                ast.notes.push(PlacedAst { name, first, second });
            }
            ANNOTATIONS => {
                self.r.skip();
                let at = self.pair(Self::coordinate)?;
                self.end_of_line()?;
                ast.annotations.push(at);
            }
            ANNOTATION => {
                self.r.skip();
                let number = self.int()?;
                self.r.expect(COMMA)?;
                let (x, y) = self.pair(Self::coordinate)?;
                let text = string_value(self.r.expect(STRING_KIND)?);
                self.end_of_line()?;
                ast.annotation.push(AnnotationAst { number, x, y, text });
            }
            ACCELERATOR | DEACCELERATOR => {
                self.r.skip();
                let name = self.name()?;
                let (first, second) = self.pair(Self::number)?;
                self.end_of_line()?;
                let force = PlacedAst { name, first, second };
                if kind == DEACCELERATOR {
                    ast.deaccelerators.push(force);
                } else {
                    ast.accelerators.push(force);
                }
            }
            _ => {
                if !self.is_name() {
                    return Err(self.r.fail("a statement"));
                }
                let from = self.name()?;
                let from_port = if self.at(LINK_PORT) { Some(self.r.take().to_vec()) } else { None };
                let arrow = if self.at(LINK_ARROW) || self.at(ARROW) { Some(self.r.take().to_vec()) } else { None };
                let to = self.name()?;
                let to_port = if self.at(LINK_PORT) { Some(self.r.take().to_vec()) } else { None };
                let link_label = if self.at(LINK_LABEL) { Some(self.r.take()[1..].trim_js().to_vec()) } else { None };
                self.end_of_line()?;
                ast.links.push(LinkAst { from, from_port, arrow, to, to_port, link_label });
            }
        }
        Ok(())
    }
}

/// Pele's `parseWardley`.
pub(crate) fn parse_wardley(src: &[u16]) -> Result<WardleyAst, SyntaxError> {
    let mut p = Parser { r: Reader::new(src, tokenize(src, &WARDLEY_TOKENS)?, &WARDLEY_TOKENS) };
    let mut ast = WardleyAst::default();
    while p.r.accept(NEWLINE_KIND) {}
    p.r.expect(WARDLEY)?;
    while let Some(kind) = p.r.kind() {
        p.statement(kind, &mut ast)?;
    }
    Ok(ast)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn hand_written_patterns_answer_as_their_expressions() {
        let end = |scan: fn(&[u16], usize) -> Option<usize>, text: &str| scan(&units(text), 0);
        assert_eq!(end(int, "0123"), Some(1));
        assert_eq!(end(int, "12.5"), Some(1));
        assert_eq!(end(int, "1.5"), None);
        assert_eq!(end(int, "123"), Some(3));
        assert_eq!(end(id, "a--b-"), Some(4));
        assert_eq!(end(id, "_-"), Some(1));
        assert_eq!(end(name_with_spaces, "Tea  cup -> x"), Some(8));
        assert_eq!(end(name_with_spaces, "a-> b"), Some(1));
        assert_eq!(end(name_with_spaces, "title x"), None);
        assert_eq!(end(name_with_spaces, "titles x"), Some(8));
        assert_eq!(end(name_with_spaces, "a  1"), Some(1));
        assert_eq!(end(link_arrow, "+'a'<>b"), Some(6));
        assert_eq!(end(link_arrow, "+'a'x"), None);
    }
}
