//! The model as the oracle's `c4_front` op prints Pele's: `JSON.stringify` of each record in its
//! interface's field order, without the fields that are undefined. A shape's or a boundary's
//! parent is the place of that boundary in the list of boundaries.

use super::{Boundary, C4Model, Element, ElementField, Named, Rel, RelField, Shape};
use crate::input::JsText;
use crate::js;
use indexmap::IndexMap;

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

/// An object being written, one field at a time.
pub(super) struct Fields<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Fields<'a> {
    pub(super) fn open(out: &'a mut String) -> Self {
        out.push('{');
        Fields { out, first: true }
    }

    pub(super) fn key(&mut self, name: &str) -> &mut String {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        self.out.push('"');
        self.out.push_str(name);
        self.out.push_str("\":");
        self.out
    }

    pub(super) fn text(&mut self, name: &str, text: &[u16]) {
        string(self.key(name), text);
    }

    fn maybe(&mut self, name: &str, text: Option<&JsText>) {
        if let Some(text) = text {
            self.text(name, text);
        }
    }

    /// A number as the oracle sends it: `String(n)`, as a JSON string.
    pub(super) fn number(&mut self, name: &str, value: f64) {
        let out = self.key(name);
        out.push('"');
        out.push_str(&js::to_string(value));
        out.push('"');
    }

    pub(super) fn raw(&mut self, name: &str, json: &str) {
        self.key(name).push_str(json);
    }

    fn named(&mut self, name: &str, named: Option<&Named>) {
        if let Some(named) = named {
            let mut f = Fields::open(self.key(name));
            f.text("key", &named.key);
            f.text("value", &named.value);
            f.close();
        }
    }

    fn extra(&mut self, extra: Option<&IndexMap<JsText, JsText>>) {
        if let Some(extra) = extra {
            let out = self.key("extra");
            out.push('[');
            for (index, (key, value)) in extra.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push('[');
                string(out, key);
                out.push(',');
                string(out, value);
                out.push(']');
            }
            out.push(']');
        }
    }

    pub(super) fn close(self) {
        self.out.push('}');
    }
}

fn element(f: &mut Fields<'_>, element: &Element) {
    f.text("alias", &element.alias);
    f.text("label", &element.label);
    f.named("aliasAttr", element.alias_attr.as_ref());
    f.named("labelAttr", element.label_attr.as_ref());
    if let Some(parent) = element.parent {
        f.raw("parent", &parent.to_string());
    }
    for field in ElementField::ALL {
        f.maybe(field.name(), element.field(field));
    }
    f.extra(element.extra.as_ref());
}

fn shape(out: &mut String, shape: &Shape) {
    let mut f = Fields::open(out);
    element(&mut f, &shape.element);
    f.text("kind", &JsText::from(shape.kind.name().as_str()));
    f.close();
}

fn boundary(out: &mut String, boundary: &Boundary) {
    let mut f = Fields::open(out);
    element(&mut f, &boundary.element);
    if let Some(node_type) = boundary.node_type {
        f.text("nodeType", &JsText::from(node_type.name()));
    }
    f.close();
}

fn rel(out: &mut String, rel: &Rel) {
    let mut f = Fields::open(out);
    f.text("type", &JsText::from(rel.kind.name()));
    f.text("from", &rel.from);
    f.text("to", &rel.to);
    f.text("label", &rel.label);
    f.named("fromAttr", rel.from_attr.as_ref());
    f.named("toAttr", rel.to_attr.as_ref());
    f.named("labelAttr", rel.label_attr.as_ref());
    for field in RelField::ALL {
        f.maybe(field.name(), rel.field(field));
    }
    if let Some(x) = rel.offset_x {
        f.number("offsetX", x);
    }
    if let Some(y) = rel.offset_y {
        f.number("offsetY", y);
    }
    f.extra(rel.extra.as_ref());
    f.close();
}

fn list<T>(out: &mut String, items: &[T], each: fn(&mut String, &T)) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        each(out, item);
    }
    out.push(']');
}

/// What the oracle's `c4_front` op answers for a source Pele parses.
pub(crate) fn front_json(model: &C4Model, mermaid_type: &str) -> String {
    let mut out = String::new();
    let mut f = Fields::open(&mut out);
    f.text("type", &JsText::from("c4"));
    f.text("mermaidType", &JsText::from(mermaid_type));
    f.text("c4Type", &JsText::from(model.c4_type.name()));
    if let Some(title) = &model.title {
        f.text("title", title);
    }
    if let Some(descr) = &model.acc_descr {
        f.text("accDescr", descr);
    }
    list(f.key("shapes"), &model.shapes, shape);
    list(f.key("boundaries"), &model.boundaries, boundary);
    list(f.key("rels"), &model.rels, rel);
    f.number("shapeInRow", model.shape_in_row);
    f.number("boundaryInRow", model.boundary_in_row);
    f.close();
    out
}
