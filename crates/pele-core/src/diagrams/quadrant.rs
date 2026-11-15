//! Quadrant charts, ported from Pele's `src/diagrams/quadrant`. The lexer turns a source into
//! tokens, the parser fills the db, and `render` lays out the four quadrants, the axis labels,
//! the points and the label of each point where it is free.

pub(crate) mod json;
mod lexer;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::JsText;
use crate::scene::Scene;
use indexmap::IndexMap;

/// Text a chart shows, which may be markdown.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuadrantText {
    pub(crate) text: JsText,
    pub(crate) markdown: bool,
}

/// Pele's `PointStyle`: what a point or a class sets, as the source writes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PointStyle {
    pub(crate) radius: Option<f64>,
    pub(crate) color: Option<JsText>,
    pub(crate) stroke_color: Option<JsText>,
    /// A number of pixels, such as `2px`.
    pub(crate) stroke_width: Option<JsText>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuadrantPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) text: QuadrantText,
    pub(crate) class_name: JsText,
    pub(crate) style: PointStyle,
}

/// Pele's `QuadrantModel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct QuadrantModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    /// Quadrants 1 to 4: top right, top left, bottom left, bottom right.
    pub(crate) quadrants: [Option<QuadrantText>; 4],
    pub(crate) x_axis_left: Option<QuadrantText>,
    pub(crate) x_axis_right: Option<QuadrantText>,
    pub(crate) y_axis_bottom: Option<QuadrantText>,
    pub(crate) y_axis_top: Option<QuadrantText>,
    pub(crate) points: Vec<QuadrantPoint>,
    pub(crate) classes: IndexMap<JsText, PointStyle>,
}

/// Pele's quadrant `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<QuadrantModel, RenderError> {
    let mut model = QuadrantModel { title: cx.title.cloned(), ..QuadrantModel::default() };
    parser::parse(cx.text, &mut model).map_err(|failure| match failure {
        parser::Failure::Syntax(error) => cx.syntax(error),
        parser::Failure::Semantic(message) => cx.semantic(message),
    })?;
    Ok(model)
}

pub(crate) struct Quadrant;

impl Diagram for Quadrant {
    type Model = QuadrantModel;

    fn parse(cx: &ParseContext<'_>) -> Result<QuadrantModel, RenderError> {
        parse(cx)
    }

    fn scene(model: &QuadrantModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::draw(model, cx)
    }
}
