//! Pele's `renderArchitecture` (`architecture/render.ts`): the grid's rows and columns sized to
//! what they hold, the room around each group, an orthogonal route for each edge between the
//! ports of its ends, and the drawing.

use super::layout::{Grid, place_on_grid};
use super::{ArchitectureModel, End, NodeKind, Service};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::graph::edges::{Marker, MarkerUse, Tip, draw_edge_label_as, draw_marker};
use crate::input::{JsText, JsValue, Units};
use crate::layout::num::hypot;
use crate::measure::{Emphasis, Label, LabelRun, TextMeasurer, layout_label, width};
use crate::scene::{
    Accessible, Entity, EntityName, Facet, IconGlyph, LabelLook, LayerSpec, LineCap, LineJoin, Number, Paint, PartId,
    PathProgram, Point, Role, Scene, SceneBuilder, Size, Style, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

const STUB: f64 = 12.0;
const CORNER: f64 = 6.0;
const LABEL_GAP: f64 = 4.0;
const DOT: f64 = 3.0;
const DX: [f64; 4] = [-1.0, 1.0, 0.0, 0.0];
const DY: [f64; 4] = [0.0, 0.0, -1.0, 1.0];

/// Pele's `setting`: a number of the `architecture` section of the config, kept between `min`
/// and `max`.
fn setting(cx: &DrawContext<'_>, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    let value = cx.config.get("architecture").and_then(JsValue::as_record).and_then(|section| section.get(key));
    match value.and_then(JsValue::as_number) {
        Some(value) if value.is_finite() => js::min(max, js::max(min, value)),
        _ => fallback,
    }
}

/// Pele's `rank`: the grid lines in use, in order, so that empty rows and columns take no room.
struct Rank(Vec<i64>);

impl Rank {
    fn of(mut values: Vec<i64>) -> Rank {
        values.sort_unstable();
        values.dedup();
        Rank(values)
    }

    fn get(&self, line: i64) -> usize {
        self.0.binary_search(&line).expect("every line a node or group stands on is ranked")
    }
}

/// Pele's `fit`: cuts a label down to a box, ending what is left with an ellipsis.
fn fit(label: Label, max_width: f64, max_lines: usize, measurer: &dyn TextMeasurer) -> Label {
    let cut = label.lines.len() > max_lines;
    if !cut && label.width <= max_width {
        return label;
    }
    let ellipsis: Vec<u16> = "\u{2026}".encode_utf16().collect();
    let dots = width(measurer, &ellipsis, label.size, Emphasis::default());
    let mut lines = Vec::new();
    let mut widest = 0.0;
    for (i, line) in label.lines.iter().take(max_lines).enumerate() {
        let mut line = line.clone();
        if line.width > max_width || (cut && i == max_lines - 1) {
            loop {
                let Some(run) = line.runs.last_mut() else {
                    line.runs.push(LabelRun {
                        text: ellipsis.as_slice().into(),
                        style: Emphasis::default(),
                        width: dots,
                    });
                    line.width = dots;
                    break;
                };
                let rest = line.width - run.width;
                if rest + dots < max_width {
                    let (mut lo, mut hi) = (0, run.text.len());
                    while lo < hi {
                        let mid = (lo + hi + 1) >> 1;
                        if rest + width(measurer, &run.text[..mid], label.size, run.style) + dots <= max_width {
                            lo = mid;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    if lo > 0 && (0xd800..=0xdbff).contains(&run.text[lo - 1]) {
                        lo -= 1;
                    }
                    let mut text = JsText::from(run.text[..lo].trim_end_js());
                    text.push_units(&ellipsis);
                    run.width = width(measurer, &text, label.size, run.style);
                    run.text = text;
                    line.width = rest + run.width;
                    break;
                }
                line.runs.pop();
                line.width = rest;
            }
        }
        widest = js::max(widest, line.width);
        lines.push(line);
    }
    Label { height: lines.len() as f64 * label.line_height, width: widest, lines, ..label }
}

/// Where an edge meets a node, and which way it leaves it.
#[derive(Clone, Copy)]
struct Port {
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    /// The box the route has to stay out of when it cannot leave straight.
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    /// How far short of the port an arrowhead stops: the radius of a junction's dot.
    gap: f64,
    /// Set where a junction is reached through the border of its group: the line carries on to
    /// the junction.
    wire: bool,
}

/// Pele's `route`: an orthogonal route from one port to another, each left along its side's normal.
fn route(s: &Port, e: &Port) -> Vec<f64> {
    let (sx, sy, sdx, sdy) = (s.x, s.y, s.dx, s.dy);
    let (ex, ey, edx, edy) = (e.x, e.y, e.dx, e.dy);
    let s1x = sx + sdx * STUB;
    let s1y = sy + sdy * STUB;
    let e1x = ex + edx * STUB;
    let e1y = ey + edy * STUB;
    if sdx * edx + sdy * edy == 0.0 {
        let cx = if sdx != 0.0 { ex } else { sx };
        let cy = if sdx != 0.0 { sy } else { ey };
        if (cx - sx) * sdx + (cy - sy) * sdy > 0.0 && (cx - ex) * edx + (cy - ey) * edy > 0.0 {
            return vec![sx, sy, cx, cy, ex, ey];
        }
        return if sdx != 0.0 {
            vec![sx, sy, s1x, s1y, s1x, e1y, e1x, e1y, ex, ey]
        } else {
            vec![sx, sy, s1x, s1y, e1x, s1y, e1x, e1y, ex, ey]
        };
    }
    let sideways = sdx != 0.0;
    let offset = if sideways { ey - sy } else { ex - sx };
    if sdx == -edx && sdy == -edy {
        let ahead = (ex - sx) * sdx + (ey - sy) * sdy;
        if ahead > 0.0 {
            if offset.abs() < 0.5 {
                return vec![sx, sy, ex, ey];
            }
            // Turn midway between the two boxes, which is clear of both titles, when there is
            // such a place.
            if sideways {
                let lo = if sdx > 0.0 { s.x1 } else { e.x1 };
                let hi = if sdx > 0.0 { e.x0 } else { s.x0 };
                let mx = if lo < hi { (lo + hi) / 2.0 } else { (sx + ex) / 2.0 };
                return vec![sx, sy, mx, sy, mx, ey, ex, ey];
            }
            let lo = if sdy > 0.0 { s.y1 } else { e.y1 };
            let hi = if sdy > 0.0 { e.y0 } else { s.y0 };
            let my = if lo < hi { (lo + hi) / 2.0 } else { (sy + ey) / 2.0 };
            return vec![sx, sy, sx, my, ex, my, ex, ey];
        }
    } else if offset.abs() >= 1.0 {
        if sideways {
            let x = if sdx > 0.0 { js::max(sx, ex) + STUB } else { js::min(sx, ex) - STUB };
            return vec![sx, sy, x, sy, x, ey, ex, ey];
        }
        let y = if sdy > 0.0 { js::max(sy, ey) + STUB } else { js::min(sy, ey) - STUB };
        return vec![sx, sy, sx, y, ex, y, ex, ey];
    }
    // The straight way is blocked by one of the two nodes: go between them, or around both.
    if sideways {
        let lane = if s.y1 + STUB <= e.y0 {
            (s.y1 + e.y0) / 2.0
        } else if e.y1 + STUB <= s.y0 {
            (e.y1 + s.y0) / 2.0
        } else {
            js::max(s.y1, e.y1) + STUB
        };
        return vec![sx, sy, s1x, sy, s1x, lane, e1x, lane, e1x, ey, ex, ey];
    }
    let lane = if s.x1 + STUB <= e.x0 {
        (s.x1 + e.x0) / 2.0
    } else if e.x1 + STUB <= s.x0 {
        (e.x1 + s.x0) / 2.0
    } else {
        js::max(s.x1, e.x1) + STUB
    };
    vec![sx, sy, sx, s1y, lane, s1y, lane, e1y, ex, e1y, ex, ey]
}

/// Pele's `simplify`: drops repeated points and joins runs that continue in a straight line.
fn simplify(p: &[f64]) -> Vec<f64> {
    let mut out = vec![p[0], p[1]];
    for pair in p[2..].chunks(2) {
        let (x, y) = (pair[0], pair[1]);
        let n = out.len();
        let same_x = (x - out[n - 2]).abs() < 0.01;
        let same_y = (y - out[n - 1]).abs() < 0.01;
        if same_x && same_y {
            continue;
        }
        if n >= 4
            && ((same_x && (out[n - 4] - out[n - 2]).abs() < 0.01)
                || (same_y && (out[n - 3] - out[n - 1]).abs() < 0.01))
        {
            out[n - 2] = x;
            out[n - 1] = y;
        } else {
            out.extend([x, y]);
        }
    }
    out
}

/// Pele's `pathData`: the route with its ends trimmed for arrowheads and its corners rounded.
fn path_data(p: &[f64], start_trim: f64, end_trim: f64) -> PathProgram {
    let n = p.len() / 2;
    let mut xs: Vec<f64> = p.iter().step_by(2).copied().collect();
    let mut ys: Vec<f64> = p.iter().skip(1).step_by(2).copied().collect();
    let mut trim = |at: usize, toward: usize, amount: f64| {
        let len = hypot(xs[toward] - xs[at], ys[toward] - ys[at]);
        let t = js::min(amount, js::max(0.0, len - 1.0));
        if len > 0.0 && t > 0.0 {
            xs[at] += ((xs[toward] - xs[at]) / len) * t;
            ys[at] += ((ys[toward] - ys[at]) / len) * t;
        }
    };
    trim(0, 1, start_trim);
    trim(n - 1, n - 2, end_trim);
    let mut d = PathProgram::default().m(Number::num(xs[0]), Number::num(ys[0]));
    for i in 1..n - 1 {
        let before = hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
        let after = hypot(xs[i + 1] - xs[i], ys[i + 1] - ys[i]);
        let r = js::min(js::min(CORNER, before / 2.0), after / 2.0);
        if r < 0.5 {
            d = d.l(Number::num(xs[i]), Number::num(ys[i]));
            continue;
        }
        let in_x = xs[i] - ((xs[i] - xs[i - 1]) / before) * r;
        let in_y = ys[i] - ((ys[i] - ys[i - 1]) / before) * r;
        let out_x = xs[i] + ((xs[i + 1] - xs[i]) / after) * r;
        let out_y = ys[i] + ((ys[i + 1] - ys[i]) / after) * r;
        d = d.l(Number::num(in_x), Number::num(in_y));
        d.steps.push(crate::scene::PathStep::Quad([
            [Number::num(xs[i]), Number::num(ys[i])],
            [Number::num(out_x), Number::num(out_y)],
        ]));
    }
    d.l(Number::num(xs[n - 1]), Number::num(ys[n - 1]))
}

/// The sizes one draw works with, from the config, the host and whether the draw is tight.
struct Sizes {
    icon: f64,
    font: f64,
    small: f64,
    group_pad: f64,
    pad: f64,
    half: f64,
    badge: f64,
}

/// An edge as it is drawn.
struct EdgeView {
    edge: usize,
    path: PathProgram,
    start: Option<Tip>,
    end: Option<Tip>,
    label: Option<(Label, Point)>,
}

/// What a group's heading holds: an icon, and its title.
struct Head {
    icon: Option<(f64, f64)>,
    label: Label,
    at: Point,
}

struct ClusterView {
    group: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    head: Option<Head>,
}

/// What a service shows in its square.
enum Inner {
    None,
    Icon { x: f64, y: f64, side: f64 },
    Text(Label),
}

struct NodeView {
    node: usize,
    at: Point,
    inner: Inner,
    label: Label,
}

/// One draw of the diagram, with every place worked out.
struct Plan {
    sizes: Sizes,
    text_size: f64,
    size: Size,
    title: TitleLayout,
    offset: Point,
    clusters: Vec<ClusterView>,
    edges: Vec<EdgeView>,
    nodes: Vec<NodeView>,
}

/// Pele's `draw` up to where it starts writing.
#[allow(clippy::too_many_lines, reason = "one pass of Pele's draw, whose steps share its tables")]
fn plan(model: &ArchitectureModel, cx: &DrawContext<'_>, tight: bool) -> Plan {
    let icon = setting(cx, "iconSize", 48.0, 8.0, 512.0);
    let font = setting(cx, "fontSize", cx.options.typography.normal.size, 4.0, 128.0);
    let tight_pad = if tight { 12.0 } else { f64::INFINITY };
    let sizes = Sizes {
        icon,
        font,
        small: js::round(font * 0.875),
        group_pad: js::min(setting(cx, "padding", 20.0, 0.0, 256.0), tight_pad),
        pad: cx.options.padding_or_default(),
        half: icon / 2.0,
        badge: js::round(font * 1.25),
    };
    let measurer = cx.measure;
    let half = sizes.half;
    let nodes = &model.nodes;
    let groups = &model.groups;
    let n = nodes.len();
    let g_count = groups.len();
    let grid: Grid = place_on_grid(model);

    // A service is its icon square with the title beneath; a junction takes the same room.
    let wrap = if tight { js::max(icon, 64.0) } else { js::max(icon * 2.25, 96.0) };
    let labels: Vec<Label> = nodes
        .iter()
        .map(|node| {
            let title = match &node.kind {
                NodeKind::Service(Service { title: Some(title), .. }) => &title[..],
                _ => &[],
            };
            layout_label(title, true, measurer, font, wrap, Emphasis::default())
        })
        .collect();
    let box_w: Vec<f64> = labels.iter().map(|label| js::max(icon, 2.0 * (label.width / 2.0).ceil())).collect();
    let box_h: Vec<f64> =
        labels.iter().map(|label| icon + if label.height > 0.0 { LABEL_GAP + label.height } else { 0.0 }).collect();

    let empty_groups = || (0..g_count).filter(|&g| grid.group_empty[g]);
    let col_of = Rank::of(grid.col.iter().copied().chain(empty_groups().map(|g| grid.group_col[g])).collect());
    let row_of = Rank::of(grid.row.iter().copied().chain(empty_groups().map(|g| grid.group_row[g])).collect());
    let (c_count, r_count) = (col_of.0.len(), row_of.0.len());
    let mut col_w = vec![0.0; c_count];
    let mut row_h = vec![0.0; r_count];
    let gap = if tight { js::max(24.0, 2.0 * STUB) } else { js::max(40.0, js::round(icon * 0.75)) };
    let mut gap_x = vec![gap; c_count];
    let mut gap_y = vec![gap; r_count];
    let node_col: Vec<usize> = grid.col.iter().map(|&col| col_of.get(col)).collect();
    let node_row: Vec<usize> = grid.row.iter().map(|&row| row_of.get(row)).collect();
    for i in 0..n {
        col_w[node_col[i]] = js::max(col_w[node_col[i]], box_w[i]);
        row_h[node_row[i]] = js::max(row_h[node_row[i]], box_h[i]);
    }
    for g in empty_groups() {
        let (c, r) = (col_of.get(grid.group_col[g]), row_of.get(grid.group_row[g]));
        col_w[c] = js::max(col_w[c], icon);
        row_h[r] = js::max(row_h[r], half);
    }

    // An edge label needs the gap its edge crosses to be wide enough to hold it.
    let edge_labels: Vec<Label> = model
        .edges
        .iter()
        .map(|edge| {
            let title = edge.title.as_deref().unwrap_or_default();
            layout_label(title, true, measurer, sizes.small, 160.0, Emphasis::default())
        })
        .collect();
    for (edge, label) in model.edges.iter().zip(&edge_labels) {
        let (a, b) = (edge.lhs.node, edge.rhs.node);
        if label.width == 0.0 {
            continue;
        }
        if node_col[a] != node_col[b] {
            let c = node_col[a].min(node_col[b]);
            gap_x[c] = js::max(gap_x[c], label.width.ceil() + 28.0);
        } else {
            col_w[node_col[a]] = js::max(col_w[node_col[a]], 2.0 * (label.width / 2.0).ceil() + 8.0);
            if node_row[a] != node_row[b] {
                let r = node_row[a].min(node_row[b]);
                gap_y[r] = js::max(gap_y[r], label.height.ceil() + 24.0);
            }
        }
    }

    // Room around each group, kept on the rows and columns at its edges. A group that shares an
    // edge with a group inside it stands that much further out. Children come after their
    // parents, so walking backwards settles every group before the one that holds it.
    let span = |rank: &Rank, at: i64, cells: i64| (rank.get(at), rank.get(at + cells - 1));
    let cols: Vec<(usize, usize)> =
        (0..g_count).map(|g| span(&col_of, grid.group_col[g], grid.group_cols[g])).collect();
    let rows: Vec<(usize, usize)> =
        (0..g_count).map(|g| span(&row_of, grid.group_row[g], grid.group_rows[g])).collect();
    let mut ins_l = vec![0.0; g_count];
    let mut ins_r = vec![0.0; g_count];
    let mut ins_t = vec![0.0; g_count];
    let mut ins_b = vec![0.0; g_count];
    let mut col_l = vec![0.0; c_count];
    let mut col_r = vec![0.0; c_count];
    let mut row_t = vec![0.0; r_count];
    let mut row_b = vec![0.0; r_count];
    let room = if tight { js::max(80.0, fit::title_room(cx.options) / 2.0) } else { 320.0 };
    let group_labels: Vec<Label> = groups
        .iter()
        .map(|group| {
            let title = group.title.as_deref().unwrap_or_default();
            layout_label(title, true, measurer, font, room, Emphasis::default())
        })
        .collect();
    let has_icon = |g: usize| groups[g].icon.as_ref().is_some_and(|icon| !icon.is_empty());
    let head_w: Vec<f64> = (0..g_count)
        .map(|g| {
            let label = &group_labels[g];
            let icon = if has_icon(g) { sizes.badge } else { 0.0 };
            label.width + icon + if label.width > 0.0 && icon > 0.0 { 6.0 } else { 0.0 }
        })
        .collect();
    let head_h: Vec<f64> =
        (0..g_count).map(|g| js::max(group_labels[g].height, if has_icon(g) { sizes.badge } else { 0.0 })).collect();
    for g in (0..g_count).rev() {
        let (c0, c1) = cols[g];
        let (r0, r1) = rows[g];
        let left = sizes.group_pad + ins_l[g];
        let mut right = sizes.group_pad + ins_r[g];
        let top =
            (if head_h[g] > 0.0 { js::max(sizes.group_pad, head_h[g] + 12.0) } else { sizes.group_pad }) + ins_t[g];
        let bottom = sizes.group_pad + ins_b[g];
        if head_w[g] > 0.0 {
            let mut inner = left + right;
            for c in c0..=c1 {
                inner += col_w[c] + if c < c1 { col_r[c] + gap_x[c] + col_l[c + 1] } else { 0.0 };
            }
            if head_w[g] + 20.0 > inner {
                right += (head_w[g] + 20.0 - inner).ceil();
            }
        }
        ins_l[g] = left;
        ins_r[g] = right;
        ins_t[g] = top;
        ins_b[g] = bottom;
        col_l[c0] = js::max(col_l[c0], left);
        col_r[c1] = js::max(col_r[c1], right);
        row_t[r0] = js::max(row_t[r0], top);
        row_b[r1] = js::max(row_b[r1], bottom);
        let Some(p) = grid.group_parent[g] else {
            continue;
        };
        if cols[p].0 == c0 {
            ins_l[p] = js::max(ins_l[p], left);
        }
        if cols[p].1 == c1 {
            ins_r[p] = js::max(ins_r[p], right);
        }
        if rows[p].0 == r0 {
            ins_t[p] = js::max(ins_t[p], top);
        }
        if rows[p].1 == r1 {
            ins_b[p] = js::max(ins_b[p], bottom);
        }
    }

    let mut col_x = vec![0.0; c_count];
    let mut row_y = vec![0.0; r_count];
    let mut inner_w = 0.0;
    for c in 0..c_count {
        col_x[c] = inner_w + col_l[c];
        inner_w = col_x[c] + col_w[c] + col_r[c] + if c < c_count - 1 { gap_x[c] } else { 0.0 };
    }
    let mut inner_h = 0.0;
    for r in 0..r_count {
        row_y[r] = inner_h + row_t[r];
        inner_h = row_y[r] + row_h[r] + row_b[r] + if r < r_count - 1 { gap_y[r] } else { 0.0 };
    }
    // What is drawn, which an edge that has to go around a node can push past the grid.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (0.0, 0.0, inner_w, inner_h);

    let left = |g: usize| col_x[cols[g].0] - ins_l[g];
    let right = |g: usize| col_x[cols[g].1] + col_w[cols[g].1] + ins_r[g];
    let top = |g: usize| row_y[rows[g].0] - ins_t[g];
    let bottom = |g: usize| row_y[rows[g].1] + row_h[rows[g].1] + ins_b[g];
    let center_x = |i: usize| col_x[node_col[i]] + col_w[node_col[i]] / 2.0;
    let junction = |i: usize| nodes[i].kind == NodeKind::Junction;

    let port = |end: &End| -> Port {
        let i = end.node;
        let side = end.side.index();
        let (dx, dy) = (DX[side], DY[side]);
        let cx = center_x(i);
        let y = row_y[node_row[i]];
        let mut p = Port {
            x: cx,
            y: y + half,
            dx,
            dy,
            x0: cx - box_w[i] / 2.0,
            y0: y,
            x1: cx + box_w[i] / 2.0,
            y1: y + box_h[i],
            gap: 0.0,
            wire: false,
        };
        if junction(i) {
            (p.x0, p.x1) = (cx, cx);
            (p.y0, p.y1) = (p.y, p.y);
            p.gap = DOT;
        } else if dx != 0.0 {
            p.x = cx + dx * half;
        } else {
            p.y = if dy < 0.0 { y } else { y + box_h[i] + if labels[i].height > 0.0 { 2.0 } else { 0.0 } };
        }
        if let Some(g) = grid.node_parent[i].filter(|_| end.via_group) {
            (p.x0, p.x1, p.y0, p.y1) = (left(g), right(g), top(g), bottom(g));
            p.gap = 0.0;
            p.wire = junction(i);
            if dx != 0.0 {
                p.x = if dx < 0.0 { p.x0 } else { p.x1 };
            } else {
                p.y = if dy < 0.0 { p.y0 } else { p.y1 };
            }
        }
        p
    };

    let mut edges = Vec::new();
    // Vertical runs of the edges, to keep group titles out from under them.
    let mut runs: Vec<[f64; 3]> = Vec::new();
    let trim_length = Marker::Point.trim();
    for (k, edge) in model.edges.iter().enumerate() {
        let (a, b) = (edge.lhs.node, edge.rhs.node);
        let mut s = port(&edge.lhs);
        let mut e = port(&edge.rhs);
        if s.x == e.x && s.y == e.y && s.dx == e.dx {
            // An edge from a side back to the same side: part the two ends so it shows as a loop.
            s.x -= 8.0 * s.dy.abs();
            s.y -= 8.0 * s.dx.abs();
            e.x += 8.0 * s.dy.abs();
            e.y += 8.0 * s.dx.abs();
        }
        let mut routed = route(&s, &e);
        if s.wire {
            routed.splice(0..0, [center_x(a), row_y[node_row[a]] + half]);
        }
        if e.wire {
            routed.extend([center_x(b), row_y[node_row[b]] + half]);
        }
        let points = simplify(&routed);
        if points.len() < 4 {
            continue;
        }
        let start_trim = if edge.lhs.into && !s.wire { trim_length + s.gap } else { 0.0 };
        let end_trim = if edge.rhs.into && !e.wire { trim_length + e.gap } else { 0.0 };
        let tip = |p: &Port| Tip { x: p.x + p.dx * p.gap, y: p.y + p.dy * p.gap, dx: -p.dx, dy: -p.dy };
        let mut view = EdgeView {
            edge: k,
            path: path_data(&points, start_trim, end_trim),
            start: edge.lhs.into.then(|| tip(&s)),
            end: edge.rhs.into.then(|| tip(&e)),
            label: None,
        };
        for pair in points.chunks(2) {
            min_x = js::min(min_x, pair[0] - 4.0);
            max_x = js::max(max_x, pair[0] + 4.0);
            min_y = js::min(min_y, pair[1] - 4.0);
            max_y = js::max(max_y, pair[1] + 4.0);
        }
        let (mut best, mut best_length, mut flat, mut flat_length) = (0, -1.0, 0, -1.0);
        for i in (0..points.len() - 3).step_by(2) {
            let horizontal = (points[i + 1] - points[i + 3]).abs() < 0.01;
            let length = (points[i] - points[i + 2]).abs() + (points[i + 1] - points[i + 3]).abs();
            if !horizontal {
                let (lo, hi) = (js::min(points[i + 1], points[i + 3]), js::max(points[i + 1], points[i + 3]));
                runs.push([points[i], lo, hi]);
            }
            if length > best_length {
                (best, best_length) = (i, length);
            }
            if horizontal && length > flat_length {
                (flat, flat_length) = (i, length);
            }
        }
        let label = &edge_labels[k];
        if label.width > 0.0 {
            let at = if flat_length >= label.width + 8.0 { flat } else { best };
            let mut x = (points[at] + points[at + 2]) / 2.0;
            let mut y = (points[at + 1] + points[at + 3]) / 2.0;
            let w = label.width + 8.0;
            let h = label.height;
            let upright = (points[at] - points[at + 2]).abs() < 0.01;
            // On a short run next to a node the label would land on the node, so it moves off to
            // the side.
            for p in [&s, &e] {
                if x + w / 2.0 <= p.x0 || x - w / 2.0 >= p.x1 || y + h / 2.0 <= p.y0 || y - h / 2.0 >= p.y1 {
                    continue;
                }
                if upright {
                    x = if x >= (p.x0 + p.x1) / 2.0 { p.x1 + w / 2.0 + 4.0 } else { p.x0 - w / 2.0 - 4.0 };
                } else {
                    y = if y >= (p.y0 + p.y1) / 2.0 { p.y1 + h / 2.0 + 4.0 } else { p.y0 - h / 2.0 - 4.0 };
                }
            }
            min_x = js::min(min_x, x - w / 2.0);
            max_x = js::max(max_x, x + w / 2.0);
            min_y = js::min(min_y, y - h / 2.0);
            max_y = js::max(max_y, y + h / 2.0);
            view.label = Some((label.clone(), Point { x, y }));
        }
        edges.push(view);
    }

    let mut clusters = Vec::with_capacity(g_count);
    for (g, label) in group_labels.into_iter().enumerate() {
        let (x, y) = (left(g), top(g));
        let w = right(g) - x;
        let head = (head_w[g] > 0.0).then(|| {
            // The title starts at the left, or after the edges that come down through that corner.
            let band = y + head_h[g] + 12.0;
            let mut crossings: Vec<f64> = runs
                .iter()
                .filter(|run| run[1] < band && run[2] > y && run[0] > x && run[0] < x + w)
                .map(|run| run[0])
                .collect();
            crossings.sort_by(|a, b| crate::layout::num::by_difference(a - b));
            crossings.push(x + w - 4.0);
            let mut start = x + 10.0;
            for crossing in crossings {
                if crossing - 6.0 - start >= head_w[g] {
                    break;
                }
                start = js::max(start, crossing + 6.0);
            }
            if start + head_w[g] > x + w - 4.0 {
                start = x + 10.0;
            }
            let cy = y + 6.0 + head_h[g] / 2.0;
            let mut text_x = start;
            let icon = has_icon(g).then(|| (start, cy - sizes.badge / 2.0));
            if icon.is_some() {
                text_x += sizes.badge + 6.0;
            }
            Head { icon, at: Point { x: text_x + label.width / 2.0, y: cy }, label }
        });
        clusters.push(ClusterView { group: g, x, y, w, h: bottom(g) - y, head });
    }

    let glyph = icon * 0.5;
    let text_size = js::max(6.0, js::round(font * 0.75));
    let views = labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| {
            let inner = match &nodes[i].kind {
                NodeKind::Service(Service { icon: Some(name), .. }) if !name.is_empty() => {
                    Inner::Icon { x: -glyph / 2.0, y: (icon - glyph) / 2.0, side: glyph }
                }
                NodeKind::Service(Service { icon_text: Some(text), .. }) if !text.is_empty() => {
                    let laid = layout_label(text, false, measurer, text_size, icon - 8.0, Emphasis::default());
                    let line = if laid.line_height == 0.0 { 1.0 } else { laid.line_height };
                    let lines = js::max(1.0, ((icon - 4.0) / line).floor()) as usize;
                    Inner::Text(fit(laid, icon - 8.0, lines, measurer))
                }
                _ => Inner::None,
            };
            NodeView { node: i, at: Point { x: center_x(i), y: row_y[node_row[i]] }, inner, label }
        })
        .collect();

    let title_text = model.title.as_deref().unwrap_or_default();
    let title_room = if tight { fit::title_room(cx.options) } else { 4000.0 };
    let title = layout_label(title_text, false, measurer, font, title_room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let pad = sizes.pad;
    let width = (js::max(max_x - min_x, title.width) + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let offset =
        Point { x: js::round((width - (max_x - min_x)) / 2.0 - min_x), y: js::round(pad + title_height - min_y) };
    let title_at = Point { x: width / 2.0, y: pad + title.height / 2.0 };
    Plan {
        sizes,
        text_size,
        size: Size { width, height },
        title: TitleLayout { label: title, at: title_at },
        offset,
        clusters,
        edges,
        nodes: views,
    }
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

/// One draw as a Scene: the title, then the groups, edges, edge labels and nodes.
fn draw(model: &ArchitectureModel, cx: &DrawContext<'_>, tight: bool) -> Result<Scene, RenderError> {
    let plan = plan(model, cx, tight);
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, plan.size, accessible).root_font_size(plan.sizes.font);
    out.title(&plan.title);

    let node_parts: Vec<PartId> = model.nodes.iter().map(|node| out.part(Entity::node(&node.id))).collect();
    let mut seen: HashMap<EntityName, u32> = HashMap::new();
    let edge_parts: Vec<PartId> = plan
        .edges
        .iter()
        .map(|view| {
            let edge = &model.edges[view.edge];
            let (from, to) = (&model.nodes[edge.lhs.node].id, &model.nodes[edge.rhs.node].id);
            let mut id = from.clone();
            id.push(u16::from(b'-'));
            id.push_units(to);
            let kind = format!("{}-{}", edge.lhs.side.letter(), edge.rhs.side.letter());
            let name = EntityName::Between { from: from.to_vec(), to: to.to_vec(), kind };
            let duplicate = seen.entry(name.clone()).or_insert(0);
            let part = out.part(Entity::edge(&id, name, *duplicate));
            *duplicate += 1;
            out.relation(part, node_parts[edge.lhs.node], node_parts[edge.rhs.node]);
            part
        })
        .collect();
    let glyph = |name: &JsText| IconGlyph::named(name);

    out.layer(LayerSpec::group().translate(plan.offset), |out| {
        out.layer(LayerSpec::class("pele-clusters"), |out| {
            for cluster in &plan.clusters {
                let group = &model.groups[cluster.group];
                let part = out.part(Entity::group(&group.id));
                out.layer(LayerSpec::class("pele-cluster").part(part, Facet::Shape).data_id(), |out| {
                    out.element(part, Facet::Shape)
                        .rect(pair(cluster.x, cluster.y), pair(cluster.w, cluster.h), None)
                        .fill(Paint::Role(Role::SurfaceAlt))
                        .fill_opacity(Number::raw(0.5))
                        .stroke(Paint::Role(Role::Border))
                        .style(Some(Style::rounded(None)))
                        .finish();
                    let Some(head) = &cluster.head else {
                        return;
                    };
                    if let (Some((x, y)), Some(name)) = (head.icon, &group.icon) {
                        let side = Number::num(plan.sizes.badge);
                        out.icon_with(part, name, pair(x, y), side, glyph(name));
                    }
                    let look = LabelLook::class("pele-cluster-label").fill(Some(Paint::Role(Role::Muted)));
                    out.label(part, Facet::Label, &head.label, head.at, look);
                });
            }
        });

        let lines = LayerSpec::class("pele-edges")
            .fill(Paint::None)
            .stroke(Paint::Role(Role::Line))
            .cap(LineCap::Round)
            .join(LineJoin::Round);
        out.layer(lines, |out| {
            for (view, &part) in plan.edges.iter().zip(&edge_parts) {
                out.layer(LayerSpec::class("pele-edge").part(part, Facet::Shape).data_id(), |out| {
                    out.element(part, Facet::Shape).path(view.path.clone()).finish();
                    let ink = MarkerUse::Edge(Paint::Role(Role::Line));
                    if let Some(tip) = view.start {
                        draw_marker(out, part, Facet::StartMarker, Marker::Point, tip, ink);
                    }
                    if let Some(tip) = view.end {
                        draw_marker(out, part, Facet::EndMarker, Marker::Point, tip, ink);
                    }
                });
            }
        });

        out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(plan.sizes.small)), |out| {
            for (view, &part) in plan.edges.iter().zip(&edge_parts) {
                if let Some((label, at)) = &view.label {
                    draw_edge_label_as(out, part, label, *at, LabelLook::default());
                }
            }
        });

        out.layer(LayerSpec::class("pele-nodes"), |out| {
            for view in &plan.nodes {
                let node = &model.nodes[view.node];
                let part = node_parts[view.node];
                let (half, icon) = (plan.sizes.half, plan.sizes.icon);
                let NodeKind::Service(service) = &node.kind else {
                    let class = LayerSpec::class("pele-node pele-junction");
                    out.layer(class.part(part, Facet::Shape).data_id().translate(view.at), |out| {
                        out.element(part, Facet::Shape)
                            .circle_at([None, Some(Number::num(half))], Number::raw(DOT))
                            .fill(Paint::Role(Role::Line))
                            .finish();
                    });
                    continue;
                };
                let class = LayerSpec::class("pele-node pele-service");
                out.layer(class.part(part, Facet::Shape).data_id().translate(view.at), |out| {
                    out.element(part, Facet::Shape)
                        .rect_from_top(Number::num(-half), [Number::num(icon), Number::num(icon)])
                        .fill(Paint::Role(Role::Surface))
                        .stroke(Paint::Role(Role::Border))
                        .style(Some(Style::rounded(None)))
                        .finish();
                    match (&view.inner, &service.icon) {
                        (Inner::Icon { x, y, side }, Some(name)) => {
                            out.icon_with(part, name, pair(*x, *y), Number::num(*side), glyph(name));
                        }
                        (Inner::Text(text), _) => {
                            let look = LabelLook::class("pele-icon-text").font_size(Number::raw(plan.text_size));
                            out.label(part, Facet::Shape, text, Point { x: 0.0, y: half }, look);
                        }
                        _ => {}
                    }
                    let at = Point { x: 0.0, y: icon + LABEL_GAP + view.label.height / 2.0 };
                    out.label(part, Facet::Label, &view.label, at, LabelLook::class("pele-label"));
                });
            }
        });
    });
    out.finish()
}

pub(crate) struct Architecture;

impl Diagram for Architecture {
    type Model = ArchitectureModel;

    fn parse(cx: &ParseContext<'_>) -> Result<ArchitectureModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderArchitecture`: drawn with room to spare, and tightly when that is too wide.
    fn scene(model: &ArchitectureModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        fit::tighten(cx.options, |tight| draw(model, cx, tight))
    }
}
