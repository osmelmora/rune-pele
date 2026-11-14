//! Layout of a graph whose nodes may be nested in groups, ported from Pele's
//! `layout/compound.ts`. Each group is laid out on its own, in its own direction, and then
//! placed in its parent as a single box. An edge that crosses a group's border passes through a
//! port on the side facing its other end.
//!
//! As in `layered`, every sum, sort and loop keeps Pele's order, and names follow Pele's.

use super::layered::{
    EdgeStyle, Groups, Kind, LEdge, LNode, LayeredOptions, LayeredResult, ease, layered, orient, rank,
};
use super::num::{by_difference, hypot};
use crate::js;
use std::collections::HashMap;

/// The way ranks run: top to bottom, bottom to top, left to right or right to left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    TB,
    BT,
    LR,
    RL,
}

impl Dir {
    /// The direction a diagram names, or top to bottom for anything else.
    pub fn named(name: Option<&str>) -> Dir {
        match name {
            Some("BT") => Dir::BT,
            Some("LR") => Dir::LR,
            Some("RL") => Dir::RL,
            _ => Dir::TB,
        }
    }

    fn vertical(self) -> bool {
        matches!(self, Dir::TB | Dir::BT)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CNode {
    pub w: f64,
    pub h: f64,
    /// The group this node sits in. The layout clears one that is not a group or holds itself.
    pub parent: Option<usize>,
    pub is_group: bool,
    pub dir: Option<Dir>,
    pub pad_x: f64,
    pub pad_top: f64,
    pub pad_bottom: f64,
    pub min_w: f64,
    /// Declaration order. A group takes the smallest value among its members.
    pub seq: f64,
    /// How much of a side its edges may spread along, measured across the flow. 0 keeps them
    /// at its middle.
    pub span: f64,
    /// The centre, once laid out.
    pub x: f64,
    pub y: f64,
}

impl CNode {
    pub fn new(w: f64, h: f64) -> Self {
        CNode {
            w,
            h,
            parent: None,
            is_group: false,
            dir: None,
            pad_x: 0.0,
            pad_top: 0.0,
            pad_bottom: 0.0,
            min_w: 0.0,
            seq: 0.0,
            span: 0.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

/// The way an edge runs into a point of its route. Pele writes 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutePoint {
    pub x: f64,
    pub y: f64,
    pub axis: Axis,
}

impl RoutePoint {
    /// The coordinate that changes along `axis`.
    fn along(&self, axis: Axis) -> f64 {
        match axis {
            Axis::Vertical => self.x,
            Axis::Horizontal => self.y,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CEdge {
    pub src: usize,
    pub dst: usize,
    pub minlen: f64,
    pub label_w: f64,
    pub label_h: f64,
    pub route: Vec<RoutePoint>,
    pub label_x: f64,
    pub label_y: f64,
}

impl CEdge {
    pub fn new(src: usize, dst: usize) -> Self {
        CEdge { src, dst, minlen: 1.0, label_w: 0.0, label_h: 0.0, route: Vec::new(), label_x: 0.0, label_y: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

struct PortEdge {
    e: usize,
    inner: usize,
    out: bool,
    side: Side,
    /// Cross-flow position outside the group from a previous layout, or NaN if unknown.
    hint: f64,
    px: f64,
    py: f64,
    route: Vec<RoutePoint>,
}

struct LevelEdge {
    e: usize,
    a: usize,
    b: usize,
}

struct Level {
    /// The group this level lays out, or none for the root.
    id: Option<usize>,
    dir: Dir,
    items: Vec<usize>,
    edges: Vec<LevelEdge>,
    ports: Vec<PortEdge>,
    /// The port of an edge's end here, by `port_key`.
    port_of: HashMap<usize, usize>,
    routes: HashMap<usize, Vec<RoutePoint>>,
    x: f64,
    y: f64,
}

impl Level {
    fn new(id: Option<usize>, dir: Dir) -> Self {
        Level {
            id,
            dir,
            items: Vec::new(),
            edges: Vec::new(),
            ports: Vec::new(),
            port_of: HashMap::new(),
            routes: HashMap::new(),
            x: 0.0,
            y: 0.0,
        }
    }

    fn port(&self, e: usize, out: bool) -> &PortEdge {
        &self.ports[self.port_of[&port_key(e, out)]]
    }
}

fn port_key(e: usize, out: bool) -> usize {
    e * 2 + usize::from(!out)
}

const MAX_PORTS: usize = 40000;
const MAX_ORDERED_PORTS: usize = 400;
const MAX_ORDERED_EDGES: usize = 500;
const FLAT_SLACK: f64 = 1.5;
const FLAT_SPARE: f64 = 4.0;

struct Snapshot {
    nodes: Vec<[f64; 4]>,
    routes: Vec<Vec<RoutePoint>>,
    labels: Vec<[f64; 2]>,
    levels: Vec<f64>,
    total: Size,
}

const FAN: f64 = 24.0;
const IN_LINE: f64 = 0.5;
const CURVE_PIECES: usize = 3;
const CORNER_PIECES: usize = 2;
const STEP_PIECES: usize = 3;
const HANDLE: f64 = 0.62;
const MAX_INSERT: usize = 128;

/// The segments that stand in for the drawn edges while their crossings are counted.
struct Segments {
    ax: Vec<f64>,
    ay: Vec<f64>,
    bx: Vec<f64>,
    by: Vec<f64>,
    low: Vec<f64>,
    high: Vec<f64>,
    owner: Vec<usize>,
    /// Endpoint flags: 1 for the first segment, 2 for the last.
    ends: Vec<u8>,
    count: usize,
    top: f64,
    bottom: f64,
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
}

impl Segments {
    fn new(size: usize) -> Self {
        Segments {
            ax: vec![0.0; size],
            ay: vec![0.0; size],
            bx: vec![0.0; size],
            by: vec![0.0; size],
            low: vec![0.0; size],
            high: vec![0.0; size],
            owner: vec![0; size],
            ends: vec![0; size],
            count: 0,
            top: f64::INFINITY,
            bottom: f64::NEG_INFINITY,
            from_x: f64::NAN,
            from_y: f64::NAN,
            to_x: f64::NAN,
            to_y: f64::NAN,
        }
    }

    fn add(&mut self, ei: usize, part: u8, mut x0: f64, mut y0: f64, x1: f64, y1: f64) {
        let on =
            x0 == self.to_x && y0 == self.to_y && ((y1 == y0 && self.from_y == y0) || (x1 == x0 && self.from_x == x0));
        if on && self.count > 0 {
            let p = self.count - 1;
            if self.owner[p] == ei
                && self.ends[p] == part
                && (x0 - self.from_x) * (x1 - x0) + (y0 - self.from_y) * (y1 - y0) > 0.0
            {
                self.count -= 1;
                x0 = self.from_x;
                y0 = self.from_y;
            }
        }
        self.from_x = x0;
        self.from_y = y0;
        self.to_x = x1;
        self.to_y = y1;
        let count = self.count;
        self.ends[count] = part;
        self.ax[count] = js::min(x0, x1);
        self.bx[count] = js::max(x0, x1);
        self.ay[count] = if x0 <= x1 { y0 } else { y1 };
        self.by[count] = if x0 <= x1 { y1 } else { y0 };
        self.low[count] = js::min(y0, y1);
        self.high[count] = js::max(y0, y1);
        if self.low[count] < self.top {
            self.top = self.low[count];
        }
        if self.high[count] > self.bottom {
            self.bottom = self.high[count];
        }
        self.owner[count] = ei;
        self.count += 1;
    }
}

/// How often the routes cross as drawn, leaving out where edges that meet at one node fan out.
fn route_crossings(nodes: &[CNode], edges: &[CEdge], style: EdgeStyle) -> u64 {
    let mut size = 0;
    for e in edges {
        if e.src != e.dst && e.route.len() > 1 {
            size += (e.route.len() - 1) * STEP_PIECES;
        }
    }
    let mut segs = Segments::new(size);
    for (ei, e) in edges.iter().enumerate() {
        if e.src == e.dst {
            continue;
        }
        let r = &e.route;
        let aligned = |k: usize| (r[k].x - r[k - 1].x).abs() < 0.01 || (r[k].y - r[k - 1].y).abs() < 0.01;
        let length = |k: usize| (r[k].x - r[k - 1].x).abs() + (r[k].y - r[k - 1].y).abs();
        let turns = |k: usize| {
            style == EdgeStyle::Curved
                && k > 0
                && k + 1 < r.len()
                && aligned(k)
                && aligned(k + 1)
                && length(k) >= 0.01
                && length(k + 1) >= 0.01
                && ((r[k].x - r[k - 1].x).abs() < 0.01) != ((r[k + 1].x - r[k].x).abs() < 0.01)
        };
        if r.is_empty() {
            continue;
        }
        let mut at_x = r[0].x;
        let mut at_y = r[0].y;
        for k in 1..r.len() {
            let part = u8::from(k == 1) | if k == r.len() - 1 { 2 } else { 0 };
            let x0 = at_x;
            let y0 = at_y;
            let x1 = r[k].x;
            let y1 = r[k].y;
            at_x = x1;
            at_y = y1;
            if aligned(k) || style == EdgeStyle::Linear {
                if !turns(k) {
                    segs.add(ei, part, x0, y0, x1, y1);
                    continue;
                }
                let before = if turns(k - 1) { length(k) / 2.0 } else { length(k) };
                let after = if turns(k + 1) { length(k + 1) / 2.0 } else { length(k + 1) };
                let ux = (x1 - r[k - 1].x) / length(k);
                let uy = (y1 - r[k - 1].y) / length(k);
                let vx = (r[k + 1].x - x1) / length(k + 1);
                let vy = (r[k + 1].y - y1) / length(k + 1);
                let sx = x1 - ux * before;
                let sy = y1 - uy * before;
                segs.add(ei, part, x0, y0, sx, sy);
                let c1x = x1 - ux * before * (1.0 - HANDLE);
                let c1y = y1 - uy * before * (1.0 - HANDLE);
                let c2x = x1 + vx * after * (1.0 - HANDLE);
                let c2y = y1 + vy * after * (1.0 - HANDLE);
                at_x = x1 + vx * after;
                at_y = y1 + vy * after;
                let turn = part | if k + 1 == r.len() - 1 { 2 } else { 0 };
                let mut px = sx;
                let mut py = sy;
                for i in 1..=CORNER_PIECES {
                    let t = i as f64 / CORNER_PIECES as f64;
                    let u = 1.0 - t;
                    let x = u * u * u * sx + 3.0 * u * u * t * c1x + 3.0 * u * t * t * c2x + t * t * t * at_x;
                    let y = u * u * u * sy + 3.0 * u * u * t * c1y + 3.0 * u * t * t * c2y + t * t * t * at_y;
                    segs.add(ei, turn, px, py, x, y);
                    px = x;
                    py = y;
                }
                continue;
            }
            let down = r[k].axis == Axis::Vertical;
            if style == EdgeStyle::Stepped {
                let mx = if down { x0 } else { (x0 + x1) / 2.0 };
                let my = if down { (y0 + y1) / 2.0 } else { y0 };
                segs.add(ei, part, x0, y0, mx, my);
                segs.add(ei, part, mx, my, if down { x1 } else { mx }, if down { my } else { y1 });
                segs.add(ei, part, if down { x1 } else { mx }, if down { my } else { y1 }, x1, y1);
                continue;
            }
            let c = if down { ease(x1 - x0, y1 - y0) } else { ease(y1 - y0, x1 - x0) };
            let mut px = x0;
            let mut py = y0;
            for i in 1..=CURVE_PIECES {
                let t = i as f64 / CURVE_PIECES as f64;
                let u = 1.0 - t;
                let across = t * t * (3.0 - 2.0 * t);
                let along = 3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t;
                let x = x0 + (x1 - x0) * if down { across } else { along };
                let y = y0 + (y1 - y0) * if down { along } else { across };
                segs.add(ei, part, px, py, x, y);
                px = x;
                py = y;
            }
        }
    }
    let Segments { ax, ay, bx, by, low, high, owner, ends, count, top, bottom, .. } = segs;

    let bands = if bottom > top { js::round((count as f64).sqrt()) as usize } else { 1 };
    let scale = if bottom > top { bands as f64 / (bottom - top) } else { 0.0 };
    let band_of = |y: f64| js::min((bands - 1) as f64, ((y - top) * scale).floor()) as usize;
    let mut first = vec![0usize; count];
    let mut band_start = vec![0usize; bands + 1];
    for p in 0..count {
        first[p] = band_of(low[p]);
        for b in first[p]..=band_of(high[p]) {
            band_start[b + 1] += 1;
        }
    }
    for b in 0..bands {
        band_start[b + 1] += band_start[b];
    }
    let mut fill = band_start[..bands].to_vec();
    let mut in_band = vec![0usize; band_start[bands]];
    for p in 0..count {
        for b in first[p]..=band_of(high[p]) {
            in_band[fill[b]] = p;
            fill[b] += 1;
        }
    }
    let mut runs = vec![0.0; edges.len() * 8];
    let in_line = |r: &[RoutePoint], a: usize, b: usize, c: usize| {
        ((r[a].x - r[b].x).abs() < IN_LINE && (r[a].x - r[c].x).abs() < IN_LINE)
            || ((r[a].y - r[b].y).abs() < IN_LINE && (r[a].y - r[c].y).abs() < IN_LINE)
    };
    for (ei, e) in edges.iter().enumerate() {
        let r = &e.route;
        if r.len() < 2 {
            continue;
        }
        let mut k = 1;
        while k + 1 < r.len() && in_line(r, 0, k, k + 1) {
            k += 1;
        }
        let last = r.len() - 1;
        let mut j = last - 1;
        while j >= 1 && in_line(r, last, j, j - 1) {
            j -= 1;
        }
        runs[ei * 8..ei * 8 + 8]
            .copy_from_slice(&[r[0].x, r[0].y, r[k].x, r[k].y, r[j].x, r[j].y, r[last].x, r[last].y]);
    }
    let on_run = |ei: usize, end: u8, x: f64, y: f64| -> bool {
        let o = ei * 8 + if end == 1 { 0 } else { 4 };
        let x0 = runs[o];
        let y0 = runs[o + 1];
        let dx = runs[o + 2] - x0;
        let dy = runs[o + 3] - y0;
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 { js::max(0.0, js::min(1.0, ((x - x0) * dx + (y - y0) * dy) / length)) } else { 0.0 };
        hypot(x - x0 - t * dx, y - y0 - t * dy) < IN_LINE
    };
    let mut total = 0;
    for band in 0..bands {
        let band_end = band_start[band + 1];
        if band_end - band_start[band] > MAX_INSERT {
            in_band[band_start[band]..band_end].sort_by(|&p, &q| by_difference(ax[p] - ax[q]).then(p.cmp(&q)));
        } else {
            for i in band_start[band] + 1..band_end {
                let p = in_band[i];
                let mut j = i;
                while j > band_start[band] && ax[in_band[j - 1]] > ax[p] {
                    in_band[j] = in_band[j - 1];
                    j -= 1;
                }
                in_band[j] = p;
            }
        }
        for s in band_start[band]..band_end {
            let p = in_band[s];
            let dx = bx[p] - ax[p];
            let dy = by[p] - ay[p];
            let here = first[p] == band;
            for t in s + 1..band_end {
                let q = in_band[t];
                if ax[q] > bx[p] {
                    break;
                }
                if high[q] < low[p] || low[q] > high[p] || owner[p] == owner[q] || (!here && first[q] != band) {
                    continue;
                }
                let d1 = dx * (ay[q] - ay[p]) - dy * (ax[q] - ax[p]);
                let d2 = dx * (by[q] - ay[p]) - dy * (bx[q] - ax[p]);
                if (d1 > 0.0) == (d2 > 0.0) {
                    continue;
                }
                let ex = bx[q] - ax[q];
                let ey = by[q] - ay[q];
                let d3 = ex * (ay[p] - ay[q]) - ey * (ax[p] - ax[q]);
                let d4 = ex * (by[p] - ay[q]) - ey * (bx[p] - ax[q]);
                if (d3 > 0.0) == (d4 > 0.0) {
                    continue;
                }
                let ep = &edges[owner[p]];
                let eq = &edges[owner[q]];
                let at = d3 / (d3 - d4);
                let x = ax[p] + at * dx;
                let y = ay[p] + at * dy;
                let fan = |v: usize, pe: u8, qe: u8| -> bool {
                    ((ends[p] & pe) != 0 && (ends[q] & qe) != 0)
                        || ((x - nodes[v].x).abs() < nodes[v].w / 2.0 + FAN
                            && (y - nodes[v].y).abs() < nodes[v].h / 2.0 + FAN)
                        || (on_run(owner[p], pe, x, y) && on_run(owner[q], qe, x, y))
                };
                if ep.src == eq.src && fan(ep.src, 1, 1) {
                    continue;
                }
                if ep.src == eq.dst && fan(ep.src, 1, 2) {
                    continue;
                }
                if ep.dst == eq.src && fan(ep.dst, 2, 1) {
                    continue;
                }
                if ep.dst == eq.dst && fan(ep.dst, 2, 2) {
                    continue;
                }
                total += 1;
            }
        }
    }
    total
}

/// Match Mermaid: groups crossed by edges inherit their parent's direction. `ends` holds each
/// edge's two nodes, with none for an end that names no node.
pub fn inherit_crossed_directions(nodes: &mut [CNode], ends: impl IntoIterator<Item = (Option<usize>, Option<usize>)>) {
    fn ancestors(nodes: &[CNode], i: usize) -> Vec<usize> {
        let mut set = Vec::new();
        let mut p = nodes[i].parent;
        while let Some(at) = p
            && !set.contains(&at)
        {
            set.push(at);
            p = nodes[at].parent;
        }
        set
    }
    for (src, dst) in ends {
        let (Some(src), Some(dst)) = (src, dst) else { continue };
        if src == dst {
            continue;
        }
        let a = ancestors(nodes, src);
        let b = ancestors(nodes, dst);
        for &g in &a {
            if !b.contains(&g) {
                nodes[g].dir = None;
            }
        }
        for &g in &b {
            if !a.contains(&g) {
                nodes[g].dir = None;
            }
        }
    }
}

pub fn shift_layout(nodes: &mut [CNode], edges: &mut [CEdge], ox: f64, oy: f64) {
    for c in nodes {
        c.x += ox;
        c.y += oy;
    }
    for e in edges {
        shift_edge(e, ox, oy);
    }
}

pub(super) fn shift_edge(e: &mut CEdge, ox: f64, oy: f64) {
    for point in &mut e.route {
        point.x += ox;
        point.y += oy;
    }
    e.label_x += ox;
    e.label_y += oy;
}

fn start_side(dir: Dir) -> Side {
    match dir {
        Dir::TB => Side::Top,
        Dir::BT => Side::Bottom,
        Dir::LR => Side::Left,
        Dir::RL => Side::Right,
    }
}

fn opposite(side: Side) -> Side {
    match side {
        Side::Top => Side::Bottom,
        Side::Right => Side::Left,
        Side::Bottom => Side::Top,
        Side::Left => Side::Right,
    }
}

/// Whether the end of `e` on the `out` side lies inside `item` and not on it.
fn endpoint_below(item: usize, e: &CEdge, out: bool) -> bool {
    (if out { e.src } else { e.dst }) != item
}

/// The flat layout that coordinates node and port order across groups.
struct Flat {
    ledge_of: Vec<Option<usize>>,
    ledges: Vec<LEdge>,
    /// Last rank occupied by each group, or -1.
    last: Vec<i32>,
}

/// One layered layout of a level, kept so that an unchanged level is not laid out again.
struct Laid {
    given: Vec<f64>,
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    size: LayeredResult,
}

struct Built {
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    dummies: Vec<Option<usize>>,
}

struct Compound<'a> {
    nodes: &'a mut [CNode],
    edges: &'a mut [CEdge],
    opt: &'a LayeredOptions,
    /// Root first, then every group's level after its parent's.
    levels: Vec<Level>,
    /// The level that lays out each group.
    level_index: Vec<usize>,
    local: Vec<usize>,
    ordered: bool,
    key: Vec<f64>,
    keyed: bool,
    flat: Option<Flat>,
    laid: Vec<Vec<Laid>>,
    total: Size,
}

/// `Object.is` on two numbers.
fn same_value(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

impl Compound<'_> {
    fn level_of(&self, parent: Option<usize>) -> usize {
        parent.map_or(0, |group| self.level_index[group])
    }

    /// Orders outgoing ports at the group's last rank, breaking ties using the next rank.
    fn far_hint(&self, flat: &Flat, level: &Level, port: &PortEdge) -> f64 {
        let (Some(le), Some(id)) = (flat.ledge_of[port.e], level.id) else { return f64::NAN };
        if !port.out || port.side != opposite(start_side(level.dir)) {
            return f64::NAN;
        }
        let mut inside = self.key[self.edges[port.e].src];
        let mut outside = 0.0;
        for point in &flat.ledges[le].points {
            if point[0] == f64::from(flat.last[id]) {
                inside = point[1];
            }
            if point[0] == f64::from(flat.last[id] + 1) {
                outside = point[1];
            }
        }
        inside + outside * 1e-6
    }

    fn build(&mut self, q: usize, sized: bool) -> Built {
        let level = &self.levels[q];
        let vertical = level.dir.vertical();
        let start = start_side(level.dir);
        let mut lnodes: Vec<LNode> = Vec::new();
        let mut ledges: Vec<LEdge> = Vec::new();
        for (i, &item) in level.items.iter().enumerate() {
            self.local[item] = i;
            let node = &self.nodes[item];
            let mut ln = if sized {
                LNode::new(if vertical { node.w } else { node.h }, if vertical { node.h } else { node.w }, Kind::Node)
            } else {
                LNode::new(0.0, 0.0, Kind::Node)
            };
            ln.span = if node.is_group { js::min(node.span, ln.w - 2.0 * node.pad_x) } else { node.span };
            ln.key = if self.keyed { self.key[item] } else { f64::NAN };
            lnodes.push(ln);
        }
        let (nodes, edges, local) = (&*self.nodes, &*self.edges, &self.local);
        let held = |item: usize, ei: usize, out: bool| nodes[item].is_group && endpoint_below(item, &edges[ei], out);
        let offset = |item: usize, ei: usize, out: bool| -> f64 {
            if !sized || !nodes[item].is_group || !endpoint_below(item, &edges[ei], out) {
                return 0.0;
            }
            let port = self.levels[self.level_index[item]].port(ei, out);
            if vertical { port.px - nodes[item].w / 2.0 } else { port.py - nodes[item].h / 2.0 }
        };
        for le in &level.edges {
            let e = &edges[le.e];
            let mut edge = LEdge::new(local[le.a], local[le.b], e.minlen);
            if sized {
                edge.label_w = if vertical { e.label_w } else { e.label_h };
                edge.label_h = if vertical { e.label_h } else { e.label_w };
                edge.tail_dx = offset(le.a, le.e, true);
                edge.head_dx = offset(le.b, le.e, false);
                edge.tail_fixed = held(le.a, le.e, true);
                edge.head_fixed = held(le.b, le.e, false);
            }
            ledges.push(edge);
        }
        let mut dummies: Vec<Option<usize>> = vec![None; level.ports.len()];
        for (i, port) in level.ports.iter().enumerate() {
            if port.side == start || port.side == opposite(start) {
                let d = lnodes.len();
                let mut ln = LNode::new(0.0, 0.0, if port.side == start { Kind::StartPort } else { Kind::EndPort });
                ln.key = if self.keyed {
                    self.key[if port.out { edges[port.e].dst } else { edges[port.e].src }]
                } else {
                    f64::NAN
                };
                ln.hint = match &self.flat {
                    Some(flat) if port.hint.is_nan() && self.keyed => self.far_hint(flat, level, port),
                    _ => port.hint,
                };
                lnodes.push(ln);
                dummies[i] = Some(d);
                let mut edge = if port.out {
                    LEdge::new(local[port.inner], d, 1.0)
                } else {
                    LEdge::new(d, local[port.inner], 1.0)
                };
                if port.out {
                    edge.tail_dx = offset(port.inner, port.e, true);
                    edge.tail_fixed = held(port.inner, port.e, true);
                } else {
                    edge.head_dx = offset(port.inner, port.e, false);
                    edge.head_fixed = held(port.inner, port.e, false);
                }
                ledges.push(edge);
            } else {
                lnodes[local[port.inner]].pin = if port.side == Side::Top || port.side == Side::Left { -1 } else { 1 };
            }
        }
        Built { lnodes, ledges, dummies }
    }

    /// Lays out one level, or finds the layout of the same input from an earlier attempt.
    /// Returns its place in `laid[q]`.
    fn lay_out(&mut self, q: usize, mut lnodes: Vec<LNode>, mut ledges: Vec<LEdge>) -> usize {
        if !self.ordered {
            let size = layered(&mut lnodes, &mut ledges, self.opt, None, false);
            self.laid[q].clear();
            self.laid[q].push(Laid { given: Vec::new(), lnodes, ledges, size });
            return 0;
        }
        let mut given: Vec<f64> = Vec::new();
        for ln in &lnodes {
            given.extend([ln.w, ln.h, f64::from(ln.kind as u8), f64::from(ln.pin), ln.hint, ln.key, ln.span]);
        }
        for le in &ledges {
            given.extend([
                le.tail as f64,
                le.head as f64,
                le.minlen,
                le.label_w,
                le.label_h,
                le.tail_dx,
                le.head_dx,
                f64::from(u8::from(le.tail_fixed)),
                f64::from(u8::from(le.head_fixed)),
            ]);
        }
        let past = &mut self.laid[q];
        if let Some(found) = past
            .iter()
            .position(|p| p.given.len() == given.len() && p.given.iter().zip(&given).all(|(&a, &b)| same_value(a, b)))
        {
            return found;
        }
        let size = layered(&mut lnodes, &mut ledges, self.opt, None, false);
        past.push(Laid { given, lnodes, ledges, size });
        past.len() - 1
    }

    fn set_port_side(&mut self, group: usize, e: usize, out: bool, side: Side) {
        let level = &mut self.levels[self.level_index[group]];
        let port = level.port_of[&port_key(e, out)];
        level.ports[port].side = side;
    }

    fn place(&mut self) {
        // Rank parents first to determine which group sides edges cross.
        for q in 0..self.levels.len() {
            for at in 0..self.levels[q].ports.len() {
                let port = &self.levels[q].ports[at];
                let (inner, e, out, side) = (port.inner, port.e, port.out, port.side);
                if self.nodes[inner].is_group && endpoint_below(inner, &self.edges[e], out) {
                    self.set_port_side(inner, e, out, side);
                }
            }
            if self.levels[q].edges.is_empty() {
                continue;
            }
            let crosses = |nodes: &[CNode], edges: &[CEdge], le: &LevelEdge| {
                let e = &edges[le.e];
                (nodes[le.a].is_group && e.src != le.a, nodes[le.b].is_group && e.dst != le.b)
            };
            let needed = self.levels[q].edges.iter().any(|le| {
                let (a, b) = crosses(self.nodes, self.edges, le);
                a || b
            });
            if !needed {
                continue;
            }
            let Built { mut lnodes, mut ledges, .. } = self.build(q, false);
            orient(&lnodes, &mut ledges);
            rank(&mut lnodes, &ledges, 1);
            let start = start_side(self.levels[q].dir);
            for i in 0..self.levels[q].edges.len() {
                let le = &self.levels[q].edges[i];
                let (a, b, e) = (le.a, le.b, le.e);
                let (a_crosses, b_crosses) = crosses(self.nodes, self.edges, le);
                let a_above = !ledges[i].reversed;
                if a_crosses {
                    self.set_port_side(a, e, true, if a_above { opposite(start) } else { start });
                }
                if b_crosses {
                    self.set_port_side(b, e, false, if a_above { start } else { opposite(start) });
                }
            }
        }

        // Lay out children first so their sizes are known.
        for q in (0..self.levels.len()).rev() {
            let dir = self.levels[q].dir;
            let group = self.levels[q].id;
            let vertical = dir.vertical();
            let flip = dir == Dir::BT || dir == Dir::RL;
            let built = self.build(q, true);
            let dummies = built.dummies;
            let found = self.lay_out(q, built.lnodes, built.ledges);
            let Laid { lnodes, ledges, size, .. } = &self.laid[q][found];
            let content_w = if vertical { size.width } else { size.height };
            let content_h = if vertical { size.height } else { size.width };
            let pad_x = group.map_or(0.0, |group| self.nodes[group].pad_x);
            let pad_top = group.map_or(0.0, |group| self.nodes[group].pad_top);
            let box_w = js::max(content_w + 2.0 * pad_x, group.map_or(0.0, |group| self.nodes[group].min_w));
            let box_h = content_h + pad_top + group.map_or(0.0, |group| self.nodes[group].pad_bottom);
            let off_x = (box_w - content_w) / 2.0;
            let off_y = pad_top;
            let height = size.height;
            let tx = |lx: f64, ly: f64| {
                off_x
                    + if vertical {
                        lx
                    } else if flip {
                        height - ly
                    } else {
                        ly
                    }
            };
            let ty = |lx: f64, ly: f64| off_y + if vertical { if flip { height - ly } else { ly } } else { lx };
            let axis = if vertical { Axis::Vertical } else { Axis::Horizontal };
            let convert = |points: &[[f64; 2]]| -> Vec<RoutePoint> {
                points
                    .iter()
                    .map(|point| RoutePoint { x: tx(point[0], point[1]), y: ty(point[0], point[1]), axis })
                    .collect()
            };

            let level = &mut self.levels[q];
            for (i, &item) in level.items.iter().enumerate() {
                self.nodes[item].x = tx(lnodes[i].x, lnodes[i].y);
                self.nodes[item].y = ty(lnodes[i].x, lnodes[i].y);
            }
            for (i, le) in level.edges.iter().enumerate() {
                let edge = &ledges[i];
                level.routes.insert(le.e, convert(&edge.points));
                let e = &mut self.edges[le.e];
                if e.label_w > 0.0 {
                    e.label_x = tx(edge.label_x, edge.label_y);
                    e.label_y = ty(edge.label_x, edge.label_y);
                }
            }
            let mut k = self.levels[q].edges.len();
            for i in 0..self.levels[q].ports.len() {
                let port = &self.levels[q].ports[i];
                let item = &self.nodes[port.inner];
                let border = |x: f64, y: f64| match port.side {
                    Side::Top => (x, 0.0),
                    Side::Bottom => (x, box_h),
                    Side::Left => (0.0, y),
                    Side::Right => (box_w, y),
                };
                let (bx, by, route) = if dummies[i].is_some() {
                    let mut route = convert(&ledges[k].points);
                    k += 1;
                    let at = if port.out { route.len() - 1 } else { 0 };
                    let (bx, by) = border(route[at].x, route[at].y);
                    if port.out {
                        route.push(RoutePoint { x: bx, y: by, axis });
                    } else {
                        route.insert(0, RoutePoint { x: bx, y: by, axis });
                    }
                    (bx, by, route)
                } else {
                    let mut ix = item.x;
                    let mut iy = item.y;
                    if item.is_group && endpoint_below(port.inner, &self.edges[port.e], port.out) {
                        let inner = self.levels[self.level_index[port.inner]].port(port.e, port.out);
                        ix += inner.px - item.w / 2.0;
                        iy += inner.py - item.h / 2.0;
                    } else {
                        match port.side {
                            Side::Top => iy -= item.h / 2.0,
                            Side::Bottom => iy += item.h / 2.0,
                            Side::Left => ix -= item.w / 2.0,
                            Side::Right => ix += item.w / 2.0,
                        }
                    }
                    let (bx, by) = border(ix, iy);
                    let a = if port.side == Side::Top || port.side == Side::Bottom {
                        Axis::Vertical
                    } else {
                        Axis::Horizontal
                    };
                    let inner = RoutePoint { x: ix, y: iy, axis: a };
                    let outer = RoutePoint { x: bx, y: by, axis: a };
                    (bx, by, if port.out { vec![inner, outer] } else { vec![outer, inner] })
                };
                let port = &mut self.levels[q].ports[i];
                port.px = bx;
                port.py = by;
                port.route = route;
            }

            match group {
                Some(group) => {
                    self.nodes[group].w = box_w;
                    self.nodes[group].h = box_h;
                }
                None => self.total = Size { width: box_w, height: box_h },
            }
        }

        for q in 0..self.levels.len() {
            let ox = self.levels[q].x;
            let oy = self.levels[q].y;
            for at in 0..self.levels[q].items.len() {
                let item = self.levels[q].items[at];
                let node = &mut self.nodes[item];
                node.x += ox;
                node.y += oy;
                if node.is_group {
                    let (x, y) = (node.x - node.w / 2.0, node.y - node.h / 2.0);
                    let child = &mut self.levels[self.level_index[item]];
                    child.x = x;
                    child.y = y;
                }
            }
        }

        fn append(route: &mut Vec<RoutePoint>, part: &[RoutePoint], ox: f64, oy: f64) {
            for point in part {
                let x = point.x + ox;
                let y = point.y + oy;
                if let Some(last) = route.last()
                    && (last.x - x).abs() < 0.01
                    && (last.y - y).abs() < 0.01
                {
                    continue;
                }
                route.push(RoutePoint { x, y, axis: point.axis });
            }
        }

        for q in 0..self.levels.len() {
            let level = &self.levels[q];
            for le in &level.edges {
                let (src, dst) = (self.edges[le.e].src, self.edges[le.e].dst);
                let mut route: Vec<RoutePoint> = Vec::new();
                let mut item = src;
                while self.nodes[item].parent != level.id {
                    let lv = &self.levels[self.level_of(self.nodes[item].parent)];
                    append(&mut route, &lv.port(le.e, true).route, lv.x, lv.y);
                    item = self.nodes[item].parent.expect("an end below this level has a parent");
                }
                append(&mut route, &level.routes[&le.e], level.x, level.y);
                let mut inward: Vec<usize> = Vec::new();
                let mut item = dst;
                while self.nodes[item].parent != level.id {
                    inward.push(self.level_of(self.nodes[item].parent));
                    item = self.nodes[item].parent.expect("an end below this level has a parent");
                }
                for &lv in inward.iter().rev() {
                    let lv = &self.levels[lv];
                    append(&mut route, &lv.port(le.e, false).route, lv.x, lv.y);
                }
                let e = &mut self.edges[le.e];
                e.route = route;
                if e.label_w > 0.0 {
                    e.label_x += level.x;
                    e.label_y += level.y;
                }
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        let mut places = Vec::new();
        for level in &self.levels {
            places.extend([level.x, level.y]);
            for port in &level.ports {
                places.extend([port.px, port.py]);
            }
        }
        Snapshot {
            nodes: self.nodes.iter().map(|c| [c.x, c.y, c.w, c.h]).collect(),
            routes: self.edges.iter().map(|e| e.route.clone()).collect(),
            labels: self.edges.iter().map(|e| [e.label_x, e.label_y]).collect(),
            levels: places,
            total: self.total,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        for (c, at) in self.nodes.iter_mut().zip(&s.nodes) {
            [c.x, c.y, c.w, c.h] = *at;
        }
        for ((e, route), label) in self.edges.iter_mut().zip(s.routes).zip(&s.labels) {
            e.route = route;
            [e.label_x, e.label_y] = *label;
        }
        let mut places = s.levels.into_iter();
        let mut next = || places.next().expect("the snapshot holds every level and port");
        for level in &mut self.levels {
            level.x = next();
            level.y = next();
            for port in &mut level.ports {
                port.px = next();
                port.py = next();
            }
        }
        self.total = s.total;
    }
}

/// Lays out `nodes` and `edges` with ranks running along `root_dir`. Sets every node's centre,
/// every group's size, and every edge's route and label place, and returns the size of the whole.
pub fn compound_layout(nodes: &mut [CNode], edges: &mut [CEdge], root_dir: Dir, opt: &LayeredOptions) -> Size {
    let n = nodes.len();
    let mut levels: Vec<Level> = vec![Level::new(None, root_dir)];
    let mut level_index = vec![usize::MAX; n];

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut root_items: Vec<usize> = Vec::new();
    for i in 0..n {
        match nodes[i].parent {
            Some(p) if p < n && p != i && nodes[p].is_group => children[p].push(i),
            _ => {
                nodes[i].parent = None;
                root_items.push(i);
            }
        }
    }
    levels[0].items = root_items;
    let mut depth = vec![0usize; n];
    let mut placed = vec![false; n];
    fn descend(
        from: usize,
        levels: &mut Vec<Level>,
        level_index: &mut [usize],
        nodes: &[CNode],
        children: &[Vec<usize>],
        placed: &mut [bool],
        depth: &mut [usize],
    ) {
        let mut q = from;
        while q < levels.len() {
            let (id, dir) = (levels[q].id, levels[q].dir);
            for at in 0..levels[q].items.len() {
                let item = levels[q].items[at];
                placed[item] = true;
                depth[item] = id.map_or(0, |id| depth[id] + 1);
                if nodes[item].is_group {
                    level_index[item] = levels.len();
                    let mut child = Level::new(Some(item), nodes[item].dir.unwrap_or(dir));
                    child.items = children[item].clone();
                    levels.push(child);
                }
            }
            q += 1;
        }
    }
    descend(0, &mut levels, &mut level_index, nodes, &children, &mut placed, &mut depth);
    // Groups that contain each other are unreachable from the root; lift them out.
    for i in 0..n {
        if placed[i] {
            continue;
        }
        let former = nodes[i].parent.expect("a node the root does not reach sits in a group");
        children[former].retain(|&c| c != i);
        nodes[i].parent = None;
        levels[0].items.push(i);
        placed[i] = true;
        depth[i] = 0;
        if nodes[i].is_group {
            let from = levels.len();
            level_index[i] = from;
            let mut child = Level::new(Some(i), nodes[i].dir.unwrap_or(root_dir));
            child.items = children[i].clone();
            levels.push(child);
            descend(from, &mut levels, &mut level_index, nodes, &children, &mut placed, &mut depth);
        }
    }

    for q in (1..levels.len()).rev() {
        let mut seq = f64::INFINITY;
        for &item in &levels[q].items {
            if nodes[item].seq < seq {
                seq = nodes[item].seq;
            }
        }
        if seq != f64::INFINITY {
            nodes[levels[q].id.expect("only the root has no group")].seq = seq;
        }
    }
    for level in &mut levels {
        level.items.sort_by(|&a, &b| by_difference(nodes[a].seq - nodes[b].seq).then(a.cmp(&b)));
    }

    // Every group border an edge crosses costs a port. Past the budget, an edge is drawn between
    // the groups that hold its ends, at the level they share.
    let above = |nodes: &[CNode], item: usize| nodes[item].parent.expect("a deeper node has a parent");
    let mut ports = 0usize;
    for e in edges.iter_mut() {
        let mut a = e.src;
        let mut b = e.dst;
        let mut crossed = 0;
        while depth[a] > depth[b] {
            a = above(nodes, a);
            crossed += 1;
        }
        while depth[b] > depth[a] {
            b = above(nodes, b);
            crossed += 1;
        }
        while a != b && nodes[a].parent != nodes[b].parent {
            a = above(nodes, a);
            b = above(nodes, b);
            crossed += 2;
        }
        if a == b {
            continue;
        }
        if ports + crossed > MAX_PORTS {
            e.src = a;
            e.dst = b;
        } else {
            ports += crossed;
        }
    }

    let mut loops: Vec<usize> = Vec::new();
    let mut stray: Vec<usize> = Vec::new();
    for ei in 0..edges.len() {
        let e = &mut edges[ei];
        e.route = Vec::new();
        if e.src == e.dst {
            loops.push(ei);
            continue;
        }
        let mut a = e.src;
        let mut b = e.dst;
        let mut a_path: Vec<usize> = Vec::new();
        let mut b_path: Vec<usize> = Vec::new();
        while depth[a] > depth[b] {
            a_path.push(a);
            a = above(nodes, a);
        }
        while depth[b] > depth[a] {
            b_path.push(b);
            b = above(nodes, b);
        }
        if a == b {
            stray.push(ei);
            continue;
        }
        while nodes[a].parent != nodes[b].parent {
            a_path.push(a);
            b_path.push(b);
            a = above(nodes, a);
            b = above(nodes, b);
        }
        let level_of = |parent: Option<usize>| parent.map_or(0, |group| level_index[group]);
        levels[level_of(nodes[a].parent)].edges.push(LevelEdge { e: ei, a, b });
        for (path, out) in [(&a_path, true), (&b_path, false)] {
            for &inner in path {
                let level = &mut levels[level_of(nodes[inner].parent)];
                level.port_of.insert(port_key(ei, out), level.ports.len());
                level.ports.push(PortEdge {
                    e: ei,
                    inner,
                    out,
                    side: Side::Top,
                    hint: f64::NAN,
                    px: 0.0,
                    py: 0.0,
                    route: Vec::new(),
                });
            }
        }
    }

    let ordered = ports > 1 && ports <= MAX_ORDERED_PORTS && edges.len() <= MAX_ORDERED_EDGES;

    // Use a flat layout to coordinate node and port order across groups.
    let mut key = vec![f64::NAN; n];
    let mut flat_crossings = 0;
    let mut flat_hints: Option<Flat> = None;
    let flat = levels.len() > 1 && ordered && levels.iter().all(|level| level.dir == root_dir);
    if flat {
        let vertical = root_dir.vertical();
        let mut leaf: Vec<Option<usize>> = vec![None; n];
        let mut group = vec![-1i32; n];
        let mut lnodes: Vec<LNode> = Vec::new();
        let mut of: Vec<i32> = Vec::new();
        let mut parent: Vec<i32> = Vec::new();
        for i in 0..n {
            if !nodes[i].is_group {
                continue;
            }
            group[i] = parent.len() as i32;
            parent.push(-1);
        }
        for i in 0..n {
            let p = nodes[i].parent.map_or(-1, |p| group[p]);
            if nodes[i].is_group {
                parent[group[i] as usize] = p;
            } else {
                leaf[i] = Some(lnodes.len());
                lnodes.push(LNode::new(
                    if vertical { nodes[i].w } else { nodes[i].h },
                    if vertical { nodes[i].h } else { nodes[i].w },
                    Kind::Node,
                ));
                of.push(p);
            }
        }
        let mut ledges: Vec<LEdge> = Vec::new();
        let mut ledge_of: Vec<Option<usize>> = vec![None; edges.len()];
        for (ei, e) in edges.iter().enumerate() {
            let (Some(src), Some(dst)) = (leaf[e.src], leaf[e.dst]) else { continue };
            if e.src == e.dst {
                continue;
            }
            ledge_of[ei] = Some(ledges.len());
            let mut edge = LEdge::new(src, dst, e.minlen);
            edge.label_w = if vertical { e.label_w } else { e.label_h };
            edge.label_h = if vertical { e.label_h } else { e.label_w };
            ledges.push(edge);
        }
        let flat_layout = layered(&mut lnodes, &mut ledges, opt, Some(&Groups { of, parent }), true);
        flat_crossings = flat_layout.crossings.map_or(0, |(crossings, declared)| crossings.min(declared));
        let mut sum = vec![0.0; n];
        let mut count = vec![0usize; n];
        let mut last = vec![-1i32; n];
        for i in 0..n {
            let Some(at) = leaf[i] else { continue };
            key[i] = lnodes[at].x;
            let mut p = nodes[i].parent;
            while let Some(group) = p {
                sum[group] += key[i];
                count[group] += 1;
                last[group] = last[group].max(lnodes[at].rank);
                p = nodes[group].parent;
            }
        }
        for i in 0..n {
            if count[i] > 0 {
                key[i] = sum[i] / count[i] as f64;
            }
        }
        flat_hints = Some(Flat { ledge_of, ledges, last });
    }

    let style = opt.style;
    let level_count = levels.len();
    let mut layout = Compound {
        nodes,
        edges,
        opt,
        levels,
        level_index,
        local: vec![0; n],
        ordered,
        key,
        keyed: flat,
        flat: flat_hints,
        laid: (0..level_count).map(|_| Vec::new()).collect(),
        total: Size { width: 0.0, height: 0.0 },
    };
    // The flat layout ignores group boxes, so independent ordering may yield fewer crossings.
    layout.place();
    let mut crossings: Option<u64> = None;
    if flat {
        let keyed_crossings = route_crossings(layout.nodes, layout.edges, style);
        crossings = Some(keyed_crossings);
        if keyed_crossings as f64 > flat_crossings as f64 * FLAT_SLACK + FLAT_SPARE {
            let first = layout.snapshot();
            layout.keyed = false;
            layout.place();
            let own = route_crossings(layout.nodes, layout.edges, style);
            if own >= keyed_crossings {
                layout.keyed = true;
                layout.restore(first);
            } else {
                crossings = Some(own);
            }
        }
    }

    // Refine port order using routes outside each group. Skip this if independent ordering won;
    // it rarely helps enough to justify another layout.
    if layout.keyed == flat && ordered {
        let mut again = false;
        for q in 0..layout.levels.len() {
            let Some(id) = layout.levels[q].id else { continue };
            let level = &mut layout.levels[q];
            let node = &layout.nodes[id];
            let vertical = level.dir.vertical();
            let end = opposite(start_side(level.dir));
            let mut seen: Vec<[f64; 2]> = Vec::new();
            for port in &mut level.ports {
                if port.side != end {
                    continue;
                }
                // Skip the straight run out of the port to find where the edge turns.
                let route = &layout.edges[port.e].route;
                let bx = level.x + port.px;
                let by = level.y + port.py;
                let mut at: Option<usize> = None;
                for (k, point) in route.iter().enumerate() {
                    if at.is_some() && !port.out {
                        break;
                    }
                    if (point.x - bx).abs() < 0.01 && (point.y - by).abs() < 0.01 {
                        at = Some(k);
                    }
                }
                let Some(at) = at else { continue };
                let a = if vertical { Axis::Vertical } else { Axis::Horizontal };
                let step: i64 = if port.out { 1 } else { -1 };
                let mut next = at as i64 + step;
                while next >= 0
                    && (next as usize) < route.len()
                    && (route[next as usize].along(a) - route[at].along(a)).abs() < IN_LINE
                {
                    next += step;
                }
                if next < 0 || next as usize >= route.len() {
                    continue;
                }
                port.hint = route[next as usize].along(a)
                    - if vertical { node.x - node.w / 2.0 } else { node.y - node.h / 2.0 };
                seen.push([if vertical { port.px } else { port.py }, port.hint]);
            }
            seen.sort_by(|p, q| by_difference(p[0] - q[0]));
            for i in 1..seen.len() {
                if seen[i - 1][1] > seen[i][1] {
                    again = true;
                }
            }
        }
        if again {
            let before = crossings.unwrap_or_else(|| route_crossings(layout.nodes, layout.edges, style));
            if before > 0 {
                let first = layout.snapshot();
                layout.place();
                if route_crossings(layout.nodes, layout.edges, style) >= before {
                    layout.restore(first);
                }
            }
        }
    }

    let Compound { nodes, edges, total, .. } = layout;
    for ei in stray {
        let (a, b) = (&nodes[edges[ei].src], &nodes[edges[ei].dst]);
        let e = &mut edges[ei];
        e.route = vec![
            RoutePoint { x: a.x, y: a.y, axis: Axis::Vertical },
            RoutePoint { x: b.x, y: b.y, axis: Axis::Vertical },
        ];
        e.label_x = (a.x + b.x) / 2.0;
        e.label_y = (a.y + b.y) / 2.0;
    }
    for ei in loops {
        let node = &nodes[edges[ei].src];
        let e = &mut edges[ei];
        e.label_x = node.x + node.w / 2.0 + 28.0 + e.label_w / 2.0;
        e.label_y = node.y;
    }

    total
}
