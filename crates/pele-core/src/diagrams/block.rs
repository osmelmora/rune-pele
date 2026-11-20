//! Block diagrams, ported from Pele's `src/diagrams/block`. The lexer and the parser read the
//! statements, the db builds them into a tree of blocks, `layout` places that tree on nested
//! grids, and `render` draws it with its edges and block arrows.

mod arrows;
mod db;
pub(crate) mod json;
/// Not part of the API: public for the workspace's tests, through the crate root.
#[doc(hidden)]
pub mod layout;
mod lexer;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;
use indexmap::IndexMap;

pub(crate) use render::Block;

/// Pele's `typeStr2Type`, and the kinds a statement list holds besides nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Na,
    Square,
    Round,
    Circle,
    RectLeftInvArrow,
    Diamond,
    Hexagon,
    Stadium,
    Subroutine,
    Cylinder,
    DoubleCircle,
    LeanRight,
    LeanLeft,
    Trapezoid,
    InvTrapezoid,
    BlockArrow,
    Composite,
    Space,
}

impl Kind {
    /// The name Mermaid gives the kind, which is the model's `type`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Na => "na",
            Kind::Square => "square",
            Kind::Round => "round",
            Kind::Circle => "circle",
            Kind::RectLeftInvArrow => "rect_left_inv_arrow",
            Kind::Diamond => "diamond",
            Kind::Hexagon => "hexagon",
            Kind::Stadium => "stadium",
            Kind::Subroutine => "subroutine",
            Kind::Cylinder => "cylinder",
            Kind::DoubleCircle => "doublecircle",
            Kind::LeanRight => "lean_right",
            Kind::LeanLeft => "lean_left",
            Kind::Trapezoid => "trapezoid",
            Kind::InvTrapezoid => "inv_trapezoid",
            Kind::BlockArrow => "block_arrow",
            Kind::Composite => "composite",
            Kind::Space => "space",
        }
    }

    /// Pele's `typeStr2Type`: the kind a node's brackets give it.
    fn of_brackets(brackets: Option<&[u16]>) -> Kind {
        let Some(brackets) = brackets else {
            return Kind::Na;
        };
        let text = String::from_utf16_lossy(brackets);
        match text.as_str() {
            "[]" => Kind::Square,
            "()" => Kind::Round,
            "(())" => Kind::Circle,
            ">]" => Kind::RectLeftInvArrow,
            "{}" => Kind::Diamond,
            "{{}}" => Kind::Hexagon,
            "([])" => Kind::Stadium,
            "[[]]" => Kind::Subroutine,
            "[()]" => Kind::Cylinder,
            "((()))" => Kind::DoubleCircle,
            "[//]" => Kind::LeanRight,
            "[\\\\]" => Kind::LeanLeft,
            "[/\\]" => Kind::Trapezoid,
            "[\\/]" => Kind::InvTrapezoid,
            "<[]>" => Kind::BlockArrow,
            _ => Kind::Na,
        }
    }
}

/// What is drawn at an end of an edge, as Mermaid names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Head {
    /// The name `edgeStrToEdgeData` gives a link that ends in no head: the empty string.
    Unnamed,
    Open,
    Point,
    Circle,
    Cross,
}

impl Head {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Head::Unnamed => "",
            Head::Open => "arrow_open",
            Head::Point => "arrow_point",
            Head::Circle => "arrow_circle",
            Head::Cross => "arrow_cross",
        }
    }
}

/// A block of the tree, or one that a `class` or `style` statement named before any block had
/// its id. Each field is Pele's of the same name; a field Pele leaves undefined is `None`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockNode {
    pub(crate) id: JsText,
    pub(crate) kind: Kind,
    pub(crate) label: Option<JsText>,
    /// Positions in [`BlockModel::blocks`].
    pub(crate) children: Vec<usize>,
    pub(crate) columns: Option<f64>,
    pub(crate) width_in_columns: Option<f64>,
    /// The number of cells a `space` statement asked for.
    pub(crate) width: Option<f64>,
    pub(crate) directions: Option<Vec<JsText>>,
    pub(crate) classes: Option<Vec<JsText>>,
    pub(crate) styles: Option<Vec<JsText>>,
    pub(crate) color_index: Option<f64>,
}

/// An edge between two blocks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockEdge {
    /// `<n>-<start>-<end>`, where n counts the edges between the same two ids.
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) directions: Option<Vec<JsText>>,
    pub(crate) start: JsText,
    pub(crate) end: JsText,
    pub(crate) arrow_type_start: Head,
    pub(crate) arrow_type_end: Head,
    pub(crate) thick: bool,
    pub(crate) dotted: bool,
}

/// Pele's `BlockClassDef`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockClassDef {
    pub(crate) id: JsText,
    pub(crate) styles: Vec<JsText>,
    pub(crate) text_styles: Vec<JsText>,
}

/// Pele's `BlockModel`, as its `BlockDb` leaves it. The root is block 0.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockModel {
    pub(crate) blocks: Vec<BlockNode>,
    pub(crate) edges: Vec<BlockEdge>,
    pub(crate) classes: IndexMap<JsText, BlockClassDef>,
    pub(crate) by_id: IndexMap<JsText, usize>,
    pub(crate) title: Option<JsText>,
}

impl BlockModel {
    pub(crate) fn root(&self) -> &BlockNode {
        &self.blocks[0]
    }
}

/// Pele's block `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<BlockModel, RenderError> {
    let mut db = db::BlockDb::new();
    let statements = parser::parse(cx.text, &mut db).map_err(|error| cx.syntax(error))?;
    db.set_hierarchy(statements)?;
    let mut model = db.finish();
    model.title = cx.title.filter(|title| !title.is_empty()).cloned();
    Ok(model)
}
