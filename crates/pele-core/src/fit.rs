//! Width adaptation shared by the diagram types, ported from Pele's
//! `src/diagrams/common/fit-width.ts`. A chart is drawn again from its model for each candidate;
//! a finished drawing is never scaled.

use crate::scene::Scene;
use crate::{LayoutOptions, RenderError, js};

/// The width the host has for the diagram, when it gives one.
fn available(options: &LayoutOptions) -> Option<f64> {
    options.max_width.filter(|&width| width > 0.0)
}

/// Pele's `fitWidth`: draws a chart whose plot can be any width so that the whole diagram fits
/// the width the host has. What surrounds the plot is only known once it is drawn, so the plot
/// gives up the overflow and is drawn again, up to three times. `narrow` tells the chart that it
/// did not fit at its own width. The last draw is the result, whether or not it fits.
pub(crate) fn fit_width(
    options: &LayoutOptions,
    natural: f64,
    least: f64,
    mut draw: impl FnMut(f64, bool) -> Result<Scene, RenderError>,
) -> Result<Scene, RenderError> {
    let mut result = draw(natural, false)?;
    let Some(available) = available(options) else {
        return Ok(result);
    };
    if result.size().width <= available {
        return Ok(result);
    }
    let floor = js::min(least, natural);
    let mut plot = natural;
    result = draw(plot, true)?;
    for _ in 0..3 {
        if !(result.size().width > available && plot > floor) {
            break;
        }
        plot = js::max(floor, plot - (result.size().width - available));
        result = draw(plot, true)?;
    }
    Ok(result)
}

/// Pele's `titleRoom`: the width a title may take before it wraps.
pub(crate) fn title_room(options: &LayoutOptions) -> f64 {
    available(options).map_or(4000.0, |available| js::max(80.0, available - 2.0 * options.padding))
}
