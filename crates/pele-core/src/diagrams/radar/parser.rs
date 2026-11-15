//! The radar tokenizer and parser, ported from Pele's `radar/parser.ts`.

use crate::diagrams::tokens::{
    ACC_TITLE, DIRECTIVE, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, Scanner, SyntaxError, TokenType,
    WHITESPACE, YAML, acc_descr, acc_descr_value, acc_title_value, dashed_name, keyword, string_value, title_before,
    title_value, token, tokenize,
};
use crate::input::{Units, decimal, is_digit, is_space, is_word};

const fn unit(c: u8) -> u16 {
    c as u16
}

const RADAR_COLON: usize = 0;
const RADAR: usize = 1;
const SHOW_LEGEND: usize = 2;
const GRATICULE: usize = 3;
const CURVE: usize = 4;
const TICKS: usize = 5;
const AXIS: usize = 6;
const MAX: usize = 7;
const MIN: usize = 8;
const COLON: usize = 9;
const COMMA: usize = 10;
const OPEN: usize = 11;
const CLOSE: usize = 12;
const OPEN_BRACE: usize = 13;
const CLOSE_BRACE: usize = 14;
const GRATICULE_VALUE: usize = 15;
const BOOLEAN: usize = 16;
const ACC_DESCR: usize = 17;
const ACC_TITLE_KIND: usize = 18;
const TITLE: usize = 19;
const NUMBER: usize = 20;
const STRING_KIND: usize = 21;
const ID: usize = 22;
const NEWLINE_KIND: usize = 23;

fn is_dash_or_word(c: u16) -> bool {
    c == unit(b'-') || is_word(c)
}

/// `[\w]([-\w]*\w)?`
fn id(src: &[u16], at: usize) -> Option<usize> {
    if !src.at(at).is_some_and(is_word) {
        return None;
    }
    let run = src.run(at + 1, is_dash_or_word);
    Some((at + 1..run).rev().find(|&i| is_word(src[i])).map_or(at + 1, |last| last + 1))
}

/// Pele's `optionValue`: `pattern`, giving way to a longer name except right after its option,
/// as in `graticule polygonaxis`.
fn option_value(src: &[u16], at: usize, previous: Option<usize>, words: [&str; 2], option: usize) -> Option<usize> {
    let word = words.iter().find(|word| src.starts_at(at, word))?;
    let end = at + word.len();
    if previous == Some(option) || !dashed_name(src, end) {
        return Some(end);
    }
    None
}

/// `circle|polygon`, after `graticule`.
fn graticule(src: &[u16], at: usize, previous: Option<usize>) -> Option<usize> {
    option_value(src, at, previous, ["circle", "polygon"], GRATICULE)
}

/// `true|false`, after `showLegend`.
fn boolean(src: &[u16], at: usize, previous: Option<usize>) -> Option<usize> {
    option_value(src, at, previous, ["true", "false"], SHOW_LEGEND)
}

/// Pele's `matchNumber`, for `(?:[0-9]+\.[0-9]+(?!\.))|(?:0|[1-9][0-9]*(?!\.))` without its
/// backtracking: the run of digits a position is in is read once.
fn number_scanner() -> Scanner {
    let (mut from, mut end) = (0, 0);
    Box::new(move |src, at| {
        if at < from || at >= end {
            from = at;
            end = src.run(at, is_digit);
        }
        let whole = end;
        if whole == at {
            return None;
        }
        let dot = src.at(whole) == Some(unit(b'.'));
        if dot {
            let fraction = src.run(whole + 1, is_digit);
            let length = fraction - whole - 1;
            if length > 0 && src.at(fraction) != Some(unit(b'.')) {
                return Some(fraction);
            }
            if length > 1 {
                return Some(fraction - 1);
            }
        }
        if src.at(at) == Some(unit(b'0')) {
            return Some(at + 1);
        }
        if !dot {
            return Some(whole);
        }
        (whole - 1 > at).then_some(whole - 1)
    })
}

/// Pele's `matchAccDescr`: `ACC_DESCR`, except that a braced description after the last closing
/// brace of the text fails at once, as `accDescr` is also a name here and `{` a token.
fn acc_descr_scanner() -> Scanner {
    let mut last_brace: Option<Option<usize>> = None;
    Box::new(move |src, at| {
        let word = src.run(at, |c| c == 9 || c == 32);
        if src.starts_at(word, "accDescr") {
            let brace = src.run(word + 8, is_space);
            if src.at(brace) == Some(unit(b'{')) {
                let last = *last_brace.get_or_insert_with(|| src.iter().rposition(|&c| c == unit(b'}')));
                if last.is_none_or(|last| brace + 1 > last) {
                    return None;
                }
            }
        }
        acc_descr(src, at)
    })
}

const fn word(text: &'static str) -> TokenType {
    token(text, Matcher::Literal(text)).longer(&[ID])
}

const fn mark(text: &'static str) -> TokenType {
    token(text, Matcher::Literal(text))
}

static TOKENS: [TokenType; 28] = [
    mark("radar-beta:"),
    keyword("radar-beta").longer(&[ID]),
    word("showLegend"),
    word("graticule"),
    word("curve"),
    word("ticks"),
    word("axis"),
    word("max"),
    word("min"),
    mark(":"),
    mark(","),
    mark("["),
    mark("]"),
    mark("{"),
    mark("}"),
    token("GRATICULE", Matcher::After(graticule)),
    token("BOOLEAN", Matcher::After(boolean)),
    token("ACC_DESCR", Matcher::Scanner(acc_descr_scanner)),
    ACC_TITLE,
    title_before(dashed_name),
    token("NUMBER", Matcher::Scanner(number_scanner)),
    STRING,
    token("ID", Matcher::Pattern(id)),
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

pub(crate) struct AstAxis {
    pub(crate) name: Vec<u16>,
    pub(crate) label: Option<Vec<u16>>,
}

/// An entry of a curve: a value alone, or a value for the axis it names.
pub(crate) struct AstEntry {
    pub(crate) axis: Option<Vec<u16>>,
    pub(crate) value: f64,
}

pub(crate) struct AstCurve {
    pub(crate) name: Vec<u16>,
    pub(crate) label: Option<Vec<u16>>,
    pub(crate) entries: Vec<AstEntry>,
}

pub(crate) enum OptionValue {
    Flag(bool),
    Number(f64),
    Graticule(Vec<u16>),
}

pub(crate) struct AstOption {
    /// The option's keyword.
    pub(crate) name: usize,
    pub(crate) value: OptionValue,
}

#[derive(Default)]
pub(crate) struct RadarAst {
    pub(crate) title: Option<Vec<u16>>,
    pub(crate) acc_title: Option<Vec<u16>>,
    pub(crate) acc_descr: Option<Vec<u16>>,
    pub(crate) axes: Vec<AstAxis>,
    pub(crate) curves: Vec<AstCurve>,
    pub(crate) options: Vec<AstOption>,
}

pub(crate) const OPTION_SHOW_LEGEND: usize = SHOW_LEGEND;
pub(crate) const OPTION_GRATICULE: usize = GRATICULE;
pub(crate) const OPTION_TICKS: usize = TICKS;
pub(crate) const OPTION_MAX: usize = MAX;
pub(crate) const OPTION_MIN: usize = MIN;

struct Parser<'a> {
    r: Reader<'a>,
}

impl Parser<'_> {
    fn newlines(&mut self) {
        while self.r.accept(NEWLINE_KIND) {}
    }

    fn end_of_line(&mut self) -> Result<(), SyntaxError> {
        if self.r.kind().is_none() {
            return Ok(());
        }
        if self.r.kind() != Some(NEWLINE_KIND) {
            return Err(self.r.fail("a line break or the end of input"));
        }
        self.newlines();
        Ok(())
    }

    fn label(&mut self) -> Result<Option<Vec<u16>>, SyntaxError> {
        if !self.r.accept(OPEN) {
            return Ok(None);
        }
        let text = string_value(self.r.expect(STRING_KIND)?);
        self.r.expect(CLOSE)?;
        Ok(Some(text))
    }

    fn number(&mut self) -> Result<f64, SyntaxError> {
        Ok(decimal(self.r.expect(NUMBER)?))
    }

    fn entry(&mut self, detailed: bool) -> Result<AstEntry, SyntaxError> {
        if !detailed {
            return Ok(AstEntry { axis: None, value: self.number()? });
        }
        let axis = self.r.expect(ID)?.to_vec();
        self.r.accept(COLON);
        Ok(AstEntry { axis: Some(axis), value: self.number()? })
    }

    fn curve(&mut self) -> Result<AstCurve, SyntaxError> {
        let name = self.r.expect(ID)?.to_vec();
        let label = self.label()?;
        self.r.expect(OPEN_BRACE)?;
        self.newlines();
        if self.r.kind() != Some(NUMBER) && self.r.kind() != Some(ID) {
            return Err(self.r.fail("a number or an axis name"));
        }
        let detailed = self.r.kind() == Some(ID);
        let mut entries = vec![self.entry(detailed)?];
        while self.r.accept(COMMA) {
            self.newlines();
            entries.push(self.entry(detailed)?);
        }
        self.newlines();
        self.r.expect(CLOSE_BRACE)?;
        Ok(AstCurve { name, label, entries })
    }

    fn axis(&mut self) -> Result<AstAxis, SyntaxError> {
        let name = self.r.expect(ID)?.to_vec();
        Ok(AstAxis { name, label: self.label()? })
    }

    fn is_option(&self) -> bool {
        matches!(self.r.kind(), Some(SHOW_LEGEND | TICKS | MAX | MIN | GRATICULE))
    }

    fn option(&mut self) -> Result<AstOption, SyntaxError> {
        let name = self.r.kind().expect("an option");
        self.r.skip();
        let value = match name {
            SHOW_LEGEND => OptionValue::Flag(self.r.expect(BOOLEAN)?.is_str("true")),
            GRATICULE => OptionValue::Graticule(self.r.expect(GRATICULE_VALUE)?.to_vec()),
            _ => OptionValue::Number(self.number()?),
        };
        Ok(AstOption { name, value })
    }
}

/// Pele's `parseRadar`.
pub(crate) fn parse_radar(src: &[u16]) -> Result<RadarAst, SyntaxError> {
    let mut p = Parser { r: Reader::new(src, tokenize(src, &TOKENS)?, &TOKENS) };
    let mut ast = RadarAst::default();
    p.newlines();
    if !p.r.accept(RADAR_COLON) {
        p.r.expect(RADAR)?;
        p.r.accept(COLON);
    }
    p.newlines();
    while let Some(kind) = p.r.kind() {
        match kind {
            NEWLINE_KIND => p.r.skip(),
            ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(p.r.take()));
                p.end_of_line()?;
            }
            ACC_TITLE_KIND => {
                ast.acc_title = Some(acc_title_value(p.r.take()));
                p.end_of_line()?;
            }
            TITLE => {
                ast.title = Some(title_value(p.r.take()));
                p.end_of_line()?;
            }
            AXIS => {
                p.r.skip();
                loop {
                    let axis = p.axis()?;
                    ast.axes.push(axis);
                    if !p.r.accept(COMMA) {
                        break;
                    }
                }
            }
            CURVE => {
                p.r.skip();
                loop {
                    let curve = p.curve()?;
                    ast.curves.push(curve);
                    if !p.r.accept(COMMA) {
                        break;
                    }
                }
            }
            _ => {
                if !p.is_option() {
                    return Err(p.r.fail("an axis, a curve, an option, a title, or a line break"));
                }
                let option = p.option()?;
                ast.options.push(option);
                while p.r.accept(COMMA) {
                    if !p.is_option() {
                        return Err(p.r.fail("an option"));
                    }
                    let option = p.option()?;
                    ast.options.push(option);
                }
            }
        }
    }
    Ok(ast)
}
