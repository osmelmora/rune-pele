//! What an ER diagram draws, ported from Pele's `src/diagrams/er/graph.ts`: subgraphs as groups,
//! entities as nodes with their class styles, and relationships as edges with a marker at each
//! end.

use super::{Cardinality, Direction, ErAttribute, ErModel, Identification};
use crate::input::{JsText, Units, split_unit};
use std::collections::{HashMap, HashSet};

/// Pele's `ErGraphNode`. A group's label type is `markdown`, as every entity's is.
pub(crate) struct ErGraphNode<'a> {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) alias: JsText,
    pub(crate) attributes: &'a [ErAttribute],
    pub(crate) is_group: bool,
    pub(crate) parent_id: Option<JsText>,
    pub(crate) css_styles: &'a [JsText],
    pub(crate) css_compiled_styles: Vec<JsText>,
    pub(crate) css_classes: JsText,
    pub(crate) dir: Option<Direction>,
}

/// Pele's `ErGraphEdge`.
pub(crate) struct ErGraphEdge {
    pub(crate) id: JsText,
    pub(crate) start: JsText,
    pub(crate) end: JsText,
    pub(crate) label: JsText,
    pub(crate) arrow_type_start: Cardinality,
    pub(crate) arrow_type_end: Cardinality,
    pub(crate) pattern: Identification,
}

pub(crate) struct ErGraph<'a> {
    pub(crate) nodes: Vec<ErGraphNode<'a>>,
    pub(crate) edges: Vec<ErGraphEdge>,
}

/// Pele's `buildErGraph`.
pub(crate) fn build_er_graph(model: &ErModel) -> ErGraph<'_> {
    let compiled = |names: &mut dyn Iterator<Item = &[u16]>| {
        let mut out = Vec::new();
        for name in names {
            if let Some(def) = model.classes.get(name) {
                out.extend(def.styles.iter().map(|style| style.trim_js().into()));
                out.extend(def.text_styles.iter().map(|style| style.trim_js().into()));
            }
        }
        out
    };

    let mut parent_of: HashMap<&[u16], &JsText> = HashMap::new();
    let mut subgraph_ids: HashSet<&[u16]> = HashSet::new();
    for subgraph in model.subgraphs.iter().rev() {
        subgraph_ids.insert(&subgraph.id);
        for id in &subgraph.nodes {
            parent_of.insert(id, &subgraph.id);
        }
    }

    let mut nodes = Vec::new();
    for subgraph in model.subgraphs.iter().rev() {
        let mut css_classes = JsText::default();
        for (index, name) in subgraph.classes.iter().enumerate() {
            if index > 0 {
                css_classes.push(32);
            }
            css_classes.push_units(name);
        }
        nodes.push(ErGraphNode {
            id: subgraph.id.clone(),
            label: subgraph.title.clone(),
            alias: JsText::default(),
            attributes: &[],
            is_group: true,
            parent_id: parent_of.get(&*subgraph.id).map(|&id| id.clone()),
            css_styles: &subgraph.css_styles,
            css_compiled_styles: compiled(&mut subgraph.classes.iter().map(|name| &**name)),
            css_classes,
            dir: subgraph.dir,
        });
    }

    // A name that is also a subgraph id stands for the subgraph. Mermaid drops such an entity
    // and leaves relationships written before the subgraph pointing at nothing; here they
    // follow it.
    let mut moved: HashMap<&[u16], &[u16]> = HashMap::new();
    for (name, entity) in &model.entities {
        if subgraph_ids.contains(&**name) {
            moved.insert(&entity.id, name);
            continue;
        }
        nodes.push(ErGraphNode {
            id: entity.id.clone(),
            label: entity.label.clone(),
            alias: entity.alias.clone(),
            attributes: &entity.attributes,
            is_group: false,
            parent_id: parent_of.get(&**name).map(|&id| id.clone()),
            css_styles: &entity.css_styles,
            css_compiled_styles: compiled(&mut split_unit(&entity.css_classes, 32).into_iter()),
            css_classes: entity.css_classes.clone(),
            dir: None,
        });
    }

    let edges = model
        .relationships
        .iter()
        .enumerate()
        .map(|(index, relationship)| {
            let mut id = JsText::from("id_");
            id.push_units(&relationship.entity_a);
            id.push(95);
            id.push_units(&relationship.entity_b);
            id.push(95);
            id.push_str(&index.to_string());
            let follow = |end: &JsText| moved.get(&**end).map_or_else(|| end.clone(), |&name| name.into());
            ErGraphEdge {
                id,
                start: follow(&relationship.entity_a),
                end: follow(&relationship.entity_b),
                label: relationship.role_a.clone(),
                arrow_type_start: relationship.rel_spec.card_b,
                arrow_type_end: relationship.rel_spec.card_a,
                pattern: relationship.rel_spec.rel_type,
            }
        })
        .collect();

    ErGraph { nodes, edges }
}
