//! Agentflow diagrams, ported from Pele's `src/diagrams/agentflow`. The lexer, the parser and
//! the db build Pele's model, `build_agent_graph` resolves it into the nodes and edges that are
//! drawn and records the model's diagnostics, and `render` lays those out and draws them.

mod db;
mod graph;
pub(crate) mod json;
mod lexer;
mod parser;
mod render;
mod shapes;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::diagrams::flowchart::{Direction, FlowClassDef, LabelType, LinkTarget};
use crate::input::{JsObject, JsText, JsValue};
use indexmap::{IndexMap, IndexSet};
use std::collections::HashMap;

pub(crate) use graph::AgentGraph;
pub(crate) use render::Agentflow;
use shapes::NodeShape;

/// The arrowhead a link ends in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrow {
    Open,
    Point,
    Cross,
}

impl Arrow {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Arrow::Open => "arrow_open",
            Arrow::Point => "arrow_point",
            Arrow::Cross => "arrow_cross",
        }
    }
}

/// How a link's line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stroke {
    Normal,
    Dotted,
}

impl Stroke {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Stroke::Normal => "normal",
            Stroke::Dotted => "dotted",
        }
    }
}

/// What an edge means, which its arrow and line say together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EdgeSemantic {
    Sequence,
    Reference,
    Failure,
}

impl EdgeSemantic {
    pub(crate) fn name(self) -> &'static str {
        match self {
            EdgeSemantic::Sequence => "sequence",
            EdgeSemantic::Reference => "reference",
            EdgeSemantic::Failure => "failure",
        }
    }
}

/// What a link's arrow says about the edge. A link whose two halves disagree is `Invalid`,
/// which Pele records as the type and stroke `INVALID` with no length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    Valid { arrow: Arrow, stroke: Stroke, length: usize },
    Invalid,
}

impl Link {
    pub(crate) fn type_name(self) -> &'static str {
        match self {
            Link::Valid { arrow, .. } => arrow.name(),
            Link::Invalid => "INVALID",
        }
    }

    pub(crate) fn stroke_name(self) -> &'static str {
        match self {
            Link::Valid { stroke, .. } => stroke.name(),
            Link::Invalid => "INVALID",
        }
    }

    pub(crate) fn length(self) -> Option<usize> {
        match self {
            Link::Valid { length, .. } => Some(length),
            Link::Invalid => None,
        }
    }

    /// Pele's `edgeSemantic`.
    pub(crate) fn semantic(self) -> Option<EdgeSemantic> {
        match self {
            Link::Valid { arrow: Arrow::Point, stroke: Stroke::Normal, .. } => Some(EdgeSemantic::Sequence),
            Link::Valid { arrow: Arrow::Cross, stroke: Stroke::Normal, .. } => Some(EdgeSemantic::Failure),
            Link::Valid { arrow: Arrow::Open, stroke: Stroke::Dotted, .. } => Some(EdgeSemantic::Reference),
            _ => None,
        }
    }
}

/// Pele's `AgentNode`. A connector is a node too, with `is_connector` set.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentNode {
    pub(crate) id: JsText,
    pub(crate) text: JsText,
    pub(crate) label_type: LabelType,
    /// Pele's `type`: the shape the source gave, by bracket or by name.
    pub(crate) shape: Option<NodeShape>,
    pub(crate) styles: Vec<JsText>,
    pub(crate) classes: Vec<JsText>,
    /// Absent on a connector no statement has named since.
    pub(crate) props: Option<JsObject>,
    pub(crate) link: Option<JsText>,
    pub(crate) link_target: Option<LinkTarget>,
    pub(crate) metadata: Option<JsObject>,
    pub(crate) is_connector: bool,
}

/// Pele's `AgentEdge`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentEdge {
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
    pub(crate) metadata: Option<JsObject>,
}

/// Pele's `AgentSubgraph`: a `flow` block.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentSubgraph {
    pub(crate) id: JsText,
    pub(crate) nodes: Vec<JsText>,
    pub(crate) title: JsText,
    pub(crate) classes: Vec<JsText>,
    pub(crate) dir: Option<Direction>,
    pub(crate) label_type: LabelType,
    pub(crate) metadata: Option<JsObject>,
}

/// Where a statement sits in the source: lines from 1 with the front matter's lines added,
/// columns from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ElementPosition {
    pub(crate) start_line: usize,
    pub(crate) start_column: usize,
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatementType {
    Vertex,
    Edge,
    Subgraph,
    Connector,
    Attachment,
}

impl StatementType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            StatementType::Vertex => "vertex",
            StatementType::Edge => "edge",
            StatementType::Subgraph => "subgraph",
            StatementType::Connector => "connector",
            StatementType::Attachment => "attachment",
        }
    }
}

/// Pele's `ElementMapping`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementMapping {
    pub(crate) id: JsText,
    pub(crate) kind: StatementType,
    pub(crate) position: ElementPosition,
}

/// The diagnostics a parsed agentflow can carry. Mermaid's vocabulary names more, which only
/// tools that check a diagram's meaning raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticId {
    ShapeUnsupported,
    ShapeRemoved,
    ContainmentViolation,
}

impl DiagnosticId {
    pub(crate) fn name(self) -> &'static str {
        match self {
            DiagnosticId::ShapeUnsupported => "SHAPE_UNSUPPORTED",
            DiagnosticId::ShapeRemoved => "SHAPE_REMOVED",
            DiagnosticId::ContainmentViolation => "CONTAINMENT_VIOLATION",
        }
    }

    /// A removed shape is an error, and the rest are warnings.
    pub(crate) fn severity(self) -> &'static str {
        match self {
            DiagnosticId::ShapeRemoved => "error",
            DiagnosticId::ShapeUnsupported | DiagnosticId::ContainmentViolation => "warning",
        }
    }
}

/// Pele's `Diagnostic`, for a node.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentDiagnostic {
    pub(crate) id: DiagnosticId,
    pub(crate) message: JsText,
    pub(crate) node_id: JsText,
    /// Where the first statement that names the node sits.
    pub(crate) position: Option<ElementPosition>,
}

/// What a node is, which picks its palette slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VertexKind {
    Tool,
    Task,
    Decision,
    Input,
    Refdoc,
    Connector,
    Action,
}

impl VertexKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            VertexKind::Tool => "tool",
            VertexKind::Task => "task",
            VertexKind::Decision => "decision",
            VertexKind::Input => "input",
            VertexKind::Refdoc => "refdoc",
            VertexKind::Connector => "connector",
            VertexKind::Action => "action",
        }
    }

    /// Pele's `KIND_SLOT`: the series colour of the kind, which editing the diagram never changes.
    pub(crate) fn slot(self) -> usize {
        match self {
            VertexKind::Tool => 0,
            VertexKind::Task => 1,
            VertexKind::Decision => 2,
            VertexKind::Input => 3,
            VertexKind::Refdoc => 4,
            VertexKind::Connector => 5,
            VertexKind::Action => 6,
        }
    }

    /// Pele's `KIND_COUNT`: containers count through the slots above the kinds.
    pub(crate) const COUNT: usize = 7;
}

/// Pele's `AgentflowModel`, as its `AgentflowDb` leaves it once the graph is built.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentflowModel {
    pub(crate) direction: Direction,
    pub(crate) nodes: IndexMap<JsText, AgentNode>,
    pub(crate) edges: Vec<AgentEdge>,
    /// In the order they close, so an inner flow comes before the one around it.
    pub(crate) subgraphs: Vec<AgentSubgraph>,
    /// The ids of the nodes that are connectors, in the order they were declared.
    pub(crate) connectors: IndexSet<JsText>,
    pub(crate) classes: IndexMap<JsText, FlowClassDef>,
    pub(crate) tooltips: IndexMap<JsText, JsText>,
    pub(crate) default_edge_style: Option<Vec<JsText>>,
    pub(crate) default_interpolate: Option<JsText>,
    /// Where each statement sits in the source, in the order the statements were read.
    pub(crate) mappings: Vec<ElementMapping>,
    pub(crate) diagnostics: Vec<AgentDiagnostic>,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    /// What is drawn. Building it is what finds the diagnostics, so it is built with the model.
    pub(crate) graph: AgentGraph,
    /// Each flow's id, with its position in `subgraphs`.
    subgraph_index: HashMap<JsText, usize>,
}

impl AgentflowModel {
    /// Pele's `AgentflowDb.subgraph`.
    pub(crate) fn subgraph(&self, id: &[u16]) -> Option<&AgentSubgraph> {
        self.subgraph_index.get(id).map(|&at| &self.subgraphs[at])
    }

    /// Pele's `vertexKind`.
    pub(crate) fn vertex_kind(vertex: &AgentNode) -> VertexKind {
        let Some(shape) = vertex.shape else {
            return VertexKind::Task;
        };
        if shapes::is_tool(shape.as_str()) {
            return VertexKind::Tool;
        }
        match shapes::resolve_alias(shape.as_str()) {
            "hexagon" | "hex" => VertexKind::Action,
            "lean-right" | "lean_right" => VertexKind::Input,
            "lin-doc" | "lined-document" => VertexKind::Refdoc,
            "diamond" => VertexKind::Decision,
            _ => VertexKind::Task,
        }
    }
}

/// The curve `flowchart.curve` sets for every edge, when it is a string. Mermaid reads this
/// setting from the flowchart section for agentflow too.
fn default_curve(config: &JsObject) -> Option<&JsText> {
    let section = config.get("flowchart").and_then(JsValue::as_record)?;
    section.get("curve").and_then(JsValue::as_text)
}

/// Pele's agentflow `parse`: the model a source describes with its graph, or the first error
/// in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<AgentflowModel, RenderError> {
    let section = cx.config.get("flowchart").and_then(JsValue::as_record);
    let inherit_dir = section.and_then(|section| section.get("inheritDir")) == Some(&JsValue::Bool(true));
    let mut db = db::AgentflowDb::new(cx, inherit_dir);
    db.model.title = cx.title.filter(|title| !title.is_empty()).cloned();
    parser::parse(cx, &mut db)?;
    let mut model = db.model;
    let (graph, diagnostics) = graph::build_agent_graph(&model, render::PALETTE, default_curve(cx.config));
    model.graph = graph;
    model.diagnostics = diagnostics;
    Ok(model)
}
