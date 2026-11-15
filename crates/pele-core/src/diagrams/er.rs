//! Entity relationship diagrams, ported from Pele's `src/diagrams/er`. The lexer, the parser and
//! the db build Pele's model, `build_er_graph` resolves it into the nodes and edges that are
//! drawn, and `render` lays those out and draws them.

mod db;
mod graph;
pub(crate) mod json;
mod lexer;
mod markers;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;
use indexmap::IndexMap;

pub(crate) use graph::build_er_graph;
pub(crate) use render::Er;

/// The way a diagram, or a subgraph in it, runs.
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

/// How many of one entity relate to one of the other, at one end of a relationship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cardinality {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    MdParent,
}

impl Cardinality {
    /// Mermaid's name, as the model holds it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "ZERO_OR_ONE",
            Cardinality::ZeroOrMore => "ZERO_OR_MORE",
            Cardinality::OneOrMore => "ONE_OR_MORE",
            Cardinality::OnlyOne => "ONLY_ONE",
            Cardinality::MdParent => "MD_PARENT",
        }
    }

    /// The name of the marker drawn for it: Mermaid's name in lower case.
    pub(crate) fn marker_name(self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "zero_or_one",
            Cardinality::ZeroOrMore => "zero_or_more",
            Cardinality::OneOrMore => "one_or_more",
            Cardinality::OnlyOne => "only_one",
            Cardinality::MdParent => "md_parent",
        }
    }
}

/// Whether a relationship is identifying, drawn solid, or not, drawn dashed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Identification {
    NonIdentifying,
    Identifying,
}

impl Identification {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Identification::NonIdentifying => "NON_IDENTIFYING",
            Identification::Identifying => "IDENTIFYING",
        }
    }
}

/// Pele's `ErAttribute`: one row of an entity's table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErAttribute {
    pub(crate) kind: JsText,
    pub(crate) name: JsText,
    pub(crate) keys: Vec<JsText>,
    pub(crate) comment: JsText,
}

/// Pele's `ErEntity`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErEntity {
    /// `entity-<name>-<n>`, where n counts the entities before it.
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    /// The name an entity is shown by, where the source gives one.
    pub(crate) alias: JsText,
    pub(crate) attributes: Vec<ErAttribute>,
    /// `default`, then each class the source gives, with a space before each.
    pub(crate) css_classes: JsText,
    pub(crate) css_styles: Vec<JsText>,
}

/// Pele's `RelSpec`. `card_b` sits at entity A's end of the line and `card_a` at entity B's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RelSpec {
    pub(crate) card_a: Cardinality,
    pub(crate) rel_type: Identification,
    pub(crate) card_b: Cardinality,
}

/// Pele's `ErRelationship`. Each end holds an entity's id, or a subgraph's id when the name was a
/// subgraph at the time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErRelationship {
    pub(crate) entity_a: JsText,
    pub(crate) role_a: JsText,
    pub(crate) entity_b: JsText,
    pub(crate) rel_spec: RelSpec,
}

/// Pele's `ErClassDef`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErClassDef {
    pub(crate) id: JsText,
    pub(crate) styles: Vec<JsText>,
    pub(crate) text_styles: Vec<JsText>,
}

/// Pele's `ErSubgraph`. Its label type is always `markdown`: the parser never gives another.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErSubgraph {
    pub(crate) id: JsText,
    pub(crate) nodes: Vec<JsText>,
    pub(crate) title: JsText,
    pub(crate) classes: Vec<JsText>,
    pub(crate) css_styles: Vec<JsText>,
    pub(crate) dir: Option<Direction>,
}

/// Pele's `ErModel`, as its `ErDb` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErModel {
    pub(crate) direction: Direction,
    pub(crate) entities: IndexMap<JsText, ErEntity>,
    pub(crate) relationships: Vec<ErRelationship>,
    pub(crate) classes: IndexMap<JsText, ErClassDef>,
    pub(crate) subgraphs: Vec<ErSubgraph>,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
}

/// Pele's ER `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<ErModel, RenderError> {
    let mut db = db::ErDb::new();
    db.model.title = cx.title.filter(|title| !title.is_empty()).cloned();
    parser::parse(cx.text, &mut db).map_err(|error| cx.syntax(error))?;
    Ok(db.model)
}
