//! Timeline diagrams, ported from Pele's `src/diagrams/timeline`. The lexer and parser follow the
//! parser Mermaid generates from `timeline.jison`, the model is Pele's `TimelineDb`, and `render`
//! draws the periods along an axis, across or down, with their events beside them.

pub(crate) mod json;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsText, Units, is_space};
use parser::Failure;

pub(crate) use render::Timeline;

/// Which way the periods run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    /// `LR`, which turns to run down where the host is too narrow.
    Across,
    /// `TD`.
    Down,
}

/// Pele's `TimelinePeriod`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Period {
    pub(crate) text: JsText,
    pub(crate) events: Vec<JsText>,
    /// Name of the section in force when the period was declared, empty before the first one.
    pub(crate) section: JsText,
    /// Place in `sections` of that section, none before the first one.
    pub(crate) section_index: Option<usize>,
}

/// Pele's `TimelineModel`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimelineModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) direction: Direction,
    pub(crate) sections: Vec<JsText>,
    pub(crate) periods: Vec<Period>,
}

/// `text.replace(/\n\s+/g, '\n')`
fn unindent(text: &[u16]) -> JsText {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        let spaces = text.run(at + 1, is_space);
        at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
    }
    out.into()
}

/// Pele's `TimelineDb`.
impl TimelineModel {
    fn set_acc_title(&mut self, text: &[u16]) {
        self.acc_title = Some(text.trim_start_js().into());
    }

    fn set_acc_description(&mut self, text: &[u16]) {
        self.acc_descr = Some(unindent(text));
    }

    fn add_section(&mut self, text: JsText) {
        self.sections.push(text);
    }

    fn add_task(&mut self, text: JsText) {
        let section = self.sections.last().cloned().unwrap_or_default();
        let section_index = self.sections.len().checked_sub(1);
        self.periods.push(Period { text, events: Vec::new(), section, section_index });
    }

    /// Mermaid fails here too, with a TypeError.
    fn add_event(&mut self, event: JsText) -> Result<(), &'static str> {
        let period = self.periods.last_mut().ok_or("An event needs a time period before it.")?;
        period.events.push(event);
        Ok(())
    }
}

/// Pele's timeline `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<TimelineModel, RenderError> {
    let mut model = TimelineModel {
        title: cx.title.cloned(),
        acc_title: None,
        acc_descr: None,
        direction: Direction::Across,
        sections: Vec::new(),
        periods: Vec::new(),
    };
    parser::parse(cx.text, &mut model).map_err(|failure| match failure {
        Failure::Syntax(error) => cx.syntax(error),
        Failure::Semantic(message) => cx.semantic(message.to_owned()),
    })?;
    Ok(model)
}
