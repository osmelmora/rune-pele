//! Pele's `placeOnGrid` (`architecture/layout.ts`).
//!
//! Mermaid hands the edge sides to a force-directed solver as relative placement constraints.
//! Pele applies them directly: every service and junction takes a cell, an edge `a:R -- L:b` puts
//! b in the cell to the right of a, and a group is a solid rectangle of cells, so a group never
//! encloses a node that is not in it. Each container (the diagram, then every group) is laid out
//! on its own, innermost first, and a finished group is placed in its parent as one block.
//!
//! Pele keeps cells in `Int32Array`s. Every coordinate here is a small integer, far inside that
//! range, so `i64` holds the same values.

use super::{Alignment, ArchitectureModel, Side};
use std::collections::HashMap;

/// Where each node and group sits on the grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Grid {
    /// The cell of each node, in the model's order.
    pub(crate) col: Vec<i64>,
    pub(crate) row: Vec<i64>,
    /// The cell rectangle and parent of each group. A group with nothing in it takes one cell.
    pub(crate) group_col: Vec<i64>,
    pub(crate) group_row: Vec<i64>,
    pub(crate) group_cols: Vec<i64>,
    pub(crate) group_rows: Vec<i64>,
    pub(crate) group_parent: Vec<Option<usize>>,
    pub(crate) group_empty: Vec<bool>,
    pub(crate) node_parent: Vec<Option<usize>>,
}

const DX: [i64; 4] = [-1, 1, 0, 0];
const DY: [i64; 4] = [0, 0, -1, 1];

// Blocks up to this many cells are found through the cell map; larger ones are compared one by one.
const SMALL: i64 = 64;
const OFFSET: f64 = 33_554_432.0;
const SPAN: f64 = 67_108_864.0;

/// The key of a cell in Pele's cell map, `(cx + OFFSET) * SPAN + cy + OFFSET`, as the bits of the
/// double it computes.
fn cell_key(cx: i64, cy: i64) -> u64 {
    ((cx as f64 + OFFSET) * SPAN + cy as f64 + OFFSET).to_bits()
}

/// One request that a node sit beside another: both ends, the sides, and the two children of
/// the innermost container that holds both ends, which the request places.
struct Link {
    a: usize,
    b: usize,
    side_a: usize,
    side_b: usize,
    item_a: usize,
    item_b: usize,
}

/// What laying out one container works with. Items are the nodes (`0..n`), then the groups.
struct Placer<'a> {
    n: usize,
    parent: &'a [usize],
    links: &'a [Link],
    edge_links: &'a [Vec<usize>],
    x: Vec<i64>,
    y: Vec<i64>,
    w: Vec<i64>,
    h: Vec<i64>,
    placed: Vec<bool>,
    order: Vec<usize>,
    cells: HashMap<u64, usize>,
    large: Vec<usize>,
    reach: HashMap<[i64; 6], i64>,
    first: usize,
}

impl Placer<'_> {
    /// The offset of `node` inside `item`, which holds it.
    fn anchor(&self, node: usize, item: usize) -> (i64, i64) {
        let (mut ax, mut ay) = (0, 0);
        let mut cur = node;
        while cur != item {
            ax += self.x[cur];
            ay += self.y[cur];
            cur = self.n + self.parent[cur];
        }
        (ax, ay)
    }

    fn put(&mut self, item: usize, px: i64, py: i64) {
        self.x[item] = px;
        self.y[item] = py;
        self.placed[item] = true;
        self.order.push(item);
        if self.w[item] * self.h[item] > SMALL {
            self.large.push(item);
            return;
        }
        for cy in py..py + self.h[item] {
            for cx in px..px + self.w[item] {
                self.cells.insert(cell_key(cx, cy), item);
            }
        }
    }

    fn overlaps(&self, item: usize, rx: i64, ry: i64, rw: i64, rh: i64) -> bool {
        self.x[item] < rx + rw
            && rx < self.x[item] + self.w[item]
            && self.y[item] < ry + rh
            && ry < self.y[item] + self.h[item]
    }

    fn collide(&self, rx: i64, ry: i64, rw: i64, rh: i64) -> Option<usize> {
        if rw * rh > SMALL {
            return self.order[self.first..].iter().copied().find(|&item| self.overlaps(item, rx, ry, rw, rh));
        }
        for cy in ry..ry + rh {
            for cx in rx..rx + rw {
                if let Some(&hit) = self.cells.get(&cell_key(cx, cy)) {
                    return Some(hit);
                }
            }
        }
        self.large.iter().copied().find(|&item| self.overlaps(item, rx, ry, rw, rh))
    }

    /// How far a block must slide along (sx, sy) to be clear. Each step jumps past what it hit,
    /// and the result is kept, because a later block starting from the same place cannot stop
    /// sooner.
    fn slide(&mut self, rx: i64, ry: i64, rw: i64, rh: i64, sx: i64, sy: i64) -> i64 {
        let key = [rx, ry, rw, rh, sx, sy];
        let mut k = self.reach.get(&key).copied().unwrap_or(0);
        while let Some(hit) = self.collide(rx + k * sx, ry + k * sy, rw, rh) {
            k = if sx > 0 {
                self.x[hit] + self.w[hit] - rx
            } else if sx < 0 {
                rx + rw - self.x[hit]
            } else if sy > 0 {
                self.y[hit] + self.h[hit] - ry
            } else {
                ry + rh - self.y[hit]
            };
        }
        self.reach.insert(key, k);
        k
    }

    fn follow(&mut self, from: usize, k: usize) {
        let link = &self.links[k];
        let forward = link.item_a == from;
        let to = if forward { link.item_b } else { link.item_a };
        if self.placed[to] {
            return;
        }
        let sp = if forward { link.side_a } else { link.side_b };
        let sq = if forward { link.side_b } else { link.side_a };
        let (pax, pay) = self.anchor(if forward { link.a } else { link.b }, from);
        let (ax, ay) = self.anchor(if forward { link.b } else { link.a }, to);
        let (mut dx, mut dy) = (DX[sp], DY[sp]);
        let (mut shift_x, mut shift_y) = (0, 0);
        let facing = DX[sp] + DX[sq] == 0 && DY[sp] + DY[sq] == 0;
        if sp == sq {
            // Like sides: the edge loops around, so the nodes stack across the direction it leaves in.
            dx = i64::from(DX[sp] == 0);
            dy = 1 - dx;
        } else if !facing {
            // A bend: one step along the side the edge leaves, one step back from the side it enters.
            shift_x = -DX[sq];
            shift_y = -DY[sq];
        }
        let (mut bx, mut by);
        if dx != 0 {
            bx = if dx > 0 { self.x[from] + self.w[from] } else { self.x[from] - self.w[to] };
            by = self.y[from] + pay + shift_y - ay;
        } else {
            by = if dy > 0 { self.y[from] + self.h[from] } else { self.y[from] - self.h[to] };
            bx = self.x[from] + pax + shift_x - ax;
        }
        if facing {
            // When several placed nodes reach this one through the same pair of sides, line it up
            // with the middle one, not with whichever happened to be visited first.
            let list = &self.edge_links[to];
            if list.len() > 1 {
                let mut wanted = Vec::new();
                for &j in list {
                    let link = &self.links[j];
                    let to_is_b = link.item_b == to;
                    let other = if to_is_b { link.item_a } else { link.item_b };
                    let (side_other, side_to) =
                        if to_is_b { (link.side_a, link.side_b) } else { (link.side_b, link.side_a) };
                    if !self.placed[other] || side_other != sp || side_to != sq {
                        continue;
                    }
                    let (oax, oay) = self.anchor(if to_is_b { link.a } else { link.b }, other);
                    let at = if dx != 0 { self.y[other] + oay } else { self.x[other] + oax };
                    let (tax, tay) = self.anchor(if to_is_b { link.b } else { link.a }, to);
                    wanted.push(at - if dx != 0 { tay } else { tax });
                }
                if wanted.len() > 1 {
                    wanted.sort_unstable();
                    let middle = wanted[(wanted.len() - 1) >> 1];
                    if dx != 0 {
                        by = middle;
                    } else {
                        bx = middle;
                    }
                }
            }
            // Taken: move sideways, to whichever side is nearer, so the edge can still leave straight.
            let px = i64::from(dx == 0);
            let py = 1 - px;
            let (wt, ht) = (self.w[to], self.h[to]);
            let forth = self.slide(bx, by, wt, ht, px, py);
            let back = if forth == 0 { 0 } else { self.slide(bx, by, wt, ht, -px, -py) };
            let near = if forth <= back { forth } else { -back };
            bx += px * near;
            by += py * near;
        } else {
            let far = self.slide(bx, by, self.w[to], self.h[to], dx, dy);
            bx += dx * far;
            by += dy * far;
        }
        self.put(to, bx, by);
    }

    /// Lays out the children of one container, and sizes it when it is a group.
    fn layout(&mut self, items: &[usize], hint_links: &[Vec<usize>], group: Option<usize>) {
        if items.is_empty() {
            return;
        }
        self.order.clear();
        let mut starts = Vec::new();
        for &start in items {
            if self.placed[start] {
                continue;
            }
            self.first = self.order.len();
            starts.push(self.first);
            self.cells.clear();
            self.large.clear();
            self.reach.clear();
            self.put(start, 0, 0);
            // Declared alignments are followed as soon as a node is placed, ahead of any edge.
            let mut hint_at = self.first;
            let mut edge_at = self.first;
            let mut link_at = 0;
            loop {
                if hint_at < self.order.len() {
                    let from = self.order[hint_at];
                    hint_at += 1;
                    for &k in &hint_links[from] {
                        self.follow(from, k);
                    }
                    continue;
                }
                if edge_at >= self.order.len() {
                    break;
                }
                let from = self.order[edge_at];
                let Some(&k) = self.edge_links[from].get(link_at) else {
                    edge_at += 1;
                    link_at = 0;
                    continue;
                };
                link_at += 1;
                self.follow(from, k);
            }
        }
        starts.push(self.order.len());

        // Unconnected parts go side by side, wrapping into rows that keep the whole roughly square.
        let count = starts.len() - 1;
        let mut bounds = Vec::with_capacity(count);
        let mut area = 0.0;
        let mut widest = 0;
        for p in 0..count {
            let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
            for &item in &self.order[starts[p]..starts[p + 1]] {
                x0 = x0.min(self.x[item]);
                y0 = y0.min(self.y[item]);
                x1 = x1.max(self.x[item] + self.w[item]);
                y1 = y1.max(self.y[item] + self.h[item]);
            }
            let (part_w, part_h) = (x1 - x0, y1 - y0);
            bounds.push((x0, y0, part_w, part_h));
            area += (part_w * part_h) as f64;
            widest = widest.max(part_w);
        }
        let limit = widest.max((area * 2.0).sqrt().ceil() as i64);
        let (mut cx, mut cy, mut shelf, mut width) = (0, 0, 0, 0);
        for (p, &(min_x, min_y, part_w, part_h)) in bounds.iter().enumerate() {
            if cx > 0 && cx + part_w > limit {
                cy += shelf;
                cx = 0;
                shelf = 0;
            }
            for &item in &self.order[starts[p]..starts[p + 1]] {
                self.x[item] += cx - min_x;
                self.y[item] += cy - min_y;
            }
            cx += part_w;
            shelf = shelf.max(part_h);
            width = width.max(cx);
        }
        if let Some(group) = group {
            self.w[self.n + group] = width;
            self.h[self.n + group] = cy + shelf;
        }
    }
}

fn side_index(side: Side) -> usize {
    side.index()
}

/// Pele's `placeOnGrid`.
pub(crate) fn place_on_grid(model: &ArchitectureModel) -> Grid {
    let n = model.nodes.len();
    let g_count = model.groups.len();
    let root = g_count;
    // Items are nodes (0..n) then groups (n..n+g). Containers are groups (0..g) then the root.
    let mut parent = vec![root; n + g_count];
    let mut depth = vec![0usize; g_count + 1];
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); g_count + 1];
    for (g, group) in model.groups.iter().enumerate() {
        match group.parent {
            Some(p) => {
                parent[n + g] = p;
                depth[g] = depth[p] + 1;
            }
            None => depth[g] = 1,
        }
    }
    for (i, node) in model.nodes.iter().enumerate() {
        if let Some(p) = node.parent {
            parent[i] = p;
        }
        kids[parent[i]].push(i);
    }
    for g in 0..g_count {
        kids[parent[n + g]].push(n + g);
    }

    // A link asks for one node to sit beside another. It belongs to the innermost container that
    // holds both ends, and there it relates the two children of that container the ends are in.
    let mut links = Vec::new();
    let mut hint_links = vec![Vec::new(); n + g_count];
    let mut edge_links = vec![Vec::new(); n + g_count];
    let mut add_link = |a: usize, b: usize, side_a: usize, side_b: usize, lists: &mut Vec<Vec<usize>>| {
        if a == b {
            return;
        }
        let (mut item_a, mut item_b) = (a, b);
        let (mut ca, mut cb) = (parent[a], parent[b]);
        while ca != cb {
            if depth[ca] >= depth[cb] {
                item_a = n + ca;
                ca = parent[item_a];
            } else {
                item_b = n + cb;
                cb = parent[item_b];
            }
        }
        let k = links.len();
        links.push(Link { a, b, side_a, side_b, item_a, item_b });
        lists[item_a].push(k);
        lists[item_b].push(k);
    };
    for hint in &model.hints {
        let row = hint.direction == Alignment::Row;
        for pair in hint.members.windows(2) {
            let (side_a, side_b) = if row { (1, 0) } else { (3, 2) };
            add_link(pair[0], pair[1], side_a, side_b, &mut hint_links);
        }
    }
    // An edge between two like sides says least about where its ends go, so those come last.
    for like_sides in [false, true] {
        for edge in &model.edges {
            if (edge.lhs.side == edge.rhs.side) != like_sides {
                continue;
            }
            let (side_a, side_b) = (side_index(edge.lhs.side), side_index(edge.rhs.side));
            add_link(edge.lhs.node, edge.rhs.node, side_a, side_b, &mut edge_links);
        }
    }

    let mut placer = Placer {
        n,
        parent: &parent,
        links: &links,
        edge_links: &edge_links,
        x: vec![0; n + g_count],
        y: vec![0; n + g_count],
        w: vec![1; n + g_count],
        h: vec![1; n + g_count],
        placed: vec![false; n + g_count],
        order: Vec::new(),
        cells: HashMap::new(),
        large: Vec::new(),
        reach: HashMap::new(),
        first: 0,
    };
    for c in (0..g_count).rev() {
        placer.layout(&kids[c], &hint_links, Some(c));
    }
    placer.layout(&kids[root], &hint_links, None);

    let mut grid = Grid {
        col: vec![0; n],
        row: vec![0; n],
        group_col: vec![0; g_count],
        group_row: vec![0; g_count],
        group_cols: placer.w[n..].to_vec(),
        group_rows: placer.h[n..].to_vec(),
        group_parent: vec![None; g_count],
        group_empty: vec![false; g_count],
        node_parent: vec![None; n],
    };
    for g in 0..g_count {
        let p = parent[n + g];
        let (base_col, base_row) = if p == root { (0, 0) } else { (grid.group_col[p], grid.group_row[p]) };
        grid.group_parent[g] = (p != root).then_some(p);
        grid.group_col[g] = placer.x[n + g] + base_col;
        grid.group_row[g] = placer.y[n + g] + base_row;
        grid.group_empty[g] = kids[g].is_empty();
    }
    for i in 0..n {
        let p = parent[i];
        let (base_col, base_row) = if p == root { (0, 0) } else { (grid.group_col[p], grid.group_row[p]) };
        grid.node_parent[i] = (p != root).then_some(p);
        grid.col[i] = placer.x[i] + base_col;
        grid.row[i] = placer.y[i] + base_row;
    }
    grid
}
