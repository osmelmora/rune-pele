//! Architecture diagrams, ported from Pele's `src/diagrams/architecture`. The parser reads the
//! statements, the db checks them as Mermaid's `populateDb` does, `layout` puts every service and
//! junction on a grid cell, and `render` sizes the grid's rows and columns and draws the result.

mod db;
pub(crate) mod json;
mod layout;
mod parser;
pub mod probe;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;

pub(crate) use render::Architecture;

/// The side of a service or junction an edge leaves or enters by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    fn of_letter(letter: u16) -> Side {
        match u8::try_from(letter) {
            Ok(b'L') => Side::Left,
            Ok(b'R') => Side::Right,
            Ok(b'T') => Side::Top,
            _ => Side::Bottom,
        }
    }

    pub(crate) fn letter(self) -> &'static str {
        match self {
            Side::Left => "L",
            Side::Right => "R",
            Side::Top => "T",
            Side::Bottom => "B",
        }
    }

    /// Pele's `sideIndex`, which indexes its `DX` and `DY`.
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// What a service shows in its square: an icon by name, or text in place of one.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Service {
    pub(crate) icon: Option<JsText>,
    pub(crate) icon_text: Option<JsText>,
    pub(crate) title: Option<JsText>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NodeKind {
    Service(Service),
    /// A point where edges meet, drawn as a dot.
    Junction,
}

/// A service or a junction. `parent` is the place of its group in [`ArchitectureModel::groups`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    pub(crate) id: JsText,
    pub(crate) kind: NodeKind,
    pub(crate) parent: Option<usize>,
}

/// A group of services. A group's parent always comes before it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Group {
    pub(crate) id: JsText,
    pub(crate) icon: Option<JsText>,
    pub(crate) title: Option<JsText>,
    pub(crate) parent: Option<usize>,
}

/// One end of an edge: the place of its node, the side it meets, whether an arrowhead points
/// into it, and whether it crosses the border of the node's group (`{group}`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct End {
    pub(crate) node: usize,
    pub(crate) side: Side,
    pub(crate) into: bool,
    pub(crate) via_group: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Edge {
    pub(crate) lhs: End,
    pub(crate) rhs: End,
    pub(crate) title: Option<JsText>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Alignment {
    Row,
    Column,
}

impl Alignment {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Alignment::Row => "row",
            Alignment::Column => "column",
        }
    }
}

/// An `align row` or `align column` directive, its members as places in the nodes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutHint {
    pub(crate) direction: Alignment,
    pub(crate) members: Vec<usize>,
}

/// Pele's `ArchitectureModel`. The nodes are the services in source order, then the junctions.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ArchitectureModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) groups: Vec<Group>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) hints: Vec<LayoutHint>,
}

/// Pele's architecture `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<ArchitectureModel, RenderError> {
    let ast = parser::parse(cx.text).map_err(|error| cx.syntax(error))?;
    let front_title = cx.title.filter(|title| !title.is_empty());
    db::populate(ast, front_title).map_err(|message| cx.semantic(message))
}
