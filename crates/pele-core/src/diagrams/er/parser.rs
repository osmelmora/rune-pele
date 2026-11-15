//! The ER parser, ported from Pele's `src/diagrams/er/parser.ts`: a hand-written parser over the
//! lexer's tokens that makes the calls into the db that Mermaid's grammar makes.

use super::db::{DocItem, ErDb, ParsedAttribute};
use super::lexer::{T, Tokens, tokenize};
use super::{Cardinality, Direction, Identification, RelSpec};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units};

fn cardinality(t: T) -> Option<Cardinality> {
    match t {
        T::ZeroOrOne => Some(Cardinality::ZeroOrOne),
        T::ZeroOrMore => Some(Cardinality::ZeroOrMore),
        T::OneOrMore => Some(Cardinality::OneOrMore),
        T::OnlyOne => Some(Cardinality::OnlyOne),
        T::MdParent => Some(Cardinality::MdParent),
        _ => None,
    }
}

fn is_name(t: T) -> bool {
    matches!(t, T::UnicodeText | T::EntityName | T::Num | T::DecimalNum | T::EntityOne)
}

fn is_style_component(t: T) -> bool {
    matches!(t, T::StyleText | T::Num | T::Colon | T::Brkt)
}

const NAME: &str = "'UNICODE_TEXT', 'NUM', 'ENTITY_NAME', 'DECIMAL_NUM', 'ENTITY_ONE'";
const CARDS: &str = "'ZERO_OR_ONE', 'ZERO_OR_MORE', 'ONE_OR_MORE', 'ONLY_ONE', 'MD_PARENT'";
const LINE: &str = "'EOF', 'NEWLINE', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'SUBGRAPH', \
'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'CLASSDEF', 'CLASS', 'STYLE', \
'UNICODE_TEXT', 'NUM', 'ENTITY_NAME', 'DECIMAL_NUM', 'ENTITY_ONE'";

/// A subgraph being read: what Mermaid's grammar hands to `addSubGraph`, the value of every
/// statement inside. Besides entity names that includes accessibility text and the keywords
/// `style`, `class` and `classDef`.
struct Frame {
    id: JsText,
    title: JsText,
    list: Vec<DocItem>,
}

/// Adds a statement's value to the subgraph being read, where there is one.
fn note(frames: &mut [Frame], item: DocItem) {
    if let [_, .., inner] = frames {
        inner.list.push(item);
    }
}

struct Parser<'a> {
    src: &'a [u16],
    tokens: Tokens,
    i: usize,
}

type Parsed<T> = Result<T, SyntaxError>;

impl<'a> Parser<'a> {
    /// The current token. Lexing stops at an error token, which no rule takes, so parsing never
    /// reads past it.
    fn kind(&self) -> T {
        self.tokens.types.get(self.i).copied().unwrap_or(T::End)
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
        let at = self.tokens.starts.get(self.i).copied().unwrap_or(self.src.len());
        if t == T::Error {
            return SyntaxError { at, detail: String::new(), lexical: true };
        }
        let got = if t == T::Char { String::from_utf16_lossy(self.text(self.i)) } else { t.name().to_owned() };
        SyntaxError { at, detail: format!("Expecting {expected}, got '{got}'"), lexical: false }
    }

    fn expect(&mut self, kind: T) -> Parsed<&'a [u16]> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Parsed<()> {
        if !matches!(self.kind(), T::Newline | T::Semi | T::Eof) {
            return Err(self.fail("'EOF', 'NEWLINE', 'SEMI'"));
        }
        self.i += 1;
        Ok(())
    }

    fn entity_name(&mut self) -> Parsed<JsText> {
        let t = self.kind();
        if !is_name(t) {
            return Err(self.fail(NAME));
        }
        let text = self.take();
        Ok(if t == T::EntityName { &text[1..text.len() - 1] } else { text }.into())
    }

    fn id_list(&mut self) -> Parsed<Vec<JsText>> {
        let mut ids = Vec::new();
        loop {
            if !matches!(self.kind(), T::UnicodeText | T::StyleText) {
                return Err(self.fail("'UNICODE_TEXT', 'STYLE_TEXT'"));
            }
            ids.push(self.take().into());
            if self.kind() != T::Comma {
                return Ok(ids);
            }
            self.i += 1;
        }
    }

    fn styles(&mut self) -> Parsed<Vec<JsText>> {
        let mut styles = Vec::new();
        loop {
            if !is_style_component(self.kind()) {
                return Err(self.fail("'STYLE_TEXT', 'COLON', 'NUM', 'BRKT'"));
            }
            let mut style = JsText::from(self.take());
            while is_style_component(self.kind()) {
                style.push_units(self.take());
            }
            styles.push(style);
            if self.kind() != T::Comma {
                return Ok(styles);
            }
            self.i += 1;
        }
    }

    fn rel_spec(&mut self) -> Parsed<RelSpec> {
        let card_b = cardinality(self.kind()).expect("a relationship starts at a cardinality");
        self.i += 1;
        let rel_type = match self.kind() {
            T::Identifying => Identification::Identifying,
            T::NonIdentifying => Identification::NonIdentifying,
            _ => return Err(self.fail("'NON_IDENTIFYING', 'IDENTIFYING'")),
        };
        self.i += 1;
        let Some(card_a) = cardinality(self.kind()) else {
            return Err(self.fail(CARDS));
        };
        self.i += 1;
        Ok(RelSpec { card_a, rel_type, card_b })
    }

    fn role(&mut self) -> Parsed<JsText> {
        let t = self.kind();
        if !matches!(t, T::Word | T::EntityName | T::UnicodeText) {
            return Err(self.fail("'UNICODE_TEXT', 'ENTITY_NAME', 'WORD'"));
        }
        let text = self.take();
        Ok(if t == T::UnicodeText { text } else { &text[1..text.len() - 1] }.into())
    }

    fn is_char(&self, c: u8) -> bool {
        self.kind() == T::Char && self.text(self.i) == [u16::from(c)]
    }

    /// The attributes of a block, in the order they are written.
    fn attributes(&mut self) -> Parsed<Vec<ParsedAttribute>> {
        let mut list = Vec::new();
        loop {
            let mut kind = JsText::from(self.expect(T::AttributeWord)?);
            if self.is_char(b'?') {
                kind.push_units(self.take());
            }
            if self.kind() != T::AttributeWord {
                let expected = if kind.last() == Some(&63) { "'ATTRIBUTE_WORD'" } else { "'ATTRIBUTE_WORD', '?'" };
                return Err(self.fail(expected));
            }
            let mut attribute = ParsedAttribute { kind, name: self.take().into(), keys: None, comment: None };
            if self.kind() == T::AttributeKey {
                let mut keys = vec![JsText::from(self.take())];
                while self.is_char(b',') {
                    self.i += 1;
                    keys.push(self.expect(T::AttributeKey)?.into());
                }
                attribute.keys = Some(keys);
            }
            if self.kind() == T::Comment {
                let text = self.take();
                attribute.comment = Some(text[1..text.len() - 1].into());
            }
            list.push(attribute);
            if self.kind() == T::BlockStop {
                return Ok(list);
            }
        }
    }

    /// An optional `{ ... }` block: nothing when there is none or it is empty.
    fn block(&mut self) -> Parsed<Option<Vec<ParsedAttribute>>> {
        if self.kind() != T::BlockStart {
            return Ok(None);
        }
        self.i += 1;
        if self.kind() == T::BlockStop {
            self.i += 1;
            return Ok(None);
        }
        let list = self.attributes()?;
        self.i += 1;
        Ok(Some(list))
    }

    fn classes_after_separator(&mut self) -> Parsed<Option<Vec<JsText>>> {
        if self.kind() != T::StyleSeparator {
            return Ok(None);
        }
        self.i += 1;
        self.id_list().map(Some)
    }

    /// An entity, an entity with an alias, or a relationship. Answers the names it holds.
    fn entity_statement(&mut self, db: &mut ErDb) -> Parsed<Vec<JsText>> {
        let a = self.entity_name()?;
        if self.kind() == T::Sqs {
            self.i += 1;
            let alias = self.entity_name()?;
            self.expect(T::Sqe)?;
            let classes = self.classes_after_separator()?;
            let attributes = self.block()?;
            db.add_entity(&a, &alias);
            if let Some(attributes) = attributes {
                db.add_attributes(&a, attributes);
            }
            if let Some(classes) = classes {
                db.set_class(std::slice::from_ref(&a), &classes);
            }
            return Ok(vec![a]);
        }
        let classes = self.classes_after_separator()?;
        if cardinality(self.kind()).is_some() {
            let spec = self.rel_spec()?;
            let b = self.entity_name()?;
            let classes_b = self.classes_after_separator()?;
            self.expect(T::Colon)?;
            let label = self.role()?;
            db.add_entity(&a, &[]);
            db.add_entity(&b, &[]);
            db.add_relationship(&a, label, &b, spec);
            if let Some(classes) = classes {
                db.set_class(std::slice::from_ref(&a), &classes);
            }
            if let Some(classes) = classes_b {
                db.set_class(std::slice::from_ref(&b), &classes);
            }
            return Ok(vec![a, b]);
        }
        let attributes = self.block()?;
        db.add_entity(&a, &[]);
        if let Some(attributes) = attributes {
            db.add_attributes(&a, attributes);
        }
        if let Some(classes) = classes {
            db.set_class(std::slice::from_ref(&a), &classes);
        }
        Ok(vec![a])
    }

    fn document(&mut self, db: &mut ErDb) -> Parsed<()> {
        self.expect(T::ErDiagram)?;
        // Subgraphs nest on this stack rather than on the call stack; the first frame is the
        // diagram itself.
        let mut frames = vec![Frame { id: JsText::default(), title: JsText::default(), list: Vec::new() }];
        let expected = |frames: &[Frame]| if frames.len() > 1 { format!("'END', {LINE}") } else { LINE.to_owned() };
        loop {
            let nested = frames.len() > 1;
            let t = self.kind();
            match t {
                T::Newline => self.i += 1,
                T::Eof => {
                    self.i += 1;
                    if nested {
                        return Err(self.fail(&expected(&frames)));
                    }
                    return Ok(());
                }
                T::EndKw => {
                    if !nested {
                        return Err(self.fail(LINE));
                    }
                    self.i += 1;
                    let done = frames.pop().expect("a subgraph's frame");
                    let id = db.add_subgraph(&done.id, done.list, &done.title);
                    note(&mut frames, DocItem::Name(id));
                }
                T::Subgraph => {
                    self.i += 1;
                    let id = self.entity_name()?;
                    let mut title = id.clone();
                    if self.kind() == T::Sqs {
                        self.i += 1;
                        title = self.entity_name()?;
                        while is_name(self.kind()) {
                            title.push(32);
                            let more = self.entity_name()?;
                            title.push_units(&more);
                        }
                        self.expect(T::Sqe)?;
                    }
                    self.separator()?;
                    frames.push(Frame { id, title, list: Vec::new() });
                }
                T::AccTitle => {
                    self.i += 1;
                    let value: JsText = self.expect(T::AccTitleValue)?.trim_js().into();
                    db.set_acc_title(&value);
                    note(&mut frames, DocItem::Name(value));
                }
                T::AccDescr => {
                    self.i += 1;
                    let value: JsText = self.expect(T::AccDescrValue)?.trim_js().into();
                    db.set_acc_description(&value);
                    note(&mut frames, DocItem::Name(value));
                }
                T::AccDescrMultilineValue => {
                    let value: JsText = self.take().trim_js().into();
                    db.set_acc_description(&value);
                    note(&mut frames, DocItem::Name(value));
                }
                T::DirectionTb | T::DirectionBt | T::DirectionRl | T::DirectionLr => {
                    self.i += 1;
                    let value = match t {
                        T::DirectionTb => Direction::TB,
                        T::DirectionBt => Direction::BT,
                        T::DirectionRl => Direction::RL,
                        _ => Direction::LR,
                    };
                    if nested {
                        note(&mut frames, DocItem::Dir(value));
                    } else {
                        db.model.direction = value;
                    }
                }
                T::ClassDef | T::Style => {
                    let keyword = self.take();
                    let ids = self.id_list()?;
                    let styles = self.styles()?;
                    self.separator()?;
                    if t == T::ClassDef {
                        db.add_class(&ids, &styles);
                    } else {
                        db.add_css_styles(&ids, &styles);
                    }
                    note(&mut frames, DocItem::Name(keyword.into()));
                }
                T::Class => {
                    let keyword = self.take();
                    let ids = self.id_list()?;
                    let classes = self.id_list()?;
                    db.set_class(&ids, &classes);
                    note(&mut frames, DocItem::Name(keyword.into()));
                }
                _ => {
                    if !is_name(t) {
                        return Err(self.fail(&expected(&frames)));
                    }
                    let names = self.entity_statement(db)?;
                    for name in names {
                        note(&mut frames, DocItem::Name(name));
                    }
                }
            }
        }
    }
}

/// Parses `src` into `db`, or answers where it first goes wrong.
pub(super) fn parse(src: &[u16], db: &mut ErDb) -> Result<(), SyntaxError> {
    let mut parser = Parser { src, tokens: tokenize(src), i: 0 };
    parser.document(db)
}
