//! The model, graph and semantic model as the oracle's `agentflow_front` op prints Pele's:
//! `JSON.stringify` of each record in its interface's field order, without the fields that are
//! undefined.

use super::graph::{GraphEdge, GraphNode};
use super::{AgentDiagnostic, AgentNode, AgentflowModel, ElementPosition, shapes};
use crate::input::{JsObject, JsText, JsValue, Units};
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

/// How a number that JSON cannot hold is written.
#[derive(Clone, Copy)]
enum NonFinite {
    /// As `JSON.stringify` writes it.
    Null,
    /// As the oracle writes it, so that NaN and the infinities can be told apart.
    Tagged,
}

fn value(out: &mut String, item: &JsValue, non_finite: NonFinite) {
    match item {
        JsValue::Null => out.push_str("null"),
        JsValue::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        JsValue::Number(n) if n.is_finite() => out.push_str(&js::to_string(*n)),
        JsValue::Number(n) => match non_finite {
            NonFinite::Null => out.push_str("null"),
            NonFinite::Tagged => out.push_str(&format!("{{\"$number\":\"{}\"}}", js::to_string(*n))),
        },
        JsValue::String(text) => string(out, text),
        JsValue::Array(items) => list(out, items, |out, item| value(out, item, non_finite)),
        JsValue::Object(object) => record(out, object, non_finite),
    }
}

fn record(out: &mut String, object: &JsObject, non_finite: NonFinite) {
    let mut fields = Fields::open(out);
    for (key, item) in object.iter() {
        fields.key_units(key);
        value(fields.out, item, non_finite);
    }
    fields.close();
}

/// `JSON.stringify(value)`, for a message that quotes a metadata value.
pub(super) fn stringify(item: &JsValue) -> String {
    let mut out = String::new();
    value(&mut out, item, NonFinite::Null);
    out
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

fn strings(out: &mut String, items: &[JsText]) {
    list(out, items, |out, item| string(out, item));
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

    fn some_text(&mut self, name: &str, text: Option<&JsText>) {
        if let Some(text) = text {
            self.text(name, text);
        }
    }

    fn name(&mut self, name: &str, text: &str) {
        let out = self.key(name);
        out.push('"');
        out.push_str(text);
        out.push('"');
    }

    fn some_name(&mut self, name: &str, text: Option<&str>) {
        if let Some(text) = text {
            self.name(name, text);
        }
    }

    fn texts(&mut self, name: &str, items: &[JsText]) {
        strings(self.key(name), items);
    }

    fn flag(&mut self, name: &str, flag: bool) {
        self.key(name).push_str(if flag { "true" } else { "false" });
    }

    fn number(&mut self, name: &str, n: usize) {
        self.key(name).push_str(&n.to_string());
    }

    /// A value YAML gave, where the model holds one.
    fn yaml(&mut self, name: &str, item: Option<&JsValue>) {
        if let Some(item) = item {
            value(self.key(name), item, NonFinite::Tagged);
        }
    }

    fn metadata(&mut self, object: Option<&JsObject>) {
        if let Some(object) = object {
            record(self.key("metadata"), object, NonFinite::Tagged);
        }
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn position(out: &mut String, position: &ElementPosition) {
    let mut f = Fields::open(out);
    f.number("startLine", position.start_line);
    f.number("startColumn", position.start_column);
    f.number("endLine", position.end_line);
    f.number("endColumn", position.end_column);
    f.number("startIndex", 0);
    f.number("endIndex", 0);
    f.close();
}

fn node(out: &mut String, node: &AgentNode) {
    let mut f = Fields::open(out);
    f.text("id", &node.id);
    f.text("text", &node.text);
    f.name("labelType", node.label_type.name());
    f.some_name("type", node.shape.map(|shape| shape.as_str()));
    f.texts("styles", &node.styles);
    f.texts("classes", &node.classes);
    if let Some(props) = &node.props {
        record(f.key("props"), props, NonFinite::Null);
    }
    f.some_text("link", node.link.as_ref());
    f.some_name("linkTarget", node.link_target.map(|target| target.name()));
    f.metadata(node.metadata.as_ref());
    if node.is_connector {
        f.flag("isConnector", true);
    }
    f.close();
}

fn diagnostic(out: &mut String, diagnostic: &AgentDiagnostic) {
    let mut f = Fields::open(out);
    f.name("id", diagnostic.id.name());
    f.name("severity", diagnostic.id.severity());
    f.text("message", &diagnostic.message);
    f.text("nodeId", &diagnostic.node_id);
    if let Some(at) = &diagnostic.position {
        position(f.key("position"), at);
    }
    f.close();
}

fn model(out: &mut String, model: &AgentflowModel, mermaid_type: &str) {
    let mut f = Fields::open(out);
    f.name("type", "agentflow");
    f.name("mermaidType", mermaid_type);
    f.name("direction", model.direction.name());
    list(f.key("nodes"), model.nodes.values(), node);
    list(f.key("edges"), &model.edges, |out, edge| {
        let mut f = Fields::open(out);
        f.text("start", &edge.start);
        f.text("end", &edge.end);
        f.name("type", edge.link.type_name());
        f.text("text", &edge.text);
        f.name("labelType", edge.label_type.name());
        f.name("stroke", edge.link.stroke_name());
        f.some_name("edgeSemantic", edge.link.semantic().map(|semantic| semantic.name()));
        if let Some(length) = edge.link.length() {
            f.number("length", length);
        }
        if let Some(style) = &edge.style {
            f.texts("style", style);
        }
        f.texts("classes", &edge.classes);
        f.text("id", &edge.id);
        f.flag("isUserDefinedId", edge.is_user_defined_id);
        f.yaml("interpolate", edge.interpolate.as_ref());
        f.yaml("animate", edge.animate.as_ref());
        f.yaml("animation", edge.animation.as_ref());
        f.metadata(edge.metadata.as_ref());
        f.close();
    });
    list(f.key("subgraphs"), &model.subgraphs, |out, subgraph| {
        let mut f = Fields::open(out);
        f.text("id", &subgraph.id);
        f.texts("nodes", &subgraph.nodes);
        f.text("title", &subgraph.title);
        f.texts("classes", &subgraph.classes);
        f.some_name("dir", subgraph.dir.map(|dir| dir.name()));
        f.name("labelType", subgraph.label_type.name());
        f.name("type", "flow");
        f.metadata(subgraph.metadata.as_ref());
        f.close();
    });
    list(f.key("connectors"), &model.connectors, |out, id| string(out, id));
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
    f.some_text("defaultInterpolate", model.default_interpolate.as_ref());
    list(f.key("mappings"), &model.mappings, |out, mapping| {
        let mut f = Fields::open(out);
        f.text("id", &mapping.id);
        f.name("type", mapping.kind.name());
        position(f.key("position"), &mapping.position);
        f.close();
    });
    list(f.key("diagnostics"), &model.diagnostics, diagnostic);
    f.some_text("title", model.title.as_ref());
    f.some_text("accTitle", model.acc_title.as_ref());
    f.some_text("accDescr", model.acc_descr.as_ref());
    f.close();
}

fn graph_node(out: &mut String, node: &GraphNode) {
    let mut f = Fields::open(out);
    f.text("id", &node.id);
    f.text("label", &node.label);
    f.name("labelType", node.label_type.name());
    f.name("shape", node.shape.name());
    f.flag("isGroup", node.is_group());
    f.some_text("parentId", node.parent_id.as_ref());
    f.texts("cssStyles", &node.css_styles);
    f.texts("cssCompiledStyles", &node.css_compiled_styles);
    f.text("cssClasses", &node.css_classes);
    f.some_name("dir", node.dir.map(|dir| dir.name()));
    f.some_text("link", node.link.as_ref());
    f.some_name("linkTarget", node.link_target.map(|target| target.name()));
    f.some_text("tooltip", node.tooltip.as_ref());
    if let Some(index) = node.color_index {
        f.number("colorIndex", index);
    }
    f.some_name("kind", node.kind.map(|kind| kind.name()));
    f.metadata(node.metadata.as_ref());
    f.close();
}

fn graph_edge(out: &mut String, edge: &GraphEdge) {
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
        f.number("minlen", length);
    }
    f.name("arrowTypeStart", "none");
    f.name("arrowTypeEnd", edge.arrow_end().map_or("none", |arrow| arrow.name()));
    f.texts("cssCompiledStyles", &edge.css_compiled_styles);
    f.texts("style", &edge.style);
    f.name("classes", "edge-thickness-normal edge-pattern-solid flowchart-link");
    f.yaml("animate", edge.animate.as_ref());
    f.yaml("animation", edge.animation.as_ref());
    f.yaml("curve", edge.curve.as_ref());
    f.metadata(edge.metadata.as_ref());
    f.close();
}

/// Presentation-only metadata keys, left out of the semantic model.
const PRESENTATION_KEYS: [&str; 11] =
    ["shape", "view", "icon", "img", "form", "pos", "w", "h", "class", "style", "labelType"];

/// Pele's `semanticMetadata`: what metadata says about meaning, or nothing where it says none.
fn semantic_metadata(metadata: Option<&JsObject>) -> Option<JsObject> {
    let mut out = JsObject::default();
    for (key, item) in metadata?.iter() {
        if !PRESENTATION_KEYS.iter().any(|presentation| key.is_str(presentation)) {
            out.set(key.clone(), item.clone());
        }
    }
    let any = out.iter().next().is_some();
    any.then_some(out)
}

/// Pele's `semanticModel`: the diagram with presentation-only controls left out, for tools that
/// read what it means. Its diagnostics are the model's own, which are written there.
fn semantic(out: &mut String, model: &AgentflowModel) {
    let mut f = Fields::open(out);
    f.name("direction", model.direction.name());
    let plain = || model.nodes.values().filter(|node| model.subgraph(&node.id).is_none());
    list(f.key("vertices"), plain().filter(|node| !node.is_connector), |out, node| {
        let mut f = Fields::open(out);
        f.text("id", &node.id);
        f.text("label", &node.text);
        f.some_name("shape", node.shape.map(|shape| shapes::resolve_alias(shape.as_str())));
        f.name("vertexKind", AgentflowModel::vertex_kind(node).name());
        f.metadata(semantic_metadata(node.metadata.as_ref()).as_ref());
        f.close();
    });
    list(f.key("edges"), &model.edges, |out, edge| {
        let mut f = Fields::open(out);
        f.text("start", &edge.start);
        f.text("end", &edge.end);
        f.text("id", &edge.id);
        f.some_text("label", Some(&edge.text).filter(|text| !text.is_empty()));
        f.name("type", edge.link.type_name());
        f.name("stroke", edge.link.stroke_name());
        f.some_name("edgeSemantic", edge.link.semantic().map(|semantic| semantic.name()));
        if let Some(length) = edge.link.length() {
            f.number("length", length);
        }
        f.metadata(edge.metadata.as_ref().filter(|metadata| metadata.iter().next().is_some()));
        f.close();
    });
    list(f.key("subGraphs"), &model.subgraphs, |out, subgraph| {
        let mut f = Fields::open(out);
        f.text("id", &subgraph.id);
        f.name("type", "flow");
        f.text("title", &subgraph.title);
        f.texts("nodes", &subgraph.nodes);
        f.metadata(semantic_metadata(subgraph.metadata.as_ref()).as_ref());
        f.some_name("direction", subgraph.dir.map(|dir| dir.name()));
        f.close();
    });
    list(f.key("connectors"), plain().filter(|node| node.is_connector), |out, node| {
        let mut f = Fields::open(out);
        f.text("id", &node.id);
        f.some_text("title", Some(&node.text).filter(|text| **text != node.id));
        f.metadata(semantic_metadata(node.metadata.as_ref()).as_ref());
        f.close();
    });
    f.close();
}

/// The `agentflow_front` answer for a parsed agentflow.
pub(crate) fn front_json(parsed: &AgentflowModel, mermaid_type: &str) -> String {
    let mut out = String::new();
    let mut f = Fields::open(&mut out);
    model(f.key("model"), parsed, mermaid_type);
    let mut g = Fields::open(f.key("graph"));
    list(g.key("nodes"), &parsed.graph.nodes, graph_node);
    list(g.key("edges"), &parsed.graph.edges, graph_edge);
    g.close();
    semantic(f.key("semantic"), parsed);
    f.close();
    out
}
