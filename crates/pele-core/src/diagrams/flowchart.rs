//! Flowcharts, ported from Pele's `src/diagrams/flowchart`. The lexer, the parser and the db
//! build Pele's model, `build_flow_graph` resolves it into the nodes and edges that are drawn,
//! and `render` lays those out and draws them.

mod db;
mod graph;
pub(crate) mod json;
mod lexer;
mod parser;
mod render;
mod shapes;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsObject, JsText, JsValue, Units};
use indexmap::IndexMap;
use std::collections::HashMap;

pub(crate) use graph::{FlowGraph, GraphEdge, GraphNode, build_flow_graph};
pub(crate) use render::Flowchart;
pub(crate) use shapes::Shape;
pub(crate) use shapes::ShapeName;

/// How a label's text is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LabelType {
    Text,
    String,
    Markdown,
}

impl LabelType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::String => "string",
            LabelType::Markdown => "markdown",
        }
    }

    /// Pele's `labelType`: a value that names one of the three, or markdown.
    fn of(value: Option<&JsValue>) -> LabelType {
        let name = |text: &str| value.and_then(JsValue::as_text).is_some_and(|value| value.is_str(text));
        if name("string") {
            LabelType::String
        } else if name("text") {
            LabelType::Text
        } else {
            LabelType::Markdown
        }
    }
}

/// A label as the parser reads it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowText {
    pub(crate) text: JsText,
    pub(crate) kind: LabelType,
}

/// A flowchart's or a subgraph's direction, as written. The flowchart's own is never `TD`, and a
/// subgraph's is never `BR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    TB,
    BT,
    RL,
    LR,
    TD,
    BR,
}

impl Direction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Direction::TB => "TB",
            Direction::BT => "BT",
            Direction::RL => "RL",
            Direction::LR => "LR",
            Direction::TD => "TD",
            Direction::BR => "BR",
        }
    }
}

/// The arrowheads a link is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrowKind {
    Open,
    Point,
    Cross,
    Circle,
    DoublePoint,
    DoubleCross,
    DoubleCircle,
}

impl ArrowKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            ArrowKind::Open => "arrow_open",
            ArrowKind::Point => "arrow_point",
            ArrowKind::Cross => "arrow_cross",
            ArrowKind::Circle => "arrow_circle",
            ArrowKind::DoublePoint => "double_arrow_point",
            ArrowKind::DoubleCross => "double_arrow_cross",
            ArrowKind::DoubleCircle => "double_arrow_circle",
        }
    }
}

/// How a link's line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stroke {
    Normal,
    Thick,
    Dotted,
    Invisible,
}

impl Stroke {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Stroke::Normal => "normal",
            Stroke::Thick => "thick",
            Stroke::Dotted => "dotted",
            Stroke::Invisible => "invisible",
        }
    }
}

/// What a link's arrow says about the edge. A link whose two ends disagree is `Invalid`, which
/// Pele records as the type and stroke `INVALID` with no length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    Valid { arrow: ArrowKind, stroke: Stroke, length: usize },
    Invalid,
}

impl Link {
    /// The edge's `type`.
    pub(crate) fn type_name(self) -> &'static str {
        match self {
            Link::Valid { arrow, .. } => arrow.name(),
            Link::Invalid => "INVALID",
        }
    }

    /// The edge's `stroke`, which the flow graph calls its thickness and pattern.
    pub(crate) fn stroke_name(self) -> &'static str {
        match self {
            Link::Valid { stroke, .. } => stroke.name(),
            Link::Invalid => "INVALID",
        }
    }

    /// The edge's `length`, which the flow graph calls its `minlen`.
    pub(crate) fn length(self) -> Option<usize> {
        match self {
            Link::Valid { length, .. } => Some(length),
            Link::Invalid => None,
        }
    }
}

/// Where a click link opens.
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
}

/// Pele's `FlowNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowNode {
    pub(crate) id: JsText,
    pub(crate) text: JsText,
    pub(crate) label_type: LabelType,
    /// Pele's `type`: the shape the source gave, by bracket or by name.
    pub(crate) shape: Option<ShapeName>,
    pub(crate) styles: Vec<JsText>,
    pub(crate) classes: Vec<JsText>,
    pub(crate) props: JsObject,
    pub(crate) link: Option<JsText>,
    pub(crate) link_target: Option<LinkTarget>,
    pub(crate) icon: Option<JsText>,
    pub(crate) form: Option<JsText>,
    pub(crate) pos: Option<JsText>,
    pub(crate) img: Option<JsText>,
    pub(crate) constraint: Option<JsText>,
    pub(crate) asset_width: Option<f64>,
    pub(crate) asset_height: Option<f64>,
}

/// Pele's `FlowEdge`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowEdge {
    pub(crate) start: JsText,
    pub(crate) end: JsText,
    pub(crate) link: Link,
    pub(crate) text: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) style: Option<Vec<JsText>>,
    pub(crate) classes: Vec<JsText>,
    pub(crate) id: JsText,
    pub(crate) is_user_defined_id: bool,
    /// A `linkStyle` curve, or whatever an edge's `curve` metadata holds.
    pub(crate) interpolate: Option<JsValue>,
    pub(crate) animate: Option<JsValue>,
    pub(crate) animation: Option<JsValue>,
}

/// Pele's `FlowClassDef`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowClassDef {
    pub(crate) id: JsText,
    pub(crate) styles: Vec<JsText>,
    pub(crate) text_styles: Vec<JsText>,
}

/// Pele's `FlowSubgraph`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowSubgraph {
    pub(crate) id: JsText,
    pub(crate) nodes: Vec<JsText>,
    pub(crate) title: JsText,
    pub(crate) classes: Vec<JsText>,
    pub(crate) dir: Option<Direction>,
    pub(crate) label_type: LabelType,
    pub(crate) metadata: Option<JsObject>,
}

/// Pele's `FlowchartModel`, as its `FlowDb` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowchartModel {
    pub(crate) direction: Direction,
    pub(crate) nodes: IndexMap<JsText, FlowNode>,
    pub(crate) edges: Vec<FlowEdge>,
    /// In the order they close, so an inner subgraph comes before the one around it.
    pub(crate) subgraphs: Vec<FlowSubgraph>,
    pub(crate) classes: IndexMap<JsText, FlowClassDef>,
    pub(crate) tooltips: IndexMap<JsText, JsText>,
    pub(crate) default_edge_style: Option<Vec<JsText>>,
    pub(crate) default_interpolate: Option<JsText>,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    /// Each subgraph id, with its position in `subgraphs`.
    subgraph_index: HashMap<JsText, usize>,
}

impl FlowchartModel {
    /// Pele's `FlowDb.subgraph`.
    pub(crate) fn subgraph(&self, id: &[u16]) -> Option<&FlowSubgraph> {
        self.subgraph_index.get(id).map(|&at| &self.subgraphs[at])
    }
}

/// Pele's flowchart `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<FlowchartModel, RenderError> {
    let section = cx.config.get("flowchart").and_then(JsValue::as_record);
    let inherit_dir = section.and_then(|section| section.get("inheritDir")) == Some(&JsValue::Bool(true));
    let mut db = db::FlowDb::new(inherit_dir, cx.limits.max_edges);
    db.model.title = cx.title.filter(|title| !title.is_empty()).cloned();
    parser::parse(cx, &mut db)?;
    Ok(db.model)
}

/// The curve `flowchart.curve` sets for every edge, when it is a string.
pub(crate) fn default_curve(config: &JsObject) -> Option<&JsText> {
    let section = config.get("flowchart").and_then(JsValue::as_record)?;
    section.get("curve").and_then(JsValue::as_text)
}
