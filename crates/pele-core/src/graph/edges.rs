//! How an edge is drawn, ported from Pele's `src/svg/edges.ts`: the path along its route, the
//! markers at its ends, its label on a patch of the background, and what keeps it legible over
//! a filled group. An edge inside one fill takes that fill's ink. An edge that crosses a
//! fill's border gets a casing: a wider stroke in the page's colour underneath it.

use crate::js;
use crate::layout::num::hypot;
use crate::layout::{Axis, RoutePoint, ease};
use crate::measure::Label;
use crate::scene::{
    Facet, LabelLook, LayerSpec, LineCap, LineJoin, Number, Paint, PartId, PathProgram, PathStep, Point, Role,
    SceneBuilder, Style, ink_on, px,
};
use std::collections::HashMap;

const ARROW: f64 = 8.0;
// The most a corner is rounded by in a stepped route.
const CORNER: f64 = 12.0;
// How far the handles of a wide turn reach along its two legs. A little more than a circle's,
// which makes the turn fuller.
const HANDLE: f64 = 0.62;
// How far an edge runs straight before a marker at its end, where it has the room.
const LEAD: f64 = 6.0;
// How much wider than its edge a casing is.
const CASE: f64 = 4.0;

/// What is drawn at an end of an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Marker {
    None,
    /// An arrowhead.
    Point,
    Circle,
    Cross,
}

impl Marker {
    /// Pele's `markerTrim`: how much of the edge's end the marker takes.
    pub(crate) fn trim(self) -> f64 {
        match self {
            Marker::Point => ARROW - 1.0,
            Marker::Circle => ARROW,
            Marker::Cross => 4.0,
            Marker::None => 0.0,
        }
    }
}

/// How a route is drawn between its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Curve {
    /// Curves that leave and arrive along the flow.
    Smooth,
    Linear,
    Step,
    StepBefore,
    StepAfter,
}

impl Curve {
    /// The curve a source names. Any other name is drawn smooth.
    pub(crate) fn named(name: Option<&[u16]>) -> Curve {
        let is = |text: &str| name.is_some_and(|name| name.iter().copied().eq(text.encode_utf16()));
        if is("linear") {
            Curve::Linear
        } else if is("step") {
            Curve::Step
        } else if is("stepBefore") {
            Curve::StepBefore
        } else if is("stepAfter") {
            Curve::StepAfter
        } else {
            Curve::Smooth
        }
    }

    fn stepped(self) -> bool {
        matches!(self, Curve::Step | Curve::StepBefore | Curve::StepAfter)
    }
}

/// An end of a drawn edge: where its marker's tip is, and the way the marker points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tip {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) dx: f64,
    pub(crate) dy: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EdgePath {
    pub(crate) d: PathProgram,
    pub(crate) start: Tip,
    pub(crate) end: Tip,
}

/// How far from each side of each node the edges that meet it start to turn: as far as the
/// largest marker there needs, for every edge on the side, so that none turns sooner than its
/// neighbour and crosses it.
#[derive(Default)]
pub(crate) struct SideReach {
    reach: HashMap<(usize, i32), f64>,
}

impl SideReach {
    /// Takes a node, the side of it, and the marker of an end there.
    pub(crate) fn add(&mut self, node: usize, side: i32, marker: Marker) {
        let trim = marker.trim();
        if side >= 0 && trim > 0.0 && trim + LEAD > self.of(node, side) {
            self.reach.insert((node, side), trim + LEAD);
        }
    }

    pub(crate) fn of(&self, node: usize, side: i32) -> f64 {
        self.reach.get(&(node, side)).copied().unwrap_or(0.0)
    }
}

/// `Math.sign(x) || 1`
fn sign_or_one(x: f64) -> f64 {
    if x < 0.0 { -1.0 } else { 1.0 }
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

fn near(ax: f64, ay: f64, bx: f64, by: f64) -> bool {
    (ax - bx).abs() < 0.01 && (ay - by).abs() < 0.01
}

/// The points of a route while its path is built.
struct Points {
    xs: Vec<f64>,
    ys: Vec<f64>,
    axes: Vec<Axis>,
    linear: bool,
}

impl Points {
    fn push(&mut self, x: f64, y: f64, axis: Axis) {
        self.xs.push(x);
        self.ys.push(y);
        self.axes.push(axis);
    }

    fn insert(&mut self, at: usize, x: f64, y: f64, axis: Axis) {
        self.xs.insert(at, x);
        self.ys.insert(at, y);
        self.axes.insert(at, axis);
    }

    fn last(&self) -> usize {
        self.xs.len() - 1
    }

    fn aligned(&self, i: usize) -> bool {
        (self.xs[i] - self.xs[i - 1]).abs() < 0.01 || (self.ys[i] - self.ys[i - 1]).abs() < 0.01
    }

    fn straight(&self, i: usize) -> bool {
        self.linear || self.aligned(i)
    }

    /// The way the route heads from point i to point j: along the flow where an S joins them.
    fn unit(&self, i: usize, j: usize, axis: Axis) -> [f64; 2] {
        if !self.straight(i.max(j)) {
            return match axis {
                Axis::Vertical => [0.0, sign_or_one(self.ys[j] - self.ys[i])],
                Axis::Horizontal => [sign_or_one(self.xs[j] - self.xs[i]), 0.0],
            };
        }
        let dx = self.xs[j] - self.xs[i];
        let dy = self.ys[j] - self.ys[i];
        let len = hypot(dx, dy);
        let len = if len == 0.0 || len.is_nan() { 1.0 } else { len };
        [dx / len, dy / len]
    }

    fn length(&self, i: usize) -> f64 {
        (self.xs[i] - self.xs[i - 1]).abs() + (self.ys[i] - self.ys[i - 1]).abs()
    }

    /// Whether the route turns a corner at point i, from running one way to running the other.
    fn turns(&self, i: usize) -> bool {
        !self.linear
            && i > 0
            && i < self.last()
            && self.aligned(i)
            && self.aligned(i + 1)
            && self.length(i) >= 0.01
            && self.length(i + 1) >= 0.01
            && ((self.xs[i] - self.xs[i - 1]).abs() < 0.01) != ((self.xs[i + 1] - self.xs[i]).abs() < 0.01)
    }

    fn along(&self, i: usize, ux: f64, uy: f64) -> f64 {
        (self.xs[i] - self.xs[i - 1]).abs() * ux.abs() + (self.ys[i] - self.ys[i - 1]).abs() * uy.abs()
    }
}

/// Of the way from the end of a run to where it turns, the part that makes up for a larger
/// marker beside this one is kept where it can be, and the rest is at most a quarter of what
/// is left, so that ends on one side of a node turn equally far from it.
fn lead(reach: f64, trim: f64, run: f64) -> f64 {
    let room = js::min(js::max(0.0, reach - LEAD - trim), run / 4.0);
    room + js::min(reach - trim - room, (run - room) / 4.0)
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

/// Pele's `routePath`: the path for a route, with both ends trimmed to leave room for markers.
/// Between two points that are not in line, a route is one S that leaves and arrives along the
/// flow, or a step in a stepped route. Where the route itself turns a corner, a stepped route
/// rounds it a little, and any other but a linear one turns as widely as it can: the turn takes
/// the whole of a leg that ends at a node and half of a leg it shares with the next turn.
/// `reach` says how far from its nodes the edge starts to turn at each end, where the other
/// edges on the same side of a node need it to.
pub(crate) fn route_path(route: &[RoutePoint], curve: Curve, trim: [f64; 2], reach: [f64; 2]) -> EdgePath {
    let stepped = curve.stepped();
    let mut p = Points { xs: Vec::new(), ys: Vec::new(), axes: Vec::new(), linear: curve == Curve::Linear };
    for point in route {
        let RoutePoint { x, y, axis } = *point;
        // A stepped route turns at right angles where a smooth one would curve.
        if stepped && !p.xs.is_empty() {
            let (px, py) = (p.xs[p.last()], p.ys[p.last()]);
            if (x - px).abs() >= 0.01 && (y - py).abs() >= 0.01 {
                let (mx, my) = match curve {
                    Curve::StepBefore => (px, py),
                    Curve::StepAfter => (x, y),
                    _ => ((px + x) / 2.0, (py + y) / 2.0),
                };
                let turns = if axis == Axis::Vertical { [px, my, x, my] } else { [mx, py, mx, y] };
                for k in [0, 2] {
                    if near(turns[k], turns[k + 1], px, py) || near(turns[k], turns[k + 1], x, y) {
                        continue;
                    }
                    p.push(turns[k], turns[k + 1], axis);
                }
            }
        }
        p.push(x, y, axis);
    }
    let [start_trim, end_trim] = trim;
    let [start_reach, end_reach] = reach;
    let last = p.last();
    let [sdx, sdy] = p.unit(0, 1, p.axes[1]);
    let [edx, edy] = p.unit(last - 1, last, p.axes[last]);
    let (sx, sy) = (p.xs[0], p.ys[0]);
    let (ex, ey) = (p.xs[last], p.ys[last]);
    p.xs[0] += sdx * start_trim;
    p.ys[0] += sdy * start_trim;
    p.xs[last] -= edx * end_trim;
    p.ys[last] -= edy * end_trim;
    // An edge that curves into a marker runs straight for a little way first, where it has the
    // room, so that the marker does not sit on the bend.
    let arrives = if end_reach > end_trim && (!p.straight(last) || p.turns(last - 1)) {
        lead(end_reach, end_trim, p.along(last, edx, edy))
    } else {
        0.0
    };
    let leaves = if start_reach > start_trim && (!p.straight(1) || p.turns(1)) {
        lead(start_reach, start_trim, p.along(1, sdx, sdy))
    } else {
        0.0
    };
    if arrives >= 1.0 {
        p.insert(last, p.xs[last] - edx * arrives, p.ys[last] - edy * arrives, p.axes[last]);
    }
    if leaves >= 1.0 {
        p.insert(1, p.xs[0] + sdx * leaves, p.ys[0] + sdy * leaves, p.axes[1]);
    }
    let last = p.last();

    let mut steps = vec![PathStep::Move(pair(p.xs[0], p.ys[0]))];
    let (mut at_x, mut at_y) = (p.xs[0], p.ys[0]);
    let line_to = |steps: &mut Vec<PathStep>, at_x: &mut f64, at_y: &mut f64, x: f64, y: f64| {
        if near(x, y, *at_x, *at_y) {
            return;
        }
        steps.push(PathStep::Line(pair(x, y)));
        (*at_x, *at_y) = (x, y);
    };
    for i in 1..=last {
        let (x, y) = (p.xs[i], p.ys[i]);
        if !p.straight(i) {
            let down = p.axes[i] == Axis::Vertical;
            let along = if down { y - at_y } else { x - at_x };
            let k = along.abs() * ease(if down { x - at_x } else { y - at_y }, along);
            let ux = if down { 0.0 } else { sign(along) * k };
            let uy = if down { sign(along) * k } else { 0.0 };
            steps.push(PathStep::Cubic([pair(at_x + ux, at_y + uy), pair(x - ux, y - uy), pair(x, y)]));
            (at_x, at_y) = (x, y);
            continue;
        }
        if !p.turns(i) {
            // A point part of the way along one straight run adds nothing to the path.
            if i < last
                && p.aligned(i)
                && p.aligned(i + 1)
                && (x - p.xs[i - 1]) * (p.xs[i + 1] - x) + (y - p.ys[i - 1]) * (p.ys[i + 1] - y) > 0.0
            {
                continue;
            }
            line_to(&mut steps, &mut at_x, &mut at_y, x, y);
            continue;
        }
        // How much of the leg before the turn and of the leg after it the turn takes: all of one
        // that ends at a node, short of the straight run into a marker, and half of one it shares.
        let mut before = if p.turns(i - 1) { p.length(i) / 2.0 } else { p.length(i) };
        let mut after = if p.turns(i + 1) { p.length(i + 1) / 2.0 } else { p.length(i + 1) };
        if stepped {
            before = js::min(js::min(CORNER, before), after);
            after = before;
        }
        let [ux, uy] = p.unit(i - 1, i, p.axes[i]);
        let [vx, vy] = p.unit(i, i + 1, p.axes[i + 1]);
        line_to(&mut steps, &mut at_x, &mut at_y, x - ux * before, y - uy * before);
        at_x = x + vx * after;
        at_y = y + vy * after;
        if stepped {
            steps.push(PathStep::Quad([pair(x, y), pair(at_x, at_y)]));
        } else {
            steps.push(PathStep::Cubic([
                pair(x - ux * before * (1.0 - HANDLE), y - uy * before * (1.0 - HANDLE)),
                pair(x + vx * after * (1.0 - HANDLE), y + vy * after * (1.0 - HANDLE)),
                pair(at_x, at_y),
            ]));
        }
    }
    EdgePath {
        d: PathProgram { steps },
        start: Tip { x: sx, y: sy, dx: -sdx, dy: -sdy },
        end: Tip { x: ex, y: ey, dx: edx, dy: edy },
    }
}

/// Where a self-loop's label goes, and the loop itself.
pub(crate) struct Loop {
    pub(crate) path: EdgePath,
    pub(crate) label: Point,
}

/// Pele's `loopPath`: a self-loop beside a node at `centre` whose drawn size is `size`, out of
/// the side that lies across the flow and back in. `k` counts the loops already on the node, so
/// each one reaches a little further. `label` is the room the edge's label takes, and the label
/// goes beyond the loop.
pub(crate) fn loop_path(
    centre: Point,
    size: [f64; 2],
    k: f64,
    sideways: bool,
    base: f64,
    trim: f64,
    label: [f64; 2],
) -> Loop {
    let [w, h] = size;
    let half = (if sideways { w } else { h }) / 2.0;
    let spread = js::min(half - 4.0, 8.0 + k * 6.0);
    let reach = base + k * 8.0;
    let len = hypot(reach, spread);
    let len = if len == 0.0 || len.is_nan() { 1.0 } else { len };
    // Local frame: `out` points away from the node, `along` runs along its side.
    let at = |out: f64, along: f64| {
        if sideways {
            pair(centre.x + along, centre.y + h / 2.0 + out)
        } else {
            pair(centre.x + w / 2.0 + out, centre.y + along)
        }
    };
    let tx = (reach / len) * trim;
    let ty = (spread / len) * trim;
    let [label_w, label_h] = label;
    let label = if sideways {
        Point { x: centre.x, y: centre.y + h / 2.0 + reach + 4.0 + label_h / 2.0 }
    } else {
        Point { x: centre.x + w / 2.0 + reach + 4.0 + label_w / 2.0, y: centre.y }
    };
    let d = PathProgram::default().m(at(0.0, -spread)[0], at(0.0, -spread)[1]).c(
        at(reach, -spread * 2.0),
        at(reach, spread * 2.0),
        at(tx, spread + ty),
    );
    let (start, end) = if sideways {
        (
            Tip { x: centre.x - spread, y: centre.y + h / 2.0, dx: 0.0, dy: -1.0 },
            Tip { x: centre.x + spread, y: centre.y + h / 2.0, dx: -spread / len, dy: -reach / len },
        )
    } else {
        (
            Tip { x: centre.x + w / 2.0, y: centre.y - spread, dx: -1.0, dy: 0.0 },
            Tip { x: centre.x + w / 2.0, y: centre.y + spread, dx: -reach / len, dy: -spread / len },
        )
    };
    Loop { path: EdgePath { d, start, end }, label }
}

/// Where a marker is drawn: on its edge, or under it as part of the edge's casing.
#[derive(Clone, Copy)]
pub(crate) enum MarkerUse {
    /// In the edge's colour, with the class marker styles apply to.
    Edge(Paint),
    /// In the page's colour, with no class, so that marker styles skip it.
    Casing,
}

/// Pele's `marker`: draws a marker whose tip is at `tip`, pointing along its direction.
pub(crate) fn draw_marker(
    out: &mut SceneBuilder,
    part: PartId,
    facet: Facet,
    marker: Marker,
    tip: Tip,
    used: MarkerUse,
) {
    let Tip { x, y, dx, dy } = tip;
    let nx = -dy;
    let ny = dx;
    fn start(out: &mut SceneBuilder, part: PartId, facet: Facet, used: MarkerUse) -> crate::scene::ElementStart<'_> {
        let element = out.element(part, facet);
        match used {
            MarkerUse::Edge(_) => element.class("pele-marker"),
            MarkerUse::Casing => element,
        }
    }
    let element = |out| start(out, part, facet, used);
    let (fill, stroke) = match used {
        MarkerUse::Edge(colour) => (colour, Paint::None),
        MarkerUse::Casing => (Paint::Role(Role::Background), Paint::Role(Role::Background)),
    };
    match marker {
        Marker::None => {}
        Marker::Point => {
            let bx = x - dx * ARROW;
            let by = y - dy * ARROW;
            let half = ARROW * 0.42;
            let head = PathProgram::default()
                .m(Number::num(x), Number::num(y))
                .l(Number::num(bx + nx * half), Number::num(by + ny * half))
                .l(Number::num(bx - nx * half), Number::num(by - ny * half))
                .z();
            element(out).path(head).fill(fill).stroke(stroke).finish();
        }
        Marker::Circle => {
            let centre = [Some(Number::num(x - dx * 4.0)), Some(Number::num(y - dy * 4.0))];
            element(out).circle_at(centre, Number::raw(3.5)).fill(fill).stroke(stroke).finish();
        }
        Marker::Cross => {
            let cx = x - dx * 4.0;
            let cy = y - dy * 4.0;
            let a = 3.5;
            let ux = (dx + nx) * a;
            let uy = (dy + ny) * a;
            let vx = (dx - nx) * a;
            let vy = (dy - ny) * a;
            let cross = PathProgram::default()
                .m(Number::num(cx - ux), Number::num(cy - uy))
                .l(Number::num(cx + ux), Number::num(cy + uy))
                .m(Number::num(cx - vx), Number::num(cy - vy))
                .l(Number::num(cx + vx), Number::num(cy + vy));
            element(out).path(cross).finish();
        }
    }
}

/// Pele's `sample`: the points of an edge's path as it is written, with each curve as eight
/// straight pieces. A pair of NaN separates subpaths.
fn sample(path: &PathProgram) -> Vec<f64> {
    let at = |pair: &[Number; 2]| [pair[0].effective(), pair[1].effective()];
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    for step in &path.steps {
        let (first, second, end) = match step {
            PathStep::Move(to) | PathStep::Line(to) => {
                if matches!(step, PathStep::Move(_)) && !out.is_empty() {
                    out.extend([f64::NAN, f64::NAN]);
                }
                [x, y] = at(to);
                out.extend([x, y]);
                continue;
            }
            PathStep::Quad([handle, end]) => (at(handle), None, at(end)),
            PathStep::Cubic([first, second, end]) => (at(first), Some(at(second)), at(end)),
            other => unreachable!("an edge's path holds no {other:?}"),
        };
        let cubic = second.is_some();
        let [x1, y1] = first;
        let [x2, y2] = second.unwrap_or(end);
        let [x3, y3] = end;
        let mut t = 0.125;
        while t <= 1.0 {
            // De Casteljau in Pele's order of operations, which decides the rounding.
            let mut ax = x + (x1 - x) * t;
            let mut ay = y + (y1 - y) * t;
            let mut bx = x1 + (x2 - x1) * t;
            let mut by = y1 + (y2 - y1) * t;
            if cubic {
                let cx = x2 + (x3 - x2) * t;
                let cy = y2 + (y3 - y2) * t;
                ax += (bx - ax) * t;
                ay += (by - ay) * t;
                bx += (cx - bx) * t;
                by += (cy - by) * t;
            }
            out.extend([ax + (bx - ax) * t, ay + (by - ay) * t]);
            t += 0.125;
        }
        (x, y) = (x3, y3);
    }
    out
}

/// Whether a segment from (x, y) by (dx, dy) overlaps a box, by Liang-Barsky clipping.
#[allow(clippy::too_many_arguments, reason = "Pele's signature, kept so the port reads beside its source")]
fn clips(x: f64, y: f64, dx: f64, dy: f64, l: f64, t: f64, r: f64, b: f64) -> bool {
    let mut lo = 0.0;
    let mut hi = 1.0;
    for (p, q) in [(-dx, x - l), (dx, r - x), (-dy, y - t), (dy, b - y)] {
        if p == 0.0 {
            if q < 0.0 {
                hi = -1.0;
            }
        } else if p < 0.0 {
            lo = js::max(lo, q / p);
        } else {
            hi = js::min(hi, q / p);
        }
    }
    lo < hi
}

fn enters(points: &[f64], l: f64, t: f64, r: f64, b: f64) -> bool {
    let mut k = 0;
    while k + 2 < points.len() {
        let (x, y, x2, y2) = (points[k], points[k + 1], points[k + 2], points[k + 3]);
        k += 2;
        if (x < l && x2 < l) || (x > r && x2 > r) || (y < t && y2 < t) || (y > b && y2 > b) {
            continue;
        }
        if clips(x, y, x2 - x, y2 - y, l, t, r, b) {
            return true;
        }
    }
    false
}

/// A filled area an edge may run over: its bounds as left, top, right and bottom, and the style
/// that fills it.
pub(crate) struct Fill<'a> {
    pub(crate) bounds: [f64; 4],
    pub(crate) shape: Option<&'a Style>,
}

/// Where an edge runs, as far as fills go.
enum Under<'f, 'a> {
    /// Over no fill.
    Page,
    /// Across the border of a fill.
    Border,
    /// Wholly inside a fill: the innermost that holds it.
    Fill(&'f Fill<'a>),
}

/// Pele's `fillUnder`, for the path of an edge and the tips of its two markers.
fn fill_under<'f, 'a>(path: &PathProgram, tips: [f64; 4], fills: &'f [Fill<'a>]) -> Under<'f, 'a> {
    if fills.is_empty() {
        return Under::Page;
    }
    let points = sample(path);
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in points.chunks(2).chain(tips.chunks(2)) {
        let (x, y) = (point[0], point[1]);
        if x < min_x {
            min_x = x;
        }
        if x > max_x {
            max_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }
    let mut holder = Under::Page;
    let mut least = f64::INFINITY;
    for fill in fills {
        let [l, t, r, b] = fill.bounds;
        if max_x < l + 0.5 || min_x > r - 0.5 || max_y < t + 0.5 || min_y > b - 0.5 {
            continue;
        }
        let touched = enters(&points, l + 0.5, t + 0.5, r - 0.5, b - 0.5)
            || tips.chunks(2).any(|tip| tip[0] > l + 0.5 && tip[0] < r - 0.5 && tip[1] > t + 0.5 && tip[1] < b - 0.5);
        if !touched {
            continue;
        }
        if min_x < l || max_x > r || min_y < t || max_y > b {
            return Under::Border;
        }
        let area = (r - l) * (b - t);
        if area < least {
            least = area;
            holder = Under::Fill(fill);
        }
    }
    holder
}

/// What an edge needs to stay legible where it runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Over {
    /// It runs over the page, in the theme's line colour or its own.
    Page,
    /// It crosses a fill's border, or lies on a fill whose ink is unknown or which its own
    /// stroke overrides: it gets a casing.
    Cased,
    /// It lies inside a fill, in that fill's ink.
    Ink(Role),
}

/// Pele's `overFills`: the ink of the innermost fill that holds the whole edge and its marker
/// tips, a casing where it crosses a border, or neither off fills. `line` is the edge's own
/// line style.
pub(crate) fn over_fills(path: &EdgePath, fills: &[Fill<'_>], line: Option<&Style>) -> Over {
    let tips = [path.start.x, path.start.y, path.end.x, path.end.y];
    match fill_under(&path.d, tips, fills) {
        Under::Page => Over::Page,
        Under::Border => Over::Cased,
        Under::Fill(fill) => {
            let ink = if line.is_some_and(Style::sets_stroke) { None } else { ink_on(fill.shape, None) };
            ink.map_or(Over::Cased, Over::Ink)
        }
    }
}

/// Pele's `casing`: a wider stroke in the page's colour under an edge, as faint as the edge,
/// with the edge's markers. An edge whose own style draws no stroke gets none.
pub(crate) fn draw_casing(
    out: &mut SceneBuilder,
    part: PartId,
    path: &EdgePath,
    markers: [Marker; 2],
    line: Option<&Style>,
    thick: bool,
) {
    let declared = |property: &str| line.and_then(|line| line.first(property));
    let stroke = declared("stroke");
    if stroke.as_deref().is_some_and(|stroke| stroke == "none" || stroke == "transparent") {
        return;
    }
    let width = if thick { Some("2.5".to_owned()) } else { declared("stroke-width") };
    let mut wider = None;
    let mut style = Style::terminated();
    match width.as_deref().map(|width| (width, px(width))) {
        Some((_, Some(width))) => {
            if width != 1.0 {
                wider = Some(Number::num(width + CASE));
            }
        }
        Some((width, None)) => {
            style.declare_written("stroke-width", format!("calc({width} + {}px)", js::to_string(CASE)));
        }
        None => {}
    }
    for property in ["opacity", "stroke-opacity"] {
        if let Some(value) = declared(property) {
            style.declare_written(property, value);
        }
    }
    let body = |out: &mut SceneBuilder| {
        out.element(part, Facet::Casing).path(path.d.clone()).finish();
        draw_marker(out, part, Facet::Casing, markers[0], path.start, MarkerUse::Casing);
        draw_marker(out, part, Facet::Casing, markers[1], path.end, MarkerUse::Casing);
    };
    if wider.is_none() && style.is_empty() {
        body(out);
        return;
    }
    let mut group = LayerSpec::group();
    if let Some(wider) = wider {
        group = group.stroke_width(wider);
    }
    out.layer(group.style((!style.is_empty()).then_some(style)), body);
}

/// Pele's `casingsSvg`: the layer the casings are drawn in.
pub(crate) fn casings_layer() -> LayerSpec {
    LayerSpec::class("pele-casings")
        .fill(Paint::None)
        .stroke(Paint::Role(Role::Background))
        .stroke_width(Number::raw(1.0 + CASE))
        .cap(LineCap::Round)
        .join(LineJoin::Round)
        .untouchable()
}

/// Pele's `edgeLabelSvg`: an edge label on a patch of the background, so the line does not run
/// through the text.
pub(crate) fn draw_edge_label(out: &mut SceneBuilder, part: PartId, label: &Label, at: Point, text: Option<&Style>) {
    let w = label.width + 8.0;
    let h = label.height;
    out.layer(LayerSpec::class("pele-edge-label").part(part, Facet::Label).data_id(), |out| {
        out.element(part, Facet::Label)
            .rect(pair(at.x - w / 2.0, at.y - h / 2.0), pair(w, h), Some(Number::raw(3.0)))
            .fill(Paint::Role(Role::Background))
            .finish();
        out.label(part, Facet::Label, label, at, LabelLook::default().style(text));
    });
}
