//! The sequence parser, ported from Pele's `src/diagrams/sequence/parser.ts`: a recursive descent
//! over the lexer's tokens that gives the statements Mermaid's grammar gives its `yy` object.
//!
//! Pele nests the statements of a block in a list and walks the lists once the source has parsed.
//! The walk visits them in source order, so the statements are kept here in one list in that order.

use super::db::{SeqDb, extract_wrap, parse_box_data};
use super::lexer::{T, Token, tokenize};
use super::{Central, LineType, NoteActors, Placement, SeqText};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units, string_to_number};

/// What a box line gives: Pele's `SeqBoxData`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SeqBoxData {
    pub(super) text: Option<JsText>,
    pub(super) color: JsText,
    pub(super) wrap: Option<bool>,
}

/// How a `participant` or `actor` statement asks for its participant to be drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Draw {
    Participant,
    Actor,
}

/// A participant as a statement names it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Named {
    pub(super) actor: JsText,
    pub(super) draw: Option<Draw>,
    pub(super) description: Option<SeqText>,
    pub(super) config: Option<JsText>,
}

impl Named {
    fn only(actor: JsText) -> Self {
        Named { actor, draw: None, description: None, config: None }
    }
}

/// The text of a participant's menu statement, by the statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Menu {
    Links,
    Link,
    Properties,
    Details,
}

/// One call the grammar makes on the model builder.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Statement {
    AddParticipant(Named),
    CreateParticipant(Named),
    DestroyParticipant(JsText),
    /// The start or end of an activation, or a central connection, of one participant.
    Mark {
        actor: JsText,
        kind: LineType,
    },
    AddNote {
        actor: NoteActors,
        placement: Placement,
        text: SeqText,
    },
    Menu {
        menu: Menu,
        actor: JsText,
        text: SeqText,
    },
    AddMessage {
        from: JsText,
        to: JsText,
        kind: LineType,
        msg: SeqText,
        activate: bool,
        central: Option<Central>,
    },
    BoxStart(SeqBoxData),
    BoxEnd,
    /// The start of a block or of one of its sections.
    Block {
        text: SeqText,
        kind: LineType,
    },
    BlockEnd(LineType),
    Numbering {
        start: Option<f64>,
        step: Option<f64>,
        visible: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum K {
    Top,
    Box,
    Loop,
    Rect,
    Opt,
    Break,
    Alt,
    Par,
    Critical,
}

fn signal_type(token: T) -> Option<LineType> {
    Some(match token {
        T::SolidOpenArrow => LineType::SolidOpen,
        T::DottedOpenArrow => LineType::DottedOpen,
        T::SolidArrow => LineType::Solid,
        T::DottedArrow => LineType::Dotted,
        T::BidirectionalSolidArrow => LineType::BidirectionalSolid,
        T::BidirectionalDottedArrow => LineType::BidirectionalDotted,
        T::SolidCross => LineType::SolidCross,
        T::DottedCross => LineType::DottedCross,
        T::SolidPoint => LineType::SolidPoint,
        T::DottedPoint => LineType::DottedPoint,
        T::SolidArrowTop => LineType::SolidTop,
        T::SolidArrowBottom => LineType::SolidBottom,
        T::StickArrowTop => LineType::StickTop,
        T::StickArrowBottom => LineType::StickBottom,
        T::SolidArrowTopDotted => LineType::SolidTopDotted,
        T::SolidArrowBottomDotted => LineType::SolidBottomDotted,
        T::StickArrowTopDotted => LineType::StickTopDotted,
        T::StickArrowBottomDotted => LineType::StickBottomDotted,
        T::SolidArrowTopReverse => LineType::SolidArrowTopReverse,
        T::SolidArrowBottomReverse => LineType::SolidArrowBottomReverse,
        T::StickArrowTopReverse => LineType::StickArrowTopReverse,
        T::StickArrowBottomReverse => LineType::StickArrowBottomReverse,
        T::SolidArrowTopReverseDotted => LineType::SolidArrowTopReverseDotted,
        T::SolidArrowBottomReverseDotted => LineType::SolidArrowBottomReverseDotted,
        T::StickArrowTopReverseDotted => LineType::StickArrowTopReverseDotted,
        T::StickArrowBottomReverseDotted => LineType::StickArrowBottomReverseDotted,
        _ => return None,
    })
}

/// The kind of block a keyword opens, and the entry its first section starts with. A box holds
/// participants and starts with no entry.
fn block(token: T) -> Option<(K, Option<LineType>)> {
    Some(match token {
        T::Loop => (K::Loop, Some(LineType::LoopStart)),
        T::Rect => (K::Rect, Some(LineType::RectStart)),
        T::Opt => (K::Opt, Some(LineType::OptStart)),
        T::Break => (K::Break, Some(LineType::BreakStart)),
        T::Alt => (K::Alt, Some(LineType::AltStart)),
        T::Par => (K::Par, Some(LineType::ParStart)),
        T::ParOver => (K::Par, Some(LineType::ParOverStart)),
        T::Critical => (K::Critical, Some(LineType::CriticalStart)),
        T::Box => (K::Box, None),
        _ => return None,
    })
}

fn block_end(kind: K) -> LineType {
    match kind {
        K::Loop => LineType::LoopEnd,
        K::Rect => LineType::RectEnd,
        K::Opt => LineType::OptEnd,
        K::Break => LineType::BreakEnd,
        K::Alt => LineType::AltEnd,
        K::Par => LineType::ParEnd,
        K::Critical => LineType::CriticalEnd,
        K::Top | K::Box => unreachable!("only a block of messages ends in an entry"),
    }
}

const SIGNAL_TOKENS: &str = "'SOLID_OPEN_ARROW', 'DOTTED_OPEN_ARROW', 'SOLID_ARROW', 'DOTTED_ARROW', 'SOLID_CROSS', 'DOTTED_CROSS', 'SOLID_POINT', 'DOTTED_POINT'";

struct Parser<'a> {
    tokens: &'a [Token],
    i: usize,
    out: Vec<Statement>,
}

type Parsed<V> = Result<V, SyntaxError>;

impl Parser<'_> {
    fn kind(&self) -> T {
        self.tokens[self.i].kind
    }

    fn fail<V>(&self, expected: &str) -> Parsed<V> {
        let token = &self.tokens[self.i];
        if token.kind == T::Error {
            return Err(SyntaxError { at: token.start, detail: String::new(), lexical: true });
        }
        let detail = format!("Expecting {expected}, got '{}'", token.kind.name());
        Err(SyntaxError { at: token.start, detail, lexical: false })
    }

    fn expect(&mut self, kind: T) -> Parsed<&JsText> {
        if self.kind() != kind {
            return self.fail(&format!("'{}'", kind.name()));
        }
        self.i += 1;
        Ok(&self.tokens[self.i - 1].text)
    }

    fn actor(&mut self) -> Parsed<JsText> {
        if !matches!(self.kind(), T::Actor | T::Link) {
            return self.fail("'ACTOR'");
        }
        self.i += 1;
        Ok(self.tokens[self.i - 1].text.clone())
    }

    fn text2(&mut self) -> Parsed<SeqText> {
        let text = self.expect(T::Txt)?.trim_js();
        Ok(extract_wrap(&text[1.min(text.len())..]))
    }

    /// A `participant`, `actor` or `destroy` statement, from its keyword.
    fn participant(&mut self, create: bool) -> Parsed<Statement> {
        let keyword = self.kind();
        self.i += 1;
        if keyword == T::Destroy {
            let actor = self.actor()?;
            self.expect(T::Newline)?;
            return Ok(if create {
                Statement::CreateParticipant(Named::only(actor))
            } else {
                Statement::DestroyParticipant(actor)
            });
        }
        let named_by = self.kind();
        let mut node = Named::only(self.actor()?);
        if self.kind() == T::ConfigStart && named_by == T::Actor {
            self.i += 1;
            let content = self.expect(T::ConfigContent)?.trim_js().into();
            self.expect(T::ConfigEnd)?;
            node.config = Some(content);
        }
        node.draw = Some(if keyword == T::Participant { Draw::Participant } else { Draw::Actor });
        if self.kind() == T::As {
            self.i += 1;
            let alias = extract_wrap(self.expect(T::RestOfLine)?);
            self.expect(T::Newline)?;
            node.description = Some(alias);
        } else if self.kind() == T::Newline {
            self.i += 1;
        } else {
            return self.fail("'AS', 'NEWLINE'");
        }
        Ok(if create { Statement::CreateParticipant(node) } else { Statement::AddParticipant(node) })
    }

    fn signal(&mut self, from: JsText) -> Parsed<()> {
        let central_from = self.kind() == T::Central;
        if central_from {
            self.i += 1;
        }
        let Some(kind) = signal_type(self.kind()) else {
            return self.fail(SIGNAL_TOKENS);
        };
        self.i += 1;
        let mark = self.kind();
        if mark == T::Central || (!central_from && matches!(mark, T::Plus | T::Minus)) {
            self.i += 1;
        }
        let to = self.actor()?;
        let msg = self.text2()?;
        let mark_of = |actor: &JsText, kind| Statement::Mark { actor: actor.clone(), kind };
        let (activate, central, after) = if central_from {
            let dual = mark == T::Central;
            let mut after = Vec::new();
            if dual {
                after.push(mark_of(&to, LineType::CentralConnection));
            }
            after.push(mark_of(&from, LineType::CentralConnectionReverse));
            (dual, Some(if dual { Central::Both } else { Central::From }), after)
        } else {
            match mark {
                T::Plus => (true, None, vec![mark_of(&to, LineType::ActiveStart)]),
                T::Minus => (false, None, vec![mark_of(&from, LineType::ActiveEnd)]),
                T::Central => (true, Some(Central::To), vec![mark_of(&to, LineType::CentralConnection)]),
                _ => (false, None, Vec::new()),
            }
        };
        self.out.push(Statement::AddParticipant(Named::only(from.clone())));
        self.out.push(Statement::AddParticipant(Named::only(to.clone())));
        self.out.push(Statement::AddMessage { from, to, kind, msg, activate, central });
        self.out.extend(after);
        Ok(())
    }

    fn menu(&mut self, menu: Menu) -> Parsed<()> {
        self.i += 1;
        let actor = self.actor()?;
        let text = self.text2()?;
        self.out.push(Statement::AddParticipant(Named::only(actor.clone())));
        self.out.push(Statement::Menu { menu, actor, text });
        Ok(())
    }

    fn note(&mut self) -> Parsed<()> {
        self.i += 1;
        let placed = self.kind();
        if placed == T::Over {
            self.i += 1;
            let first = self.actor()?;
            let mut second = None;
            if self.kind() == T::Comma {
                self.i += 1;
                second = Some(self.actor()?);
            }
            let text = self.text2()?;
            self.out.push(Statement::AddParticipant(Named::only(first.clone())));
            if let Some(second) = &second {
                self.out.push(Statement::AddParticipant(Named::only(second.clone())));
            }
            let actor = NoteActors::Span(first.clone(), second.unwrap_or(first));
            self.out.push(Statement::AddNote { actor, placement: Placement::Over, text });
            return Ok(());
        }
        if !matches!(placed, T::LeftOf | T::RightOf) {
            return self.fail("'over', 'left_of', 'right_of'");
        }
        self.i += 1;
        let who = self.actor()?;
        let placement = if placed == T::LeftOf { Placement::LeftOf } else { Placement::RightOf };
        let text = self.text2()?;
        self.out.push(Statement::AddParticipant(Named::only(who.clone())));
        self.out.push(Statement::AddNote { actor: NoteActors::One(who), placement, text });
        Ok(())
    }

    fn autonumber(&mut self) -> Parsed<()> {
        self.i += 1;
        let (mut start, mut step, mut visible) = (None, None, true);
        if self.kind() == T::Num {
            start = Some(string_to_number(&self.tokens[self.i].text));
            self.i += 1;
            step = Some(1.0);
            if self.kind() == T::Num {
                step = Some(string_to_number(&self.tokens[self.i].text));
                self.i += 1;
            }
        } else if self.kind() == T::Off {
            self.i += 1;
            visible = false;
        }
        if self.kind() != T::Newline {
            return self.fail("'NEWLINE', 'NUM', 'off'");
        }
        self.i += 1;
        self.out.push(Statement::Numbering { start, step, visible });
        Ok(())
    }

    fn document(&mut self, db: &mut SeqDb) -> Parsed<()> {
        while self.kind() == T::Newline {
            self.i += 1;
        }
        self.expect(T::Sd)?;

        // The kind of each open block, the innermost last.
        let mut open: Vec<K> = Vec::new();
        loop {
            let t = self.kind();
            let frame = open.last().copied().unwrap_or(K::Top);
            if frame == K::Box {
                match t {
                    T::Participant | T::ParticipantActor | T::Destroy => {
                        let statement = self.participant(false)?;
                        self.out.push(statement);
                    }
                    T::Newline => self.i += 1,
                    T::EndKw => {
                        self.i += 1;
                        open.pop();
                        self.out.push(Statement::BoxEnd);
                    }
                    _ => return self.fail("'end', 'NEWLINE', 'participant', 'participant_actor', 'destroy'"),
                }
                continue;
            }
            match t {
                T::Newline | T::Invalid => self.i += 1,
                T::Participant | T::ParticipantActor | T::Destroy => {
                    let statement = self.participant(false)?;
                    self.out.push(statement);
                }
                T::Create => {
                    self.i += 1;
                    if !matches!(self.kind(), T::Participant | T::ParticipantActor | T::Destroy) {
                        return self.fail("'participant', 'participant_actor', 'destroy'");
                    }
                    let statement = self.participant(true)?;
                    self.out.push(statement);
                }
                T::Actor => {
                    let from = self.actor()?;
                    self.signal(from)?;
                    self.expect(T::Newline)?;
                }
                T::Link => {
                    if matches!(self.tokens[self.i + 1].kind, T::Actor | T::Link) {
                        self.menu(Menu::Link)?;
                    } else {
                        let from = self.actor()?;
                        self.signal(from)?;
                    }
                    self.expect(T::Newline)?;
                }
                T::Links | T::Properties | T::Details => {
                    self.menu(match t {
                        T::Links => Menu::Links,
                        T::Properties => Menu::Properties,
                        _ => Menu::Details,
                    })?;
                    self.expect(T::Newline)?;
                }
                T::Note => {
                    self.note()?;
                    self.expect(T::Newline)?;
                }
                T::Autonumber => self.autonumber()?,
                T::Activate | T::Deactivate => {
                    self.i += 1;
                    let actor = self.actor()?;
                    self.expect(T::Newline)?;
                    let kind = if t == T::Activate { LineType::ActiveStart } else { LineType::ActiveEnd };
                    self.out.push(Statement::Mark { actor, kind });
                }
                T::Title | T::LegacyTitle => {
                    let text = &self.tokens[self.i].text;
                    db.model.title = Some(text[if t == T::Title { 6 } else { 7 }.min(text.len())..].into());
                    self.i += 1;
                }
                T::AccTitle => {
                    self.i += 1;
                    let value = self.expect(T::AccTitleValue)?.trim_js();
                    db.set_acc_title(value);
                }
                T::AccDescr => {
                    self.i += 1;
                    let value = self.expect(T::AccDescrValue)?.trim_js();
                    db.set_acc_description(value);
                }
                T::AccDescrMultilineValue => {
                    db.set_acc_description(self.tokens[self.i].text.trim_js());
                    self.i += 1;
                }
                T::Loop | T::Rect | T::Opt | T::Break | T::Alt | T::Par | T::ParOver | T::Critical | T::Box => {
                    self.i += 1;
                    let text = self.expect(T::RestOfLine)?.clone();
                    let (kind, start) = block(t).expect("a block keyword");
                    open.push(kind);
                    self.out.push(match start {
                        Some(kind) => Statement::Block { text: extract_wrap(&text), kind },
                        None => Statement::BoxStart(parse_box_data(&text)),
                    });
                }
                T::Else | T::And | T::Option => {
                    let (within, divider) = match t {
                        T::Else => (K::Alt, LineType::AltElse),
                        T::And => (K::Par, LineType::ParAnd),
                        _ => (K::Critical, LineType::CriticalOption),
                    };
                    if frame != within {
                        return self.fail("'end'");
                    }
                    self.i += 1;
                    let text = extract_wrap(self.expect(T::RestOfLine)?);
                    self.out.push(Statement::Block { text, kind: divider });
                }
                T::EndKw => {
                    if frame == K::Top {
                        return self.fail("'NEWLINE', 'EOF'");
                    }
                    self.i += 1;
                    open.pop();
                    self.out.push(Statement::BlockEnd(block_end(frame)));
                }
                T::End => {
                    if frame != K::Top {
                        return self.fail("'end'");
                    }
                    return Ok(());
                }
                _ => return self.fail("'NEWLINE', 'ACTOR', 'participant', 'note', 'end'"),
            }
        }
    }
}

/// Pele's `parseSequence` up to its call of `apply`: the statements of a source in order. The
/// title and the accessible texts are set on the way, as Pele sets them.
pub(super) fn parse(src: &[u16], db: &mut SeqDb) -> Result<Vec<Statement>, SyntaxError> {
    let tokens = tokenize(src);
    let mut parser = Parser { tokens: &tokens, i: 0, out: Vec::new() };
    parser.document(db)?;
    Ok(parser.out)
}
