//! Radar charts: curves of values on axes around a centre. Ported from Pele's
//! `src/diagrams/radar`.

mod model;
mod parser;
mod render;

use crate::catalog::{Diagram, DiagramKind, DrawContext, ParseContext};
use crate::diagrams::railroad::probe::{maybe, string};
use crate::scene::Scene;
use crate::{Origin, ParseOptions, RenderError, js, tagged};
use model::Graticule;
pub(crate) use model::RadarModel;

fn parse(cx: &ParseContext<'_>) -> Result<RadarModel, RenderError> {
    let ast = parser::parse_radar(cx.text).map_err(|error| cx.syntax(error))?;
    model::build_radar(ast, cx.title).map_err(|message| cx.semantic(message))
}

pub(crate) struct Radar;

impl Diagram for Radar {
    type Model = RadarModel;

    fn parse(cx: &ParseContext<'_>) -> Result<RadarModel, RenderError> {
        parse(cx)
    }

    fn scene(model: &RadarModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::scene(model, cx)
    }
}

/// A number as `String(number)`, since JSON has no Infinity.
fn number(out: &mut String, value: f64) {
    out.push('"');
    out.push_str(&js::to_string(value));
    out.push('"');
}

/// The radar front end's model as JSON, in the words of the oracle's `radar_front` op, for the
/// differential test. A source of another type answers only that type.
pub fn front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = crate::front(source, &options)?;
    if front.found.kind != DiagramKind::Radar {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    let mut out = String::from("{\"type\":\"radar\",\"title\":");
    maybe(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    maybe(&mut out, model.acc_title.as_deref());
    out.push_str(",\"accDescr\":");
    maybe(&mut out, model.acc_descr.as_deref());
    out.push_str(",\"axes\":[");
    for (index, axis) in model.axes.iter().enumerate() {
        out.push_str(if index > 0 { ",{\"name\":" } else { "{\"name\":" });
        string(&mut out, &axis.name);
        out.push_str(",\"label\":");
        string(&mut out, &axis.label);
        out.push('}');
    }
    out.push_str("],\"curves\":[");
    for (index, curve) in model.curves.iter().enumerate() {
        out.push_str(if index > 0 { ",{\"name\":" } else { "{\"name\":" });
        string(&mut out, &curve.name);
        out.push_str(",\"label\":");
        string(&mut out, &curve.label);
        out.push_str(",\"entries\":[");
        for (index, &value) in curve.entries.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            number(&mut out, value);
        }
        out.push_str("]}");
    }
    let options = &model.options;
    out.push_str(&format!("],\"options\":{{\"showLegend\":{},\"ticks\":", options.show_legend));
    number(&mut out, options.ticks);
    out.push_str(",\"max\":");
    match options.max {
        Some(max) => number(&mut out, max),
        None => out.push_str("null"),
    }
    out.push_str(",\"min\":");
    number(&mut out, options.min);
    let graticule = if options.graticule == Graticule::Polygon { "polygon" } else { "circle" };
    out.push_str(&format!(",\"graticule\":\"{graticule}\"}}}}"));
    Ok(out)
}
