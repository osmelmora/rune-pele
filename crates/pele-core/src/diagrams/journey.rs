//! User journey diagrams, ported from Pele's `src/diagrams/journey`. The lexer and parser follow
//! the parser Mermaid generates from `journey.jison`, the model is Pele's `JourneyDb`, and
//! `render` draws the tasks as cards over a chart of their scores, across or down.

pub(crate) mod json;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsText, Units, is_space, string_to_number};

pub(crate) use render::Journey;

/// Pele's `JourneyTask`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JourneyTask {
    pub(crate) task: JsText,
    /// As written: any number, or NaN when the score is not one.
    pub(crate) score: f64,
    pub(crate) people: Vec<JsText>,
    /// Name of the section in force when the task was declared, empty before the first one.
    pub(crate) section: JsText,
}

/// Pele's `JourneyModel`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JourneyModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) sections: Vec<JsText>,
    pub(crate) tasks: Vec<JourneyTask>,
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

/// Pele's `JourneyDb`.
impl JourneyModel {
    fn set_acc_title(&mut self, text: &[u16]) {
        self.acc_title = Some(text.trim_start_js().into());
    }

    fn set_acc_description(&mut self, text: &[u16]) {
        self.acc_descr = Some(unindent(text));
    }

    fn add_section(&mut self, text: JsText) {
        self.sections.push(text);
    }

    /// `data` is the rest of the line from its first colon: `:score` or `:score:actor, actor`.
    fn add_task(&mut self, task: JsText, data: &[u16]) {
        let pieces: Vec<&[u16]> = data[1..].split(|&c| c == 58).collect();
        let people = match pieces.get(1) {
            Some(names) => names.split(|&c| c == 44).map(|name| name.trim_js().into()).collect(),
            None => Vec::new(),
        };
        let section = self.sections.last().cloned().unwrap_or_default();
        self.tasks.push(JourneyTask { task, score: string_to_number(pieces[0]), people, section });
    }

    /// Pele's `journeyActors`: everyone named on a task, once each, in code unit order.
    pub(crate) fn actors(&self) -> Vec<&JsText> {
        let mut names: Vec<&JsText> = self.tasks.iter().flat_map(|task| &task.people).collect();
        names.sort_by(|a, b| a[..].cmp(&b[..]));
        names.dedup();
        names
    }
}

/// Pele's journey `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<JourneyModel, RenderError> {
    let mut model = JourneyModel {
        title: cx.title.cloned(),
        acc_title: None,
        acc_descr: None,
        sections: Vec::new(),
        tasks: Vec::new(),
    };
    parser::parse(cx.text, &mut model).map_err(|error| cx.syntax(error))?;
    Ok(model)
}
