//! Drawing a state diagram, ported from Pele's `src/diagrams/state/render.ts`.

use super::StateModel;
use crate::RenderError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::scene::Scene;

pub(crate) struct State;

impl Diagram for State {
    type Model = StateModel;

    fn parse(cx: &ParseContext<'_>) -> Result<StateModel, RenderError> {
        super::parse(cx)
    }

    fn scene(_model: &StateModel, _cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        Err(RenderError::NotPorted)
    }
}
