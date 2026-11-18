//! Drawing a block diagram, ported from Pele's `src/diagrams/block/render.ts`: blocks are sized
//! from their labels and laid out on nested grids, edges are routed between them with their
//! labels kept apart, and the whole is drawn as a Scene.

use super::arrows::{Axis, OVERHANG, arrow_axis, arrow_mask, arrow_points, arrow_size};
use super::layout::{Cell, cell, layout};
use super::{BlockEdge, BlockModel, BlockNode, Head, Kind};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::diagrams::flowchart::Shape;
use crate::graph::edges::{
    EdgePath, Fill, Marker, MarkerUse, Over, Tip, casings_layer, draw_casing, draw_edge_label, draw_marker, over_fills,
};
use crate::graph::shapes::{draw_shape, shape_filled, shape_inset, shape_size};
use crate::input::{JsText, JsValue, Units};
use crate::layout::num::hypot;
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, AuthorClasses, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineJoin, LineLook, Number,
    Paint, PartId, PathProgram, PathStep, Point, ResolvedStyle, Role, Scene, SceneBuilder, ShapeLook, Size, Style,
    TitleLayout, ink_on, paints, resolve_style,
};
use crate::{RenderError, fit, js};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

const GAP: f64 = 12.0;
const WRAP: f64 = 200.0;
const CORNER: f64 = 8.0;
/// Checking every edge against every block is quadratic, so large diagrams route without it.
const OBSTACLE_BUDGET: usize = 2_000_000;
const LABEL_GAP: f64 = 4.0;
/// Pele's `ARROW` of `svg/edges.ts`: the length of an arrowhead.
const ARROW: f64 = 8.0;
const SPOTS: [f64; 5] = [0.5, 0.3, 0.7, 0.15, 0.85];

/// Pele's `SHAPES`: the shape each kind of block is drawn as. A block of no kind is a rectangle.
fn shape_of(kind: Kind) -> Shape {
    match kind {
        Kind::Round => Shape::Rounded,
        Kind::Circle => Shape::Circle,
        Kind::DoubleCircle => Shape::DblCirc,
        Kind::Diamond => Shape::Diam,
        Kind::Hexagon => Shape::Hex,
        Kind::Stadium => Shape::Stadium,
        Kind::Subroutine => Shape::FrRect,
        Kind::Cylinder => Shape::Cyl,
        Kind::RectLeftInvArrow => Shape::Odd,
        Kind::LeanRight => Shape::LeanR,
        Kind::LeanLeft => Shape::LeanL,
        Kind::Trapezoid => Shape::TrapB,
        Kind::InvTrapezoid => Shape::TrapT,
        _ => Shape::Rect,
    }
}

/// Shapes that keep their own size in a larger cell, where the others stretch to fill it.
fn fixed(shape: Shape) -> bool {
    matches!(shape, Shape::Circle | Shape::DblCirc | Shape::Diam)
}

/// Shapes that only widen, because their slants and curves are set by their height.
fn wide(shape: Shape) -> bool {
    matches!(
        shape,
        Shape::Stadium | Shape::Hex | Shape::Odd | Shape::LeanR | Shape::LeanL | Shape::TrapB | Shape::TrapT
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewKind {
    Leaf(Shape),
    Arrow,
    Composite,
    Space,
}

/// A block as it is drawn. Its cell in the layout has the same index.
struct View<'m> {
    block: &'m BlockNode,
    kind: ViewKind,
    index: usize,
    label: Label,
    style: ResolvedStyle,
    mask: u8,
    dx: f64,
    dy: f64,
    /// Where the layout put it.
    x: f64,
    y: f64,
    /// Size as drawn, which for a fixed shape is smaller than its cell.
    bw: f64,
    bh: f64,
}

impl View<'_> {
    fn is_round(&self) -> bool {
        matches!(self.kind, ViewKind::Leaf(Shape::Circle | Shape::DblCirc))
    }

    fn is_diamond(&self) -> bool {
        self.kind == ViewKind::Leaf(Shape::Diam)
    }

    fn emphasis(&self) -> Emphasis {
        Emphasis { bold: self.style.bold, italic: self.style.italic, ..Emphasis::default() }
    }
}

/// A box that labels must keep clear of: a block, or the title of a container, which is no block
/// an edge joins.
#[derive(Clone, Copy)]
struct Obstacle {
    view: Option<usize>,
    x: f64,
    y: f64,
    bw: f64,
    bh: f64,
}

impl Obstacle {
    fn of(v: &View<'_>) -> Self {
        Obstacle { view: Some(v.index), x: v.x, y: v.y, bw: v.bw, bh: v.bh }
    }

    fn is(&self, v: &View<'_>) -> bool {
        self.view == Some(v.index)
    }
}

/// What one run of the layout leaves: the size of the grid, the blocks labels must keep clear
/// of, and the titles of the containers.
struct Placement {
    content: (f64, f64),
    obstacles: Vec<Obstacle>,
    titles: Vec<Obstacle>,
}

struct Route {
    /// Two points for a straight line, three for a bend, none for a self-loop.
    points: Vec<f64>,
    /// Counts the self-loops on the block up to this one, or 0 for an edge between two blocks.
    loop_k: f64,
    label: Label,
    lx: f64,
    ly: f64,
}

/// `x || 1`, for a length that may be 0 or NaN.
fn or_one(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() { 1.0 } else { x }
}

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

/// Where a vertical line at x meets the outline, below the centre (sign 1) or above it (sign -1).
fn edge_y(v: &View<'_>, x: f64, sign: f64) -> f64 {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let dx = (x - v.x).abs();
    let reach = match v.kind {
        _ if v.is_round() => js::max(hw * hw - dx * dx, 0.0).sqrt(),
        _ if v.is_diamond() => hh * js::max(1.0 - dx / hw, 0.0),
        ViewKind::Leaf(shape) => hh - shape_inset(shape, v.bw, v.bh, if sign > 0.0 { 2 } else { 0 }, 0.0),
        ViewKind::Arrow if arrow_axis(v.mask) == Axis::X => hh - OVERHANG,
        _ => hh,
    };
    v.y + sign * reach
}

/// Where a horizontal line at y meets the outline, right of the centre (sign 1) or left of it
/// (sign -1).
fn edge_x(v: &View<'_>, y: f64, sign: f64) -> f64 {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let dy = (y - v.y).abs();
    let reach = match v.kind {
        _ if v.is_round() => js::max(hh * hh - dy * dy, 0.0).sqrt(),
        _ if v.is_diamond() => hw * js::max(1.0 - dy / hh, 0.0),
        ViewKind::Leaf(shape) => hw - shape_inset(shape, v.bw, v.bh, if sign > 0.0 { 1 } else { 3 }, 0.0),
        ViewKind::Arrow if arrow_axis(v.mask) == Axis::Y => hw - OVERHANG,
        _ => hw,
    };
    v.x + sign * reach
}

/// Where the line from the centre toward (tx, ty) leaves the outline.
fn border(v: &View<'_>, tx: f64, ty: f64, out: &mut Vec<f64>) {
    let dx = tx - v.x;
    let dy = ty - v.y;
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let mut k = if v.is_round() {
        hw / or_one(hypot(dx, dy))
    } else if v.is_diamond() {
        1.0 / or_one(dx.abs() / hw + dy.abs() / hh)
    } else {
        let across = if dx == 0.0 { f64::INFINITY } else { hw / dx.abs() };
        let along = if dy == 0.0 { f64::INFINITY } else { hh / dy.abs() };
        js::min(across, along)
    };
    if k == f64::INFINITY {
        k = 0.0;
    }
    out.extend([v.x + dx * k, v.y + dy * k]);
}

fn crosses_box(x1: f64, y1: f64, x2: f64, y2: f64, v: &Obstacle) -> bool {
    let l = v.x - v.bw / 2.0 + 1.0;
    let r = v.x + v.bw / 2.0 - 1.0;
    let t = v.y - v.bh / 2.0 + 1.0;
    let b = v.y + v.bh / 2.0 - 1.0;
    if (x1 < l && x2 < l) || (x1 > r && x2 > r) || (y1 < t && y2 < t) || (y1 > b && y2 > b) {
        return false;
    }
    if x1 == x2 || y1 == y2 {
        return true;
    }
    // The segment is diagonal and its bounding box overlaps: the box is hit unless all four
    // corners are on one side.
    let dx = x2 - x1;
    let dy = y2 - y1;
    let side = |x: f64, y: f64| sign(dx * (y - y1) - dy * (x - x1));
    let first = side(l, t);
    first == 0.0 || side(r, t) != first || side(r, b) != first || side(l, b) != first
}

fn hits(points: &[f64], s: &View<'_>, t: &View<'_>, obstacles: &[Obstacle]) -> usize {
    let mut count = 0;
    for v in obstacles {
        if v.is(s) || v.is(t) {
            continue;
        }
        let mut i = 0;
        while i + 3 < points.len() {
            if crosses_box(points[i], points[i + 1], points[i + 2], points[i + 3], v) {
                count += 1;
                break;
            }
            i += 2;
        }
    }
    count
}

/// A straight line where the two blocks share a row or a column, otherwise one bend between
/// facing sides.
fn route_edge(s: &View<'_>, t: &View<'_>, shift: f64, obstacles: Option<&[Obstacle]>) -> Vec<f64> {
    let sl = s.x - s.bw / 2.0;
    let sr = s.x + s.bw / 2.0;
    let st = s.y - s.bh / 2.0;
    let sb = s.y + s.bh / 2.0;
    let tl = t.x - t.bw / 2.0;
    let tr = t.x + t.bw / 2.0;
    let tt = t.y - t.bh / 2.0;
    let tb = t.y + t.bh / 2.0;
    let x1 = js::max(sl, tl);
    let x2 = js::min(sr, tr);
    let y1 = js::max(st, tt);
    let y2 = js::min(sb, tb);
    let over_x = x2 - x1 > 6.0;
    let over_y = y2 - y1 > 6.0;

    if over_x && over_y {
        // One block is inside the other: join the inner one to the nearest side of the outer one.
        let s_is_inner = s.bw * s.bh <= t.bw * t.bh;
        let (inner, outer) = if s_is_inner { (s, t) } else { (t, s) };
        let up = inner.y - inner.bh / 2.0 - (outer.y - outer.bh / 2.0);
        let down = outer.y + outer.bh / 2.0 - (inner.y + inner.bh / 2.0);
        let left = inner.x - inner.bw / 2.0 - (outer.x - outer.bw / 2.0);
        let right = outer.x + outer.bw / 2.0 - (inner.x + inner.bw / 2.0);
        let least = js::min(js::min(js::min(up, down), left), right);
        let points = if least == down {
            vec![inner.x, edge_y(inner, inner.x, 1.0), inner.x, outer.y + outer.bh / 2.0]
        } else if least == up {
            vec![inner.x, edge_y(inner, inner.x, -1.0), inner.x, outer.y - outer.bh / 2.0]
        } else if least == right {
            vec![edge_x(inner, inner.y, 1.0), inner.y, outer.x + outer.bw / 2.0, inner.y]
        } else {
            vec![edge_x(inner, inner.y, -1.0), inner.y, outer.x - outer.bw / 2.0, inner.y]
        };
        return if s_is_inner { points } else { vec![points[2], points[3], points[0], points[1]] };
    }
    if over_x {
        let margin = js::min(6.0, (x2 - x1) / 2.0);
        let x = js::min(js::max((x1 + x2) / 2.0 + shift, x1 + margin), x2 - margin);
        let down = if t.y > s.y { 1.0 } else { -1.0 };
        return vec![x, edge_y(s, x, down), x, edge_y(t, x, -down)];
    }
    if over_y {
        let margin = js::min(6.0, (y2 - y1) / 2.0);
        let y = js::min(js::max((y1 + y2) / 2.0 + shift, y1 + margin), y2 - margin);
        let right = if t.x > s.x { 1.0 } else { -1.0 };
        return vec![edge_x(s, y, right), y, edge_x(t, y, -right), y];
    }

    let right = if t.x > s.x { 1.0 } else { -1.0 };
    let down = if t.y > s.y { 1.0 } else { -1.0 };
    let across = vec![edge_x(s, s.y, right), s.y, t.x, s.y, t.x, edge_y(t, t.x, -down)];
    let along = vec![s.x, edge_y(s, s.x, down), s.x, t.y, edge_x(t, t.y, -right), t.y];
    let wide = (t.x - s.x).abs() >= (t.y - s.y).abs();
    let (mut best, other) = if wide { (across, along) } else { (along, across) };
    let Some(obstacles) = obstacles else {
        return best;
    };
    let mut least = hits(&best, s, t, obstacles);
    if least == 0 {
        return best;
    }
    let count = hits(&other, s, t, obstacles);
    if count < least {
        best = other;
        least = count;
    }
    if least == 0 {
        return best;
    }
    let mut direct = Vec::new();
    border(s, t.x, t.y, &mut direct);
    border(t, s.x, s.y, &mut direct);
    if hits(&direct, s, t, obstacles) < least { direct } else { best }
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

/// Pele's `trace`: the path through two or three points, rounding the bend and leaving room for
/// markers.
fn trace(p: &[f64], mut start_trim: f64, mut end_trim: f64) -> EdgePath {
    let n = p.len();
    let (sx, sy) = (p[0], p[1]);
    let (ex, ey) = (p[n - 2], p[n - 1]);
    let mut ax = p[2] - sx;
    let mut ay = p[3] - sy;
    let first = hypot(ax, ay);
    ax /= or_one(first);
    ay /= or_one(first);
    let mut bx = ex - p[n - 4];
    let mut by = ey - p[n - 3];
    let last = hypot(bx, by);
    bx /= or_one(last);
    by /= or_one(last);
    if n == 4 && start_trim + end_trim > first {
        let scale = first / (start_trim + end_trim);
        start_trim *= scale;
        end_trim *= scale;
    }
    start_trim = js::min(start_trim, first);
    end_trim = js::min(end_trim, last);
    let mut steps = vec![PathStep::Move(pair(sx + ax * start_trim, sy + ay * start_trim))];
    if n == 6 {
        let r = js::max(0.0, js::min(js::min(CORNER, first - start_trim), last - end_trim));
        steps.push(PathStep::Line(pair(p[2] - ax * r, p[3] - ay * r)));
        steps.push(PathStep::Quad([pair(p[2], p[3]), pair(p[2] + bx * r, p[3] + by * r)]));
    }
    steps.push(PathStep::Line(pair(ex - bx * end_trim, ey - by * end_trim)));
    EdgePath {
        d: PathProgram { steps },
        start: Tip { x: sx, y: sy, dx: -ax, dy: -ay },
        end: Tip { x: ex, y: ey, dx: bx, dy: by },
    }
}

/// A self-loop is an arc around the top right corner of the block, or around the matching point
/// on a round or diamond outline. Gives the arc's centre and radius and the angles where it
/// leaves and returns.
fn loop_arc(v: &View<'_>, k: f64) -> [f64; 5] {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let r = js::max(1.0, js::min(js::min(10.0 + 5.0 * (k - 1.0), hw), hh));
    if v.is_round() {
        let px = v.x + hw * FRAC_1_SQRT_2;
        let py = v.y - hw * FRAC_1_SQRT_2;
        let half = 2.0 * libm::asin(js::min(1.0, r / (2.0 * hw)));
        let angle = |at: f64| libm::atan2(v.y - hw * js::sin(at) - py, v.x + hw * js::cos(at) - px);
        return [px, py, r, angle(PI / 4.0 + half), angle(PI / 4.0 - half)];
    }
    if v.is_diamond() {
        let slope = libm::atan2(hh, hw);
        return [v.x + hw / 2.0, v.y - hh / 2.0, r, slope + PI, slope];
    }
    [v.x + hw, v.y - hh, r, PI, PI / 2.0]
}

fn loop_path(v: &View<'_>, k: f64, start_trim: f64, end_trim: f64) -> EdgePath {
    let [px, py, r, a1, a2] = loop_arc(v, k);
    let from = a1 + start_trim / r;
    let to = a2 - end_trim / r;
    let mut sweep = (to - from) % (2.0 * PI);
    if sweep <= 0.0 {
        sweep += 2.0 * PI;
    }
    let x1 = px + r * js::cos(from);
    let y1 = py + r * js::sin(from);
    let x2 = px + r * js::cos(to);
    let y2 = py + r * js::sin(to);
    let sx = px + r * js::cos(a1);
    let sy = py + r * js::sin(a1);
    let ex = px + r * js::cos(a2);
    let ey = py + r * js::sin(a2);
    // Each marker lies along the chord it covers, so it follows the curve instead of the tangent
    // at its tip.
    let sl = hypot(sx - x1, sy - y1);
    let el = hypot(ex - x2, ey - y2);
    let radii = [Number::num(r); 2];
    let d = PathProgram::default().m(Number::num(x1), Number::num(y1));
    let d = if sweep > PI { d.a_large(radii, true, pair(x2, y2)) } else { d.a(radii, true, pair(x2, y2)) };
    let start = if sl > 0.0 {
        Tip { x: sx, y: sy, dx: (sx - x1) / sl, dy: (sy - y1) / sl }
    } else {
        Tip { x: sx, y: sy, dx: js::sin(a1), dy: -js::cos(a1) }
    };
    let end = if el > 0.0 {
        Tip { x: ex, y: ey, dx: (ex - x2) / el, dy: (ey - y2) / el }
    } else {
        Tip { x: ex, y: ey, dx: -js::sin(a2), dy: js::cos(a2) }
    };
    EdgePath { d, start, end }
}

fn covers(x: f64, y: f64, w: f64, h: f64, s: &View<'_>, t: &View<'_>, obstacles: Option<&[Obstacle]>) -> bool {
    let Some(obstacles) = obstacles else {
        return false;
    };
    obstacles
        .iter()
        .any(|v| !v.is(s) && !v.is(t) && (x - v.x).abs() < (w + v.bw) / 2.0 && (y - v.y).abs() < (h + v.bh) / 2.0)
}

/// Picks the point on a leg where a label of the given size covers no block other than the two
/// it joins.
fn label_spot(p: &[f64], at: usize, size: [f64; 2], s: &View<'_>, t: &View<'_>, obstacles: Option<&[Obstacle]>) -> f64 {
    if obstacles.is_none() {
        return 0.5;
    }
    for spot in SPOTS {
        let x = p[at] + (p[at + 2] - p[at]) * spot;
        let y = p[at + 1] + (p[at + 3] - p[at + 1]) * spot;
        if !covers(x, y, size[0], size[1], s, t, obstacles) {
            return spot;
        }
    }
    0.5
}

/// Stacks overlapping labels along their shared line, if they fit between its markers and cover
/// no other block or title. Answers the extra length needed to fit the labels.
fn stack_labels(
    stack: &[usize],
    routes: &mut [Route],
    legs: &[[f64; 4]],
    s: &View<'_>,
    t: &View<'_>,
    obstacles: Option<&[Obstacle]>,
) -> f64 {
    let vertical = legs[stack[0]][0] == legs[stack[0]][2];
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    let mut need = -LABEL_GAP;
    let mut crowded = false;
    for (a, &edge) in stack.iter().enumerate() {
        let leg = legs[edge];
        if if vertical { leg[0] != leg[2] } else { leg[1] != leg[3] } {
            return 0.0;
        }
        lo = js::max(lo, if vertical { js::min(leg[1], leg[3]) } else { js::min(leg[0], leg[2]) });
        hi = js::min(hi, if vertical { js::max(leg[1], leg[3]) } else { js::max(leg[0], leg[2]) });
        let p = &routes[edge];
        need += (if vertical { p.label.height } else { p.label.width + 8.0 }) + LABEL_GAP;
        for &other in &stack[..a] {
            let q = &routes[other];
            if (p.lx - q.lx).abs() < (p.label.width + q.label.width) / 2.0 + 8.0
                && (p.ly - q.ly).abs() < (p.label.height + q.label.height) / 2.0
            {
                crowded = true;
            }
        }
    }
    if !crowded {
        return 0.0;
    }
    if need > hi - lo - 2.0 * ARROW {
        return need - (hi - lo - 2.0 * ARROW);
    }
    let mut along = Vec::new();
    let mut next = (lo + hi - need) / 2.0;
    for &edge in stack {
        let Route { label, lx, ly, .. } = &routes[edge];
        let size = if vertical { label.height } else { label.width + 8.0 };
        let at = next + size / 2.0;
        let (x, y) = if vertical { (*lx, at) } else { (at, *ly) };
        if covers(x, y, label.width + 8.0, label.height, s, t, obstacles) {
            return 0.0;
        }
        along.push(at);
        next += size + LABEL_GAP;
    }
    for (&edge, at) in stack.iter().zip(along) {
        if vertical {
            routes[edge].ly = at;
        } else {
            routes[edge].lx = at;
        }
    }
    0.0
}

/// The blocks and the grid they are laid out on, which the gaps for edge labels widen.
struct Grid<'m> {
    views: Vec<View<'m>>,
    cells: Vec<Cell>,
    parents: Vec<Option<usize>>,
}

impl Grid<'_> {
    /// Widens the gap in the blocks' nearest shared container. `base` keeps the gaps as they were
    /// when the labels were measured, by container and by whether they are row gaps.
    fn widen(
        &mut self,
        s: usize,
        t: usize,
        vertical: bool,
        short: f64,
        base: &mut HashMap<(usize, bool), Vec<f64>>,
    ) -> bool {
        let mut chain = HashSet::new();
        let mut v = Some(s);
        while let Some(at) = v {
            chain.insert(at);
            v = self.parents[at];
        }
        let mut tb = None;
        let mut container = Some(t);
        while let Some(at) = container.filter(|at| !chain.contains(at)) {
            tb = Some(at);
            container = self.parents[at];
        }
        let (Some(container), Some(tb)) = (container, tb) else {
            return false;
        };
        if container == s {
            return false;
        }
        let mut sb = s;
        while self.parents[sb] != Some(container) {
            sb = self.parents[sb].expect("a block under the container has a parent");
        }
        let (sb, tb) = (&self.cells[sb], &self.cells[tb]);
        let at = if vertical {
            if sb.row == tb.row {
                return false;
            }
            js::max(sb.row, tb.row) - 1.0
        } else {
            let (l, r) = if sb.col < tb.col { (sb, tb) } else { (tb, sb) };
            if r.col < l.col + l.span {
                return false;
            }
            r.col - 1.0
        };
        let cell = &mut self.cells[container];
        let gaps = if vertical { &mut cell.row_gaps } else { &mut cell.col_gaps };
        let base = base.entry((container, vertical)).or_insert_with(|| gaps.clone());
        let at = at as usize;
        if gaps.len() <= at {
            gaps.resize(at + 1, 0.0);
        }
        gaps[at] = js::max(gaps[at], base.get(at).copied().unwrap_or(0.0) + short);
        true
    }
}

/// The key of the two blocks an edge joins, whichever way it runs.
fn pair_key(s: usize, t: usize) -> (usize, usize) {
    (s.min(t), s.max(t))
}

impl From<Head> for Marker {
    fn from(head: Head) -> Marker {
        match head {
            Head::Point => Marker::Point,
            Head::Circle => Marker::Circle,
            Head::Cross => Marker::Cross,
            Head::Unnamed | Head::Open => Marker::None,
        }
    }
}

/// An edge as it is drawn.
struct EdgeView<'m> {
    edge: &'m BlockEdge,
    part: PartId,
    path: EdgePath,
    over: Over,
    markers: [Marker; 2],
}

/// Pele's `draw`. `tight` sets blocks closer together and wraps their labels sooner.
fn draw<'m>(model: &'m BlockModel, cx: &DrawContext<'_>, tight: bool) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let edge_size = js::round(size * 0.875);
    let measure = cx.measure;
    let pad = cx.options.padding_or_default();
    let configured = cx.config.get("block").and_then(JsValue::as_record).and_then(|section| section.get("padding"));
    let spacing = match configured {
        Some(&JsValue::Number(padding)) if padding >= 0.0 => js::min(padding, 200.0),
        _ => GAP,
    };
    let gap = js::min(spacing, if tight { 6.0 } else { f64::INFINITY });
    let wrap = if tight { 80.0 } else { WRAP };
    let room = if tight { fit::title_room(cx.options) } else { 4000.0 };
    let default_class = [JsText::from("default")];
    let no_styles: [JsText; 0] = [];

    let style_of = |block: &BlockNode| -> ResolvedStyle {
        // A style list is split on commas only, so one entry can still hold several declarations.
        let mut declarations: Vec<JsText> = Vec::new();
        let mut add = |styles: &[JsText]| {
            for style in styles {
                declarations.extend(style.split(|&c| c == 59).map(JsText::from));
            }
        };
        let names = block.classes.as_deref().filter(|classes| !classes.is_empty()).unwrap_or(&default_class[..]);
        for name in names {
            if let Some(def) = model.classes.get(name) {
                add(&def.styles);
            }
        }
        if let Some(styles) = &block.styles {
            add(styles);
        }
        resolve_style(&declarations)
    };

    let view = |block: &'m BlockNode, index: usize| -> (View<'m>, Cell) {
        let kind = match block.kind {
            Kind::Space => ViewKind::Space,
            Kind::Composite => ViewKind::Composite,
            Kind::BlockArrow => ViewKind::Arrow,
            other => ViewKind::Leaf(shape_of(other)),
        };
        let style = if kind == ViewKind::Space { resolve_style(&no_styles) } else { style_of(block) };
        let text_style = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
        let font_size = style.font_size.unwrap_or(size);
        let text = block.label.as_deref().unwrap_or_default();
        let (mut nw, mut nh, mut head, mut mask, mut dx, mut dy) = (0.0, 0.0, 0.0, 0, 0.0, 0.0);
        let label = match kind {
            ViewKind::Space => Label::default(),
            ViewKind::Composite => {
                let label = layout_label(text, false, measure, font_size, room, text_style);
                head = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
                nw = label.width + 2.0 * gap;
                nh = head + 2.0 * gap;
                label
            }
            ViewKind::Arrow => {
                let label = layout_label(text, false, measure, font_size, wrap, text_style);
                let blank = label.lines.iter().flat_map(|line| &line.runs).all(|run| run.text.trim_js().is_empty());
                mask = arrow_mask(block.directions.as_deref());
                let s = if blank { arrow_size(mask, 0.0, 0.0) } else { arrow_size(mask, label.width, label.height) };
                (nw, nh, dx, dy) = (s.w, s.h, s.dx, s.dy);
                label
            }
            ViewKind::Leaf(shape) => {
                let label = layout_label(text, false, measure, font_size, wrap, text_style);
                let s = shape_size(shape, label.width, label.height);
                nw = s.w;
                // A cylinder's caps deepen as it widens, so it is sized for the deepest ones.
                nh = if shape == Shape::Cyl { js::max(s.h, label.height + 40.0) } else { s.h };
                dy = s.dy;
                label
            }
        };
        let span = if kind == ViewKind::Space { 1.0 } else { block.width_in_columns.unwrap_or(1.0) };
        let mut c = cell(nw, nh, span);
        c.columns = if kind == ViewKind::Composite { block.columns.unwrap_or(-1.0) } else { -1.0 };
        c.space = kind == ViewKind::Space;
        c.head = head;
        let v = View { block, kind, index, label, style, mask, dx, dy, x: 0.0, y: 0.0, bw: 0.0, bh: 0.0 };
        (v, c)
    };

    let (root, mut root_cell) = view(model.root(), 0);
    root_cell.nw = 0.0;
    root_cell.nh = 0.0;
    let mut grid = Grid { views: vec![root], cells: vec![root_cell], parents: vec![None] };
    let mut by_id: HashMap<&[u16], usize> = HashMap::new();
    let mut i = 0;
    while i < grid.views.len() {
        if grid.views[i].kind == ViewKind::Composite {
            for &child in &grid.views[i].block.children {
                let block = &model.blocks[child];
                let at = grid.views.len();
                let (v, c) = view(block, at);
                if v.kind != ViewKind::Space {
                    by_id.insert(&block.id, at);
                }
                grid.views.push(v);
                grid.cells.push(c);
                grid.cells[i].children.push(at);
                grid.parents.push(Some(i));
            }
        }
        i += 1;
    }

    // Layout keeps the widest a container has been, so each run starts from the widths it needs.
    let widths: Vec<f64> = grid.cells.iter().map(|c| c.nw).collect();
    let place = |grid: &mut Grid<'_>| -> Result<Placement, RenderError> {
        for (c, &width) in grid.cells.iter_mut().zip(&widths) {
            c.nw = width;
        }
        let content = layout(&mut grid.cells, gap)?;
        let mut obstacles = Vec::new();
        let mut titles = Vec::new();
        for (v, c) in grid.views.iter_mut().zip(&grid.cells) {
            (v.x, v.y) = (c.x, c.y);
            match v.kind {
                ViewKind::Leaf(shape) if fixed(shape) => (v.bw, v.bh) = (c.nw, c.nh),
                ViewKind::Arrow => (v.bw, v.bh) = (if c.span > 1.0 { c.w } else { c.nw }, c.nh),
                ViewKind::Leaf(shape) if wide(shape) => (v.bw, v.bh) = (c.w, c.nh),
                _ => (v.bw, v.bh) = (c.w, c.h),
            }
            if v.kind == ViewKind::Leaf(Shape::Cyl) {
                v.dy = js::min(12.0, js::max(6.0, v.bw / 14.0)) / 2.0;
            }
            if matches!(v.kind, ViewKind::Leaf(_) | ViewKind::Arrow) {
                obstacles.push(Obstacle::of(v));
            } else if c.head > 0.0 {
                let y = v.y - c.h / 2.0 + (c.head + gap) / 2.0 + 2.0;
                titles.push(Obstacle { view: None, x: v.x, y, bw: v.label.width, bh: v.label.height });
            }
        }
        Ok(Placement { content, obstacles, titles })
    };
    let Placement { mut content, mut obstacles, mut titles } = place(&mut grid)?;

    // Edges between the same two blocks are spread apart instead of drawn on top of each other.
    let mut pairs: HashMap<(usize, usize), usize> = HashMap::new();
    let mut labelled: HashSet<(usize, usize)> = HashSet::new();
    let mut ends: Vec<(usize, usize)> = Vec::new();
    let mut drawn: Vec<&BlockEdge> = Vec::new();
    for edge in &model.edges {
        let (Some(&s), Some(&t)) = (by_id.get(&*edge.start), by_id.get(&*edge.end)) else {
            continue;
        };
        *pairs.entry(pair_key(s, t)).or_insert(0) += 1;
        if !edge.label.is_empty() {
            labelled.insert(pair_key(s, t));
        }
        ends.push((s, t));
        drawn.push(edge);
    }
    let checked = drawn.len().saturating_mul(obstacles.len()) <= OBSTACLE_BUDGET;
    let mut routes: Vec<Route> = Vec::new();
    let mut stacks: IndexMap<(usize, usize), Vec<usize>> = IndexMap::new();
    let mut legs: Vec<[f64; 4]> = vec![[0.0; 4]; drawn.len()];
    let route = |grid: &Grid<'_>,
                 obstacles: &[Obstacle],
                 routes: &mut Vec<Route>,
                 stacks: &mut IndexMap<(usize, usize), Vec<usize>>,
                 legs: &mut Vec<[f64; 4]>| {
        let check = checked.then_some(obstacles);
        let mut seen: HashMap<(usize, usize), usize> = HashMap::new();
        routes.clear();
        stacks.clear();
        for (i, &(s, t)) in ends.iter().enumerate() {
            let key = pair_key(s, t);
            let total = pairs.get(&key).copied().unwrap_or(1) as f64;
            let k = seen.get(&key).copied().unwrap_or(0) as f64;
            seen.insert(key, k as usize + 1);
            let label = layout_label(&drawn[i].label, false, measure, edge_size, WRAP, Emphasis::default());
            let (sv, tv) = (&grid.views[s], &grid.views[t]);
            let (points, lx, ly);
            if s == t {
                let [px, py, r, ..] = loop_arc(sv, k + 1.0);
                points = Vec::new();
                lx = px + r + 6.0 + label.width / 2.0;
                ly = py - r / 2.0;
            } else {
                let step = if labelled.contains(&key) { js::round(edge_size * 1.5) + 2.0 } else { 14.0 };
                let p = route_edge(sv, tv, (k - (total - 1.0) / 2.0) * step, check);
                let n = p.len();
                // The label goes on the longer leg.
                let first_leg = (p[2] - p[0]).abs() + (p[3] - p[1]).abs();
                let last_leg = (p[n - 2] - p[n - 4]).abs() + (p[n - 1] - p[n - 3]).abs();
                let at = if n == 6 && last_leg > first_leg { 2 } else { 0 };
                let spot = if label.width > 0.0 {
                    label_spot(&p, at, [label.width + 8.0, label.height], sv, tv, check)
                } else {
                    0.5
                };
                lx = p[at] + (p[at + 2] - p[at]) * spot;
                ly = p[at + 1] + (p[at + 3] - p[at + 1]) * spot;
                if label.width > 0.0 && total > 1.0 {
                    legs[i] = [p[at], p[at + 1], p[at + 2], p[at + 3]];
                    stacks.entry(key).or_default().push(i);
                }
                points = p;
            }
            let loop_k = if s == t { k + 1.0 } else { 0.0 };
            routes.push(Route { points, loop_k, label, lx, ly });
        }
    };

    let settle = |grid: &mut Grid<'_>,
                  obstacles: &[Obstacle],
                  titles: &[Obstacle],
                  routes: &mut Vec<Route>,
                  stacks: &mut IndexMap<(usize, usize), Vec<usize>>,
                  legs: &mut Vec<[f64; 4]>,
                  grow: bool| {
        route(grid, obstacles, routes, stacks, legs);
        let blocked: Option<Vec<Obstacle>> = checked.then(|| obstacles.iter().chain(titles).copied().collect());
        let mut base = HashMap::new();
        let mut grown = false;
        for stack in stacks.values() {
            if stack.len() < 2 {
                continue;
            }
            let (s, t) = ends[stack[0]];
            let short = stack_labels(stack, routes, legs, &grid.views[s], &grid.views[t], blocked.as_deref());
            let vertical = legs[stack[0]][0] == legs[stack[0]][2];
            // Keep columns narrow to fit the host in tight layouts.
            if grow && short > 0.0 && (vertical || !tight) && grid.widen(s, t, vertical, short.ceil(), &mut base) {
                grown = true;
            }
        }
        grown
    };
    let overlap = |routes: &[Route]| {
        let mut area = 0.0;
        for (i, p) in routes.iter().enumerate() {
            for q in &routes[i + 1..] {
                let pw = (p.label.width + 8.0) / 2.0;
                let qw = (q.label.width + 8.0) / 2.0;
                let w = js::min(p.lx + pw, q.lx + qw) - js::max(p.lx - pw, q.lx - qw);
                let h = js::min(p.ly + p.label.height / 2.0, q.ly + q.label.height / 2.0)
                    - js::max(p.ly - p.label.height / 2.0, q.ly - q.label.height / 2.0);
                if p.label.width > 0.0 && q.label.width > 0.0 && w > 0.0 && h > 0.0 {
                    area += w * h;
                }
            }
        }
        area
    };
    // The drawing's extent, as left, top, right and bottom.
    let bound = |grid: &Grid<'_>, content: (f64, f64), routes: &[Route]| {
        let mut extent = [0.0, 0.0, content.0, content.1];
        let mut grow = |x: f64, y: f64, w: f64, h: f64| {
            if x - w / 2.0 < extent[0] {
                extent[0] = x - w / 2.0;
            }
            if x + w / 2.0 > extent[2] {
                extent[2] = x + w / 2.0;
            }
            if y - h / 2.0 < extent[1] {
                extent[1] = y - h / 2.0;
            }
            if y + h / 2.0 > extent[3] {
                extent[3] = y + h / 2.0;
            }
        };
        for (route, &(s, _)) in routes.iter().zip(&ends) {
            if route.loop_k > 0.0 {
                let [px, py, r, ..] = loop_arc(&grid.views[s], route.loop_k);
                grow(px, py, 2.0 * r + 2.0, 2.0 * r + 2.0);
            }
            if route.label.width > 0.0 {
                grow(route.lx, route.ly, route.label.width + 8.0, route.label.height);
            }
        }
        extent
    };

    let budget = drawn.len().saturating_mul(drawn.len()) <= OBSTACLE_BUDGET;
    if settle(&mut grid, &obstacles, &titles, &mut routes, &mut stacks, &mut legs, budget) {
        let before = overlap(&routes);
        let narrow = bound(&grid, content, &routes);
        let narrow = narrow[2] - narrow[0];
        // A container stretched by its row gives some of a new gap back, so the gap may take a
        // few rounds.
        let mut grown = true;
        let mut round = 0;
        while round < 3 && grown {
            Placement { content, obstacles, titles } = place(&mut grid)?;
            grown = settle(&mut grid, &obstacles, &titles, &mut routes, &mut stacks, &mut legs, round < 2);
            round += 1;
        }
        // The drawing only grows if that leaves less of the labels covering each other, and
        // within maxWidth.
        let wide = bound(&grid, content, &routes);
        let wide = wide[2] - wide[0];
        let room = cx.options.max_width.filter(|&width| width > 0.0).unwrap_or(f64::INFINITY);
        if overlap(&routes) > before - 1.0 || (wide > narrow && (wide + 2.0 * pad).ceil() > room) {
            for c in &mut grid.cells {
                c.row_gaps = Vec::new();
                c.col_gaps = Vec::new();
            }
            Placement { content, obstacles, titles } = place(&mut grid)?;
            settle(&mut grid, &obstacles, &titles, &mut routes, &mut stacks, &mut legs, false);
        }
    }

    let [min_x, min_y, max_x, max_y] = bound(&grid, content, &routes);
    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measure, size, room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = js::max(max_x - min_x, title.width);
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;
    for v in &mut grid.views {
        v.x += ox;
        v.y += oy;
    }
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();

    let mut out = SceneBuilder::new(cx, Size { width, height }, Accessible::default());
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });
    let parts: Vec<Option<PartId>> = grid
        .views
        .iter()
        .map(|v| match v.kind {
            _ if v.index == 0 => None,
            ViewKind::Space => None,
            ViewKind::Composite => Some(out.part(Entity::group(&v.block.id))),
            ViewKind::Leaf(_) | ViewKind::Arrow => Some(out.part(Entity::node(&v.block.id))),
        })
        .collect();

    let mut fills: Vec<Fill<'_>> = Vec::new();
    for (v, c) in grid.views.iter().zip(&grid.cells).skip(1) {
        let shape = v.style.shape.as_ref();
        if v.kind == ViewKind::Composite && paints(shape) {
            let top = v.y - c.h / 2.0;
            fills.push(Fill { bounds: [v.x - c.w / 2.0, top, v.x + c.w / 2.0, top + c.h], shape });
        }
    }

    let mut ranks: HashMap<(&[u16], &[u16]), u32> = HashMap::new();
    let mut edges: Vec<EdgeView<'_>> = Vec::new();
    for (i, route) in routes.iter_mut().enumerate() {
        let edge = drawn[i];
        let (s, t) = ends[i];
        let rank = ranks.entry((&edge.start[..], &edge.end[..])).or_insert(0);
        let name = EntityName::Between { from: edge.start.to_vec(), to: edge.end.to_vec(), kind: String::new() };
        let part = out.part(Entity::edge(&edge.id, name, *rank));
        *rank += 1;
        if let (Some(from), Some(to)) = (parts[s], parts[t]) {
            out.relation(part, from, to);
        }
        let markers: [Marker; 2] = [edge.arrow_type_start.into(), edge.arrow_type_end.into()];
        let path = if route.loop_k > 0.0 {
            loop_path(&grid.views[s], route.loop_k, markers[0].trim(), markers[1].trim())
        } else {
            for k in (0..route.points.len()).step_by(2) {
                route.points[k] += ox;
                route.points[k + 1] += oy;
            }
            trace(&route.points, markers[0].trim(), markers[1].trim())
        };
        let over = over_fills(&path, &fills, None);
        edges.push(EdgeView { edge, part, path, over, markers });
    }

    let label_look = |v: &View<'_>| {
        LabelLook::class("pele-cluster-label")
            .fill(Some(Paint::Role(ink_on(v.style.shape.as_ref(), v.style.text.as_ref()).unwrap_or(Role::Muted))))
            .style(v.style.text.as_ref())
    };
    out.layer(LayerSpec::class("pele-clusters"), |out| {
        for (v, c) in grid.views.iter().zip(&grid.cells).skip(1) {
            let (ViewKind::Composite, Some(part)) = (v.kind, parts[v.index]) else {
                continue;
            };
            let classes = author_classes(v.block);
            out.layer(LayerSpec::classes("pele-cluster", &classes).part(part, Facet::Shape).data_id(), |out| {
                let shape = v.style.shape.as_ref();
                let top = v.y - c.h / 2.0;
                let frame = out
                    .element(part, Facet::Shape)
                    .rect(pair(v.x - c.w / 2.0, top), pair(c.w, c.h), None)
                    .fill(Paint::Role(Role::SurfaceAlt));
                let frame =
                    if shape.is_some_and(Style::sets_fill) { frame } else { frame.fill_opacity(Number::raw(0.5)) };
                frame.stroke(Paint::Role(Role::Border)).style(Some(Style::rounded(shape))).finish();
                let at = Point { x: v.x, y: top + (c.head + gap) / 2.0 + 2.0 };
                out.label(part, Facet::Title, &v.label, at, label_look(v));
            });
        }
    });

    out.layer(casings_layer(), |out| {
        for e in edges.iter().filter(|e| e.over == Over::Cased) {
            draw_casing(out, e.part, &e.path, e.markers, None, e.edge.thick);
        }
    });

    let edges_layer =
        LayerSpec::class("pele-edges").fill(Paint::None).stroke(Paint::Role(Role::Line)).cap(LineCap::Round);
    out.layer(edges_layer, |out| {
        for e in &edges {
            let mut group = LayerSpec::class("pele-edge").part(e.part, Facet::Shape).data_id();
            let colour = match e.over {
                Over::Ink(ink) => {
                    group = group.stroke(Paint::Role(ink));
                    Paint::Role(ink)
                }
                Over::Page | Over::Cased => Paint::Role(Role::Line),
            };
            out.layer(group, |out| {
                let mut path = out.element(e.part, Facet::Shape).path(e.path.d.clone());
                if e.edge.thick {
                    path = path.stroke_width(Number::raw(2.5));
                }
                if e.edge.dotted {
                    path = path.dash(&[Number::raw(3.0), Number::raw(4.0)]);
                }
                path.finish();
                draw_marker(out, e.part, Facet::StartMarker, e.markers[0], e.path.start, MarkerUse::Edge(colour));
                draw_marker(out, e.part, Facet::EndMarker, e.markers[1], e.path.end, MarkerUse::Edge(colour));
            });
        }
    });

    out.layer(LayerSpec::class("pele-nodes").keep_empty(), |out| {
        for (v, c) in grid.views.iter().zip(&grid.cells).skip(1) {
            let Some(part) = parts[v.index].filter(|_| matches!(v.kind, ViewKind::Leaf(_) | ViewKind::Arrow)) else {
                continue;
            };
            draw_node(out, part, v, c, measure);
        }
    });

    out.layer(LayerSpec::class("pele-edge-labels").font_size(Number::raw(edge_size)), |out| {
        for (e, route) in edges.iter().zip(&routes) {
            if route.label.width > 0.0 {
                let at = Point { x: route.lx + ox, y: route.ly + oy };
                draw_edge_label(out, e.part, &route.label, at, None);
            }
        }
    });
    out.finish()
}

/// `classNames((block.classes ?? []).join(' '))`
fn author_classes(block: &BlockNode) -> AuthorClasses {
    let mut joined = JsText::default();
    for (index, class) in block.classes.iter().flatten().enumerate() {
        if index > 0 {
            joined.push(32);
        }
        joined.push_units(class);
    }
    AuthorClasses::new(&joined)
}

/// A block or a block arrow, with its label.
fn draw_node(out: &mut SceneBuilder, part: PartId, v: &View<'_>, c: &Cell, measure: &dyn crate::TextMeasurer) {
    let arrow = v.kind == ViewKind::Arrow;
    let shape = match v.kind {
        ViewKind::Leaf(shape) => Some(shape),
        _ => None,
    };
    // Reuse a wider cell without changing the node size or the routes computed from it.
    let rewrap = c.span > 1.0
        && v.label.width > 0.0
        && v.label.lines.len() > 1
        && v.bw > c.nw
        && (arrow || !shape.is_some_and(fixed));
    let label = if rewrap {
        let text = v.block.label.as_deref().unwrap_or_default();
        let room = v.label.width + (v.bw - c.nw);
        layout_label(text, false, measure, v.label.size, room, v.emphasis())
    } else {
        v.label.clone()
    };
    let name = shape.map_or("block-arrow", Shape::name);
    let own = format!("pele-node pele-shape-{name}");
    let group = LayerSpec::classes(&own, &author_classes(v.block))
        .part(part, Facet::Shape)
        .data_id()
        .translate(Point { x: v.x, y: v.y })
        .keep_empty();
    out.layer(group, |out| {
        let a = ShapeLook::new(Role::Surface, Role::Border, v.style.shape.as_ref());
        match shape {
            None => {
                let points = arrow_points(v.mask, v.bw, v.bh).chunks(2).map(|p| pair(p[0], p[1])).collect();
                out.element(part, Facet::Shape).polygon(points).join(LineJoin::Round).look(a.plain()).finish();
            }
            Some(shape) => {
                let line = LineLook::new(Role::Border, v.style.line.as_ref());
                draw_shape(out, part, shape, v.bw, v.bh, &a, &line);
            }
        }
        let ink = if arrow || shape.is_some_and(shape_filled) {
            ink_on(v.style.shape.as_ref(), v.style.text.as_ref())
        } else {
            None
        };
        let look = LabelLook::class("pele-label").fill(ink.map(Paint::Role)).style(v.style.text.as_ref());
        out.label(part, Facet::Label, &label, Point { x: v.dx, y: v.dy }, look);
    });
}

pub(crate) struct Block;

impl Diagram for Block {
    type Model = BlockModel;

    fn parse(cx: &ParseContext<'_>) -> Result<BlockModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderBlock`: with room to spare, then tightly.
    fn scene(model: &BlockModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        fit::tighten(cx.options, |tight| draw(model, cx, tight))
    }
}
