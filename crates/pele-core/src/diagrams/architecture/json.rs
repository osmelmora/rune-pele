//! The model as the oracle's `architecture_front` op prints Pele's: `JSON.stringify` of each
//! record with its fields in the order Pele's db builds it, without the ones that are undefined.

use super::{ArchitectureModel, End, NodeKind};
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

    fn maybe(&mut self, name: &str, text: Option<&JsText>) {
        if let Some(text) = text {
            self.text(name, text);
        }
    }

    fn flag(&mut self, name: &str, value: bool) {
        self.key(name).push_str(if value { "true" } else { "false" });
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn list<T>(out: &mut String, items: &[T], mut item: impl FnMut(&mut String, &T)) {
    out.push('[');
    for (index, value) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        item(out, value);
    }
    out.push(']');
}

pub(crate) fn front_json(model: &ArchitectureModel) -> String {
    let group_id = |parent: Option<usize>| parent.map(|group| &model.groups[group].id);
    let mut out = String::new();
    let mut root = Fields::open(&mut out);
    let mut m = Fields::open(root.key("model"));
    m.maybe("title", model.title.as_ref());
    m.maybe("accTitle", model.acc_title.as_ref());
    m.maybe("accDescr", model.acc_descr.as_ref());
    list(m.key("nodes"), &model.nodes, |out, node| {
        let mut f = Fields::open(out);
        f.text("id", &node.id);
        match &node.kind {
            NodeKind::Service(service) => {
                f.key("type").push_str("\"service\"");
                f.maybe("icon", service.icon.as_ref());
                f.maybe("iconText", service.icon_text.as_ref());
                f.maybe("title", service.title.as_ref());
            }
            NodeKind::Junction => f.key("type").push_str("\"junction\""),
        }
        f.maybe("in", group_id(node.parent));
        f.close();
    });
    list(m.key("groups"), &model.groups, |out, group| {
        let mut f = Fields::open(out);
        f.text("id", &group.id);
        f.maybe("icon", group.icon.as_ref());
        f.maybe("title", group.title.as_ref());
        f.maybe("in", group_id(group.parent));
        f.close();
    });
    list(m.key("edges"), &model.edges, |out, edge| {
        let mut f = Fields::open(out);
        let mut end = |name: &str, end: &End| {
            f.text(&format!("{name}Id"), &model.nodes[end.node].id);
            f.key(&format!("{name}Dir")).push_str(&format!("\"{}\"", end.side.letter()));
            f.flag(&format!("{name}Into"), end.into);
            f.flag(&format!("{name}Group"), end.via_group);
        };
        end("lhs", &edge.lhs);
        end("rhs", &edge.rhs);
        f.maybe("title", edge.title.as_ref());
        f.close();
    });
    list(m.key("layoutHints"), &model.hints, |out, hint| {
        let mut f = Fields::open(out);
        f.key("direction").push_str(&format!("\"{}\"", hint.direction.name()));
        list(f.key("members"), &hint.members, |out, &member| string(out, &model.nodes[member].id));
        f.close();
    });
    m.close();
    root.close();
    out
}
