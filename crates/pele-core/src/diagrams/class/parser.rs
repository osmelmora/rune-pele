//! The class diagram parser, ported from Pele's `src/diagrams/class/parser.ts`: a hand-written
//! parser over the lexer's tokens that makes the calls into the db that Mermaid's grammar makes.

use super::db::ClassDb;
use super::lexer::{T, Tokens, tokenize};
use super::{ClassRelation, Direction, LineType, RelationEnd};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units};

fn alpha_num(t: T) -> bool {
    matches!(t, T::UnicodeText | T::Alpha | T::Minus)
}

fn style_part(t: T) -> bool {
    matches!(t, T::Alpha | T::Colon | T::Brkt | T::Style | T::Pct | T::Label)
}

/// The marker a token stands for at the end of a relation, where it is one.
fn end_type(t: T) -> Option<RelationEnd> {
    match t {
        T::Aggregation => Some(RelationEnd::Aggregation),
        T::Extension => Some(RelationEnd::Extension),
        T::Composition => Some(RelationEnd::Composition),
        T::Dependency => Some(RelationEnd::Dependency),
        T::Lollipop => Some(RelationEnd::Lollipop),
        _ => None,
    }
}

const NAME_TOKENS: &str = "'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS', 'BQUOTE_STR'";

/// A namespace being read: its id, and the classes and notes written in it.
struct Frame {
    id: JsText,
    classes: Vec<JsText>,
    notes: Vec<JsText>,
}

struct Parser<'a> {
    src: &'a [u16],
    tokens: Tokens,
    i: usize,
}

type Parsed<T> = Result<T, SyntaxError>;

impl<'a> Parser<'a> {
    fn kind(&self) -> T {
        self.tokens.types[self.i]
    }

    fn kind_at(&self, at: usize) -> Option<T> {
        self.tokens.types.get(at).copied()
    }

    fn text(&self, at: usize) -> &'a [u16] {
        &self.src[self.tokens.starts[at]..self.tokens.ends[at]]
    }

    fn take(&mut self) -> &'a [u16] {
        self.i += 1;
        self.text(self.i - 1)
    }

    fn fail(&self, expected: &str) -> SyntaxError {
        let t = self.kind();
        let at = self.tokens.starts[self.i];
        if t == T::Error {
            return SyntaxError { at, detail: String::new(), lexical: true };
        }
        SyntaxError { at, detail: format!("Expecting {expected}, got '{}'", t.name()), lexical: false }
    }

    fn expect(&mut self, kind: T) -> Parsed<&'a [u16]> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    fn alpha_num(&mut self) -> Parsed<&'a [u16]> {
        if !alpha_num(self.kind()) {
            return Err(self.fail("'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS'"));
        }
        Ok(self.take())
    }

    fn name(&mut self, generic: bool) -> Parsed<JsText> {
        let mut out = JsText::default();
        loop {
            if self.kind() == T::BquoteStr {
                out.push_units(self.take());
                break;
            }
            if !alpha_num(self.kind()) {
                return Err(self.fail(NAME_TOKENS));
            }
            out.push_units(self.take());
            let t = self.kind();
            if t == T::Dot {
                out.push(46);
                self.i += 1;
            } else if !alpha_num(t) && t != T::BquoteStr {
                break;
            }
        }
        if generic && self.kind() == T::GenericType {
            out.push(126);
            out.push_units(self.take());
            out.push(126);
        }
        Ok(out)
    }

    fn class_label(&mut self) -> Parsed<&'a [u16]> {
        self.i += 1;
        let label = self.expect(T::Str)?;
        self.expect(T::Sqe)?;
        Ok(label)
    }

    /// The members of a body, last to first as the grammar collects them.
    fn members(&mut self) -> Parsed<Vec<&'a [u16]>> {
        if self.kind() != T::Member {
            return Err(self.fail("'STRUCT_STOP', 'MEMBER'"));
        }
        let mut list = Vec::new();
        while self.kind() == T::Member {
            list.push(self.take());
        }
        self.expect(T::StructStop)?;
        list.reverse();
        Ok(list)
    }

    fn body(&mut self, db: &mut ClassDb, id: &[u16]) -> Parsed<()> {
        if self.kind() == T::StructStop {
            self.i += 1;
        } else {
            let members = self.members()?;
            db.add_members(id, members);
        }
        Ok(())
    }

    fn class_statement(&mut self, db: &mut ClassDb) -> Parsed<JsText> {
        self.i += 1;
        let id = self.name(true)?;
        db.add_class(&id);
        if self.kind() == T::Sqs {
            let label = self.class_label()?;
            db.set_class_label(&id, label);
        }
        match self.kind() {
            T::StyleSeparator => {
                self.i += 1;
                let css = self.alpha_num()?;
                db.set_css_class(&id, css);
                if self.kind() == T::StructStart {
                    self.i += 1;
                    let members = self.members()?;
                    db.add_members(&id, members);
                }
            }
            T::StructStart => {
                self.i += 1;
                self.body(db, &id)?;
            }
            T::AnnotationStart => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(T::AnnotationEnd)?;
                db.add_annotation(&id, annotation);
                if self.kind() == T::StructStart {
                    self.i += 1;
                    self.body(db, &id)?;
                }
            }
            _ => {}
        }
        Ok(id)
    }

    fn note_statement(&mut self, db: &mut ClassDb) -> Parsed<JsText> {
        let kind = self.kind();
        self.i += 1;
        if kind == T::Note {
            let text = self.expect(T::Str)?;
            return Ok(db.add_note(text, None));
        }
        let id = self.name(true)?;
        let text = self.expect(T::Str)?;
        Ok(db.add_note(text, Some(&id)))
    }

    /// Namespaces nest, so they are parsed with a stack of open ones.
    fn namespace_statement(&mut self, db: &mut ClassDb) -> Parsed<()> {
        let mut open: Vec<Frame> = Vec::new();
        loop {
            self.i += 1;
            let ns = self.name(false)?;
            let id = if self.kind() == T::Sqs {
                let label = self.class_label()?;
                db.add_namespace(&ns, Some(label))
            } else {
                db.add_namespace(&ns, None)
            };
            self.expect(T::StructStart)?;
            if self.kind() == T::Newline {
                self.i += 1;
            }
            open.push(Frame { id, classes: Vec::new(), notes: Vec::new() });
            let mut item = true;
            while item {
                let t = self.kind();
                if t == T::Namespace {
                    break;
                }
                if t == T::Class {
                    let class = self.class_statement(db)?;
                    open.last_mut().expect("a namespace is open").classes.push(class);
                } else if t == T::Note || t == T::NoteFor {
                    let note = self.note_statement(db)?;
                    open.last_mut().expect("a namespace is open").notes.push(note);
                } else {
                    return Err(self.fail("'NAMESPACE', 'CLASS', 'NOTE_FOR', 'NOTE'"));
                }
                loop {
                    if self.kind() == T::Newline {
                        let next = self.kind_at(self.i + 1);
                        if matches!(next, Some(T::Namespace | T::Class | T::Note | T::NoteFor)) {
                            self.i += 1;
                            break;
                        }
                        self.i += 1;
                    }
                    self.expect(T::StructStop)?;
                    let done = open.pop().expect("a namespace is open");
                    db.add_classes_to_namespace(&done.id, &done.classes, &done.notes);
                    db.pop_namespace();
                    if open.is_empty() {
                        item = false;
                        break;
                    }
                }
            }
            if open.is_empty() {
                return Ok(());
            }
        }
    }

    fn relation_end(&mut self) -> RelationEnd {
        match end_type(self.kind()) {
            Some(end) => {
                self.i += 1;
                end
            }
            None => RelationEnd::None,
        }
    }

    fn class_or_relation(&mut self, db: &mut ClassDb) -> Parsed<()> {
        let id1 = self.name(true)?;
        let t = self.kind();
        if t == T::Label {
            let label = ClassDb::cleanup_label(self.take());
            db.add_member(&id1, &label);
            return Ok(());
        }
        let mut title1: JsText = "none".into();
        if t == T::Str {
            title1 = self.take().into();
        } else if end_type(t).is_none() && t != T::Line && t != T::DottedLine {
            return Ok(());
        }
        let type1 = self.relation_end();
        let t = self.kind();
        if t != T::Line && t != T::DottedLine {
            return Err(
                self.fail("'AGGREGATION', 'EXTENSION', 'COMPOSITION', 'DEPENDENCY', 'LOLLIPOP', 'LINE', 'DOTTED_LINE'")
            );
        }
        self.i += 1;
        let type2 = self.relation_end();
        let title2: JsText = if self.kind() == T::Str { self.take().into() } else { "none".into() };
        let id2 = self.name(true)?;
        let title = if self.kind() == T::Label { Some(ClassDb::cleanup_label(self.take())) } else { None };
        let line_type = if t == T::Line { LineType::Line } else { LineType::Dotted };
        let relation = ClassRelation {
            id1,
            id2,
            relation_title1: title1,
            relation_title2: title2,
            title,
            type1,
            type2,
            line_type,
        };
        db.add_relation(relation);
        Ok(())
    }

    fn styles(&mut self) -> Parsed<Vec<JsText>> {
        let mut out = Vec::new();
        loop {
            if !style_part(self.kind()) {
                return Err(self.fail("'ALPHA', 'NUM', 'COLON', 'UNIT', 'SPACE', 'BRKT', 'STYLE', 'PCT', 'LABEL'"));
            }
            let mut style = JsText::from(self.take());
            while style_part(self.kind()) {
                style.push_units(self.take());
            }
            out.push(style);
            if self.kind() != T::Comma {
                return Ok(out);
            }
            self.i += 1;
        }
    }

    fn tooltip(&mut self, db: &mut ClassDb, id: &[u16]) {
        if self.kind() == T::Str {
            let tip = self.take();
            db.set_tooltip(id, tip);
        }
    }

    fn link_statement(&mut self, db: &mut ClassDb, id: &[u16]) -> Parsed<()> {
        let href = self.expect(T::Str)?;
        let tip = if self.kind() == T::Str { Some(self.take()) } else { None };
        if self.kind() == T::LinkTarget {
            let target = self.take();
            db.set_link(id, href, Some(target));
        } else {
            db.set_link(id, href, None);
        }
        if let Some(tip) = tip {
            db.set_tooltip(id, tip);
        }
        Ok(())
    }

    fn statement(&mut self, db: &mut ClassDb) -> Parsed<()> {
        let t = self.kind();
        match t {
            T::UnicodeText | T::Alpha | T::Minus | T::BquoteStr => return self.class_or_relation(db),
            T::Namespace => return self.namespace_statement(db),
            T::Class => return self.class_statement(db).map(|_| ()),
            T::Member => {
                self.i += 1;
                return Ok(());
            }
            T::AnnotationStart => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(T::AnnotationEnd)?;
                let class = self.name(true)?;
                db.add_annotation(&class, annotation);
                return Ok(());
            }
            T::Callback => {
                self.i += 1;
                let id = self.name(true)?;
                self.expect(T::Str)?;
                db.set_click_event(&id);
                self.tooltip(db, &id);
                return Ok(());
            }
            T::Link => {
                self.i += 1;
                let id = self.name(true)?;
                return self.link_statement(db, &id);
            }
            T::Click => {
                self.i += 1;
                let id = self.name(true)?;
                if self.kind() == T::Href {
                    self.i += 1;
                    return self.link_statement(db, &id);
                }
                self.expect(T::CallbackName)?;
                if self.kind() == T::CallbackArgs {
                    self.i += 1;
                }
                db.set_click_event(&id);
                self.tooltip(db, &id);
                return Ok(());
            }
            T::Style => {
                self.i += 1;
                let id = self.expect(T::Alpha)?;
                let styles = self.styles()?;
                db.set_css_style(id, &styles);
                return Ok(());
            }
            T::CssClass => {
                self.i += 1;
                let ids = self.expect(T::Str)?;
                let class = self.expect(T::Alpha)?;
                db.set_css_class(ids, class);
                return Ok(());
            }
            T::Note | T::NoteFor => return self.note_statement(db).map(|_| ()),
            T::ClassDef => {
                self.i += 1;
                let mut ids = vec![self.expect(T::Alpha)?];
                while self.kind() == T::Comma {
                    self.i += 1;
                    ids.push(self.expect(T::Alpha)?);
                }
                let styles = self.styles()?;
                db.define_class(&ids, &styles);
                return Ok(());
            }
            T::DirectionTb | T::DirectionBt | T::DirectionRl | T::DirectionLr => {
                self.i += 1;
                db.set_direction(match t {
                    T::DirectionTb => Direction::TB,
                    T::DirectionBt => Direction::BT,
                    T::DirectionRl => Direction::RL,
                    _ => Direction::LR,
                });
                return Ok(());
            }
            T::AccTitle => {
                self.i += 1;
                let value = self.expect(T::AccTitleValue)?;
                db.set_acc_title(value.trim_js());
                return Ok(());
            }
            T::AccDescr => {
                self.i += 1;
                let value = self.expect(T::AccDescrValue)?;
                db.set_acc_description(value.trim_js());
                return Ok(());
            }
            T::AccDescrMultilineValue => {
                let value = self.take();
                db.set_acc_description(value.trim_js());
                return Ok(());
            }
            _ => {}
        }
        Err(self.fail(
            "'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'NAMESPACE', 'ANNOTATION_START', 'CLASS', 'MEMBER', 'NOTE_FOR', 'NOTE', 'CLASSDEF', 'direction_tb', 'CALLBACK', 'LINK', 'CLICK', 'STYLE', 'CSSCLASS', 'ALPHA', 'MINUS', 'BQUOTE_STR'",
        ))
    }
}

/// Pele's `parseClassDiagram`.
pub(super) fn parse(src: &[u16], db: &mut ClassDb) -> Result<(), SyntaxError> {
    let mut parser = Parser { src, tokens: tokenize(src), i: 0 };
    // Mermaid's grammar also accepts statements with no `classDiagram` line before them.
    let header = parser.kind() == T::ClassDiagram;
    if header {
        parser.i += 1;
        parser.expect(T::Newline)?;
    }
    loop {
        parser.statement(db)?;
        if parser.kind() != T::Newline {
            break;
        }
        parser.i += 1;
        if matches!(parser.kind(), T::Eof | T::End) {
            break;
        }
    }
    if header {
        parser.expect(T::Eof)?;
    }
    parser.expect(T::End)?;
    Ok(())
}
