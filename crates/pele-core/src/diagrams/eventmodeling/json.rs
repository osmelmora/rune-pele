//! The entry point that lets `tests/eventmodeling_front.rs` hold the front end to Pele before
//! anything is drawn: the model as the oracle's `eventmodeling_front` op prints Pele's.

use super::model::{EventModel, Lane};
use crate::catalog::DiagramKind;
use crate::diagrams::railroad::probe::{maybe, string};
use crate::input::JsText;
use crate::{Origin, ParseOptions, RenderError, tagged};

fn strings(out: &mut String, list: &[JsText]) {
    out.push('[');
    for (index, item) in list.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        string(out, item);
    }
    out.push(']');
}

fn lane(out: &mut String, lane: &Lane) {
    out.push_str(&format!("{{\"index\":{},\"label\":", lane.index));
    string(out, &lane.label);
    out.push_str(&format!(",\"group\":\"{}\",\"namespace\":", lane.group.name()));
    maybe(out, lane.namespace.as_deref());
    out.push('}');
}

fn model_json(model: &EventModel) -> String {
    let mut out = String::from("{\"type\":\"eventmodeling\",\"title\":");
    maybe(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    maybe(&mut out, model.acc_title.as_deref());
    out.push_str(",\"accDescr\":");
    maybe(&mut out, model.acc_descr.as_deref());
    out.push_str(",\"boxes\":[");
    for (index, item) in model.boxes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        string(&mut out, &item.id);
        out.push_str(&format!(",\"kind\":\"{}\",\"entity\":", item.kind.name()));
        string(&mut out, &item.entity);
        out.push_str(",\"name\":");
        string(&mut out, &item.name);
        out.push_str(",\"namespace\":");
        maybe(&mut out, item.namespace.as_deref());
        out.push_str(&format!(",\"reset\":{},\"lane\":{},\"data\":", item.reset, item.lane));
        maybe(&mut out, item.data.as_deref());
        out.push_str(",\"dataType\":");
        maybe(&mut out, item.data_type.as_deref());
        out.push('}');
    }
    out.push_str("],\"lanes\":[");
    for (index, item) in model.lanes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        lane(&mut out, item);
    }
    out.push_str("],\"relations\":[");
    for (index, relation) in model.relations.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("[{},{}]", relation.source, relation.target));
    }
    out.push_str("],\"notes\":[");
    for (index, note) in model.notes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"frame\":");
        string(&mut out, &note.frame);
        out.push_str(",\"text\":");
        string(&mut out, &note.text);
        out.push_str(",\"dataType\":");
        maybe(&mut out, note.data_type.as_deref());
        out.push('}');
    }
    out.push_str("],\"specifications\":[");
    for (index, spec) in model.specifications.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"frame\":");
        string(&mut out, &spec.frame);
        for (name, list) in [("given", &spec.given), ("when", &spec.when), ("then", &spec.then)] {
            out.push_str(&format!(",\"{name}\":"));
            strings(&mut out, list);
        }
        out.push('}');
    }
    out.push_str("],\"entities\":");
    strings(&mut out, &model.entities);
    out.push('}');
    out
}

/// The event modeling front end's model as JSON, in the words of the oracle's
/// `eventmodeling_front` op, for the differential test. A source of another type answers only
/// that type.
pub fn front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = crate::front(source, &options)?;
    if front.found.kind != DiagramKind::Eventmodeling {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = super::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(model_json(&model))
}
