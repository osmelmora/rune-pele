//! The board as the oracle's `kanban_front` op prints Pele's: every field of each node in the
//! order Pele's `KanbanNode` declares them, with one that is undefined as `null`.

use super::{KanbanModel, KanbanNode};

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

fn node(out: &mut String, node: &KanbanNode) {
    out.push_str("{\"id\":");
    string(out, &node.id);
    out.push_str(&format!(",\"level\":{},\"label\":", node.level));
    string(out, &node.label);
    for (name, value) in [
        ("icon", &node.icon),
        ("assigned", &node.assigned),
        ("ticket", &node.ticket),
        ("priority", &node.priority),
        ("cssClasses", &node.css_classes),
    ] {
        out.push_str(&format!(",\"{name}\":"));
        optional(out, value.as_deref());
    }
}

pub(crate) fn front_json(model: &KanbanModel) -> String {
    let mut out = String::from("{\"model\":{\"title\":");
    optional(&mut out, model.title.as_deref());
    out.push_str(",\"sections\":[");
    for (index, section) in model.sections.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        node(&mut out, &section.node);
        out.push_str(",\"items\":[");
        for (index, item) in section.items.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            node(&mut out, item);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("]}}");
    out
}
