//! The venn parser and db, ported from Pele's `src/diagrams/venn/parser.ts` and `db.ts`. A token
//! is read only when the grammar has to look at one to decide, because the lexer asks the model
//! whether an indented `text` continues a set, and the statements are what tell the model so.

use super::lexer::{IndentHost, Lexer, T};
use super::{VennModel, VennStyle, VennSubset, VennText, normalize_text};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, decimal};
use indexmap::IndexMap;
use std::collections::HashSet;

const STATEMENT: &str = "'EOF', 'NEWLINE', 'TITLE', 'SET', 'UNION', 'TEXT', 'INDENT_TEXT', 'STYLE'";
const VALUE: &str = "'NUMERIC', 'IDENTIFIER', 'STRING', 'HEXCOLOR', 'RGBCOLOR', 'RGBACOLOR'";

/// Why a source is no venn diagram: Pele's syntax errors, and the semantic errors its db throws.
pub(super) enum Failure {
    Syntax(SyntaxError),
    Semantic(String),
}

fn normalize_ids(ids: &[JsText]) -> Vec<JsText> {
    let mut sets: Vec<JsText> = ids.iter().map(|id| normalize_text(id)).collect();
    sets.sort_by(|a, b| a[..].cmp(&b[..]));
    sets
}

/// Pele's `VennDb`.
#[derive(Default)]
pub(super) struct VennDb {
    pub(super) model: VennModel,
    known: HashSet<JsText>,
    current: Option<Vec<JsText>>,
    indent: bool,
}

impl IndentHost for VennDb {
    fn indent_mode(&self) -> bool {
        self.indent
    }

    fn set_indent_mode(&mut self, enabled: bool) {
        self.indent = enabled;
    }
}

impl VennDb {
    /// A set without a size gets 10, and an overlap of n sets 10 / n².
    fn add_subset(&mut self, ids: &[JsText], label: Option<&[u16]>, size: Option<f64>) {
        let sets = normalize_ids(ids);
        self.current = Some(sets.clone());
        if sets.len() == 1 {
            self.known.insert(sets[0].clone());
        }
        let count = ids.len() as f64;
        let label = label.filter(|label| !label.is_empty()).map(normalize_text);
        self.model.subsets.push(VennSubset { sets, size: size.unwrap_or(10.0 / (count * count)), label });
    }

    fn add_text(&mut self, ids: Vec<JsText>, id: &[u16], label: Option<&[u16]>) {
        let label = label.filter(|label| !label.is_empty()).map(normalize_text);
        self.model.text_nodes.push(VennText { sets: normalize_ids(&ids), id: normalize_text(id), label });
    }

    fn add_style(&mut self, ids: &[JsText], data: Vec<(JsText, JsText)>) {
        let mut styles = IndexMap::new();
        for (key, value) in data {
            styles.insert(key, normalize_text(&value));
        }
        self.model.styles.push(VennStyle { targets: normalize_ids(ids), styles });
    }

    fn validate_union(&self, ids: &[JsText]) -> Result<(), Failure> {
        let unknown: Vec<String> = ids
            .iter()
            .map(|id| normalize_text(id))
            .filter(|id| !self.known.contains(id))
            .map(|id| id.to_display())
            .collect();
        if unknown.is_empty() {
            Ok(())
        } else {
            Err(Failure::Semantic(format!("unknown set identifier: {}", unknown.join(", "))))
        }
    }
}

struct Parser<'a, 'db> {
    lexer: Lexer<'a>,
    db: &'db mut VennDb,
    peeked: Option<T>,
}

impl<'a> Parser<'a, '_> {
    fn peek(&mut self) -> T {
        match self.peeked {
            Some(kind) => kind,
            None => {
                let kind = self.lexer.next(self.db);
                self.peeked = Some(kind);
                kind
            }
        }
    }

    fn fail<X>(&mut self, expected: &str) -> Result<X, Failure> {
        let kind = self.peek();
        let at = self.lexer.start;
        Err(Failure::Syntax(if kind == T::Error {
            SyntaxError { at, detail: String::new(), lexical: true }
        } else {
            SyntaxError { at, detail: format!("Expecting {expected}, got '{}'", kind.name()), lexical: false }
        }))
    }

    fn take(&mut self) -> &'a [u16] {
        self.peeked = None;
        self.lexer.text
    }

    fn expect(&mut self, wanted: T) -> Result<&'a [u16], Failure> {
        if self.peek() != wanted {
            return self.fail(&format!("'{}'", wanted.name()));
        }
        Ok(self.take())
    }

    fn identifier(&mut self) -> Result<JsText, Failure> {
        if !matches!(self.peek(), T::Identifier | T::String) {
            return self.fail("'IDENTIFIER', 'STRING'");
        }
        Ok(self.take().into())
    }

    fn identifiers(&mut self) -> Result<Vec<JsText>, Failure> {
        let mut list = vec![self.identifier()?];
        while self.peek() == T::Comma {
            self.take();
            list.push(self.identifier()?);
        }
        Ok(list)
    }

    fn subset(&mut self, ids: Vec<JsText>, union: bool) -> Result<(), Failure> {
        let label = if self.peek() == T::BracketLabel { Some(self.take()) } else { None };
        let mut size = None;
        if self.peek() == T::Colon {
            self.take();
            size = Some(decimal(self.expect(T::Numeric)?));
        }
        if union {
            if ids.len() < 2 {
                return Err(Failure::Semantic("union requires multiple identifiers".to_owned()));
            }
            self.db.validate_union(&ids)?;
        }
        self.db.add_subset(&ids, label, size);
        self.db.set_indent_mode(true);
        Ok(())
    }

    /// The id of a text, and the label in brackets that an id which is not a number may carry.
    /// `sets` is `None` for an indented text, which belongs to the set or overlap before it.
    fn text(&mut self, sets: Option<Vec<JsText>>) -> Result<(), Failure> {
        let kind = self.peek();
        if !matches!(kind, T::Identifier | T::String | T::Numeric) {
            return self.fail("'NUMERIC', 'IDENTIFIER', 'STRING'");
        }
        let id = self.take();
        let label = if kind != T::Numeric && self.peek() == T::BracketLabel { Some(self.take()) } else { None };
        let sets = match sets {
            Some(sets) => sets,
            None => match &self.db.current {
                Some(current) => current.clone(),
                None => return Err(Failure::Semantic("text requires set".to_owned())),
            },
        };
        self.db.add_text(sets, id, label);
        Ok(())
    }

    fn style(&mut self) -> Result<(), Failure> {
        let ids = self.identifiers()?;
        let mut styles = Vec::new();
        loop {
            let key = self.expect(T::Identifier)?.into();
            self.expect(T::Colon)?;
            let value: JsText = if self.peek() == T::String {
                self.take().into()
            } else {
                let mut parts: Vec<&[u16]> = Vec::new();
                while matches!(self.peek(), T::Identifier | T::Numeric | T::HexColor | T::RgbColor | T::RgbaColor) {
                    parts.push(self.take());
                }
                if parts.is_empty() {
                    return self.fail(VALUE);
                }
                parts.join(&32).into()
            };
            styles.push((key, value));
            if self.peek() != T::Comma {
                break;
            }
            self.take();
        }
        self.db.add_style(&ids, styles);
        Ok(())
    }
}

/// Pele's `parseVenn`.
pub(super) fn parse(src: &[u16], db: &mut VennDb) -> Result<(), Failure> {
    let mut p = Parser { lexer: Lexer::new(src), db, peeked: None };
    while p.peek() == T::Newline {
        p.take();
    }
    p.expect(T::Venn)?;
    loop {
        let statement = p.peek();
        if statement == T::Eof {
            return Ok(());
        }
        if statement < T::Newline || statement > T::Style || statement == T::Venn {
            return p.fail(STATEMENT);
        }
        let text = p.take();
        match statement {
            T::Title => p.db.model.title = Some(text[6..].into()),
            T::Set => {
                let id = p.identifier()?;
                p.subset(vec![id], false)?;
            }
            T::Union => {
                let ids = p.identifiers()?;
                p.subset(ids, true)?;
            }
            T::Text => {
                let ids = p.identifiers()?;
                p.text(Some(ids))?;
            }
            T::IndentText => p.text(None)?,
            T::Style => p.style()?,
            _ => {}
        }
    }
}
