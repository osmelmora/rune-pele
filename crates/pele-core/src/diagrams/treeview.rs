//! Tree views: files and folders, written indented or as box drawing. Ported from Pele's
//! `src/diagrams/treeview`.

mod box_drawing;
mod icons;
mod model;
mod parser;
mod render;

use crate::catalog::{Diagram, DiagramKind, DrawContext, ParseContext};
use crate::diagrams::railroad::probe::{maybe, string};
use crate::scene::Scene;
use crate::{Origin, ParseOptions, RenderError, tagged};
pub(crate) use model::TreeViewModel;
use model::{NodeType, read};

pub(crate) struct TreeView;

impl Diagram for TreeView {
    type Model = TreeViewModel;

    fn parse(cx: &ParseContext<'_>) -> Result<TreeViewModel, RenderError> {
        read(cx)
    }

    fn scene(model: &TreeViewModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        render::scene(model, cx)
    }
}

/// A node and what is under it, as `JSON.stringify` writes Pele's `TreeNode`. Written with a
/// stack, as a tree can nest deeper than a call stack.
fn node_json(out: &mut String, model: &TreeViewModel) {
    enum Work {
        Node(usize),
        Text(&'static str),
    }
    let mut work = vec![Work::Node(0)];
    while let Some(next) = work.pop() {
        let id = match next {
            Work::Text(text) => {
                out.push_str(text);
                continue;
            }
            Work::Node(id) => id,
        };
        let node = &model.nodes[id];
        out.push_str(&format!("{{\"id\":{},\"level\":{},\"name\":", node.id, node.level));
        string(out, &node.name);
        let kind = if node.node_type == NodeType::Directory { "directory" } else { "file" };
        out.push_str(&format!(",\"nodeType\":\"{kind}\",\"icon\":"));
        maybe(out, node.icon.as_deref());
        out.push_str(",\"cssClass\":");
        maybe(out, node.class.as_deref());
        out.push_str(",\"description\":");
        maybe(out, node.description.as_deref());
        out.push_str(",\"children\":[");
        work.push(Work::Text("]}"));
        for (index, &child) in node.children.iter().enumerate().rev() {
            work.push(Work::Node(child));
            if index > 0 {
                work.push(Work::Text(","));
            }
        }
    }
}

/// The tree view front end's model as JSON, in the words of the oracle's `treeview_front` op,
/// for the differential test. A source of another type answers only that type.
pub fn front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = crate::front(source, &options)?;
    if front.found.kind != DiagramKind::TreeView {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = read(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    let mut out = String::from("{\"type\":\"treeView\",\"title\":");
    maybe(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    maybe(&mut out, model.acc_title.as_deref());
    out.push_str(",\"accDescr\":");
    maybe(&mut out, model.acc_descr.as_deref());
    out.push_str(",\"root\":");
    node_json(&mut out, &model);
    out.push_str(&format!(",\"count\":{}}}", model.nodes.len()));
    Ok(out)
}
