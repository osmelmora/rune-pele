//! Use case diagrams, ported from Pele's `src/diagrams/usecase`. The lexer and the parser read
//! the statements, `db` resolves them into Pele's model, `build_usecase_graph` turns that into
//! the nodes and edges that are drawn, and `render` lays those out and draws them.
//!
//! Pele's model also keeps Mermaid's syntax tree (`statements`, `source`, `headerSpan`), which
//! nothing it draws reads. This port does not build it.

mod db;
mod graph;
pub(crate) mod json;
mod lexer;
mod parser;
pub(crate) mod view;

use crate::catalog::ParseContext;
use crate::input::JsText;
use crate::{Diagnostic, ErrorCode, Location, RenderError, Written};
use indexmap::IndexMap;
use std::cell::OnceCell;

pub(crate) use graph::build_usecase_graph;

/// Start and end offsets in the text the parser reads, end exclusive.
pub(crate) type Span = [usize; 2];

/// How a label's text is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LabelType {
    Text,
    Markdown,
}

impl LabelType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::Markdown => "markdown",
        }
    }
}

/// How an actor is drawn. `Icon` is what an actor with an icon becomes; a source cannot name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActorType {
    Normal,
    Hollow,
    Awesome,
    Icon,
}

impl ActorType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            ActorType::Normal => "normal",
            ActorType::Hollow => "hollow",
            ActorType::Awesome => "awesome",
            ActorType::Icon => "icon",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UseCaseShape {
    Ellipse,
    Rect,
}

impl UseCaseShape {
    pub(crate) fn name(self) -> &'static str {
        match self {
            UseCaseShape::Ellipse => "ellipse",
            UseCaseShape::Rect => "rect",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundaryType {
    Rect,
    Package,
}

impl BoundaryType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            BoundaryType::Rect => "rect",
            BoundaryType::Package => "package",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelationshipType {
    Association,
    Include,
    Extend,
    Generalization,
}

impl RelationshipType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RelationshipType::Association => "association",
            RelationshipType::Include => "include",
            RelationshipType::Extend => "extend",
            RelationshipType::Generalization => "generalization",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Animation {
    Fast,
    Slow,
}

impl Animation {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Animation::Fast => "fast",
            Animation::Slow => "slow",
        }
    }
}

/// The direction a diagram runs in. `direction TD` is recorded as `TB`.
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

/// The line and the markers an association is written with, in the order of Pele's `Arrow`:
/// `SOLID_ARROW`, `BACK_ARROW`, `LINE_SOLID`, `CIRCLE_ARROW`, `CROSS_ARROW` and the last two
/// reversed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrow {
    Forward,
    Backward,
    Plain,
    Circle,
    Cross,
    CircleReversed,
    CrossReversed,
}

impl Arrow {
    /// Pele's `arrowType` number.
    pub(crate) fn number(self) -> u8 {
        self as u8
    }
}

/// Pele's `Actor`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Actor {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) kind: ActorType,
    pub(crate) icon: Option<JsText>,
    pub(crate) business: bool,
    pub(crate) stereotype: Option<JsText>,
    pub(crate) parent_id: Option<JsText>,
    pub(crate) classes: Vec<JsText>,
    pub(crate) styles: Vec<JsText>,
}

/// Pele's `UseCase`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UseCase {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) shape: UseCaseShape,
    pub(crate) business: bool,
    pub(crate) stereotype: Option<JsText>,
    pub(crate) parent_id: Option<JsText>,
    pub(crate) classes: Vec<JsText>,
    pub(crate) styles: Vec<JsText>,
}

/// Pele's `SystemBoundary`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SystemBoundary {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) kind: BoundaryType,
    pub(crate) members: Vec<JsText>,
    pub(crate) classes: Vec<JsText>,
    pub(crate) styles: Vec<JsText>,
}

/// Pele's `Relationship`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Relationship {
    pub(crate) id: JsText,
    pub(crate) explicit_id: bool,
    pub(crate) source: JsText,
    pub(crate) target: JsText,
    pub(crate) kind: RelationshipType,
    pub(crate) arrow: Arrow,
    pub(crate) label: Option<JsText>,
    pub(crate) label_type: Option<LabelType>,
    pub(crate) minlen: usize,
    pub(crate) classes: Vec<JsText>,
    pub(crate) styles: Vec<JsText>,
    pub(crate) animate: bool,
    pub(crate) animation: Option<Animation>,
}

/// Pele's `UsecaseNote`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Note {
    pub(crate) id: JsText,
    pub(crate) target: JsText,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
}

/// Pele's `UsecaseJsonNode`. The value keeps each object's keys in the order they were written,
/// which is what Pele's `propertyOrder` records beside a `JSON.parse` result.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JsonNode {
    pub(crate) id: JsText,
    pub(crate) value: json::Json,
    pub(crate) classes: Vec<JsText>,
    pub(crate) styles: Vec<JsText>,
}

/// Pele's `ClassDef`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassDef {
    pub(crate) id: JsText,
    pub(crate) styles: Vec<JsText>,
}

/// Pele's `UsecaseModel`, as `resolveUsecase` leaves it. Each map is in the order Pele's is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UsecaseModel {
    pub(crate) actors: IndexMap<JsText, Actor>,
    pub(crate) use_cases: IndexMap<JsText, UseCase>,
    pub(crate) system_boundaries: IndexMap<JsText, SystemBoundary>,
    pub(crate) relationships: Vec<Relationship>,
    pub(crate) notes: IndexMap<JsText, Note>,
    pub(crate) json_nodes: IndexMap<JsText, JsonNode>,
    pub(crate) class_defs: IndexMap<JsText, ClassDef>,
    pub(crate) direction: Direction,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: JsText,
    pub(crate) acc_descr: JsText,
}

/// Where the parser's text sits in the source the author wrote, for the positions in Pele's
/// messages. The mapping is worked out only when a source is rejected.
pub(crate) struct Positions<'a> {
    cx: &'a ParseContext<'a>,
    written: OnceCell<Written>,
}

impl<'a> Positions<'a> {
    fn new(cx: &'a ParseContext<'a>) -> Self {
        Positions { cx, written: OnceCell::new() }
    }

    fn map(&self) -> &Written {
        self.written.get_or_init(|| self.cx.origin.written())
    }

    /// Pele's `written(src, offset)[1]`: an offset of the parsed text, in the written source.
    pub(crate) fn written(&self, offset: usize) -> usize {
        self.map().offset(offset)
    }

    /// Pele's `lineColumn`: both counted from 1, the column in UTF-16 code units. A `\r\n` pair
    /// is one line end.
    pub(crate) fn line_column(&self, offset: usize) -> (usize, usize) {
        let source = self.map().source();
        let offset = self.written(offset);
        let mut line = 1;
        let mut line_start = 0;
        let mut i = 0;
        while i < offset.min(source.len()) {
            let c = source[i];
            if c == 10 || c == 13 {
                if c == 13 && i + 1 < offset && source.get(i + 1) == Some(&10) {
                    i += 1;
                }
                line += 1;
                line_start = i + 1;
            }
            i += 1;
        }
        (line, offset + 1 - line_start)
    }

    /// Pele's `locationText`.
    pub(crate) fn location_text(&self, span: Span) -> String {
        let (line, column) = self.line_column(span[0]);
        format!("line {line}, column {column} [{},{})", self.written(span[0]), self.written(span[1]))
    }

    /// A `PeleError` of this diagram type, positioned at `offset`.
    pub(crate) fn error(&self, code: ErrorCode, message: String, offset: usize) -> RenderError {
        let (line, column) = self.line_column(offset);
        self.error_at(code, message, line, column)
    }

    pub(crate) fn error_at(&self, code: ErrorCode, message: String, line: usize, column: usize) -> RenderError {
        let location = Location { line, column_utf16: column, snippet: String::new() };
        RenderError::Diagnostic(Diagnostic { location: Some(location), ..Diagnostic::new(code, message) })
    }
}

/// Pele's `parseUsecase`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<UsecaseModel, RenderError> {
    let positions = Positions::new(cx);
    let drafts = parser::parse(cx.text, &positions)?;
    db::resolve(drafts, cx.title.cloned(), &positions)
}
