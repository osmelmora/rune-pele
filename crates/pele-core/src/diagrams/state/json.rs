//! The model and graph as the oracle's `state_front` op prints Pele's: `JSON.stringify` of each
//! record in its interface's field order, without the fields that are undefined.

use super::{DbState, Description, DocId, GraphEdge, GraphNode, StateModel, StateNote, StateStmt, Stmt};
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

fn list<T>(out: &mut String, items: &[T], mut each: impl FnMut(&mut String, &T)) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        each(out, item);
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

    fn text(&mut self, name: &str, text: &[u16]) {
        string(self.key(name), text);
    }

    fn some_text(&mut self, name: &str, text: Option<&JsText>) {
        if let Some(text) = text {
            self.text(name, text);
        }
    }

    fn name(&mut self, name: &str, value: &str) {
        let out = self.key(name);
        out.push('"');
        out.push_str(value);
        out.push('"');
    }

    fn texts(&mut self, name: &str, items: &[JsText]) {
        strings(self.key(name), items);
    }

    fn flag(&mut self, name: &str, value: bool) {
        self.key(name).push_str(if value { "true" } else { "false" });
    }

    fn close(self) {
        self.out.push('}');
    }
}

struct Writer<'a> {
    docs: &'a [Vec<Stmt>],
}

impl Writer<'_> {
    fn doc(&self, out: &mut String, doc: DocId) {
        list(out, &self.docs[doc.0], |out, item| self.statement(out, item));
    }

    fn note(out: &mut String, note: &StateNote) {
        let mut fields = Fields::open(out);
        fields.text("position", &note.position);
        fields.text("text", &note.text);
        fields.close();
    }

    fn description(out: &mut String, description: &Description) {
        match description {
            Description::Text(text) => string(out, text),
            Description::Lines(lines) => strings(out, lines),
        }
    }

    fn state(&self, out: &mut String, state: &StateStmt) {
        let mut fields = Fields::open(out);
        fields.name("stmt", "state");
        fields.text("id", &state.id);
        if let Some(kind) = state.kind {
            fields.name("type", kind.name());
        }
        if let Some(description) = &state.description {
            Self::description(fields.key("description"), description);
        }
        if let Some(doc) = state.doc {
            self.doc(fields.key("doc"), doc);
        }
        if let Some(note) = &state.note {
            Self::note(fields.key("note"), note);
        }
        if let Some(start) = state.start {
            fields.flag("start", start);
        }
        if let Some(classes) = &state.classes {
            fields.texts("classes", classes);
        }
        fields.close();
    }

    fn db_state(&self, out: &mut String, state: &DbState) {
        let mut fields = Fields::open(out);
        fields.name("stmt", "state");
        fields.text("id", &state.id);
        fields.name("type", state.kind.name());
        fields.texts("descriptions", &state.descriptions);
        if let Some(doc) = state.doc {
            self.doc(fields.key("doc"), doc);
        }
        if let Some(note) = &state.note {
            Self::note(fields.key("note"), note);
        }
        fields.texts("classes", &state.classes);
        fields.texts("styles", &state.styles);
        fields.texts("textStyles", &state.text_styles);
        fields.close();
    }

    fn statement(&self, out: &mut String, item: &Stmt) {
        let pair = |out: &mut String, kind: &str, names: [&str; 2], values: [&JsText; 2]| {
            let mut fields = Fields::open(out);
            fields.name("stmt", kind);
            fields.text(names[0], values[0]);
            fields.text(names[1], values[1]);
            fields.close();
        };
        match item {
            Stmt::Text(text) => string(out, text),
            Stmt::State(state) => self.state(out, state),
            Stmt::Relation { state1, state2, description } => {
                let mut fields = Fields::open(out);
                fields.name("stmt", "relation");
                self.state(fields.key("state1"), state1);
                self.state(fields.key("state2"), state2);
                fields.some_text("description", description.as_ref());
                fields.close();
            }
            Stmt::ClassDef { id, classes } => pair(out, "classDef", ["id", "classes"], [id, classes]),
            Stmt::Style { id, style_class } => pair(out, "style", ["id", "styleClass"], [id, style_class]),
            Stmt::ApplyClass { id, style_class } => pair(out, "applyClass", ["id", "styleClass"], [id, style_class]),
            Stmt::Dir(value) => {
                let mut fields = Fields::open(out);
                fields.name("stmt", "dir");
                fields.name("value", value.name());
                fields.close();
            }
            Stmt::Click { id, url, tooltip } => {
                let mut fields = Fields::open(out);
                fields.name("stmt", "click");
                self.state(fields.key("id"), id);
                fields.text("url", url);
                fields.text("tooltip", tooltip);
                fields.close();
            }
        }
    }
}

fn node(out: &mut String, node: &GraphNode) {
    let mut fields = Fields::open(out);
    fields.text("id", &node.id);
    if let Some(shape) = node.shape {
        fields.name("shape", shape.name());
    }
    fields.some_text("label", node.label.as_ref());
    fields.texts("description", &node.description);
    fields.text("cssClasses", &node.css_classes);
    fields.texts("cssCompiledStyles", &node.css_compiled_styles);
    fields.texts("cssStyles", &node.css_styles);
    if let Some(kind) = node.kind {
        fields.name("type", kind);
    }
    fields.flag("isGroup", node.is_group);
    fields.some_text("parentId", node.parent_id.as_ref());
    if let Some(dir) = node.dir {
        fields.name("dir", dir.name());
    }
    fields.some_text("position", node.position.as_ref());
    fields.some_text("noteFor", node.note_for.as_ref());
    if let Some(index) = node.color_index {
        fields.key("colorIndex").push_str(&index.to_string());
    }
    fields.close();
}

fn edge(out: &mut String, edge: &GraphEdge) {
    let mut fields = Fields::open(out);
    fields.text("id", &edge.id);
    fields.text("start", &edge.start);
    fields.text("end", &edge.end);
    fields.text("label", &edge.label);
    fields.name("classes", edge.classes);
    if edge.dashed {
        fields.name("pattern", "dashed");
    }
    fields.close();
}

/// The oracle's `state_front` answer for a model.
pub(crate) fn front_json(model: &StateModel) -> String {
    let writer = Writer { docs: &model.docs };
    let mut out = String::new();
    let mut top = Fields::open(&mut out);
    {
        let mut fields = Fields::open(top.key("model"));
        fields.name("direction", model.direction().name());
        fields.some_text("title", model.title.as_ref());
        fields.some_text("accTitle", model.acc_title.as_ref());
        fields.some_text("accDescr", model.acc_descr.as_ref());
        writer.doc(fields.key("rootDoc"), super::ROOT);
        let states: Vec<&DbState> = model.states.values().collect();
        list(fields.key("states"), &states, |out, state| writer.db_state(out, state));
        list(fields.key("relations"), &model.relations, |out, relation| {
            let mut fields = Fields::open(out);
            fields.text("id1", &relation.id1);
            fields.text("id2", &relation.id2);
            fields.text("relationTitle", &relation.title);
            fields.close();
        });
        let classes: Vec<_> = model.classes.values().collect();
        list(fields.key("classes"), &classes, |out, class| {
            let mut fields = Fields::open(out);
            fields.text("id", &class.id);
            fields.texts("styles", &class.styles);
            fields.texts("textStyles", &class.text_styles);
            fields.close();
        });
        let links: Vec<_> = model.links.iter().collect();
        list(fields.key("links"), &links, |out, (id, link)| {
            out.push('[');
            string(out, id);
            out.push(',');
            let mut fields = Fields::open(out);
            fields.text("url", &link.url);
            fields.text("tooltip", &link.tooltip);
            fields.close();
            out.push(']');
        });
        fields.close();
    }
    {
        let mut fields = Fields::open(top.key("graph"));
        list(fields.key("nodes"), &model.graph.nodes, node);
        list(fields.key("edges"), &model.graph.edges, edge);
        fields.close();
    }
    top.close();
    out
}
