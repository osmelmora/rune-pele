//! The quadrant chart parser and db, ported from Pele's `src/diagrams/quadrant/parser.ts` and
//! `db.ts`.

use super::lexer::{T, Token, tokenize};
use super::{PointStyle, QuadrantModel, QuadrantPoint, QuadrantText};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units, decimal, is_digit, is_hex, string_to_number};

/// Why a source is no quadrant chart: Pele's syntax errors, and the semantic errors its db throws.
pub(super) enum Failure {
    Syntax(SyntaxError),
    Semantic(String),
}

/// The tokens that continue a text.
fn word(t: T) -> bool {
    matches!(
        t,
        T::Punctuation
            | T::Amp
            | T::Num
            | T::Alpha
            | T::Comma
            | T::Plus
            | T::Equals
            | T::Mult
            | T::Dot
            | T::Brkt
            | T::Underscore
            | T::UnicodeText
    )
}

/// The tokens of a class's name.
fn id(t: T) -> bool {
    matches!(t, T::Alpha | T::Num | T::Minus | T::Comma | T::Colon | T::Amp | T::Brkt | T::Mult | T::UnicodeText)
}

/// The tokens of a style declaration.
fn style(t: T) -> bool {
    matches!(t, T::Alpha | T::Num | T::Colon | T::Space | T::Brkt | T::Minus)
}

fn eol(t: T) -> bool {
    matches!(t, T::Newline | T::Semi | T::Eof)
}

fn is_text(t: T) -> bool {
    t == T::Str || t == T::MdStr || word(t)
}

const TEXT_TOKENS: &str = "'STR', 'MD_STR', 'ALPHA', 'NUM', 'UNICODE_TEXT', 'PUNCTUATION'";

fn invalid(style: &[u16], value: &[u16], kind: &str) -> Failure {
    Failure::Semantic(format!(
        "value for {} {} is invalid, please use a valid {kind}",
        String::from_utf16_lossy(style),
        String::from_utf16_lossy(value)
    ))
}

/// The first two parts of Mermaid's `style.trim().split(/\s*:\s*/)`.
fn key_value(style: &[u16]) -> (&[u16], &[u16]) {
    const UNDEFINED: &[u16] = &[117, 110, 100, 101, 102, 105, 110, 101, 100];
    let s = style.trim_js();
    let Some(colon) = s.find_unit(58, 0) else {
        return (s, UNDEFINED);
    };
    let rest = s[colon + 1..].trim_start_js();
    let value = match rest.find_unit(58, 0) {
        None => rest,
        Some(next) => rest[..next].trim_end_js(),
    };
    (s[..colon].trim_end_js(), value)
}

fn is_number(value: &[u16]) -> bool {
    !value.is_empty() && value.iter().all(|&c| is_digit(c))
}

/// `/^#?([\dA-Fa-f]{6}|[\dA-Fa-f]{3})$/`
fn is_hex_code(value: &[u16]) -> bool {
    let digits = value.strip_prefix(&[35]).unwrap_or(value);
    matches!(digits.len(), 3 | 6) && digits.iter().all(|&c| is_hex(c))
}

/// `/^\d+px$/`
fn is_pixels(value: &[u16]) -> bool {
    value.strip_suffix(&[112, 120]).is_some_and(is_number)
}

/// Pele's `parseStyles`.
pub(super) fn parse_styles(styles: &[JsText]) -> Result<PointStyle, Failure> {
    let mut out = PointStyle::default();
    for style in styles {
        let (key, value) = key_value(style);
        if key.is_str("radius") {
            if !is_number(value) {
                return Err(invalid(key, value, "number"));
            }
            out.radius = Some(decimal(value));
        } else if key.is_str("color") {
            if !is_hex_code(value) {
                return Err(invalid(key, value, "hex code"));
            }
            out.color = Some(value.into());
        } else if key.is_str("stroke-color") {
            if !is_hex_code(value) {
                return Err(invalid(key, value, "hex code"));
            }
            out.stroke_color = Some(value.into());
        } else if key.is_str("stroke-width") {
            if !is_pixels(value) {
                return Err(invalid(key, value, "number of pixels (eg. 10px)"));
            }
            out.stroke_width = Some(value.into());
        } else {
            return Err(Failure::Semantic(format!("style named {} is not supported.", String::from_utf16_lossy(key))));
        }
    }
    Ok(out)
}

fn trimmed(label: QuadrantText) -> QuadrantText {
    QuadrantText { text: label.text.trim_js().into(), markdown: label.markdown }
}

struct Parser<'a> {
    src: &'a [u16],
    tokens: Vec<Token>,
    i: usize,
}

impl<'a> Parser<'a> {
    fn kind(&self) -> T {
        self.tokens[self.i].kind
    }

    fn text_at(&self, i: usize) -> &'a [u16] {
        let token = self.tokens[i];
        &self.src[token.start..token.end]
    }

    fn take(&mut self) -> &'a [u16] {
        self.i += 1;
        self.text_at(self.i - 1)
    }

    fn fail<X>(&self, expected: &str) -> Result<X, Failure> {
        let token = self.tokens[self.i];
        Err(Failure::Syntax(if token.kind == T::Error {
            SyntaxError { at: token.start, detail: String::new(), lexical: true }
        } else {
            SyntaxError {
                at: token.start,
                detail: format!("Expecting {expected}, got '{}'", token.kind.name()),
                lexical: false,
            }
        }))
    }

    fn expect(&mut self, wanted: T) -> Result<&'a [u16], Failure> {
        if self.kind() != wanted {
            return self.fail(&format!("'{}'", wanted.name()));
        }
        Ok(self.take())
    }

    fn text(&mut self) -> Result<QuadrantText, Failure> {
        let first = self.kind();
        if !is_text(first) {
            return self.fail(TEXT_TOKENS);
        }
        let mut s = JsText::from(self.take());
        while word(self.kind()) || matches!(self.kind(), T::Space | T::Minus) {
            s.push_units(self.take());
        }
        Ok(QuadrantText { text: s, markdown: first == T::MdStr })
    }

    fn styles(&mut self) -> Result<Vec<JsText>, Failure> {
        let mut out = Vec::new();
        loop {
            if !style(self.kind()) {
                return self.fail("'ALPHA', 'NUM', 'COLON', 'SPACE', 'BRKT', 'MINUS'");
            }
            let mut s = JsText::from(self.take());
            while style(self.kind()) {
                s.push_units(self.take());
            }
            out.push(s.trim_js().into());
            if self.kind() != T::Comma {
                return Ok(out);
            }
            self.i += 1;
        }
    }
}

/// `text.replace(/\n\s+/g, '\n')`
fn unindent(text: &[u16]) -> JsText {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        let spaces = text.run(at + 1, crate::input::is_space);
        at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
    }
    out.into()
}

/// Pele's `parseQuadrant`.
pub(super) fn parse(src: &[u16], db: &mut QuadrantModel) -> Result<(), Failure> {
    let mut p = Parser { src, tokens: tokenize(src), i: 0 };
    while eol(p.kind()) || p.kind() == T::Space {
        p.i += 1;
    }
    p.expect(T::Quadrant)?;

    while p.kind() != T::End {
        while p.kind() == T::Space {
            p.i += 1;
        }
        let t = p.kind();
        match t {
            T::ClassDef => {
                p.i += 1;
                p.expect(T::Space)?;
                if !id(p.kind()) {
                    return p.fail("'ALPHA', 'NUM', 'UNICODE_TEXT'");
                }
                let mut name = JsText::from(p.take());
                while id(p.kind()) {
                    name.push_units(p.take());
                }
                p.expect(T::Space)?;
                let styles = p.styles()?;
                db.classes.insert(name, parse_styles(&styles)?);
            }
            T::XAxis | T::YAxis => {
                p.i += 1;
                let mut from = p.text()?;
                let mut to = None;
                if p.kind() == T::Delimiter {
                    p.i += 1;
                    if is_text(p.kind()) {
                        to = Some(p.text()?);
                    } else {
                        from.text.push_str(" \u{27f6} ");
                    }
                }
                let (near, far) = if t == T::XAxis {
                    (&mut db.x_axis_left, &mut db.x_axis_right)
                } else {
                    (&mut db.y_axis_bottom, &mut db.y_axis_top)
                };
                *near = Some(trimmed(from));
                if let Some(to) = to {
                    *far = Some(trimmed(to));
                }
            }
            T::Quadrant1 | T::Quadrant2 | T::Quadrant3 | T::Quadrant4 => {
                p.i += 1;
                let quadrant = [T::Quadrant1, T::Quadrant2, T::Quadrant3, T::Quadrant4]
                    .iter()
                    .position(|&q| q == t)
                    .expect("one of the four");
                db.quadrants[quadrant] = Some(trimmed(p.text()?));
            }
            T::Title => {
                p.i += 1;
                db.title = Some(p.expect(T::TitleValue)?.trim_js().into());
            }
            T::AccTitle => {
                p.i += 1;
                db.acc_title = Some(p.expect(T::AccTitleValue)?.trim_js().trim_start_js().into());
            }
            T::AccDescr => {
                p.i += 1;
                db.acc_descr = Some(unindent(p.expect(T::AccDescrValue)?.trim_js()));
            }
            T::AccDescrMultilineValue => {
                db.acc_descr = Some(unindent(p.take().trim_js()));
            }
            _ if is_text(t) => {
                let label = p.text()?;
                let class_name: JsText = if p.kind() == T::ClassName { p.take().into() } else { JsText::default() };
                p.expect(T::PointStart)?;
                let x = p.expect(T::PointX)?;
                let y = p.expect(T::PointY)?;
                let styles = if style(p.kind()) { p.styles()? } else { Vec::new() };
                let style = parse_styles(&styles)?;
                db.points.push(QuadrantPoint {
                    x: string_to_number(x),
                    y: string_to_number(y),
                    text: trimmed(label),
                    class_name,
                    style,
                });
            }
            _ => {}
        }
        if !eol(p.kind()) {
            return p.fail("'NEWLINE', 'SEMI', 'EOF'");
        }
        p.i += 1;
    }
    Ok(())
}
