//! The model and ER graph as the oracle's `er_front` op prints Pele's: `JSON.stringify` of each
//! record in its interface's field order, without the fields that are undefined.

use super::graph::{ErGraph, ErGraphEdge, ErGraphNode};
use super::{ErAttribute, ErModel, Identification};

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

/// An object being written, one field at a time.
struct Fields<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Fields<'a> {
    fn open(out: &'a mut String) -> Self {
        out.push('{');
        Fields { out, first: true }
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

    fn text(&mut self, name: &str, text: &[u16]) {
        string(self.key(name), text);
    }

    fn raw(&mut self, name: &str, json: &str) {
        self.key(name).push_str(json);
    }

    fn list<T>(&mut self, name: &str, items: impl IntoIterator<Item = T>, mut each: impl FnMut(&mut String, T)) {
        let out = self.key(name);
        out.push('[');
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            each(out, item);
        }
        out.push(']');
    }

    fn texts<'t>(&mut self, name: &str, items: impl IntoIterator<Item = &'t [u16]>) {
        self.list(name, items, string);
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn attribute(out: &mut String, attribute: &ErAttribute) {
    let mut f = Fields::open(out);
    f.text("type", &attribute.kind);
    f.text("name", &attribute.name);
    f.texts("keys", attribute.keys.iter().map(|key| &**key));
    f.text("comment", &attribute.comment);
    f.close();
}

fn model_json(out: &mut String, model: &ErModel, mermaid: &str) {
    let mut f = Fields::open(out);
    f.raw("type", "\"er\"");
    f.raw("mermaidType", &format!("\"{mermaid}\""));
    f.raw("direction", &format!("\"{}\"", model.direction.name()));
    f.list("entities", model.entities.values(), |out, entity| {
        let mut f = Fields::open(out);
        f.text("id", &entity.id);
        f.text("label", &entity.label);
        f.text("alias", &entity.alias);
        f.list("attributes", &entity.attributes, attribute);
        f.text("cssClasses", &entity.css_classes);
        f.texts("cssStyles", entity.css_styles.iter().map(|style| &**style));
        f.close();
    });
    f.list("relationships", &model.relationships, |out, relationship| {
        let mut f = Fields::open(out);
        f.text("entityA", &relationship.entity_a);
        f.text("roleA", &relationship.role_a);
        f.text("entityB", &relationship.entity_b);
        let spec = relationship.rel_spec;
        let spec = format!(
            "{{\"cardA\":\"{}\",\"relType\":\"{}\",\"cardB\":\"{}\"}}",
            spec.card_a.name(),
            spec.rel_type.name(),
            spec.card_b.name()
        );
        f.raw("relSpec", &spec);
        f.close();
    });
    f.list("classes", model.classes.values(), |out, def| {
        let mut f = Fields::open(out);
        f.text("id", &def.id);
        f.texts("styles", def.styles.iter().map(|style| &**style));
        f.texts("textStyles", def.text_styles.iter().map(|style| &**style));
        f.close();
    });
    f.list("subgraphs", &model.subgraphs, |out, subgraph| {
        let mut f = Fields::open(out);
        f.text("id", &subgraph.id);
        f.texts("nodes", subgraph.nodes.iter().map(|node| &**node));
        f.text("title", &subgraph.title);
        f.texts("classes", subgraph.classes.iter().map(|name| &**name));
        f.texts("cssStyles", subgraph.css_styles.iter().map(|style| &**style));
        if let Some(dir) = subgraph.dir {
            f.raw("dir", &format!("\"{}\"", dir.name()));
        }
        f.raw("labelType", "\"markdown\"");
        f.close();
    });
    for (name, text) in [("title", &model.title), ("accTitle", &model.acc_title), ("accDescr", &model.acc_descr)] {
        if let Some(text) = text {
            f.text(name, text);
        }
    }
    f.close();
}

fn graph_node(out: &mut String, node: &ErGraphNode<'_>) {
    let mut f = Fields::open(out);
    f.text("id", &node.id);
    f.text("label", &node.label);
    f.text("alias", &node.alias);
    f.list("attributes", node.attributes, attribute);
    f.raw("isGroup", if node.is_group { "true" } else { "false" });
    if let Some(parent) = &node.parent_id {
        f.text("parentId", parent);
    }
    f.texts("cssStyles", node.css_styles.iter().map(|style| &**style));
    f.texts("cssCompiledStyles", node.css_compiled_styles.iter().map(|style| &**style));
    f.text("cssClasses", &node.css_classes);
    if let Some(dir) = node.dir {
        f.raw("dir", &format!("\"{}\"", dir.name()));
    }
    f.raw("labelType", "\"markdown\"");
    f.close();
}

fn graph_edge(out: &mut String, edge: &ErGraphEdge) {
    let mut f = Fields::open(out);
    f.text("id", &edge.id);
    f.text("start", &edge.start);
    f.text("end", &edge.end);
    f.text("label", &edge.label);
    f.raw("arrowTypeStart", &format!("\"{}\"", edge.arrow_type_start.marker_name()));
    f.raw("arrowTypeEnd", &format!("\"{}\"", edge.arrow_type_end.marker_name()));
    let pattern = if edge.pattern == Identification::Identifying { "\"solid\"" } else { "\"dashed\"" };
    f.raw("pattern", pattern);
    f.close();
}

/// The model and ER graph as one JSON object, as the oracle's `er_front` op writes Pele's.
pub(crate) fn front_json(model: &ErModel, graph: &ErGraph<'_>, mermaid: &str) -> String {
    let mut out = String::new();
    out.push_str("{\"model\":");
    model_json(&mut out, model, mermaid);
    out.push_str(",\"graph\":");
    let mut f = Fields::open(&mut out);
    f.list("nodes", &graph.nodes, graph_node);
    f.list("edges", &graph.edges, graph_edge);
    f.close();
    out.push('}');
    out
}
