//! The requirement parser, ported from Pele's `src/diagrams/requirement/parser.ts`: a hand-written
//! reading of Mermaid's `requirementDiagram.jison` that fails with Jison's wording.

use super::db::{ElementFields, RequirementDb, RequirementFields};
use super::lexer::{T, Tokens, tokenize};
use super::{Relationship, RequirementType, Risk, VerifyMethod};
use crate::RenderError;
use crate::catalog::ParseContext;
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units};
use crate::layout::Dir;

fn requirement_type(t: T) -> RequirementType {
    match t {
        T::Requirement => RequirementType::Requirement,
        T::FunctionalRequirement => RequirementType::Functional,
        T::InterfaceRequirement => RequirementType::Interface,
        T::PerformanceRequirement => RequirementType::Performance,
        T::PhysicalRequirement => RequirementType::Physical,
        _ => RequirementType::DesignConstraint,
    }
}

fn risk(t: T) -> Risk {
    match t {
        T::LowRisk => Risk::Low,
        T::MedRisk => Risk::Medium,
        _ => Risk::High,
    }
}

fn verify_method(t: T) -> VerifyMethod {
    match t {
        T::VerifyAnalysis => VerifyMethod::Analysis,
        T::VerifyDemonstration => VerifyMethod::Demonstration,
        T::VerifyInspection => VerifyMethod::Inspection,
        _ => VerifyMethod::Test,
    }
}

fn relationship(t: T) -> Relationship {
    match t {
        T::Contains => Relationship::Contains,
        T::Copies => Relationship::Copies,
        T::Derives => Relationship::Derives,
        T::Satisfies => Relationship::Satisfies,
        T::Verifies => Relationship::Verifies,
        T::Refines => Relationship::Refines,
        _ => Relationship::Traces,
    }
}

fn is_style_part(t: T) -> bool {
    matches!(t, T::Alpha | T::Colon | T::Brkt | T::Minus | T::Semicolon)
}

/// What a body field holds: a string, or for `risk` and `verifymethod` a keyword.
enum Value {
    Text(JsText),
    Risk(Risk),
    Verify(VerifyMethod),
}

struct Parser<'a> {
    src: &'a [u16],
    tokens: Tokens,
    i: usize,
}

impl Parser<'_> {
    fn kind(&self) -> T {
        self.tokens.types[self.i]
    }

    fn text(&self) -> JsText {
        self.tokens.text(self.src, self.i).into()
    }

    /// Fails with Jison's wording, naming the token types `from` to `to` and `others` as expected.
    fn fail(&self, from: T, to: T, others: &[T]) -> SyntaxError {
        let at = self.tokens.starts[self.i];
        let t = self.kind();
        if t == T::Error {
            return SyntaxError { at, detail: String::new(), lexical: true };
        }
        let expected: Vec<String> =
            from.through(to).iter().chain(others).map(|kind| format!("'{}'", kind.name())).collect();
        let detail = format!("Expecting {}, got '{}'", expected.join(", "), t.name());
        SyntaxError { at, detail, lexical: false }
    }

    fn expect(&mut self, kind: T) -> Result<JsText, SyntaxError> {
        if self.kind() != kind {
            return Err(self.fail(kind, kind, &[]));
        }
        self.i += 1;
        Ok(self.tokens.text(self.src, self.i - 1).into())
    }

    /// The next token if it is a keyword in the given range of token types.
    fn keyword(&mut self, from: T, to: T) -> Result<T, SyntaxError> {
        let t = self.kind();
        if t < from || t > to {
            return Err(self.fail(from, to, &[]));
        }
        self.i += 1;
        Ok(t)
    }

    fn string(&mut self) -> Result<JsText, SyntaxError> {
        let t = self.kind();
        if t != T::UnqString && t != T::QString {
            return Err(self.fail(T::QString, T::UnqString, &[]));
        }
        self.i += 1;
        Ok(self.tokens.text(self.src, self.i - 1).into())
    }

    fn directive(&mut self, db: &mut RequirementDb) -> Result<bool, SyntaxError> {
        match self.kind() {
            T::AccTitle => {
                self.i += 1;
                db.set_acc_title(self.expect(T::AccTitleValue)?.trim_js());
            }
            T::AccDescr => {
                self.i += 1;
                db.set_acc_description(self.expect(T::AccDescrValue)?.trim_js());
            }
            T::AccDescrMultilineValue => {
                db.set_acc_description(self.text().trim_js());
                self.i += 1;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn id_list(&mut self) -> Result<Vec<JsText>, SyntaxError> {
        let mut list = Vec::new();
        loop {
            if !matches!(self.kind(), T::Alpha | T::UnqString | T::QString) {
                return Err(self.fail(T::QString, T::UnqString, &[T::Alpha]));
            }
            list.push(self.text());
            self.i += 1;
            if self.kind() != T::Comma {
                return Ok(list);
            }
            self.i += 1;
        }
    }

    fn styles_opt(&mut self) -> Result<Vec<JsText>, SyntaxError> {
        let mut styles = Vec::new();
        loop {
            if !is_style_part(self.kind()) {
                return Err(self.fail(T::Alpha, T::Semicolon, &[T::Minus, T::Brkt]));
            }
            let mut style = JsText::default();
            while is_style_part(self.kind()) {
                style.push_units(self.tokens.text(self.src, self.i));
                self.i += 1;
            }
            styles.push(style);
            if self.kind() != T::Comma {
                return Ok(styles);
            }
            self.i += 1;
        }
    }

    fn class_list(&mut self) -> Result<Option<Vec<JsText>>, SyntaxError> {
        let mut classes = None;
        match self.kind() {
            T::StyleSeparator => {
                self.i += 1;
                classes = Some(self.id_list()?);
                self.expect(T::StructStart)?;
            }
            T::StructStart => self.i += 1,
            _ => return Err(self.fail(T::StructStart, T::StructStart, &[T::StyleSeparator])),
        }
        self.expect(T::Newline)?;
        Ok(classes)
    }

    /// Reads a body up to its closing brace: each field with the token type that named it.
    fn body(&mut self, requirement: bool) -> Result<Vec<(T, Value)>, SyntaxError> {
        let mut fields = Vec::new();
        loop {
            let t = self.kind();
            if t == T::StructStop {
                self.i += 1;
                return Ok(fields);
            }
            if t == T::Newline {
                self.i += 1;
                continue;
            }
            let known =
                if requirement { (T::Id..=T::Verifymthd).contains(&t) } else { matches!(t, T::Type | T::Docref) };
            if !known {
                let (from, to) = if requirement { (T::Id, T::Verifymthd) } else { (T::Type, T::Docref) };
                return Err(self.fail(from, to, &[T::Newline, T::StructStop]));
            }
            self.i += 1;
            self.expect(T::Colonsep)?;
            let value = match t {
                T::Risk => Value::Risk(risk(self.keyword(T::LowRisk, T::HighRisk)?)),
                T::Verifymthd => Value::Verify(verify_method(self.keyword(T::VerifyAnalysis, T::VerifyTest)?)),
                _ => Value::Text(self.string()?),
            };
            fields.push((t, value));
            self.expect(T::Newline)?;
        }
    }
}

/// The grammar is right recursive, so Mermaid applies a body's fields last to first and the
/// first of two repeated fields wins.
fn requirement_fields(fields: Vec<(T, Value)>) -> RequirementFields {
    let mut out = RequirementFields::default();
    for (t, value) in fields {
        match (t, value) {
            (T::Id, Value::Text(text)) => {
                out.id.get_or_insert(text);
            }
            (T::Text, Value::Text(text)) => {
                out.text.get_or_insert(text);
            }
            (_, Value::Risk(risk)) => {
                out.risk.get_or_insert(risk);
            }
            (_, Value::Verify(method)) => {
                out.verify_method.get_or_insert(method);
            }
            _ => unreachable!("a requirement's body holds only its own fields"),
        }
    }
    out
}

fn element_fields(fields: Vec<(T, Value)>) -> ElementFields {
    let mut out = ElementFields::default();
    for (t, value) in fields {
        let Value::Text(text) = value else { unreachable!("an element's fields are strings") };
        if t == T::Type {
            out.kind.get_or_insert(text);
        } else {
            out.doc_ref.get_or_insert(text);
        }
    }
    out
}

/// Either a syntax error at a place in the text, or an error of the model.
enum Failure {
    Syntax(SyntaxError),
    Model(RenderError),
}

impl From<SyntaxError> for Failure {
    fn from(error: SyntaxError) -> Self {
        Failure::Syntax(error)
    }
}

impl From<RenderError> for Failure {
    fn from(error: RenderError) -> Self {
        Failure::Model(error)
    }
}

/// Pele's `parseRequirement`.
pub(super) fn parse(cx: &ParseContext<'_>, db: &mut RequirementDb) -> Result<(), RenderError> {
    let src = cx.text;
    let mut parser = Parser { src, tokens: tokenize(src), i: 0 };
    statements(&mut parser, db).map_err(|failure| match failure {
        Failure::Syntax(error) => cx.syntax(error),
        Failure::Model(error) => error,
    })
}

fn statements(p: &mut Parser<'_>, db: &mut RequirementDb) -> Result<(), Failure> {
    while p.directive(db)? {
        if p.kind() == T::Newline {
            p.i += 1;
        }
    }
    if p.kind() != T::Rd {
        return Err(p.fail(T::Rd, T::Rd, &[T::AccTitle, T::AccDescr, T::AccDescrMultilineValue]).into());
    }
    p.i += 1;
    p.expect(T::Newline)?;

    loop {
        let t = p.kind();
        match t {
            T::Eof => return Ok(()),
            T::Newline => p.i += 1,
            _ if p.directive(db)? => {}
            T::DirectionTb | T::DirectionBt | T::DirectionRl | T::DirectionLr => {
                p.i += 1;
                db.set_direction(match t {
                    T::DirectionTb => Dir::TB,
                    T::DirectionBt => Dir::BT,
                    T::DirectionRl => Dir::RL,
                    _ => Dir::LR,
                });
            }
            _ if (T::Requirement..=T::DesignConstraint).contains(&t) => {
                let kind = requirement_type(p.keyword(T::Requirement, T::DesignConstraint)?);
                let name = p.string()?;
                let classes = p.class_list()?;
                let fields = requirement_fields(p.body(true)?);
                db.add_requirement(&name, kind, fields);
                if let Some(classes) = classes {
                    db.set_class(&[name], &classes)?;
                }
            }
            T::Element => {
                p.i += 1;
                let name = p.string()?;
                let classes = p.class_list()?;
                let fields = element_fields(p.body(false)?);
                db.add_element(&name, fields);
                if let Some(classes) = classes {
                    db.set_class(&[name], &classes)?;
                }
            }
            T::Style => {
                p.i += 1;
                let ids = p.id_list()?;
                let styles = p.styles_opt()?;
                db.set_css_style(&ids, &styles)?;
            }
            T::Classdef => {
                p.i += 1;
                let ids = p.id_list()?;
                let styles = p.styles_opt()?;
                db.define_class(&ids, &styles)?;
            }
            T::Class => {
                p.i += 1;
                let ids = p.id_list()?;
                let names = p.id_list()?;
                db.set_class(&ids, &names)?;
            }
            T::UnqString | T::QString => {
                let id = p.text();
                p.i += 1;
                let next = p.kind();
                if next == T::StyleSeparator {
                    p.i += 1;
                    let names = p.id_list()?;
                    db.set_class(&[id], &names)?;
                } else if next == T::EndArrowL || next == T::Line {
                    p.i += 1;
                    let kind = relationship(p.keyword(T::Contains, T::Traces)?);
                    p.expect(if next == T::Line { T::EndArrowR } else { T::Line })?;
                    let other = p.string()?;
                    if next == T::Line {
                        db.add_relationship(kind, &id, &other);
                    } else {
                        db.add_relationship(kind, &other, &id);
                    }
                } else {
                    return Err(p.fail(T::StyleSeparator, T::StyleSeparator, &[T::EndArrowL, T::Line]).into());
                }
            }
            _ => {
                let others = [
                    T::AccTitle,
                    T::AccDescr,
                    T::AccDescrMultilineValue,
                    T::DirectionTb,
                    T::DirectionBt,
                    T::DirectionRl,
                    T::DirectionLr,
                    T::Requirement,
                    T::FunctionalRequirement,
                    T::InterfaceRequirement,
                    T::PerformanceRequirement,
                    T::PhysicalRequirement,
                    T::DesignConstraint,
                    T::Element,
                    T::Classdef,
                    T::Class,
                    T::Style,
                    T::UnqString,
                    T::QString,
                ];
                return Err(p.fail(T::Newline, T::Eof, &others).into());
            }
        }
    }
}
