//! State diagrams, ported from Pele's `src/diagrams/state`. The lexer and the parser build
//! Mermaid's statement tree, the db reads it into states, relations and classes, `graph` resolves
//! it into the nodes and edges that are drawn, and `render` lays those out and draws them.

mod db;
mod graph;
pub(crate) mod json;
mod lexer;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;
use indexmap::IndexMap;

pub(crate) use graph::{GraphEdge, GraphNode, StateGraph};
pub(crate) use render::State;

/// A `direction` statement's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    TB,
    BT,
    RL,
    LR,
}

impl Direction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Direction::TB => "TB",
            Direction::BT => "BT",
            Direction::RL => "RL",
            Direction::LR => "LR",
        }
    }
}

/// What a statement declares a state to be: Pele's `type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateType {
    Default,
    Fork,
    Join,
    Choice,
    /// A region of a composite, between `--` lines.
    Divider,
}

impl StateType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            StateType::Default => "default",
            StateType::Fork => "fork",
            StateType::Join => "join",
            StateType::Choice => "choice",
            StateType::Divider => "divider",
        }
    }
}

/// A state's description as a statement gives it: one text, or a title and the lines under it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Description {
    Text(JsText),
    Lines(Vec<JsText>),
}

impl Description {
    /// JavaScript's `length`: code units of a text, items of a list.
    fn length(&self) -> usize {
        match self {
            Description::Text(text) => text.len(),
            Description::Lines(lines) => lines.len(),
        }
    }

    /// Whether JavaScript reads it as true: any list, and a text that is not empty.
    fn truthy(&self) -> bool {
        match self {
            Description::Text(text) => !text.is_empty(),
            Description::Lines(_) => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateNote {
    /// `left of` or `right of`.
    pub(crate) position: JsText,
    pub(crate) text: JsText,
}

/// A document: the statements of the diagram or of one composite state, by its place in
/// [`StateModel::docs`]. Composites nest thousands deep, so documents are not nested values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocId(pub(crate) usize);

/// Pele's `StateStmt` as the statement tree holds it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateStmt {
    pub(crate) id: JsText,
    pub(crate) kind: Option<StateType>,
    pub(crate) description: Option<Description>,
    pub(crate) doc: Option<DocId>,
    pub(crate) note: Option<StateNote>,
    /// Whether a `[*]` is where its document starts, rather than where it ends.
    pub(crate) start: Option<bool>,
    pub(crate) classes: Option<Vec<JsText>>,
}

impl StateStmt {
    fn new(id: JsText, kind: Option<StateType>) -> Self {
        StateStmt { id, kind, description: None, doc: None, note: None, start: None, classes: None }
    }
}

/// Pele's `Stmt`. Mermaid's grammar also leaves the text of statements that have no effect in a
/// document (`hide empty description`, `scale`, a bare `state name`, a floating note).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Stmt {
    Text(JsText),
    State(StateStmt),
    Relation { state1: StateStmt, state2: StateStmt, description: Option<JsText> },
    ClassDef { id: JsText, classes: JsText },
    Style { id: JsText, style_class: JsText },
    ApplyClass { id: JsText, style_class: JsText },
    Dir(Direction),
    Click { id: StateStmt, url: JsText, tooltip: JsText },
}

/// A state as the db records it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DbState {
    pub(crate) id: JsText,
    pub(crate) descriptions: Vec<JsText>,
    pub(crate) kind: StateType,
    pub(crate) doc: Option<DocId>,
    pub(crate) note: Option<StateNote>,
    pub(crate) classes: Vec<JsText>,
    pub(crate) styles: Vec<JsText>,
    pub(crate) text_styles: Vec<JsText>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Relation {
    pub(crate) id1: JsText,
    pub(crate) id2: JsText,
    pub(crate) title: JsText,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassDef {
    pub(crate) id: JsText,
    pub(crate) styles: Vec<JsText>,
    pub(crate) text_styles: Vec<JsText>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateLink {
    pub(crate) url: JsText,
    pub(crate) tooltip: JsText,
}

/// Pele's `StateDb` once it has parsed a source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateModel {
    /// Every document. The diagram's own is the first.
    pub(crate) docs: Vec<Vec<Stmt>>,
    pub(crate) states: IndexMap<JsText, DbState>,
    pub(crate) relations: Vec<Relation>,
    pub(crate) classes: IndexMap<JsText, ClassDef>,
    pub(crate) links: IndexMap<JsText, StateLink>,
    pub(crate) graph: StateGraph,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
}

pub(crate) const ROOT: DocId = DocId(0);

/// The direction a document is laid out in: its last `direction` statement, or top to bottom.
pub(crate) fn direction_of(doc: &[Stmt]) -> Direction {
    doc.iter().fold(Direction::TB, |dir, item| if let Stmt::Dir(value) = item { *value } else { dir })
}

impl StateModel {
    /// The direction the root document is laid out in.
    pub(crate) fn direction(&self) -> Direction {
        direction_of(&self.docs[ROOT.0])
    }
}

/// Pele's state `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<StateModel, RenderError> {
    let mut db = db::StateDb::new();
    db.title = cx.title.filter(|title| !title.is_empty()).cloned();
    parser::parse(cx, &mut db)?;
    db.finish(cx)
}
