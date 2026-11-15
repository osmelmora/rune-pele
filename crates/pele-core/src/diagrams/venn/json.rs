//! The model as the oracle's `venn_front` op prints Pele's: `JSON.stringify` of each record in
//! its interface's field order, without the fields that are undefined, and each size as the bits
//! of its double.

use super::VennModel;

/// `JSON.stringify` of a string.
pub(super) fn string(out: &mut String, text: &[u16]) {
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

fn strings<'a>(out: &mut String, list: impl IntoIterator<Item = &'a [u16]>) {
    out.push('[');
    for (index, text) in list.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        string(out, text);
    }
    out.push(']');
}

fn label(out: &mut String, label: Option<&[u16]>) {
    if let Some(label) = label {
        out.push_str(",\"label\":");
        string(out, label);
    }
}

pub(crate) fn front_json(model: &VennModel) -> String {
    let mut out = String::from("{\"type\":\"venn\"");
    if let Some(title) = &model.title {
        out.push_str(",\"title\":");
        string(&mut out, title);
    }
    out.push_str(",\"subsets\":[");
    for (index, subset) in model.subsets.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"sets\":");
        strings(&mut out, subset.sets.iter().map(|set| &set[..]));
        out.push_str(&format!(",\"size\":\"{:016x}\"", subset.size.to_bits()));
        label(&mut out, subset.label.as_deref());
        out.push('}');
    }
    out.push_str("],\"textNodes\":[");
    for (index, text) in model.text_nodes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"sets\":");
        strings(&mut out, text.sets.iter().map(|set| &set[..]));
        out.push_str(",\"id\":");
        string(&mut out, &text.id);
        label(&mut out, text.label.as_deref());
        out.push('}');
    }
    out.push_str("],\"styles\":[");
    for (index, style) in model.styles.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"targets\":");
        strings(&mut out, style.targets.iter().map(|target| &target[..]));
        out.push_str(",\"styles\":[");
        for (index, (key, value)) in style.styles.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            strings(&mut out, [&key[..], &value[..]]);
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}
