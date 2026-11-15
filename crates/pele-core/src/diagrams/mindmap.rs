//! Mindmaps, ported from Pele's `src/diagrams/mindmap`. The shared outline grammar reads the
//! source, the db here nests its nodes by indentation, and `render` spreads the branches to both
//! sides of the root, or runs them down as an outline on a narrow host.

pub(crate) mod json;
mod render;

use super::outline::{Decoration, NodeShape, OutlineDb, OutlineError, OutlineNode, parse_outline};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::JsText;
use crate::scene::Scene;
use crate::{Diagnostic, ErrorCode, RenderError};

/// A first-level branch's index wraps after eleven, as in Mermaid.
const SECTIONS: usize = 11;

/// Pele's `MindmapNode`. Its `id` is its place in [`MindmapModel::nodes`], and the first node is
/// the root.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MindmapNode {
    pub(crate) node_id: JsText,
    /// How far the node is set in past the root, in UTF-16 code units. Below the root it is less
    /// than zero.
    pub(crate) level: i64,
    pub(crate) descr: JsText,
    pub(crate) shape: NodeShape,
    /// The places of its children, in source order.
    pub(crate) children: Vec<usize>,
    /// The index of the first-level branch it belongs to. The root has none.
    pub(crate) section: Option<usize>,
    pub(crate) class: Option<JsText>,
    pub(crate) icon: Option<JsText>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MindmapModel {
    pub(crate) title: Option<JsText>,
    /// In source order, which is depth first.
    pub(crate) nodes: Vec<MindmapNode>,
}

/// Pele's `MindmapDb`.
struct MindmapDb {
    model: MindmapModel,
    base_level: i64,
    /// The chain of nodes that can still take children, with strictly increasing levels.
    open: Vec<usize>,
}

impl OutlineDb for MindmapDb {
    type Error = RenderError;

    fn add_node(&mut self, outline: OutlineNode<'_>) -> Result<(), RenderError> {
        let nodes = &mut self.model.nodes;
        let is_root = nodes.is_empty();
        let level = outline.level as i64;
        if is_root {
            self.base_level = level;
        }
        let level = level - self.base_level;
        // Mermaid's parent is the last node with a smaller indentation, wherever it is in the tree.
        let mut depth = self.open.len();
        while depth > 0 && nodes[self.open[depth - 1]].level >= level {
            depth -= 1;
        }
        let parent = depth.checked_sub(1).map(|at| self.open[at]);
        if parent.is_none() && !is_root {
            let descr = String::from_utf16_lossy(outline.descr);
            let message = format!("There can be only one root. No parent could be found for (\"{descr}\")");
            return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message)));
        }
        let id = nodes.len();
        let section = parent.map(|parent| match nodes[parent].section {
            Some(section) if parent > 0 => section,
            _ => nodes[parent].children.len() % SECTIONS,
        });
        if let Some(parent) = parent {
            nodes[parent].children.push(id);
        }
        nodes.push(MindmapNode {
            node_id: outline.id.into(),
            level,
            descr: outline.descr.into(),
            shape: outline.shape,
            children: Vec::new(),
            section,
            class: None,
            icon: None,
        });
        self.open.truncate(depth);
        self.open.push(id);
        Ok(())
    }

    fn decorate_node(&mut self, decoration: Decoration<'_>) {
        let Some(node) = self.model.nodes.last_mut() else {
            return;
        };
        match decoration {
            Decoration::Icon(icon) if !icon.is_empty() => node.icon = Some(icon.into()),
            Decoration::Class(class) if !class.is_empty() => node.class = Some(class.into()),
            _ => {}
        }
    }
}

/// Pele's mindmap `parse`: the map a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<MindmapModel, RenderError> {
    let mut db = MindmapDb {
        model: MindmapModel { title: cx.title.cloned(), nodes: Vec::new() },
        base_level: 0,
        open: Vec::new(),
    };
    parse_outline(cx.text, &mut db, false).map_err(|error| match error {
        OutlineError::Syntax(error) => cx.syntax(error),
        OutlineError::Db(error) => error,
    })?;
    Ok(db.model)
}

pub(crate) struct Mindmap;

impl Diagram for Mindmap {
    type Model = MindmapModel;

    fn parse(cx: &ParseContext<'_>) -> Result<MindmapModel, RenderError> {
        parse(cx)
    }

    /// Pele's `renderMindmap`: branches to both sides, or an outline when that is narrower on a
    /// host too narrow for them.
    fn scene(model: &MindmapModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::draw(model, cx)
    }
}
