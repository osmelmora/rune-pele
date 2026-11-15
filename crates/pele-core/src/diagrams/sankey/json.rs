//! The model as the oracle's `sankey_front` op prints Pele's: the nodes by name, the links with
//! each value as the 16 hex digits of its bits, and the title.

use super::SankeyModel;

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

pub(crate) fn front_json(model: &SankeyModel) -> String {
    let mut out = String::from("{\"model\":{\"nodes\":[");
    for (index, node) in model.nodes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        string(&mut out, node);
    }
    out.push_str("],\"links\":[");
    for (index, link) in model.links.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("[{},{},\"{:016x}\"]", link.source, link.target, link.value.to_bits()));
    }
    out.push_str("],\"title\":");
    match &model.title {
        Some(title) => string(&mut out, title),
        None => out.push_str("null"),
    }
    out.push_str("}}");
    out
}
