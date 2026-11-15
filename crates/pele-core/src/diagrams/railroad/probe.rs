//! The entry point that lets `tests/railroad_front.rs` hold each notation's front end to Pele
//! before anything is drawn: the rule model as the oracle's `railroad_front` op prints Pele's.

use super::{Grammar, Node, NodeId, RailroadModel};
use crate::catalog::DiagramKind;
use crate::{Origin, ParseOptions, RenderError, js, tagged};

/// `JSON.stringify` of a string.
pub(crate) fn string(out: &mut String, text: &[u16]) {
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

/// `JSON.stringify` of a string or `undefined`, which the op writes as `null`.
pub(crate) fn maybe(out: &mut String, text: Option<&[u16]>) {
    match text {
        Some(text) => string(out, text),
        None => out.push_str("null"),
    }
}

/// What remains to be written of a definition: a node, or punctuation between nodes.
enum Work {
    Node(NodeId),
    Text(String),
}

/// A definition as `JSON.stringify` writes Pele's `RailroadNode`, with each number as
/// `String(number)`. Written with a stack, as a definition can nest deeper than a call stack.
fn definition(out: &mut String, grammar: &Grammar, root: NodeId) {
    let mut work = vec![Work::Node(root)];
    while let Some(next) = work.pop() {
        let id = match next {
            Work::Text(text) => {
                out.push_str(&text);
                continue;
            }
            Work::Node(id) => id,
        };
        let mut list = |open: &str, parts: &[NodeId], work: &mut Vec<Work>| {
            out.push_str(open);
            work.push(Work::Text("]}".to_owned()));
            for (index, &part) in parts.iter().enumerate().rev() {
                work.push(Work::Node(part));
                if index > 0 {
                    work.push(Work::Text(",".to_owned()));
                }
            }
        };
        match grammar.node(id) {
            Node::Terminal(value) => {
                out.push_str("{\"type\":\"terminal\",\"value\":");
                string(out, value);
                out.push('}');
            }
            Node::NonTerminal(name) => {
                out.push_str("{\"type\":\"nonterminal\",\"name\":");
                string(out, name);
                out.push('}');
            }
            Node::Special(text) => {
                out.push_str("{\"type\":\"special\",\"text\":");
                string(out, text);
                out.push('}');
            }
            Node::Sequence(parts) => list("{\"type\":\"sequence\",\"elements\":[", parts, &mut work),
            Node::Choice(parts) => list("{\"type\":\"choice\",\"alternatives\":[", parts, &mut work),
            Node::Optional(element) => {
                out.push_str("{\"type\":\"optional\",\"element\":");
                work.push(Work::Text("}".to_owned()));
                work.push(Work::Node(*element));
            }
            Node::Repetition { element, min, max } => {
                out.push_str("{\"type\":\"repetition\",\"element\":");
                let tail = format!(",\"min\":\"{}\",\"max\":\"{}\"}}", js::to_string(*min), js::to_string(*max));
                work.push(Work::Text(tail));
                work.push(Work::Node(*element));
            }
        }
    }
}

fn model_json(model: &RailroadModel, mermaid: &str) -> String {
    let mut out = String::new();
    out.push_str("{\"type\":\"railroad\",\"mermaidType\":\"");
    out.push_str(mermaid);
    out.push_str("\",\"notation\":\"");
    out.push_str(model.notation.name());
    out.push_str("\",\"title\":");
    maybe(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    maybe(&mut out, model.accessible.title.as_deref());
    out.push_str(",\"accDescr\":");
    maybe(&mut out, model.accessible.description.as_deref());
    out.push_str(",\"rules\":[");
    for (index, rule) in model.grammar.rules.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        string(&mut out, &rule.name);
        out.push_str(",\"definition\":");
        definition(&mut out, &model.grammar, rule.definition);
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// The railroad front end's model as JSON, in the words of the oracle's `railroad_front` op. A
/// source of another type answers only that type.
pub fn front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = crate::front(source, &options)?;
    if front.found.kind != DiagramKind::Railroad {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = super::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(model_json(&model, front.found.mermaid.name()))
}
