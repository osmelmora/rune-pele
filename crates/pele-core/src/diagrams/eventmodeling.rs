//! Event modeling diagrams: frames on a timeline in lanes by what they hold, the flows between
//! them, and notes and specifications under the lanes. Ported from Pele's
//! `src/diagrams/eventmodeling`.

pub(crate) mod json;
mod model;
mod parser;
mod render;

use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::scene::Scene;
use crate::{Diagnostic, ErrorCode, Location, RenderError};
pub(crate) use model::EventModel;

/// Pele's `eventmodeling.parse`: the parsed frames built into a model, or the frame that is
/// named twice, with the line it is on.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<EventModel, RenderError> {
    let ast = parser::parse_event_model(cx.text).map_err(|error| cx.syntax(error))?;
    model::build_event_model(ast, cx.title).map_err(|duplicate| {
        let line = cx.origin.locate(cx.text, duplicate.offset).line;
        let message = format!(
            "Duplicate event modeling frame ID \"{}\" on line {line}",
            String::from_utf16_lossy(&duplicate.name)
        );
        let location = Location { line, column_utf16: 0, snippet: String::new() };
        RenderError::Diagnostic(Diagnostic {
            location: Some(location),
            ..Diagnostic::new(ErrorCode::Semantic, message)
        })
    })
}

pub(crate) struct Eventmodeling;

impl Diagram for Eventmodeling {
    type Model = EventModel;

    fn parse(cx: &ParseContext<'_>) -> Result<EventModel, RenderError> {
        parse(cx)
    }

    fn scene(model: &EventModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::scene(model, cx)
    }
}
