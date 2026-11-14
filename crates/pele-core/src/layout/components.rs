//! Moves disconnected parts of a layout into rows, ported from Pele's `layout/components.ts`.

use super::compound::{CEdge, CNode, Size, shift_edge};
use crate::js;
use indexmap::IndexMap;

struct Part {
    nodes: Vec<usize>,
    edges: Vec<usize>,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Part {
    fn include(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.left = js::min(self.left, x - w / 2.0);
        self.top = js::min(self.top, y - h / 2.0);
        self.right = js::max(self.right, x + w / 2.0);
        self.bottom = js::max(self.bottom, y + h / 2.0);
    }
}

/// Moves disconnected parts into rows no wider than `width`, `gap` apart, without changing the
/// layout inside a part. Returns the new size, or none if the layout is one part or rows would
/// not make it narrower, and then nothing has moved.
pub fn wrap_components(nodes: &mut [CNode], edges: &mut [CEdge], width: f64, gap: f64) -> Option<Size> {
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut join = |a: usize, b: usize| {
        let from = root(&mut parent, a);
        let to = root(&mut parent, b);
        parent[from] = to;
    };
    for (i, node) in nodes.iter().enumerate() {
        if let Some(group) = node.parent {
            join(i, group);
        }
    }
    for e in edges.iter() {
        join(e.src, e.dst);
    }

    let mut parts: IndexMap<usize, Part> = IndexMap::new();
    for (i, c) in nodes.iter().enumerate() {
        let part = parts.entry(root(&mut parent, i)).or_insert_with(|| Part {
            nodes: Vec::new(),
            edges: Vec::new(),
            left: f64::INFINITY,
            top: f64::INFINITY,
            right: f64::NEG_INFINITY,
            bottom: f64::NEG_INFINITY,
        });
        part.nodes.push(i);
        part.include(c.x, c.y, c.w, c.h);
    }
    if parts.len() < 2 {
        return None;
    }
    for (ei, e) in edges.iter().enumerate() {
        let part = parts.get_mut(&root(&mut parent, e.src)).expect("every node has a part");
        part.edges.push(ei);
        for point in &e.route {
            part.include(point.x, point.y, 0.0, 0.0);
        }
        if e.label_w > 0.0 {
            part.include(e.label_x, e.label_y, e.label_w, e.label_h);
        }
    }

    let (mut x, mut y, mut row_height, mut right) = (0.0, 0.0, 0.0, 0.0);
    let (mut from, mut to) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut shifts: Vec<[f64; 2]> = Vec::new();
    for part in parts.values() {
        let w = part.right - part.left;
        let h = part.bottom - part.top;
        if x > 0.0 && x + w > width {
            y += row_height + gap;
            x = 0.0;
            row_height = 0.0;
        }
        shifts.push([x - part.left, y - part.top]);
        from = js::min(from, part.left);
        to = js::max(to, part.right);
        right = js::max(right, x + w);
        row_height = js::max(row_height, h);
        x += w + gap;
    }
    if right >= to - from {
        return None;
    }
    for (part, [ox, oy]) in parts.values().zip(shifts) {
        for &i in &part.nodes {
            nodes[i].x += ox;
            nodes[i].y += oy;
        }
        for &ei in &part.edges {
            shift_edge(&mut edges[ei], ox, oy);
        }
    }
    Some(Size { width: right, height: y + row_height })
}
