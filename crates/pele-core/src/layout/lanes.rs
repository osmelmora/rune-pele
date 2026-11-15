//! Layout for swimlanes, ported from Pele's `layout/lanes.ts`. Every top-level group is a lane: a
//! band that runs the length of the diagram along the flow. Ranks are shared by all lanes, so a
//! step sits level with the steps that happen at the same time in other lanes, and each node
//! stays inside its own lane.
//!
//! As in `layered`, every sum, sort and loop keeps Pele's order, and names follow Pele's.

use super::compound::{Axis, CEdge, CNode, Dir, RoutePoint, Size};
use super::layered::{Kind, LEdge, LNode, LayeredOptions, orient, rank};
use super::num::by_difference;
use crate::js;
use std::collections::HashSet;

const LANE_HEAD: f64 = 34.0;
/// The room a lane keeps around its items unless the caller asks for less.
pub const LANE_PAD: f64 = 24.0;
const SWEEPS: usize = 8;
const MAX_DUMMIES: i64 = 20000;
const MAX_HOPS: usize = 10000;
const ROUTE_BUDGET: f64 = 2_000_000.0;
// Parallel lines closer than this are drawn as one.
const NEAR: f64 = 4.0;
const ROUNDS: usize = 3;
const RELAY: f64 = 8.0;
const TURN: f64 = 20.0;
// The room a lane keeps beside a handoff along its margin, and how much of it is between the
// handoff and the lane's items.
const MARGIN: f64 = 20.0;
const LEG: f64 = 14.0;
const CLOSE: f64 = 3.0;

/// `Math.sign`
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// What a lane layout knows before it places anything: the lanes, the items it ranks and the
/// relations between them.
struct Lanes {
    sideways: bool,
    flipped: bool,
    /// The top-level groups in the order they were written.
    lanes: Vec<usize>,
    lane_count: usize,
    lane_slot: Vec<Option<usize>>,
    lane_pads: Vec<f64>,
    end_pad: f64,
    /// Each item's node: nodes, and groups that are empty and so act as nodes.
    real: Vec<usize>,
    item_of: Vec<Option<usize>>,
    item_lane: Vec<i64>,
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    /// Each ledge's edge.
    edge_of: Vec<usize>,
    step: i32,
}

/// What one placement leaves besides the nodes and edges it moves.
struct Placed {
    size: Size,
    routed: Vec<bool>,
    margins: Vec<bool>,
    work: f64,
}

/// Pele's `laneLayout`. `lane_pad` is the room a lane keeps around its items.
pub fn lane_layout(nodes: &mut [CNode], edges: &mut [CEdge], dir: Dir, opt: &LayeredOptions, lane_pad: f64) -> Size {
    let n = nodes.len();
    if n == 0 {
        return Size { width: 0.0, height: 0.0 };
    }
    let sideways = matches!(dir, Dir::LR | Dir::RL);
    let flipped = matches!(dir, Dir::BT | Dir::RL);

    // Lanes in the order they were written. The graph lists groups last-declared first.
    let lanes: Vec<usize> = (0..n).rev().filter(|&i| nodes[i].parent.is_none() && nodes[i].is_group).collect();
    let lane_count = lanes.len().max(1);
    let mut lane_slot = vec![None; n];
    for (k, &id) in lanes.iter().enumerate() {
        lane_slot[id] = Some(k);
    }
    let mut has_child = vec![false; n];
    for node in nodes.iter() {
        if let Some(parent) = node.parent {
            has_child[parent] = true;
        }
    }

    let mut item_of = vec![None; n];
    let mut real = Vec::new();
    for i in 0..n {
        if lane_slot[i].is_some() {
            continue;
        }
        if !nodes[i].is_group || !has_child[i] {
            item_of[i] = Some(real.len());
            real.push(i);
        }
    }
    let along = |c: &CNode| if sideways { c.w } else { c.h };
    let cross = |c: &CNode| if sideways { c.h } else { c.w };

    let mut lnodes: Vec<LNode> =
        real.iter().map(|&i| LNode::new(cross(&nodes[i]), along(&nodes[i]), Kind::Node)).collect();
    let mut ledges: Vec<LEdge> = Vec::new();
    let mut edge_of = Vec::new();
    let mut step = 1;
    {
        let mut seen = HashSet::new();
        for (ei, e) in edges.iter().enumerate() {
            let (Some(a), Some(b)) = (item_of[e.src], item_of[e.dst]) else {
                continue;
            };
            if a == b {
                continue;
            }
            let key = a.min(b) * real.len() + a.max(b);
            if e.label_w > 0.0 || seen.contains(&key) {
                step = 2;
            }
            seen.insert(key);
            let mut ledge = LEdge::new(a, b, e.minlen);
            ledge.label_w = if sideways { e.label_h } else { e.label_w };
            ledge.label_h = if sideways { e.label_w } else { e.label_h };
            ledges.push(ledge);
            edge_of.push(ei);
        }
    }
    orient(&lnodes, &mut ledges);
    rank(&mut lnodes, &ledges, step);

    // Room a lane keeps around its items, so that the groups drawn around them stay inside it.
    let mut lane_pads = vec![lane_pad; lane_count];
    let mut end_pad = lane_pad;
    let item_lane: Vec<i64> = real
        .iter()
        .map(|&i| {
            let mut t = i;
            let mut room = lane_pad;
            let mut hops = 0;
            while let Some(parent) = nodes[t].parent.filter(|_| hops < MAX_HOPS) {
                t = parent;
                if lane_slot[t].is_none() {
                    room += js::max(js::max(nodes[t].pad_x, nodes[t].pad_top), nodes[t].pad_bottom);
                }
                hops += 1;
            }
            let lane = lane_slot[t].unwrap_or(0);
            if room > lane_pads[lane] {
                lane_pads[lane] = room;
            }
            if room > end_pad {
                end_pad = room;
            }
            lane as i64
        })
        .collect();

    let layout = Lanes {
        sideways,
        flipped,
        lanes,
        lane_count,
        lane_slot,
        lane_pads,
        end_pad,
        real,
        item_of,
        item_lane,
        lnodes,
        ledges,
        edge_of,
        step,
    };
    let m = layout.ledges.len();
    let mut plain = vec![false; m];
    let mut margins = vec![false; lane_count];
    let mut placed = layout.place(nodes, edges, opt, &plain, &margins);
    // A handoff that was given a simple route needs no room kept for it through the ranks it
    // passes, so the lanes are laid out again without it. A handoff that then loses its route
    // goes back to the ranked one. Routing is most of the work, so a large graph is laid out once.
    let mut round = 0;
    while round < ROUNDS && placed.work <= ROUTE_BUDGET / RELAY {
        let next: Vec<bool> = if round == 0 {
            placed.routed.clone()
        } else {
            plain.iter().zip(&placed.routed).map(|(&on, &routed)| on && routed).collect()
        };
        if next == plain && placed.margins == margins {
            break;
        }
        plain = next;
        margins = placed.margins;
        placed = layout.place(nodes, edges, opt, &plain, &margins);
        round += 1;
    }
    if plain.iter().zip(&placed.routed).any(|(&on, &routed)| on && !routed) {
        placed = layout.place(nodes, edges, opt, &vec![false; m], &vec![false; lane_count]);
    }
    placed.size
}

impl Lanes {
    /// Lays the ranked items out and routes the relations. A plain relation is a handoff that
    /// takes a simple route of its own, and so keeps no room through the ranks it passes.
    fn place(
        &self,
        nodes: &mut [CNode],
        edges: &mut [CEdge],
        opt: &LayeredOptions,
        plain: &[bool],
        margins: &[bool],
    ) -> Placed {
        let Lanes {
            sideways, flipped, lanes, lane_count, lane_slot, end_pad, real, item_of, lnodes, ledges, step, ..
        } = self;
        let (sideways, flipped, lane_count, end_pad, step) = (*sideways, *flipped, *lane_count, *end_pad, *step);
        let n = nodes.len();
        let m = ledges.len();
        let pad: Vec<f64> = self
            .lane_pads
            .iter()
            .enumerate()
            .map(|(lane, &room)| if margins[lane] { js::max(room, MARGIN) } else { room })
            .collect();
        let mut used = vec![false; lane_count];
        // Items: the real ones, then a dummy for every rank an edge passes through.
        let mut dummies: i64 = 0;
        let mut span = vec![0i32; m];
        for k in 0..m {
            span[k] = (lnodes[ledges[k].head].rank - lnodes[ledges[k].tail].rank).abs();
            if !plain[k] {
                dummies += i64::from(span[k]) - 1;
            }
        }
        let direct = dummies > MAX_DUMMIES;
        let total = real.len() + if direct { 0 } else { usize::try_from(dummies).unwrap_or(0) };
        let mut rank_of = vec![0i32; total];
        let mut lane_of = vec![0i64; total];
        let mut size = vec![0.0f64; total];
        let mut depth = vec![0.0f64; total];
        let mut dummy = vec![false; total];
        let mut max_rank = 0;
        for k in 0..real.len() {
            rank_of[k] = lnodes[k].rank;
            lane_of[k] = self.item_lane[k];
            size[k] = lnodes[k].w;
            depth[k] = lnodes[k].h;
            if rank_of[k] > max_rank {
                max_rank = rank_of[k];
            }
        }
        let mut first_dummy: Vec<Option<usize>> = vec![None; m];
        let mut label_item: Vec<Option<usize>> = vec![None; m];
        let mut links: Vec<usize> = Vec::new();
        {
            let mut next = real.len();
            for k in 0..m {
                let e = &ledges[k];
                let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
                if direct || span[k] < 2 || plain[k] {
                    links.extend([a, b]);
                    continue;
                }
                let ra = rank_of[a];
                let rb = rank_of[b];
                let mut label_rank = -1;
                if e.label_w > 0.0 {
                    label_rank = ra + ((rb - ra) >> 1);
                    if step == 2 && (label_rank & 1) == 0 {
                        label_rank -= 1;
                    }
                }
                first_dummy[k] = Some(next);
                let mut prev = a;
                for r in ra + 1..rb {
                    let d = next;
                    next += 1;
                    rank_of[d] = r;
                    // The edge crosses from its first lane to its last one evenly along the way.
                    let share = ((lane_of[b] - lane_of[a]) * i64::from(r - ra)) as f64 / f64::from(rb - ra);
                    lane_of[d] = lane_of[a] + js::round(share) as i64;
                    dummy[d] = true;
                    if r == label_rank {
                        size[d] = e.label_w;
                        depth[d] = e.label_h;
                        label_item[k] = Some(d);
                    }
                    links.extend([prev, d]);
                    prev = d;
                }
                links.extend([prev, b]);
            }
        }

        let mut up_start = vec![0usize; total + 1];
        let mut down_start = vec![0usize; total + 1];
        for pair in links.chunks(2) {
            down_start[pair[0] + 1] += 1;
            up_start[pair[1] + 1] += 1;
        }
        for i in 0..total {
            up_start[i + 1] += up_start[i];
            down_start[i + 1] += down_start[i];
        }
        let mut up = vec![0usize; links.len() / 2];
        let mut down = vec![0usize; links.len() / 2];
        {
            let mut uf = up_start[..total].to_vec();
            let mut df = down_start[..total].to_vec();
            for pair in links.chunks(2) {
                down[df[pair[0]]] = pair[1];
                df[pair[0]] += 1;
                up[uf[pair[1]]] = pair[0];
                uf[pair[1]] += 1;
            }
        }

        // Order within each rank: by lane first, then by where the item's neighbours sit.
        let max_rank = max_rank as usize;
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
        for i in 0..total {
            rows[rank_of[i] as usize].push(i);
        }
        let mut pos = vec![0.0f64; total];
        let mut key = vec![0.0f64; total];
        let settle = |row: &mut Vec<usize>, key: &[f64], pos: &mut [f64]| {
            row.sort_by(|&a, &b| lane_of[a].cmp(&lane_of[b]).then(by_difference(key[a] - key[b])).then(a.cmp(&b)));
            for (k, &i) in row.iter().enumerate() {
                pos[i] = k as f64;
            }
        };
        for row in &mut rows {
            for (k, &i) in row.iter().enumerate() {
                key[i] = k as f64;
            }
            settle(row, &key, &mut pos);
        }
        for sweep in 0..SWEEPS {
            if max_rank == 0 {
                break;
            }
            let downward = sweep % 2 == 0;
            for s in 1..=max_rank {
                let row = &mut rows[if downward { s } else { max_rank - s }];
                let (start, list) = if downward { (&up_start, &up) } else { (&down_start, &down) };
                for &i in row.iter() {
                    let mut sum = 0.0;
                    let count = start[i + 1] - start[i];
                    for k in start[i]..start[i + 1] {
                        sum += pos[list[k]];
                    }
                    key[i] = if count > 0 { sum / count as f64 } else { pos[i] };
                }
                settle(row, &key, &mut pos);
            }
        }

        // The items of one rank that share a lane sit side by side as a run.
        let gap = |a: usize, b: usize| if dummy[a] || dummy[b] { opt.edge_sep } else { opt.node_sep };
        let runs = |row: &[usize]| {
            let mut out = Vec::new();
            let mut from = 0;
            while from < row.len() {
                let mut to = from;
                while to + 1 < row.len() && lane_of[row[to + 1]] == lane_of[row[from]] {
                    to += 1;
                }
                out.push((from, to));
                from = to + 1;
            }
            out
        };
        let run_size = |row: &[usize], from: usize, to: usize| {
            let mut total = size[row[from]];
            for k in from..to {
                total += gap(row[k], row[k + 1]) + size[row[k + 1]];
            }
            total
        };

        // Lane breadth: enough for its fullest rank, and for its title.
        let mut breadth = vec![0.0f64; lane_count];
        let mut head = if lanes.is_empty() { 0.0 } else { LANE_HEAD };
        for (k, &lane) in lanes.iter().enumerate() {
            let lane = &nodes[lane];
            breadth[k] = js::max(0.0, lane.min_w - 2.0 * pad[k]);
            if lane.pad_top > head {
                head = lane.pad_top;
            }
        }
        for row in &rows {
            for (from, to) in runs(row) {
                let total = run_size(row, from, to);
                let lane = lane_of[row[from]] as usize;
                if total > breadth[lane] {
                    breadth[lane] = total;
                }
            }
        }
        let mut lane_start = vec![0.0f64; lane_count + 1];
        for k in 0..lane_count {
            lane_start[k + 1] = lane_start[k] + breadth[k] + 2.0 * pad[k];
        }

        // Across the flow: each lane's items start centered, then line up with their neighbours
        // in the same lane as far as the lane and the items beside them allow.
        let mut c = vec![0.0f64; total];
        let mut want = vec![0.0f64; total];
        for row in &rows {
            for (from, to) in runs(row) {
                let lane = lane_of[row[from]] as usize;
                let mut at =
                    (lane_start[lane] + pad[lane] + (lane_start[lane + 1] - pad[lane]) - run_size(row, from, to)) / 2.0;
                for k in from..=to {
                    c[row[k]] = at + size[row[k]] / 2.0;
                    if k < to {
                        at += size[row[k]] + gap(row[k], row[k + 1]);
                    }
                }
            }
        }
        for pass in 0..4 {
            if max_rank == 0 {
                break;
            }
            let downward = pass % 2 == 0;
            for s in 1..=max_rank {
                let row = &rows[if downward { s } else { max_rank - s }];
                let (start, list) = if downward { (&up_start, &up) } else { (&down_start, &down) };
                for &i in row {
                    let mut sum = 0.0;
                    let mut count = 0;
                    for k in start[i]..start[i + 1] {
                        if lane_of[list[k]] != lane_of[i] {
                            continue;
                        }
                        sum += c[list[k]];
                        count += 1;
                    }
                    want[i] = if count > 0 { sum / f64::from(count) } else { c[i] };
                }
                for (from, to) in runs(row) {
                    let lane = lane_of[row[from]] as usize;
                    let mut floor = lane_start[lane] + pad[lane];
                    for k in from..=to {
                        let i = row[k];
                        c[i] = js::max(want[i], floor + size[i] / 2.0);
                        if k < to {
                            floor = c[i] + size[i] / 2.0 + gap(i, row[k + 1]);
                        }
                    }
                    let mut ceil = lane_start[lane + 1] - pad[lane];
                    for k in (from..=to).rev() {
                        let i = row[k];
                        c[i] = js::min(c[i], ceil - size[i] / 2.0);
                        if k > from {
                            ceil = c[i] - size[i] / 2.0 - gap(row[k - 1], i);
                        }
                    }
                }
            }
        }

        // Along the flow: one band per rank, after the lane titles.
        let mut band_start = vec![0.0f64; max_rank + 1];
        let mut band_end = vec![0.0f64; max_rank + 1];
        // The room after a rank is for the relations that turn there or are only as long as
        // they were asked to be. Where just plain handoffs and straight lines pass, the ranks
        // close up, as far as the label of a handoff between them leaves it a line to sit on.
        let full = opt.rank_sep / f64::from(step);
        let mut sep = vec![full; max_rank + 1];
        if plain.iter().any(|&on| on) {
            sep.fill(full / CLOSE);
            for k in 0..m {
                let e = &ledges[k];
                let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
                let (ra, rb) = (rank_of[a] as usize, rank_of[b] as usize);
                if plain[k] {
                    if e.label_h <= 0.0 || span[k] > step {
                        continue;
                    }
                    let room = js::min(
                        full,
                        (e.label_h + 2.0 * TURN - js::min(depth[a], depth[b]) / 2.0) / f64::from(span[k]),
                    );
                    for r in ra..rb {
                        if room > sep[r] {
                            sep[r] = room;
                        }
                    }
                    continue;
                }
                let Some(d0) = first_dummy[k].filter(|_| f64::from(span[k]) > f64::from(step) * e.minlen) else {
                    for r in ra..rb {
                        sep[r] = full;
                    }
                    continue;
                };
                let last = d0 + span[k] as usize - 1;
                for d in d0 - 1..last {
                    let u = if d < d0 { a } else { d };
                    let v = if d + 1 < last { d + 1 } else { b };
                    if lane_of[u] != lane_of[v] || (c[u] - c[v]).abs() >= 0.5 {
                        sep[rank_of[u] as usize] = full;
                    }
                }
            }
        }
        let mut at = head + end_pad;
        for r in 0..=max_rank {
            let mut deepest = 0.0;
            for &i in &rows[r] {
                if depth[i] > deepest {
                    deepest = depth[i];
                }
            }
            band_start[r] = at;
            band_end[r] = at + deepest;
            at += deepest + if r == max_rank { 0.0 } else { sep[r] };
        }
        let length = at + end_pad;
        let breadth_total = lane_start[lane_count];
        let flip = |a: f64| if flipped { head + length - a } else { a };
        let px = |c: f64, a: f64| if sideways { flip(a) } else { c };
        let py = |c: f64, a: f64| if sideways { c } else { flip(a) };
        let axis = if sideways { Axis::Horizontal } else { Axis::Vertical };
        let across = if sideways { Axis::Vertical } else { Axis::Horizontal };

        for k in 0..real.len() {
            let node = &mut nodes[real[k]];
            let a = (band_start[rank_of[k] as usize] + band_end[rank_of[k] as usize]) / 2.0;
            node.x = px(c[k], a);
            node.y = py(c[k], a);
        }
        for (k, &lane) in lanes.iter().enumerate() {
            let lane = &mut nodes[lane];
            let middle = (lane_start[k] + lane_start[k + 1]) / 2.0;
            let size = lane_start[k + 1] - lane_start[k];
            lane.x = if sideways { length / 2.0 } else { middle };
            lane.y = if sideways { middle } else { length / 2.0 };
            lane.w = if sideways { length } else { size };
            lane.h = if sideways { size } else { length };
            lane.pad_top = head;
        }

        // Groups inside a lane are drawn around their members, deepest first.
        let mut depth_of = vec![0usize; n];
        let mut inner = Vec::new();
        for i in 0..n {
            if !nodes[i].is_group || lane_slot[i].is_some() || item_of[i].is_some() {
                continue;
            }
            let mut d = 0;
            let mut parent = nodes[i].parent;
            while let Some(p) = parent.filter(|_| d < MAX_HOPS) {
                d += 1;
                parent = nodes[p].parent;
            }
            depth_of[i] = d;
            inner.push(i);
        }
        inner.sort_by(|&a, &b| depth_of[b].cmp(&depth_of[a]).then(a.cmp(&b)));
        let mut bounds = vec![[f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY]; n];
        let grow = |bounds: &mut [[f64; 4]], g: usize, c: &CNode| {
            let b = &mut bounds[g];
            if c.x - c.w / 2.0 < b[0] {
                b[0] = c.x - c.w / 2.0;
            }
            if c.y - c.h / 2.0 < b[1] {
                b[1] = c.y - c.h / 2.0;
            }
            if c.x + c.w / 2.0 > b[2] {
                b[2] = c.x + c.w / 2.0;
            }
            if c.y + c.h / 2.0 > b[3] {
                b[3] = c.y + c.h / 2.0;
            }
        };
        for &i in real {
            if let Some(parent) = nodes[i].parent.filter(|&parent| lane_slot[parent].is_none()) {
                grow(&mut bounds, parent, &nodes[i]);
            }
        }
        for &g in &inner {
            let b = bounds[g];
            if b[0] == f64::INFINITY {
                continue;
            }
            let group = &mut nodes[g];
            let w = js::max(b[2] - b[0] + 2.0 * group.pad_x, group.min_w);
            group.x = (b[0] + b[2]) / 2.0;
            group.w = w;
            group.y = (b[1] - group.pad_top + b[3] + group.pad_bottom) / 2.0;
            group.h = b[3] - b[1] + group.pad_top + group.pad_bottom;
            if let Some(parent) = group.parent.filter(|&parent| lane_slot[parent].is_none()) {
                let group = nodes[g].clone();
                grow(&mut bounds, parent, &group);
            }
        }

        for e in edges.iter_mut() {
            e.route.clear();
        }
        for k in 0..m {
            let e = &ledges[k];
            let out = &mut edges[self.edge_of[k]];
            let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
            let (ra, rb) = (rank_of[a] as usize, rank_of[b] as usize);
            let mut pts = vec![[c[a], band_end[ra] - (band_end[ra] - band_start[ra] - depth[a]) / 2.0]];
            if let Some(d0) = first_dummy[k] {
                for d in d0..d0 + span[k] as usize - 1 {
                    let r = rank_of[d] as usize;
                    pts.push([c[d], band_start[r]]);
                    if band_end[r] > band_start[r] {
                        pts.push([c[d], band_end[r]]);
                    }
                }
            }
            pts.push([c[b], band_start[rb] + (band_end[rb] - band_start[rb] - depth[b]) / 2.0]);
            let mut route: Vec<RoutePoint> =
                pts.iter().map(|&[c, a]| RoutePoint { x: px(c, a), y: py(c, a), axis }).collect();
            if e.reversed {
                route.reverse();
            }
            match label_item[k] {
                Some(label) => {
                    let r = rank_of[label] as usize;
                    let mid = (band_start[r] + band_end[r]) / 2.0;
                    out.label_x = px(c[label], mid);
                    out.label_y = py(c[label], mid);
                }
                None => {
                    out.label_x = (route[0].x + route[route.len() - 1].x) / 2.0;
                    out.label_y = (route[0].y + route[route.len() - 1].y) / 2.0;
                }
            }
            out.route = route;
        }

        // An edge that starts or ends at a lane or a group runs straight to its border.
        for e in edges.iter_mut() {
            if !e.route.is_empty() || e.src == e.dst {
                continue;
            }
            let (from, to) = (&nodes[e.src], &nodes[e.dst]);
            let [ax, ay] = border(from, to);
            let [bx, by] = border(to, from);
            let flat = if (bx - ax).abs() > (by - ay).abs() { Axis::Horizontal } else { Axis::Vertical };
            e.route = vec![RoutePoint { x: ax, y: ay, axis: flat }, RoutePoint { x: bx, y: by, axis: flat }];
            e.label_x = (ax + bx) / 2.0;
            e.label_y = (ay + by) / 2.0;
        }

        // A handoff can leave towards the other lane, then arrive along the flow (or vice
        // versa). In particular, a decision's return branch must not leave through its incoming
        // side and retrace the path past the preceding step. Try the two single-corner routes,
        // keeping the ranked route when a node or another label blocks them.
        let mut routed = vec![false; m];
        let route_points: f64 = edges.iter().map(|edge| edge.route.len() as f64).sum();
        let work = m as f64 * (real.len() as f64 + edges.len() as f64 + route_points);
        if work <= ROUTE_BUDGET {
            let router = Router {
                sideways,
                c: &c,
                size: &size,
                depth: &depth,
                band_start: &band_start,
                band_end: &band_end,
                rank_of: &rank_of,
                real: real.len(),
                flip: &flip,
            };
            let mut labels: Vec<[f64; 2]> = edges
                .iter()
                .map(|e| {
                    let (across, along) = if sideways { (e.label_y, e.label_x) } else { (e.label_x, e.label_y) };
                    [across, flip(along)]
                })
                .collect();
            for k in 0..m {
                let e = &ledges[k];
                let ei = self.edge_of[k];
                let (a, b) = (e.tail, e.head);
                if lane_of[a] == lane_of[b] || (c[b] - c[a]).abs() < 1.0 {
                    continue;
                }
                let (ac, bc, aa, ba) = (c[a], c[b], router.amid(a), router.amid(b));
                let dc = sign(bc - ac);
                let da = sign(ba - aa);
                let mut handoff = Handoff {
                    router: &router,
                    edges,
                    labels: &mut labels,
                    e,
                    ei,
                    a,
                    b,
                    length,
                    breadth_total,
                    axis,
                    across,
                };
                if handoff.attempt(&[ac + dc * size[a] / 2.0, aa, bc, aa, bc, ba - da * depth[b] / 2.0], 2.0) {
                    routed[k] = true;
                    continue;
                }
                if handoff.attempt(&[ac, aa + da * depth[a] / 2.0, ac, ba, bc - dc * size[b] / 2.0, ba], 2.0) {
                    routed[k] = true;
                    continue;
                }
                // With both corners blocked, as for a return past the steps between, the handoff
                // goes around: out to the margin of a lane, along it, and back in level with its
                // end. It takes the margin that crosses the fewest other relations and shares the
                // fewest ends with them, then the shortest way.
                if work > ROUTE_BUDGET / RELAY {
                    continue;
                }
                let mut around: Vec<(Vec<f64>, f64, usize)> = Vec::new();
                let pair = if lane_of[a] < lane_of[b] { [lane_of[b], lane_of[a]] } else { [lane_of[a], lane_of[b]] };
                for lane in pair.map(|lane| lane as usize) {
                    // A lane that keeps no margin yet has less room there, and the handoff stays inside it.
                    let inset = js::max(pad[lane] - LEG, js::min(pad[lane] / 2.0, MARGIN - LEG));
                    for ch in [lane_start[lane] + inset, lane_start[lane + 1] - inset] {
                        if (ch - ac).abs() < size[a] / 2.0 + 4.0 || (ch - bc).abs() < size[b] / 2.0 + 4.0 {
                            continue;
                        }
                        let pts = vec![
                            ac + sign(ch - ac) * size[a] / 2.0,
                            aa,
                            ch,
                            aa,
                            ch,
                            ba,
                            bc + sign(ch - bc) * size[b] / 2.0,
                            ba,
                        ];
                        let ends = meets(handoff.edges, px(pts[0], pts[1]), py(pts[0], pts[1]), ei)
                            + meets(handoff.edges, px(pts[6], pts[7]), py(pts[6], pts[7]), ei);
                        let mut cost = ends as f64 * 1e6;
                        for p in (2..pts.len()).step_by(2) {
                            let (x0, y0) = (px(pts[p - 2], pts[p - 1]), py(pts[p - 2], pts[p - 1]));
                            let (x1, y1) = (px(pts[p], pts[p + 1]), py(pts[p], pts[p + 1]));
                            let crossed = crossings(
                                handoff.edges,
                                js::min(x0, x1),
                                js::min(y0, y1),
                                js::max(x0, x1),
                                js::max(y0, y1),
                                ei,
                            );
                            cost += crossed as f64 * 1e6 + (x1 - x0).abs() + (y1 - y0).abs();
                        }
                        around.push((pts, cost, lane));
                    }
                }
                around.sort_by(|p, q| by_difference(p.1 - q.1));
                for (pts, _, lane) in &around {
                    if !handoff.attempt(pts, 1.0) {
                        continue;
                    }
                    routed[k] = true;
                    used[*lane] = true;
                    break;
                }
            }
        }

        let size = if sideways {
            Size { width: length, height: breadth_total }
        } else {
            Size { width: breadth_total, height: length }
        };
        Placed { size, routed, margins: used, work }
    }
}

/// What routing a handoff reads of a placement: where each item is across and along the flow.
struct Router<'a> {
    sideways: bool,
    c: &'a [f64],
    size: &'a [f64],
    depth: &'a [f64],
    band_start: &'a [f64],
    band_end: &'a [f64],
    rank_of: &'a [i32],
    real: usize,
    flip: &'a dyn Fn(f64) -> f64,
}

impl Router<'_> {
    fn amid(&self, i: usize) -> f64 {
        let r = self.rank_of[i] as usize;
        (self.band_start[r] + self.band_end[r]) / 2.0
    }

    fn px(&self, c: f64, a: f64) -> f64 {
        if self.sideways { (self.flip)(a) } else { c }
    }

    fn py(&self, c: f64, a: f64) -> f64 {
        if self.sideways { c } else { (self.flip)(a) }
    }
}

fn overlaps(box_: [f64; 4], lc: f64, la: f64, cw: f64, ah: f64) -> bool {
    let [c0, a0, c1, a1] = box_;
    c1 > lc - cw / 2.0 && c0 < lc + cw / 2.0 && a1 > la - ah / 2.0 && a0 < la + ah / 2.0
}

/// How many other relations have a stretch inside a box. A stretch's whole box counts,
/// including its curve.
fn crossings(edges: &[CEdge], x0: f64, y0: f64, x1: f64, y1: f64, ei: usize) -> usize {
    let mut count = 0;
    for (i, edge) in edges.iter().enumerate() {
        if i == ei {
            continue;
        }
        let route = &edge.route;
        for j in 1..route.len() {
            let (p, q) = (route[j - 1], route[j]);
            if js::max(p.x, q.x) > x0 && js::min(p.x, q.x) < x1 && js::max(p.y, q.y) > y0 && js::min(p.y, q.y) < y1 {
                count += 1;
                break;
            }
        }
    }
    count
}

/// How many other relations start or end at a point.
fn meets(edges: &[CEdge], x: f64, y: f64, ei: usize) -> usize {
    let mut count = 0;
    for (i, edge) in edges.iter().enumerate() {
        let route = &edge.route;
        if i == ei || route.is_empty() {
            continue;
        }
        let (first, last) = (route[0], route[route.len() - 1]);
        if ((first.x - x).abs() < 1.0 && (first.y - y).abs() < 1.0)
            || ((last.x - x).abs() < 1.0 && (last.y - y).abs() < 1.0)
        {
            count += 1;
        }
    }
    count
}

/// One handoff being routed: the relation, and what its route must keep clear of.
struct Handoff<'a, 'r> {
    router: &'a Router<'r>,
    edges: &'a mut [CEdge],
    labels: &'a mut [[f64; 2]],
    e: &'a LEdge,
    ei: usize,
    a: usize,
    b: usize,
    length: f64,
    breadth_total: f64,
    axis: Axis,
    across: Axis,
}

impl Handoff<'_, '_> {
    /// Whether a box, across and along the flow, is clear of the items other than `src` and
    /// `dst`, and of the labels of the other relations.
    fn clear(&self, box_: [f64; 4], ends: Option<(usize, usize)>) -> bool {
        let r = self.router;
        for i in 0..r.real {
            if ends.is_some_and(|(src, dst)| i == src || i == dst) {
                continue;
            }
            if overlaps(box_, r.c[i], r.amid(i), r.size[i] + 8.0, r.depth[i] + 8.0) {
                return false;
            }
        }
        for (i, e) in self.edges.iter().enumerate() {
            // A step's own loop keeps its label inside the step's box, and has no place here.
            if i == self.ei || e.label_w <= 0.0 || e.src == e.dst {
                continue;
            }
            let (cw, ah) = if r.sideways { (e.label_h, e.label_w) } else { (e.label_w, e.label_h) };
            if overlaps(box_, self.labels[i][0], self.labels[i][1], cw, ah) {
                return false;
            }
        }
        true
    }

    /// Two relations drawn along one line read as a single path. Those that leave the same step
    /// or arrive at the same step may share a trunk; any other pair must not.
    fn retraces(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
        let upright = (x1 - x0).abs() < 0.5;
        let (lo, hi) = if upright { (js::min(y0, y1), js::max(y0, y1)) } else { (js::min(x0, x1), js::max(x0, x1)) };
        let line = if upright { x0 } else { y0 };
        let own = &self.edges[self.ei];
        for (i, other) in self.edges.iter().enumerate() {
            if i == self.ei || other.src == own.src || other.dst == own.dst {
                continue;
            }
            let route = &other.route;
            for j in 1..route.len() {
                let (p, q) = (route[j - 1], route[j]);
                let (u0, u1) = if upright { (p.x, q.x) } else { (p.y, q.y) };
                if (u0 - line).abs() >= NEAR || (u1 - line).abs() >= NEAR {
                    continue;
                }
                let (v0, v1) = if upright { (p.y, q.y) } else { (p.x, q.x) };
                if js::min(hi, js::max(v0, v1)) - js::max(lo, js::min(v0, v1)) > 1.0 {
                    return true;
                }
            }
        }
        false
    }

    /// Routes the handoff through `pts`, across and along the flow in turn, where the way is
    /// clear and its label has a stretch to sit on.
    fn attempt(&mut self, pts: &[f64], margin: f64) -> bool {
        let r = self.router;
        for p in (2..pts.len()).step_by(2) {
            let box_ = [
                js::min(pts[p - 2], pts[p]) - margin,
                js::min(pts[p - 1], pts[p + 1]) - margin,
                js::max(pts[p - 2], pts[p]) + margin,
                js::max(pts[p - 1], pts[p + 1]) + margin,
            ];
            if !self.clear(box_, Some((self.a, self.b))) {
                return false;
            }
            let (x0, y0) = (r.px(pts[p - 2], pts[p - 1]), r.py(pts[p - 2], pts[p - 1]));
            if self.retraces(x0, y0, r.px(pts[p], pts[p + 1]), r.py(pts[p], pts[p + 1])) {
                return false;
            }
        }
        let out_w = self.edges[self.ei].label_w;
        let out_h = self.edges[self.ei].label_h;
        let (mut lc, mut la, mut longest) = (0.0, 0.0, -1.0);
        if out_w > 0.0 {
            let (cw, ah) = (self.e.label_w, self.e.label_h);
            for p in (2..pts.len()).step_by(2) {
                let cc = (pts[p - 2] + pts[p]) / 2.0;
                let ca = (pts[p - 1] + pts[p + 1]) / 2.0;
                let across = (pts[p] - pts[p - 2]).abs();
                let along = (pts[p + 1] - pts[p - 1]).abs();
                let run = across + along;
                if run <= longest || (if across > 0.0 { across < cw + 8.0 } else { along < ah + 8.0 }) {
                    continue;
                }
                if cc - cw / 2.0 < 0.0
                    || cc + cw / 2.0 > self.breadth_total
                    || ca - ah / 2.0 < 0.0
                    || ca + ah / 2.0 > self.length
                {
                    continue;
                }
                // Include the ends here: the label must clear both shapes it connects.
                if !self.clear([cc - cw / 2.0, ca - ah / 2.0, cc + cw / 2.0, ca + ah / 2.0], None) {
                    continue;
                }
                // A label moved onto a handoff must not cover a return path that keeps its
                // ranked route. A segment's whole box is kept clear, including its curve.
                let (lx, ly) = (r.px(cc, ca), r.py(cc, ca));
                let (x0, y0) = (lx - out_w / 2.0, ly - out_h / 2.0);
                if crossings(self.edges, x0, y0, lx + out_w / 2.0, ly + out_h / 2.0, self.ei) > 0 {
                    continue;
                }
                (lc, la, longest) = (cc, ca, run);
            }
            if longest < 0.0 {
                return false;
            }
        }
        // A point short of each corner and one past it keep the turn close to the corner, so
        // that the line stays under its label and the handoff takes little room.
        let mut way = vec![pts[0], pts[1]];
        for p in (2..pts.len()).step_by(2) {
            let dx = pts[p] - pts[p - 2];
            let dy = pts[p + 1] - pts[p - 1];
            let run = dx.abs() + dy.abs();
            let first = if p > 2 { TURN } else { 0.0 };
            let last = if p + 2 < pts.len() { TURN } else { 0.0 };
            if run > first + last + 1.0 {
                if first != 0.0 {
                    way.extend([pts[p - 2] + (dx / run) * first, pts[p - 1] + (dy / run) * first]);
                }
                if last != 0.0 {
                    way.extend([pts[p] - (dx / run) * last, pts[p + 1] - (dy / run) * last]);
                }
            }
            way.extend([pts[p], pts[p + 1]]);
        }
        let route: Vec<RoutePoint> = (0..way.len())
            .step_by(2)
            .map(|p| {
                let crosswise = if p == 0 { way[0] != way[2] } else { way[p] != way[p - 2] };
                RoutePoint {
                    x: r.px(way[p], way[p + 1]),
                    y: r.py(way[p], way[p + 1]),
                    axis: if crosswise { self.across } else { self.axis },
                }
            })
            .collect();
        let out = &mut self.edges[self.ei];
        out.route = route;
        if out_w > 0.0 {
            out.label_x = r.px(lc, la);
            out.label_y = r.py(lc, la);
            self.labels[self.ei] = [lc, la];
        }
        true
    }
}

/// The point where the line from one box's center to another's leaves the first box. When the
/// other center is inside the box, the point is on the nearest side instead.
fn border(from: &CNode, to: &CNode) -> [f64; 2] {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let hw = js::max(from.w / 2.0, 0.5);
    let hh = js::max(from.h / 2.0, 0.5);
    if dx.abs() < hw && dy.abs() < hh {
        let to_side = hw - dx.abs();
        let to_end = hh - dy.abs();
        return if to_side < to_end {
            [from.x + if dx < 0.0 { -hw } else { hw }, to.y]
        } else {
            [to.x, from.y + if dy < 0.0 { -hh } else { hh }]
        };
    }
    let scale = js::min(
        if dx != 0.0 { hw / dx.abs() } else { f64::INFINITY },
        if dy != 0.0 { hh / dy.abs() } else { f64::INFINITY },
    );
    [from.x + dx * scale, from.y + dy * scale]
}
