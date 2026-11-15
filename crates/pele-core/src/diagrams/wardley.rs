//! Wardley maps, ported from Pele's `src/diagrams/wardley`. The parser reads Mermaid's Langium
//! grammar into a syntax tree, `model` fills Pele's builder from it, and `render` draws the map.

pub(crate) mod json;
mod model;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::scene::Scene;

pub(crate) use model::WardleyModel;

/// Pele's `wardley.parse`: the syntax tree, then the model, with Pele's syntax and semantic errors.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<WardleyModel, RenderError> {
    let ast = parser::parse_wardley(cx.text).map_err(|error| cx.syntax(error))?;
    model::build_wardley(ast, cx.title).map_err(|message| cx.semantic(message))
}

pub(crate) struct Wardley;

impl Diagram for Wardley {
    type Model = WardleyModel;

    fn parse(cx: &ParseContext<'_>) -> Result<WardleyModel, RenderError> {
        parse(cx)
    }

    /// Pele's `renderWardley`: at the map's own width, then narrower until it fits the host.
    fn scene(model: &WardleyModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::scene(model, cx)
    }
}
