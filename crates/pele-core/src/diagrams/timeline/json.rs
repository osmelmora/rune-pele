//! The model as the oracle's `timeline_front` op prints Pele's: `JSON.stringify` of each record
//! with its fields in the order Pele builds it, without the ones that are undefined.

use super::{Direction, TimelineModel};
use crate::diagrams::railroad::probe::string;
use crate::input::JsText;

fn strings(out: &mut String, list: &[JsText]) {
    out.push('[');
    for (index, text) in list.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        string(out, text);
    }
    out.push(']');
}

pub(crate) fn front_json(model: &TimelineModel) -> String {
    let mut out = String::from("{\"model\":{");
    for (name, text) in [("title", &model.title), ("accTitle", &model.acc_title), ("accDescr", &model.acc_descr)] {
        if let Some(text) = text {
            out.push_str(&format!("\"{name}\":"));
            string(&mut out, text);
            out.push(',');
        }
    }
    let direction = match model.direction {
        Direction::Across => "LR",
        Direction::Down => "TD",
    };
    out.push_str(&format!("\"direction\":\"{direction}\",\"sections\":"));
    strings(&mut out, &model.sections);
    out.push_str(",\"periods\":[");
    for (index, period) in model.periods.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"text\":");
        string(&mut out, &period.text);
        out.push_str(",\"events\":");
        strings(&mut out, &period.events);
        out.push_str(",\"section\":");
        string(&mut out, &period.section);
        let section_index = period.section_index.map_or(-1, |index| index as i64);
        out.push_str(&format!(",\"sectionIndex\":{section_index}}}"));
    }
    out.push_str("]}}");
    out
}
