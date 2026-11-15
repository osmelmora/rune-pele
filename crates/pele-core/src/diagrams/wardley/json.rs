//! The model as the oracle's `wardley_front` op prints Pele's: each field in a fixed order, an
//! undefined one as `null`, and a number JSON cannot hold, or `-0`, as `{"$number":"…"}`.

use super::model::{Mark, WardleyModel};
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
    if value == 0.0 && value.is_sign_negative() {
        out.push_str("{\"$number\":\"-0\"}");
    } else if value.is_finite() {
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

    fn text(&mut self, name: &str, text: Option<&[u16]>) {
        match text {
            Some(text) => string(self.key(name), text),
            None => self.key(name).push_str("null"),
        }
    }

    fn number(&mut self, name: &str, value: Option<f64>) {
        match value {
            Some(value) => number(self.key(name), value),
            None => self.key(name).push_str("null"),
        }
    }

    fn flag(&mut self, name: &str, value: Option<bool>) {
        let written = match value {
            Some(true) => "true",
            Some(false) => "false",
            None => "null",
        };
        self.key(name).push_str(written);
    }

    fn list<T>(&mut self, name: &str, items: &[T], mut write: impl FnMut(&mut String, &T)) {
        let out = self.key(name);
        out.push('[');
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write(out, item);
        }
        out.push(']');
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn marks(fields: &mut Fields<'_>, name: &str, key: &str, list: &[Mark]) {
    fields.list(name, list, |out, mark| {
        let mut fields = Fields::open(out);
        fields.text(key, Some(&mark.text));
        fields.number("x", Some(mark.x));
        fields.number("y", Some(mark.y));
        fields.close();
    });
}

pub(crate) fn front_json(model: &WardleyModel) -> String {
    let mut out = String::from("{\"model\":");
    let mut fields = Fields::open(&mut out);
    fields.text("title", model.title.as_deref());
    fields.text("accTitle", model.accessible.title.as_deref());
    fields.text("accDescr", model.accessible.description.as_deref());
    fields.list("nodes", &model.nodes, |out, node| {
        let mut fields = Fields::open(out);
        fields.text("id", Some(&node.id));
        fields.text("label", Some(&node.label));
        fields.number("x", Some(node.x));
        fields.number("y", Some(node.y));
        string(fields.key("className"), &node.class.name().encode_utf16().collect::<Vec<_>>());
        fields.number("labelOffsetX", node.label_offset_x);
        fields.number("labelOffsetY", node.label_offset_y);
        fields.flag("inPipeline", Some(node.in_pipeline));
        fields.flag("isPipelineParent", Some(node.is_pipeline_parent));
        fields.flag("inertia", node.inertia);
        let strategy: Option<Vec<u16>> = node.strategy.map(|strategy| strategy.name().encode_utf16().collect());
        fields.text("sourceStrategy", strategy.as_deref());
        fields.close();
    });
    fields.list("links", &model.links, |out, link| {
        let mut fields = Fields::open(out);
        fields.text("source", Some(&link.source));
        fields.text("target", Some(&link.target));
        fields.flag("dashed", Some(link.dashed));
        fields.text("label", link.label.as_deref());
        let flow: Option<Vec<u16>> = link.flow.map(|flow| flow.name().encode_utf16().collect());
        fields.text("flow", flow.as_deref());
        fields.close();
    });
    fields.list("trends", &model.trends, |out, trend| {
        let mut fields = Fields::open(out);
        fields.text("nodeId", Some(&trend.node_id));
        fields.number("targetX", Some(trend.target_x));
        fields.number("targetY", Some(trend.target_y));
        fields.close();
    });
    fields.list("pipelines", &model.pipelines, |out, pipeline| {
        let mut fields = Fields::open(out);
        fields.text("nodeId", Some(&pipeline.node_id));
        fields.list("componentIds", &pipeline.component_ids, |out, id| string(out, id));
        fields.close();
    });
    fields.list("annotations", &model.annotations, |out, annotation| {
        let mut fields = Fields::open(out);
        fields.number("number", Some(annotation.number));
        fields.list("coordinates", &[annotation.at], |out, &(x, y)| {
            let mut fields = Fields::open(out);
            fields.number("x", Some(x));
            fields.number("y", Some(y));
            fields.close();
        });
        fields.text("text", Some(&annotation.text));
        fields.close();
    });
    marks(&mut fields, "notes", "text", &model.notes);
    marks(&mut fields, "accelerators", "name", &model.accelerators);
    marks(&mut fields, "deaccelerators", "name", &model.deaccelerators);
    match model.annotations_box {
        Some((x, y)) => {
            let mut inner = Fields::open(fields.key("annotationsBox"));
            inner.number("x", Some(x));
            inner.number("y", Some(y));
            inner.close();
        }
        None => fields.key("annotationsBox").push_str("null"),
    }
    {
        let mut axes = Fields::open(fields.key("axes"));
        match &model.axes {
            Some((stages, boundaries)) => {
                axes.list("stages", stages, |out, stage| string(out, stage));
                axes.list("stageBoundaries", boundaries, |out, &boundary| number(out, boundary));
            }
            None => {
                axes.key("stages").push_str("null");
                axes.key("stageBoundaries").push_str("null");
            }
        }
        axes.close();
    }
    match model.size {
        Some((width, height)) => {
            let mut inner = Fields::open(fields.key("size"));
            inner.number("width", Some(width));
            inner.number("height", Some(height));
            inner.close();
        }
        None => fields.key("size").push_str("null"),
    }
    fields.close();
    out.push('}');
    out
}
