//! The model as the oracle's `sequence_front` op prints Pele's: `JSON.stringify` of each record
//! in the op's field order, without the fields that are undefined.

use super::{Entry, NoteActors, SeqActor, SeqMessage, SequenceModel, number_messages};
use crate::input::{JsText, JsValue};
use crate::js;
use indexmap::IndexMap;

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

/// `JSON.stringify` of a value `JSON.parse` gave.
fn value(out: &mut String, item: &JsValue) {
    match item {
        JsValue::Null => out.push_str("null"),
        JsValue::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        JsValue::Number(number) if number.is_finite() => out.push_str(&js::to_string(*number)),
        JsValue::Number(_) => out.push_str("null"),
        JsValue::String(text) => string(out, text),
        JsValue::Array(items) => list(out, items, value),
        JsValue::Object(object) => {
            out.push('{');
            for (index, (key, item)) in object.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                string(out, key);
                out.push(':');
                value(out, item);
            }
            out.push('}');
        }
    }
}

fn list<T>(out: &mut String, items: impl IntoIterator<Item = T>, mut each: impl FnMut(&mut String, T)) {
    out.push('[');
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        each(out, item);
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

    fn text(&mut self, name: &str, text: &[u16]) {
        string(self.key(name), text);
    }

    /// A field that is left out where the model has no value for it.
    fn text_if(&mut self, name: &str, text: Option<&JsText>) {
        if let Some(text) = text {
            self.text(name, text);
        }
    }

    fn raw(&mut self, name: &str, json: impl std::fmt::Display) {
        let out = self.key(name);
        out.push_str(&json.to_string());
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn menu(out: &mut String, entries: &IndexMap<JsText, JsValue>) {
    list(out, entries, |out, (key, item)| {
        out.push('[');
        string(out, key);
        out.push(',');
        value(out, item);
        out.push(']');
    });
}

fn actor(out: &mut String, id: &JsText, actor: &SeqActor) {
    out.push('[');
    string(out, id);
    out.push(',');
    let mut f = Fields::open(out);
    if let Some(index) = actor.in_box {
        f.raw("box", index);
    }
    f.text("name", &actor.name);
    f.text("description", &actor.description);
    f.raw("wrap", actor.wrap);
    f.text_if("prevActor", actor.prev_actor.as_ref());
    f.text_if("nextActor", actor.next_actor.as_ref());
    menu(f.key("links"), &actor.links);
    menu(f.key("properties"), &actor.properties);
    f.text_if("details", actor.details.as_ref());
    f.text("type", &actor.kind);
    f.close();
    out.push(']');
}

fn message(out: &mut String, index: usize, message: &SeqMessage) {
    let mut f = Fields::open(out);
    f.text("id", &JsText::from(index.to_string().as_str()));
    f.text_if("from", message.from.as_ref());
    f.text_if("to", message.to.as_ref());
    match &message.entry {
        Entry::Numbering(numbering) => {
            let mut inner = Fields::open(f.key("message"));
            for (name, number) in [("start", numbering.start), ("step", numbering.step)] {
                if let Some(number) = number {
                    inner.text(name, &JsText::from(js::to_string(number).as_str()));
                }
            }
            inner.raw("visible", numbering.visible);
            inner.close();
        }
        _ => f.text("message", &message.text),
    }
    f.raw("wrap", message.wrap);
    f.raw("type", message.kind().number());
    match message.entry {
        Entry::Signal { activate, central, .. } => {
            f.raw("activate", activate);
            f.raw("centralConnection", central.map_or(0, |central| central.number()));
        }
        Entry::Note { placement } => f.raw("placement", placement.number()),
        Entry::Numbering(_) => {}
    }
    f.close();
}

pub(crate) fn front_json(model: &SequenceModel, mermaid_type: &str) -> String {
    let mut out = String::from("{\"model\":");
    let mut f = Fields::open(&mut out);
    f.raw("type", "\"sequence\"");
    f.raw("mermaidType", format!("\"{mermaid_type}\""));
    list(f.key("actors"), &model.actors, |out, (id, item)| actor(out, id, item));
    let counted = |out: &mut String, (id, count): (&JsText, &usize)| {
        out.push('[');
        string(out, id);
        out.push_str(&format!(",{count}]"));
    };
    list(f.key("createdActors"), &model.created_actors, counted);
    list(f.key("destroyedActors"), &model.destroyed_actors, counted);
    list(f.key("boxes"), &model.boxes, |out, item| {
        let mut f = Fields::open(out);
        f.text_if("name", item.name.as_ref());
        f.raw("wrap", item.wrap);
        f.text("fill", &item.fill);
        list(f.key("actorKeys"), &item.actor_keys, |out, key| string(out, key));
        f.close();
    });
    list(f.key("messages"), model.messages.iter().enumerate(), |out, (index, item)| message(out, index, item));
    list(f.key("notes"), &model.notes, |out, note| {
        let mut f = Fields::open(out);
        match &note.actor {
            NoteActors::One(actor) => f.text("actor", actor),
            NoteActors::Span(from, to) => list(f.key("actor"), [from, to], |out, actor| string(out, actor)),
        }
        f.raw("placement", note.placement.number());
        f.text("message", &note.message);
        f.raw("wrap", note.wrap);
        f.close();
    });
    f.text_if("title", model.title.as_ref());
    f.text_if("accTitle", model.acc_title.as_ref());
    f.text_if("accDescr", model.acc_descr.as_ref());
    let numbering = number_messages(&model.messages, false);
    list(f.key("numbers"), &numbering.index, |out, number| {
        string(out, &JsText::from(js::to_string(*number).as_str()));
    });
    list(f.key("shown"), &numbering.shown, |out, shown| out.push_str(if *shown { "true" } else { "false" }));
    f.raw("numbered", numbering.visible);
    f.close();
    out.push('}');
    out
}
