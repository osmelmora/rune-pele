//! The store the ER parser fills, ported from Pele's `src/diagrams/er/db.ts`.

use super::{Direction, ErAttribute, ErClassDef, ErEntity, ErModel, ErRelationship, ErSubgraph, RelSpec};
use crate::input::{JsText, Units, is_space};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

/// What the parser hands over for one attribute. Keys and a comment are there only when written.
pub(super) struct ParsedAttribute {
    pub(super) kind: JsText,
    pub(super) name: JsText,
    pub(super) keys: Option<Vec<JsText>>,
    pub(super) comment: Option<JsText>,
}

/// The value of one statement inside a subgraph.
pub(super) enum DocItem {
    /// An entity name, an accessibility text, a keyword or a nested subgraph's id.
    Name(JsText),
    Dir(Direction),
}

/// `text.replace(/\n\s+/g, '\n')`
fn collapse_indents(text: &[u16]) -> JsText {
    let mut out = JsText::default();
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        if text[at] == 10 && text.at(at + 1).is_some_and(is_space) {
            at = text.run(at + 1, is_space);
        } else {
            at += 1;
        }
    }
    out
}

/// `style.replace('fill', 'bgFill')`: the first `fill` only.
fn background_fill(style: &[u16]) -> JsText {
    match style.find_str("fill", 0) {
        Some(at) => {
            let mut out = JsText::from(&style[..at]);
            out.push_str("bgFill");
            out.push_units(&style[at + 4..]);
            out
        }
        None => style.into(),
    }
}

pub(super) struct ErDb {
    pub(super) model: ErModel,
    /// Each subgraph's place in the model, by id. A later subgraph with the same id replaces it.
    subgraph_lookup: HashMap<JsText, usize>,
    /// The names some subgraph already holds.
    claimed: HashSet<JsText>,
}

impl ErDb {
    pub(super) fn new() -> Self {
        ErDb {
            model: ErModel {
                direction: Direction::TB,
                entities: IndexMap::new(),
                relationships: Vec::new(),
                classes: IndexMap::new(),
                subgraphs: Vec::new(),
                title: None,
                acc_title: None,
                acc_descr: None,
            },
            subgraph_lookup: HashMap::new(),
            claimed: HashSet::new(),
        }
    }

    pub(super) fn add_entity(&mut self, name: &[u16], alias: &[u16]) -> &mut ErEntity {
        let count = self.model.entities.len();
        let entity = self.model.entities.entry(name.into()).or_insert_with(|| {
            let mut id = JsText::from("entity-");
            id.push_units(name);
            id.push(45);
            id.push_str(&count.to_string());
            ErEntity {
                id,
                label: name.into(),
                alias: JsText::default(),
                attributes: Vec::new(),
                css_classes: "default".into(),
                css_styles: Vec::new(),
            }
        });
        if entity.alias.is_empty() && !alias.is_empty() {
            entity.alias = alias.into();
        }
        entity
    }

    pub(super) fn add_attributes(&mut self, name: &[u16], attributes: Vec<ParsedAttribute>) {
        let entity = self.add_entity(name, &[]);
        for attribute in attributes {
            entity.attributes.push(ErAttribute {
                kind: attribute.kind,
                name: attribute.name,
                keys: attribute.keys.unwrap_or_default(),
                comment: attribute.comment.unwrap_or_default(),
            });
        }
    }

    pub(super) fn add_relationship(&mut self, a: &[u16], role_a: JsText, b: &[u16], rel_spec: RelSpec) {
        let mut end = |name: &[u16]| {
            if self.subgraph_lookup.contains_key(name) { name.into() } else { self.add_entity(name, &[]).id.clone() }
        };
        let entity_a = end(a);
        let entity_b = end(b);
        self.model.relationships.push(ErRelationship { entity_a, role_a, entity_b, rel_spec });
    }

    pub(super) fn add_css_styles(&mut self, ids: &[JsText], styles: &[JsText]) {
        for id in ids {
            if let Some(entity) = self.model.entities.get_mut(id) {
                entity.css_styles.extend(styles.iter().cloned());
            }
            if let Some(&at) = self.subgraph_lookup.get(id) {
                self.model.subgraphs[at].css_styles.extend(styles.iter().cloned());
            }
        }
    }

    pub(super) fn add_class(&mut self, ids: &[JsText], styles: &[JsText]) {
        for id in ids {
            let def = self.model.classes.entry(id.clone()).or_insert_with(|| ErClassDef {
                id: id.clone(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
            for style in styles {
                if style.has("color") {
                    def.text_styles.push(background_fill(style));
                }
                def.styles.push(style.clone());
            }
        }
    }

    pub(super) fn set_class(&mut self, ids: &[JsText], names: &[JsText]) {
        for id in ids {
            if let Some(entity) = self.model.entities.get_mut(id) {
                for name in names {
                    entity.css_classes.push(32);
                    entity.css_classes.push_units(name);
                }
            }
            if let Some(&at) = self.subgraph_lookup.get(id) {
                self.model.subgraphs[at].classes.extend(names.iter().cloned());
            }
        }
    }

    /// Answers the subgraph's id, trimmed. A name belongs to the first subgraph that lists it.
    pub(super) fn add_subgraph(&mut self, id: &[u16], list: Vec<DocItem>, title: &[u16]) -> JsText {
        let id: JsText = id.trim_js().into();
        let mut seen: HashSet<JsText> = HashSet::new();
        let mut nodes: Vec<JsText> = Vec::new();
        let mut dir = None;
        for item in list {
            let item = match item {
                DocItem::Dir(value) => {
                    dir = Some(value);
                    continue;
                }
                DocItem::Name(item) => item,
            };
            let trimmed = item.trim_js();
            if trimmed.is_empty() || !seen.insert(trimmed.into()) {
                continue;
            }
            if !self.claimed.contains(&item) {
                nodes.push(item);
            }
        }
        self.claimed.extend(nodes.iter().cloned());
        let subgraph = ErSubgraph {
            id: id.clone(),
            nodes,
            title: title.trim_js().into(),
            classes: Vec::new(),
            css_styles: Vec::new(),
            dir,
        };
        self.model.subgraphs.push(subgraph);
        self.subgraph_lookup.insert(id.clone(), self.model.subgraphs.len() - 1);
        id
    }

    pub(super) fn set_acc_title(&mut self, text: &[u16]) {
        self.model.acc_title = Some(text.trim_start_js().into());
    }

    pub(super) fn set_acc_description(&mut self, text: &[u16]) {
        self.model.acc_descr = Some(collapse_indents(text));
    }
}
