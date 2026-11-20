//! Venn diagrams, ported from Pele's `src/diagrams/venn`. The lexer and the parser fill the db,
//! `layout` places one circle per set and finds where the text of each region goes, and `render`
//! scales the drawing until each region holds its text.

pub(crate) mod json;
mod layout;
mod lexer;
mod parser;
/// Not part of the API: public for the workspace's tests, through the crate root.
#[doc(hidden)]
pub mod probe;
mod render;

use crate::RenderError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::{JsText, Units};
use crate::scene::Scene;
use indexmap::IndexMap;

/// A set (one id) or an overlap of sets (two or more ids, sorted), with its relative area.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VennSubset {
    pub(crate) sets: Vec<JsText>,
    pub(crate) size: f64,
    pub(crate) label: Option<JsText>,
}

/// A line of text shown inside the region of the given sets.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VennText {
    pub(crate) sets: Vec<JsText>,
    pub(crate) id: JsText,
    pub(crate) label: Option<JsText>,
}

/// Targets are the sorted ids of a set or overlap, or the id of a text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VennStyle {
    pub(crate) targets: Vec<JsText>,
    pub(crate) styles: IndexMap<JsText, JsText>,
}

/// Pele's `VennModel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VennModel {
    pub(crate) title: Option<JsText>,
    pub(crate) subsets: Vec<VennSubset>,
    pub(crate) text_nodes: Vec<VennText>,
    pub(crate) styles: Vec<VennStyle>,
}

/// Pele's `normalizeText`: Mermaid takes one pair of quotes off an id, label or style value.
pub(crate) fn normalize_text(text: &[u16]) -> JsText {
    let trimmed = text.trim_js();
    let quoted = trimmed.len() >= 2 && trimmed[0] == 34 && trimmed[trimmed.len() - 1] == 34;
    if quoted { trimmed[1..trimmed.len() - 1].into() } else { trimmed.into() }
}

/// Pele's venn `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<VennModel, RenderError> {
    let mut db = parser::VennDb::default();
    if let Some(title) = cx.title.filter(|title| !title.is_empty()) {
        db.model.title = Some(title.clone());
    }
    parser::parse(cx.text, &mut db).map_err(|failure| match failure {
        parser::Failure::Syntax(error) => cx.syntax(error),
        parser::Failure::Semantic(message) => cx.semantic(message),
    })?;
    Ok(db.model)
}

pub(crate) struct Venn;

impl Diagram for Venn {
    type Model = VennModel;

    fn parse(cx: &ParseContext<'_>) -> Result<VennModel, RenderError> {
        parse(cx)
    }

    fn scene(model: &VennModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::draw(model, cx, &layout::Ported)
    }
}
