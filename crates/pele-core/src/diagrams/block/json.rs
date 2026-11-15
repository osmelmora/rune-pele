//! The model as the oracle's `block_front` op prints Pele's: `JSON.stringify` of each block in
//! the field order of Pele's `Block` interface, without the fields that are undefined. A number
//! JSON cannot hold travels as `{"$number":"Infinity"}`.

use super::{BlockEdge, BlockModel, BlockNode};
use crate::js;

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

fn number(out: &mut String, value: f64) {
    if value.is_finite() {
        out.push_str(&js::to_string(value));
    } else {
        out.push_str(&format!("{{\"$number\":\"{}\"}}", js::to_string(value)));
    }
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

    /// A string field whose value is one of Pele's names.
    fn name(&mut self, name: &str, value: &str) {
        let units: Vec<u16> = value.encode_utf16().collect();
        self.text(name, &units);
    }

    fn number(&mut self, name: &str, value: Option<f64>) {
        if let Some(value) = value {
            number(self.key(name), value);
        }
    }

    fn texts(&mut self, name: &str, items: Option<&[crate::input::JsText]>) {
        let Some(items) = items else {
            return;
        };
        let out = self.key(name);
        out.push('[');
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            string(out, item);
        }
        out.push(']');
    }

    fn close(self) {
        self.out.push('}');
    }
}

/// A block, with its children in full when `nested`, or else only how many it has.
fn block(out: &mut String, model: &BlockModel, block: &BlockNode, nested: bool) {
    let mut f = Fields::open(out);
    f.text("id", &block.id);
    f.name("type", block.kind.name());
    if let Some(label) = &block.label {
        f.text("label", label);
    }
    let children = f.key("children");
    if nested {
        children.push('[');
        for (index, &child) in block.children.iter().enumerate() {
            if index > 0 {
                children.push(',');
            }
            self::block(children, model, &model.blocks[child], true);
        }
        children.push(']');
    } else {
        children.push_str(&block.children.len().to_string());
    }
    f.number("columns", block.columns);
    f.number("widthInColumns", block.width_in_columns);
    f.number("width", block.width);
    f.texts("directions", block.directions.as_deref());
    f.texts("classes", block.classes.as_deref());
    f.texts("styles", block.styles.as_deref());
    f.number("colorIndex", block.color_index);
    f.close();
}

fn edge(out: &mut String, edge: &BlockEdge) {
    let mut f = Fields::open(out);
    f.text("id", &edge.id);
    f.name("type", "edge");
    f.text("label", &edge.label);
    f.texts("directions", edge.directions.as_deref());
    f.text("start", &edge.start);
    f.text("end", &edge.end);
    f.name("arrowTypeStart", edge.arrow_type_start.name());
    f.name("arrowTypeEnd", edge.arrow_type_end.name());
    f.name("thickness", if edge.thick { "thick" } else { "normal" });
    f.name("pattern", if edge.dotted { "dotted" } else { "solid" });
    f.close();
}

/// What the oracle's `block_front` op answers for a source Pele parses.
pub(crate) fn front_json(model: &BlockModel) -> String {
    let mut out = String::from("{\"model\":");
    let mut f = Fields::open(&mut out);
    if let Some(title) = &model.title {
        f.text("title", title);
    }
    block(f.key("root"), model, model.root(), true);
    let edges = f.key("edges");
    edges.push('[');
    for (index, item) in model.edges.iter().enumerate() {
        if index > 0 {
            edges.push(',');
        }
        edge(edges, item);
    }
    edges.push(']');
    let classes = f.key("classes");
    classes.push('[');
    for (index, class) in model.classes.values().enumerate() {
        if index > 0 {
            classes.push(',');
        }
        let mut c = Fields::open(classes);
        c.text("id", &class.id);
        c.texts("styles", Some(&class.styles));
        c.texts("textStyles", Some(&class.text_styles));
        c.close();
    }
    classes.push(']');
    let by_id = f.key("byId");
    by_id.push('[');
    for (index, (id, &at)) in model.by_id.iter().enumerate() {
        if index > 0 {
            by_id.push(',');
        }
        by_id.push('[');
        string(by_id, id);
        by_id.push(',');
        block(by_id, model, &model.blocks[at], false);
        by_id.push(']');
    }
    by_id.push(']');
    f.close();
    out.push('}');
    out
}
