//! Entry points that let `tests/treemap_layout.rs` hold the squarified layout and the value format
//! to Pele on their own, before anything is drawn. They answer in plain values, so the port's
//! types stay private.

use super::format::Formatter;
use super::layout::{Insets, layout as squarified};
use super::{Content, TreemapNode};

/// A node of a tree to lay out: a section and its rows, or a leaf and its value.
#[derive(Clone, Debug)]
pub enum Node {
    Section { name: String, children: Vec<Node> },
    Leaf { name: String, value: f64 },
}

/// A box as Pele's `layoutTreemap` returns it. `parent` is the place of the parent's box in the
/// list, or -1 for a box at the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub name: String,
    pub parent: i64,
    pub series: usize,
    pub value: f64,
    pub headed: bool,
    pub header: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn tree(node: &Node) -> TreemapNode {
    let (name, content) = match node {
        Node::Section { name, children } => (name, Content::Section(children.iter().map(tree).collect())),
        Node::Leaf { name, value } => (name, Content::Leaf(*value)),
    };
    TreemapNode { name: name.as_str().into(), content, class_selector: None, css_compiled_styles: None }
}

/// Pele's `layoutTreemap(roots, width, height, { gap, header, short, side })`.
pub fn layout(roots: &[Node], width: f64, height: f64, insets: [f64; 4]) -> Vec<Placed> {
    let roots: Vec<TreemapNode> = roots.iter().map(tree).collect();
    let [gap, header, short, side] = insets;
    let laid = squarified(&roots, width, height, Insets { gap, header, short, side });
    let mut place_of = vec![-2; laid.tiles.len()];
    for (place, &at) in laid.placed.iter().enumerate() {
        place_of[at] = place as i64;
    }
    let place = |tile: usize| place_of[tile];
    laid.placed
        .iter()
        .map(|&at| {
            let b = &laid.tiles[at];
            Placed {
                name: b.node.name.to_display(),
                parent: b.parent.map_or(-1, place),
                series: b.series,
                value: b.value,
                headed: b.headed,
                header: b.header,
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
            }
        })
        .collect()
}

/// Pele's `formatter(spec)(value)`.
pub fn format(spec: &str, value: f64) -> String {
    Formatter::new(&spec.encode_utf16().collect::<Vec<_>>()).format(value)
}
