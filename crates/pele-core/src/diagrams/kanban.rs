//! Kanban boards, ported from Pele's `src/diagrams/kanban`. The shared outline grammar reads the
//! source, the db here turns its nodes into columns and cards, and `render` lays out the board.

pub(crate) mod json;
mod render;

use super::outline::{Decoration, OutlineDb, OutlineError, OutlineNode, parse_outline};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::{JsText, JsValue, parse_yaml, to_lowercase};
use crate::scene::Scene;
use crate::{Diagnostic, ErrorCode, RenderError};

/// Pele's `KanbanNode`: a column or a card.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KanbanNode {
    pub(crate) id: JsText,
    /// How far the node's line is set in, in UTF-16 code units.
    pub(crate) level: usize,
    pub(crate) label: JsText,
    pub(crate) icon: Option<JsText>,
    pub(crate) assigned: Option<JsText>,
    pub(crate) ticket: Option<JsText>,
    pub(crate) priority: Option<JsText>,
    pub(crate) css_classes: Option<JsText>,
}

/// A column and its cards, in source order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KanbanSection {
    pub(crate) node: KanbanNode,
    pub(crate) items: Vec<KanbanNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KanbanModel {
    pub(crate) title: Option<JsText>,
    pub(crate) sections: Vec<KanbanSection>,
}

/// Which node the source named last, which a decoration line applies to: always the last
/// column or the last card of the last column.
#[derive(Clone, Copy)]
enum Last {
    Section,
    Card,
}

/// Pele's `KanbanDb`.
struct KanbanDb {
    model: KanbanModel,
    last: Option<Last>,
    section_level: usize,
    /// The label of a card indented less than the first column, after which nothing may follow.
    misplaced: Option<JsText>,
}

fn syntax(message: String) -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(ErrorCode::Syntax, message))
}

fn semantic(message: String) -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message))
}

impl KanbanDb {
    fn last_node(&mut self) -> Option<&mut KanbanNode> {
        let section = self.model.sections.last_mut()?;
        match self.last? {
            Last::Section => Some(&mut section.node),
            Last::Card => section.items.last_mut(),
        }
    }

    /// The metadata of `@{ … }` laid over the node. Metadata that is not a mapping is ignored.
    fn apply_metadata(node: &mut KanbanNode, shape_data: &[u16]) -> Result<(), RenderError> {
        let mut yaml = Vec::with_capacity(shape_data.len() + 4);
        if shape_data.contains(&10) {
            yaml.extend_from_slice(shape_data);
            yaml.push(10);
        } else {
            yaml.extend("{\n".encode_utf16());
            yaml.extend_from_slice(shape_data);
            yaml.extend("\n}".encode_utf16());
        }
        let doc = parse_yaml(&yaml).map_err(|message| {
            let id = String::from_utf16_lossy(&node.id);
            syntax(format!("Metadata of \"{id}\" is not valid YAML. {message}"))
        })?;
        let JsValue::Object(doc) = doc else {
            return Ok(());
        };
        let field = |key: &str| doc.get(key).filter(|value| value.truthy()).map(JsValue::to_js_string);
        let shape = field("shape").unwrap_or_default();
        if to_lowercase(&shape) != *shape || shape.contains(&u16::from(b'_')) {
            let shape = String::from_utf16_lossy(&shape);
            return Err(semantic(format!("No such shape: {shape}. Shape names should be lowercase.")));
        }
        if let Some(label) = field("label") {
            node.label = label;
        }
        for (key, slot) in [
            ("icon", &mut node.icon),
            ("assigned", &mut node.assigned),
            ("ticket", &mut node.ticket),
            ("priority", &mut node.priority),
        ] {
            if let Some(value) = field(key) {
                *slot = Some(value);
            }
        }
        Ok(())
    }
}

impl OutlineDb for KanbanDb {
    type Error = RenderError;

    fn add_node(&mut self, outline: OutlineNode<'_>) -> Result<(), RenderError> {
        let mut node = KanbanNode {
            id: outline.id.into(),
            level: outline.level,
            label: outline.descr.into(),
            icon: None,
            assigned: None,
            ticket: None,
            priority: None,
            css_classes: None,
        };
        if let Some(shape_data) = outline.shape_data {
            Self::apply_metadata(&mut node, shape_data)?;
        }
        if let Some(label) = &self.misplaced {
            let label = String::from_utf16_lossy(label);
            return Err(semantic(format!("Items without section detected, found section (\"{label}\")")));
        }
        let sections = &mut self.model.sections;
        if sections.is_empty() {
            self.section_level = node.level;
        }
        if node.level == self.section_level {
            sections.push(KanbanSection { node, items: Vec::new() });
            self.last = Some(Last::Section);
        } else {
            if node.level < self.section_level {
                self.misplaced = Some(node.label.clone());
            }
            sections.last_mut().expect("the first node is a column").items.push(node);
            self.last = Some(Last::Card);
        }
        Ok(())
    }

    fn decorate_node(&mut self, decoration: Decoration<'_>) {
        let Some(node) = self.last_node() else {
            return;
        };
        match decoration {
            Decoration::Icon(icon) if !icon.is_empty() => node.icon = Some(icon.into()),
            Decoration::Class(class) if !class.is_empty() => node.css_classes = Some(class.into()),
            _ => {}
        }
    }
}

/// Pele's kanban `parse`: the board a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<KanbanModel, RenderError> {
    let mut db = KanbanDb {
        model: KanbanModel { title: cx.title.cloned(), sections: Vec::new() },
        last: None,
        section_level: 0,
        misplaced: None,
    };
    parse_outline(cx.text, &mut db, true).map_err(|error| match error {
        OutlineError::Syntax(error) => cx.syntax(error),
        OutlineError::Db(error) => error,
    })?;
    Ok(db.model)
}

pub(crate) struct Kanban;

impl Diagram for Kanban {
    type Model = KanbanModel;

    fn parse(cx: &ParseContext<'_>) -> Result<KanbanModel, RenderError> {
        parse(cx)
    }

    fn scene(model: &KanbanModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::draw(model, cx)
    }
}
