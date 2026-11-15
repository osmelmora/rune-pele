//! Pele's `XyChartDb` (`db.ts`): what the parser fills, which the chart is drawn from.

use crate::input::{JsText, Units, is_space};
use crate::js;

pub(crate) struct DataPoint {
    pub(crate) value: f64,
    pub(crate) label: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Orientation {
    Vertical,
    Horizontal,
}

/// The x axis: categories, or a range of numbers.
#[derive(Clone, Debug)]
pub(crate) enum XAxis {
    Band { title: JsText, categories: Vec<JsText> },
    Linear { title: JsText, min: f64, max: f64 },
}

impl XAxis {
    pub(crate) fn title(&self) -> &JsText {
        match self {
            XAxis::Band { title, .. } | XAxis::Linear { title, .. } => title,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct YAxis {
    pub(crate) title: JsText,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

/// A series: each value with its place on the x axis, a category or a number written as text. A
/// category with no value has none.
#[derive(Clone, Debug)]
pub(crate) struct Plot {
    pub(crate) line: bool,
    pub(crate) title: JsText,
    pub(crate) data: Vec<(JsText, Option<f64>)>,
    pub(crate) point_labels: Option<Vec<JsText>>,
}

pub(crate) struct XyChartDb {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) orientation: Option<Orientation>,
    pub(crate) x_axis: XAxis,
    pub(crate) y_axis: YAxis,
    pub(crate) has_set_x_axis: bool,
    /// False while the y range comes from the data and not from a `y-axis` statement.
    pub(crate) has_set_y_axis: bool,
    pub(crate) plots: Vec<Plot>,
}

fn trimmed(text: &[u16]) -> JsText {
    text.trim_js().into()
}

impl XyChartDb {
    pub(crate) fn new(title: Option<&JsText>) -> Self {
        XyChartDb {
            title: title.cloned(),
            acc_title: None,
            acc_descr: None,
            orientation: None,
            x_axis: XAxis::Band { title: JsText::default(), categories: Vec::new() },
            y_axis: YAxis { title: JsText::default(), min: f64::INFINITY, max: f64::NEG_INFINITY },
            has_set_x_axis: false,
            has_set_y_axis: false,
            plots: Vec::new(),
        }
    }

    pub(crate) fn set_orientation(&mut self, text: &[u16]) {
        let horizontal = text.is_str("horizontal");
        self.orientation = Some(if horizontal { Orientation::Horizontal } else { Orientation::Vertical });
    }

    pub(crate) fn set_x_axis_title(&mut self, title: &[u16]) {
        match &mut self.x_axis {
            XAxis::Band { title: own, .. } | XAxis::Linear { title: own, .. } => *own = trimmed(title),
        }
    }

    pub(crate) fn set_x_axis_range(&mut self, min: f64, max: f64) {
        self.x_axis = XAxis::Linear { title: self.x_axis.title().clone(), min, max };
        self.has_set_x_axis = true;
    }

    pub(crate) fn set_x_axis_band(&mut self, categories: &[Vec<u16>]) {
        let categories = categories.iter().map(|category| trimmed(category)).collect();
        self.x_axis = XAxis::Band { title: self.x_axis.title().clone(), categories };
        self.has_set_x_axis = true;
    }

    pub(crate) fn set_y_axis_title(&mut self, title: &[u16]) {
        self.y_axis.title = trimmed(title);
    }

    pub(crate) fn set_y_axis_range(&mut self, min: f64, max: f64) {
        self.y_axis = YAxis { title: self.y_axis.title.clone(), min, max };
        self.has_set_y_axis = true;
    }

    fn plot_data(&mut self, values: &[f64]) -> Vec<(JsText, Option<f64>)> {
        if values.is_empty() {
            return Vec::new();
        }
        // The first series fixes the x range when no x-axis statement did.
        if !self.has_set_x_axis {
            self.set_x_axis_range(1.0, values.len() as f64);
        }
        let values = match &self.x_axis {
            XAxis::Band { categories, .. } if values.len() > categories.len() => &values[..categories.len()],
            _ => values,
        };
        if !self.has_set_y_axis {
            let (mut min, mut max) = (self.y_axis.min, self.y_axis.max);
            for &value in values {
                if value < min {
                    min = value;
                }
                if value > max {
                    max = value;
                }
            }
            self.y_axis = YAxis { title: self.y_axis.title.clone(), min, max };
        }
        match &self.x_axis {
            XAxis::Band { categories, .. } => {
                categories.iter().enumerate().map(|(i, category)| (category.clone(), values.get(i).copied())).collect()
            }
            XAxis::Linear { min, max, .. } => {
                if values.len() == 1 {
                    return vec![(js::to_string(*min).as_str().into(), Some(values[0]))];
                }
                let step = (max - min) / (values.len() as f64 - 1.0);
                values
                    .iter()
                    .enumerate()
                    .map(|(i, &value)| (js::to_string(min + i as f64 * step).as_str().into(), Some(value)))
                    .collect()
            }
        }
    }

    pub(crate) fn add_plot(&mut self, line: bool, title: &[u16], data: &[DataPoint]) {
        let values: Vec<f64> = data.iter().map(|point| point.value).collect();
        let data_points = self.plot_data(&values);
        // Mermaid accepts labels on the points of a bar series and ignores them.
        let labelled = line && data.iter().any(|point| !point.label.trim_js().is_empty());
        let point_labels = labelled.then(|| data.iter().map(|point| trimmed(&point.label)).collect());
        self.plots.push(Plot { line, title: trimmed(title), data: data_points, point_labels });
    }

    pub(crate) fn set_acc_title(&mut self, text: &[u16]) {
        self.acc_title = Some(text.trim_start_js().into());
    }

    /// `text.replace(/\n\s+/g, '\n')`
    pub(crate) fn set_acc_description(&mut self, text: &[u16]) {
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            out.push(text[at]);
            let spaces = text.run(at + 1, is_space);
            at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
        }
        self.acc_descr = Some(out.into());
    }
}

impl XyChartDb {
    pub(crate) fn accessible(&self) -> crate::scene::Accessible {
        crate::scene::Accessible { title: self.acc_title.clone(), description: self.acc_descr.clone() }
    }
}
