//! The model and the graph as the oracle's `usecase_front` op prints Pele's (`harness/
//! usecase.ts`): `JSON.stringify` of each record in its interface's field order, without the
//! fields that are undefined. A JSON node is shown by the order of its keys, as Pele's
//! `propertyOrder` records it.

use super::graph::{GraphEdge, GraphNode, NodeShape, UsecaseGraph};
use super::json::{Json, Node};
use super::{UsecaseModel, graph::JsonRow};
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

/// One JSON object being written. A field whose value is `None` is left out.
struct Record<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Record<'a> {
    fn open(out: &'a mut String) -> Self {
        out.push('{');
        Record { out, first: true }
    }

    fn key(&mut self, name: &str) -> &mut String {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        self.out.push('"');
        self.out.push_str(name);
        self.out.push_str("\":");
        self.out
    }

    fn text(mut self, name: &str, value: &[u16]) -> Self {
        string(self.key(name), value);
        self
    }

    fn name(mut self, name: &str, value: &str) -> Self {
        let out = self.key(name);
        out.push('"');
        out.push_str(value);
        out.push('"');
        self
    }

    fn maybe_text(self, name: &str, value: Option<&JsText>) -> Self {
        match value {
            Some(value) => self.text(name, value),
            None => self,
        }
    }

    fn maybe_name(self, name: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.name(name, value),
            None => self,
        }
    }

    fn raw(mut self, name: &str, value: impl std::fmt::Display) -> Self {
        let out = self.key(name);
        out.push_str(&value.to_string());
        self
    }

    fn maybe_flag(self, name: &str, value: Option<bool>) -> Self {
        match value {
            Some(value) => self.raw(name, value),
            None => self,
        }
    }

    fn texts(mut self, name: &str, values: &[JsText]) -> Self {
        list(self.key(name), values, |out, value| string(out, value));
        self
    }

    fn list<I>(mut self, name: &str, items: impl IntoIterator<Item = I>, write: impl Fn(&mut String, I)) -> Self {
        list(self.key(name), items, write);
        self
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn list<I>(out: &mut String, items: impl IntoIterator<Item = I>, write: impl Fn(&mut String, I)) {
    out.push('[');
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write(out, item);
    }
    out.push(']');
}

/// Pele's `propertyOrder` for a parsed JSON text: the keys of each object by its JSON Pointer,
/// sorted by pointer.
fn property_order(json: &Json) -> Vec<(JsText, Vec<JsText>)> {
    let mut order = Vec::new();
    let mut stack = vec![(json.root(), JsText::default())];
    while let Some((at, pointer)) = stack.pop() {
        let child = |token: &[u16]| {
            let mut out = pointer.clone();
            out.push(u16::from(b'/'));
            for &c in token {
                match c {
                    0x7e => out.push_str("~0"),
                    0x2f => out.push_str("~1"),
                    _ => out.push(c),
                }
            }
            out
        };
        match json.node(at) {
            Node::Object(entries) => {
                stack.extend(entries.iter().map(|(key, &value)| (value, child(key))));
                order.push((pointer, entries.keys().cloned().collect()));
            }
            Node::Array(items) => {
                let index = |i: usize| i.to_string().encode_utf16().collect::<Vec<u16>>();
                stack.extend(items.iter().enumerate().map(|(i, &item)| (item, child(&index(i)))));
            }
            _ => {}
        }
    }
    order.sort_by(|a, b| a.0[..].cmp(&b.0[..]));
    order
}

fn row(out: &mut String, row: &JsonRow) {
    Record::open(out)
        .text("key", &row.key)
        .text("accessibleKey", &row.accessible_key)
        .text("value", &row.value)
        .close();
}

fn graph_node(out: &mut String, node: &GraphNode) {
    let actor = match node.shape {
        NodeShape::Actor(kind) => Some(kind.name()),
        _ => None,
    };
    let mut record = Record::open(out)
        .text("id", &node.id)
        .text("label", &node.label)
        .name("labelType", node.label_type.name())
        .name("shape", node.shape.name())
        .raw("isGroup", node.is_group)
        .raw("padding", node.padding)
        .text("cssClasses", &node.css_classes)
        .texts("cssStyles", &node.css_styles)
        .texts("cssCompiledStyles", &node.css_compiled_styles)
        .maybe_name("actorType", actor)
        .maybe_flag("business", node.business)
        .maybe_text("icon", node.icon.as_ref())
        .maybe_text("stereotype", node.stereotype.as_ref())
        .maybe_text("parentId", node.parent_id.as_ref())
        .maybe_text("noteTarget", node.note_target.as_ref());
    if let Some(rows) = &node.json_rows {
        record = record.list("jsonRows", rows, row);
    }
    record.maybe_name("boundaryType", node.boundary_type.map(|kind| kind.name())).close();
}

fn graph_edge(out: &mut String, edge: &GraphEdge) {
    Record::open(out)
        .text("id", &edge.id)
        .text("start", &edge.start)
        .text("end", &edge.end)
        .name("relationshipType", edge.kind.name())
        .name("pattern", if edge.dotted { "dotted" } else { "solid" })
        .name("arrowTypeStart", edge.arrow_start.name())
        .name("arrowTypeEnd", edge.arrow_end.name())
        .maybe_text("label", edge.label.as_ref())
        .maybe_name("labelType", edge.label_type.map(|kind| kind.name()))
        .text("classes", &edge.classes)
        .texts("style", &edge.style)
        .texts("cssCompiledStyles", &edge.css_compiled_styles)
        .raw("animate", edge.animate)
        .maybe_name("animation", edge.animation.map(|animation| animation.name()))
        .raw("minlen", edge.minlen)
        .raw("explicitId", edge.explicit_id)
        .raw("internal", edge.internal)
        .close();
}

/// The model and the graph of one source, as the oracle prints Pele's.
pub(crate) fn front_json(model: &UsecaseModel, graph: &UsecaseGraph, mermaid_type: &str) -> String {
    let mut out = String::new();
    let record = Record::open(&mut out)
        .name("type", "usecase")
        .name("mermaidType", mermaid_type)
        .list("actors", model.actors.values(), |out, actor| {
            Record::open(out)
                .text("id", &actor.id)
                .text("label", &actor.label)
                .name("labelType", actor.label_type.name())
                .name("type", actor.kind.name())
                .maybe_text("icon", actor.icon.as_ref())
                .raw("business", actor.business)
                .maybe_text("stereotype", actor.stereotype.as_ref())
                .maybe_text("parentId", actor.parent_id.as_ref())
                .texts("classes", &actor.classes)
                .texts("styles", &actor.styles)
                .close();
        })
        .list("useCases", model.use_cases.values(), |out, use_case| {
            Record::open(out)
                .text("id", &use_case.id)
                .text("label", &use_case.label)
                .name("labelType", use_case.label_type.name())
                .name("shape", use_case.shape.name())
                .raw("business", use_case.business)
                .maybe_text("stereotype", use_case.stereotype.as_ref())
                .maybe_text("parentId", use_case.parent_id.as_ref())
                .texts("classes", &use_case.classes)
                .texts("styles", &use_case.styles)
                .close();
        })
        .list("boundaries", model.system_boundaries.values(), |out, boundary| {
            Record::open(out)
                .text("id", &boundary.id)
                .text("label", &boundary.label)
                .name("labelType", boundary.label_type.name())
                .name("type", boundary.kind.name())
                .texts("members", &boundary.members)
                .texts("classes", &boundary.classes)
                .texts("styles", &boundary.styles)
                .close();
        })
        .list("relationships", &model.relationships, |out, relationship| {
            Record::open(out)
                .text("id", &relationship.id)
                .raw("explicitId", relationship.explicit_id)
                .text("source", &relationship.source)
                .text("target", &relationship.target)
                .name("type", relationship.kind.name())
                .raw("arrowType", relationship.arrow.number())
                .maybe_text("label", relationship.label.as_ref())
                .maybe_name("labelType", relationship.label_type.map(|kind| kind.name()))
                .raw("minlen", relationship.minlen)
                .texts("classes", &relationship.classes)
                .texts("styles", &relationship.styles)
                .raw("animate", relationship.animate)
                .maybe_name("animation", relationship.animation.map(|animation| animation.name()))
                .close();
        })
        .list("notes", model.notes.values(), |out, note| {
            Record::open(out)
                .text("id", &note.id)
                .text("target", &note.target)
                .text("label", &note.label)
                .name("labelType", note.label_type.name())
                .close();
        })
        .list("jsonNodes", model.json_nodes.values(), |out, json| {
            Record::open(out)
                .text("id", &json.id)
                .list("order", property_order(&json.value), |out, (pointer, keys)| {
                    out.push('[');
                    string(out, &pointer);
                    out.push(',');
                    list(out, &keys, |out, key| string(out, key));
                    out.push(']');
                })
                .texts("classes", &json.classes)
                .texts("styles", &json.styles)
                .close();
        })
        .list("classDefs", model.class_defs.values(), |out, class_def| {
            Record::open(out).text("id", &class_def.id).texts("styles", &class_def.styles).close();
        })
        .name("direction", model.direction.name())
        .maybe_text("title", model.title.as_ref())
        .text("accTitle", &model.acc_title)
        .text("accDescr", &model.acc_descr);
    record.close();
    let model = out;

    let mut out = String::from("{\"model\":");
    out.push_str(&model);
    out.push_str(",\"graph\":");
    Record::open(&mut out).list("nodes", &graph.nodes, graph_node).list("edges", &graph.edges, graph_edge).close();
    out.push('}');
    out
}
