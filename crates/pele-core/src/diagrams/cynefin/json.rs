//! The model as the oracle's `cynefin_front` op prints Pele's: `JSON.stringify` of each record
//! with its fields in the order Pele builds it, without the ones that are undefined.

use super::CynefinModel;

/// `JSON.stringify` of a string.
fn string(out: &mut String, text: &[u16]) {
    out.push('"');
    for item in char::decode_utf16(text.iter().copied()) {
        match item {
            Ok('"') => out.push_str("\\\""),
            Ok('\\') => out.push_str("\\\\"),
            Ok('\u{8}') => out.push_str("\\b"),
            Ok('\u{c}') => out.push_str("\\f"),
            Ok('\n') => out.push_str("\\n"),
            Ok('\r') => out.push_str("\\r"),
            Ok('\t') => out.push_str("\\t"),
            Ok(c) if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            Ok(c) => out.push(c),
            Err(lone) => out.push_str(&format!("\\u{:04x}", lone.unpaired_surrogate())),
        }
    }
    out.push('"');
}

pub(crate) fn front_json(model: &CynefinModel) -> String {
    let mut out = String::from("{\"model\":{");
    let mut first = true;
    let mut key = |out: &mut String, name: &str| {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!("\"{name}\":"));
    };
    for (name, text) in [("title", &model.title), ("accTitle", &model.acc_title), ("accDescr", &model.acc_descr)] {
        if let Some(text) = text {
            key(&mut out, name);
            string(&mut out, text);
        }
    }
    key(&mut out, "domains");
    out.push('[');
    for (index, (domain, items)) in model.domains.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"name\":\"{}\",\"items\":[", domain.name()));
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"label\":");
            string(&mut out, item);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push(']');
    key(&mut out, "transitions");
    out.push('[');
    for (index, transition) in model.transitions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"from\":\"{}\",\"to\":\"{}\"", transition.from.name(), transition.to.name()));
        if let Some(label) = &transition.label {
            out.push_str(",\"label\":");
            string(&mut out, label);
        }
        out.push('}');
    }
    out.push_str("]}}");
    out
}
