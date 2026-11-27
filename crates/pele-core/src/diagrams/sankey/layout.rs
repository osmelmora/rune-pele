//! Pele's `layoutSankey` (`src/diagrams/sankey/layout.ts`): nodes in columns by their distance
//! from a source, sized by their flow, then settled beside their neighbours so the bands cross
//! less. Every number is the double Pele computes; `tests/sankey_layout.rs` holds it to that.

use super::SankeyModel;
use crate::js;
use std::cmp::Ordering;

/// Which column a node goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    /// Sinks in the last column, everything else by its distance from a source.
    Justify,
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutOptions {
    pub width: f64,
    pub height: f64,
    pub node_width: f64,
    pub node_padding: f64,
    /// Height of a label line; thin nodes are kept at least this far apart where there is room.
    pub label_height: f64,
    pub alignment: Alignment,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedNode {
    pub layer: usize,
    /// Total flow through the node, in the diagram's units.
    pub value: f64,
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedLink {
    pub source: usize,
    pub target: usize,
    pub value: f64,
    pub thickness: f64,
    /// Vertical centres of the band where it leaves its source and where it enters its target.
    pub source_y: f64,
    pub target_y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLayout {
    pub nodes: Vec<PlacedNode>,
    pub links: Vec<PlacedLink>,
}

const PASSES: u32 = 6;

/// `a - b || tie`, as a comparator Pele's sort reads: a difference that is zero or NaN leaves it
/// to the tie.
fn by(difference: f64, tie: Ordering) -> Ordering {
    if difference < 0.0 {
        Ordering::Less
    } else if difference > 0.0 {
        Ordering::Greater
    } else {
        tie
    }
}

/// Pele's `sort`: insertion for a handful of bands, the built-in sort otherwise. Each comparator
/// breaks ties by index, so both give the one order a total order has.
fn sort(list: &mut [usize], compare: impl Fn(usize, usize) -> Ordering) {
    if list.len() > 8 {
        list.sort_by(|&a, &b| compare(a, b));
        return;
    }
    for i in 1..list.len() {
        let item = list[i];
        let mut j = i;
        while j > 0 && compare(list[j - 1], item) == Ordering::Greater {
            list[j] = list[j - 1];
            j -= 1;
        }
        list[j] = item;
    }
}

/// The values the links carry, as a model or a test gives them.
#[derive(Debug)]
pub struct Flows<'a> {
    pub nodes: usize,
    pub links: &'a [(usize, usize, f64)],
}

impl<'a> Flows<'a> {
    pub fn new(nodes: usize, links: &'a [(usize, usize, f64)]) -> Self {
        Flows { nodes, links }
    }
}

/// Pele's `layoutSankey`. The error names a node on a cycle, as Pele's message does.
pub(crate) fn layout_sankey(model: &SankeyModel, options: &LayoutOptions) -> Result<SankeyLayout, usize> {
    let links: Vec<(usize, usize, f64)> =
        model.links.iter().map(|link| (link.source, link.target, link.value)).collect();
    layout(&Flows::new(model.nodes.len(), &links), options)
}

/// Pele's `layoutSankey` on bare flows, each a source, a target and a value. `Err` holds the node
/// on a cycle that Pele's message names.
pub fn layout(flows: &Flows<'_>, options: &LayoutOptions) -> Result<SankeyLayout, usize> {
    let n = flows.nodes;
    let mut links: Vec<PlacedLink> = flows
        .links
        .iter()
        .map(|&(source, target, value)| PlacedLink {
            source,
            target,
            value: if value.is_finite() && value > 0.0 { value } else { 0.0 },
            thickness: 0.0,
            source_y: 0.0,
            target_y: 0.0,
        })
        .collect();
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, link) in links.iter().enumerate() {
        outgoing[link.source].push(k);
        incoming[link.target].push(k);
    }

    // Longest distance from a source, in topological order; what is left over lies on a cycle.
    let mut depth = vec![0_usize; n];
    let mut pending: Vec<usize> = incoming.iter().map(Vec::len).collect();
    let mut order: Vec<usize> = (0..n).filter(|&i| pending[i] == 0).collect();
    let mut q = 0;
    while q < order.len() {
        let i = order[q];
        for &k in &outgoing[i] {
            let t = links[k].target;
            if depth[t] < depth[i] + 1 {
                depth[t] = depth[i] + 1;
            }
            pending[t] -= 1;
            if pending[t] == 0 {
                order.push(t);
            }
        }
        q += 1;
    }
    if order.len() < n {
        // Every node left over has a predecessor that is left over too; walking back finds the loop.
        let mut stuck = (0..n).find(|&i| pending[i] != 0).expect("a node is left over");
        let mut seen = vec![false; n];
        while !seen[stuck] {
            seen[stuck] = true;
            if let Some(&k) = incoming[stuck].iter().find(|&&k| pending[links[k].source] > 0) {
                stuck = links[k].source;
            }
        }
        return Err(stuck);
    }
    let mut layers = 1;
    for &d in &depth {
        layers = layers.max(d + 1);
    }
    let mut reach = vec![0_usize; n];
    for &i in order.iter().rev() {
        for &k in &outgoing[i] {
            if reach[i] < reach[links[k].target] + 1 {
                reach[i] = reach[links[k].target] + 1;
            }
        }
    }

    // Flows are scaled to the largest one, so that sums of huge values stay finite.
    let mut largest = 0.0;
    for link in &links {
        if link.value > largest {
            largest = link.value;
        }
    }
    let unit = if largest > 0.0 { 1.0 / largest } else { 0.0 };

    let mut nodes = Vec::with_capacity(n);
    let mut flow = vec![0.0; n];
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); layers];
    let step = if layers > 1 { (options.width - options.node_width) / (layers - 1) as f64 } else { 0.0 };
    for i in 0..n {
        let layer = match options.alignment {
            Alignment::Left => depth[i],
            Alignment::Right => layers - 1 - reach[i],
            Alignment::Center => {
                let mut layer = depth[i];
                if incoming[i].is_empty() && !outgoing[i].is_empty() {
                    layer = layers;
                    for &k in &outgoing[i] {
                        layer = layer.min(depth[links[k].target] - 1);
                    }
                }
                layer
            }
            Alignment::Justify => {
                if outgoing[i].is_empty() {
                    layers - 1
                } else {
                    depth[i]
                }
            }
        };
        let (mut into, mut out, mut value, mut value_out) = (0.0, 0.0, 0.0, 0.0);
        for &k in &incoming[i] {
            into += links[k].value * unit;
            value += links[k].value;
        }
        for &k in &outgoing[i] {
            out += links[k].value * unit;
            value_out += links[k].value;
        }
        flow[i] = js::max(into, out);
        nodes.push(PlacedNode { layer, value: js::max(value, value_out), x: layer as f64 * step, y: 0.0, height: 0.0 });
        columns[layer].push(i);
    }

    let mut tallest = 1;
    for column in &columns {
        tallest = tallest.max(column.len());
    }
    let gap = if tallest > 1 {
        js::min(options.node_padding, options.height / (tallest - 1) as f64)
    } else {
        options.node_padding
    };
    let mut scale = f64::INFINITY;
    for column in &columns {
        let mut sum = 0.0;
        for &i in column {
            sum += flow[i];
        }
        if sum > 0.0 {
            scale = js::min(scale, (options.height - (column.len() - 1) as f64 * gap) / sum);
        }
    }
    if !scale.is_finite() || scale < 0.0 {
        scale = 0.0;
    }

    // A thin node still needs room for its label. Where every column can afford it, each node
    // claims at least a label's height, and the flows are scaled down to make that fit.
    let mut claim = js::max(0.0, options.label_height - gap);
    let fits = |s: f64, claim: f64| {
        columns.iter().all(|column| {
            let mut extent = (column.len() as f64 - 1.0) * gap;
            for &i in column {
                extent += js::max(flow[i] * s, claim);
            }
            extent <= options.height + 1e-6
        })
    };
    if claim > 0.0 && !fits(scale, claim) {
        if fits(0.0, claim) {
            let mut low = 0.0;
            for _ in 0..40 {
                let mid = (low + scale) / 2.0;
                if fits(mid, claim) {
                    low = mid;
                } else {
                    scale = mid;
                }
            }
            scale = low;
        } else {
            claim = 0.0;
        }
    }

    // The room a node keeps clear above and below itself.
    let mut margin = vec![0.0; n];
    for link in &mut links {
        link.thickness = link.value * unit * scale;
    }
    for column in &columns {
        let mut y = 0.0;
        for &i in column {
            nodes[i].height = flow[i] * scale;
            margin[i] = js::max(0.0, claim - nodes[i].height) / 2.0;
            nodes[i].y = y + margin[i];
            y += nodes[i].height + 2.0 * margin[i] + gap;
        }
        // Spare room is shared out between the nodes of a column.
        let spare = (options.height - y + gap) / (column.len() + 1) as f64;
        for (k, &i) in column.iter().enumerate() {
            nodes[i].y += spare * (k + 1) as f64;
        }
    }

    let mut state = Settling { nodes, links, outgoing, incoming, margin, gap, height: options.height };
    let mut source_offset = vec![0.0; state.links.len()];
    let mut target_offset = vec![0.0; state.links.len()];
    state.stack_all(&mut source_offset, &mut target_offset);
    for pass in 0..PASSES {
        let pull = js::pow(0.99, f64::from(pass));
        for l in (0..layers.saturating_sub(1)).rev() {
            state.settle(&mut columns[l], pull, false, &mut source_offset, &mut target_offset);
        }
        state.stack_all(&mut source_offset, &mut target_offset);
        for column in columns.iter_mut().skip(1) {
            state.settle(column, pull, true, &mut source_offset, &mut target_offset);
        }
        state.stack_all(&mut source_offset, &mut target_offset);
    }
    let Settling { nodes, mut links, .. } = state;
    for (k, link) in links.iter_mut().enumerate() {
        link.source_y = nodes[link.source].y + source_offset[k];
        link.target_y = nodes[link.target].y + target_offset[k];
    }
    Ok(SankeyLayout { nodes, links })
}

/// What the passes that settle the columns move.
struct Settling {
    nodes: Vec<PlacedNode>,
    links: Vec<PlacedLink>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    margin: Vec<f64>,
    gap: f64,
    height: f64,
}

impl Settling {
    fn center(&self, i: usize) -> f64 {
        self.nodes[i].y + self.nodes[i].height / 2.0
    }

    /// Bands leave a node in the order of where they go, so they do not cross at it. These are
    /// the offsets of each band's middle from the top of the node.
    fn stack_out(&mut self, i: usize, source_offset: &mut [f64]) {
        let mut list = std::mem::take(&mut self.outgoing[i]);
        sort(&mut list, |a, b| by(self.center(self.links[a].target) - self.center(self.links[b].target), a.cmp(&b)));
        let mut y = 0.0;
        for &k in &list {
            source_offset[k] = y + self.links[k].thickness / 2.0;
            y += self.links[k].thickness;
        }
        self.outgoing[i] = list;
    }

    fn stack_in(&mut self, i: usize, target_offset: &mut [f64]) {
        let mut list = std::mem::take(&mut self.incoming[i]);
        sort(&mut list, |a, b| by(self.center(self.links[a].source) - self.center(self.links[b].source), a.cmp(&b)));
        let mut y = 0.0;
        for &k in &list {
            target_offset[k] = y + self.links[k].thickness / 2.0;
            y += self.links[k].thickness;
        }
        self.incoming[i] = list;
    }

    fn stack_all(&mut self, source_offset: &mut [f64], target_offset: &mut [f64]) {
        for i in 0..self.nodes.len() {
            self.stack_out(i, source_offset);
            self.stack_in(i, target_offset);
        }
    }

    /// Moves each node of a column towards where its bands would run level, then restores the
    /// order and the gaps. `forward` looks at the bands coming in, otherwise at those going out.
    fn settle(
        &mut self,
        column: &mut [usize],
        pull: f64,
        forward: bool,
        source_offset: &mut [f64],
        target_offset: &mut [f64],
    ) {
        for &i in column.iter() {
            let list = if forward { &self.incoming[i] } else { &self.outgoing[i] };
            if list.is_empty() {
                continue;
            }
            if forward {
                self.stack_in(i, target_offset);
            } else {
                self.stack_out(i, source_offset);
            }
            let list = if forward { &self.incoming[i] } else { &self.outgoing[i] };
            let (mut weighted, mut weight, mut plain) = (0.0, 0.0, 0.0);
            for &k in list {
                let link = &self.links[k];
                let level = if forward {
                    self.nodes[link.source].y + source_offset[k] - target_offset[k]
                } else {
                    self.nodes[link.target].y + target_offset[k] - source_offset[k]
                };
                weighted += level * link.thickness;
                weight += link.thickness;
                plain += self.center(if forward { link.source } else { link.target }) - self.nodes[i].height / 2.0;
            }
            let wanted = if weight > 0.0 { weighted / weight } else { plain / list.len() as f64 };
            self.nodes[i].y += (wanted - self.nodes[i].y) * pull;
        }
        sort(column, |a, b| by(self.center(a) - self.center(b), a.cmp(&b)));
        let mut y = 0.0;
        for &i in column.iter() {
            if self.nodes[i].y - self.margin[i] < y {
                self.nodes[i].y = y + self.margin[i];
            }
            y = self.nodes[i].y + self.nodes[i].height + self.margin[i] + self.gap;
        }
        y = self.height;
        for &i in column.iter().rev() {
            if self.nodes[i].y + self.nodes[i].height + self.margin[i] > y {
                self.nodes[i].y = y - self.nodes[i].height - self.margin[i];
            }
            y = self.nodes[i].y - self.margin[i] - self.gap;
        }
    }
}
