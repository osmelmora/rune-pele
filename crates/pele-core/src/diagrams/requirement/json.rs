//! The model and the graph to lay out, as the oracle's `requirement_front` op prints Pele's.

use super::{RequirementModel, Styling, direction_name};
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

/// Writes `"name":` after a comma where one is due.
fn key(out: &mut String, first: &mut bool, name: &str) {
    if !*first {
        out.push(',');
    }
    *first = false;
    out.push('"');
    out.push_str(name);
    out.push_str("\":");
}

fn styling(out: &mut String, styling: &Styling) {
    out.push_str(",\"cssStyles\":");
    strings(out, &styling.css_styles);
    out.push_str(",\"classes\":");
    strings(out, &styling.classes);
}

fn list<T>(out: &mut String, items: impl IntoIterator<Item = T>, mut item: impl FnMut(&mut String, T)) {
    out.push('[');
    for (index, value) in items.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        item(out, value);
    }
    out.push(']');
}

/// The model in its interfaces' field order, and Pele's `buildRequirementGraph`.
pub(crate) fn front_json(model: &RequirementModel, mermaid_type: &str) -> String {
    let mut out = String::new();
    let first = &mut true;
    out.push('{');
    key(&mut out, first, "mermaidType");
    string(&mut out, &mermaid_type.encode_utf16().collect::<Vec<_>>());
    key(&mut out, first, "direction");
    out.push_str(&format!("\"{}\"", direction_name(model.direction)));
    key(&mut out, first, "requirements");
    list(&mut out, model.requirements.values(), |out, r| {
        out.push_str("{\"name\":");
        string(out, &r.name);
        out.push_str(&format!(",\"type\":\"{}\",\"requirementId\":", r.kind.name()));
        string(out, &r.requirement_id);
        out.push_str(",\"text\":");
        string(out, &r.text);
        out.push_str(&format!(",\"risk\":\"{}\"", r.risk.map_or("", |risk| risk.name())));
        out.push_str(&format!(",\"verifyMethod\":\"{}\"", r.verify_method.map_or("", |method| method.name())));
        styling(out, &r.styling);
        out.push('}');
    });
    key(&mut out, first, "elements");
    list(&mut out, model.elements.values(), |out, e| {
        out.push_str("{\"name\":");
        string(out, &e.name);
        out.push_str(",\"type\":");
        string(out, &e.kind);
        out.push_str(",\"docRef\":");
        string(out, &e.doc_ref);
        styling(out, &e.styling);
        out.push('}');
    });
    key(&mut out, first, "relations");
    list(&mut out, &model.relations, |out, relation| {
        out.push_str(&format!("{{\"type\":\"{}\",\"src\":", relation.kind.name()));
        string(out, &relation.src);
        out.push_str(",\"dst\":");
        string(out, &relation.dst);
        out.push('}');
    });
    key(&mut out, first, "classes");
    list(&mut out, model.classes.values(), |out, class| {
        out.push_str("{\"id\":");
        string(out, &class.id);
        out.push_str(",\"styles\":");
        strings(out, &class.styles);
        out.push_str(",\"textStyles\":");
        strings(out, &class.text_styles);
        out.push('}');
    });
    key(&mut out, first, "title");
    optional(&mut out, model.title.as_deref());
    key(&mut out, first, "accTitle");
    optional(&mut out, model.acc_title.as_deref());
    key(&mut out, first, "accDescr");
    optional(&mut out, model.acc_descr.as_deref());

    key(&mut out, first, "graph");
    out.push_str("{\"nodes\":");
    let nodes = model
        .requirements
        .values()
        .map(|r| (&r.name, &r.styling))
        .chain(model.elements.values().map(|e| (&e.name, &e.styling)));
    list(&mut out, nodes, |out, (name, styling)| {
        out.push_str("{\"id\":");
        string(out, name);
        out.push_str(",\"cssClasses\":");
        string(out, &styling.css_classes());
        out.push('}');
    });
    out.push_str(",\"edges\":");
    let known = |id: &JsText| model.requirements.contains_key(id) || model.elements.contains_key(id);
    list(&mut out, model.relations.iter().enumerate(), |out, (index, relation)| {
        out.push_str("{\"id\":");
        string(out, &relation.id(index));
        out.push_str(",\"start\":");
        optional(out, known(&relation.src).then_some(&*relation.src));
        out.push_str(",\"end\":");
        optional(out, known(&relation.dst).then_some(&*relation.dst));
        out.push_str(&format!(",\"type\":\"{}\"", relation.kind.name()));
        out.push_str(&format!(",\"contains\":{}}}", relation.kind == super::Relationship::Contains));
    });
    out.push_str("}}");
    out
}
