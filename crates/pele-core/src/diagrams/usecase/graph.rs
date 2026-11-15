//! What a use case diagram draws, ported from Pele's `src/diagrams/usecase/graph.ts`: every
//! actor, use case, note, JSON table and boundary as a node with its class styles, and every
//! relationship and note connector as an edge with its markers. Pele ports `getData` from
//! Mermaid's use case DB here, and keeps Mermaid's names for shapes and markers.

use super::json::{Json, Node};
use super::{ActorType, Animation, Arrow, BoundaryType, LabelType, Relationship, RelationshipType, UsecaseModel};
use crate::input::{JsText, Units};
use crate::js;
use indexmap::IndexMap;

/// One leaf of a JSON object, as a row of its table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JsonRow {
    /// Blank on the second and later values of an array.
    pub(crate) key: JsText,
    /// The key again, for every row.
    pub(crate) accessible_key: JsText,
    pub(crate) value: JsText,
}

/// What a node is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeShape {
    Actor(ActorType),
    Ellipse,
    /// An ellipse with the slash of a business use case.
    Business,
    Rect,
    Note,
    JsonTable,
    SystemBoundary,
}

impl NodeShape {
    /// Mermaid's name for the shape.
    pub(crate) fn name(self) -> &'static str {
        match self {
            NodeShape::Actor(ActorType::Normal) => "usecaseActor",
            NodeShape::Actor(ActorType::Hollow) => "usecaseActorHollow",
            NodeShape::Actor(ActorType::Awesome) => "usecaseActorAwesome",
            NodeShape::Actor(ActorType::Icon) => "usecaseActorIcon",
            NodeShape::Ellipse => "usecaseEllipse",
            NodeShape::Business => "usecaseBusiness",
            NodeShape::Rect => "rect",
            NodeShape::Note => "note",
            NodeShape::JsonTable => "usecaseJsonTable",
            NodeShape::SystemBoundary => "usecaseSystemBoundary",
        }
    }
}

/// Pele's `UsecaseGraphNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphNode {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) label_type: LabelType,
    pub(crate) shape: NodeShape,
    pub(crate) is_group: bool,
    pub(crate) padding: f64,
    pub(crate) css_classes: JsText,
    pub(crate) css_styles: Vec<JsText>,
    pub(crate) css_compiled_styles: Vec<JsText>,
    pub(crate) business: Option<bool>,
    pub(crate) icon: Option<JsText>,
    pub(crate) stereotype: Option<JsText>,
    pub(crate) parent_id: Option<JsText>,
    pub(crate) note_target: Option<JsText>,
    pub(crate) json_rows: Option<Vec<JsonRow>>,
    pub(crate) boundary_type: Option<BoundaryType>,
}

/// What an edge says about its two ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EdgeKind {
    Relationship(RelationshipType),
    /// The dotted line from a note to what it is about.
    Note,
}

impl EdgeKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            EdgeKind::Relationship(kind) => kind.name(),
            EdgeKind::Note => "note",
        }
    }

    /// Include and extend, which are labelled with their kind.
    pub(crate) fn semantic(self) -> bool {
        matches!(self, EdgeKind::Relationship(RelationshipType::Include | RelationshipType::Extend))
    }
}

/// What is drawn at an end of an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    None,
    Point,
    Circle,
    Cross,
    /// The hollow triangle of a generalization.
    Extension,
}

impl Mark {
    /// Mermaid's name for the marker.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Mark::None => "none",
            Mark::Point => "arrow_point",
            Mark::Circle => "arrow_circle",
            Mark::Cross => "arrow_cross",
            Mark::Extension => "extension",
        }
    }
}

/// Pele's `UsecaseGraphEdge`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphEdge {
    pub(crate) id: JsText,
    pub(crate) start: JsText,
    pub(crate) end: JsText,
    pub(crate) kind: EdgeKind,
    pub(crate) dotted: bool,
    pub(crate) arrow_start: Mark,
    pub(crate) arrow_end: Mark,
    pub(crate) label: Option<JsText>,
    pub(crate) label_type: Option<LabelType>,
    pub(crate) classes: JsText,
    pub(crate) style: Vec<JsText>,
    pub(crate) css_compiled_styles: Vec<JsText>,
    pub(crate) animate: bool,
    pub(crate) animation: Option<Animation>,
    pub(crate) minlen: usize,
    pub(crate) explicit_id: bool,
    pub(crate) internal: bool,
}

pub(crate) struct UsecaseGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

/// Pele's `scalar`: a JSON value that is no object or array, as text.
fn scalar(node: &Node) -> JsText {
    match node {
        Node::String(text) => text.clone(),
        Node::Null => "null".into(),
        Node::Bool(flag) => (if *flag { "true" } else { "false" }).into(),
        Node::Number(value) => js::to_string(*value).as_str().into(),
        Node::Array(_) | Node::Object(_) => unreachable!("a container is not a scalar"),
    }
}

/// Pele's `flattenJsonRows`: one row for each leaf of a JSON object, depth first in the order
/// the keys were written. What is still to be read is kept on a list, so depth is limited by
/// memory and not by the call stack.
pub(crate) fn flatten_json_rows(json: &Json) -> Vec<JsonRow> {
    let mut rows = Vec::new();
    let mut stack: Vec<(usize, JsText)> = vec![(json.root(), JsText::default())];
    let row = |key: &JsText, value: JsText| JsonRow { key: key.clone(), accessible_key: key.clone(), value };
    while let Some((at, path)) = stack.pop() {
        let mut children: Vec<(usize, JsText)> = Vec::new();
        match json.node(at) {
            Node::Array(items) => {
                let leaf = |&item: &usize| !matches!(json.node(item), Node::Array(_) | Node::Object(_));
                if items.is_empty() {
                    rows.push(row(&path, "[]".into()));
                } else if items.iter().all(leaf) {
                    for (i, &item) in items.iter().enumerate() {
                        let key = if i == 0 { path.clone() } else { JsText::default() };
                        rows.push(JsonRow { key, accessible_key: path.clone(), value: scalar(json.node(item)) });
                    }
                } else {
                    for (i, &item) in items.iter().enumerate() {
                        let mut child = path.clone();
                        child.push_str(&format!("[{i}]"));
                        children.push((item, child));
                    }
                }
            }
            Node::Object(entries) => {
                if entries.is_empty() {
                    rows.push(row(&path, "{}".into()));
                }
                for (key, &value) in entries {
                    let mut child = path.clone();
                    if !path.is_empty() {
                        child.push(u16::from(b'.'));
                    }
                    child.push_units(key);
                    children.push((value, child));
                }
            }
            leaf => rows.push(row(&path, scalar(leaf))),
        }
        stack.extend(children.into_iter().rev());
    }
    rows
}

/// Pele's `classList`: the names that are not empty, with a space between them.
fn class_list<'a>(names: impl IntoIterator<Item = &'a [u16]>) -> JsText {
    let mut out = JsText::default();
    for name in names.into_iter().filter(|name| !name.is_empty()) {
        if !out.is_empty() {
            out.push(32);
        }
        out.push_units(name);
    }
    out
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn markers(relationship: &Relationship) -> (Mark, Mark) {
    match (relationship.kind, relationship.arrow) {
        (RelationshipType::Generalization, _) => (Mark::None, Mark::Extension),
        (RelationshipType::Include | RelationshipType::Extend, _) => (Mark::None, Mark::Point),
        (RelationshipType::Association, Arrow::Forward) => (Mark::None, Mark::Point),
        (RelationshipType::Association, Arrow::Backward) => (Mark::Point, Mark::None),
        (RelationshipType::Association, Arrow::Plain) => (Mark::None, Mark::None),
        (RelationshipType::Association, Arrow::Circle) => (Mark::None, Mark::Circle),
        (RelationshipType::Association, Arrow::Cross) => (Mark::None, Mark::Cross),
        (RelationshipType::Association, Arrow::CircleReversed) => (Mark::Circle, Mark::None),
        (RelationshipType::Association, Arrow::CrossReversed) => (Mark::Cross, Mark::None),
    }
}

/// Pele's `buildUsecaseGraph`.
pub(crate) fn build_usecase_graph(model: &UsecaseModel) -> UsecaseGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // A later class wins a property but keeps the place the property first took.
    let compiled = |class_names: &[JsText]| -> Vec<JsText> {
        let mut out: IndexMap<JsText, JsText> = IndexMap::new();
        let default = JsText::from("default");
        for name in std::iter::once(&default).chain(class_names) {
            let Some(definition) = model.class_defs.get(name) else {
                continue;
            };
            for raw in &definition.styles {
                let style = raw.trim_js();
                let property = match style.find_unit(u16::from(b':'), 0) {
                    Some(colon) => style[..colon].trim_js(),
                    None => style,
                };
                if !property.is_empty() {
                    out.insert(property.into(), style.into());
                }
            }
        }
        out.into_values().collect()
    };
    let classes = |own: &[&str], author: &[JsText], last: Option<String>| {
        let own: Vec<Vec<u16>> = own.iter().map(|name| units(name)).collect();
        let last = last.map(|name| units(&name));
        class_list(own.iter().map(|name| &name[..]).chain(author.iter().map(|name| &name[..])).chain(last.as_deref()))
    };
    let filled = |text: &Option<JsText>| text.clone().filter(|text| !text.is_empty());
    let plain = |id: &JsText, label: &JsText, label_type: LabelType, shape: NodeShape| GraphNode {
        id: id.clone(),
        label: label.clone(),
        label_type,
        shape,
        is_group: false,
        padding: 10.0,
        css_classes: JsText::default(),
        css_styles: Vec::new(),
        css_compiled_styles: Vec::new(),
        business: None,
        icon: None,
        stereotype: None,
        parent_id: None,
        note_target: None,
        json_rows: None,
        boundary_type: None,
    };

    for actor in model.actors.values() {
        let kind = format!("usecase-actor-{}", actor.kind.name());
        let business = if actor.business { "usecase-business" } else { "" };
        nodes.push(GraphNode {
            css_classes: classes(&["default", "usecase-actor", &kind, business], &actor.classes, None),
            css_styles: actor.styles.clone(),
            css_compiled_styles: compiled(&actor.classes),
            business: Some(actor.business),
            icon: filled(&actor.icon),
            stereotype: filled(&actor.stereotype),
            parent_id: filled(&actor.parent_id),
            ..plain(&actor.id, &actor.label, actor.label_type, NodeShape::Actor(actor.kind))
        });
    }
    for use_case in model.use_cases.values() {
        let ellipse = use_case.shape == super::UseCaseShape::Ellipse;
        let shape = match (ellipse, use_case.business) {
            (false, _) => NodeShape::Rect,
            (true, true) => NodeShape::Business,
            (true, false) => NodeShape::Ellipse,
        };
        let kind = format!("usecase-{}", use_case.shape.name());
        let business = if use_case.business { "usecase-business" } else { "" };
        nodes.push(GraphNode {
            padding: if ellipse { 20.0 } else { 10.0 },
            css_classes: classes(&["default", "usecase-element", &kind, business], &use_case.classes, None),
            css_styles: use_case.styles.clone(),
            css_compiled_styles: compiled(&use_case.classes),
            business: Some(use_case.business),
            stereotype: filled(&use_case.stereotype),
            parent_id: filled(&use_case.parent_id),
            ..plain(&use_case.id, &use_case.label, use_case.label_type, shape)
        });
    }
    for note in model.notes.values() {
        nodes.push(GraphNode {
            css_classes: "default usecase-note".into(),
            css_compiled_styles: compiled(&[]),
            note_target: Some(note.target.clone()),
            ..plain(&note.id, &note.label, note.label_type, NodeShape::Note)
        });
    }
    for json in model.json_nodes.values() {
        nodes.push(GraphNode {
            css_classes: classes(&["default", "usecase-json-table"], &json.classes, None),
            css_styles: json.styles.clone(),
            css_compiled_styles: compiled(&json.classes),
            json_rows: Some(flatten_json_rows(&json.value)),
            ..plain(&json.id, &json.id, LabelType::Text, NodeShape::JsonTable)
        });
    }
    for boundary in model.system_boundaries.values() {
        let kind = format!("system-boundary-{}", boundary.kind.name());
        nodes.push(GraphNode {
            is_group: true,
            padding: 20.0,
            css_classes: classes(&["default", "system-boundary", &kind], &boundary.classes, None),
            css_styles: boundary.styles.clone(),
            css_compiled_styles: compiled(&boundary.classes),
            boundary_type: Some(boundary.kind),
            ..plain(&boundary.id, &boundary.label, boundary.label_type, NodeShape::SystemBoundary)
        });
    }

    for relationship in &model.relationships {
        let kind = EdgeKind::Relationship(relationship.kind);
        let (arrow_start, arrow_end) = markers(relationship);
        let animated = relationship.animate || relationship.animation.is_some();
        let animation =
            animated.then(|| format!("edge-animation-{}", relationship.animation.unwrap_or(Animation::Fast).name()));
        let own = format!("relationship-{}", relationship.kind.name());
        // Include and extend are labelled with their kind, whatever label the relationship carries.
        let (label, label_type) = if kind.semantic() {
            (Some(relationship.kind.name().into()), Some(LabelType::Text))
        } else if relationship.kind == RelationshipType::Association {
            (filled(&relationship.label), relationship.label_type)
        } else {
            (None, None)
        };
        edges.push(GraphEdge {
            id: relationship.id.clone(),
            start: relationship.source.clone(),
            end: relationship.target.clone(),
            kind,
            dotted: kind.semantic(),
            arrow_start,
            arrow_end,
            label,
            label_type,
            classes: classes(&["default", "relationship", &own], &relationship.classes, animation),
            style: relationship.styles.clone(),
            css_compiled_styles: compiled(&relationship.classes),
            animate: relationship.animate,
            animation: relationship.animation,
            minlen: relationship.minlen,
            explicit_id: relationship.explicit_id,
            internal: false,
        });
    }
    for note in model.notes.values() {
        let mut id = note.id.clone();
        id.push_str("-edge");
        edges.push(GraphEdge {
            id,
            start: note.id.clone(),
            end: note.target.clone(),
            kind: EdgeKind::Note,
            dotted: true,
            arrow_start: Mark::None,
            arrow_end: Mark::None,
            label: None,
            label_type: None,
            classes: "default relationship relationship-note".into(),
            style: Vec::new(),
            css_compiled_styles: compiled(&[]),
            animate: false,
            animation: None,
            minlen: 1,
            explicit_id: false,
            internal: true,
        });
    }
    UsecaseGraph { nodes, edges }
}
