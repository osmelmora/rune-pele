//! The model as the oracle's `packet_front` op prints Pele's: each number as JavaScript writes it,
//! since a field can be wider than JSON's numbers reach.

use super::PacketModel;
use crate::js;

/// `JSON.stringify` of a string: quotes, the short escapes, `\u00xx` for other controls and
/// `\udxxx` for a lone surrogate.
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

fn optional(out: &mut String, text: Option<&[u16]>) {
    match text {
        Some(text) => string(out, text),
        None => out.push_str("null"),
    }
}

pub(crate) fn front_json(model: &PacketModel) -> String {
    let mut out = String::from("{\"model\":{\"title\":");
    optional(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    optional(&mut out, model.accessible.title.as_deref());
    out.push_str(",\"accDescr\":");
    optional(&mut out, model.accessible.description.as_deref());
    out.push_str(&format!(",\"bitsPerRow\":\"{}\",\"rows\":[", js::to_string(model.bits_per_row)));
    for (index, row) in model.rows.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('[');
        for (place, block) in row.iter().enumerate() {
            if place > 0 {
                out.push(',');
            }
            let [start, end, bits] = [block.start, block.end, block.bits].map(js::to_string);
            out.push_str(&format!("[\"{start}\",\"{end}\",\"{bits}\","));
            string(&mut out, &block.label);
            out.push(']');
        }
        out.push(']');
    }
    out.push_str("]}}");
    out
}
