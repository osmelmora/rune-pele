//! Ishikawa (fishbone) diagrams, ported from Pele's `src/diagrams/ishikawa`. The parser reads one
//! line of text per node, the db nests them by indentation, and `render` hangs the categories off
//! the spine in pairs, or runs the causes down as an indented list on a narrow host.

pub(crate) mod json;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::JsText;
use crate::scene::Scene;

/// A node of the outline: the effect, a category of causes, or a cause.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IshikawaNode {
    pub(crate) text: JsText,
    /// The places of its children in [`IshikawaModel::nodes`], in source order.
    pub(crate) children: Vec<usize>,
}

/// Pele's `IshikawaModel`, with the tree as an arena in source order, which is preorder. The
/// first node, when there is one, is the effect; its children are the categories, theirs the
/// causes, and so on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IshikawaModel {
    pub(crate) title: Option<JsText>,
    pub(crate) nodes: Vec<IshikawaNode>,
}

/// Pele's `IshikawaDb`. The first line is the effect. Every later line is a cause, nested by its
/// indentation relative to the first cause, so the effect itself may be indented more than the
/// causes.
#[derive(Default)]
struct IshikawaDb {
    nodes: Vec<IshikawaNode>,
    /// The line of ancestors of the last node, each with its level.
    stack: Vec<(usize, usize)>,
    base_level: Option<usize>,
}

impl IshikawaDb {
    fn add_node(&mut self, raw_level: usize, text: &[u16]) {
        let node = self.nodes.len();
        self.nodes.push(IshikawaNode { text: text.into(), children: Vec::new() });
        if node == 0 {
            self.stack = vec![(0, node)];
            return;
        }
        let base = *self.base_level.get_or_insert(raw_level);
        let level = (raw_level as i64 - base as i64 + 1).max(1) as usize;
        while self.stack.len() > 1 && self.stack.last().is_some_and(|&(top, _)| top >= level) {
            self.stack.pop();
        }
        let &(_, parent) = self.stack.last().expect("the effect stays on the stack");
        self.nodes[parent].children.push(node);
        self.stack.push((level, node));
    }
}

/// Pele's Ishikawa `parse`: the outline a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<IshikawaModel, RenderError> {
    let mut db = IshikawaDb::default();
    parser::parse(cx.text, |level, text| db.add_node(level, text)).map_err(|error| cx.syntax(error))?;
    Ok(IshikawaModel { title: cx.title.cloned(), nodes: db.nodes })
}

pub(crate) struct Ishikawa;

impl Diagram for Ishikawa {
    type Model = IshikawaModel;

    fn parse(cx: &ParseContext<'_>) -> Result<IshikawaModel, RenderError> {
        parse(cx)
    }

    /// Pele's `renderIshikawa`: across, or down when that is narrower on a host too narrow for it.
    fn scene(model: &IshikawaModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::draw(model, cx)
    }
}
