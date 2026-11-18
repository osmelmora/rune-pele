//! The model as the oracle's `git_front` op prints Pele's: one JSON object, each field in a fixed
//! order, with `null` where Pele's is undefined.

use super::{CommitType, GitModel};
use crate::input::JsText;

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

fn strings(out: &mut String, items: &[JsText]) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        string(out, item);
    }
    out.push(']');
}

pub(crate) fn front_json(model: &GitModel) -> String {
    let mut out = String::from("{\"model\":{\"title\":");
    optional(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    optional(&mut out, model.accessible.title.as_deref());
    out.push_str(",\"accDescr\":");
    optional(&mut out, model.accessible.description.as_deref());
    out.push_str(&format!(",\"direction\":\"{}\",\"commits\":[", model.direction.name()));
    for (index, commit) in model.commits.values().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        string(&mut out, &commit.id);
        out.push_str(",\"message\":");
        string(&mut out, &commit.message);
        out.push_str(&format!(",\"seq\":{},\"type\":{},\"tags\":", commit.seq, commit.kind.code()));
        strings(&mut out, &commit.tags);
        out.push_str(",\"parents\":");
        strings(&mut out, &commit.parents);
        out.push_str(",\"branch\":");
        string(&mut out, &commit.branch);
        out.push_str(",\"customType\":");
        match commit.custom_type {
            Some(kind) => out.push_str(&kind.code().to_string()),
            None => out.push_str("null"),
        }
        out.push_str(",\"customId\":");
        match (commit.kind, commit.own_id) {
            (CommitType::Merge, own) => out.push_str(if own { "true" } else { "false" }),
            _ => out.push_str("null"),
        }
        out.push('}');
    }
    out.push_str("],\"branches\":[");
    for (index, (name, head)) in model.branches.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('[');
        string(&mut out, name);
        out.push(',');
        optional(&mut out, head.as_deref());
        out.push(']');
    }
    out.push_str("],\"lanes\":");
    strings(&mut out, &model.lanes);
    out.push_str(",\"currentBranch\":");
    string(&mut out, &model.current_branch);
    out.push_str(",\"head\":");
    optional(&mut out, model.head.as_deref());
    out.push_str(",\"warnings\":");
    strings(&mut out, &model.warnings);
    out.push_str("}}");
    out
}
