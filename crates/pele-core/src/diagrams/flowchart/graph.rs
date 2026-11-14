//! Pele's `buildFlowGraph` (`src/diagrams/flowchart/graph.ts`): the nodes and edges that are
//! drawn, with group membership, collapsed subgraphs, class styles and edge defaults resolved.

use super::shapes::ShapeName;
use super::{ArrowKind, Direction, FlowNode, FlowchartModel, LabelType, Link, LinkTarget, Stroke};
use crate::input::{JsObject, JsText, JsValue, Units};
use std::collections::{HashMap, HashSet};

/// Pele's `GraphNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphNode {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) shape: ShapeName,
    pub(crate) is_group: bool,
    pub(crate) parent_id: Option<JsText>,
    pub(crate) css_styles: Vec<JsText>,
    pub(crate) css_compiled_styles: Vec<JsText>,
    pub(crate) css_classes: JsText,
    pub(crate) dir: Option<Direction>,
    pub(crate) link: Option<JsText>,
    pub(crate) link_target: Option<LinkTarget>,
    pub(crate) tooltip: Option<JsText>,
    pub(crate) icon: Option<JsText>,
    pub(crate) pos: Option<JsText>,
    pub(crate) img: Option<JsText>,
    pub(crate) asset_width: Option<f64>,
    pub(crate) asset_height: Option<f64>,
    pub(crate) constraint: Option<JsText>,
    /// A subgraph's position in declaration order, for its colour.
    pub(crate) color_index: Option<usize>,
    pub(crate) metadata: Option<JsObject>,
}

/// An arrowhead at one end of a drawn edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrowHead {
    None,
    Point,
    Circle,
    Cross,
}

impl ArrowHead {
    pub(crate) fn name(self) -> &'static str {
        match self {
            ArrowHead::None => "none",
            ArrowHead::Point => "arrow_point",
            ArrowHead::Circle => "arrow_circle",
            ArrowHead::Cross => "arrow_cross",
        }
    }
}

/// Pele's `GraphEdge`. Its `type`, `thickness`, `pattern`, `minlen`, arrowheads and classes all
/// follow from the link.
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
}

impl GraphEdge {
    /// `arrowTypeStart` and `arrowTypeEnd`.
    pub(crate) fn arrows(&self) -> (ArrowHead, ArrowHead) {
        let Link::Valid { arrow, stroke, .. } = self.link else {
            return (ArrowHead::None, ArrowHead::Point);
        };
        if stroke == Stroke::Invisible {
            return (ArrowHead::None, ArrowHead::None);
        }
        match arrow {
            ArrowKind::Open => (ArrowHead::None, ArrowHead::None),
            ArrowKind::Point => (ArrowHead::None, ArrowHead::Point),
            ArrowKind::Circle => (ArrowHead::None, ArrowHead::Circle),
            ArrowKind::Cross => (ArrowHead::None, ArrowHead::Cross),
            ArrowKind::DoublePoint => (ArrowHead::Point, ArrowHead::Point),
            ArrowKind::DoubleCircle => (ArrowHead::Circle, ArrowHead::Circle),
            ArrowKind::DoubleCross => (ArrowHead::Cross, ArrowHead::Cross),
        }
    }

    /// The edge's `classes`: none for an invisible link.
    pub(crate) fn classes(&self) -> &'static str {
        match self.link {
            Link::Valid { stroke: Stroke::Invisible, .. } => "",
            _ => "edge-thickness-normal edge-pattern-solid flowchart-link",
        }
    }
}

/// Pele's `FlowGraph`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

fn shape_of(vertex: &FlowNode) -> ShapeName {
    if vertex.img.as_ref().is_some_and(|img| !img.is_empty()) {
        return ShapeName::IMAGE_SQUARE;
    }
    if vertex.icon.as_ref().is_some_and(|icon| !icon.is_empty()) {
        let form = |name: &str| vertex.form.as_ref().is_some_and(|form| form.is_str(name));
        return if form("circle") {
            ShapeName::ICON_CIRCLE
        } else if form("square") {
            ShapeName::ICON_SQUARE
        } else if form("rounded") {
            ShapeName::ICON_ROUNDED
        } else {
            ShapeName::ICON
        };
    }
    match vertex.shape {
        None => ShapeName::SQUARE_RECT,
        Some(shape) if shape == ShapeName::SQUARE => ShapeName::SQUARE_RECT,
        Some(shape) if shape == ShapeName::ROUND => ShapeName::ROUNDED_RECT,
        Some(shape) => shape,
    }
}

fn is_collapsed(metadata: Option<&JsObject>) -> bool {
    metadata
        .and_then(|metadata| metadata.get("view"))
        .and_then(JsValue::as_text)
        .is_some_and(|view| view.is_str("collapsed"))
}

/// The styles of the classes named, each trimmed: a class's own styles, then its text styles.
fn compiled<'a>(model: &FlowchartModel, class_names: impl Iterator<Item = &'a [u16]>) -> Vec<JsText> {
    let mut out = Vec::new();
    for name in class_names {
        let Some(def) = model.classes.get(name) else {
            continue;
        };
        out.extend(def.styles.iter().chain(&def.text_styles).map(|s| JsText::from(s.trim_js())));
    }
    out
}

static DEFAULT: std::sync::LazyLock<JsText> = std::sync::LazyLock::new(|| "default".into());
static NODE: std::sync::LazyLock<JsText> = std::sync::LazyLock::new(|| "node".into());

/// The outermost collapsed subgraph around a subgraph, resolved once per chain of parents and
/// remembered in `outermost`.
fn outermost_collapsed<'m>(
    model: &'m FlowchartModel,
    id: &'m [u16],
    subgraph_parent: &HashMap<&'m [u16], &'m JsText>,
    outermost: &mut HashMap<&'m [u16], Option<&'m [u16]>>,
) -> Option<&'m [u16]> {
    let mut chain: Vec<&[u16]> = Vec::new();
    let mut seen: HashSet<&[u16]> = HashSet::new();
    let mut current = Some(id);
    while let Some(at) = current.filter(|at| !outermost.contains_key(at) && !seen.contains(at)) {
        seen.insert(at);
        chain.push(at);
        current = subgraph_parent.get(at).map(|parent| &***parent);
    }
    let mut result = current.and_then(|at| outermost.get(at).copied().flatten());
    for &link in chain.iter().rev() {
        if result.is_none() && is_collapsed(model.subgraph(link).and_then(|sg| sg.metadata.as_ref())) {
            result = Some(link);
        }
        outermost.insert(link, result);
    }
    result
}

/// `a || b || c`.
fn first_truthy(values: [Option<JsValue>; 3]) -> Option<JsValue> {
    let [a, b, c] = values;
    a.filter(JsValue::truthy).or_else(|| b.filter(JsValue::truthy)).or(c)
}

/// Pele's `buildFlowGraph`. `default_curve` is the `flowchart.curve` config, when it is a string.
pub(crate) fn build_flow_graph(model: &FlowchartModel, default_curve: Option<&JsText>) -> FlowGraph {
    let subgraphs = &model.subgraphs;

    let mut subgraph_parent: HashMap<&[u16], &JsText> = HashMap::new();
    for sg in subgraphs {
        for child in &sg.nodes {
            if model.subgraph(child).is_some() {
                subgraph_parent.insert(child, &sg.id);
            }
        }
    }

    let mut declaration_index: HashMap<&[u16], usize> = HashMap::new();
    let mut children_of: HashMap<&[u16], Vec<&JsText>> = HashMap::new();
    for sg in subgraphs {
        if let Some(parent) = subgraph_parent.get(&*sg.id) {
            children_of.entry(parent).or_default().push(&sg.id);
        }
    }
    let mut next_index = 0;
    for sg in subgraphs {
        if subgraph_parent.contains_key(&*sg.id) {
            continue;
        }
        let mut pending: Vec<&JsText> = vec![&sg.id];
        while let Some(id) = pending.pop() {
            if declaration_index.contains_key(&**id) {
                continue;
            }
            declaration_index.insert(id, next_index);
            next_index += 1;
            if let Some(kids) = children_of.get(&**id) {
                pending.extend(kids.iter().rev());
            }
        }
    }

    let any_collapsed = subgraphs.iter().any(|sg| is_collapsed(sg.metadata.as_ref()));
    let mut outermost: HashMap<&[u16], Option<&[u16]>> = HashMap::new();

    let mut hidden: HashSet<&[u16]> = HashSet::new();
    let mut collapsed_into: HashMap<&[u16], &[u16]> = HashMap::new();
    if any_collapsed {
        for sg in subgraphs {
            let Some(ancestor) = outermost_collapsed(model, &sg.id, &subgraph_parent, &mut outermost) else {
                continue;
            };
            if *sg.id != *ancestor {
                hidden.insert(&sg.id);
                collapsed_into.insert(&sg.id, ancestor);
            }
            for child in &sg.nodes {
                if **child == *ancestor {
                    continue;
                }
                hidden.insert(child);
                collapsed_into.insert(child, ancestor);
            }
        }
    }

    let mut parent_of: HashMap<&[u16], &JsText> = HashMap::new();
    let mut groups: HashSet<&[u16]> = HashSet::new();
    for sg in subgraphs.iter().rev() {
        if hidden.contains(&*sg.id) {
            continue;
        }
        if !sg.nodes.is_empty() {
            groups.insert(&sg.id);
        }
        for id in &sg.nodes {
            parent_of.insert(id, &sg.id);
        }
    }

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut by_id: HashMap<&[u16], usize> = HashMap::new();
    for sg in subgraphs.iter().rev() {
        if hidden.contains(&*sg.id) {
            continue;
        }
        let collapsed = is_collapsed(sg.metadata.as_ref());
        by_id.insert(&sg.id, nodes.len());
        nodes.push(GraphNode {
            id: sg.id.clone(),
            label: sg.title.clone(),
            label_type: sg.label_type,
            shape: if collapsed { ShapeName::COLLAPSED_GROUP } else { ShapeName::RECT },
            is_group: !collapsed,
            parent_id: parent_of.get(&*sg.id).map(|&parent| parent.clone()),
            css_styles: Vec::new(),
            css_compiled_styles: compiled(model, sg.classes.iter().map(|c| &**c)),
            css_classes: join(&sg.classes, ""),
            dir: sg.dir,
            link: None,
            link_target: None,
            tooltip: None,
            icon: None,
            pos: None,
            img: None,
            asset_width: None,
            asset_height: None,
            constraint: None,
            color_index: declaration_index.get(&*sg.id).copied(),
            metadata: if collapsed { None } else { sg.metadata.clone() },
        });
    }

    for vertex in model.nodes.values() {
        if hidden.contains(&*vertex.id) {
            continue;
        }
        if let Some(&at) = by_id.get(&*vertex.id) {
            let existing = &mut nodes[at];
            existing.css_styles = vertex.styles.clone();
            existing.css_compiled_styles = compiled(model, vertex.classes.iter().map(|c| &**c));
            existing.css_classes = join(&vertex.classes, "");
            continue;
        }
        let is_group = groups.contains(&*vertex.id);
        nodes.push(GraphNode {
            id: vertex.id.clone(),
            label: vertex.text.clone(),
            label_type: vertex.label_type,
            shape: if is_group { ShapeName::RECT } else { shape_of(vertex) },
            is_group,
            parent_id: parent_of.get(&*vertex.id).map(|&parent| parent.clone()),
            css_styles: vertex.styles.clone(),
            css_compiled_styles: compiled(
                model,
                [&**DEFAULT, &**NODE].into_iter().chain(vertex.classes.iter().map(|c| &**c)),
            ),
            css_classes: join(&vertex.classes, "default "),
            dir: None,
            link: vertex.link.clone(),
            link_target: vertex.link_target,
            tooltip: model.tooltips.get(&*vertex.id).cloned(),
            icon: vertex.icon.clone(),
            pos: vertex.pos.clone(),
            img: vertex.img.clone(),
            asset_width: vertex.asset_width,
            asset_height: vertex.asset_height,
            constraint: vertex.constraint.clone(),
            color_index: None,
            metadata: None,
        });
    }

    let mut edges = Vec::new();
    for raw in &model.edges {
        let start = collapsed_into.get(&*raw.start).copied().unwrap_or(&raw.start);
        let end = collapsed_into.get(&*raw.end).copied().unwrap_or(&raw.end);
        if start == end && (collapsed_into.contains_key(&*raw.start) || collapsed_into.contains_key(&*raw.end)) {
            continue;
        }
        let mut style = model.default_edge_style.clone().unwrap_or_default();
        style.extend(raw.style.iter().flatten().cloned());
        edges.push(GraphEdge {
            id: raw.id.clone(),
            is_user_defined_id: raw.is_user_defined_id,
            start: start.into(),
            end: end.into(),
            link: raw.link,
            label: raw.text.clone(),
            label_type: raw.label_type,
            css_compiled_styles: compiled(model, raw.classes.iter().map(|c| &**c)),
            style,
            animate: raw.animate.clone(),
            animation: raw.animation.clone(),
            curve: first_truthy([
                raw.interpolate.clone(),
                model.default_interpolate.clone().map(JsValue::String),
                default_curve.cloned().map(JsValue::String),
            ]),
        });
    }

    FlowGraph { nodes, edges }
}

/// `prefix + names.join(' ')`.
fn join(names: &[JsText], prefix: &str) -> JsText {
    let mut out = JsText::from(prefix);
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            out.push(u16::from(b' '));
        }
        out.push_units(name);
    }
    out
}
