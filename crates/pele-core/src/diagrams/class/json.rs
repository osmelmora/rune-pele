//! The model and class graph as the oracle's `class_front` op prints Pele's: `JSON.stringify` of
//! each record in its interface's field order, without the fields that are undefined.

use super::graph::{ClassGraph, GraphEdge, GraphNode};
use super::{ClassMember, ClassModel, ClassNode, NamespaceNode, RelationEnd};
use crate::input::JsText;

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

    fn some(&mut self, name: &str, text: Option<&JsText>) {
        if let Some(text) = text {
            self.text(name, text);
        }
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

    fn texts<'t>(&mut self, name: &str, items: impl IntoIterator<Item = &'t JsText>) {
        self.list(name, items, |out, item| string(out, item));
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn member(out: &mut String, member: &ClassMember) {
    let mut f = Fields::open(out);
    f.text("id", &member.id);
    f.raw("memberType", &format!("\"{}\"", member.member_type.name()));
    f.raw("visibility", &format!("\"{}\"", member.visibility.mark()));
    f.raw("classifier", &format!("\"{}\"", member.classifier.mark()));
    f.text("parameters", &member.parameters);
    f.text("returnType", &member.return_type);
    f.text("display", &member.display_text());
    f.close();
}

fn class(out: &mut String, node: &ClassNode) {
    let mut f = Fields::open(out);
    f.text("id", &node.id);
    f.text("type", &node.generic);
    f.text("label", &node.label);
    f.text("cssClasses", &node.css_classes);
    f.list("methods", &node.methods, member);
    f.list("members", &node.members, member);
    f.texts("annotations", &node.annotations);
    f.texts("styles", &node.styles);
    f.some("parent", node.parent.as_ref());
    f.some("link", node.link.as_ref());
    if let Some(target) = node.link_target {
        f.raw("linkTarget", &format!("\"{}\"", target.name()));
    }
    if node.have_callback {
        f.raw("haveCallback", "true");
    }
    f.some("tooltip", node.tooltip.as_ref());
    f.close();
}

fn end(end: RelationEnd) -> String {
    end.number().map_or("\"none\"".to_owned(), |number| number.to_string())
}

fn namespace(out: &mut String, node: &NamespaceNode) {
    let mut f = Fields::open(out);
    f.text("id", &node.id);
    f.text("label", &node.label);
    f.texts("classes", &node.classes);
    f.texts("notes", &node.notes);
    f.texts("children", &node.children);
    f.some("parent", node.parent.as_ref());
    f.raw("explicit", if node.explicit { "true" } else { "false" });
    f.close();
}

fn model_json(out: &mut String, model: &ClassModel, mermaid: &str) {
    let mut f = Fields::open(out);
    f.raw("type", "\"class\"");
    f.raw("mermaidType", &format!("\"{mermaid}\""));
    f.raw("direction", &format!("\"{}\"", model.direction.name()));
    f.list("classes", model.classes.values(), class);
    f.list("relations", &model.relations, |out, relation| {
        let mut f = Fields::open(out);
        f.text("id1", &relation.id1);
        f.text("id2", &relation.id2);
        f.text("relationTitle1", &relation.relation_title1);
        f.text("relationTitle2", &relation.relation_title2);
        f.some("title", relation.title.as_ref());
        let line = if relation.line_type == super::LineType::Line { 0 } else { 1 };
        let ends =
            format!("{{\"type1\":{},\"type2\":{},\"lineType\":{line}}}", end(relation.type1), end(relation.type2));
        f.raw("relation", &ends);
        f.close();
    });
    f.list("notes", model.notes.values(), |out, note| {
        let mut f = Fields::open(out);
        f.text("id", &note.id);
        f.some("class", note.class.as_ref());
        f.text("text", &note.text);
        f.raw("index", &note.index.to_string());
        f.some("parent", note.parent.as_ref());
        f.close();
    });
    f.list("namespaces", model.namespaces.values(), namespace);
    f.list("interfaces", &model.interfaces, |out, item| {
        let mut f = Fields::open(out);
        f.text("id", &item.id);
        f.text("label", &item.label);
        f.text("classId", &item.class_id);
        f.close();
    });
    f.list("styleClasses", model.style_classes.values(), |out, style| {
        let mut f = Fields::open(out);
        f.text("id", &style.id);
        f.texts("styles", &style.styles);
        f.texts("textStyles", &style.text_styles);
        f.close();
    });
    f.some("title", model.title.as_ref());
    f.some("accTitle", model.acc_title.as_ref());
    f.some("accDescr", model.acc_descr.as_ref());
    f.close();
}

fn graph_node(out: &mut String, node: &GraphNode, model: &ClassModel) {
    let mut f = Fields::open(out);
    f.text("id", &node.id);
    f.text("label", &node.label);
    f.raw("isGroup", if node.is_group { "true" } else { "false" });
    f.raw("shape", &format!("\"{}\"", node.shape.name()));
    f.some("parentId", node.parent_id.as_ref());
    f.raw("parent", &node.parent.map_or("-1".to_owned(), |parent| parent.to_string()));
    if let Some(index) = node.class {
        let (key, _) = model.classes.get_index(index).expect("a class of the model");
        f.text("classNode", key);
    }
    if let Some(index) = node.color_index {
        f.raw("colorIndex", &index.to_string());
    }
    f.close();
}

fn graph_edge(out: &mut String, edge: &GraphEdge) {
    let index = |at: Option<usize>| at.map_or("-1".to_owned(), |at| at.to_string());
    let mut f = Fields::open(out);
    f.text("id", &edge.id);
    f.text("start", &edge.start);
    f.text("end", &edge.end);
    f.raw("source", &index(edge.source));
    f.raw("target", &index(edge.target));
    f.some("label", edge.label.as_ref());
    f.raw("arrowTypeStart", &format!("\"{}\"", edge.arrow_type_start.marker_name()));
    f.raw("arrowTypeEnd", &format!("\"{}\"", edge.arrow_type_end.marker_name()));
    f.text("startLabelRight", &edge.start_label_right);
    f.text("endLabelLeft", &edge.end_label_left);
    f.raw("pattern", &format!("\"{}\"", edge.pattern.name()));
    f.raw("classes", "\"relation\"");
    f.close();
}

/// The model and class graph as one JSON object, as the oracle's `class_front` op writes Pele's.
pub(crate) fn front_json(model: &ClassModel, graph: &ClassGraph, mermaid: &str) -> String {
    let mut out = String::new();
    out.push_str("{\"model\":");
    model_json(&mut out, model, mermaid);
    out.push_str(",\"graph\":{\"nodes\":[");
    for (index, node) in graph.nodes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        graph_node(&mut out, node, model);
    }
    out.push_str("],\"edges\":[");
    for (index, edge) in graph.edges.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        graph_edge(&mut out, edge);
    }
    out.push_str("]}}");
    out
}
