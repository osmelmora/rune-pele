//! The outline as the oracle's `ishikawa_front` op prints Pele's: the nodes in preorder, each with
//! its depth, which is the tree and nothing else.

use super::IshikawaModel;

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

pub(crate) fn front_json(model: &IshikawaModel) -> String {
    let mut out = String::from("{\"model\":{\"title\":");
    match &model.title {
        Some(title) => string(&mut out, title),
        None => out.push_str("null"),
    }
    out.push_str(",\"nodes\":[");
    let mut depth = vec![0usize; model.nodes.len()];
    for (index, node) in model.nodes.iter().enumerate() {
        for &child in &node.children {
            depth[child] = depth[index] + 1;
        }
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"text\":");
        string(&mut out, &node.text);
        out.push_str(&format!(",\"depth\":{}}}", depth[index]));
    }
    out.push_str("]}}");
    out
}
