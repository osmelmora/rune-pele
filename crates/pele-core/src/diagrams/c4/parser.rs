//! Pele's C4 parser (`src/diagrams/c4/parser.ts`). It accepts what the parser Jison generates
//! from `c4Diagram.jison` accepts, and calls the db in the order its semantic actions do.
//! Boundaries nest through a counter, not recursion.

use super::db::C4Db;
use super::lexer::{T, Tokens, tokenize};
use super::{Attr, C4Kind, Named, NodeType, RelType, ShapeKind, Store, Tier};
use crate::RenderError;
use crate::catalog::ParseContext;
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units};

const HEADERS: &str = "'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'C4_CONTEXT', 'C4_CONTAINER', 'C4_COMPONENT', 'C4_DYNAMIC', 'C4_DEPLOYMENT'";
const ATTRIBUTES: &str = "'STR', 'STR_KEY', 'ATTRIBUTE', 'ATTRIBUTE_EMPTY'";

/// What may come next.
#[derive(Clone, Copy, PartialEq, Eq)]
enum P {
    /// A statement is required.
    Item,
    /// Only a diagram statement may come next: the first one in a boundary, or one written
    /// straight after a title.
    Diagram,
    ItemOrEnd,
    AfterOther,
    AfterDiagram,
}

fn is_other(t: T) -> bool {
    matches!(t, T::Title | T::AccDescription | T::AccTitle | T::AccDescr | T::AccDescrMultilineValue)
}

fn is_diagram(t: T) -> bool {
    t >= T::Person && t <= T::UpdateLayoutConfig
}

/// Mermaid's `typeC4Shape` for a shape keyword.
fn shape_kind(t: T) -> ShapeKind {
    let index = t as usize - T::Person as usize;
    let (tier, within) = match index {
        0..=1 => (Tier::Person, index),
        2..=7 => (Tier::System, index - 2),
        8..=13 => (Tier::Container, index - 8),
        _ => (Tier::Component, index - 14),
    };
    if tier == Tier::Person {
        return ShapeKind { tier, external: within == 1, store: Store::Plain };
    }
    let store = [Store::Plain, Store::Db, Store::Queue][within % 3];
    ShapeKind { tier, external: within >= 3, store }
}

fn rel_type(t: T) -> RelType {
    match t {
        T::Rel | T::RelIndex => RelType::Rel,
        T::BiRel => RelType::BiRel,
        T::RelU => RelType::Up,
        T::RelD => RelType::Down,
        T::RelL => RelType::Left,
        T::RelR => RelType::Right,
        _ => RelType::Back,
    }
}

fn c4_kind(t: T) -> C4Kind {
    match t {
        T::C4Context => C4Kind::Context,
        T::C4Container => C4Kind::Container,
        T::C4Component => C4Kind::Component,
        T::C4Dynamic => C4Kind::Dynamic,
        _ => C4Kind::Deployment,
    }
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

    fn text(&self, at: usize) -> &[u16] {
        &self.src[self.tokens.starts[at]..self.tokens.ends[at]]
    }

    fn fail(&self, expected: &str) -> SyntaxError {
        let t = self.kind();
        let at = self.tokens.starts[self.i];
        if t == T::Error {
            return SyntaxError { at, detail: String::new(), lexical: true };
        }
        SyntaxError { at, detail: format!("Expecting {expected}, got '{}'", t.name()), lexical: false }
    }

    fn expect(&mut self, kind: T) -> Result<(), SyntaxError> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        self.i += 1;
        Ok(())
    }

    fn attributes(&mut self) -> Result<Vec<Attr>, SyntaxError> {
        let mut attrs = Vec::new();
        loop {
            match self.kind() {
                T::Str => {
                    attrs.push(Attr::Text(self.text(self.i).trim_js().into()));
                    self.i += 1;
                }
                T::AttributeEmpty => {
                    attrs.push(Attr::Text(JsText::default()));
                    self.i += 1;
                }
                T::StrKey => {
                    self.i += 1;
                    if self.kind() != T::StrValue {
                        return Err(self.fail("'STR_VALUE'"));
                    }
                    let key = self.text(self.i - 1).trim_js().into();
                    let value = self.text(self.i).trim_js().into();
                    attrs.push(Attr::Named(Named { key, value }));
                    self.i += 1;
                }
                _ => break,
            }
        }
        if attrs.is_empty() {
            return Err(self.fail(ATTRIBUTES));
        }
        Ok(attrs)
    }

    fn other(&mut self, t: T, db: &mut C4Db) -> Result<(), SyntaxError> {
        match t {
            T::Title => {
                db.set_title(self.text(self.i).get(6..).unwrap_or_default().into());
                self.i += 1;
            }
            T::AccDescription => {
                let descr = self.text(self.i).get(15..).unwrap_or_default().to_vec();
                db.set_acc_description(&descr);
                self.i += 1;
            }
            T::AccDescrMultilineValue => {
                let descr = self.text(self.i).trim_js().to_vec();
                db.set_acc_description(&descr);
                self.i += 1;
            }
            T::AccTitle => {
                self.i += 1;
                if self.kind() != T::AccTitleValue {
                    return Err(self.fail("'acc_title_value'"));
                }
                // The grammar hands `accTitle:` to setTitle, so it becomes the visible title.
                db.set_title(self.text(self.i).trim_js().into());
                self.i += 1;
            }
            _ => {
                self.i += 1;
                if self.kind() != T::AccDescrValue {
                    return Err(self.fail("'acc_descr_value'"));
                }
                let descr = self.text(self.i).trim_js().to_vec();
                db.set_acc_description(&descr);
                self.i += 1;
            }
        }
        Ok(())
    }

    /// One diagram statement. Answers whether it opens a boundary.
    fn diagram(&mut self, t: T, db: &mut C4Db) -> Result<bool, SyntaxError> {
        self.i += 1;
        let mut attrs = self.attributes()?;
        if t <= T::SystemExtQueue {
            db.add_person_or_system(shape_kind(t), &attrs);
        } else if t <= T::ComponentExtQueue {
            db.add_container(shape_kind(t), &attrs);
        } else if t <= T::NodeR {
            let kind = match t {
                T::EnterpriseBoundary => Some("ENTERPRISE"),
                T::SystemBoundary => Some("SYSTEM"),
                T::ContainerBoundary => Some("CONTAINER"),
                _ => None,
            };
            if let Some(kind) = kind {
                // `splice(2, 0, kind)`, which puts it last in a shorter list.
                attrs.insert(attrs.len().min(2), Attr::Text(kind.into()));
            }
            match t {
                T::Node => db.add_deployment_node(NodeType::Node, &attrs),
                T::NodeL => db.add_deployment_node(NodeType::NodeL, &attrs),
                T::NodeR => db.add_deployment_node(NodeType::NodeR, &attrs),
                T::ContainerBoundary => db.add_person_or_system_boundary(&attrs, "container"),
                _ => db.add_person_or_system_boundary(&attrs, "system"),
            }
            match self.kind() {
                T::LBrace => {
                    self.i += 1;
                    self.expect(T::Newline)?;
                }
                T::Newline => {
                    self.i += 1;
                    self.expect(T::LBrace)?;
                    if self.kind() == T::Newline {
                        self.i += 1;
                    }
                }
                _ => return Err(self.fail("'NEWLINE', 'LBRACE'")),
            }
            return Ok(true);
        } else if t <= T::RelIndex {
            // RelIndex takes a sequence number first, which Mermaid ignores.
            if t == T::RelIndex {
                attrs.remove(0);
            }
            db.add_rel(rel_type(t), &attrs);
        } else if t == T::UpdateElStyle {
            db.update_el_style(&attrs);
        } else if t == T::UpdateRelStyle {
            db.update_rel_style(&attrs);
        } else {
            db.update_layout_config(&attrs);
        }
        Ok(false)
    }

    fn run(&mut self, db: &mut C4Db) -> Result<C4Kind, SyntaxError> {
        let header = self.kind();
        if header < T::C4Context || header > T::C4Deployment {
            return Err(self.fail(HEADERS));
        }
        self.i += 1;
        self.expect(T::Newline)?;

        let mut depth = 0usize;
        let mut state = P::Item;
        loop {
            let t = self.kind();
            if state == P::AfterOther || state == P::AfterDiagram {
                if t == T::Newline {
                    self.i += 1;
                    state = P::ItemOrEnd;
                    continue;
                }
                if state == P::AfterOther && is_diagram(t) {
                    state = P::Diagram;
                    continue;
                }
            } else if is_diagram(t) {
                if self.diagram(t, db)? {
                    depth += 1;
                    state = P::Diagram;
                } else {
                    state = P::AfterDiagram;
                }
                continue;
            } else if is_other(t) && state != P::Diagram {
                self.other(t, db)?;
                state = P::AfterOther;
                continue;
            } else if state != P::ItemOrEnd {
                return Err(self.fail(
                    "'title', 'accDescription', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'PERSON', 'REL'",
                ));
            }

            if depth > 0 {
                if t != T::RBrace {
                    return Err(self.fail("'NEWLINE', 'RBRACE'"));
                }
                self.i += 1;
                db.pop_boundary_parse_stack();
                depth -= 1;
                state = P::AfterDiagram;
            } else {
                if t != T::Eof {
                    return Err(self.fail("'NEWLINE', 'EOF'"));
                }
                self.i += 1;
                self.expect(T::End)?;
                return Ok(c4_kind(self.tokens.types[0]));
            }
        }
    }
}

/// Pele's `parseC4`: fills `db` from the text, and answers the header keyword.
pub(super) fn parse(cx: &ParseContext<'_>, db: &mut C4Db) -> Result<C4Kind, RenderError> {
    let mut parser = Parser { src: cx.text, tokens: tokenize(cx.text), i: 0 };
    parser.run(db).map_err(|error| cx.syntax(error))
}
