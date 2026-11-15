//! Class diagrams, ported from Pele's `src/diagrams/class`. The lexer, the parser and the db
//! build Pele's model, `build_class_graph` resolves it into the nodes and edges that are drawn,
//! and `render` lays those out and draws them.

mod db;
mod graph;
pub(crate) mod json;
mod lexer;
mod markers;
mod members;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsObject, JsText, JsValue};
use indexmap::{IndexMap, IndexSet};

pub(crate) use graph::build_class_graph;
pub(crate) use members::{ClassMember, Classifier, MemberType};
pub(crate) use render::Class;

/// The way a class diagram runs.
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

/// The marker at one end of a relation. Pele's `RelationEnd`: a number from 0 to 4, or `none`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelationEnd {
    None,
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
}

impl RelationEnd {
    /// Mermaid's `relationType` number.
    pub(crate) fn number(self) -> Option<u8> {
        match self {
            RelationEnd::None => None,
            RelationEnd::Aggregation => Some(0),
            RelationEnd::Extension => Some(1),
            RelationEnd::Composition => Some(2),
            RelationEnd::Dependency => Some(3),
            RelationEnd::Lollipop => Some(4),
        }
    }

    /// The name the class graph gives the marker.
    pub(crate) fn marker_name(self) -> &'static str {
        match self {
            RelationEnd::None => "none",
            RelationEnd::Aggregation => "aggregation",
            RelationEnd::Extension => "extension",
            RelationEnd::Composition => "composition",
            RelationEnd::Dependency => "dependency",
            RelationEnd::Lollipop => "lollipop",
        }
    }
}

/// Whether a relation's line is solid or dotted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineType {
    Line,
    Dotted,
}

/// Where a class link opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkTarget {
    SelfFrame,
    Blank,
    Parent,
    Top,
}

impl LinkTarget {
    pub(crate) fn name(self) -> &'static str {
        match self {
            LinkTarget::SelfFrame => "_self",
            LinkTarget::Blank => "_blank",
            LinkTarget::Parent => "_parent",
            LinkTarget::Top => "_top",
        }
    }

    fn named(text: &[u16]) -> Option<LinkTarget> {
        let is = |name: &str| text.iter().copied().eq(name.encode_utf16());
        [LinkTarget::SelfFrame, LinkTarget::Blank, LinkTarget::Parent, LinkTarget::Top]
            .into_iter()
            .find(|target| is(target.name()))
    }
}

/// Pele's `ClassNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassNode {
    pub(crate) id: JsText,
    /// The generic type written after the name, as in `List~T~`.
    pub(crate) generic: JsText,
    pub(crate) label: JsText,
    /// `default`, then each class the source gives, with a space before each.
    pub(crate) css_classes: JsText,
    pub(crate) methods: Vec<ClassMember>,
    pub(crate) members: Vec<ClassMember>,
    pub(crate) annotations: Vec<JsText>,
    pub(crate) styles: Vec<JsText>,
    pub(crate) parent: Option<JsText>,
    /// Set by a link: the address as sanitized, or nothing for an empty one.
    pub(crate) link: Option<JsText>,
    pub(crate) link_target: Option<LinkTarget>,
    pub(crate) have_callback: bool,
    pub(crate) tooltip: Option<JsText>,
}

/// Pele's `ClassRelation`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassRelation {
    pub(crate) id1: JsText,
    pub(crate) id2: JsText,
    /// The text at each end, or `none`.
    pub(crate) relation_title1: JsText,
    pub(crate) relation_title2: JsText,
    pub(crate) title: Option<JsText>,
    pub(crate) type1: RelationEnd,
    pub(crate) type2: RelationEnd,
    pub(crate) line_type: LineType,
}

/// Pele's `ClassNote`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassNote {
    pub(crate) id: JsText,
    pub(crate) class: Option<JsText>,
    pub(crate) text: JsText,
    pub(crate) index: usize,
    pub(crate) parent: Option<JsText>,
}

/// Pele's `NamespaceNode`. Its classes, notes and children are named by id, in the order they
/// were first added.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NamespaceNode {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) classes: IndexSet<JsText>,
    pub(crate) notes: IndexSet<JsText>,
    pub(crate) children: IndexSet<JsText>,
    pub(crate) parent: Option<JsText>,
    /// False for the ancestors created on the way to a dotted name such as `A.B.C`.
    pub(crate) explicit: bool,
}

/// Pele's `ClassInterface`: the far end of a lollipop.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassInterface {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) class_id: JsText,
}

/// Pele's `StyleClass`: a `classDef`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StyleClass {
    pub(crate) id: JsText,
    pub(crate) styles: Vec<JsText>,
    pub(crate) text_styles: Vec<JsText>,
}

/// Pele's `ClassModel`, as its `ClassDb` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassModel {
    pub(crate) direction: Direction,
    pub(crate) classes: IndexMap<JsText, ClassNode>,
    pub(crate) relations: Vec<ClassRelation>,
    pub(crate) notes: IndexMap<JsText, ClassNote>,
    pub(crate) namespaces: IndexMap<JsText, NamespaceNode>,
    pub(crate) interfaces: Vec<ClassInterface>,
    pub(crate) style_classes: IndexMap<JsText, StyleClass>,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
}

/// Pele's class `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<ClassModel, RenderError> {
    let mut db = db::ClassDb::new();
    db.model.title = cx.title.filter(|title| !title.is_empty()).cloned();
    parser::parse(cx.text, &mut db).map_err(|error| cx.syntax(error))?;
    Ok(db.model)
}

/// The class section of the config, where it is an object.
pub(crate) fn section(config: &JsObject) -> Option<&JsObject> {
    config.get("class").and_then(JsValue::as_record)
}

/// Whether namespaces nest as their dotted names say. `class.hierarchicalNamespaces: false`
/// keeps only the namespaces a source declares, each named in full.
pub(crate) fn hierarchical(config: &JsObject) -> bool {
    section(config).and_then(|section| section.get("hierarchicalNamespaces")) != Some(&JsValue::Bool(false))
}
