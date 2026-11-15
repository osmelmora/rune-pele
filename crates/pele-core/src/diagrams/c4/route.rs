//! How a relation runs between two boxes, ported from Pele's `src/diagrams/c4/route.ts`: a
//! straight line where nothing is in its way, an elbow round the shapes that are, or a detour
//! along a lane beside a row or a column, kept apart from the routes drawn before it.
//!
//! Boxes are named by their place in one list, so that "the same box" is the same number, as it
//! is the same object in Pele.

use crate::graph::edges::{EdgePath, Tip};
use crate::js;
use crate::layout::num::hypot;
use crate::scene::{Number, PathProgram, PathStep};
use std::collections::HashMap;

const LOOP: f64 = 28.0;
pub(super) const LANE: f64 = 20.0;
const LANE_STEP: f64 = 7.0;
const CORNER: f64 = 8.0;
// A wide label on an upright lane leans this far out from it, to the middle of the gap between columns.
const LEAN: f64 = 30.0;
// Label widths that fit the gap between two columns, and where there is room to spare.
pub(super) const NARROW: f64 = 88.0;
const WIDE: f64 = 160.0;
// The space a label's background adds to the width of its text.
pub(super) const LABEL_PAD: f64 = 8.0;
// The shared person shape: a head of radius 13 centred 14 below the top, over a body that starts 26 below it.
pub(super) const HEAD: f64 = 26.0;
// Parallel stretches closer than this read as one line, and a line this close to a boundary's frame
// reads as part of it.
const NEAR: f64 = 6.0;
const FRAME_NEAR: f64 = 3.0;
const PORT_STEP: f64 = 12.0;

/// The shared shape a box is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Form {
    Rect,
    Person,
    Cyl,
    HCyl,
}

impl Form {
    pub(super) fn name(self) -> &'static str {
        match self {
            Form::Rect => "rect",
            Form::Person => "person",
            Form::Cyl => "cyl",
            Form::HCyl => "h-cyl",
        }
    }
}

/// A laid out shape or boundary: its top-left corner, its size, and the shape it is drawn as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Box {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) form: Form,
}

/// A box by its place in the list of boxes.
pub(super) type BoxId = usize;

/// The line of one relation.
#[derive(Clone, Debug)]
pub(super) struct Route {
    /// The path, and where the markers go: each tip and the way it points.
    pub(super) path: EdgePath,
    /// The stretch the label may sit on.
    pub(super) x0: f64,
    pub(super) y0: f64,
    pub(super) x1: f64,
    pub(super) y1: f64,
    /// The side of that stretch the label is pushed to, as a unit vector; zero centres it on the
    /// stretch.
    pub(super) nx: f64,
    pub(super) ny: f64,
    /// When set, the label stays over the line and moves that way by at most this much.
    pub(super) lean: f64,
    /// The widest the label should be.
    pub(super) room: f64,
    /// The corners of the line, as x, y pairs.
    pub(super) points: Vec<f64>,
}

/// The cap radii `drawShape` uses for the two cylinders.
pub(super) fn cyl_ry(w: f64) -> f64 {
    js::min(12.0, js::max(6.0, w / 14.0))
}

pub(super) fn queue_rx(h: f64) -> f64 {
    js::min(12.0, h / 4.0)
}

/// `Math.sign`, which keeps the sign of a zero and a NaN.
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// Whether a number is truthy in JavaScript.
pub(super) fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

/// `(k & 1 ? 1 : -1) * Math.ceil(k / 2)`: 0, 1, -1, 2, -2 and so on, the middle first.
fn spread(k: u32) -> f64 {
    (if k & 1 == 1 { 1.0 } else { -1.0 }) * (f64::from(k) / 2.0).ceil()
}

/// How far along the segment from (px, py) by (dx, dy) it first meets the rectangle; infinity
/// when it misses.
#[allow(clippy::too_many_arguments, reason = "Pele's signature, kept so the port reads beside its source")]
fn hit_rect(x0: f64, y0: f64, x1: f64, y1: f64, px: f64, py: f64, dx: f64, dy: f64) -> f64 {
    let mut enter = 0.0;
    let mut leave = 1.0;
    for (low, high, p, d) in [(x0, x1, px, dx), (y0, y1, py, dy)] {
        if d == 0.0 {
            if p < low || p > high {
                return f64::INFINITY;
            }
        } else {
            let a = (low - p) / d;
            let b = (high - p) / d;
            enter = js::max(enter, js::min(a, b));
            leave = js::min(leave, js::max(a, b));
        }
    }
    if enter <= leave { enter } else { f64::INFINITY }
}

#[allow(clippy::too_many_arguments, reason = "Pele's signature, kept so the port reads beside its source")]
fn hit_ellipse(cx: f64, cy: f64, rx: f64, ry: f64, px: f64, py: f64, dx: f64, dy: f64) -> f64 {
    let ux = dx / rx;
    let uy = dy / ry;
    let vx = (px - cx) / rx;
    let vy = (py - cy) / ry;
    let a = ux * ux + uy * uy;
    let b = 2.0 * (ux * vx + uy * vy);
    let c = vx * vx + vy * vy - 1.0;
    let disc = b * b - 4.0 * a * c;
    if a == 0.0 || disc < 0.0 {
        return f64::INFINITY;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    if (0.0..=1.0).contains(&t) {
        t
    } else if c <= 0.0 {
        0.0
    } else {
        f64::INFINITY
    }
}

/// Where a segment from outside first meets the outline of a shape, as a fraction of the segment.
fn hit(b: &Box, px: f64, py: f64, qx: f64, qy: f64) -> f64 {
    let Box { x, y, w, h, form } = *b;
    let dx = qx - px;
    let dy = qy - py;
    match form {
        Form::Person => js::min(
            hit_rect(x, y + HEAD, x + w, y + h, px, py, dx, dy),
            hit_ellipse(x + w / 2.0, y + 14.0, 13.0, 13.0, px, py, dx, dy),
        ),
        Form::Cyl => {
            let ry = cyl_ry(w);
            js::min(
                js::min(
                    hit_rect(x, y + ry, x + w, y + h - ry, px, py, dx, dy),
                    hit_ellipse(x + w / 2.0, y + ry, w / 2.0, ry, px, py, dx, dy),
                ),
                hit_ellipse(x + w / 2.0, y + h - ry, w / 2.0, ry, px, py, dx, dy),
            )
        }
        Form::HCyl => {
            let rx = queue_rx(h);
            js::min(
                js::min(
                    hit_rect(x + rx, y, x + w - rx, y + h, px, py, dx, dy),
                    hit_ellipse(x + rx, y + h / 2.0, rx, h / 2.0, px, py, dx, dy),
                ),
                hit_ellipse(x + w - rx, y + h / 2.0, rx, h / 2.0, px, py, dx, dy),
            )
        }
        Form::Rect => hit_rect(x, y, x + w, y + h, px, py, dx, dy),
    }
}

/// Whether the line through `points` crosses the rectangle.
pub(super) fn crosses(points: &[f64], x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    (2..points.len()).step_by(2).any(|i| {
        let (px, py) = (points[i - 2], points[i - 1]);
        hit_rect(x0, y0, x1, y1, px, py, points[i] - px, points[i + 1] - py) <= 1.0
    })
}

fn contains(outer: &Box, inner: &Box) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x + outer.w >= inner.x + inner.w
        && outer.y + outer.h >= inner.y + inner.h
}

/// The straight line from a to b: x, y of its start, then of its end.
fn connect(a: &Box, b: &Box, shift: f64) -> [f64; 4] {
    let ax = a.x + a.w / 2.0;
    let ay = a.y + a.h / 2.0;
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let down = contains(a, b);
    if down || contains(b, a) {
        // One holds the other: a short line from the nearest side of the outer box.
        let (outer, inner) = if down { (a, b) } else { (b, a) };
        let ix = inner.x + inner.w / 2.0;
        let iy = inner.y + inner.h / 2.0;
        let left = inner.x - outer.x;
        let right = outer.x + outer.w - inner.x - inner.w;
        let top = inner.y - outer.y;
        let bottom = outer.y + outer.h - inner.y - inner.h;
        let least = js::min(js::min(js::min(left, right), top), bottom);
        let ox = if least == left {
            outer.x
        } else if least == right {
            outer.x + outer.w
        } else {
            ix
        };
        let oy = if least == left || least == right {
            iy
        } else if least == bottom {
            outer.y + outer.h
        } else {
            outer.y
        };
        let t = js::min(hit(inner, ox, oy, ix, iy), 1.0);
        let mx = ox + (ix - ox) * t;
        let my = oy + (iy - oy) * t;
        return if down { [ox, oy, mx, my] } else { [mx, my, ox, oy] };
    }

    let (mut px, mut py, mut qx, mut qy) = (ax, ay, bx, by);
    // Shapes that share a column or a row are joined by an upright or level line through the
    // shared span.
    let x0 = js::max(a.x, b.x);
    let x1 = js::min(a.x + a.w, b.x + b.w);
    let y0 = js::max(a.y, b.y);
    let y1 = js::min(a.y + a.h, b.y + b.h);
    if x0 < x1 {
        px = (x0 + x1) / 2.0;
        qx = px;
    } else if y0 < y1 {
        py = (y0 + y1) / 2.0;
        qy = py;
    }
    let len = hypot(qx - px, qy - py);
    let len = if truthy(len) { len } else { 1.0 };
    let nx = (-(qy - py) / len) * shift;
    let ny = ((qx - px) / len) * shift;
    let mut ta = hit(a, qx + nx, qy + ny, px + nx, py + ny);
    let mut tb = hit(b, px + nx, py + ny, qx + nx, qy + ny);
    if ta <= 1.0 && tb <= 1.0 {
        px += nx;
        py += ny;
        qx += nx;
        qy += ny;
    } else {
        (px, py, qx, qy) = (ax, ay, bx, by);
        ta = js::min(hit(a, qx, qy, px, py), 1.0);
        tb = js::min(hit(b, px, py, qx, qy), 1.0);
    }
    [qx + (px - qx) * ta, qy + (py - qy) * ta, px + (qx - px) * tb, py + (qy - py) * tb]
}

/// What the routes drawn so far take up, so that the next keeps apart from them: upright
/// stretches as x, top, bottom; level ones as y, left, right; and where lines leave or reach
/// each side of a box.
#[derive(Default)]
pub(super) struct Lanes {
    upright: Vec<f64>,
    level: Vec<f64>,
    frames: [Vec<f64>; 2],
    taken: HashMap<BoxId, Vec<(u8, f64)>>,
    /// Routes that found no lane of their own: a line left straight behind a shape, or a lane
    /// that runs along another.
    pub(super) squeezed: usize,
}

impl Lanes {
    fn free(&self, upright: bool, at: f64, a0: f64, a1: f64) -> bool {
        let lo = js::min(a0, a1);
        let hi = js::max(a0, a1);
        let near = |list: &[f64], gap: f64| {
            list.chunks(3).any(|lane| (lane[0] - at).abs() < gap && lane[1] < hi && lane[2] > lo)
        };
        !near(if upright { &self.upright } else { &self.level }, NEAR)
            && !near(&self.frames[if upright { 0 } else { 1 }], FRAME_NEAR)
    }

    fn crossings(&self, points: &[f64]) -> usize {
        let mut count = 0;
        for i in (2..points.len()).step_by(2) {
            let upright = points[i] == points[i - 2];
            let at = if upright { points[i] } else { points[i + 1] };
            let (from, to) = if upright { (points[i - 1], points[i + 1]) } else { (points[i - 2], points[i]) };
            let lo = js::min(from, to);
            let hi = js::max(from, to);
            let list = if upright { &self.level } else { &self.upright };
            count += list.chunks(3).filter(|lane| lane[0] > lo && lane[0] < hi && lane[1] < at && lane[2] > at).count();
        }
        count
    }

    pub(super) fn frame(&mut self, b: &Box) {
        self.frames[0].extend([b.x, b.y, b.y + b.h, b.x + b.w, b.y, b.y + b.h]);
        self.frames[1].extend([b.y, b.x, b.x + b.w, b.y + b.h, b.x, b.x + b.w]);
    }

    fn take(&mut self, points: &[f64]) {
        for i in (2..points.len()).step_by(2) {
            if points[i] == points[i - 2] {
                let (a, b) = (points[i - 1], points[i + 1]);
                self.upright.extend([points[i], js::min(a, b), js::max(a, b)]);
            } else if points[i + 1] == points[i - 1] {
                let (a, b) = (points[i - 2], points[i]);
                self.level.extend([points[i + 1], js::min(a, b), js::max(a, b)]);
            }
        }
    }

    /// Where on one side of a box (0 left, 1 right, 2 top, 3 bottom) a line can leave or arrive,
    /// between lo and hi: the places clear of the lines already there, nearest `want` first.
    fn ports(&self, b: BoxId, side: u8, want: f64, lo: f64, hi: f64) -> Vec<f64> {
        let used = self.taken.get(&b).map_or(&[][..], Vec::as_slice);
        let mut out = Vec::new();
        for k in 0..9 {
            if out.len() >= 4 {
                break;
            }
            let at = want + spread(k) * PORT_STEP;
            if at < lo || at > hi {
                continue;
            }
            if used.iter().all(|&(s, there)| s != side || (there - at).abs() >= PORT_STEP) {
                out.push(at);
            }
        }
        if out.is_empty() { vec![want] } else { out }
    }

    fn claim(&mut self, b: BoxId, side: u8, at: f64) {
        self.taken.entry(b).or_default().push((side, at));
    }
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

/// The path through `points`, its corners rounded, its ends trimmed for the markers.
fn path(points: &[f64], start_trim: f64, end_trim: f64) -> PathProgram {
    let n = points.len();
    let ux = sign(points[2] - points[0]);
    let uy = sign(points[3] - points[1]);
    let mut d =
        PathProgram::default().m(Number::num(points[0] + ux * start_trim), Number::num(points[1] + uy * start_trim));
    for i in (2..n - 2).step_by(2) {
        let (x, y) = (points[i], points[i + 1]);
        let ix = sign(x - points[i - 2]);
        let iy = sign(y - points[i - 1]);
        let ox = sign(points[i + 2] - x);
        let oy = sign(points[i + 3] - y);
        let r = js::min(
            js::min(CORNER, (x - points[i - 2] + y - points[i - 1]).abs() / 2.0),
            (points[i + 2] - x + points[i + 3] - y).abs() / 2.0,
        );
        d = if truthy(ix) { d.h(Number::num(x - ix * r)) } else { d.v(Number::num(y - iy * r)) };
        d.steps.push(PathStep::Quad([pair(x, y), pair(x + ox * r, y + oy * r)]));
    }
    let lx = sign(points[n - 2] - points[n - 4]);
    let ly = sign(points[n - 1] - points[n - 3]);
    if truthy(lx) {
        d.h(Number::num(points[n - 2] - lx * end_trim))
    } else {
        d.v(Number::num(points[n - 1] - ly * end_trim))
    }
}

fn clear_of(points: &[f64], others: &[BoxId], boxes: &[Box], a: BoxId, b: BoxId) -> bool {
    others.iter().all(|&other| {
        let o = &boxes[other];
        other == a || other == b || !crosses(points, o.x, o.y, o.x + o.w, o.y + o.h)
    })
}

/// What a bent route says about its label, beside the stretch it sits on.
struct Leaning {
    nx: f64,
    lean: f64,
    room: f64,
}

/// The route for a line with corners through `points`, from a's outline to b's. `along` is the
/// stretch its label sits on.
fn bent(points: Vec<f64>, along: usize, start_trim: f64, end_trim: f64, label: Leaning) -> Route {
    let n = points.len();
    let start = Tip { x: points[0], y: points[1], dx: -sign(points[2] - points[0]), dy: -sign(points[3] - points[1]) };
    let end = Tip {
        x: points[n - 2],
        y: points[n - 1],
        dx: sign(points[n - 2] - points[n - 4]),
        dy: sign(points[n - 1] - points[n - 3]),
    };
    Route {
        path: EdgePath { d: path(&points, start_trim, end_trim), start, end },
        x0: points[along],
        y0: points[along + 1],
        x1: points[along + 2],
        y1: points[along + 3],
        nx: label.nx,
        ny: 0.0,
        lean: label.lean,
        room: label.room,
        points,
    }
}

/// The places to try for a lane across the gap from `lo` to `hi`, the middle first.
fn across(lo: f64, hi: f64) -> Vec<f64> {
    let mid = (lo + hi) / 2.0;
    let step = js::max(LANE_STEP, (hi - lo) / 4.0);
    (0..9).map(|k| mid + spread(k) * step).filter(|&at| at >= lo + NEAR && at <= hi - NEAR).collect()
}

/// What a relation is routed between: the list of boxes, the two it joins, and how much of
/// each end its markers take.
pub(super) struct Ends<'a> {
    pub(super) boxes: &'a [Box],
    pub(super) a: BoxId,
    pub(super) b: BoxId,
    pub(super) start_trim: f64,
    pub(super) end_trim: f64,
}

/// The best elbow found so far.
struct Elbow {
    points: Vec<f64>,
    cost: f64,
    upright: bool,
    sides: [u8; 2],
}

/// A route of three stretches round the shapes in the way, across the gap between a and b.
fn elbow(ends: &Ends<'_>, others: &[BoxId], lanes: &mut Lanes) -> Option<Route> {
    let (a, b) = (&ends.boxes[ends.a], &ends.boxes[ends.b]);
    let ax = a.x + a.w / 2.0;
    let ay = a.y + a.h / 2.0;
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let mut best: Option<Elbow> = None;
    // Answers whether the points are the best elbow so far.
    let consider = |best: &mut Option<Elbow>, lanes: &Lanes, points: Vec<f64>, upright: bool, sides: [u8; 2]| {
        if !clear_of(&points, others, ends.boxes, ends.a, ends.b) {
            return false;
        }
        for i in (2..points.len()).step_by(2) {
            let up = points[i] == points[i - 2];
            let (at, from, to) =
                if up { (points[i], points[i - 1], points[i + 1]) } else { (points[i + 1], points[i - 2], points[i]) };
            if !lanes.free(up, at, from, to) {
                return false;
            }
        }
        let mut cost = lanes.crossings(&points) as f64 * 1e6;
        for i in (2..points.len()).step_by(2) {
            cost += (points[i] - points[i - 2]).abs() + (points[i + 1] - points[i - 1]).abs();
        }
        if best.as_ref().is_some_and(|best| cost >= best.cost) {
            return false;
        }
        *best = Some(Elbow { points, cost, upright, sides });
        true
    };
    if a.x + a.w <= b.x || b.x + b.w <= a.x {
        let right = bx > ax;
        let down = sign(by - ay);
        let sides = if right { [1, 0] } else { [0, 1] };
        let lanes_across = across(if right { a.x + a.w } else { b.x + b.w }, if right { b.x } else { a.x });
        'search: for y0 in lanes.ports(ends.a, sides[0], ay + (down * a.h) / 4.0, a.y + 8.0, a.y + a.h - 8.0) {
            for y1 in lanes.ports(ends.b, sides[1], by - (down * b.h) / 4.0, b.y + 8.0, b.y + b.h - 8.0) {
                for &lane in &lanes_across {
                    let x0 = lane + (ax - lane) * js::min(hit(a, lane, y0, ax, y0), 1.0);
                    let x1 = lane + (bx - lane) * js::min(hit(b, lane, y1, bx, y1), 1.0);
                    if consider(&mut best, lanes, vec![x0, y0, lane, y0, lane, y1, x1, y1], true, sides) {
                        break 'search;
                    }
                }
            }
        }
    }
    if a.y + a.h <= b.y || b.y + b.h <= a.y {
        let below = by > ay;
        let sideways = sign(bx - ax);
        let sides = if below { [3, 2] } else { [2, 3] };
        let lanes_across = across(if below { a.y + a.h } else { b.y + b.h }, if below { b.y } else { a.y });
        'search: for x0 in lanes.ports(ends.a, sides[0], ax + (sideways * a.w) / 4.0, a.x + 8.0, a.x + a.w - 8.0) {
            for x1 in lanes.ports(ends.b, sides[1], bx - (sideways * b.w) / 4.0, b.x + 8.0, b.x + b.w - 8.0) {
                for &lane in &lanes_across {
                    let y0 = lane + (ay - lane) * js::min(hit(a, x0, lane, x0, ay), 1.0);
                    let y1 = lane + (by - lane) * js::min(hit(b, x1, lane, x1, by), 1.0);
                    if consider(&mut best, lanes, vec![x0, y0, x0, lane, x1, lane, x1, y1], false, sides) {
                        break 'search;
                    }
                }
            }
        }
    }
    let Elbow { points, upright, sides, .. } = best?;
    lanes.take(&points);
    lanes.claim(ends.a, sides[0], if upright { points[1] } else { points[0] });
    lanes.claim(ends.b, sides[1], if upright { points[7] } else { points[6] });
    // The label sits on the middle stretch, across the gap.
    let room = if upright { NARROW } else { WIDE };
    Some(bent(points, 2, ends.start_trim, ends.end_trim, Leaning { nx: 0.0, lean: 0.0, room }))
}

/// A detour along one side, as `along` finds it.
struct Way {
    points: Vec<f64>,
    side: u8,
    p0: f64,
    p1: f64,
    /// How many lanes out it went; 5 when none was free.
    step: u32,
}

/// Routes a relation from a to b. `shift` moves the line sideways when another runs the opposite
/// way. `others` are the shapes to steer clear of; `None` skips that check. `lanes` holds what
/// the routes before it take up, so that this one keeps apart from them.
pub(super) fn route(ends: &Ends<'_>, shift: f64, others: Option<&[BoxId]>, lanes: &mut Lanes) -> Route {
    let (a, b) = (&ends.boxes[ends.a], &ends.boxes[ends.b]);
    let (start_trim, end_trim) = (ends.start_trim, ends.end_trim);
    if ends.a == ends.b {
        let spread = js::min(a.h / 2.0 - 4.0, 10.0);
        let len = hypot(LOOP, spread);
        let x = a.x + a.w;
        let cy = a.y + a.h / 2.0;
        let ux = LOOP / len;
        let uy = spread / len;
        let d =
            PathProgram::default().m(Number::num(x + ux * start_trim), Number::num(cy - spread - uy * start_trim)).c(
                pair(x + LOOP, cy - spread * 2.0),
                pair(x + LOOP, cy + spread * 2.0),
                pair(x + ux * end_trim, cy + spread + uy * end_trim),
            );
        let start = Tip { x, y: cy - spread, dx: -ux, dy: uy };
        let end = Tip { x, y: cy + spread, dx: -ux, dy: -uy };
        return Route {
            path: EdgePath { d, start, end },
            x0: x + LOOP + 4.0,
            y0: cy,
            x1: x + LOOP + 4.0,
            y1: cy,
            nx: 1.0,
            ny: 0.0,
            lean: 0.0,
            room: 64.0,
            points: Vec::new(),
        };
    }

    let [sx, sy, ex, ey] = connect(a, b, shift);
    let upright = sx == ex;
    let in_way = |other: BoxId| {
        let o = &ends.boxes[other];
        other != ends.a && other != ends.b && hit_rect(o.x, o.y, o.x + o.w, o.y + o.h, sx, sy, ex - sx, ey - sy) <= 1.0
    };
    if let Some(others) = others {
        let blocked = !contains(a, b) && !contains(b, a) && others.iter().any(|&other| in_way(other));
        if blocked && !upright && sy != ey {
            if let Some(around) = elbow(ends, others, lanes) {
                return around;
            }
            lanes.squeezed += 1;
        }
        if blocked && (upright || sy == ey) {
            return detour(ends, others, lanes, upright, [sx, sy, ex, ey], &in_way);
        }
    }

    let len = hypot(ex - sx, ey - sy);
    let len = if truthy(len) { len } else { 1.0 };
    let dx = (ex - sx) / len;
    let dy = (ey - sy) / len;
    if upright || sy == ey {
        lanes.take(&[sx, sy, ex, ey]);
    }
    let d = PathProgram::default()
        .m(Number::num(sx + dx * start_trim), Number::num(sy + dy * start_trim))
        .l(Number::num(ex - dx * end_trim), Number::num(ey - dy * end_trim));
    Route {
        path: EdgePath { d, start: Tip { x: sx, y: sy, dx: -dx, dy: -dy }, end: Tip { x: ex, y: ey, dx, dy } },
        x0: sx,
        y0: sy,
        x1: ex,
        y1: ey,
        nx: if truthy(shift) { -dy } else { 0.0 },
        ny: if truthy(shift) { dx } else { 0.0 },
        lean: 0.0,
        room: if dx.abs() > dy.abs() { NARROW } else { WIDE },
        points: vec![sx, sy, ex, ey],
    }
}

/// The detour of an upright or level line that a shape is in the way of: it runs along a lane
/// beside the row or the column.
fn detour(
    ends: &Ends<'_>,
    others: &[BoxId],
    lanes: &mut Lanes,
    upright: bool,
    [sx, sy, ex, ey]: [f64; 4],
    in_way: &dyn Fn(BoxId) -> bool,
) -> Route {
    let boxes = ends.boxes;
    let (a, b) = (&boxes[ends.a], &boxes[ends.b]);
    let lane_on = |forward: bool| {
        let mut lane = if forward { f64::NEG_INFINITY } else { f64::INFINITY };
        let mut widen = |o: &Box| {
            let edge = if upright { o.x } else { o.y };
            lane = if forward { js::max(lane, edge + if upright { o.w } else { o.h }) } else { js::min(lane, edge) };
        };
        widen(a);
        widen(b);
        for &other in others {
            if in_way(other) {
                widen(&boxes[other]);
            }
        }
        lane
    };
    let lo = if upright { js::min(a.y, b.y) } else { js::min(a.x, b.x) };
    let hi = if upright { js::max(a.y + a.h, b.y + b.h) } else { js::max(a.x + a.w, b.x + b.w) };
    let outside = |forward: bool, lane: f64| {
        others.iter().filter(|&&other| other != ends.a && other != ends.b).all(|&other| {
            let o = &boxes[other];
            let near = if upright { o.y < hi && o.y + o.h > lo } else { o.x < hi && o.x + o.w > lo };
            let start = if upright { o.x } else { o.y };
            let end = start + if upright { o.w } else { o.h };
            !(near && if forward { end > lane } else { start < lane })
        })
    };
    let ax = a.x + a.w / 2.0;
    let ay = a.y + a.h / 2.0;
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let towards = if upright { sign(by - ay) } else { sign(bx - ax) };
    // The route on one side: each end leaves its shape a quarter of the way from the middle
    // towards the other, clear of the straight lines that meet the middle of that side and of
    // other routes there. The lane runs a little further out for each route already along that
    // gap, and never behind a shape.
    let along = |lanes: &Lanes, forward: bool| -> Way {
        let lane = lane_on(forward);
        let sign = if forward { 1.0 } else { -1.0 };
        let side = if upright {
            u8::from(forward)
        } else if forward {
            3
        } else {
            2
        };
        let (p0, p1) = if upright {
            (
                lanes.ports(ends.a, side, ay + (towards * a.h) / 4.0, a.y + 8.0, a.y + a.h - 8.0)[0],
                lanes.ports(ends.b, side, by - (towards * b.h) / 4.0, b.y + 8.0, b.y + b.h - 8.0)[0],
            )
        } else {
            (
                lanes.ports(ends.a, side, ax + (towards * a.w) / 4.0, a.x + 8.0, a.x + a.w - 8.0)[0],
                lanes.ports(ends.b, side, bx - (towards * b.w) / 4.0, b.x + 8.0, b.x + b.w - 8.0)[0],
            )
        };
        let points_at = |at: f64| -> Vec<f64> {
            if upright {
                let x0 = at + (ax - at) * js::min(hit(a, at, p0, ax, p0), 1.0);
                let x1 = at + (bx - at) * js::min(hit(b, at, p1, bx, p1), 1.0);
                vec![x0, p0, at, p0, at, p1, x1, p1]
            } else {
                let y0 = at + (ay - at) * js::min(hit(a, p0, at, p0, ay), 1.0);
                let y1 = at + (by - at) * js::min(hit(b, p1, at, p1, by), 1.0);
                vec![p0, y0, p0, at, p1, at, p1, y1]
            }
        };
        let mut points = points_at(lane + sign * LANE);
        for k in 0..=4u32 {
            let at = lane + sign * (LANE + f64::from(k) * LANE_STEP);
            let tried = points_at(at);
            if !clear_of(&tried, others, boxes, ends.a, ends.b) {
                continue;
            }
            points = tried;
            if lanes.free(upright, at, p0, p1) {
                return Way { points, side, p0, p1, step: k };
            }
        }
        Way { points, side, p0, p1, step: 5 }
    };
    // A detour takes the outside of the drawing when only one side is outside, leaving the gaps
    // between columns and rows to the relations that cross them. Otherwise it takes the side
    // where it crosses fewer lines, then the one where it runs nearer its shapes, so that
    // detours spread over both sides.
    let natural = if upright { ey > sy } else { ex < sx };
    let out = outside(natural, lane_on(natural));
    let back = outside(!natural, lane_on(!natural));
    let first = along(lanes, natural);
    let other = (out == back).then(|| along(lanes, !natural));
    let cost = |way: &Way| lanes.crossings(&way.points) as f64 * 8.0 + f64::from(way.step);
    let turn = (back && !out) || other.as_ref().is_some_and(|other| cost(other) < cost(&first));
    let forward = if turn { !natural } else { natural };
    let Way { points, side, p0, p1, step } =
        if forward == natural { first } else { other.unwrap_or_else(|| along(lanes, !natural)) };
    if step > 4 {
        lanes.squeezed += 1;
    }
    lanes.take(&points);
    lanes.claim(ends.a, side, p0);
    lanes.claim(ends.b, side, p1);
    let label = if upright {
        Leaning { nx: if forward { 1.0 } else { -1.0 }, lean: LEAN, room: NARROW }
    } else {
        Leaning { nx: 0.0, lean: 0.0, room: WIDE }
    };
    bent(points, 2, ends.start_trim, ends.end_trim, label)
}
