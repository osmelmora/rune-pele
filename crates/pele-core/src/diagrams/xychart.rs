//! XY charts, ported from Pele's `src/diagrams/xychart`. The lexer and parser fill Pele's
//! `XyChartDb`, `ticks` finds the steps of a value axis, and `render` draws the chart.

mod db;
pub(crate) mod json;
mod parser;
mod render;
/// Not part of the API: public for the workspace's tests, through the crate root.
#[doc(hidden)]
pub mod ticks;

use crate::RenderError;
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::scene::Scene;

pub(crate) use db::XyChartDb;

/// Pele's `xychart.parse`: a db that starts with the title front matter sets, filled by the parser.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<XyChartDb, RenderError> {
    let mut db = XyChartDb::new(cx.title);
    parser::parse_xychart(cx.text, &mut db).map_err(|error| cx.syntax(error))?;
    Ok(db)
}

pub(crate) struct Xychart;

impl Diagram for Xychart {
    type Model = XyChartDb;

    fn parse(cx: &ParseContext<'_>) -> Result<XyChartDb, RenderError> {
        parse(cx)
    }

    /// Pele's `renderXyChart`, drawn once at the width it takes.
    fn scene(model: &XyChartDb, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::scene(model, cx)
    }
}
