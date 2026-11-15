//! Pele's `layoutTreemap` (`treemap/layout.ts`): a squarified treemap of a tree, each section's
//! children laid out inside it below its header. Every number goes through `crate::js`, in Pele's
//! order of operations, so each box lands on the same pixel.

use super::{Content, TreemapNode};
use crate::js;
use std::cmp::Ordering;

/// Space a section keeps for its header, and around its children on the other three sides.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Insets {
    pub(crate) gap: f64,
    pub(crate) header: f64,
    pub(crate) short: f64,
    pub(crate) side: f64,
}

/// Pele's `Box`: one node of the tree and where it is drawn.
#[derive(Clone, Debug)]
pub(crate) struct Tile<'a> {
    pub(crate) node: &'a TreemapNode,
    /// Where the parent sits among the tiles.
    pub(crate) parent: Option<usize>,
    /// Position among the top-level nodes of the branch this tile belongs to, which picks its colour.
    pub(crate) series: usize,
    /// A leaf's own value, or the sum of the leaves under a section.
    pub(crate) value: f64,
    pub(crate) children: Vec<usize>,
    /// Small sections give their header space to their children.
    pub(crate) headed: bool,
    pub(crate) header: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// Every tile of the tree in breadth-first order, and the ones laid out, parents before children.
pub(crate) struct Layout<'a> {
    pub(crate) tiles: Vec<Tile<'a>>,
    pub(crate) placed: Vec<usize>,
}

fn tile(node: &TreemapNode, parent: Option<usize>, series: usize) -> Tile<'_> {
    let value = match node.content {
        Content::Leaf(value) if value.is_finite() && value > 0.0 => value,
        _ => 0.0,
    };
    let (headed, header, x, y, w, h) = (false, 0.0, 0.0, 0.0, 0.0, 0.0);
    Tile { node, parent, series, value, children: Vec::new(), headed, header, x, y, w, h }
}

fn clamp(v: f64, low: f64, high: f64) -> f64 {
    if v >= low { js::min(v, high) } else { low }
}

/// Pele's `squarify`: lays out `boxes` in the rectangle as one squarified treemap. Rows are added
/// along the shorter side, and a row stops growing when another box would make its boxes less
/// square.
#[allow(clippy::too_many_arguments, reason = "the arguments of Pele's squarify")]
fn squarify(tiles: &mut [Tile<'_>], boxes: &[usize], total: f64, x: f64, y: f64, w: f64, h: f64, gap: f64) {
    let (mut x, mut y, mut w, mut h) = (x, y, w, h);
    let mut rest = total;
    let mut from = 0;
    while from < boxes.len() {
        let scale = (w * h) / rest;
        let side = js::min(w, h);
        let mut sum = tiles[boxes[from]].value * scale;
        let largest = sum;
        let mut worst = js::max((side * side * largest) / (sum * sum), (sum * sum) / (side * side * largest));
        let mut to = from + 1;
        while to < boxes.len() {
            let smallest = tiles[boxes[to]].value * scale;
            let next = sum + smallest;
            let ratio = js::max((side * side * largest) / (next * next), (next * next) / (side * side * smallest));
            if ratio > worst {
                break;
            }
            worst = ratio;
            sum = next;
            to += 1;
        }

        let thickness = sum / side;
        let mut offset = 0.0;
        for i in from..to {
            let b = &mut tiles[boxes[i]];
            let length = if i == to - 1 { side - offset } else { (b.value * scale) / thickness };
            let across = w >= h;
            let bx = if across { x } else { x + offset };
            let by = if across { y + offset } else { y };
            let bw = if across { thickness } else { length };
            let bh = if across { length } else { thickness };
            // Omit the gap if it would erase the box.
            let gx = if js::round(bx + bw - gap / 2.0) > js::round(bx + gap / 2.0) { gap / 2.0 } else { 0.0 };
            let gy = if js::round(by + bh - gap / 2.0) > js::round(by + gap / 2.0) { gap / 2.0 } else { 0.0 };
            b.x = js::round(bx + gx);
            b.y = js::round(by + gy);
            b.w = js::round(bx + bw - gx) - b.x;
            b.h = js::round(by + bh - gy) - b.y;
            offset += length;
            rest -= b.value;
        }
        if w >= h {
            x += thickness;
            w -= thickness;
        } else {
            y += thickness;
            h -= thickness;
        }
        from = to;
    }
}

/// Pele's `place`: the children with positive values, largest first, squarified into the
/// rectangle and clamped to it.
#[allow(clippy::too_many_arguments, reason = "the arguments of Pele's place, and what it adds to")]
fn place(
    tiles: &mut [Tile<'_>],
    placed: &mut Vec<usize>,
    children: &[usize],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    gap: f64,
) {
    let mut shown: Vec<usize> = children.iter().copied().filter(|&b| tiles[b].value > 0.0).collect();
    // `(a, b) => b.value - a.value`, in a stable sort as V8's is.
    shown.sort_by(|&a, &b| {
        let d = tiles[b].value - tiles[a].value;
        if d < 0.0 {
            Ordering::Less
        } else if d > 0.0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
    let mut total = 0.0;
    for &b in &shown {
        total += tiles[b].value;
    }
    if shown.is_empty() || !total.is_finite() {
        return;
    }
    let w = js::max(0.0, w);
    let h = js::max(0.0, h);
    if w > 0.0 && h > 0.0 {
        squarify(tiles, &shown, total, x - gap / 2.0, y - gap / 2.0, w + gap, h + gap, gap);
    }
    for &i in &shown {
        let b = &mut tiles[i];
        let x0 = clamp(b.x, x, x + w);
        let y0 = clamp(b.y, y, y + h);
        b.w = clamp(b.x + b.w, x0, x + w) - x0;
        b.h = clamp(b.y + b.h, y0, y + h) - y0;
        b.x = x0;
        b.y = y0;
        placed.push(i);
    }
}

/// Pele's `layoutTreemap`: the tiles with positive values, including those of no size, parents
/// before children.
pub(crate) fn layout(roots: &[TreemapNode], width: f64, height: f64, insets: Insets) -> Layout<'_> {
    let mut tiles: Vec<Tile<'_>> = roots.iter().enumerate().map(|(index, node)| tile(node, None, index)).collect();
    let top: Vec<usize> = (0..tiles.len()).collect();
    let mut i = 0;
    while i < tiles.len() {
        let (node, series) = (tiles[i].node, tiles[i].series);
        if let Content::Section(children) = &node.content {
            for child in children {
                let at = tiles.len();
                tiles.push(tile(child, Some(i), series));
                tiles[i].children.push(at);
            }
        }
        i += 1;
    }
    for i in (0..tiles.len()).rev() {
        if let Some(parent) = tiles[i].parent {
            let value = tiles[i].value;
            tiles[parent].value += value;
        }
    }

    let Insets { gap, header, short, side } = insets;
    let mut placed = Vec::new();
    place(&mut tiles, &mut placed, &top, 0.0, 0.0, width, height, gap);
    let mut i = 0;
    while i < placed.len() {
        let at = placed[i];
        i += 1;
        if tiles[at].children.is_empty() {
            continue;
        }
        let b = &tiles[at];
        let top = if b.h - header - side >= 12.0 { header } else { short };
        let headed = b.w > js::max(24.0, 2.0 * side) && b.h - top - side >= 8.0;
        let (x, y, w, h) = (b.x, b.y, b.w, b.h);
        let children = b.children.clone();
        tiles[at].headed = headed;
        tiles[at].header = if headed { top } else { 0.0 };
        if headed {
            place(&mut tiles, &mut placed, &children, x + side, y + top, w - 2.0 * side, h - top - side, gap);
        } else {
            place(&mut tiles, &mut placed, &children, x, y, w, h, gap);
        }
    }
    Layout { tiles, placed }
}
