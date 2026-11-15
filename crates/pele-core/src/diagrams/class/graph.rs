//! The nodes and edges a class diagram draws, ported from Pele's `src/diagrams/class/graph.ts`.

use super::{ClassModel, LineType, RelationEnd};
use crate::input::{JsText, Units};
use std::collections::HashMap;

/// What a node of the class graph is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeShape {
    /// A namespace.
    Rect,
    ClassBox,
    Note,
    /// The far end of a lollipop.
    Interface,
}

impl NodeShape {
    pub(crate) fn name(self) -> &'static str {
        match self {
            NodeShape::Rect => "rect",
            NodeShape::ClassBox => "classBox",
            NodeShape::Note => "note",
            NodeShape::Interface => "interface",
        }
    }
}

/// How an edge's line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pattern {
    Solid,
    Dashed,
    /// The line from a note to its class.
    Dotted,
}

impl Pattern {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Pattern::Solid => "solid",
            Pattern::Dashed => "dashed",
            Pattern::Dotted => "dotted",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphNode {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) is_group: bool,
    pub(crate) shape: NodeShape,
    pub(crate) parent_id: Option<JsText>,
    /// The namespace node this one is drawn in.
    pub(crate) parent: Option<usize>,
    /// The class this node draws, by its key in the model.
    pub(crate) class: Option<usize>,
    /// Position among the classes, in declaration order.
    pub(crate) color_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphEdge {
    pub(crate) id: JsText,
    pub(crate) start: JsText,
    pub(crate) end: JsText,
    /// Indexes into the node list. Ids alone are ambiguous: a class may be named `note0` or
    /// `interface0`. An end that names no node is `None`, and the edge is not drawn.
    pub(crate) source: Option<usize>,
    pub(crate) target: Option<usize>,
    pub(crate) label: Option<JsText>,
    pub(crate) arrow_type_start: RelationEnd,
    pub(crate) arrow_type_end: RelationEnd,
    pub(crate) start_label_right: JsText,
    pub(crate) end_label_left: JsText,
    pub(crate) pattern: Pattern,
}

pub(crate) struct ClassGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

/// Pele's `buildClassGraph`: the nodes and edges a model draws. With `hierarchical` off, only
/// the namespaces the source declares are kept and each is labeled with its full name.
pub(crate) fn build_class_graph(model: &ClassModel, hierarchical: bool) -> ClassGraph {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut namespace_at: HashMap<&[u16], usize> = HashMap::new();
    let mut class_at: HashMap<&[u16], usize> = HashMap::new();
    let mut interface_at: HashMap<&[u16], usize> = HashMap::new();

    // The declared namespace that stands for each one walked so far.
    let mut declared: HashMap<JsText, Option<JsText>> = HashMap::new();
    let mut declared_ancestor = |id: Option<&JsText>| -> Option<JsText> {
        let id = id?;
        if hierarchical {
            return Some(id.clone());
        }
        let mut chain: Vec<JsText> = Vec::new();
        let mut current = Some(id.clone());
        let mut found = None;
        while let Some(name) = current.filter(|name| !name.is_empty()) {
            if let Some(known) = declared.get(&name) {
                found = known.clone();
                break;
            }
            let Some(namespace) = model.namespaces.get(&name) else {
                break;
            };
            if namespace.explicit {
                found = Some(name);
                break;
            }
            current = namespace.parent.clone();
            chain.push(name);
        }
        for visited in chain {
            declared.insert(visited, found.clone());
        }
        found
    };

    for namespace in model.namespaces.values() {
        if !hierarchical && !namespace.explicit {
            continue;
        }
        namespace_at.insert(&namespace.id, nodes.len());
        nodes.push(GraphNode {
            id: namespace.id.clone(),
            label: if hierarchical { namespace.label.clone() } else { namespace.id.clone() },
            is_group: true,
            shape: NodeShape::Rect,
            parent_id: if hierarchical { namespace.parent.clone() } else { None },
            parent: None,
            class: None,
            color_index: None,
        });
    }
    let group =
        |namespace_at: &HashMap<&[u16], usize>, id: Option<&JsText>| id.and_then(|id| namespace_at.get(&**id).copied());
    for node in &mut nodes {
        node.parent = group(&namespace_at, node.parent_id.as_ref());
    }

    for (color_index, class) in model.classes.values().enumerate() {
        let parent_id = declared_ancestor(class.parent.as_ref());
        class_at.insert(&class.id, nodes.len());
        let parent = group(&namespace_at, parent_id.as_ref());
        nodes.push(GraphNode {
            id: class.id.clone(),
            label: class.label.clone(),
            is_group: false,
            shape: NodeShape::ClassBox,
            parent_id,
            parent,
            class: Some(color_index),
            color_index: Some(color_index),
        });
    }

    for note in model.notes.values() {
        let parent_id = declared_ancestor(note.parent.as_ref());
        let target = note.class.as_ref().and_then(|class| class_at.get(&**class).copied());
        if let Some(target) = target {
            edges.push(GraphEdge {
                id: format!("edgeNote{}", note.index).as_str().into(),
                start: note.id.clone(),
                end: nodes[target].id.clone(),
                source: Some(nodes.len()),
                target: Some(target),
                label: None,
                arrow_type_start: RelationEnd::None,
                arrow_type_end: RelationEnd::None,
                start_label_right: JsText::default(),
                end_label_left: JsText::default(),
                pattern: Pattern::Dotted,
            });
        }
        let parent = group(&namespace_at, parent_id.as_ref());
        nodes.push(GraphNode {
            id: note.id.clone(),
            label: note.text.clone(),
            is_group: false,
            shape: NodeShape::Note,
            parent_id,
            parent,
            class: None,
            color_index: None,
        });
    }

    // Mermaid leaves every lollipop interface outside the namespaces. Here it stays beside its class.
    for item in &model.interfaces {
        let owner = class_at.get(&*item.class_id).copied();
        interface_at.insert(&item.id, nodes.len());
        nodes.push(GraphNode {
            id: item.id.clone(),
            label: item.label.clone(),
            is_group: false,
            shape: NodeShape::Interface,
            parent_id: None,
            parent: owner.and_then(|owner| nodes[owner].parent),
            class: None,
            color_index: None,
        });
    }

    for (count, relation) in model.relations.iter().enumerate() {
        let (type1, type2) = (relation.type1, relation.type2);
        // addRelation swaps in an interface for the lollipop end of a relation with no other marker.
        let start_interface = type1 == RelationEnd::Lollipop && type2 == RelationEnd::None;
        let end_interface = !start_interface && type2 == RelationEnd::Lollipop && type1 == RelationEnd::None;
        let start_at = if start_interface { &interface_at } else { &class_at };
        let end_at = if end_interface { &interface_at } else { &class_at };
        let mut id = JsText::from("id_");
        id.push_units(&relation.id1);
        id.push(95);
        id.push_units(&relation.id2);
        id.push_str(&format!("_{}", count + 1));
        let title = |text: &JsText| if text.is_str("none") { JsText::default() } else { text.clone() };
        edges.push(GraphEdge {
            id,
            start: relation.id1.clone(),
            end: relation.id2.clone(),
            source: start_at.get(&*relation.id1).copied(),
            target: end_at.get(&*relation.id2).copied(),
            label: relation.title.clone(),
            arrow_type_start: type1,
            arrow_type_end: type2,
            start_label_right: title(&relation.relation_title1),
            end_label_left: title(&relation.relation_title2),
            pattern: if relation.line_type == LineType::Dotted { Pattern::Dashed } else { Pattern::Solid },
        });
    }

    ClassGraph { nodes, edges }
}
