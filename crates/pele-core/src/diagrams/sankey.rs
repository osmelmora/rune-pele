//! Sankey diagrams, ported from Pele's `src/diagrams/sankey`. The lexer and the parser read the
//! CSV rows of Mermaid's grammar, `layout` places the nodes in columns and settles them, and
//! `render` draws the bands, the bars and their labels as a Scene.

pub(crate) mod json;
pub mod layout;
mod lexer;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;

pub(crate) use render::Sankey;

/// One row: a flow of `value` from one node to another, by their places in `nodes`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Link {
    pub(crate) source: usize,
    pub(crate) target: usize,
    pub(crate) value: f64,
}

/// Pele's `SankeyDb` once parsed: the nodes by name in the order they first appear, and the rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SankeyModel {
    pub(crate) nodes: Vec<JsText>,
    pub(crate) links: Vec<Link>,
    pub(crate) title: Option<JsText>,
}

pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<SankeyModel, RenderError> {
    let title = cx.title.filter(|title| !title.is_empty()).cloned();
    let src = parser::prepare_text_for_parsing(cx.text, None);
    parser::parse_sankey(&src, cx.text, title).map_err(|error| cx.syntax(error))
}
