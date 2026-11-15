//! The model as the oracle's `treemap_front` op prints Pele's: `JSON.stringify` of each record with
//! its fields in the order Pele builds it, without the ones that are undefined, and each value as
//! the bits of its double.

use super::{Content, TreemapModel, TreemapNode};
use crate::diagrams::railroad::probe::string;

/// The fields of one JSON object, each written after a comma but the first.
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
        self.out.push_str(&format!("\"{name}\":"));
        self.out
    }

    fn close(self) {
        self.out.push('}');
    }
}

fn strings(out: &mut String, list: &[crate::input::JsText]) {
    out.push('[');
    for (index, text) in list.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        string(out, text);
    }
    out.push(']');
}

enum Work<'a> {
    Node(&'a TreemapNode),
    Text(String),
}

/// Queues a list of nodes, then `close`, to be written in order.
fn queue<'a>(work: &mut Vec<Work<'a>>, list: &'a [TreemapNode], close: String) {
    work.push(Work::Text(close));
    for (i, node) in list.iter().enumerate().rev() {
        work.push(Work::Node(node));
        if i > 0 {
            work.push(Work::Text(",".to_owned()));
        }
    }
}

/// Written with a stack, as a tree can nest deeper than a call stack would like.
fn nodes(out: &mut String, roots: &[TreemapNode]) {
    let mut work = Vec::new();
    queue(&mut work, roots, "]".to_owned());
    out.push('[');
    while let Some(next) = work.pop() {
        let node = match next {
            Work::Text(text) => {
                out.push_str(&text);
                continue;
            }
            Work::Node(node) => node,
        };
        out.push_str("{\"name\":");
        string(out, &node.name);
        let mut tail = String::new();
        if let Some(selector) = &node.class_selector {
            tail.push_str(",\"classSelector\":");
            string(&mut tail, selector);
        }
        if let Some(styles) = &node.css_compiled_styles {
            tail.push_str(",\"cssCompiledStyles\":");
            strings(&mut tail, styles);
        }
        tail.push('}');
        match &node.content {
            Content::Leaf(value) => {
                out.push_str(&format!(",\"value\":\"{:016x}\"", value.to_bits()));
                out.push_str(&tail);
            }
            Content::Section(children) => {
                out.push_str(",\"children\":[");
                queue(&mut work, children, format!("]{tail}"));
            }
        }
    }
}

pub(crate) fn front_json(model: &TreemapModel) -> String {
    let mut out = String::from("{\"model\":");
    let mut fields = Fields::open(&mut out);
    for (name, text) in [("title", &model.title), ("accTitle", &model.acc_title), ("accDescr", &model.acc_descr)] {
        if let Some(text) = text {
            string(fields.key(name), text);
        }
    }
    nodes(fields.key("roots"), &model.roots);
    let classes = fields.key("classes");
    classes.push('[');
    for (index, (name, styles)) in model.classes.iter().enumerate() {
        if index > 0 {
            classes.push(',');
        }
        classes.push('[');
        string(classes, name);
        classes.push(',');
        strings(classes, styles);
        classes.push(']');
    }
    classes.push(']');
    fields.close();
    out.push('}');
    out
}
