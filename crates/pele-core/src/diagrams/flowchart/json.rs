//! The model and flow graph as the oracle's `flow_front` op prints Pele's: `JSON.stringify` of
//! each record in its interface's field order, without the fields that are undefined.

use super::{FlowGraph, FlowchartModel, GraphEdge, GraphNode};
use crate::input::{JsObject, JsText, JsValue};
use crate::js;

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

fn number(out: &mut String, value: f64) {
    if value.is_finite() {
        out.push_str(&js::to_string(value));
    } else {
        out.push_str("null");
    }
}

fn value(out: &mut String, item: &JsValue) {
    match item {
        JsValue::Null => out.push_str("null"),
        JsValue::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        JsValue::Number(n) => number(out, *n),
        JsValue::String(text) => string(out, text),
        JsValue::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                value(out, item);
            }
            out.push(']');
        }
        JsValue::Object(object) => record(out, object),
    }
}

fn record(out: &mut String, object: &JsObject) {
    let mut fields = Fields::open(out);
    for (key, item) in object.iter() {
        fields.key_units(key);
        value(fields.out, item);
    }
    fields.close();
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

    fn key_units(&mut self, name: &[u16]) {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        string(self.out, name);
        self.out.push(':');
    }

    fn text(&mut self, name: &str, text: &[u16]) {
        string(self.key(name), text);
    }

    fn name(&mut self, name: &str, text: &str) {
        let out = self.key(name);
        out.push('"');
        out.push_str(text);
        out.push('"');
    }

    fn texts(&mut self, name: &str, items: &[JsText]) {
        strings(self.key(name), items);
    }

    fn flag(&mut self, name: &str, flag: bool) {
        self.key(name).push_str(if flag { "true" } else { "false" });
    }

    fn number(&mut self, name: &str, n: f64) {
        number(self.key(name), n);
    }

    fn value(&mut self, name: &str, item: &JsValue) {
        value(self.key(name), item);
    }

    fn record(&mut self, name: &str, object: &JsObject) {
        record(self.key(name), object);
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn graph_node(out: &mut String, node: &GraphNode) {
    let mut f = Fields::open(out);
    f.text("id", &node.id);
    f.text("label", &node.label);
    f.name("labelType", node.label_type.name());
    f.name("shape", node.shape.as_str());
    f.flag("isGroup", node.is_group);
    if let Some(parent) = &node.parent_id {
        f.text("parentId", parent);
    }
    f.texts("cssStyles", &node.css_styles);
    f.texts("cssCompiledStyles", &node.css_compiled_styles);
    f.text("cssClasses", &node.css_classes);
    if let Some(dir) = node.dir {
        f.name("dir", dir.name());
    }
    if let Some(link) = &node.link {
        f.text("link", link);
    }
    if let Some(target) = node.link_target {
        f.name("linkTarget", target.name());
    }
    for (name, text) in [("tooltip", &node.tooltip), ("icon", &node.icon), ("pos", &node.pos), ("img", &node.img)] {
        if let Some(text) = text {
            f.text(name, text);
        }
    }
    if let Some(width) = node.asset_width {
        f.number("assetWidth", width);
    }
    if let Some(height) = node.asset_height {
        f.number("assetHeight", height);
    }
    if let Some(constraint) = &node.constraint {
        f.text("constraint", constraint);
    }
    if let Some(index) = node.color_index {
        f.number("colorIndex", index as f64);
    }
    if let Some(metadata) = &node.metadata {
        f.record("metadata", metadata);
    }
    f.close();
}

fn graph_edge(out: &mut String, edge: &GraphEdge) {
    let (start, end) = edge.arrows();
    let mut f = Fields::open(out);
    f.text("id", &edge.id);
    f.flag("isUserDefinedId", edge.is_user_defined_id);
    f.text("start", &edge.start);
    f.text("end", &edge.end);
    f.name("type", edge.link.type_name());
    f.text("label", &edge.label);
    f.name("labelType", edge.label_type.name());
    f.name("thickness", edge.link.stroke_name());
    f.name("pattern", edge.link.stroke_name());
    if let Some(length) = edge.link.length() {
        f.number("minlen", length as f64);
    }
    f.name("arrowTypeStart", start.name());
    f.name("arrowTypeEnd", end.name());
    f.texts("cssCompiledStyles", &edge.css_compiled_styles);
    f.texts("style", &edge.style);
    f.name("classes", edge.classes());
    for (name, item) in [("animate", &edge.animate), ("animation", &edge.animation), ("curve", &edge.curve)] {
        if let Some(item) = item {
            f.value(name, item);
        }
    }
    f.close();
}

fn list<T>(out: &mut String, items: impl IntoIterator<Item = T>, write: impl Fn(&mut String, T)) {
    out.push('[');
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write(out, item);
    }
    out.push(']');
}

fn model(out: &mut String, model: &FlowchartModel, mermaid_type: &str) {
    let mut f = Fields::open(out);
    f.name("type", "flowchart");
    f.name("mermaidType", mermaid_type);
    f.name("direction", model.direction.name());
    list(f.key("nodes"), model.nodes.values(), |out, node| {
        let mut f = Fields::open(out);
        f.text("id", &node.id);
        f.text("text", &node.text);
        f.name("labelType", node.label_type.name());
        if let Some(shape) = node.shape {
            f.name("type", shape.as_str());
        }
        f.texts("styles", &node.styles);
        f.texts("classes", &node.classes);
        f.record("props", &node.props);
        if let Some(link) = &node.link {
            f.text("link", link);
        }
        if let Some(target) = node.link_target {
            f.name("linkTarget", target.name());
        }
        let texts = [
            ("icon", &node.icon),
            ("form", &node.form),
            ("pos", &node.pos),
            ("img", &node.img),
            ("constraint", &node.constraint),
        ];
        for (name, text) in texts {
            if let Some(text) = text {
                f.text(name, text);
            }
        }
        if let Some(width) = node.asset_width {
            f.number("assetWidth", width);
        }
        if let Some(height) = node.asset_height {
            f.number("assetHeight", height);
        }
        f.close();
    });
    list(f.key("edges"), &model.edges, |out, edge| {
        let mut f = Fields::open(out);
        f.text("start", &edge.start);
        f.text("end", &edge.end);
        f.name("type", edge.link.type_name());
        f.text("text", &edge.text);
        f.name("labelType", edge.label_type.name());
        f.name("stroke", edge.link.stroke_name());
        if let Some(length) = edge.link.length() {
            f.number("length", length as f64);
        }
        if let Some(style) = &edge.style {
            f.texts("style", style);
        }
        f.texts("classes", &edge.classes);
        f.text("id", &edge.id);
        f.flag("isUserDefinedId", edge.is_user_defined_id);
        for (name, item) in
            [("interpolate", &edge.interpolate), ("animate", &edge.animate), ("animation", &edge.animation)]
        {
            if let Some(item) = item {
                f.value(name, item);
            }
        }
        f.close();
    });
    list(f.key("subgraphs"), &model.subgraphs, |out, subgraph| {
        let mut f = Fields::open(out);
        f.text("id", &subgraph.id);
        f.texts("nodes", &subgraph.nodes);
        f.text("title", &subgraph.title);
        f.texts("classes", &subgraph.classes);
        if let Some(dir) = subgraph.dir {
            f.name("dir", dir.name());
        }
        f.name("labelType", subgraph.label_type.name());
        if let Some(metadata) = &subgraph.metadata {
            f.record("metadata", metadata);
        }
        f.close();
    });
    list(f.key("classes"), model.classes.values(), |out, class| {
        let mut f = Fields::open(out);
        f.text("id", &class.id);
        f.texts("styles", &class.styles);
        f.texts("textStyles", &class.text_styles);
        f.close();
    });
    list(f.key("tooltips"), &model.tooltips, |out, (id, tooltip)| strings(out, &[id.clone(), tooltip.clone()]));
    if let Some(style) = &model.default_edge_style {
        f.texts("defaultEdgeStyle", style);
    }
    let texts = [
        ("defaultInterpolate", &model.default_interpolate),
        ("title", &model.title),
        ("accTitle", &model.acc_title),
        ("accDescr", &model.acc_descr),
    ];
    for (name, text) in texts {
        if let Some(text) = text {
            f.text(name, text);
        }
    }
    f.close();
}

/// The `flow_front` answer for a parsed flowchart.
pub(crate) fn front_json(parsed: &FlowchartModel, graph: &FlowGraph, mermaid_type: &str) -> String {
    let mut out = String::new();
    let mut f = Fields::open(&mut out);
    model(f.key("model"), parsed, mermaid_type);
    let mut g = Fields::open(f.key("graph"));
    list(g.key("nodes"), &graph.nodes, graph_node);
    list(g.key("edges"), &graph.edges, graph_edge);
    g.close();
    f.close();
    out
}
