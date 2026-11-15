//! The radar model Pele builds from the parsed statements, ported from `radar/model.ts`.

use super::parser::{
    OPTION_GRATICULE, OPTION_MAX, OPTION_MIN, OPTION_SHOW_LEGEND, OPTION_TICKS, OptionValue, RadarAst,
};
use crate::diagrams::railroad::close_indents;
use crate::input::{JsText, Units};
use std::collections::HashMap;

pub(crate) struct Axis {
    pub(crate) name: JsText,
    pub(crate) label: JsText,
}

pub(crate) struct Curve {
    pub(crate) name: JsText,
    pub(crate) label: JsText,
    /// One value per axis, or as many as were written for a curve of bare values.
    pub(crate) entries: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Graticule {
    Circle,
    Polygon,
}

pub(crate) struct Options {
    pub(crate) show_legend: bool,
    pub(crate) ticks: f64,
    /// `None` takes the largest value of any curve.
    pub(crate) max: Option<f64>,
    pub(crate) min: f64,
    pub(crate) graticule: Graticule,
}

/// Pele's `RadarModel`.
pub(crate) struct RadarModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) axes: Vec<Axis>,
    pub(crate) curves: Vec<Curve>,
    pub(crate) options: Options,
}

const MAX_TICKS: f64 = 32.0;

/// Pele's `buildRadar`. `title` is what front matter sets. An error is the message of Pele's
/// semantic error.
pub(crate) fn build_radar(ast: RadarAst, title: Option<&JsText>) -> Result<RadarModel, String> {
    let axes: Vec<Axis> = ast
        .axes
        .iter()
        .map(|axis| Axis {
            name: axis.name.as_slice().into(),
            label: axis.label.as_ref().unwrap_or(&axis.name).as_slice().into(),
        })
        .collect();

    let mut curves = Vec::with_capacity(ast.curves.len());
    for curve in &ast.curves {
        let entries = if curve.entries[0].axis.is_none() {
            curve.entries.iter().map(|entry| entry.value).collect()
        } else {
            if axes.is_empty() {
                return Err("Axes must be populated before curves for reference entries".to_owned());
            }
            let mut by_axis: HashMap<&[u16], f64> = HashMap::new();
            for entry in &curve.entries {
                if let Some(axis) = &entry.axis {
                    by_axis.entry(axis).or_insert(entry.value);
                }
            }
            let mut entries = Vec::with_capacity(axes.len());
            for axis in &axes {
                match by_axis.get(&axis.name[..]) {
                    Some(&value) => entries.push(value),
                    None => return Err(format!("Missing entry for axis {}", String::from_utf16_lossy(&axis.label))),
                }
            }
            entries
        };
        let label = curve.label.as_ref().unwrap_or(&curve.name).as_slice().into();
        curves.push(Curve { name: curve.name.as_slice().into(), label, entries });
    }

    let mut options = Options { show_legend: true, ticks: 5.0, max: None, min: 0.0, graticule: Graticule::Circle };
    for option in &ast.options {
        match (&option.value, option.name) {
            (OptionValue::Flag(flag), OPTION_SHOW_LEGEND) => options.show_legend = *flag,
            (OptionValue::Graticule(name), OPTION_GRATICULE) => {
                options.graticule = if name.is_str("polygon") { Graticule::Polygon } else { Graticule::Circle };
            }
            (OptionValue::Number(value), OPTION_TICKS) => options.ticks = *value,
            (OptionValue::Number(value), OPTION_MAX) => options.max = Some(*value),
            (OptionValue::Number(value), OPTION_MIN) => options.min = *value,
            _ => unreachable!("the parser pairs each option with its kind of value"),
        }
    }
    if options.ticks > MAX_TICKS {
        options.ticks = MAX_TICKS;
    }

    let filled = |text: &Option<Vec<u16>>| text.as_ref().filter(|text| !text.is_empty()).cloned();
    Ok(RadarModel {
        title: filled(&ast.title).map(JsText::from).or_else(|| title.cloned()),
        acc_title: filled(&ast.acc_title).map(|text| text.trim_start_js().into()),
        acc_descr: filled(&ast.acc_descr).map(|text| close_indents(&text)),
        axes,
        curves,
        options,
    })
}
