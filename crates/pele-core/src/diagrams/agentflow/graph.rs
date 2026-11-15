//! Pele's `buildAgentGraph` (`src/diagrams/agentflow/graph.ts`): the nodes and edges that are
//! drawn, with flow membership, collapsed flows, shapes reduced to the ones agentflow has, class
//! styles and palette slots resolved. Shape and containment problems come back as diagnostics.

use super::shapes::{self, Normalised};
use super::{AgentDiagnostic, AgentflowModel, Arrow, DiagnosticId, ElementPosition, Link, Stroke, VertexKind};
use crate::diagrams::flowchart::{Direction, LabelType, LinkTarget, ShapeName};
use crate::input::{JsObject, JsText, JsValue, Units};
use indexmap::IndexSet;
use std::collections::{HashMap, HashSet};

/// What a graph node is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphShape {
    /// A flow, drawn as a frame around its members.
    FlowGroup,
    /// A node, by one of the shape names agentflow draws. A collapsed flow is the node
    /// `collapsedGroup`, which no source can name: a shape name has to be lowercase.
    Node(ShapeName),
}

impl GraphShape {
    pub(crate) fn name(self) -> &'static str {
        match self {
            GraphShape::FlowGroup => "flowGroup",
            GraphShape::Node(shape) => shape.as_str(),
        }
    }

    /// Whether this is a collapsed flow.
    pub(crate) fn is_collapsed(self) -> bool {
        self == GraphShape::Node(ShapeName::COLLAPSED_GROUP)
    }
}

/// Pele's `GraphNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphNode {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) shape: GraphShape,
    pub(crate) parent_id: Option<JsText>,
    pub(crate) css_styles: Vec<JsText>,
    pub(crate) css_compiled_styles: Vec<JsText>,
    pub(crate) css_classes: JsText,
    pub(crate) dir: Option<Direction>,
    pub(crate) link: Option<JsText>,
    pub(crate) link_target: Option<LinkTarget>,
    pub(crate) tooltip: Option<JsText>,
    /// Palette slot of a container.
    pub(crate) color_index: Option<usize>,
    /// What a node is, which picks its palette slot.
    pub(crate) kind: Option<VertexKind>,
    pub(crate) metadata: Option<JsObject>,
}

impl GraphNode {
    pub(crate) fn is_group(&self) -> bool {
        self.shape == GraphShape::FlowGroup
    }
}

/// Pele's `GraphEdge`. Its `type`, `thickness`, `pattern`, `minlen` and arrowhead all follow
/// from the link.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphEdge {
    pub(crate) id: JsText,
    pub(crate) is_user_defined_id: bool,
    pub(crate) start: JsText,
    pub(crate) end: JsText,
    pub(crate) link: Link,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) css_compiled_styles: Vec<JsText>,
    pub(crate) style: Vec<JsText>,
    pub(crate) animate: Option<JsValue>,
    pub(crate) animation: Option<JsValue>,
    pub(crate) curve: Option<JsValue>,
    pub(crate) metadata: Option<JsObject>,
}

impl GraphEdge {
    /// `arrowTypeEnd`: the arrowhead the edge ends in, or none for an open link. A link whose
    /// halves disagree ends in a point.
    pub(crate) fn arrow_end(&self) -> Option<Arrow> {
        match self.link {
            Link::Valid { arrow: Arrow::Open, .. } => None,
            Link::Valid { arrow, .. } => Some(arrow),
            Link::Invalid => Some(Arrow::Point),
        }
    }

    pub(crate) fn dotted(&self) -> bool {
        matches!(self.link, Link::Valid { stroke: Stroke::Dotted, .. })
    }
}

/// Pele's `AgentGraph`.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct AgentGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

fn is_collapsed(metadata: Option<&JsObject>) -> bool {
    metadata
        .and_then(|metadata| metadata.get("view"))
        .and_then(JsValue::as_text)
        .is_some_and(|view| view.is_str("collapsed"))
}

/// Pele's `containerSlot`: containers count through the slots above the kinds, and a palette
/// shorter than the kinds still gives them one.
fn container_slot(n: usize, palette_length: usize) -> usize {
    let count = palette_length.saturating_sub(VertexKind::COUNT).max(1);
    let slot = VertexKind::COUNT + n % count;
    if palette_length > 0 { slot % palette_length } else { slot }
}

/// Everything inside a collapsed flow is hidden, and stands for its outermost collapsed flow.
/// Mermaid walks the whole inside of each collapsed flow, in the order the flows closed, and the
/// last walk to reach a thing wins. A thing has one container, so a later walk can only enter an
/// earlier one through its root: it stops there, and the root's owner is followed afterwards.
fn collapse(db: &AgentflowModel) -> (IndexSet<JsText>, HashMap<JsText, JsText>) {
    let mut hidden: IndexSet<JsText> = IndexSet::new();
    let mut owner: HashMap<JsText, JsText> = HashMap::new();
    let mut walked: HashSet<&JsText> = HashSet::new();
    for sg in &db.subgraphs {
        if !is_collapsed(sg.metadata.as_ref()) || hidden.contains(&sg.id) {
            continue;
        }
        let mut pending = vec![&sg.id];
        while let Some(id) = pending.pop() {
            let Some(inner) = db.subgraph(id) else {
                continue;
            };
            for child in &inner.nodes {
                if *child == sg.id || hidden.contains(child) {
                    continue;
                }
                hidden.insert(child.clone());
                owner.insert(child.clone(), sg.id.clone());
                if !walked.contains(child) {
                    pending.push(child);
                }
            }
        }
        walked.insert(&sg.id);
    }
    let mut collapsed_into: HashMap<JsText, JsText> = HashMap::new();
    for id in &hidden {
        let mut path = vec![id.clone()];
        let mut top = owner[id].clone();
        loop {
            let known = collapsed_into.get(&top).cloned();
            let Some(up) = known.clone().or_else(|| owner.get(&top).cloned()) else {
                break;
            };
            if known.is_none() {
                path.push(top);
            }
            top = up;
            if known.is_some() {
                break;
            }
        }
        for step in path {
            collapsed_into.insert(step, top.clone());
        }
    }
    (hidden, collapsed_into)
}

/// Pele's `buildAgentGraph`, with the diagnostics it records on the model.
pub(super) fn build_agent_graph(
    db: &AgentflowModel,
    palette_length: usize,
    default_curve: Option<&JsText>,
) -> (AgentGraph, Vec<AgentDiagnostic>) {
    let subgraphs = &db.subgraphs;
    let mut diagnostics: Vec<AgentDiagnostic> = Vec::new();
    let mut emit = |id: DiagnosticId, message: String, node_id: &JsText| {
        let position: Option<ElementPosition> =
            db.mappings.iter().find(|mapping| mapping.id == *node_id).map(|mapping| mapping.position);
        diagnostics.push(AgentDiagnostic { id, message: message.as_str().into(), node_id: node_id.clone(), position });
    };

    let compiled = |class_names: &mut dyn Iterator<Item = &[u16]>| {
        let mut out: Vec<JsText> = Vec::new();
        for name in class_names {
            if let Some(def) = db.classes.get(name) {
                out.extend(def.styles.iter().chain(&def.text_styles).map(|style| JsText::from(style.trim_js())));
            }
        }
        out
    };
    let own = |classes: &[JsText]| compiled(&mut classes.iter().map(|class| &**class));
    let joined = |classes: &[JsText]| classes.iter().map(|class| class.to_vec()).collect::<Vec<_>>().join(&32);

    let (hidden, collapsed_into) = collapse(db);

    let position: HashMap<&JsText, usize> = subgraphs.iter().enumerate().map(|(i, sg)| (&sg.id, i)).collect();

    // Two flows can each name the other. The nesting that would close the loop is dropped.
    let mut parent_of: HashMap<&JsText, &JsText> = HashMap::new();
    for (i, sg) in subgraphs.iter().enumerate().rev() {
        if hidden.contains(&sg.id) {
            continue;
        }
        for id in &sg.nodes {
            // Only a flow handled earlier in this pass can already be above the parent.
            let mut closes_loop = false;
            if position.get(id).is_some_and(|&child_at| child_at >= i) {
                let mut current = Some(&sg.id);
                let mut hops = 0;
                while let Some(at) = current.filter(|_| hops <= subgraphs.len()) {
                    if at == id {
                        closes_loop = true;
                        break;
                    }
                    current = parent_of.get(at).copied();
                    hops += 1;
                }
            }
            if closes_loop {
                let (container, inner) = (sg.id.to_display(), id.to_display());
                let message = format!(
                    "Container \"{container}\" cannot contain \"{inner}\" because \"{inner}\" already contains it. The nesting that would close the loop is dropped."
                );
                emit(DiagnosticId::ContainmentViolation, message, id);
                continue;
            }
            parent_of.insert(id, &sg.id);
        }
    }

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut by_id: HashMap<&JsText, usize> = HashMap::new();
    for sg in subgraphs.iter().rev() {
        if hidden.contains(&sg.id) {
            continue;
        }
        let collapsed = is_collapsed(sg.metadata.as_ref());
        let metadata = if collapsed {
            let mut metadata = sg.metadata.clone().unwrap_or_default();
            metadata.set("containerType".into(), JsValue::String("flow".into()));
            Some(metadata)
        } else {
            sg.metadata.clone()
        };
        by_id.insert(&sg.id, nodes.len());
        nodes.push(GraphNode {
            id: sg.id.clone(),
            label: sg.title.clone(),
            label_type: sg.label_type,
            shape: if collapsed { GraphShape::Node(ShapeName::COLLAPSED_GROUP) } else { GraphShape::FlowGroup },
            parent_id: parent_of.get(&sg.id).map(|&parent| parent.clone()),
            css_styles: Vec::new(),
            css_compiled_styles: own(&sg.classes),
            css_classes: joined(&sg.classes).into(),
            dir: sg.dir,
            link: None,
            link_target: None,
            tooltip: None,
            color_index: None,
            kind: None,
            metadata,
        });
    }

    for vertex in db.nodes.values() {
        if hidden.contains(&vertex.id) {
            continue;
        }
        if let Some(&at) = by_id.get(&vertex.id) {
            let existing = &mut nodes[at];
            existing.css_styles = vertex.styles.clone();
            existing.css_compiled_styles = own(&vertex.classes);
            existing.css_classes = joined(&vertex.classes).into();
            continue;
        }
        // Reduces the shape to one agentflow draws, reporting the removed and unsupported ones.
        let name = match vertex.shape.map(|shape| shapes::resolve_alias(shape.as_str())) {
            None | Some("square") => "squareRect",
            Some(name) => name,
        };
        let shape = match shapes::normalise(name) {
            Normalised::Drawn(shape) => shape,
            Normalised::Removed => {
                let message = format!("shape \"{name}\" was removed in v0.8.1, using \"{}\"", shapes::DEFAULT_SHAPE);
                emit(DiagnosticId::ShapeRemoved, message, &vertex.id);
                ShapeName::ROUNDED_RECT
            }
            Normalised::Unsupported => {
                let message = format!("shape \"{name}\" is not supported, using \"{}\"", shapes::DEFAULT_SHAPE);
                emit(DiagnosticId::ShapeUnsupported, message, &vertex.id);
                ShapeName::ROUNDED_RECT
            }
        };
        let defaults = [JsText::from("default"), JsText::from("node")];
        let mut classes = defaults.iter().chain(&vertex.classes).map(|class| &**class);
        let mut css_classes = JsText::from("default ");
        css_classes.push_units(&joined(&vertex.classes));
        by_id.insert(&vertex.id, nodes.len());
        nodes.push(GraphNode {
            id: vertex.id.clone(),
            label: vertex.text.clone(),
            label_type: vertex.label_type,
            shape: GraphShape::Node(shape),
            parent_id: parent_of.get(&vertex.id).map(|&parent| parent.clone()),
            css_styles: vertex.styles.clone(),
            css_compiled_styles: compiled(&mut classes),
            css_classes,
            dir: None,
            link: vertex.link.clone(),
            link_target: vertex.link_target,
            tooltip: db.tooltips.get(&vertex.id).cloned(),
            color_index: None,
            kind: None,
            metadata: vertex.metadata.clone(),
        });
    }

    let mut edges: Vec<GraphEdge> = Vec::new();
    for raw in &db.edges {
        let start = collapsed_into.get(&raw.start).unwrap_or(&raw.start);
        let end = collapsed_into.get(&raw.end).unwrap_or(&raw.end);
        // An edge between two things inside one collapsed flow has nothing left to connect.
        if start == end && raw.start != raw.end {
            continue;
        }
        let mut style = db.default_edge_style.clone().unwrap_or_default();
        style.extend(raw.style.iter().flatten().cloned());
        let curve = raw
            .interpolate
            .clone()
            .filter(JsValue::truthy)
            .or_else(|| db.default_interpolate.clone().map(JsValue::String))
            .or_else(|| default_curve.cloned().map(JsValue::String));
        edges.push(GraphEdge {
            id: raw.id.clone(),
            is_user_defined_id: raw.is_user_defined_id,
            start: start.clone(),
            end: end.clone(),
            link: raw.link,
            label: raw.text.clone(),
            label_type: raw.label_type,
            css_compiled_styles: own(&raw.classes),
            style,
            animate: raw.animate.clone(),
            animation: raw.animation.clone(),
            curve,
            metadata: raw.metadata.clone(),
        });
    }

    // Containers are numbered in the order they were written: a walk of the nesting, parents
    // first. `subgraphs` itself is in the order the blocks closed, with a nested flow before its
    // parent.
    let mut children: HashMap<&JsText, Vec<&JsText>> = HashMap::new();
    for sg in subgraphs {
        if let Some(&parent) = parent_of.get(&sg.id) {
            children.entry(parent).or_default().push(&sg.id);
        }
    }
    let mut order: HashMap<&JsText, usize> = HashMap::new();
    for sg in subgraphs {
        if parent_of.contains_key(&sg.id) {
            continue;
        }
        let mut pending = vec![&sg.id];
        while let Some(id) = pending.pop() {
            if order.contains_key(id) {
                continue;
            }
            order.insert(id, order.len());
            pending.extend(children.get(id).into_iter().flatten().rev());
        }
    }

    // A node takes the colour of its kind, from a fixed slot, so editing the diagram around it
    // never recolours it. A collapsed container is drawn as a node but keeps its slot, so
    // collapsing one does not recolour the containers after it.
    let mut fallback = order.len();
    for node in &mut nodes {
        if node.is_group() || node.shape.is_collapsed() {
            let n = order.get(&node.id).copied().unwrap_or_else(|| {
                fallback += 1;
                fallback - 1
            });
            node.color_index = Some(container_slot(n, palette_length));
        } else if db.connectors.contains(&node.id) {
            node.kind = Some(VertexKind::Connector);
        } else {
            node.kind = db.nodes.get(&node.id).map(AgentflowModel::vertex_kind);
        }
    }

    (AgentGraph { nodes, edges }, diagnostics)
}
