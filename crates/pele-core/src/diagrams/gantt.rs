//! Gantt charts, ported from Pele's `src/diagrams/gantt`. The lexer and the parser fill Pele's
//! `GanttDb`, scheduling resolves each task's dates with JavaScript's `Date` as V8 runs it in
//! UTC, and `render` draws the schedule on a time axis as a Scene.

mod axis;
mod date;
mod dates;
mod db;
pub(crate) mod json;
mod lexer;
mod parser;
/// Not part of the API: public for the workspace's tests, through the crate root.
#[doc(hidden)]
pub mod probe;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsText, JsValue};
use indexmap::IndexMap;
use std::collections::HashMap;

pub(crate) use db::Scheduler;
pub(crate) use render::Gantt;

/// What a task's leading tags mark it as. A task may carry several.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tags {
    pub(crate) active: bool,
    pub(crate) done: bool,
    pub(crate) crit: bool,
    pub(crate) milestone: bool,
    /// A vertical marker, which takes no row.
    pub(crate) vert: bool,
}

/// The fields of a task as written: Pele's `raw`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Raw {
    pub(crate) data: JsText,
    pub(crate) start: Option<JsText>,
    pub(crate) end: JsText,
}

/// Pele's `GanttTask` as parsing leaves it. Scheduling derives its dates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Task {
    pub(crate) id: JsText,
    /// Whether the author gave the id. Otherwise Pele numbers the task.
    pub(crate) own_id: bool,
    /// The task's label, under the name Mermaid gives it.
    pub(crate) task: JsText,
    pub(crate) section: JsText,
    pub(crate) tags: Tags,
    /// The row index in source order, or `None` for a vertical marker.
    pub(crate) order: Option<usize>,
    pub(crate) classes: Vec<JsText>,
    pub(crate) prev_task_id: Option<JsText>,
    pub(crate) raw: Raw,
}

/// The day Mermaid's weekends start on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Weekend {
    Friday,
    Saturday,
}

impl Weekend {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Weekend::Friday => "friday",
            Weekend::Saturday => "saturday",
        }
    }
}

/// Pele's `GanttModel`, as its `GanttDb` leaves it after parsing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GanttModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) date_format: JsText,
    pub(crate) axis_format: JsText,
    pub(crate) tick_interval: Option<JsText>,
    pub(crate) today_marker: JsText,
    pub(crate) includes: Vec<JsText>,
    pub(crate) excludes: Vec<JsText>,
    pub(crate) inclusive_end_dates: bool,
    pub(crate) top_axis: bool,
    pub(crate) display_mode: JsText,
    /// The day week-based ticks start on, when the chart names one.
    pub(crate) weekday: Option<&'static str>,
    pub(crate) weekend: Weekend,
    pub(crate) sections: Vec<JsText>,
    pub(crate) links: IndexMap<JsText, JsText>,
    pub(crate) tasks: Vec<Task>,
    /// The last task with each id.
    pub(crate) by_id: HashMap<JsText, usize>,
}

/// Pele's gantt `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<GanttModel, RenderError> {
    let mut db = db::GanttDb::new();
    if let Some(title) = cx.title.filter(|title| !title.is_empty()) {
        db.model.title = Some(title.clone());
    }
    let section = cx.config.get("gantt").and_then(JsValue::as_record);
    if let Some(JsValue::String(mode)) = section.and_then(|section| section.get("displayMode")) {
        db.model.display_mode = mode.clone();
    }
    parser::parse(cx, &mut db)?;
    Ok(db.model)
}
