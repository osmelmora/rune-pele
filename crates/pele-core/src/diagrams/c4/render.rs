//! Drawing a C4 diagram, ported from Pele's `src/diagrams/c4/render.ts`.

use super::C4Model;
use crate::RenderError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::scene::Scene;

pub(crate) struct C4;

impl Diagram for C4 {
    type Model = C4Model;

    fn parse(cx: &ParseContext<'_>) -> Result<C4Model, RenderError> {
        super::parse(cx)
    }

    fn scene(_model: &C4Model, _cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        Err(RenderError::NotPorted)
    }
}
