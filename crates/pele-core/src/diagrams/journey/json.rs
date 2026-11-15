//! The model as the oracle's `journey_front` op prints Pele's: `JSON.stringify` of each record
//! with its fields in the order Pele builds it, without the ones that are undefined, and each
//! score as the bits of its double.

use super::JourneyModel;
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

pub(crate) fn front_json(model: &JourneyModel) -> String {
    let mut out = String::from("{\"model\":{");
    for (name, text) in [("title", &model.title), ("accTitle", &model.acc_title), ("accDescr", &model.acc_descr)] {
        if let Some(text) = text {
            out.push_str(&format!("\"{name}\":"));
            string(&mut out, text);
            out.push(',');
        }
    }
    out.push_str("\"sections\":");
    strings(&mut out, &model.sections);
    out.push_str(",\"tasks\":[");
    for (index, task) in model.tasks.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"task\":");
        string(&mut out, &task.task);
        out.push_str(&format!(",\"score\":\"{:016x}\",\"people\":", task.score.to_bits()));
        strings(&mut out, &task.people);
        out.push_str(",\"section\":");
        string(&mut out, &task.section);
        out.push('}');
    }
    out.push_str("]}}");
    out
}
