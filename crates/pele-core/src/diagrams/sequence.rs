//! Sequence diagrams, ported from Pele's `src/diagrams/sequence`. The lexer and the parser turn a
//! source into statements, the db applies them to build Pele's model, and `render` places the
//! lifelines and draws what passes between them.

mod actors;
mod colors;
mod db;
pub(crate) mod json;
mod lexer;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsText, JsValue};
use indexmap::IndexMap;

pub(crate) use render::{Sequence, number_messages};

/// The kinds of entry in a diagram's list of messages, numbered as in Mermaid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum LineType {
    Solid = 0,
    Dotted = 1,
    Note = 2,
    SolidCross = 3,
    DottedCross = 4,
    SolidOpen = 5,
    DottedOpen = 6,
    LoopStart = 10,
    LoopEnd = 11,
    AltStart = 12,
    AltElse = 13,
    AltEnd = 14,
    OptStart = 15,
    OptEnd = 16,
    ActiveStart = 17,
    ActiveEnd = 18,
    ParStart = 19,
    ParAnd = 20,
    ParEnd = 21,
    RectStart = 22,
    RectEnd = 23,
    SolidPoint = 24,
    DottedPoint = 25,
    Autonumber = 26,
    CriticalStart = 27,
    CriticalOption = 28,
    CriticalEnd = 29,
    BreakStart = 30,
    BreakEnd = 31,
    ParOverStart = 32,
    BidirectionalSolid = 33,
    BidirectionalDotted = 34,
    SolidTop = 41,
    SolidBottom = 42,
    StickTop = 43,
    StickBottom = 44,
    SolidArrowTopReverse = 45,
    SolidArrowBottomReverse = 46,
    StickArrowTopReverse = 47,
    StickArrowBottomReverse = 48,
    SolidTopDotted = 51,
    SolidBottomDotted = 52,
    StickTopDotted = 53,
    StickBottomDotted = 54,
    SolidArrowTopReverseDotted = 55,
    SolidArrowBottomReverseDotted = 56,
    StickArrowTopReverseDotted = 57,
    StickArrowBottomReverseDotted = 58,
    CentralConnection = 59,
    CentralConnectionReverse = 60,
}

impl LineType {
    pub(crate) fn number(self) -> u8 {
        self as u8
    }
}

/// Which ends of a message stop at a circle on the lifeline: Mermaid's central connections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Central {
    /// At the receiver.
    To,
    /// At the sender.
    From,
    Both,
}

impl Central {
    /// The number Mermaid gives it.
    pub(crate) fn number(self) -> u8 {
        match self {
            Central::To => 59,
            Central::From => 60,
            Central::Both => 61,
        }
    }

    pub(crate) fn at_receiver(self) -> bool {
        matches!(self, Central::To | Central::Both)
    }

    pub(crate) fn at_sender(self) -> bool {
        matches!(self, Central::From | Central::Both)
    }
}

/// Where a note sits against its participants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    LeftOf,
    RightOf,
    Over,
}

impl Placement {
    pub(crate) fn number(self) -> u8 {
        match self {
            Placement::LeftOf => 0,
            Placement::RightOf => 1,
            Placement::Over => 2,
        }
    }
}

/// Text with what its `wrap:` or `nowrap:` prefix asks for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeqText {
    pub(crate) text: JsText,
    pub(crate) wrap: Option<bool>,
}

/// Pele's `SeqBox`: a box around participants.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeqBox {
    pub(crate) name: Option<JsText>,
    pub(crate) wrap: bool,
    pub(crate) fill: JsText,
    pub(crate) actor_keys: Vec<JsText>,
}

/// Pele's `SeqActor`: a participant.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeqActor {
    /// Its box, by the box's place in the model.
    pub(crate) in_box: Option<usize>,
    pub(crate) name: JsText,
    pub(crate) description: JsText,
    pub(crate) wrap: bool,
    pub(crate) prev_actor: Option<JsText>,
    pub(crate) next_actor: Option<JsText>,
    pub(crate) links: IndexMap<JsText, JsValue>,
    pub(crate) properties: IndexMap<JsText, JsValue>,
    /// The id of the page element named by a `details` statement.
    pub(crate) details: Option<JsText>,
    /// The kind of participant as the source names it, which may be no kind Pele draws.
    pub(crate) kind: JsText,
}

/// Pele's `SeqNumbering`: what an `autonumber` statement sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SeqNumbering {
    pub(crate) start: Option<f64>,
    pub(crate) step: Option<f64>,
    pub(crate) visible: bool,
}

/// What one entry of the message list is, with the fields only that kind has.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Entry {
    /// A message line, an activation, or the start, divider or end of a block.
    Signal {
        kind: LineType,
        activate: bool,
        central: Option<Central>,
    },
    Note {
        placement: Placement,
    },
    Numbering(SeqNumbering),
}

/// Pele's `SeqMessage`. Its id is its place in the list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeqMessage {
    pub(crate) from: Option<JsText>,
    pub(crate) to: Option<JsText>,
    /// The text of a message or a note, and of a block's title. Empty for a numbering.
    pub(crate) text: JsText,
    pub(crate) wrap: bool,
    pub(crate) entry: Entry,
}

impl SeqMessage {
    /// Pele's `type`.
    pub(crate) fn kind(&self) -> LineType {
        match self.entry {
            Entry::Signal { kind, .. } => kind,
            Entry::Note { .. } => LineType::Note,
            Entry::Numbering(_) => LineType::Autonumber,
        }
    }
}

/// The participants a note names.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NoteActors {
    One(JsText),
    Span(JsText, JsText),
}

/// Pele's `SeqNote`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeqNote {
    pub(crate) actor: NoteActors,
    pub(crate) placement: Placement,
    pub(crate) message: JsText,
    pub(crate) wrap: bool,
}

/// Pele's `SequenceModel`, as its `SeqDb` leaves it.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct SequenceModel {
    pub(crate) actors: IndexMap<JsText, SeqActor>,
    /// Each created participant with the number of messages before its creation.
    pub(crate) created_actors: IndexMap<JsText, usize>,
    pub(crate) destroyed_actors: IndexMap<JsText, usize>,
    pub(crate) boxes: Vec<SeqBox>,
    pub(crate) messages: Vec<SeqMessage>,
    pub(crate) notes: Vec<SeqNote>,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
}

/// Pele's sequence `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<SequenceModel, RenderError> {
    let section = cx.config.get("sequence").and_then(JsValue::as_record);
    let wrap = match cx.config.get("wrap") {
        Some(&JsValue::Bool(wrap)) => Some(wrap),
        _ => None,
    };
    let sequence_wrap = section.and_then(|section| section.get("wrap")) == Some(&JsValue::Bool(true));
    let mut db = db::SeqDb::new(wrap, sequence_wrap);
    db.model.title = cx.title.filter(|title| !title.is_empty()).cloned();
    let statements = parser::parse(cx.text, &mut db).map_err(|error| cx.syntax(error))?;
    for statement in statements {
        db.apply(statement).map_err(|error| match error {
            db::DbError::Semantic(message) => cx.semantic(message),
            db::DbError::Config(message) => {
                RenderError::Diagnostic(crate::Diagnostic::new(crate::ErrorCode::Syntax, message))
            }
            db::DbError::TooDeep => crate::input::nested_too_deeply(),
        })?;
    }
    Ok(db.model)
}
