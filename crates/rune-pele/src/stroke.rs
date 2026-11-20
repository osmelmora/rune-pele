//! Strokes as Rune draws them: SVG's dashes, caps and joins lowered to the two strokes Rune's
//! `Shape` has and to filled patches.
//!
//! Rune strokes a polyline one segment at a time. With round caps each segment is a capsule, so
//! every joint and both ends come out round. With butt caps each segment is a plain box, so a
//! joint is left notched. Neither is SVG's default of butt ends and mitred corners. So a stroke
//! is drawn round, and what SVG adds or takes away at a corner or an end is drawn beside it: the
//! tip of a mitre as a patch, a butt end as a box where a round stroke stops short of the end.
//! A butt end that bends, as a dash round a curve does, is a box to each segment, and the round
//! join a round stroke would have between two of them is a patch too.
//!
//! Every piece of one stroke is one colour, and pieces overlap where they meet. So a translucent
//! stroke is drawn opaque and faded once as a whole. Pieces never lie along one another, though:
//! each is antialiased on its own, and two edges drawn over each other come out bolder than one.

use crate::geometry::{Point, Polyline, unit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Butt,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// With SVG's miter limit of 4: a sharper corner is cut off straight.
    Miter,
    Round,
}

/// How a line is stroked, as SVG says it.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeLook {
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
    /// Dashes and the gaps between them, in turn. Empty for a solid line.
    pub dash: Vec<f64>,
}

/// One stroke as what Rune draws: polylines stroked round, open polylines stroked as boxes, and
/// filled patches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Strokes {
    pub round: Vec<Polyline>,
    pub butt: Vec<Vec<Point>>,
    pub patches: Vec<Vec<Point>>,
}

/// SVG's `stroke-miterlimit` default.
const MITER_LIMIT: f64 = 4.0;

/// The least worth drawing, as a share of a device pixel: a distance this share of a pixel
/// across, or an area this share of a pixel's.
const VISIBLE: f64 = 0.05;

/// The most dashes one stroke is cut into. A pattern finer than that draws solid.
pub(crate) const MOST_DASHES: f64 = 20_000.0;

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn along(a: Point, d: Point, t: f64) -> Point {
    [a[0] + d[0] * t, a[1] + d[1] * t]
}

fn length(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The pattern as SVG repeats it: an odd count runs twice, so dashes and gaps alternate. A
/// pattern with a negative length, or with nothing but zeros, draws a solid line.
fn pattern(dash: &[f64]) -> Option<Vec<f64>> {
    if dash.is_empty() || dash.iter().any(|&d| !d.is_finite() || d < 0.0) || dash.iter().sum::<f64>() <= 0.0 {
        return None;
    }
    Some(if dash.len() % 2 == 1 { [dash, dash].concat() } else { dash.to_vec() })
}

/// Whether the pattern's entry at `index` is a dash: the even ones are, and the odd are gaps.
fn on(index: usize) -> bool {
    index.is_multiple_of(2)
}

/// A point of a dash, and the corner the line has there. Where a dash starts and ends is none.
type Dashed = (Point, Option<[Point; 2]>);

/// The dashes of `line`, each an open polyline that keeps the corners the line has inside it.
/// The pattern starts at the line's first point with a dash. On a closed line, a dash that runs
/// through the start is one piece.
pub fn dashes(line: &Polyline, dash: &[f64]) -> Option<Vec<Polyline>> {
    let pattern = pattern(dash)?;
    let mut ring: Vec<Dashed> = line.points.iter().copied().zip(line.corners.iter().copied()).collect();
    if line.closed {
        ring.push(ring[0]);
    }
    let total: f64 = ring.windows(2).map(|pair| length(pair[0].0, pair[1].0)).sum();
    if total / pattern.iter().sum::<f64>() * (pattern.len() / 2) as f64 > MOST_DASHES {
        return None;
    }
    let mut pieces: Vec<Vec<Dashed>> = Vec::new();
    let (mut index, mut left) = (0, pattern[0]);
    let mut piece = vec![(ring[0].0, None)];
    // A zero-length dash still ends where it starts, so a step may cover no distance.
    let mut flip = |at: Point, piece: &mut Vec<Dashed>, index: &mut usize, left: &mut f64| {
        if on(*index) {
            pieces.push(std::mem::take(piece));
        }
        *index = (*index + 1) % pattern.len();
        *left = pattern[*index];
        if on(*index) {
            piece.push((at, None));
        }
    };
    while left <= 0.0 {
        flip(ring[0].0, &mut piece, &mut index, &mut left);
    }
    for pair in ring.windows(2) {
        let (a, b) = (pair[0].0, pair[1].0);
        let d = length(a, b);
        let mut done = 0.0;
        while d - done > left {
            done += left;
            let at = along(a, sub(b, a), done / d);
            if on(index) {
                piece.push((at, None));
            }
            flip(at, &mut piece, &mut index, &mut left);
            while left <= 0.0 {
                flip(at, &mut piece, &mut index, &mut left);
            }
        }
        left -= d - done;
        if on(index) {
            piece.push(pair[1]);
        }
    }
    let ends_on = on(index);
    if ends_on {
        pieces.push(piece);
    }
    if line.closed && ends_on && pieces.len() > 1 && pieces[0].first().map(|first| first.0) == Some(ring[0].0) {
        let first = pieces.remove(0);
        pieces.last_mut().expect("more than one piece").extend(&first[1..]);
    }
    let pieces = pieces.into_iter().map(|mut piece| {
        piece.dedup_by(|next, kept| next.0 == kept.0);
        // A dash of no length is a dot under a round cap: a stroke too short to see the length of.
        if piece.len() == 1 {
            piece.push(([piece[0].0[0] + 1e-4, piece[0].0[1]], None));
        }
        let last = piece.len() - 1;
        let corners = piece.iter().enumerate().map(|(i, point)| point.1.filter(|_| i > 0 && i < last)).collect();
        Polyline { points: piece.iter().map(|point| point.0).collect(), closed: false, corners }
    });
    Some(pieces.collect())
}

/// How a run of a stroke ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    Round,
    Butt,
}

/// The stretch of an open polyline from `from` to `to` along its length.
fn trim(points: &[Point], from: f64, to: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let mut done = 0.0;
    for pair in points.windows(2) {
        let d = length(pair[0], pair[1]);
        let at = |s: f64| along(pair[0], sub(pair[1], pair[0]), ((s - done) / d).clamp(0.0, 1.0));
        if out.is_empty() && from < done + d {
            out.push(at(from));
        }
        if !out.is_empty() {
            if to <= done + d {
                out.push(at(to));
                break;
            }
            out.push(pair[1]);
        }
        done += d;
    }
    out.dedup();
    out
}

struct Stroker<'a> {
    look: &'a StrokeLook,
    /// Half the stroke's width.
    half: f64,
    /// A device pixel, in the lines' units.
    pixel: f64,
    /// The least distance worth drawing: a mitre that reaches less far past a round join than
    /// this is left round.
    visible: f64,
    out: Strokes,
}

/// What SVG draws at a corner, past the round join Rune draws there.
#[derive(Clone, Copy, PartialEq)]
enum Corner {
    /// Nothing that shows where the stroke is round. Between two boxes, the round join itself.
    Round,
    Miter,
    Bevel,
}

impl Stroker<'_> {
    /// What the join is where the path arrives in `d1` and leaves in `d2`.
    fn corner(&self, [d1, d2]: [Point; 2]) -> Corner {
        if self.look.join == Join::Round {
            return Corner::Round;
        }
        // The cosine of half the turn, from the cosine of the turn.
        let half_turn = ((1.0 + d1[0] * d2[0] + d1[1] * d2[1]) / 2.0).max(0.0).sqrt();
        if half_turn * MITER_LIMIT < 1.0 {
            Corner::Bevel
        } else if self.half * (1.0 / half_turn - 1.0) < self.visible {
            Corner::Round
        } else {
            Corner::Miter
        }
    }

    /// The patch at a corner: from the centre line out to the stroke's outer edges, a little
    /// way back along each, and between them the mitre's tip, the bevel's straight cut or the
    /// round join's arc.
    fn patch(&mut self, before: Point, at: Point, after: Point, [d1, d2]: [Point; 2], corner: Corner) {
        let turn = d1[0] * d2[1] - d1[1] * d2[0];
        // The outer side of the corner, as a factor on each segment's left normal.
        let out = if turn > 0.0 { -self.half } else { self.half };
        let (n1, n2) = ([-d1[1] * out, d1[0] * out], [-d2[1] * out, d2[0] * out]);
        let back = self.half.min(length(before, at));
        let on = self.half.min(length(at, after));
        let (a, b) = (along(at, n1, 1.0), along(at, n2, 1.0));
        // Two boxes meet edge to edge at the point, and where both edges are soft a pixel comes
        // out pale. So a patch between boxes starts on the inner side, inside both, and covers it.
        let apex = match corner {
            Corner::Round => {
                let inner = unit([n1[0] + n2[0], n1[1] + n2[1]], [0.0; 2]).unwrap_or([-d1[0], -d1[1]]);
                // Half a width in, short of where it would leave either box: the way in leans
                // back along one segment and on along the other by the sine of half the turn.
                let lean = length(d1, d2) / 2.0;
                let reach = self.half.min(length(before, at) / lean).min(length(at, after) / lean);
                along(at, inner, reach)
            }
            Corner::Miter | Corner::Bevel => at,
        };
        let mut patch = vec![apex, along(a, d1, -back)];
        match corner {
            Corner::Miter => {
                let k = 1.0 / (1.0 + d1[0] * d2[0] + d1[1] * d2[1]);
                patch.push([at[0] + (n1[0] + n2[0]) * k, at[1] + (n1[1] + n2[1]) * k]);
            }
            Corner::Bevel => patch.extend([a, b]),
            Corner::Round => {
                // Steps short enough that each chord stays within `visible` of the arc.
                let angle = (d1[0] * d2[0] + d1[1] * d2[1]).clamp(-1.0, 1.0).acos();
                let widest = 2.0 * (1.0 - self.visible / self.half).max(-1.0).acos();
                let steps = (angle / widest).ceil().max(1.0) as usize;
                // The normals turn as the path does.
                let (sin, cos) = (angle.copysign(turn) / steps as f64).sin_cos();
                let mut arm = n1;
                patch.push(a);
                for _ in 1..steps {
                    arm = [arm[0] * cos - arm[1] * sin, arm[0] * sin + arm[1] * cos];
                    patch.push(along(at, arm, 1.0));
                }
                patch.push(b);
            }
        }
        patch.push(along(b, d2, on));
        self.out.patches.push(patch);
    }

    /// An open stretch with butt ends, which Rune strokes as a box to each segment. Where two
    /// boxes meet at an angle they leave a notch on the outer side, which shows on a dash round
    /// a curve as a pale spoke at every point the curve was cut at. A patch fills it with the
    /// round join a round stroke has there, where the notch takes enough of a pixel to show: on
    /// a stroke a pixel wide, a patch would darken more than the notch lightens.
    fn butt(&mut self, points: Vec<Point>) {
        for i in 1..points.len().saturating_sub(1) {
            let (before, at, after) = (points[i - 1], points[i], points[i + 1]);
            let (Some(d1), Some(d2)) = (unit(before, at), unit(at, after)) else { continue };
            let turn = (d1[0] * d2[0] + d1[1] * d2[1]).clamp(-1.0, 1.0).acos();
            // The notch is the slice of a disc half the stroke's width across that the turn spans.
            if self.half * self.half * turn / 2.0 >= VISIBLE * self.pixel * self.pixel {
                self.patch(before, at, after, [d1, d2], Corner::Round);
            }
        }
        self.out.butt.push(points);
    }

    /// An open run. A straight one with two butt ends is one box. Any other is round between
    /// its ends, and stops half a width short of a butt end, which a box of that length squares
    /// off: the round stroke's own end reaches no further than the box does.
    fn run(&mut self, points: Vec<Point>, ends: [End; 2]) {
        let round = |points: Vec<Point>| Polyline { corners: vec![None; points.len()], points, closed: false };
        if ends == [End::Round; 2] {
            self.out.round.push(round(points));
            return;
        }
        let total: f64 = points.windows(2).map(|pair| length(pair[0], pair[1])).sum();
        let from = if ends[0] == End::Butt { self.half } else { 0.0 };
        let to = total - if ends[1] == End::Butt { self.half } else { 0.0 };
        if (points.len() == 2 && ends == [End::Butt; 2]) || to <= from {
            self.butt(points);
            return;
        }
        self.out.round.push(round(trim(&points, from, to)));
        if ends[0] == End::Butt {
            self.butt(trim(&points, 0.0, from));
        }
        if ends[1] == End::Butt {
            self.butt(trim(&points, to, total));
        }
    }

    /// One subpath, or one dash of one: its corners, then its runs between the corners SVG cuts
    /// off straight.
    fn line(&mut self, line: &Polyline, cap: End) {
        let (points, closed) = (&line.points, line.closed);
        let n = points.len();
        if n < 2 {
            return;
        }
        let corners: Vec<(usize, [Point; 2], Corner)> = (0..n)
            .filter(|&i| (closed && n > 2) || (i > 0 && i + 1 < n))
            .filter_map(|i| Some((i, line.corners[i]?)))
            .map(|(i, directions)| (i, directions, self.corner(directions)))
            .filter(|&(_, _, corner)| corner != Corner::Round)
            .collect();
        for &(i, directions, corner) in &corners {
            self.patch(points[(i + n - 1) % n], points[i], points[(i + 1) % n], directions, corner);
        }
        let cuts: Vec<usize> = corners.iter().filter(|(_, _, c)| *c == Corner::Bevel).map(|&(i, _, _)| i).collect();
        if cuts.is_empty() {
            if closed && n > 2 {
                self.out.round.push(Polyline { corners: vec![None; n], ..line.clone() });
            } else {
                self.run(points.clone(), [cap; 2]);
            }
            return;
        }
        if closed {
            // From each cut to the next, round the ring.
            for (k, &start) in cuts.iter().enumerate() {
                let end = cuts[(k + 1) % cuts.len()];
                let count = if end > start { end - start } else { end + n - start };
                let run = (0..=count).map(|step| points[(start + step) % n]).collect();
                self.run(run, [End::Butt; 2]);
            }
        } else {
            let stops: Vec<usize> = [0].into_iter().chain(cuts).chain([n - 1]).collect();
            for (k, pair) in stops.windows(2).enumerate() {
                let start = if k == 0 { cap } else { End::Butt };
                let end = if k + 2 == stops.len() { cap } else { End::Butt };
                self.run(points[pair[0]..=pair[1]].to_vec(), [start, end]);
            }
        }
    }
}

/// The stroke of `lines` in `look`, for a window where a device pixel is `pixel` in the lines'
/// units. What would change less than a twentieth of a pixel is left out.
pub fn stroke(lines: &[Polyline], look: &StrokeLook, pixel: f64) -> Strokes {
    let (half, visible) = (look.width / 2.0, VISIBLE * pixel);
    let mut stroker = Stroker { look, half, pixel, visible, out: Strokes::default() };
    if look.width.is_nan() || look.width <= 0.0 {
        return stroker.out;
    }
    let cap = if look.cap == Cap::Round { End::Round } else { End::Butt };
    for line in lines {
        match dashes(line, &look.dash) {
            Some(pieces) => {
                for piece in pieces {
                    stroker.line(&piece, cap);
                }
            }
            // A subpath of one point draws nothing, as in a browser.
            None => stroker.line(line, cap),
        }
    }
    stroker.out
}

impl Strokes {
    /// Whether two of the pieces can overlap, so that a translucent colour would show twice.
    pub(crate) fn overlaps(&self) -> bool {
        let segments = |points: usize, closed: bool| if closed { points } else { points - 1 };
        let round: usize = self.round.iter().map(|line| segments(line.points.len(), line.closed)).sum();
        let butt: usize = self.butt.iter().map(|points| segments(points.len(), false)).sum();
        round + butt + self.patches.len() > 1
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.round.is_empty() && self.butt.is_empty() && self.patches.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(points: &[Point]) -> Polyline {
        Polyline::straight(points, false)
    }

    fn closed(points: &[Point]) -> Polyline {
        Polyline::straight(points, true)
    }

    /// What the stroker draws round: the points, with no corner among them.
    fn smooth(points: &[Point], closed: bool) -> Polyline {
        Polyline { points: points.to_vec(), closed, corners: vec![None; points.len()] }
    }

    fn look(width: f64, cap: Cap, join: Join) -> StrokeLook {
        StrokeLook { width, cap, join, dash: Vec::new() }
    }

    fn spans(pieces: &[Polyline]) -> Vec<(f64, f64)> {
        pieces.iter().map(|piece| (piece.points[0][0], piece.points.last().unwrap()[0])).collect()
    }

    const SQUARE: [Point; 4] = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];

    #[test]
    fn dashes_start_with_a_dash_and_keep_the_pattern_across_points() {
        let line = open(&[[0.0, 0.0], [7.0, 0.0], [20.0, 0.0]]);
        let pieces = dashes(&line, &[6.0, 4.0]).expect("a pattern");
        assert_eq!(spans(&pieces), [(0.0, 6.0), (10.0, 16.0)]);
        assert_eq!(pieces[0].points, [[0.0, 0.0], [6.0, 0.0]]);
        assert_eq!(
            pieces[1].points,
            [[10.0, 0.0], [16.0, 0.0]],
            "the point at 7 lies in a gap, and the pattern runs on past it"
        );
    }

    #[test]
    fn a_dash_follows_the_line_round_a_corner() {
        let line = open(&[[0.0, 0.0], [5.0, 0.0], [5.0, 20.0]]);
        let pieces = dashes(&line, &[8.0, 2.0]).expect("a pattern");
        assert_eq!(pieces[0].points, [[0.0, 0.0], [5.0, 0.0], [5.0, 3.0]]);
        assert_eq!(pieces[0].corners, [None, Some([[1.0, 0.0], [0.0, 1.0]]), None], "the dash keeps the line's corner");
        assert_eq!(pieces[1].points, [[5.0, 5.0], [5.0, 13.0]]);
        assert_eq!(pieces[2].points, [[5.0, 15.0], [5.0, 20.0]], "the last dash stops where the line does");
    }

    #[test]
    fn an_odd_pattern_repeats_so_dashes_and_gaps_alternate() {
        let line = open(&[[0.0, 0.0], [30.0, 0.0]]);
        assert_eq!(spans(&dashes(&line, &[4.0]).unwrap()), [(0.0, 4.0), (8.0, 12.0), (16.0, 20.0), (24.0, 28.0)]);
        assert_eq!(spans(&dashes(&line, &[5.0, 2.0, 1.0]).unwrap())[..3], [(0.0, 5.0), (7.0, 8.0), (13.0, 15.0)]);
    }

    #[test]
    fn a_dash_through_the_start_of_a_closed_line_is_one_piece() {
        let apart = dashes(&closed(&SQUARE), &[6.0, 4.0]).expect("a pattern");
        assert_eq!(apart.len(), 4, "the line ends in a gap, so the first dash stands alone");
        assert_eq!(apart[0].points, [[0.0, 0.0], [6.0, 0.0]]);
        assert_eq!(apart[3].points, [[0.0, 10.0], [0.0, 4.0]]);
        let through = dashes(&closed(&SQUARE), &[8.0, 3.0]).expect("a pattern");
        assert_eq!(through.len(), 3);
        assert_eq!(through[0].points, [[10.0, 1.0], [10.0, 9.0]]);
        assert_eq!(through[2].points, [[0.0, 7.0], [0.0, 0.0], [8.0, 0.0]], "the last dash runs on into the first");
        assert!(through[2].corners[1].is_some(), "round the corner the line has at its start");
    }

    #[test]
    fn a_pattern_that_cannot_dash_draws_solid() {
        let line = open(&[[0.0, 0.0], [30.0, 0.0]]);
        assert_eq!(dashes(&line, &[]), None);
        assert_eq!(dashes(&line, &[0.0, 0.0]), None);
        assert_eq!(dashes(&line, &[4.0, -1.0]), None);
        assert_eq!(dashes(&line, &[0.0001]), None, "more dashes than are worth drawing");
        let dots = dashes(&line, &[0.0, 10.0]).expect("dots");
        assert_eq!(dots.len(), 3, "at 0, 10 and 20");
        assert!(dots.iter().all(|dot| dot.points.len() == 2 && length(dot.points[0], dot.points[1]) < 1e-3));
    }

    #[test]
    fn a_round_stroke_is_one_round_polyline() {
        let strokes = stroke(&[open(&SQUARE)], &look(2.0, Cap::Round, Join::Round), 1.0);
        assert_eq!(strokes, Strokes { round: vec![smooth(&SQUARE, false)], ..Strokes::default() });
        assert!(strokes.overlaps(), "its segments meet at joints");
    }

    #[test]
    fn a_straight_line_with_butt_ends_is_one_box() {
        let line = [[0.0, 0.0], [10.0, 0.0]];
        let strokes = stroke(&[open(&line)], &look(2.0, Cap::Butt, Join::Miter), 1.0);
        assert_eq!(strokes, Strokes { butt: vec![line.to_vec()], ..Strokes::default() });
        assert!(!strokes.overlaps());
    }

    #[test]
    fn a_mitred_square_is_round_with_a_tip_at_each_corner() {
        let strokes = stroke(&[closed(&SQUARE)], &look(2.0, Cap::Butt, Join::Miter), 1.0);
        assert_eq!(strokes.round, [smooth(&SQUARE, true)]);
        assert!(strokes.butt.is_empty());
        assert_eq!(strokes.patches.len(), 4);
        let tips: Vec<Point> = strokes.patches.iter().map(|patch| patch[2]).collect();
        for tip in [[-1.0, -1.0], [11.0, -1.0], [11.0, 11.0], [-1.0, 11.0]] {
            assert!(tips.iter().any(|t| length(*t, tip) < 1e-12), "a tip at {tip:?}: {tips:?}");
        }
        let corner = strokes.patches.iter().find(|patch| length(patch[2], [11.0, -1.0]) < 1e-12).unwrap();
        assert_eq!(corner[0], [10.0, 0.0], "a patch starts on the centre line");
        assert_eq!((corner[1], corner[3]), ([9.0, -1.0], [11.0, 1.0]), "and runs back along both outer edges");
    }

    #[test]
    fn a_gentle_bend_stays_round_and_a_needle_is_cut_off_straight() {
        let bend = [[0.0, 0.0], [10.0, 0.0], [20.0, 0.5]];
        let strokes = stroke(&[open(&bend)], &look(1.0, Cap::Round, Join::Miter), 1.0);
        assert_eq!(strokes, Strokes { round: vec![smooth(&bend, false)], ..Strokes::default() });

        // 20 degrees between the two legs: past the miter limit of 4.
        let tip = [10.0, 0.0];
        let back = [10.0 - 10.0 * 20f64.to_radians().cos(), 10.0 * 20f64.to_radians().sin()];
        let needle = open(&[[0.0, 0.0], tip, back]);
        let strokes = stroke(&[needle], &look(2.0, Cap::Round, Join::Miter), 1.0);
        let [patch] = &strokes.patches[..] else { panic!("one bevel: {strokes:?}") };
        assert_eq!(patch.len(), 5, "a centre, an edge, the cut's two ends, an edge");
        let cut = [patch[2], patch[3]];
        assert!(cut.iter().all(|&p| (length(p, tip) - 1.0).abs() < 1e-12), "the cut joins the two outer corners");
        assert_eq!(strokes.round.len(), 2, "each leg is round up to half a width short of the cut");
        assert_eq!(strokes.round[0].points, [[0.0, 0.0], [9.0, 0.0]]);
        assert_eq!(strokes.butt.len(), 2, "and ends in a box there");
        assert_eq!(strokes.butt[0], [[9.0, 0.0], [10.0, 0.0]]);
    }

    #[test]
    fn butt_ends_on_a_bent_line_are_boxes_at_the_ends_of_a_shorter_round_stroke() {
        let line = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        let strokes = stroke(&[open(&line)], &look(2.0, Cap::Butt, Join::Round), 1.0);
        assert_eq!(strokes.round, [smooth(&[[1.0, 0.0], [10.0, 0.0], [10.0, 9.0]], false)]);
        assert_eq!(strokes.butt, [vec![[0.0, 0.0], [1.0, 0.0]], vec![[10.0, 9.0], [10.0, 10.0]]]);
        assert!(strokes.patches.is_empty());
        let short = [[0.0, 0.0], [1.0, 0.0], [1.0, 0.5]];
        let strokes = stroke(&[open(&short)], &look(2.0, Cap::Butt, Join::Round), 1.0);
        assert_eq!((strokes.butt, strokes.round), (vec![short.to_vec()], vec![]), "too short for a round middle");
    }

    #[test]
    fn a_point_where_a_curve_was_cut_is_no_corner_however_sharp_the_chords_meet() {
        let cut = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        let strokes = stroke(&[smooth(&cut, false)], &look(4.0, Cap::Round, Join::Miter), 1.0);
        assert_eq!(strokes, Strokes { round: vec![smooth(&cut, false)], ..Strokes::default() });
        // A line that runs into a curve the way the curve starts: the corner is by the tangents.
        let mut smooth_join = open(&cut);
        smooth_join.corners[1] = Some([[1.0, 0.0], [1.0, 0.0]]);
        let strokes = stroke(&[smooth_join], &look(4.0, Cap::Round, Join::Miter), 1.0);
        assert!(strokes.patches.is_empty(), "{strokes:?}");
    }

    #[test]
    fn dashes_take_the_strokes_caps() {
        let line = open(&[[0.0, 0.0], [20.0, 0.0]]);
        let dashed = |cap| StrokeLook { dash: vec![6.0, 4.0], ..look(1.0, cap, Join::Miter) };
        let butt = stroke(std::slice::from_ref(&line), &dashed(Cap::Butt), 1.0);
        assert_eq!(butt.butt, [vec![[0.0, 0.0], [6.0, 0.0]], vec![[10.0, 0.0], [16.0, 0.0]]]);
        assert!(butt.round.is_empty());
        let round = stroke(&[line], &dashed(Cap::Round), 1.0);
        assert_eq!(round.round.len(), 2);
        assert!(round.butt.is_empty());
    }

    #[test]
    fn no_width_draws_nothing() {
        assert!(stroke(&[open(&SQUARE)], &look(0.0, Cap::Round, Join::Round), 1.0).is_empty());
    }
}
