//! The state diagram parser (Pele's `src/diagrams/state/parser.ts`): a hand-written parser for
//! Mermaid's `stateDiagram.jison` grammar that reports errors in jison's words.

use super::db::{StateDb, trim_colon};
use super::lexer::{DIRECTIONS, T, Tokens, tokenize};
use super::{Description, Direction, DocId, ROOT, StateNote, StateStmt, StateType, Stmt};
use crate::catalog::ParseContext;
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units, split_unit};
use crate::{Diagnostic, ErrorCode, RenderError};

const STATEMENT: &str = "'NL', 'ID', 'HIDE_EMPTY', 'scale', 'COMPOSIT_STATE', 'STATE_DESCR', 'FORK', 'JOIN', 'CHOICE', \
'CONCURRENT', 'note', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'CLICK', 'classDef', 'style', 'class', \
'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'EDGE_STATE'";

struct Parser<'a, 'c> {
    cx: &'a ParseContext<'c>,
    tokens: Tokens,
    i: usize,
}

impl Parser<'_, '_> {
    fn kind(&self) -> Option<T> {
        self.tokens.types.get(self.i).copied()
    }

    fn text(&self, at: usize) -> JsText {
        self.tokens.texts[at].as_slice().into()
    }

    fn fail(&self, expected: &str) -> RenderError {
        let t = self.tokens.types[self.i];
        let at = self.tokens.starts[self.i];
        if t == T::Error {
            if let Some(message) = &self.tokens.error {
                let location = self.cx.origin.locate(self.cx.text, at);
                let diagnostic =
                    Diagnostic { location: Some(location), ..Diagnostic::new(ErrorCode::Syntax, message.clone()) };
                return RenderError::Diagnostic(diagnostic);
            }
            return self.cx.syntax(SyntaxError { at, detail: String::new(), lexical: true });
        }
        let detail = format!("Expecting {expected}, got '{}'", t.name());
        self.cx.syntax(SyntaxError { at, detail, lexical: false })
    }

    fn expect(&mut self, kind: T) -> Result<JsText, RenderError> {
        if self.kind() != Some(kind) {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        self.i += 1;
        Ok(self.text(self.i - 1))
    }

    fn id_statement(&mut self) -> Result<StateStmt, RenderError> {
        if !matches!(self.kind(), Some(T::Id | T::EdgeState)) {
            return Err(self.fail("'ID', 'EDGE_STATE'"));
        }
        let id: JsText = self.tokens.texts[self.i].trim_js().into();
        self.i += 1;
        let mut state = StateStmt::new(id, Some(StateType::Default));
        state.description = Some(Description::Text(JsText::default()));
        if self.kind() == Some(T::StyleSeparator) {
            self.i += 1;
            state.classes = Some(vec![self.expect(T::Id)?.trim_js().into()]);
        }
        Ok(state)
    }
}

/// Pele's `parseState`: builds the statement tree into the db and hands it the root document.
pub(super) fn parse(cx: &ParseContext<'_>, db: &mut StateDb) -> Result<(), RenderError> {
    let mut p = Parser { cx, tokens: tokenize(cx.text), i: 0 };
    while p.kind() == Some(T::Nl) {
        p.i += 1;
    }
    if p.kind() != Some(T::Sd) {
        return Err(p.fail("'NL', 'SD'"));
    }
    p.i += 1;

    let mut doc = ROOT;
    let mut outer: Vec<DocId> = Vec::new();
    // The grammar drops a statement whose value is the string it uses for a blank line.
    let add = |db: &mut StateDb, doc: DocId, value: Stmt| {
        if !matches!(&value, Stmt::Text(text) if text.is_str("nl")) {
            db.docs[doc.0].push(value);
        }
    };
    let open = |p: &mut Parser<'_, '_>, db: &mut StateDb, doc: &mut DocId, outer: &mut Vec<DocId>, id, description| {
        p.i += 1;
        let inner = db.doc();
        let mut state = StateStmt::new(id, Some(StateType::Default));
        state.description = Some(Description::Text(description));
        state.doc = Some(inner);
        db.docs[doc.0].push(Stmt::State(state));
        outer.push(*doc);
        *doc = inner;
    };
    let unexpected = |outer: &[DocId]| {
        if outer.is_empty() { format!("'$end', {STATEMENT}") } else { format!("'STRUCT_STOP', {STATEMENT}") }
    };

    loop {
        let Some(t) = p.kind() else {
            unreachable!("the token stream ends with END or ERROR, where parsing stops");
        };
        match t {
            T::Nl => p.i += 1,
            T::Id | T::EdgeState => {
                let mut first = p.id_statement()?;
                if p.kind() == Some(T::Descr) {
                    first.description = Some(Description::Text(trim_colon(&p.tokens.texts[p.i])));
                    p.i += 1;
                    add(db, doc, Stmt::State(first));
                } else if p.kind() == Some(T::Arrow) {
                    p.i += 1;
                    let second = p.id_statement()?;
                    let description = if p.kind() == Some(T::Descr) {
                        p.i += 1;
                        Some(trim_colon(&p.tokens.texts[p.i - 1]))
                    } else {
                        None
                    };
                    add(db, doc, Stmt::Relation { state1: first, state2: second, description });
                } else {
                    add(db, doc, Stmt::State(first));
                }
            }
            T::HideEmpty => {
                add(db, doc, Stmt::Text(p.text(p.i)));
                p.i += 1;
            }
            T::Scale => {
                p.i += 1;
                p.expect(T::Width)?;
                add(db, doc, Stmt::Text(p.text(p.i - 2)));
            }
            T::CompositState => {
                if p.tokens.types.get(p.i + 1) == Some(&T::StructStart) {
                    let id = p.text(p.i);
                    p.i += 1;
                    open(&mut p, db, &mut doc, &mut outer, id, JsText::default());
                } else {
                    add(db, doc, Stmt::Text(p.text(p.i)));
                    p.i += 1;
                }
            }
            T::StateDescr => {
                let description = p.text(p.i);
                p.i += 1;
                p.expect(T::As)?;
                let id = p.expect(T::Id)?;
                if p.kind() == Some(T::StructStart) {
                    open(&mut p, db, &mut doc, &mut outer, id, description);
                } else if let Some(colon) = id.find_unit(58, 0) {
                    let parts = split_unit(&id, 58);
                    let mut state = StateStmt::new(id[..colon].into(), Some(StateType::Default));
                    let lines = vec![description.trim_js().into(), parts[1].into()];
                    state.description = Some(Description::Lines(lines));
                    add(db, doc, Stmt::State(state));
                } else {
                    let mut state = StateStmt::new(id, Some(StateType::Default));
                    state.description = Some(Description::Text(description.trim_js().into()));
                    add(db, doc, Stmt::State(state));
                }
            }
            T::Fork | T::Join | T::Choice => {
                let kind = match t {
                    T::Fork => StateType::Fork,
                    T::Join => StateType::Join,
                    _ => StateType::Choice,
                };
                add(db, doc, Stmt::State(StateStmt::new(p.text(p.i), Some(kind))));
                p.i += 1;
            }
            T::Concurrent => {
                p.i += 1;
                let id = db.divider_id();
                add(db, doc, Stmt::State(StateStmt::new(id, Some(StateType::Divider))));
            }
            T::Note => {
                let keyword = p.text(p.i);
                p.i += 1;
                if matches!(p.kind(), Some(T::LeftOf | T::RightOf)) {
                    let position: JsText = p.tokens.texts[p.i].trim_js().into();
                    p.i += 1;
                    let id: JsText = p.expect(T::Id)?.trim_js().into();
                    let text: JsText = p.expect(T::NoteText)?.trim_js().into();
                    let mut state = StateStmt::new(id, None);
                    state.note = Some(StateNote { position, text });
                    add(db, doc, Stmt::State(state));
                } else {
                    if p.kind() != Some(T::NoteText) {
                        return Err(p.fail("'NOTE_TEXT', 'left_of', 'right_of'"));
                    }
                    p.i += 1;
                    p.expect(T::As)?;
                    p.expect(T::Id)?;
                    add(db, doc, Stmt::Text(keyword));
                }
            }
            T::DirectionTb | T::DirectionBt | T::DirectionRl | T::DirectionLr => {
                let at = DIRECTIONS.iter().position(|&kind| kind == t).expect("a direction token");
                let value = [Direction::TB, Direction::BT, Direction::RL, Direction::LR][at];
                p.i += 1;
                add(db, doc, Stmt::Dir(value));
            }
            T::AccTitle => {
                p.i += 1;
                let value: JsText = p.expect(T::AccTitleValue)?.trim_js().into();
                db.set_acc_title(&value);
                add(db, doc, Stmt::Text(value));
            }
            T::AccDescr => {
                p.i += 1;
                let value: JsText = p.expect(T::AccDescrValue)?.trim_js().into();
                db.set_acc_description(&value);
                add(db, doc, Stmt::Text(value));
            }
            T::AccDescrMultilineValue => {
                let value: JsText = p.tokens.texts[p.i].trim_js().into();
                p.i += 1;
                db.set_acc_description(&value);
                add(db, doc, Stmt::Text(value));
            }
            T::Click => {
                p.i += 1;
                let id = p.id_statement()?;
                let (url, tooltip);
                if p.kind() == Some(T::Href) {
                    p.i += 1;
                    url = p.expect(T::Str)?;
                    tooltip = JsText::default();
                } else {
                    if p.kind() != Some(T::Str) {
                        return Err(p.fail("'STRING', 'HREF'"));
                    }
                    url = p.text(p.i);
                    p.i += 1;
                    tooltip = p.expect(T::Str)?;
                }
                p.expect(T::Nl)?;
                add(db, doc, Stmt::Click { id, url, tooltip });
            }
            T::ClassDef => {
                p.i += 1;
                let id = p.expect(T::ClassDefId)?.trim_js().into();
                let classes = p.expect(T::ClassDefStyleOpts)?.trim_js().into();
                add(db, doc, Stmt::ClassDef { id, classes });
            }
            T::Style => {
                p.i += 1;
                let id = p.expect(T::StyleIds)?.trim_js().into();
                let style_class = p.expect(T::StyleDefStyleOpts)?.trim_js().into();
                add(db, doc, Stmt::Style { id, style_class });
            }
            T::Class => {
                p.i += 1;
                let id = p.expect(T::ClassEntityIds)?.trim_js().into();
                let style_class = p.expect(T::StyleClass)?.trim_js().into();
                add(db, doc, Stmt::ApplyClass { id, style_class });
            }
            T::StructStop => {
                let Some(enclosing) = outer.pop() else {
                    return Err(p.fail(&unexpected(&outer)));
                };
                p.i += 1;
                doc = enclosing;
            }
            T::End => {
                if !outer.is_empty() {
                    return Err(p.fail(&unexpected(&outer)));
                }
                return Ok(());
            }
            _ => return Err(p.fail(&unexpected(&outer))),
        }
    }
}
