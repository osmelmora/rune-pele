//! Outlines: SVG path programs lowered to lines and cubics, moved by the transforms of the groups
//! around them, and flattened to the polylines Rune's shapes are built from.
//!
//! Rune's own `Path` flattens at a fixed tenth of a point, before the window's scale. Here the
//! tolerance is the caller's, so a curve stays smooth at any device scale.

use std::f64::consts::{FRAC_PI_2, TAU};

/// A point in the drawing's user units, y down.
pub type Point = [f64; 2];

/// One absolute command of an SVG path program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(Point),
    Line(Point),
    Quad { control: Point, to: Point },
    Cubic(Cubic),
    Arc(Arc),
    Close,
}

/// SVG's endpoint arc, `A rx,ry rotation large-arc sweep x,y`, with `rotation` in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub radii: [f64; 2],
    pub rotation: f64,
    pub large: bool,
    pub sweep: bool,
    pub to: Point,
}

/// A cubic Bézier from the current point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cubic {
    pub c1: Point,
    pub c2: Point,
    pub to: Point,
}

/// An arc-free path command: what a program lowers to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Move(Point),
    Line(Point),
    Cubic(Cubic),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    edges: Vec<Edge>,
}

/// One subpath as the points a renderer joins with straight lines. A closed one runs back from
/// its last point to its first.
#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    pub points: Vec<Point>,
    pub closed: bool,
    /// For each point, the unit directions the path arrives and leaves in where two of its
    /// commands meet there. A point with none is an end, or where a curve was cut: the path runs
    /// on through it smoothly, whatever the angle between the two chords.
    pub corners: Vec<Option<[Point; 2]>>,
}

pub(crate) fn unit(from: Point, to: Point) -> Option<Point> {
    let d = (to[0] - from[0]).hypot(to[1] - from[1]);
    (d > 0.0).then(|| [(to[0] - from[0]) / d, (to[1] - from[1]) / d])
}

impl Polyline {
    /// A path of straight lines through `points`, every one of them a corner.
    pub fn straight(points: &[Point], closed: bool) -> Polyline {
        let n = points.len();
        let corner = |i: usize| {
            if !closed && (i == 0 || i + 1 == n) {
                return None;
            }
            Some([unit(points[(i + n - 1) % n], points[i])?, unit(points[i], points[(i + 1) % n])?])
        };
        Polyline { points: points.to_vec(), closed, corners: (0..n).map(corner).collect() }
    }
}

/// An affine map of the plane: `(a·x + c·y + e, b·x + d·y + f)`, as SVG's `matrix(a b c d e f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate([dx, dy]: Point) -> Affine {
        Affine { e: dx, f: dy, ..Affine::IDENTITY }
    }

    /// `factor` times as large, about the origin.
    pub fn scale(factor: f64) -> Affine {
        Affine { a: factor, d: factor, ..Affine::IDENTITY }
    }

    /// SVG's `rotate(degrees, cx, cy)`: clockwise on screen, about `centre`.
    pub fn rotate_about(degrees: f64, [cx, cy]: Point) -> Affine {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Affine { a: cos, b: sin, c: -sin, d: cos, e: cx - cos * cx + sin * cy, f: cy - sin * cx - cos * cy }
    }

    /// The map that applies `inner` first and then this one, as a nested SVG group composes.
    pub fn then(&self, inner: &Affine) -> Affine {
        Affine {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }

    pub fn apply(&self, [x, y]: Point) -> Point {
        [self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f]
    }

    /// How far the map turns the x axis, in radians clockwise on screen.
    pub fn turn(&self) -> f64 {
        self.b.atan2(self.a)
    }

    pub fn is_translation(&self) -> bool {
        (self.a, self.b, self.c, self.d) == (1.0, 0.0, 0.0, 1.0)
    }

    /// Whether the map only moves and scales alike both ways, so that an upright box stays one.
    pub fn is_upright(&self) -> bool {
        self.b == 0.0 && self.c == 0.0 && self.a == self.d && self.a > 0.0
    }
}

/// The cubics that draw `arc` from `from`, each spanning at most 90 degrees, per the SVG 1.1
/// implementation notes F.6.5 and F.6.6. A zero radius draws a straight line; an arc that ends
/// where it starts draws nothing.
pub fn arc_edges(from: Point, arc: &Arc) -> Vec<Edge> {
    let to = arc.to;
    if from == to {
        return Vec::new();
    }
    let (mut rx, mut ry) = (arc.radii[0].abs(), arc.radii[1].abs());
    if rx == 0.0 || ry == 0.0 {
        return vec![Edge::Line(to)];
    }
    let (sin, cos) = arc.rotation.to_radians().sin_cos();
    let (dx, dy) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let scale = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if scale > 1.0 {
        rx *= scale.sqrt();
        ry *= scale.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let sign = if arc.large == arc.sweep { -1.0 } else { 1.0 };
    let k = sign * (num / den).max(0.0).sqrt();
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let centre = [cos * cx1 - sin * cy1 + (from[0] + to[0]) / 2.0, sin * cx1 + cos * cy1 + (from[1] + to[1]) / 2.0];
    let start = ((y1 - cy1) / ry).atan2((x1 - cx1) / rx);
    let mut sweep = ((-y1 - cy1) / ry).atan2((-x1 - cx1) / rx) - start;
    if arc.sweep && sweep < 0.0 {
        sweep += TAU;
    } else if !arc.sweep && sweep > 0.0 {
        sweep -= TAU;
    }
    let pieces = (sweep.abs() / FRAC_PI_2 - 1e-9).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let t = 4.0 / 3.0 * (step / 4.0).tan();
    let place = |u: f64, v: f64| [centre[0] + rx * cos * u - ry * sin * v, centre[1] + rx * sin * u + ry * cos * v];
    (0..pieces)
        .map(|i| {
            let (a, b) = (start + step * i as f64, start + step * (i + 1) as f64);
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = b.sin_cos();
            let end = if i + 1 == pieces { to } else { place(cb, sb) };
            Edge::Cubic(Cubic { c1: place(ca - t * sa, sa + t * ca), c2: place(cb + t * sb, sb - t * cb), to: end })
        })
        .collect()
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

impl Outline {
    pub fn lower(program: &[Segment]) -> Outline {
        let mut edges = Vec::with_capacity(program.len());
        let (mut at, mut start) = ([0.0; 2], [0.0; 2]);
        for segment in program {
            match *segment {
                Segment::Move(p) => {
                    edges.push(Edge::Move(p));
                    (at, start) = (p, p);
                }
                Segment::Line(p) => {
                    edges.push(Edge::Line(p));
                    at = p;
                }
                // The cubic with the same curve: each handle two thirds of the way to the quad's.
                Segment::Quad { control, to } => {
                    let cubic = Cubic { c1: lerp(at, control, 2.0 / 3.0), c2: lerp(to, control, 2.0 / 3.0), to };
                    edges.push(Edge::Cubic(cubic));
                    at = to;
                }
                Segment::Cubic(c) => {
                    edges.push(Edge::Cubic(c));
                    at = c.to;
                }
                Segment::Arc(arc) => {
                    edges.extend(arc_edges(at, &arc));
                    at = arc.to;
                }
                Segment::Close => {
                    edges.push(Edge::Close);
                    at = start;
                }
            }
        }
        Outline { edges }
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The outline as a group's transform leaves it. Lines and cubics map to lines and cubics,
    /// which is why arcs are lowered first.
    pub fn transformed(&self, by: &Affine) -> Outline {
        let p = |point: Point| by.apply(point);
        let edges = self
            .edges
            .iter()
            .map(|edge| match *edge {
                Edge::Move(a) => Edge::Move(p(a)),
                Edge::Line(a) => Edge::Line(p(a)),
                Edge::Cubic(c) => Edge::Cubic(Cubic { c1: p(c.c1), c2: p(c.c2), to: p(c.to) }),
                Edge::Close => Edge::Close,
            })
            .collect();
        Outline { edges }
    }

    /// The outline as polylines that stay within `tolerance` of its curves.
    pub fn flatten(&self, tolerance: f64) -> Vec<Polyline> {
        let mut lines: Vec<Subpath> = Vec::new();
        let mut at = [0.0; 2];
        for edge in &self.edges {
            match *edge {
                Edge::Move(p) => {
                    lines.push(Subpath::starting(p));
                    at = p;
                }
                Edge::Close => {
                    if let Some(line) = lines.last_mut().filter(|line| !line.line.closed) {
                        let start = line.line.points[0];
                        line.close();
                        at = start;
                    }
                }
                Edge::Line(p) => {
                    if let Some(direction) = unit(at, p) {
                        open(&mut lines, at).extend([direction; 2], |points| points.push(p));
                    }
                    at = p;
                }
                Edge::Cubic(c) => {
                    // The curve leaves along its first handle that is not on its start, and
                    // arrives along its last that is not on its end.
                    let leaves = [c.c1, c.c2, c.to].into_iter().find_map(|handle| unit(at, handle));
                    let arrives = [c.c2, c.c1, at].into_iter().find_map(|handle| unit(handle, c.to));
                    if let (Some(leaves), Some(arrives)) = (leaves, arrives) {
                        open(&mut lines, at)
                            .extend([leaves, arrives], |points| flatten_cubic(at, &c, tolerance, DEEPEST, points));
                    }
                    at = c.to;
                }
            }
        }
        lines.into_iter().map(|line| line.line).collect()
    }
}

/// A subpath as it is flattened, with the directions it started and last arrived in.
struct Subpath {
    line: Polyline,
    started: Option<Point>,
    arrived: Option<Point>,
}

impl Subpath {
    fn starting(at: Point) -> Subpath {
        let line = Polyline { points: vec![at], closed: false, corners: vec![None] };
        Subpath { line, started: None, arrived: None }
    }

    /// Adds one command that leaves and arrives in `directions`: where it starts is a corner
    /// between the command before and it.
    fn extend(&mut self, [leaves, arrives]: [Point; 2], add: impl FnOnce(&mut Vec<Point>)) {
        match self.arrived {
            Some(arrived) => *self.line.corners.last_mut().expect("a subpath has a point") = Some([arrived, leaves]),
            None => self.started = Some(leaves),
        }
        add(&mut self.line.points);
        self.line.corners.resize(self.line.points.len(), None);
        self.arrived = Some(arrives);
    }

    /// Closes the subpath with a line back to its start, unless it is there already. Its start
    /// is then a corner too.
    fn close(&mut self) {
        let start = self.line.points[0];
        let end = *self.line.points.last().expect("a subpath has a point");
        match unit(end, start) {
            Some(direction) => self.extend([direction; 2], |_| {}),
            // A subpath that spells its start again at its end has that point once.
            None if self.line.points.len() > 1 => {
                self.line.points.pop();
                self.line.corners.pop();
            }
            None => {}
        }
        self.line.closed = true;
        if let (Some(arrived), Some(started)) = (self.arrived, self.started) {
            self.line.corners[0] = Some([arrived, started]);
        }
    }
}

/// The subpath being drawn. A command after a close starts a new one where the closed one
/// began, as SVG says.
fn open(lines: &mut Vec<Subpath>, at: Point) -> &mut Subpath {
    if lines.last().is_none_or(|line| line.line.closed) {
        lines.push(Subpath::starting(at));
    }
    lines.last_mut().expect("pushed above")
}

/// How often one curve is halved at most: 2 to this power is the most pieces it is cut into.
const DEEPEST: u32 = 10;

/// Appends the points after `from` of a polyline within `tolerance` of the cubic. The curve is
/// halved until each piece is flat enough, so that it is cut where it bends and left long where
/// it runs straight: Rune draws a stroke one segment at a time, and every joint shows a little.
fn flatten_cubic(from: Point, c: &Cubic, tolerance: f64, deeper: u32, out: &mut Vec<Point>) {
    let chord = [c.to[0] - from[0], c.to[1] - from[1]];
    let length = chord[0].hypot(chord[1]);
    // How far a handle lies off the chord, and whether it lies beside it: a handle past either
    // end pulls the curve out along the chord's line, where its distance from that line is none.
    let off = |handle: Point| {
        let (dx, dy) = (handle[0] - from[0], handle[1] - from[1]);
        if length == 0.0 {
            return (dx.hypot(dy), true);
        }
        let along = (dx * chord[0] + dy * chord[1]) / (length * length);
        ((dx * chord[1] - dy * chord[0]).abs() / length, (0.0..=1.0).contains(&along))
    };
    let ((d1, beside1), (d2, beside2)) = (off(c.c1), off(c.c2));
    // A cubic strays from its chord by at most three quarters of its handles' distance from it.
    if deeper == 0 || (beside1 && beside2 && 0.75 * d1.max(d2) <= tolerance) {
        out.push(c.to);
        return;
    }
    let mid = |a: Point, b: Point| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let (ab, bc, cd) = (mid(from, c.c1), mid(c.c1, c.c2), mid(c.c2, c.to));
    let (abc, bcd) = (mid(ab, bc), mid(bc, cd));
    let middle = mid(abc, bcd);
    flatten_cubic(from, &Cubic { c1: ab, c2: abc, to: middle }, tolerance, deeper - 1, out);
    flatten_cubic(middle, &Cubic { c1: bcd, c2: cd, to: c.to }, tolerance, deeper - 1, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cubic_at(from: Point, c: &Cubic, t: f64) -> Point {
        let u = 1.0 - t;
        let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        [0, 1].map(|i| b0 * from[i] + b1 * c.c1[i] + b2 * c.c2[i] + b3 * c.to[i])
    }

    fn cubics(edges: &[Edge]) -> Vec<Cubic> {
        edges
            .iter()
            .map(|edge| match edge {
                Edge::Cubic(c) => *c,
                other => panic!("an arc lowers to cubics only, got {other:?}"),
            })
            .collect()
    }

    /// Every sampled point of `edges`, starting at `from`.
    fn samples(from: Point, edges: &[Edge]) -> Vec<Point> {
        let mut at = from;
        let mut out = Vec::new();
        for c in cubics(edges) {
            out.extend((0..=16).map(|i| cubic_at(at, &c, i as f64 / 16.0)));
            at = c.to;
        }
        out
    }

    /// How far `p` is from the ellipse with these radii and rotation about `centre`, relative to
    /// its size: 0 on the ellipse.
    fn off_ellipse(p: Point, centre: Point, [rx, ry]: [f64; 2], rotation: f64) -> f64 {
        let (sin, cos) = rotation.to_radians().sin_cos();
        let (dx, dy) = (p[0] - centre[0], p[1] - centre[1]);
        let (u, v) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        ((u / rx).powi(2) + (v / ry).powi(2)).sqrt() - 1.0
    }

    fn circle_arc(to: Point, r: f64, large: bool, sweep: bool) -> Arc {
        Arc { radii: [r, r], rotation: 0.0, large, sweep, to }
    }

    /// The point at `angle` radians clockwise from twelve o'clock.
    fn polar(angle: f64, radius: f64) -> Point {
        [angle.sin() * radius, -angle.cos() * radius]
    }

    #[test]
    fn a_quarter_circle_is_one_cubic_on_the_circle_ending_exactly_at_its_endpoint() {
        let edges = arc_edges([100.0, 0.0], &circle_arc([0.0, 100.0], 100.0, false, true));
        let [c] = cubics(&edges)[..] else { panic!("one cubic for 90 degrees: {edges:?}") };
        assert_eq!(c.to, [0.0, 100.0]);
        let worst = samples([100.0, 0.0], &edges)
            .iter()
            .map(|&p| off_ellipse(p, [0.0; 2], [100.0; 2], 0.0).abs())
            .fold(0.0, f64::max);
        assert!(worst < 3e-4, "a 90-degree cubic stays within 0.03% of the radius: {worst}");
    }

    #[test]
    fn no_cubic_spans_more_than_ninety_degrees() {
        for (degrees, want) in [(45.0, 1), (90.0, 1), (90.5, 2), (180.0, 2), (284.4, 4), (359.0, 4)] {
            let end: f64 = f64::to_radians(degrees);
            let edges = arc_edges(polar(0.0, 50.0), &circle_arc(polar(end, 50.0), 50.0, degrees > 180.0, true));
            assert_eq!(cubics(&edges).len(), want, "{degrees} degrees");
        }
    }

    #[test]
    fn the_flags_choose_which_of_the_four_arcs_through_two_points() {
        let (from, to, r) = ([0.0, 0.0], [10.0, 0.0], 10.0);
        let below = |p: &Point| p[1] > 1e-9;
        for (large, sweep, centre_y) in [
            (false, true, 75f64.sqrt()),
            (false, false, -75f64.sqrt()),
            (true, true, -75f64.sqrt()),
            (true, false, 75f64.sqrt()),
        ] {
            let edges = arc_edges(from, &circle_arc(to, r, large, sweep));
            let points = samples(from, &edges);
            for p in &points {
                assert!(
                    off_ellipse(*p, [5.0, centre_y], [r, r], 0.0).abs() < 3e-4,
                    "large {large} sweep {sweep}: {p:?}"
                );
            }
            let mid = points[points.len() / 2];
            let span = if large { "long" } else { "short" };
            assert_eq!(below(&mid), (centre_y > 0.0) == large, "{span} way round, sweep {sweep}: {mid:?}");
        }
    }

    #[test]
    fn radii_too_small_to_reach_grow_to_a_half_ellipse() {
        let edges = arc_edges(
            [0.0, 0.0],
            &Arc { radii: [1.0, 2.0], rotation: 0.0, large: false, sweep: true, to: [10.0, 0.0] },
        );
        for p in samples([0.0, 0.0], &edges) {
            assert!(off_ellipse(p, [5.0, 0.0], [5.0, 10.0], 0.0).abs() < 3e-4, "{p:?}");
        }
        assert_eq!(cubics(&edges).len(), 2);
    }

    #[test]
    fn a_rotated_ellipse_arc_stays_on_its_ellipse() {
        let (centre, radii, rotation) = ([30.0, -20.0], [40.0, 15.0], 30.0);
        let (sin, cos) = f64::to_radians(rotation).sin_cos();
        let on = |a: f64| {
            let (u, v) = (radii[0] * a.cos(), radii[1] * a.sin());
            [centre[0] + cos * u - sin * v, centre[1] + sin * u + cos * v]
        };
        let (from, to) = (on(0.3), on(2.9));
        let edges = arc_edges(from, &Arc { radii, rotation, large: false, sweep: true, to });
        assert_eq!(cubics(&edges).last().map(|c| c.to), Some(to));
        for p in samples(from, &edges) {
            assert!(off_ellipse(p, centre, radii, rotation).abs() < 3e-4, "{p:?}");
        }
    }

    #[test]
    fn a_zero_radius_is_a_line_and_a_zero_length_arc_is_nothing() {
        assert_eq!(arc_edges([0.0, 0.0], &circle_arc([5.0, 5.0], 0.0, false, true)), vec![Edge::Line([5.0, 5.0])]);
        assert_eq!(arc_edges([5.0, 5.0], &circle_arc([5.0, 5.0], 3.0, false, true)), vec![]);
    }

    #[test]
    fn a_large_arc_of_a_small_chord_goes_the_long_way_round_its_circle() {
        // Usecase's actor head: almost a whole circle between two points close together.
        let (r, from, to) = (10.0, polar(0.1, 10.0), polar(-0.1, 10.0));
        let edges = arc_edges(from, &circle_arc(to, r, true, true));
        let points = samples(from, &edges);
        assert!(points.iter().all(|&p| off_ellipse(p, [0.0; 2], [r; 2], 0.0).abs() < 3e-4));
        assert!(points.iter().any(|p| p[1] > 9.9), "the arc passes six o'clock");
        assert_eq!(cubics(&edges).len(), 4);
    }

    #[test]
    fn a_quadratic_is_the_cubic_through_the_same_points() {
        let (from, control, to) = ([0.0, 0.0], [10.0, 20.0], [20.0, 0.0]);
        let outline = Outline::lower(&[Segment::Move(from), Segment::Quad { control, to }]);
        let [Edge::Move(_), Edge::Cubic(c)] = outline.edges() else { panic!("a move and a cubic") };
        for i in 0..=8 {
            let t = f64::from(i) / 8.0;
            let u = 1.0 - t;
            let quad = [0, 1].map(|k| u * u * from[k] + 2.0 * u * t * control[k] + t * t * to[k]);
            let cubic = cubic_at(from, c, t);
            assert!((quad[0] - cubic[0]).hypot(quad[1] - cubic[1]) < 1e-12, "at {t}: {quad:?} against {cubic:?}");
        }
    }

    #[test]
    fn a_flattened_curve_stays_within_its_tolerance_and_ends_on_its_points() {
        let arc = circle_arc([0.0, 50.0], 50.0, false, true);
        let outline = Outline::lower(&[Segment::Move([50.0, 0.0]), Segment::Arc(arc)]);
        for tolerance in [0.5, 0.05, 0.005] {
            let [line] = &outline.flatten(tolerance)[..] else { panic!("one subpath") };
            assert_eq!((line.points[0], *line.points.last().unwrap()), ([50.0, 0.0], [0.0, 50.0]));
            let worst = line
                .points
                .windows(2)
                .map(|pair| 50.0 - ((pair[0][0] + pair[1][0]) / 2.0).hypot((pair[0][1] + pair[1][1]) / 2.0))
                .fold(0.0, f64::max);
            assert!(worst <= tolerance, "chords sag {worst} at tolerance {tolerance}");
            assert!(worst > tolerance / 8.0, "and no finer than the tolerance asks: {worst} at {tolerance}");
        }
    }

    #[test]
    fn a_curve_is_cut_where_it_bends_and_left_long_where_it_runs_straight() {
        // An edge as Pele routes one: straight down, a short S, straight down again.
        let s = Cubic { c1: [0.0, 60.0], c2: [40.0, 20.0], to: [40.0, 80.0] };
        let outline = Outline::lower(&[Segment::Move([0.0, 0.0]), Segment::Cubic(s)]);
        let [line] = &outline.flatten(0.25)[..] else { panic!("one subpath") };
        let lengths: Vec<f64> =
            line.points.windows(2).map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1])).collect();
        let (shortest, longest) =
            (lengths.iter().copied().fold(f64::MAX, f64::min), lengths.iter().copied().fold(0.0, f64::max));
        assert!(longest > 2.0 * shortest, "pieces from {shortest} to {longest} long: {lengths:?}");
        for pair in line.points.windows(2) {
            let middle = [(pair[0][0] + pair[1][0]) / 2.0, (pair[0][1] + pair[1][1]) / 2.0];
            let nearest = (0..=2000)
                .map(|i| cubic_at([0.0, 0.0], &s, f64::from(i) / 2000.0))
                .map(|p| (p[0] - middle[0]).hypot(p[1] - middle[1]))
                .fold(f64::MAX, f64::min);
            assert!(nearest <= 0.25, "a chord's middle is {nearest} from the curve");
        }
    }

    #[test]
    fn a_curve_that_doubles_back_along_a_line_is_not_taken_for_the_line() {
        // Both handles lie on the chord's line, past its ends: the curve overshoots both ends.
        let back = Cubic { c1: [60.0, 0.0], c2: [-50.0, 0.0], to: [10.0, 0.0] };
        let outline = Outline::lower(&[Segment::Move([0.0, 0.0]), Segment::Cubic(back)]);
        let [line] = &outline.flatten(0.25)[..] else { panic!("one subpath") };
        let reach = line.points.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        assert!(reach > 18.0, "the polyline follows the curve past its end point: {reach}");
        assert!(line.points.len() < 40, "and stops halving once its pieces lie beside their chords");
    }

    #[test]
    fn a_closed_subpath_has_each_point_once_and_what_follows_starts_where_it_began() {
        let program = [
            Segment::Move([0.0, 0.0]),
            Segment::Line([10.0, 0.0]),
            Segment::Line([10.0, 10.0]),
            Segment::Line([0.0, 0.0]),
            Segment::Close,
            Segment::Line([-5.0, -5.0]),
        ];
        let lines = Outline::lower(&program).flatten(0.1);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].points.len(), lines[0].closed), (3, true));
        assert_eq!((lines[1].points.clone(), lines[1].closed), (vec![[0.0, 0.0], [-5.0, -5.0]], false));
    }

    #[test]
    fn corners_are_where_commands_meet_and_never_where_a_curve_is_cut() {
        // Down, round a quarter turn that starts straight down, then on to the right.
        let quarter = Cubic { c1: [0.0, 15.5], c2: [4.5, 20.0], to: [10.0, 20.0] };
        let program = [
            Segment::Move([0.0, 0.0]),
            Segment::Line([0.0, 10.0]),
            Segment::Cubic(quarter),
            Segment::Line([30.0, 20.0]),
        ];
        let [line] = &Outline::lower(&program).flatten(0.01)[..] else { panic!("one subpath") };
        assert!(line.points.len() > 6, "the curve is cut several times");
        assert_eq!(line.corners.len(), line.points.len());
        let corners: Vec<(usize, [Point; 2])> =
            line.corners.iter().enumerate().filter_map(|(i, corner)| Some((i, (*corner)?))).collect();
        assert_eq!(corners.len(), 2, "one where the line meets the curve, one where the curve meets the line");
        assert_eq!(corners[0], (1, [[0.0, 1.0], [0.0, 1.0]]), "the curve leaves the way the line arrives");
        assert_eq!(corners[1], (line.points.len() - 2, [[1.0, 0.0], [1.0, 0.0]]));
    }

    #[test]
    fn a_closed_path_has_a_corner_at_its_start() {
        let program =
            [Segment::Move([0.0, 0.0]), Segment::Line([10.0, 0.0]), Segment::Line([10.0, 10.0]), Segment::Close];
        let [line] = &Outline::lower(&program).flatten(0.1)[..] else { panic!("one subpath") };
        let diagonal = [-10.0 / 200f64.sqrt(); 2];
        assert_eq!(line.corners[0], Some([diagonal, [1.0, 0.0]]), "from the closing line into the first");
        assert_eq!(line.corners[2], Some([[0.0, 1.0], diagonal]));
        assert_eq!(*line, Polyline::straight(&line.points, true), "as a path of straight lines has them");
        let open = Polyline::straight(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], false);
        assert_eq!(open.corners, [None, Some([[1.0, 0.0], [0.0, 1.0]]), None], "an end is no corner");
    }

    #[test]
    fn a_turn_about_a_point_keeps_the_point_and_turns_the_rest_clockwise_on_screen() {
        let turn = Affine::rotate_about(90.0, [10.0, 10.0]);
        let near = |a: Point, b: Point| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-12;
        assert!(near(turn.apply([10.0, 10.0]), [10.0, 10.0]));
        assert!(near(turn.apply([20.0, 10.0]), [10.0, 20.0]), "right of the centre turns to below it");
        assert!((turn.turn() - FRAC_PI_2).abs() < 1e-12);
        let moved = Affine::translate([5.0, 0.0]).then(&turn);
        assert!(near(moved.apply([20.0, 10.0]), [15.0, 20.0]), "the inner map applies first");
        let inside = turn.then(&Affine::translate([10.0, 0.0]));
        assert!(near(inside.apply([10.0, 10.0]), [10.0, 20.0]), "a translation inside a turn is turned");
    }
}
