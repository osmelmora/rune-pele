//! C4 diagrams, ported from Pele's `src/diagrams/c4`. The lexer and the parser fill Pele's
//! `C4Db`. `layout` sets shapes in Mermaid's grid inside their boundaries, `route` runs each
//! relation between them, and `render` draws the result as a Scene.

mod db;
pub(crate) mod json;
mod layout;
mod lexer;
mod parser;
mod render;
mod route;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;
use indexmap::IndexMap;

pub(crate) use render::C4;

/// The header keyword, which says which of the five C4 views a diagram is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum C4Kind {
    Context,
    Container,
    Component,
    Dynamic,
    Deployment,
}

impl C4Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            C4Kind::Context => "C4Context",
            C4Kind::Container => "C4Container",
            C4Kind::Component => "C4Component",
            C4Kind::Dynamic => "C4Dynamic",
            C4Kind::Deployment => "C4Deployment",
        }
    }
}

/// A `$key="value"` argument.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Named {
    pub(crate) key: JsText,
    pub(crate) value: JsText,
}

/// One argument of a statement, as it was written: in its slot, or by name.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Attr {
    Text(JsText),
    Named(Named),
}

/// What a shape is, as Mermaid's `typeC4Shape` names it: `person`, `external_system_db` and so on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShapeKind {
    pub(crate) tier: Tier,
    pub(crate) external: bool,
    pub(crate) store: Store,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    Person,
    System,
    Container,
    Component,
}

/// How a shape keeps what it holds: as itself, as a database or as a queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Store {
    Plain,
    Db,
    Queue,
}

impl ShapeKind {
    pub(crate) fn name(self) -> String {
        let tier = match self.tier {
            Tier::Person => "person",
            Tier::System => "system",
            Tier::Container => "container",
            Tier::Component => "component",
        };
        let store = match self.store {
            Store::Plain => "",
            Store::Db => "_db",
            Store::Queue => "_queue",
        };
        format!("{}{tier}{store}", if self.external { "external_" } else { "" })
    }
}

/// The fields a named argument of a shape or a boundary may set, besides its label. Pele's
/// `ELEMENT_FIELDS`, in the order its `C4Element` declares them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementField {
    Type,
    Descr,
    Techn,
    Sprite,
    Tags,
    Link,
    BgColor,
    FontColor,
    BorderColor,
    Shadowing,
    Shape,
    LegendText,
    LegendSprite,
}

impl ElementField {
    pub(crate) const ALL: [ElementField; 13] = [
        ElementField::Type,
        ElementField::Descr,
        ElementField::Techn,
        ElementField::Sprite,
        ElementField::Tags,
        ElementField::Link,
        ElementField::BgColor,
        ElementField::FontColor,
        ElementField::BorderColor,
        ElementField::Shadowing,
        ElementField::Shape,
        ElementField::LegendText,
        ElementField::LegendSprite,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            ElementField::Type => "type",
            ElementField::Descr => "descr",
            ElementField::Techn => "techn",
            ElementField::Sprite => "sprite",
            ElementField::Tags => "tags",
            ElementField::Link => "link",
            ElementField::BgColor => "bgColor",
            ElementField::FontColor => "fontColor",
            ElementField::BorderColor => "borderColor",
            ElementField::Shadowing => "shadowing",
            ElementField::Shape => "shape",
            ElementField::LegendText => "legendText",
            ElementField::LegendSprite => "legendSprite",
        }
    }
}

/// The fields a named argument of a relation may set, besides its label and offsets. Pele's
/// `REL_FIELDS`, in the order its `C4Rel` declares them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelField {
    Techn,
    Descr,
    Sprite,
    Tags,
    Link,
    TextColor,
    LineColor,
}

impl RelField {
    pub(crate) const ALL: [RelField; 7] = [
        RelField::Techn,
        RelField::Descr,
        RelField::Sprite,
        RelField::Tags,
        RelField::Link,
        RelField::TextColor,
        RelField::LineColor,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            RelField::Techn => "techn",
            RelField::Descr => "descr",
            RelField::Sprite => "sprite",
            RelField::Tags => "tags",
            RelField::Link => "link",
            RelField::TextColor => "textColor",
            RelField::LineColor => "lineColor",
        }
    }
}

/// What a shape and a boundary have in common: Pele's `C4Element`. `parent` is the place of the
/// boundary that holds it in [`C4Model::boundaries`].
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Element {
    pub(crate) alias: JsText,
    pub(crate) label: JsText,
    /// A named argument written where the alias or the label belongs. It is applied to the field
    /// it names.
    pub(crate) alias_attr: Option<Named>,
    pub(crate) label_attr: Option<Named>,
    pub(crate) parent: Option<usize>,
    pub(crate) fields: [Option<JsText>; 13],
    /// Named arguments that set no known field, in the order they were first given.
    pub(crate) extra: Option<IndexMap<JsText, JsText>>,
}

impl Element {
    pub(crate) fn field(&self, field: ElementField) -> Option<&JsText> {
        self.fields[field as usize].as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shape {
    pub(crate) element: Element,
    pub(crate) kind: ShapeKind,
}

/// Set on deployment nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeType {
    Node,
    NodeL,
    NodeR,
}

impl NodeType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            NodeType::Node => "node",
            NodeType::NodeL => "nodeL",
            NodeType::NodeR => "nodeR",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Boundary {
    pub(crate) element: Element,
    pub(crate) node_type: Option<NodeType>,
}

/// Which way a relation is drawn, as its keyword says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelType {
    Rel,
    BiRel,
    Up,
    Down,
    Left,
    Right,
    Back,
}

impl RelType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RelType::Rel => "rel",
            RelType::BiRel => "birel",
            RelType::Up => "rel_u",
            RelType::Down => "rel_d",
            RelType::Left => "rel_l",
            RelType::Right => "rel_r",
            RelType::Back => "rel_b",
        }
    }
}

/// Pele's `C4Rel`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rel {
    pub(crate) kind: RelType,
    pub(crate) from: JsText,
    pub(crate) to: JsText,
    pub(crate) label: JsText,
    pub(crate) from_attr: Option<Named>,
    pub(crate) to_attr: Option<Named>,
    pub(crate) label_attr: Option<Named>,
    pub(crate) fields: [Option<JsText>; 7],
    /// `parseInt` of what was written, which may be NaN.
    pub(crate) offset_x: Option<f64>,
    pub(crate) offset_y: Option<f64>,
    pub(crate) extra: Option<IndexMap<JsText, JsText>>,
}

impl Rel {
    pub(crate) fn field(&self, field: RelField) -> Option<&JsText> {
        self.fields[field as usize].as_ref()
    }
}

/// Pele's `C4Model`, as its `C4Db` leaves it. The first boundary is the unnamed one that holds
/// everything at the top level.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct C4Model {
    pub(crate) c4_type: C4Kind,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) shapes: Vec<Shape>,
    pub(crate) boundaries: Vec<Boundary>,
    pub(crate) rels: Vec<Rel>,
    pub(crate) shape_in_row: f64,
    pub(crate) boundary_in_row: f64,
    /// Where each alias names a shape, and failing that a boundary: Pele's `getC4Shape`.
    pub(crate) shape_index: IndexMap<JsText, usize>,
    pub(crate) boundary_index: IndexMap<JsText, usize>,
}

/// A shape or a boundary, as `getC4Shape` finds one by its alias.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    Shape(usize),
    Boundary(usize),
}

/// Pele's C4 `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<C4Model, RenderError> {
    let mut db = db::C4Db::new();
    if let Some(title) = cx.title.filter(|title| !title.is_empty()) {
        db.set_title(title.clone());
    }
    let kind = parser::parse(cx, &mut db)?;
    Ok(db.finish(kind))
}
