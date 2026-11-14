//! Layered (Sugiyama-style) layout of one flat graph, ported from Pele's `layout/layered.ts`.
//! Ranks run down the y axis, and callers rotate the result for other directions.
//!
//! Every coordinate must be the double Pele computes, so each sum, sort and loop keeps Pele's
//! order, and the names follow Pele's so the two can be read side by side. Pele's header comment
//! in `layered.ts` says what the layout aims for; this file only says where it had to differ.

use super::num::{by_difference, hypot, to_int32};
use crate::js;
use indexmap::IndexMap;
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Node,
    Dummy,
    Label,
    StartPort,
    EndPort,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LNode {
    pub w: f64,
    pub h: f64,
    pub kind: Kind,
    /// -1 holds the node at the start of its rank, 1 at the end, and 0 leaves it free.
    pub pin: i32,
    /// A port's cross-flow destination, or NaN if unknown.
    pub hint: f64,
    /// The initial cross-flow position, or NaN if unknown.
    pub key: f64,
    /// How much of the side that edges meet may be shared out between them.
    pub span: f64,
    pub x: f64,
    pub y: f64,
    pub rank: i32,
}

impl LNode {
    pub fn new(w: f64, h: f64, kind: Kind) -> Self {
        LNode { w, h, kind, pin: 0, hint: f64::NAN, key: f64::NAN, span: 0.0, x: 0.0, y: 0.0, rank: 0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LEdge {
    pub tail: usize,
    pub head: usize,
    pub minlen: f64,
    pub label_w: f64,
    pub label_h: f64,
    pub tail_dx: f64,
    pub head_dx: f64,
    /// An end that is held where its offset puts it, and takes no part in sharing out a node's side.
    pub tail_fixed: bool,
    pub head_fixed: bool,
    pub reversed: bool,
    /// (x, y) pairs, or (rank, cross-flow position) pairs from an order-only layout.
    pub points: Vec<[f64; 2]>,
    pub label_x: f64,
    pub label_y: f64,
}

impl LEdge {
    pub fn new(tail: usize, head: usize, minlen: f64) -> Self {
        LEdge {
            tail,
            head,
            minlen,
            label_w: 0.0,
            label_h: 0.0,
            tail_dx: 0.0,
            head_dx: 0.0,
            tail_fixed: false,
            head_fixed: false,
            reversed: false,
            points: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        }
    }
}

/// How edges are drawn, which decides how they are routed. Pele carries this as its `tracks`
/// and `straight` flags, where `tracks` wins when both are set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeStyle {
    /// Curves, which turn onto a level of their own only where they would run along another.
    #[default]
    Curved,
    /// Right angles, each edge on a track of its own across the gap between two ranks.
    Stepped,
    /// Straight from point to point.
    Linear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayeredOptions {
    pub node_sep: f64,
    pub edge_sep: f64,
    pub rank_sep: f64,
    pub port_sep: f64,
    pub style: EdgeStyle,
    /// The caller sets the ends of the edges on one side of a node apart itself, so two edges
    /// between the same two nodes need no bend to keep them from being drawn as one.
    pub apart: bool,
    /// The room that the marker at the head of an edge takes, or 0.
    pub head_room: f64,
}

/// Keeps each group contiguous within ranks. `of` maps nodes to groups and `parent` maps groups
/// to parents, both with -1 for none.
#[derive(Debug, Clone, PartialEq)]
pub struct Groups {
    pub of: Vec<i32>,
    pub parent: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayeredResult {
    pub width: f64,
    pub height: f64,
    /// From an order-only layout: the final crossings, and those before grouping or reordering.
    pub crossings: Option<(u64, u64)>,
}

const MAX_SWEEPS: usize = 24;
const MAX_DUMMIES: usize = 20000;
const PORT_PITCH: f64 = 16.0;
const MIN_PITCH: f64 = 8.0;
const MIN_SLOPE: f64 = 0.125;
const MAX_GAP: f64 = 3.0;
const TRACK_MARGIN: f64 = 18.0;
const TRACK_PITCH: f64 = 10.0;
const TRACK_SPREAD: f64 = 14.0;
const LEVEL_WIDE: f64 = 2.0;
const LEVEL_MARGIN: f64 = 28.0;
const LEVEL_PITCH: f64 = 12.0;
const LEVEL_LEAD: f64 = 12.0;
const LEVEL_CLEAR: f64 = 8.0;
const LEVEL_TOUCH: f64 = 4.0;
const CROSSED_SLOPE: f64 = 0.5;
const IN_LINE: f64 = 0.5;
const TWIN: f64 = 1.0;
const SNAP: f64 = 12.0;
const BEND_SNAP: f64 = 24.0;
const LEAN_CLEAR: f64 = 40.0;
const MAX_WEIGHED: usize = 400;
const SAMPLES: usize = 16;

/// What a segment's end is to the node it meets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum End {
    #[default]
    Held,
    Leaves,
    Arrives,
}

fn is_dummy(kind: Kind) -> bool {
    kind == Kind::Dummy || kind == Kind::Label
}

fn sort_range(list: &mut [usize], from: usize, to: usize, key: &[usize]) {
    for i in from + 1..to {
        let item = list[i];
        let k = key[item];
        let mut j = i;
        while j > from && (key[list[j - 1]] > k || (key[list[j - 1]] == k && list[j - 1] > item)) {
            list[j] = list[j - 1];
            j -= 1;
        }
        list[j] = item;
    }
}

fn sort_by_bary(list: &mut [usize], count: usize, bary: &[f64], pos: &[usize], right: bool, spare: &mut [usize]) {
    let before = |a: usize, b: usize| {
        bary[a] < bary[b] || (bary[a] == bary[b] && if right { pos[a] > pos[b] } else { pos[a] < pos[b] })
    };
    const RUN: usize = 8;
    let mut s = 0;
    while s < count {
        for i in s + 1..count.min(s + RUN) {
            let item = list[i];
            let mut j = i;
            while j > s && before(item, list[j - 1]) {
                list[j] = list[j - 1];
                j -= 1;
            }
            list[j] = item;
        }
        s += RUN;
    }
    let mut in_list = true;
    let mut width = RUN;
    while width < count {
        let (from, to): (&[usize], &mut [usize]) = if in_list { (&*list, &mut *spare) } else { (&*spare, &mut *list) };
        let mut s = 0;
        while s < count {
            let mid = count.min(s + width);
            let end = count.min(s + 2 * width);
            let (mut i, mut j, mut k) = (s, mid, s);
            while i < mid && j < end {
                if before(from[j], from[i]) {
                    to[k] = from[j];
                    j += 1;
                } else {
                    to[k] = from[i];
                    i += 1;
                }
                k += 1;
            }
            while i < mid {
                to[k] = from[i];
                i += 1;
                k += 1;
            }
            while j < end {
                to[k] = from[j];
                j += 1;
                k += 1;
            }
            s += 2 * width;
        }
        in_list = !in_list;
        width *= 2;
    }
    if !in_list {
        list[..count].copy_from_slice(&spare[..count]);
    }
}

/// The pieces of edges that each cross one gap. See `Pieces` in `layered.ts`.
#[derive(Default)]
struct Pieces {
    from: Vec<f64>,
    to: Vec<f64>,
    rank: Vec<usize>,
    lower: Vec<Option<usize>>,
    track: Vec<i32>,
    levels: Vec<i32>,
    wide: Vec<bool>,
}

struct Graph {
    n: usize,
    real: usize,
    max_rank: usize,
    w: Vec<f64>,
    kind: Vec<Kind>,
    rank: Vec<usize>,
    ef: Vec<usize>,
    et: Vec<usize>,
    efd: Vec<f64>,
    etd: Vec<f64>,
    succ_start: Vec<usize>,
    succ_e: Vec<usize>,
    pred_start: Vec<usize>,
    pred_e: Vec<usize>,
    layer_start: Vec<usize>,
    order: Vec<usize>,
    pos: Vec<usize>,
    offsets: bool,
    fixed: Vec<bool>,
    grp: Option<Vec<i32>>,
    group_parent: Option<Vec<i32>>,
}

impl Graph {
    fn set_positions(&mut self) {
        for r in 0..=self.max_rank {
            let s = self.layer_start[r];
            for i in s..self.layer_start[r + 1] {
                self.pos[self.order[i]] = i - s;
            }
        }
    }
}

/// Sorts a rank by `key` unless a node has none. A stable sort stands in for Pele's tie-break
/// on each node's place before the sort.
fn sort_rank(order: &mut [usize], layer_start: &[usize], r: usize, key: impl Fn(usize) -> f64) -> bool {
    let s = layer_start[r];
    let e = layer_start[r + 1];
    if e - s < 2 {
        return false;
    }
    if order[s..e].iter().any(|&v| key(v).is_nan()) {
        return false;
    }
    order[s..e].sort_by(|&a, &b| by_difference(key(a) - key(b)));
    true
}

pub fn layered(
    nodes: &mut [LNode],
    edges: &mut [LEdge],
    opt: &LayeredOptions,
    groups: Option<&Groups>,
    order_only: bool,
) -> LayeredResult {
    let real = nodes.len();
    if real == 0 {
        return LayeredResult { width: 0.0, height: 0.0, crossings: None };
    }
    let m = edges.len();
    if real == 1 && m == 0 && nodes[0].kind == Kind::Node {
        let node = &mut nodes[0];
        node.rank = 0;
        node.x = node.w / 2.0;
        node.y = node.h / 2.0;
        return LayeredResult { width: node.w, height: node.h, crossings: None };
    }
    let tracks = opt.style == EdgeStyle::Stepped;
    let straight = opt.style == EdgeStyle::Linear;

    let mut step: usize = 1;
    {
        let mut seen = HashSet::new();
        for e in edges.iter() {
            let pair = (e.tail.min(e.head), e.tail.max(e.head));
            if e.label_w > 0.0 || !seen.insert(pair) {
                step = 2;
                break;
            }
        }
    }

    orient(nodes, edges);
    rank(nodes, edges, step as i32);

    let mut span = vec![0usize; m];
    let mut dummies = 0usize;
    for (ei, e) in edges.iter().enumerate() {
        span[ei] = (nodes[e.head].rank - nodes[e.tail].rank).unsigned_abs() as usize;
        if span[ei] > 1 {
            dummies += span[ei] - 1;
        }
    }
    let mut direct = vec![false; m];
    if dummies > MAX_DUMMIES {
        let mut longest: Vec<usize> = (0..m).filter(|&ei| span[ei] > 1).collect();
        longest.sort_by(|&a, &b| span[b].cmp(&span[a]).then(a.cmp(&b)));
        for ei in longest {
            if dummies <= MAX_DUMMIES {
                break;
            }
            direct[ei] = true;
            dummies -= span[ei] - 1;
        }
    }

    let n = real + dummies;
    let mut w = vec![0.0; n];
    let mut h = vec![0.0; n];
    let mut kind = vec![Kind::Node; n];
    let mut rank_of = vec![0usize; n];
    let mut max_rank = 0usize;
    for (i, node) in nodes.iter().enumerate() {
        w[i] = node.w;
        h[i] = node.h;
        kind[i] = node.kind;
        rank_of[i] = node.rank as usize;
        if rank_of[i] > max_rank {
            max_rank = rank_of[i];
        }
    }

    let mut segments = 0usize;
    for ei in 0..m {
        if !direct[ei] {
            segments += span[ei];
        }
    }
    let mut ef = vec![0usize; segments];
    let mut et = vec![0usize; segments];
    let mut efd = vec![0.0; segments];
    let mut etd = vec![0.0; segments];
    let mut first_dummy: Vec<Option<usize>> = vec![None; m];
    let mut heads = vec![0u8; max_rank + 1];
    let mut label_node: Vec<Option<usize>> = vec![None; m];
    let spans = nodes.iter().any(|node| node.span > 0.0 && node.kind == Kind::Node);
    let mut efk = vec![End::Held; if spans { segments } else { 0 }];
    let mut etk = vec![End::Held; if spans { segments } else { 0 }];
    let mut first_seg = vec![usize::MAX; m];
    let mut last_seg = vec![usize::MAX; m];
    let mut key = vec![f64::NAN; n];
    for (i, node) in nodes.iter().enumerate() {
        key[i] = node.key;
    }
    let mut offsets = false;
    {
        let mut next = real;
        let mut ce = 0usize;
        for ei in 0..m {
            if direct[ei] {
                continue;
            }
            let e = &edges[ei];
            let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
            let (a_dx, b_dx) = if e.reversed { (e.head_dx, e.tail_dx) } else { (e.tail_dx, e.head_dx) };
            if a_dx != 0.0 || b_dx != 0.0 {
                offsets = true;
            }
            let ra = rank_of[a] as i64;
            let rb = rank_of[b] as i64;
            let mut label_rank: i64 = -1;
            if e.label_w > 0.0 {
                label_rank = ra + ((rb - ra) >> 1);
                let toward_tail = if e.reversed { 1 } else { -1 };
                if step == 2 && rb - ra >= 4 {
                    if label_rank & 1 == 1 {
                        label_rank += toward_tail;
                    }
                } else if step == 2 && label_rank & 1 == 0 {
                    label_rank += toward_tail;
                }
            }
            let mut prev = a;
            if rb - ra > 1 {
                first_dummy[ei] = Some(next);
            }
            first_seg[ei] = ce;
            if spans && !(if e.reversed { e.head_fixed } else { e.tail_fixed }) {
                efk[ce] = if e.reversed { End::Arrives } else { End::Leaves };
            }
            for r in ra + 1..rb {
                let d = next;
                next += 1;
                rank_of[d] = r as usize;
                key[d] = key[a] + ((key[b] - key[a]) * (r - ra) as f64) / (rb - ra) as f64;
                if r == label_rank {
                    w[d] = e.label_w;
                    h[d] = e.label_h;
                    kind[d] = Kind::Label;
                    label_node[ei] = Some(d);
                    heads[r as usize] |= if e.reversed { 2 } else { 1 };
                } else {
                    kind[d] = Kind::Dummy;
                }
                ef[ce] = prev;
                et[ce] = d;
                efd[ce] = if prev == a { a_dx } else { 0.0 };
                ce += 1;
                prev = d;
            }
            ef[ce] = prev;
            et[ce] = b;
            efd[ce] = if prev == a { a_dx } else { 0.0 };
            etd[ce] = b_dx;
            if spans && !(if e.reversed { e.tail_fixed } else { e.head_fixed }) {
                etk[ce] = if e.reversed { End::Leaves } else { End::Arrives };
            }
            last_seg[ei] = ce;
            ce += 1;
        }
    }
    let bends = |ei: usize| -> std::ops::Range<usize> {
        match first_dummy[ei] {
            Some(d0) => d0..d0 + span[ei] - 1,
            None => 0..0,
        }
    };

    let grp: Option<Vec<i32>> = groups.map(|groups| {
        let (of, parent) = (&groups.of, &groups.parent);
        let mut grp = vec![0i32; n];
        grp[..real].copy_from_slice(&of[..real]);
        let depth = |mut gr: i32| -> usize {
            let mut d = 0;
            while gr >= 0 {
                d += 1;
                gr = parent[gr as usize];
            }
            d
        };
        let lca = |mut x: i32, mut y: i32| -> i32 {
            let mut dx = depth(x);
            let mut dy = depth(y);
            while dx > dy {
                x = parent[x as usize];
                dx -= 1;
            }
            while dy > dx {
                y = parent[y as usize];
                dy -= 1;
            }
            while x != y {
                x = parent[x as usize];
                y = parent[y as usize];
            }
            x
        };
        for ei in 0..m {
            if first_dummy[ei].is_none() {
                continue;
            }
            let gr = lca(of[edges[ei].tail], of[edges[ei].head]);
            for d in bends(ei) {
                grp[d] = gr;
            }
        }
        grp
    });

    let mut succ_start = vec![0usize; n + 1];
    let mut pred_start = vec![0usize; n + 1];
    for e in 0..segments {
        succ_start[ef[e] + 1] += 1;
        pred_start[et[e] + 1] += 1;
    }
    for i in 0..n {
        succ_start[i + 1] += succ_start[i];
        pred_start[i + 1] += pred_start[i];
    }
    let mut succ_e = vec![0usize; segments];
    let mut pred_e = vec![0usize; segments];
    {
        let mut sf = succ_start[..n].to_vec();
        let mut pf = pred_start[..n].to_vec();
        for e in 0..segments {
            succ_e[sf[ef[e]]] = e;
            sf[ef[e]] += 1;
            pred_e[pf[et[e]]] = e;
            pf[et[e]] += 1;
        }
    }

    let mut layer_start = vec![0usize; max_rank + 2];
    for i in 0..n {
        layer_start[rank_of[i] + 1] += 1;
    }
    for r in 0..=max_rank {
        layer_start[r + 1] += layer_start[r];
    }
    let mut order = vec![0usize; n];

    {
        let mut by_rank = vec![0usize; n];
        let mut fill = layer_start[..=max_rank].to_vec();
        for i in 0..n {
            by_rank[fill[rank_of[i]]] = i;
            fill[rank_of[i]] += 1;
        }
        fill.copy_from_slice(&layer_start[..=max_rank]);
        let mut visited = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();
        for k in 0..n {
            if visited[by_rank[k]] {
                continue;
            }
            stack.push(by_rank[k]);
            while let Some(v) = stack.pop() {
                if visited[v] {
                    continue;
                }
                visited[v] = true;
                order[fill[rank_of[v]]] = v;
                fill[rank_of[v]] += 1;
                for j in (succ_start[v]..succ_start[v + 1]).rev() {
                    let next = et[succ_e[j]];
                    if !visited[next] {
                        stack.push(next);
                    }
                }
            }
        }
    }

    for r in 0..=max_rank {
        sort_rank(&mut order, &layer_start, r, |v| key[v]);
    }

    if nodes.iter().any(|node| node.pin != 0) {
        let pin_of = |v: usize| if v < real { nodes[v].pin } else { 0 };
        for r in 0..=max_rank {
            order[layer_start[r]..layer_start[r + 1]].sort_by_key(|&v| pin_of(v));
        }
    }
    let mut fixed = vec![false; max_rank + 1];
    let hint = |v: usize| {
        if v < real && (nodes[v].kind == Kind::StartPort || nodes[v].kind == Kind::EndPort) {
            nodes[v].hint
        } else {
            f64::NAN
        }
    };
    for r in [0, max_rank] {
        if sort_rank(&mut order, &layer_start, r, hint) {
            fixed[r] = true;
        }
    }

    let mut g = Graph {
        n,
        real,
        max_rank,
        w,
        kind,
        rank: rank_of,
        ef,
        et,
        efd,
        etd,
        succ_start,
        succ_e,
        pred_start,
        pred_e,
        layer_start,
        order,
        pos: vec![0usize; n],
        offsets,
        fixed,
        grp,
        group_parent: groups.map(|groups| groups.parent.clone()),
    };
    g.set_positions();

    let mut crossings = 0;
    let mut declared = 0;
    if segments > 0 && max_rank > 0 {
        (crossings, declared) = reduce_crossings(&mut g, nodes);
        g.set_positions();
    }
    if order_only {
        let at = position(&mut g, opt);
        for i in 0..real {
            nodes[i].x = at[i];
        }
        for ei in 0..m {
            let e = &mut edges[ei];
            let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
            let mut pts = vec![[g.rank[a] as f64, at[a]]];
            if !direct[ei] {
                for d in bends(ei) {
                    pts.push([g.rank[d] as f64, at[d]]);
                }
            }
            pts.push([g.rank[b] as f64, at[b]]);
            e.points = pts;
        }
        return LayeredResult { width: 0.0, height: 0.0, crossings: Some((crossings, declared)) };
    }

    if spans {
        spread(&mut g, nodes, &efk, &etk);
    }

    let mut xs = position(&mut g, opt);

    let empty = empty_ranks(&g);
    let empty = empty.as_deref();
    let is_empty = |r: usize| empty.is_some_and(|empty| empty[r]);
    if spans {
        let free = free_ends(&g, &efk, &etk);
        align_labels(&g, nodes, &mut xs, &free, opt, empty);
        settle(&mut g, nodes, &xs, &efk, &etk, &free, empty);
        snap(&g, &mut xs, empty, opt);
        for ei in 0..m {
            if direct[ei] {
                continue;
            }
            let a_dx = g.efd[first_seg[ei]];
            let b_dx = g.etd[last_seg[ei]];
            let e = &mut edges[ei];
            e.tail_dx = if e.reversed { b_dx } else { a_dx };
            e.head_dx = if e.reversed { a_dx } else { b_dx };
        }
    }

    align_ports(&g, &mut xs, empty);

    let mut on_line = vec![false; n];
    if let Some(empty) = empty
        && !tracks
    {
        straighten(&g, &mut xs, empty, &first_dummy, &span, if opt.apart { 0.0 } else { opt.edge_sep }, &mut on_line);
    }
    if !tracks {
        past_labels(&g, &mut xs, &first_dummy, &span, opt.edge_sep, &mut on_line);
    }
    let mut piece_start = vec![0usize; m + 1];
    let mut pieces: Option<Pieces> = if tracks || (spans && !straight) { Some(Pieces::default()) } else { None };
    let mut track_count = vec![0i32; max_rank + 1];
    if let Some(pieces) = &mut pieces {
        for ei in 0..m {
            piece_start[ei] = pieces.from.len();
            if direct[ei] {
                continue;
            }
            let mut twin = false;
            for d in bends(ei) {
                let bare = is_empty(g.rank[d]);
                if !tracks && bare != on_line[d] {
                    twin = true;
                }
            }
            if twin {
                continue;
            }
            let mut x = xs[g.ef[first_seg[ei]]] + g.efd[first_seg[ei]];
            let mut rank = g.rank[g.ef[first_seg[ei]]];
            for d in bends(ei) {
                if is_empty(g.rank[d]) {
                    continue;
                }
                pieces.from.push(x);
                pieces.to.push(xs[d]);
                pieces.rank.push(rank);
                pieces.lower.push(Some(d));
                x = xs[d];
                rank = g.rank[d];
            }
            pieces.from.push(x);
            pieces.to.push(xs[g.et[last_seg[ei]]] + g.etd[last_seg[ei]]);
            pieces.rank.push(rank);
            pieces.lower.push(None);
        }
        piece_start[m] = pieces.from.len();
    }

    let port_only = |r: usize, port: Kind| -> bool {
        let mut found = false;
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            let k = g.kind[g.order[i]];
            if k == port {
                found = true;
            } else if k != Kind::Dummy {
                return false;
            }
        }
        found
    };
    let start_ranks = if port_only(0, Kind::StartPort) { step as i64 } else { 0 };
    let end_ranks = if port_only(max_rank, Kind::EndPort) { step as i64 } else { 0 };
    let mut run = vec![0.0; max_rank + 1];
    for e in 0..segments {
        let across = (xs[g.ef[e]] + g.efd[e] - xs[g.et[e]] - g.etd[e]).abs();
        if across > run[g.rank[g.ef[e]]] {
            run[g.rank[g.ef[e]]] = across;
        }
    }
    let mut band_top = vec![0.0; max_rank + 1];
    let mut band_bottom = vec![0.0; max_rank + 1];
    let usual = |r: usize| -> f64 {
        let r = r as i64;
        (if r < start_ranks || r >= max_rank as i64 - end_ranks { opt.port_sep } else { opt.rank_sep }) / step as f64
    };
    let mut gap_under = vec![0.0; max_rank + 1];
    for r in 0..max_rank {
        gap_under[r] = if pieces.is_some() {
            usual(r)
        } else {
            js::max(usual(r), js::min((MAX_GAP * opt.rank_sep) / step as f64, run[r] * MIN_SLOPE))
        };
    }
    let mut between = vec![true; max_rank + 1];
    for i in 0..real {
        between[g.rank[i]] = false;
    }
    for r in 1..max_rank {
        if tracks || !between[r] || (heads[r] != 1 && heads[r] != 2) {
            continue;
        }
        let down = heads[r] == 1;
        let shift = js::min(opt.head_room / 2.0, gap_under[if down { r - 1 } else { r }] / 2.0);
        gap_under[r - 1] += if down { -shift } else { shift };
        gap_under[r] += if down { shift } else { -shift };
    }
    let margin = if tracks { TRACK_MARGIN } else { LEVEL_MARGIN };
    let least_pitch = if tracks { TRACK_PITCH } else { LEVEL_PITCH };
    let most_pitch = if tracks { TRACK_SPREAD } else { LEVEL_PITCH };
    let mut below = vec![0usize; max_rank + 1];
    if let Some(pieces) = &mut pieces {
        let mut room = vec![0.0; max_rank + 1];
        for r in 0..max_rank {
            if is_empty(r) {
                continue;
            }
            let mut next = r + 1;
            room[r] = gap_under[r];
            while next < max_rank && is_empty(next) {
                room[r] += gap_under[next];
                next += 1;
            }
            below[r] = next;
        }
        for k in 0..pieces.from.len() {
            pieces.wide.push((pieces.from[k] - pieces.to[k]).abs() > LEVEL_WIDE * room[pieces.rank[k]]);
        }
        let mut tall = vec![0.0; max_rank + 1];
        assign_tracks(pieces, &mut track_count, !tracks, &room, &mut tall);
        for r in 0..max_rank {
            if track_count[r] > 0 {
                gap_under[r] += js::max(0.0, 2.0 * margin + (track_count[r] - 1) as f64 * least_pitch - room[r]);
            } else if tall[r] > 0.0 {
                gap_under[r] += js::max(0.0, js::min(MAX_GAP * opt.rank_sep, tall[r]) - room[r]);
            }
        }
    }
    let mut y = 0.0;
    for r in 0..=max_rank {
        let mut tallest = 0.0;
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            if h[g.order[i]] > tallest {
                tallest = h[g.order[i]];
            }
        }
        band_top[r] = y;
        band_bottom[r] = y + tallest;
        y += tallest + gap_under[r];
    }
    let height = y;
    let inset = |d: usize| -> f64 {
        if g.kind[d] == Kind::Dummy && between[g.rank[d]] && !tracks {
            (band_bottom[g.rank[d]] - band_top[g.rank[d]]) / 4.0
        } else {
            0.0
        }
    };
    let track_y = |pieces: &Pieces, piece: usize, on_track: i32| -> f64 {
        let r = pieces.rank[piece];
        let top = band_bottom[r];
        let bottom = band_top[below[r]];
        let count = if tracks { track_count[r] } else { pieces.levels[piece] };
        let pitch =
            if count > 1 { js::min(most_pitch, (bottom - top - 2.0 * margin) / (count - 1) as f64) } else { 0.0 };
        (top + bottom) / 2.0 + (on_track as f64 - (count - 1) as f64 / 2.0) * pitch
    };

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for i in 0..n {
        let half = g.w[i] / 2.0;
        if xs[i] - half < min_x {
            min_x = xs[i] - half;
        }
        if xs[i] + half > max_x {
            max_x = xs[i] + half;
        }
    }
    let mut arrives = vec![false; max_rank + 1];
    let mut leaves = vec![false; max_rank + 1];
    for i in 0..real {
        if g.kind[i] != Kind::Node || !spans {
            continue;
        }
        if g.pred_start[i + 1] > g.pred_start[i] {
            arrives[g.rank[i]] = true;
        }
        if g.succ_start[i + 1] > g.succ_start[i] {
            leaves[g.rank[i]] = true;
        }
    }
    for (i, node) in nodes.iter_mut().enumerate() {
        let r = node.rank as usize;
        node.x = xs[i] - min_x;
        node.y = if g.kind[i] == Kind::Node && leaves[r] && !arrives[r] {
            band_bottom[r] - node.h / 2.0
        } else if g.kind[i] == Kind::Node && arrives[r] && !leaves[r] {
            band_top[r] + node.h / 2.0
        } else {
            (band_top[r] + band_bottom[r]) / 2.0
        };
    }

    for ei in 0..m {
        let e = &edges[ei];
        let first = &nodes[if e.reversed { e.head } else { e.tail }];
        let last = &nodes[if e.reversed { e.tail } else { e.head }];
        let mut pts: Vec<[f64; 2]> =
            vec![[first.x + if e.reversed { e.head_dx } else { e.tail_dx }, first.y + first.h / 2.0]];
        match &pieces {
            Some(pieces) if piece_start[ei + 1] > piece_start[ei] => {
                for k in piece_start[ei]..piece_start[ei + 1] {
                    if pieces.track[k] >= 0 && (tracks || pieces.wide[k]) {
                        let across = track_y(pieces, k, pieces.track[k]);
                        pts.push([pieces.from[k] - min_x, across]);
                        pts.push([pieces.to[k] - min_x, across]);
                    } else if pieces.track[k] > 0 {
                        pts.push([pieces.from[k] - min_x, track_y(pieces, k, pieces.track[k] - 1)]);
                    }
                    let Some(d) = pieces.lower[k] else { continue };
                    pts.push([xs[d] - min_x, band_top[g.rank[d]] + inset(d)]);
                    if band_bottom[g.rank[d]] > band_top[g.rank[d]] {
                        pts.push([xs[d] - min_x, band_bottom[g.rank[d]] - inset(d)]);
                    }
                }
            }
            _ => {
                for d in bends(ei) {
                    if on_line[d] {
                        continue;
                    }
                    let x = xs[d] - min_x;
                    let top = band_top[g.rank[d]];
                    let bottom = band_bottom[g.rank[d]];
                    pts.push([x, top + inset(d)]);
                    if bottom > top {
                        pts.push([x, bottom - inset(d)]);
                    }
                }
            }
        }
        pts.push([last.x + if e.reversed { e.tail_dx } else { e.head_dx }, last.y - last.h / 2.0]);
        if e.reversed {
            pts.reverse();
        }
        let e = &mut edges[ei];
        if let Some(ln) = label_node[ei] {
            e.label_x = xs[ln] - min_x;
            e.label_y = (band_top[g.rank[ln]] + band_bottom[g.rank[ln]]) / 2.0;
        } else if e.label_w > 0.0 {
            let end = pts[pts.len() - 1];
            e.label_x = (pts[0][0] + end[0]) / 2.0;
            e.label_y = (pts[0][1] + end[1]) / 2.0;
        }
        e.points = pts;
    }

    LayeredResult { width: max_x - min_x, height, crossings: None }
}

/// Reverses the edges that close a cycle, found by depth-first search in declaration order.
pub fn orient(nodes: &[LNode], edges: &mut [LEdge]) {
    let n = nodes.len();
    let m = edges.len();
    let mut start = vec![0usize; n + 1];
    for e in edges.iter_mut() {
        e.reversed = nodes[e.head].kind == Kind::StartPort || nodes[e.tail].kind == Kind::EndPort;
        if !e.reversed {
            start[e.tail + 1] += 1;
        }
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut out = vec![0usize; start[n]];
    {
        let mut fill = start[..n].to_vec();
        for i in 0..m {
            if !edges[i].reversed {
                out[fill[edges[i].tail]] = i;
                fill[edges[i].tail] += 1;
            }
        }
    }
    let mut state = vec![0u8; n];
    let mut cursor = start[..n].to_vec();
    let mut stack: Vec<usize> = Vec::new();
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        stack.push(root);
        state[root] = 1;
        while let Some(&v) = stack.last() {
            if cursor[v] < start[v + 1] {
                let e = &mut edges[out[cursor[v]]];
                cursor[v] += 1;
                let next = e.head;
                if state[next] == 1 {
                    e.reversed = true;
                } else if state[next] == 0 {
                    state[next] = 1;
                    stack.push(next);
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
}

pub fn rank(nodes: &mut [LNode], edges: &[LEdge], step: i32) {
    let n = nodes.len();
    let m = edges.len();
    let mut from = vec![0usize; m];
    let mut to = vec![0usize; m];
    let mut len = vec![0i32; m];
    let mut out_start = vec![0usize; n + 1];
    let mut in_start = vec![0usize; n + 1];
    for (i, e) in edges.iter().enumerate() {
        let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
        from[i] = a;
        to[i] = b;
        len[i] = to_int32(js::max(1.0, e.minlen) * f64::from(step));
        out_start[a + 1] += 1;
        in_start[b + 1] += 1;
    }
    for i in 0..n {
        out_start[i + 1] += out_start[i];
        in_start[i + 1] += in_start[i];
    }
    let mut out_e = vec![0usize; m];
    let mut in_e = vec![0usize; m];
    {
        let mut of = out_start[..n].to_vec();
        let mut inf = in_start[..n].to_vec();
        for i in 0..m {
            out_e[of[from[i]]] = i;
            of[from[i]] += 1;
            in_e[inf[to[i]]] = i;
            inf[to[i]] += 1;
        }
    }

    let mut r = vec![0i32; n];
    for i in 0..n {
        if nodes[i].kind == Kind::StartPort {
            r[i] = -step;
        }
    }
    let mut indeg: Vec<usize> = (0..n).map(|i| in_start[i + 1] - in_start[i]).collect();
    let mut topo = vec![0usize; n];
    let mut count = 0;
    for i in 0..n {
        if indeg[i] == 0 {
            topo[count] = i;
            count += 1;
        }
    }
    let mut q = 0;
    while q < count {
        let v = topo[q];
        for k in out_start[v]..out_start[v + 1] {
            let ei = out_e[k];
            let next = to[ei];
            if r[v] + len[ei] > r[next] {
                r[next] = r[v] + len[ei];
            }
            indeg[next] -= 1;
            if indeg[next] == 0 {
                topo[count] = next;
                count += 1;
            }
        }
        q += 1;
    }

    for _pass in 0..8 {
        let mut changed = false;
        for q in (0..count).rev() {
            let v = topo[q];
            let ins = in_start[v + 1] - in_start[v];
            let outs = out_start[v + 1] - out_start[v];
            if ins == 0 && outs == 0 {
                continue;
            }
            let lo = (in_start[v]..in_start[v + 1]).map(|k| r[from[in_e[k]]] + len[in_e[k]]).max();
            let hi = (out_start[v]..out_start[v + 1]).map(|k| r[to[out_e[k]]] - len[out_e[k]]).min();
            let mut next = r[v];
            if outs > ins
                && let Some(hi) = hi
            {
                next = hi;
            } else if ins > outs
                && let Some(lo) = lo
            {
                next = lo;
            }
            if next != r[v] {
                r[v] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut bounds: Option<(i32, i32)> = None;
    let mut start_ports = false;
    for i in 0..n {
        let kind = nodes[i].kind;
        if kind == Kind::StartPort {
            start_ports = true;
        }
        if kind == Kind::StartPort || kind == Kind::EndPort {
            continue;
        }
        bounds = Some(match bounds {
            Some((min, max)) => (min.min(r[i]), max.max(r[i])),
            None => (r[i], r[i]),
        });
    }
    let (mut min, max) = bounds.unwrap_or((0, 0));
    for i in 0..n {
        if nodes[i].kind == Kind::StartPort {
            r[i] = min - step;
        } else if nodes[i].kind == Kind::EndPort {
            r[i] = max + step;
        }
    }
    if start_ports {
        min -= step;
    }
    for i in 0..n {
        nodes[i].rank = r[i] - min;
    }
}

/// The ends of edges that may move along one side of a node, ordered by their opposite nodes.
/// See `sideEnds` in `layered.ts`. `ids` and `room` must start empty.
fn side_ends(
    g: &Graph,
    v: usize,
    side: usize,
    span: f64,
    kinds: &[End],
    ids: &mut Vec<usize>,
    room: &mut Vec<f64>,
) -> f64 {
    let (start, list, other) =
        if side == 0 { (&g.pred_start, &g.pred_e, &g.ef) } else { (&g.succ_start, &g.succ_e, &g.et) };
    let pos = &g.pos;
    for k in start[v]..start[v + 1] {
        if kinds[list[k]] != End::Held {
            ids.push(list[k]);
        }
    }
    if ids.len() < 2 {
        return 0.0;
    }
    ids.sort_by(|&a, &b| pos[other[a]].cmp(&pos[other[b]]).then(a.cmp(&b)));
    room.resize(ids.len(), 0.0);
    let mut fill = |share: &dyn Fn(bool, bool) -> f64| -> f64 {
        let mut total = 0.0;
        room[0] = 0.0;
        for k in 1..ids.len() {
            let same = kinds[ids[k]] == kinds[ids[k - 1]];
            room[k] = share(same && kinds[ids[k]] == End::Leaves, same);
            total += room[k];
        }
        total
    };
    let mut total = fill(&|_, _| 1.0);
    if span / total < MIN_PITCH {
        total = fill(&|leaves, _| if leaves { 0.5 } else { 1.0 });
    }
    if span / total < MIN_PITCH {
        total = fill(&|_, same| if same { 0.0 } else { 1.0 });
    }
    if total == 0.0 {
        return 0.0;
    }
    let pitch = js::min(PORT_PITCH, span / total);
    for k in 1..ids.len() {
        room[k] *= pitch;
    }
    total * pitch
}

/// Gives the edges that meet one side of a node places of their own along it, around its middle.
fn spread(g: &mut Graph, nodes: &[LNode], efk: &[End], etk: &[End]) {
    for v in 0..g.real {
        let span = js::min(nodes[v].span, g.w[v]);
        if span <= 0.0 || nodes[v].kind != Kind::Node {
            continue;
        }
        for side in 0..2 {
            let mut ids = Vec::new();
            let mut room = Vec::new();
            let total = side_ends(g, v, side, span, if side == 0 { etk } else { efk }, &mut ids, &mut room);
            if total == 0.0 {
                continue;
            }
            let dx = if side == 0 { &mut g.etd } else { &mut g.efd };
            let mut at = -total / 2.0;
            for k in 0..ids.len() {
                at += room[k];
                dx[ids[k]] = at;
            }
        }
    }
}

/// The segment of an edge that reaches the next point held in place, on the way up from segment
/// `e` (side 0) or down: past the bends in ranks that hold nothing else.
fn follow(g: &Graph, empty: Option<&[bool]>, mut e: usize, side: usize) -> usize {
    let Some(empty) = empty else { return e };
    for _hops in 0..100000 {
        let u = if side == 0 { g.ef[e] } else { g.et[e] };
        if g.kind[u] != Kind::Dummy || !empty[g.rank[u]] {
            break;
        }
        e = if side == 0 { g.pred_e[g.pred_start[u]] } else { g.succ_e[g.succ_start[u]] };
    }
    e
}

/// Aligns border ports with the ends on their nodes so edges run straight across the gap.
fn align_ports(g: &Graph, xs: &mut [f64], empty: Option<&[bool]>) {
    for (r, kind) in [(0, Kind::StartPort), (g.max_rank, Kind::EndPort)] {
        let s = g.layer_start[r];
        let end = g.layer_start[r + 1];
        let mut to: Vec<f64> = Vec::new();
        for i in s..end {
            let v = g.order[i];
            let start = kind == Kind::StartPort;
            let degree =
                if start { g.succ_start[v + 1] - g.succ_start[v] } else { g.pred_start[v + 1] - g.pred_start[v] };
            if g.kind[v] != kind || degree != 1 {
                break;
            }
            let e = follow(
                g,
                empty,
                if start { g.succ_e[g.succ_start[v]] } else { g.pred_e[g.pred_start[v]] },
                if start { 1 } else { 0 },
            );
            let at = if start { xs[g.et[e]] + g.etd[e] } else { xs[g.ef[e]] + g.efd[e] };
            if let Some(&last) = to.last()
                && at < last + TWIN
            {
                break;
            }
            to.push(at);
        }
        if to.is_empty() || to.len() != end - s {
            continue;
        }
        for i in s..end {
            xs[g.order[i]] = to[i - s];
        }
    }
}

/// How many ends that may move along it each node has on the side its edges arrive at, and on
/// the other.
fn free_ends(g: &Graph, efk: &[End], etk: &[End]) -> [Vec<usize>; 2] {
    let mut free = [vec![0usize; g.n], vec![0usize; g.n]];
    for e in 0..g.ef.len() {
        if etk[e] != End::Held {
            free[0][g.et[e]] += 1;
        }
        if efk[e] != End::Held {
            free[1][g.ef[e]] += 1;
        }
    }
    free
}

/// How far along the flow the handles of a plain S reach, as a share of the way it travels along
/// it. The layout uses this to tell how near two curves will pass, and the drawing to draw them.
pub fn ease(across: f64, along: f64) -> f64 {
    let across = if across.abs() == 0.0 || across.is_nan() { 1.0 } else { across.abs() };
    js::max(0.25, js::min(0.5, (4.0 * along.abs()) / across))
}

/// Brings the labels and bends of each edge into line with an end of the edge. See `alignLabels`
/// in `layered.ts`.
fn align_labels(
    g: &Graph,
    nodes: &[LNode],
    xs: &mut [f64],
    free: &[Vec<usize>; 2],
    opt: &LayeredOptions,
    empty: Option<&[bool]>,
) {
    let sep = opt.edge_sep;
    let placed = |v: usize| {
        g.kind[v] == Kind::Label || (g.kind[v] == Kind::Dummy && !empty.is_some_and(|empty| empty[g.rank[v]]))
    };
    let end = |xs: &[f64], e: usize, side: usize| -> (f64, usize) {
        let u = if side == 0 { g.ef[e] } else { g.et[e] };
        let x = if side == 0 { xs[u] + g.efd[e] } else { xs[u] + g.etd[e] };
        (x, if u < g.real && nodes[u].kind == Kind::Node { free[1 - side][u] } else { 0 })
    };
    let room = |xs: &[f64], v: usize| -> (f64, f64) {
        let s = g.layer_start[g.rank[v]];
        let i = s + g.pos[v];
        let clear = |other: usize| -> f64 {
            g.w[other] / 2.0
                + g.w[v] / 2.0
                + (if g.kind[other] == Kind::Node {
                    opt.node_sep / 2.0 + sep / 2.0
                } else if g.kind[other] == g.kind[v] {
                    sep
                } else {
                    sep / 2.0
                })
        };
        (
            if i > s { xs[g.order[i - 1]] + clear(g.order[i - 1]) } else { f64::NEG_INFINITY },
            if i + 1 < g.layer_start[g.rank[v] + 1] {
                xs[g.order[i + 1]] - clear(g.order[i + 1])
            } else {
                f64::INFINITY
            },
        )
    };
    let choose = |x: f64, up: (f64, usize), down: (f64, usize), lo: f64, hi: f64| -> f64 {
        let up_first = if up.1 != down.1 { up.1 < down.1 } else { (up.0 - x).abs() <= (down.0 - x).abs() };
        let choices = if up_first { [up.0, down.0] } else { [down.0, up.0] };
        match choices.iter().find(|&&choice| choice >= lo && choice <= hi) {
            Some(&choice) => choice,
            None => js::max(lo, js::min(hi, choices[0])),
        }
    };
    let mut done = vec![false; g.n];
    let mut chain: Vec<usize> = Vec::new();
    for r in 1..g.max_rank {
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            if !placed(g.order[i]) || done[g.order[i]] {
                continue;
            }
            let mut top = g.order[i];
            while is_dummy(g.kind[g.ef[g.pred_e[g.pred_start[top]]]]) {
                top = g.ef[g.pred_e[g.pred_start[top]]];
            }
            let mut bottom = top;
            let mut lo = f64::NEG_INFINITY;
            let mut hi = f64::INFINITY;
            chain.clear();
            let mut d = top;
            while is_dummy(g.kind[d]) {
                bottom = d;
                if placed(d) {
                    done[d] = true;
                    chain.push(d);
                    let (least, most) = room(xs, d);
                    if least > lo {
                        lo = least;
                    }
                    if most < hi {
                        hi = most;
                    }
                }
                d = g.et[g.succ_e[g.succ_start[d]]];
            }
            if lo <= hi {
                let x = choose(
                    xs[chain[0]],
                    end(xs, g.pred_e[g.pred_start[top]], 0),
                    end(xs, g.succ_e[g.succ_start[bottom]], 1),
                    lo,
                    hi,
                );
                for &d in &chain {
                    xs[d] = x;
                }
                continue;
            }
            for &d in &chain {
                let (least, most) = room(xs, d);
                if least > most {
                    continue;
                }
                xs[d] = choose(
                    xs[d],
                    end(xs, follow(g, empty, g.pred_e[g.pred_start[d]], 0), 0),
                    end(xs, follow(g, empty, g.succ_e[g.succ_start[d]], 1), 1),
                    least,
                    most,
                );
            }
        }
    }
}

struct Side {
    v: usize,
    side: usize,
    span: f64,
    ids: Vec<usize>,
    room: Vec<f64>,
    all: Vec<usize>,
}

/// Where the end of segment `e` on node `v` should be, from the middle of its side. See `wanted`
/// in `layered.ts`.
#[expect(clippy::too_many_arguments, reason = "Pele's closure reads all of these from the layout around it")]
fn wanted(
    g: &Graph,
    nodes: &[LNode],
    xs: &[f64],
    efk: &[End],
    etk: &[End],
    free: &[Vec<usize>; 2],
    empty: Option<&[bool]>,
    e: usize,
    side: usize,
    v: usize,
    span: f64,
    again: bool,
) -> f64 {
    if free[side][v] < 2 {
        return 0.0;
    }
    let e = follow(g, empty, e, side);
    let u = if side == 0 { g.ef[e] } else { g.et[e] };
    let point = if side == 0 { xs[u] + g.efd[e] } else { xs[u] + g.etd[e] };
    if again && (point - xs[v]).abs() <= span / 2.0 + IN_LINE {
        return js::max(-span / 2.0, js::min(span / 2.0, point - xs[v]));
    }
    let mut middle = point;
    let mut width = 0.0;
    if u < g.real
        && nodes[u].kind == Kind::Node
        && (if side == 0 { efk } else { etk })[e] != End::Held
        && free[1 - side][u] >= 2
    {
        width = js::max(0.0, js::min(nodes[u].span, g.w[u]));
        if width > 0.0 {
            middle = xs[u];
        }
    }
    let lo = js::max(xs[v] - span / 2.0, middle - width / 2.0);
    let hi = js::min(xs[v] + span / 2.0, middle + width / 2.0);
    if lo > hi + IN_LINE {
        let lean = js::max(0.0, span / 2.0 - LEAN_CLEAR);
        return js::max(-lean, js::min(lean, point - xs[v]));
    }
    let place = (xs[v] * width + middle * span) / (width + span);
    js::max(lo, js::min(hi, place)) - xs[v]
}

/// Once the nodes have their places, moves an end along its side when that lets its edge run
/// straight. See `settle` in `layered.ts`.
fn settle(
    g: &mut Graph,
    nodes: &[LNode],
    xs: &[f64],
    efk: &[End],
    etk: &[End],
    free: &[Vec<usize>; 2],
    empty: Option<&[bool]>,
) {
    let mut sides: Vec<Side> = Vec::new();
    for v in 0..g.real {
        let span = js::min(nodes[v].span, g.w[v]);
        if span <= 0.0 || nodes[v].kind != Kind::Node {
            continue;
        }
        for side in 0..2 {
            let (other, start, list) =
                if side == 0 { (&g.ef, &g.pred_start, &g.pred_e) } else { (&g.et, &g.succ_start, &g.succ_e) };
            let mut ids = Vec::new();
            let mut room = Vec::new();
            side_ends(g, v, side, span, if side == 0 { etk } else { efk }, &mut ids, &mut room);
            if ids.is_empty() {
                continue;
            }
            if ids.len() == 1 {
                room = vec![0.0];
            }
            let mut all: Vec<usize> = (start[v]..start[v + 1]).map(|k| list[k]).collect();
            all.sort_by(|&a, &b| g.pos[other[a]].cmp(&g.pos[other[b]]).then(a.cmp(&b)));
            sides.push(Side { v, side, span, ids, room, all });
        }
    }
    let mut moved = true;
    let mut pass = 0;
    while moved && pass < 6 {
        moved = false;
        for Side { v, side, span, ids, room, all } in &sides {
            let (v, side, span) = (*v, *side, *span);
            let kinds = if side == 0 { etk } else { efk };
            let dx = |g: &Graph, e: usize| if side == 0 { g.etd[e] } else { g.efd[e] };
            let mut k = 0;
            let mut lo = -span / 2.0;
            for i in 0..=all.len() {
                let held = i < all.len() && kinds[all[i]] == End::Held;
                if i < all.len() && !held {
                    continue;
                }
                let mut size = 0;
                let mut j = i;
                while j > 0 && kinds[all[j - 1]] != End::Held {
                    size += 1;
                    j -= 1;
                }
                let hi = if held { dx(g, all[i]) - PORT_PITCH } else { span / 2.0 };
                if size > 0 {
                    let mut sum: Vec<f64> = Vec::new();
                    let mut count: Vec<f64> = Vec::new();
                    let mut before = 0.0;
                    for j in 0..size {
                        if j > 0 {
                            before += room[k + j];
                        }
                        sum.push(
                            wanted(g, nodes, xs, efk, etk, free, empty, ids[k + j], side, v, span, pass > 0) - before,
                        );
                        count.push(1.0);
                        let mut together = j > 0 && room[k + j] == 0.0;
                        while sum.len() > 1
                            && (together
                                || sum[sum.len() - 2] / count[count.len() - 2]
                                    > sum[sum.len() - 1] / count[count.len() - 1])
                        {
                            together = false;
                            let top = sum.pop().expect("two pools");
                            let pooled = count.pop().expect("two pools");
                            *sum.last_mut().expect("one pool") += top;
                            *count.last_mut().expect("one pool") += pooled;
                        }
                    }
                    let total = before;
                    let mut j = 0;
                    before = 0.0;
                    for b in 0..sum.len() {
                        let level = if hi - total < lo {
                            (lo + hi - total) / 2.0
                        } else {
                            js::max(lo, js::min(hi - total, sum[b] / count[b]))
                        };
                        for _c in 0..count[b] as usize {
                            if j > 0 {
                                before += room[k + j];
                            }
                            if (dx(g, ids[k + j]) - level - before).abs() > 0.01 {
                                moved = true;
                            }
                            let dx = if side == 0 { &mut g.etd } else { &mut g.efd };
                            dx[ids[k + j]] = level + before;
                            j += 1;
                        }
                    }
                    k += size;
                }
                if held {
                    lo = dx(g, all[i]) + PORT_PITCH;
                }
            }
        }
        pass += 1;
    }
}

/// Brings edges that are nearly straight into line. See `snap` in `layered.ts`.
fn snap(g: &Graph, xs: &mut [f64], empty: Option<&[bool]>, opt: &LayeredOptions) {
    let passed = |u: usize| empty.is_some_and(|empty| g.kind[u] == Kind::Dummy && empty[g.rank[u]]);
    let mut a: Vec<usize> = Vec::new();
    let mut b: Vec<usize> = Vec::new();
    let mut da: Vec<f64> = Vec::new();
    let mut db: Vec<f64> = Vec::new();
    for e in 0..g.ef.len() {
        if passed(g.ef[e]) {
            continue;
        }
        let f = follow(g, empty, e, 1);
        a.push(g.ef[e]);
        b.push(g.et[f]);
        da.push(g.efd[e]);
        db.push(g.etd[f]);
    }
    let off = |xs: &[f64], k: usize| xs[b[k]] + db[k] - xs[a[k]] - da[k];
    let mut block: Vec<usize> = (0..g.n).collect();
    let mut members: Vec<Vec<usize>> = (0..g.n).map(|i| vec![i]).collect();
    fn join(block: &mut [usize], members: &mut [Vec<usize>], x: usize, y: usize) {
        let mut keep = block[x];
        let mut drop = block[y];
        if keep == drop {
            return;
        }
        if members[keep].len() < members[drop].len() {
            std::mem::swap(&mut keep, &mut drop);
        }
        let moving = std::mem::take(&mut members[drop]);
        for &v in &moving {
            block[v] = keep;
        }
        members[keep].extend(moving);
    }
    let mut near: Vec<usize> = Vec::new();
    for k in 0..a.len() {
        let d = off(xs, k).abs();
        if d < 0.01 {
            join(&mut block, &mut members, a[k], b[k]);
        } else if d <= BEND_SNAP {
            near.push(k);
        }
    }
    if near.is_empty() {
        return;
    }
    near.sort_by(|&x, &y| by_difference(off(xs, x).abs() - off(xs, y).abs()).then(x.cmp(&y)));
    let gap = |v: usize| half_room(g, opt, v);
    let free = |xs: &[f64], block: &[usize], members: &[Vec<usize>], id: usize, shift: f64| -> bool {
        for &v in &members[id] {
            let s = g.layer_start[g.rank[v]];
            let end = g.layer_start[g.rank[v] + 1];
            let i = s + g.pos[v];
            if i > s
                && block[g.order[i - 1]] != id
                && xs[v] + shift - xs[g.order[i - 1]] < gap(v) + gap(g.order[i - 1]) - 0.01
            {
                return false;
            }
            if i + 1 < end
                && block[g.order[i + 1]] != id
                && xs[g.order[i + 1]] - xs[v] - shift < gap(v) + gap(g.order[i + 1]) - 0.01
            {
                return false;
            }
        }
        true
    };
    for &k in &near {
        let d = off(xs, k);
        if d.abs() < 0.01 {
            join(&mut block, &mut members, a[k], b[k]);
            continue;
        }
        if d.abs() > BEND_SNAP || block[a[k]] == block[b[k]] {
            continue;
        }
        let upper = block[a[k]];
        let lower = block[b[k]];
        let first = if members[upper].len() <= members[lower].len() { upper } else { lower };
        for id in [first, if first == upper { lower } else { upper }] {
            let shift = if id == upper { d } else { -d };
            if d.abs() > SNAP && !members[id].iter().all(|&v| g.kind[v] == Kind::Dummy) {
                continue;
            }
            if !free(xs, &block, &members, id, shift) {
                continue;
            }
            for &v in &members[id] {
                xs[v] += shift;
            }
            join(&mut block, &mut members, a[k], b[k]);
            break;
        }
    }
}

/// Marks the ranks that hold nothing but the bends of edges, if there are any.
fn empty_ranks(g: &Graph) -> Option<Vec<bool>> {
    let mut empty = vec![false; g.max_rank + 1];
    let mut any = false;
    for r in 1..g.max_rank {
        let layer = &g.order[g.layer_start[r]..g.layer_start[r + 1]];
        if !layer.is_empty() && layer.iter().all(|&v| g.kind[v] == Kind::Dummy) {
            empty[r] = true;
            any = true;
        }
    }
    any.then_some(empty)
}

/// Where each piece begins and ends across the flow, and how two pieces stand to each other.
struct Reach<'a> {
    from: &'a [f64],
    to: &'a [f64],
    lo: Vec<f64>,
    hi: Vec<f64>,
}

impl Reach<'_> {
    fn within(&self, x: f64, k: usize) -> i32 {
        i32::from(x >= self.lo[k] - IN_LINE && x <= self.hi[k] + IN_LINE)
    }
    fn mates(&self, a: usize, b: usize) -> bool {
        (self.from[a] - self.from[b]).abs() < IN_LINE || (self.to[a] - self.to[b]).abs() < IN_LINE
    }
    fn overlap(&self, a: usize, b: usize) -> bool {
        self.lo[a] < self.hi[b] - IN_LINE && self.lo[b] < self.hi[a] - IN_LINE
    }
    fn clash(&self, a: usize, b: usize) -> bool {
        self.overlap(a, b) && !self.mates(a, b)
    }
    fn meet(&self, a: usize, b: usize) -> bool {
        self.overlap(a, b)
            || self.within(self.to[a], b)
                + self.within(self.from[b], a)
                + self.within(self.to[b], a)
                + self.within(self.from[a], b)
                > 0
    }
    fn crossings(&self, a: usize, b: usize) -> i32 {
        self.within(self.to[a], b) + self.within(self.from[b], a)
    }
    /// Pairs of list indices, smaller first. Omitted pairs cannot meet or clash.
    fn near_pairs(&self, list: &[usize]) -> Vec<usize> {
        let mut by_lo: Vec<usize> = (0..list.len()).collect();
        by_lo.sort_by(|&a, &b| by_difference(self.lo[list[a]] - self.lo[list[b]]));
        let mut pairs = Vec::new();
        for x in 0..by_lo.len() {
            let i = by_lo[x];
            let reach = self.hi[list[i]] + 4.0 * IN_LINE;
            let mut y = x + 1;
            while y < by_lo.len() && self.lo[list[by_lo[y]]] <= reach {
                let j = by_lo[y];
                pairs.push(i.min(j));
                pairs.push(i.max(j));
                y += 1;
            }
        }
        pairs
    }

    /// Gives the pieces of one gap in `list` their tracks. See `lay` in `layered.ts`.
    fn lay(&self, list: &mut [usize], track: &mut [i32], levels: &mut [i32], curved: bool) -> i32 {
        let (lo, hi) = (&self.lo, &self.hi);
        list.sort_by(|&a, &b| {
            by_difference(hi[b] - lo[b] - (hi[a] - lo[a])).then_with(|| by_difference(lo[a] - lo[b])).then(a.cmp(&b))
        });
        let count = list.len();
        let mut above: Vec<Vec<usize>> = Vec::new();
        let mut group: Vec<usize> = (0..count).collect();
        fn find(group: &mut [usize], mut i: usize) -> usize {
            while group[i] != i {
                group[i] = group[group[i]];
                i = group[i];
            }
            i
        }
        let weighed = count <= MAX_WEIGHED;
        let pairs = if weighed { self.near_pairs(list) } else { Vec::new() };
        let mut clashes: Vec<Vec<usize>> = if weighed { vec![Vec::new(); count] } else { Vec::new() };
        for p in (0..pairs.len()).step_by(2) {
            let (i, j) = (pairs[p], pairs[p + 1]);
            if !self.clash(list[i], list[j]) {
                continue;
            }
            clashes[i].push(j);
            clashes[j].push(i);
        }
        let mut sequence: Vec<usize> = (0..count).collect();
        if weighed {
            let mut waiting = vec![0i32; count];
            let mut after: Vec<Vec<usize>> = vec![Vec::new(); count];
            above = vec![Vec::new(); count];
            for p in (0..pairs.len()).step_by(2) {
                let (i, j) = (pairs[p], pairs[p + 1]);
                let (a, b) = (list[i], list[j]);
                if !self.meet(a, b) {
                    continue;
                }
                let root_i = find(&mut group, i);
                let root_j = find(&mut group, j);
                group[root_i] = root_j;
                let ab = self.crossings(a, b);
                let ba = self.crossings(b, a);
                if ab == ba {
                    continue;
                }
                let (first, second) = if ab < ba { (i, j) } else { (j, i) };
                above[second].push(first);
                after[first].push(second);
                waiting[second] += 1;
            }
            let mut placed = vec![false; count];
            sequence.clear();
            for _done in 0..count {
                let mut pick: Option<usize> = None;
                for i in 0..count {
                    if placed[i] {
                        continue;
                    }
                    if pick.is_none_or(|pick| waiting[i] < waiting[pick]) {
                        pick = Some(i);
                    }
                    if waiting[i] == 0 {
                        break;
                    }
                }
                let pick = pick.expect("a piece is left to place");
                placed[pick] = true;
                sequence.push(pick);
                for &j in &after[pick] {
                    waiting[j] -= 1;
                }
            }
        }
        let mut taken: HashSet<i32> = HashSet::new();
        let mut on_track: Vec<Vec<usize>> = Vec::new();
        let mut rows = 0;
        for &at in &sequence {
            let k = list[at];
            let mut t = 0i32;
            if weighed {
                for &i in &above[at] {
                    if track[list[i]] >= t {
                        t = track[list[i]] + 1;
                    }
                }
                taken.clear();
                for &i in &clashes[at] {
                    taken.insert(track[list[i]]);
                }
                while taken.contains(&t) {
                    t += 1;
                }
            } else {
                while on_track.get(t as usize).is_some_and(|row| row.iter().any(|&other| self.clash(k, other))) {
                    t += 1;
                }
                if on_track.len() <= t as usize {
                    on_track.resize(t as usize + 1, Vec::new());
                }
                on_track[t as usize].push(k);
            }
            track[k] = t;
            if t >= rows {
                rows = t + 1;
            }
        }
        let mut first = vec![rows; count];
        let mut last = vec![-1i32; count];
        for i in 0..count {
            let root = find(&mut group, i);
            first[root] = first[root].min(track[list[i]]);
            last[root] = last[root].max(track[list[i]]);
        }
        for i in 0..count {
            let root = find(&mut group, i);
            levels[list[i]] = last[root] - first[root] + 1;
            if curved {
                track[list[i]] -= first[root];
            }
        }
        rows
    }

    fn sample(&self, k: usize, h: f64, out: &mut [f64; 2 * SAMPLES + 2]) {
        let (from, to) = (self.from[k], self.to[k]);
        let c = ease(to - from, h);
        for i in 0..=SAMPLES {
            let t = i as f64 / SAMPLES as f64;
            let u = 1.0 - t;
            out[2 * i] = from + (to - from) * t * t * (3.0 - 2.0 * t);
            out[2 * i + 1] = h * (3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t);
        }
    }

    fn near(&self, a: usize, b: usize, height: f64, limit: f64) -> bool {
        let h = js::max(height / 2.0, height - 2.0 * LEVEL_LEAD);
        let mut line_a = [0.0; 2 * SAMPLES + 2];
        let mut line_b = [0.0; 2 * SAMPLES + 2];
        self.sample(a, h, &mut line_a);
        self.sample(b, h, &mut line_b);
        reaches(&line_a, &line_b, limit) || reaches(&line_b, &line_a, limit)
    }
}

fn reaches(points: &[f64; 2 * SAMPLES + 2], line: &[f64; 2 * SAMPLES + 2], limit: f64) -> bool {
    let bound = limit * limit * (1.0 + 1e-9);
    for i in 0..=SAMPLES {
        let px = points[2 * i];
        let py = points[2 * i + 1];
        for j in 0..SAMPLES {
            let dx = line[2 * j + 2] - line[2 * j];
            let dy = line[2 * j + 3] - line[2 * j + 1];
            let length = dx * dx + dy * dy;
            let length = if length == 0.0 || length.is_nan() { 1.0 } else { length };
            let t = js::max(0.0, js::min(1.0, ((px - line[2 * j]) * dx + (py - line[2 * j + 1]) * dy) / length));
            let ex = px - line[2 * j] - t * dx;
            let ey = py - line[2 * j + 1] - t * dy;
            if ex * ex + ey * ey < bound && hypot(ex, ey) < limit {
                return true;
            }
        }
    }
    false
}

/// Gives every piece that does not run straight down a track across its gap, and counts the
/// tracks of each gap. See `assignTracks` in `layered.ts`.
fn assign_tracks(pieces: &mut Pieces, track_count: &mut [i32], curved: bool, room: &[f64], tall: &mut [f64]) {
    let Pieces { from, to, rank, track, levels, wide, .. } = pieces;
    let total = from.len();
    let mut by_rank: IndexMap<usize, Vec<usize>> = IndexMap::new();
    for k in 0..total {
        track.push(-1);
        levels.push(0);
        if (from[k] - to[k]).abs() < IN_LINE {
            continue;
        }
        by_rank.entry(rank[k]).or_default().push(k);
    }
    if by_rank.is_empty() {
        return;
    }
    let reach = Reach {
        from,
        to,
        lo: (0..total).map(|k| js::min(from[k], to[k])).collect(),
        hi: (0..total).map(|k| js::max(from[k], to[k])).collect(),
    };
    let (lo, hi) = (&reach.lo, &reach.hi);

    for (&r, all) in by_rank.iter_mut() {
        if !curved {
            track_count[r] = reach.lay(all, track, levels, curved);
            continue;
        }
        if all.len() > MAX_WEIGHED {
            continue;
        }
        let mut far: Vec<usize> = all.iter().copied().filter(|&k| wide[k]).collect();
        reach.lay(&mut far, track, levels, curved);
        let mut level: HashSet<usize> = far.iter().copied().filter(|&k| levels[k] > 1).collect();
        let pairs = reach.near_pairs(all);
        for p in (0..pairs.len()).step_by(2) {
            let a = all[pairs[p]];
            let b = all[pairs[p + 1]];
            if (level.contains(&a) && level.contains(&b))
                || !reach.clash(a, b)
                || (from[a] < to[a]) != (from[b] < to[b])
            {
                continue;
            }
            if reach.near(a, b, room[r], if wide[a] || wide[b] { LEVEL_CLEAR } else { LEVEL_TOUCH }) {
                level.insert(a);
                level.insert(b);
            }
        }
        for &k in &far {
            track[k] = -1;
            levels[k] = 0;
        }
        if level.is_empty() {
            continue;
        }
        let mut crossing = 0;
        let mut alongside = 0;
        let mut beside: HashSet<usize> = HashSet::new();
        for p in (0..pairs.len()).step_by(2) {
            let a = all[pairs[p]];
            let b = all[pairs[p + 1]];
            if level.contains(&a) && level.contains(&b) {
                if !reach.overlap(a, b) {
                    continue;
                }
                if (from[a] < to[a]) != (from[b] < to[b]) {
                    crossing += 1;
                } else if !reach.mates(a, b) {
                    alongside += 1;
                }
            } else if (level.contains(&a) || level.contains(&b)) && reach.meet(a, b) {
                beside.insert(a);
                beside.insert(b);
            }
        }
        if crossing > alongside {
            for &k in all.iter() {
                if (hi[k] - lo[k]) * CROSSED_SLOPE > tall[r] {
                    tall[r] = (hi[k] - lo[k]) * CROSSED_SLOPE;
                }
            }
            continue;
        }
        let mut members: Vec<usize> =
            all.iter().copied().filter(|k| level.contains(k) || (!wide[*k] && beside.contains(k))).collect();
        reach.lay(&mut members, track, levels, curved);
        let mut most = 0;
        for &k in &members {
            if levels[k] > most {
                most = levels[k];
            }
        }
        track_count[r] = most;
    }
}

/// Moves the bends in ranks that hold only bends onto the straight line between the nearest
/// points of their edges that are held in place. See `straighten` in `layered.ts`.
fn straighten(
    g: &Graph,
    xs: &mut [f64],
    empty: &[bool],
    first_dummy: &[Option<usize>],
    span: &[usize],
    sep: f64,
    on_line: &mut [bool],
) {
    let mut want = vec![0.0; xs.len()];
    let mut from = vec![0.0; xs.len()];
    let mut to = vec![0.0; xs.len()];
    for ei in 0..first_dummy.len() {
        let Some(d0) = first_dummy[ei] else { continue };
        let end = d0 + span[ei] - 1;
        let base = g.rank[d0];
        let mut d = d0;
        while d < end {
            if !empty[base + d - d0] {
                d += 1;
                continue;
            }
            let mut last = d;
            while last + 1 < end && empty[base + last + 1 - d0] {
                last += 1;
            }
            let above = g.pred_e[g.pred_start[d]];
            let below = g.succ_e[g.succ_start[last]];
            let x0 = xs[g.ef[above]] + g.efd[above];
            let x1 = xs[g.et[below]] + g.etd[below];
            let steps = (last - d + 2) as f64;
            for k in d..=last {
                want[k] = x0 + ((x1 - x0) * (k - d + 1) as f64) / steps;
                from[k] = x0;
                to[k] = x1;
            }
            d = last + 1;
        }
    }

    let mut twins: Vec<usize> = Vec::new();
    for r in 1..g.max_rank {
        if !empty[r] {
            continue;
        }
        let mut row: Vec<usize> = g.order[g.layer_start[r]..g.layer_start[r + 1]].to_vec();
        row.sort_by(|&a, &b| {
            by_difference(from[a] - from[b]).then_with(|| by_difference(to[a] - to[b])).then(a.cmp(&b))
        });
        let mut i = 0;
        while i < row.len() {
            twins.clear();
            let mut j = i;
            while j < row.len() && (from[row[j]] - from[row[i]]).abs() < TWIN && (to[row[j]] - to[row[i]]).abs() < TWIN
            {
                twins.push(row[j]);
                j += 1;
            }
            twins.sort_by_key(|&d| g.pos[d]);
            for k in 0..twins.len() {
                let d = twins[k];
                xs[d] = want[d] + (k as f64 - (twins.len() - 1) as f64 / 2.0) * sep;
                if twins.len() == 1 || sep == 0.0 {
                    on_line[d] = true;
                }
            }
            i = j;
        }
    }
}

/// Leaves out the bend that takes an edge between labels where the edge would pass clear of
/// every label anyway. See `pastLabels` in `layered.ts`.
fn past_labels(
    g: &Graph,
    xs: &mut [f64],
    first_dummy: &[Option<usize>],
    span: &[usize],
    clear: f64,
    on_line: &mut [bool],
) {
    let mut labelled = vec![false; g.max_rank + 1];
    let mut any = false;
    for r in 1..g.max_rank {
        let mut labels = false;
        let mut others = false;
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            let kind = g.kind[g.order[i]];
            if kind == Kind::Label {
                labels = true;
            } else if kind != Kind::Dummy {
                others = true;
            }
        }
        if labels && !others {
            labelled[r] = true;
            any = true;
        }
    }
    if !any {
        return;
    }
    for ei in 0..first_dummy.len() {
        let Some(d0) = first_dummy[ei] else { continue };
        let end = d0 + span[ei] - 1;
        for d in d0..end {
            if g.kind[d] != Kind::Dummy || !labelled[g.rank[d]] {
                continue;
            }
            let mut first = d;
            let mut last = d;
            while first > d0 && on_line[first - 1] {
                first -= 1;
            }
            while last + 1 < end && on_line[last + 1] {
                last += 1;
            }
            let above = g.pred_e[g.pred_start[first]];
            let below = g.succ_e[g.succ_start[last]];
            let from = xs[g.ef[above]] + g.efd[above];
            let to = xs[g.et[below]] + g.etd[below];
            let lo = js::min(from, to) - clear;
            let hi = js::max(from, to) + clear;
            let r = g.rank[d];
            let free = g.order[g.layer_start[r]..g.layer_start[r + 1]].iter().all(|&label| {
                !(g.kind[label] == Kind::Label
                    && xs[label] + g.w[label] / 2.0 > lo
                    && xs[label] - g.w[label] / 2.0 < hi)
            });
            if !free {
                continue;
            }
            on_line[d] = true;
            let steps = (last - first + 2) as f64;
            for k in first..=last {
                xs[k] = from + ((to - from) * (k - first + 1) as f64) / steps;
            }
        }
    }
}

/// The state the crossing sweeps share. See `reduceCrossings` in `layered.ts`.
struct Sweeps<'a> {
    g: &'a mut Graph,
    nodes: &'a [LNode],
    /// With offsets, the place each edge takes among those that reach its node.
    place: Option<Vec<usize>>,
    place_in: Vec<usize>,
    places: Vec<usize>,
    leaving: Vec<usize>,
    mixed: Vec<bool>,
    tree: Vec<u64>,
    targets: Vec<usize>,
    bary: Vec<f64>,
    moving: Vec<usize>,
    spare: Vec<usize>,
    depth: Vec<usize>,
    group_sum: Vec<f64>,
    group_count: Vec<usize>,
    group_first: Vec<usize>,
}

impl Sweeps<'_> {
    fn count(&mut self) -> u64 {
        let g = &*self.g;
        let targets = &mut self.targets;
        let mut total = 0u64;
        for r in 0..g.max_rank {
            let mut south_len = g.layer_start[r + 2] - g.layer_start[r + 1];
            if let Some(place) = &mut self.place {
                let mut next = 0;
                for i in g.layer_start[r + 1]..g.layer_start[r + 2] {
                    let v = g.order[i];
                    for k in g.pred_start[v]..g.pred_start[v + 1] {
                        place[g.pred_e[k]] = next + self.place_in[g.pred_e[k]];
                    }
                    next += self.places[v];
                }
                south_len = next;
            }
            if south_len < 2 {
                continue;
            }
            let mut t = 0;
            for i in g.layer_start[r]..g.layer_start[r + 1] {
                let v = g.order[i];
                let from = g.succ_start[v];
                let to = g.succ_start[v + 1];
                let t0 = t;
                for k in from..to {
                    targets[t] = match &self.place {
                        Some(place) => place[self.leaving[k]],
                        None => g.pos[g.et[g.succ_e[k]]],
                    };
                    t += 1;
                }
                let d = t - t0;
                if d < 2 {
                    continue;
                }
                if self.place.is_some() && self.mixed[v] {
                    for a in t0 + 1..t {
                        let item = targets[a];
                        let dx = g.efd[self.leaving[from + a - t0]];
                        let mut b = a;
                        while b > t0 && g.efd[self.leaving[from + b - 1 - t0]] == dx && targets[b - 1] > item {
                            targets[b] = targets[b - 1];
                            b -= 1;
                        }
                        targets[b] = item;
                    }
                } else if d <= 12 {
                    for a in t0 + 1..t {
                        let item = targets[a];
                        let mut b = a;
                        while b > t0 && targets[b - 1] > item {
                            targets[b] = targets[b - 1];
                            b -= 1;
                        }
                        targets[b] = item;
                    }
                } else {
                    targets[t0..t].sort_unstable();
                }
            }
            if t < 2 {
                continue;
            }
            let mut leaves = 1;
            while leaves < south_len {
                leaves <<= 1;
            }
            self.tree[..2 * leaves].fill(0);
            for k in 0..t {
                let mut i = targets[k] + leaves;
                self.tree[i] += 1;
                while i > 1 {
                    if i & 1 == 0 {
                        total += self.tree[i + 1];
                    }
                    i >>= 1;
                    self.tree[i] += 1;
                }
            }
        }
        total
    }

    /// Keeps groups contiguous in rank `r`, ordering each by its mean key.
    fn arrange(&mut self, r: usize, key: &[f64]) {
        let g = &mut *self.g;
        let n = g.n;
        let grp = g.grp.as_ref().expect("arrange runs with groups");
        let parent = g.group_parent.as_ref().expect("groups have parents");
        let s = g.layer_start[r];
        let e = g.layer_start[r + 1];
        for i in s..e {
            let mut gr = grp[g.order[i]];
            while gr >= 0 {
                self.group_sum[gr as usize] = 0.0;
                self.group_count[gr as usize] = 0;
                gr = parent[gr as usize];
            }
        }
        for i in s..e {
            let v = g.order[i];
            let mut gr = grp[v];
            while gr >= 0 {
                let at = gr as usize;
                if self.group_count[at] == 0 {
                    self.group_first[at] = i;
                }
                self.group_sum[at] += key[v];
                self.group_count[at] += 1;
                gr = parent[at];
            }
        }
        let (group_sum, group_count, group_first, depth, pos) =
            (&self.group_sum, &self.group_count, &self.group_first, &self.depth, &g.pos);
        let value =
            |item: usize| if item >= n { group_sum[item - n] / group_count[item - n] as f64 } else { key[item] };
        let first = |item: usize| if item >= n { group_first[item - n] } else { s + pos[item] };
        let compare = |a: usize, b: usize| -> Ordering {
            let (mut ia, mut ib) = (a, b);
            let (mut ca, mut cb) = (grp[a], grp[b]);
            let mut da = if ca < 0 { 0 } else { depth[ca as usize] };
            let mut db = if cb < 0 { 0 } else { depth[cb as usize] };
            while da > db {
                ia = n + ca as usize;
                ca = parent[ca as usize];
                da -= 1;
            }
            while db > da {
                ib = n + cb as usize;
                cb = parent[cb as usize];
                db -= 1;
            }
            while ca != cb {
                ia = n + ca as usize;
                ca = parent[ca as usize];
                ib = n + cb as usize;
                cb = parent[cb as usize];
            }
            by_difference(value(ia) - value(ib)).then_with(|| first(ia).cmp(&first(ib)))
        };
        let mut row = g.order[s..e].to_vec();
        row.sort_by(|&a, &b| compare(a, b));
        g.order[s..e].copy_from_slice(&row);
        for i in s..e {
            g.pos[g.order[i]] = i - s;
        }
    }

    fn sweep(&mut self, down: bool, bias_right: bool) {
        for step in 1..=self.g.max_rank {
            let g = &mut *self.g;
            let (start, list, other, other_dx) = if down {
                (&g.pred_start, &g.pred_e, &g.ef, &g.efd)
            } else {
                (&g.succ_start, &g.succ_e, &g.et, &g.etd)
            };
            let r = if down { step } else { g.max_rank - step };
            if g.fixed[r] {
                continue;
            }
            let s = g.layer_start[r];
            let e = g.layer_start[r + 1];
            let mut movable = 0;
            for i in s..e {
                let v = g.order[i];
                let pin = if v < g.real { self.nodes[v].pin } else { 0 };
                if pin != 0 {
                    self.bary[v] = f64::from(pin) * 1e9;
                    self.moving[movable] = v;
                    movable += 1;
                    continue;
                }
                let from = start[v];
                let to = start[v + 1];
                if from == to {
                    if g.grp.is_some() {
                        let across = if down {
                            g.layer_start[r] - g.layer_start[r - 1]
                        } else {
                            g.layer_start[r + 2] - g.layer_start[r + 1]
                        };
                        self.bary[v] = ((i - s) as f64 * across as f64) / (e - s) as f64;
                        continue;
                    }
                    self.bary[v] = -1.0;
                    continue;
                }
                let mut sum = 0.0;
                for k in from..to {
                    let u = other[list[k]];
                    sum += g.pos[u] as f64;
                    if g.offsets && g.w[u] > 0.0 {
                        sum += other_dx[list[k]] / g.w[u];
                    }
                }
                self.bary[v] = sum / (to - from) as f64;
                self.moving[movable] = v;
                movable += 1;
            }
            if g.grp.is_some() {
                let bary = std::mem::take(&mut self.bary);
                self.arrange(r, &bary);
                self.bary = bary;
                continue;
            }
            if movable < 2 {
                continue;
            }
            sort_by_bary(&mut self.moving, movable, &self.bary, &g.pos, bias_right, &mut self.spare);
            let mut k = 0;
            for i in s..e {
                if self.bary[g.order[i]] != -1.0 {
                    g.order[i] = self.moving[k];
                    k += 1;
                }
            }
            for i in s..e {
                g.pos[g.order[i]] = i - s;
            }
        }
    }

    /// How many pairs of nodes in one rank are not in the order they were given.
    fn out_of_order(&self) -> u64 {
        let g = &*self.g;
        let mut total = 0;
        for r in 0..=g.max_rank {
            for i in g.layer_start[r]..g.layer_start[r + 1] {
                if g.order[i] >= g.real {
                    continue;
                }
                for j in i + 1..g.layer_start[r + 1] {
                    if g.order[j] < g.order[i] {
                        total += 1;
                    }
                }
            }
        }
        total
    }

    fn pin_of(&self, v: usize) -> i32 {
        if v < self.g.real { self.nodes[v].pin } else { 0 }
    }

    fn at(&self, e: usize, up: bool) -> f64 {
        let g = &*self.g;
        let u = if up { g.ef[e] } else { g.et[e] };
        if g.offsets && g.w[u] > 0.0 {
            g.pos[u] as f64 + (if up { g.efd[e] } else { g.etd[e] }) / g.w[u]
        } else {
            g.pos[u] as f64
        }
    }

    /// Crossings between a and b, assuming a is left of b.
    fn between(&self, a: usize, b: usize) -> u64 {
        let g = &*self.g;
        let mut c = 0;
        for k in g.pred_start[a]..g.pred_start[a + 1] {
            let x = self.at(g.pred_e[k], true);
            for j in g.pred_start[b]..g.pred_start[b + 1] {
                if x > self.at(g.pred_e[j], true) {
                    c += 1;
                }
            }
        }
        for k in g.succ_start[a]..g.succ_start[a + 1] {
            let x = self.at(g.succ_e[k], false);
            for j in g.succ_start[b]..g.succ_start[b + 1] {
                if x > self.at(g.succ_e[j], false) {
                    c += 1;
                }
            }
        }
        c
    }

    fn transpose(&mut self) {
        for _pass in 0..8 {
            let mut improved = false;
            for r in 0..=self.g.max_rank {
                if self.g.fixed[r] {
                    continue;
                }
                let s = self.g.layer_start[r];
                let mut i = s;
                while i + 1 < self.g.layer_start[r + 1] {
                    let a = self.g.order[i];
                    let b = self.g.order[i + 1];
                    let apart =
                        self.pin_of(a) != self.pin_of(b) || self.g.grp.as_ref().is_some_and(|grp| grp[a] != grp[b]);
                    if !apart && self.between(b, a) < self.between(a, b) {
                        self.g.order[i] = b;
                        self.g.order[i + 1] = a;
                        self.g.pos[b] = i - s;
                        self.g.pos[a] = i + 1 - s;
                        improved = true;
                    }
                    i += 1;
                }
            }
            if !improved {
                break;
            }
        }
    }
}

/// Returns (final crossings, crossings before grouping or reordering).
fn reduce_crossings(g: &mut Graph, nodes: &[LNode]) -> (u64, u64) {
    let n = g.n;
    let max_rank = g.max_rank;
    let segments = g.ef.len();
    let mut widest = 0;
    for r in 0..=max_rank {
        widest = widest.max(g.layer_start[r + 1] - g.layer_start[r]);
    }
    if widest < 2 {
        return (0, 0);
    }
    let offsets = g.offsets;
    let mut place_in = vec![0usize; if offsets { segments } else { 0 }];
    let mut places = vec![0usize; if offsets { n } else { 0 }];
    let mut leaving = if offsets { g.succ_e.clone() } else { Vec::new() };
    let mut mixed = vec![false; if offsets { n } else { 0 }];
    if offsets {
        let mut arriving = g.pred_e.clone();
        let by_offset = |list: &mut [usize], from: usize, to: usize, dx: &[f64]| -> bool {
            for k in from + 1..to {
                if dx[list[k]] == dx[list[from]] {
                    continue;
                }
                list[from..to].sort_by(|&a, &b| by_difference(dx[a] - dx[b]));
                return true;
            }
            false
        };
        for v in 0..n {
            places[v] = 1;
            if by_offset(&mut arriving, g.pred_start[v], g.pred_start[v + 1], &g.etd) {
                for k in g.pred_start[v] + 1..g.pred_start[v + 1] {
                    if g.etd[arriving[k]] != g.etd[arriving[k - 1]] {
                        places[v] += 1;
                    }
                    place_in[arriving[k]] = places[v] - 1;
                }
            }
            if by_offset(&mut leaving, g.succ_start[v], g.succ_start[v + 1], &g.efd) {
                mixed[v] = true;
            }
        }
    }
    let mut size = 1;
    while size < (if offsets { widest + segments } else { widest }) {
        size <<= 1;
    }

    let group_count = g.group_parent.as_ref().map_or(0, Vec::len);
    let mut depth = vec![0usize; group_count];
    if let Some(parent) = &g.group_parent {
        for gr in 0..group_count {
            let mut p = gr as i32;
            while p >= 0 {
                depth[gr] += 1;
                p = parent[p as usize];
            }
        }
    }
    let grouped = g.grp.is_some();
    let real = g.real;
    let mut sweeps = Sweeps {
        g,
        nodes,
        place: offsets.then(|| vec![0usize; segments]),
        place_in,
        places,
        leaving,
        mixed,
        tree: vec![0; 2 * size],
        targets: vec![0; segments],
        bary: vec![0.0; n],
        moving: vec![0; widest],
        spare: vec![0; widest],
        depth,
        group_sum: vec![0.0; group_count],
        group_count: vec![0; group_count],
        group_first: vec![0; group_count],
    };

    let declared = sweeps.count();
    if grouped {
        let mut at = vec![0.0; n];
        for r in 0..=max_rank {
            for i in sweeps.g.layer_start[r]..sweeps.g.layer_start[r + 1] {
                at[sweeps.g.order[i]] = (i - sweeps.g.layer_start[r]) as f64;
            }
            sweeps.arrange(r, &at);
        }
    }
    let mut best = if grouped { sweeps.count() } else { declared };
    if best == 0 {
        return (0, declared);
    }
    let mut best_order = sweeps.g.order.clone();
    let start = best.min(declared);

    let initial = sweeps.g.order.clone();
    let small = n <= 600;
    let mut chosen = best_order.clone();
    let mut chosen_out = u64::MAX;
    for attempt in 0..(if small { 2 } else { 1 }) {
        if attempt > 0 {
            sweeps.g.order.copy_from_slice(&initial);
            sweeps.g.set_positions();
        }
        let mut found = start;
        let mut found_order = initial.clone();
        let mut i = 0;
        let mut stale = 0;
        while stale < 4 && i < MAX_SWEEPS {
            sweeps.sweep((i + attempt) % 2 == 0, i % 4 >= 2);
            let c = sweeps.count();
            if c < found {
                found = c;
                found_order.copy_from_slice(&sweeps.g.order);
                stale = 0;
                if c == 0 {
                    break;
                }
            }
            i += 1;
            stale += 1;
        }
        sweeps.g.order.copy_from_slice(&found_order);
        if small && found > 0 {
            sweeps.g.set_positions();
            sweeps.transpose();
            let c = sweeps.count();
            if c < found {
                found = c;
                found_order.copy_from_slice(&sweeps.g.order);
            }
        }
        let out = if small { sweeps.out_of_order() } else { 0 };
        if attempt == 0 || found < best || (found == best && out < chosen_out) {
            best = found;
            chosen = found_order;
            chosen_out = out;
        }
    }
    sweeps.g.order.copy_from_slice(&chosen);
    sweeps.g.set_positions();
    best_order.copy_from_slice(&chosen);
    if grouped {
        best = sweeps.count();
    }

    if !small {
        return (best, declared);
    }
    if nodes[..real].iter().any(|node| node.pin != 0) {
        return (best, declared);
    }
    if chosen_out == 0 || sweeps.g.fixed[0] || sweeps.g.fixed[max_rank] {
        return (best, declared);
    }
    for r in 0..=max_rank {
        let (s, e) = (sweeps.g.layer_start[r], sweeps.g.layer_start[r + 1]);
        sweeps.g.order[s..e].reverse();
    }
    sweeps.g.set_positions();
    if sweeps.count() > best || sweeps.out_of_order() >= chosen_out {
        sweeps.g.order.copy_from_slice(&best_order);
    }
    (best, declared)
}

/// Half the room a node, bend or port takes across the flow, with its share of the gap beside it.
fn half_room(g: &Graph, opt: &LayeredOptions, v: usize) -> f64 {
    g.w[v] / 2.0 + (if g.kind[v] == Kind::Node { opt.node_sep } else { opt.edge_sep }) / 2.0
}

/// Brandes-Köpf horizontal coordinate assignment: four aligned layouts, balanced.
fn position(g: &mut Graph, opt: &LayeredOptions) -> Vec<f64> {
    let n = g.n;
    let max_rank = g.max_rank;
    let segments = g.ef.len();
    let gap: Vec<f64> = (0..n).map(|i| half_room(g, opt, i)).collect();

    let mut key = vec![0usize; segments];
    for e in 0..segments {
        key[e] = g.pos[g.et[e]];
    }
    for v in 0..n {
        if g.succ_start[v + 1] - g.succ_start[v] > 1 {
            sort_range(&mut g.succ_e, g.succ_start[v], g.succ_start[v + 1], &key);
        }
    }
    for e in 0..segments {
        key[e] = g.pos[g.ef[e]];
    }
    for v in 0..n {
        if g.pred_start[v + 1] - g.pred_start[v] > 1 {
            sort_range(&mut g.pred_e, g.pred_start[v], g.pred_start[v + 1], &key);
        }
    }
    let g = &*g;

    let mut conflict = vec![false; segments];
    for r in 1..=max_rank {
        let prev_len = g.layer_start[r] - g.layer_start[r - 1];
        let s = g.layer_start[r];
        let end = g.layer_start[r + 1];
        let mut k0 = 0;
        let mut scan = s;
        for i in s..end {
            let v = g.order[i];
            let mut inner = None;
            if is_dummy(g.kind[v]) {
                let u = g.ef[g.pred_e[g.pred_start[v]]];
                if is_dummy(g.kind[u]) {
                    inner = Some(u);
                }
            }
            let k1 = inner.map_or(prev_len, |inner| g.pos[inner]);
            if inner.is_some() || i == end - 1 {
                for j in scan..=i {
                    let node = g.order[j];
                    let node_dummy = is_dummy(g.kind[node]);
                    for k in g.pred_start[node]..g.pred_start[node + 1] {
                        let e = g.pred_e[k];
                        let up = g.pos[g.ef[e]];
                        if (up < k0 || k1 < up) && !(node_dummy && is_dummy(g.kind[g.ef[e]])) {
                            conflict[e] = true;
                        }
                    }
                }
                scan = i + 1;
                k0 = k1;
            }
        }
    }

    let mut results: Vec<Vec<f64>> = Vec::new();
    let mut root = vec![0usize; n];
    let mut align = vec![0usize; n];
    let mut shift = vec![0.0; n];
    let mut e_from = vec![0usize; n];
    let mut e_to = vec![0usize; n];
    let mut e_w = vec![0.0; n];
    let mut indeg = vec![0usize; n];
    let mut out_start = vec![0usize; n + 1];
    let mut out_e = vec![0usize; n];
    let mut fill = vec![0usize; n];
    let mut topo = vec![0usize; n];
    let mut x = vec![0.0; n];

    for variant in 0..4 {
        let up = variant < 2;
        let right = variant & 1 == 1;
        let (start, list, other, own_dx, other_dx) = if up {
            (&g.pred_start, &g.pred_e, &g.ef, &g.etd, &g.efd)
        } else {
            (&g.succ_start, &g.succ_e, &g.et, &g.efd, &g.etd)
        };
        for i in 0..n {
            root[i] = i;
            align[i] = i;
            shift[i] = 0.0;
            indeg[i] = 0;
            out_start[i + 1] = 0;
            x[i] = 0.0;
        }

        for step in 0..=max_rank {
            let r = if up { step } else { max_rank - step };
            let s = g.layer_start[r];
            let end = g.layer_start[r + 1];
            let adjacent = if up { r as i64 - 1 } else { r as i64 + 1 };
            let adjacent_last = if adjacent >= 0 && adjacent <= max_rank as i64 {
                let adjacent = adjacent as usize;
                (g.layer_start[adjacent + 1] - g.layer_start[adjacent]) as i64 - 1
            } else {
                0
            };
            let mut prev_idx: i64 = -1;
            for across in 0..end - s {
                let i = if right { end - 1 - across } else { s + across };
                let v = g.order[i];
                let from = start[v];
                let d = start[v + 1] - from;
                if d == 0 {
                    continue;
                }
                let lo = from + ((d - 1) >> 1);
                let hi = from + (d >> 1);
                for pass in 0..2 {
                    if pass == 1 && lo == hi {
                        break;
                    }
                    let e = list[if (pass == 0) != right { lo } else { hi }];
                    let node = other[e];
                    let idx = if right { adjacent_last - g.pos[node] as i64 } else { g.pos[node] as i64 };
                    if align[v] == v && prev_idx < idx && !conflict[e] {
                        align[node] = v;
                        root[v] = root[node];
                        align[v] = root[v];
                        shift[v] = shift[node] + other_dx[e] - own_dx[e];
                        prev_idx = idx;
                    }
                }
            }
        }

        let mut block_edges = 0;
        for r in 0..=max_rank {
            let s = g.layer_start[r];
            let end = g.layer_start[r + 1];
            for i in s + 1..end {
                let a = if right { g.order[i] } else { g.order[i - 1] };
                let b = if right { g.order[i - 1] } else { g.order[i] };
                let sep = gap[a] + gap[b];
                e_from[block_edges] = root[a];
                e_to[block_edges] = root[b];
                e_w[block_edges] = if right { sep - shift[a] + shift[b] } else { sep + shift[a] - shift[b] };
                indeg[root[b]] += 1;
                out_start[root[a] + 1] += 1;
                block_edges += 1;
            }
        }
        for i in 0..n {
            out_start[i + 1] += out_start[i];
        }
        fill[..n].copy_from_slice(&out_start[..n]);
        for e in 0..block_edges {
            out_e[fill[e_from[e]]] = e;
            fill[e_from[e]] += 1;
        }

        let mut count = 0;
        for i in 0..n {
            if root[i] == i && indeg[i] == 0 {
                topo[count] = i;
                count += 1;
            }
        }
        let mut q = 0;
        while q < count {
            let v = topo[q];
            for k in out_start[v]..out_start[v + 1] {
                let e = out_e[k];
                let next = e_to[e];
                if x[v] + e_w[e] > x[next] {
                    x[next] = x[v] + e_w[e];
                }
                indeg[next] -= 1;
                if indeg[next] == 0 {
                    topo[count] = next;
                    count += 1;
                }
            }
            q += 1;
        }
        for q in (0..count).rev() {
            let v = topo[q];
            let mut min = f64::INFINITY;
            for k in out_start[v]..out_start[v + 1] {
                let e = out_e[k];
                let cand = x[e_to[e]] - e_w[e];
                if cand < min {
                    min = cand;
                }
            }
            if min != f64::INFINITY && min > x[v] {
                x[v] = min;
            }
        }
        results.push((0..n).map(|i| (if right { -x[root[i]] } else { x[root[i]] }) + shift[i]).collect());
    }

    let mut narrowest = 0;
    let mut best_width = f64::INFINITY;
    let mut mins: Vec<f64> = Vec::new();
    let mut maxs: Vec<f64> = Vec::new();
    for (k, xs) in results.iter().enumerate() {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..n {
            let half = g.w[i] / 2.0;
            if xs[i] - half < lo {
                lo = xs[i] - half;
            }
            if xs[i] + half > hi {
                hi = xs[i] + half;
            }
        }
        mins.push(lo);
        maxs.push(hi);
        if hi - lo < best_width {
            best_width = hi - lo;
            narrowest = k;
        }
    }
    for (k, xs) in results.iter_mut().enumerate() {
        let delta = if k & 1 == 0 { mins[narrowest] - mins[k] } else { maxs[narrowest] - maxs[k] };
        if delta != 0.0 {
            for value in xs.iter_mut() {
                *value += delta;
            }
        }
    }

    let mut out = vec![0.0; n];
    for i in 0..n {
        let mut a = results[0][i];
        let mut b = results[1][i];
        let mut c = results[2][i];
        let mut d = results[3][i];
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if c > d {
            std::mem::swap(&mut c, &mut d);
        }
        if a > c {
            std::mem::swap(&mut a, &mut c);
        }
        if b > d {
            std::mem::swap(&mut b, &mut d);
        }
        if b > c {
            std::mem::swap(&mut b, &mut c);
        }
        out[i] = (b + c) / 2.0;
    }
    out
}
