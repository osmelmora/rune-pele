//! The map as the oracle's `mindmap_front` op prints Pele's: every field of each node in the order
//! Pele's `MindmapNode` declares them, a child as its id, and a field that is undefined as `null`.

use super::MindmapModel;

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

fn optional(out: &mut String, text: Option<&[u16]>) {
    match text {
        Some(text) => string(out, text),
        None => out.push_str("null"),
    }
}

pub(crate) fn front_json(model: &MindmapModel) -> String {
    let mut out = String::from("{\"model\":{\"title\":");
    optional(&mut out, model.title.as_deref());
    out.push_str(",\"nodes\":[");
    for (id, node) in model.nodes.iter().enumerate() {
        if id > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"id\":{id},\"nodeId\":"));
        string(&mut out, &node.node_id);
        out.push_str(&format!(",\"level\":{},\"descr\":", node.level));
        string(&mut out, &node.descr);
        let children: Vec<String> = node.children.iter().map(usize::to_string).collect();
        out.push_str(&format!(",\"type\":{},\"children\":[{}]", node.shape as u8, children.join(",")));
        out.push_str(&format!(",\"isRoot\":{},\"section\":", id == 0));
        match node.section {
            Some(section) => out.push_str(&section.to_string()),
            None => out.push_str("null"),
        }
        out.push_str(",\"class\":");
        optional(&mut out, node.class.as_deref());
        out.push_str(",\"icon\":");
        optional(&mut out, node.icon.as_deref());
        out.push('}');
    }
    out.push_str("]}}");
    out
}
