//! Fills as Rune draws them: the area SVG's nonzero rule fills, as simple polygons.
//!
//! Rune fills one simple polygon at a time and knows no holes. Contours that lie apart are
//! filled one by one. A hole is joined to the contour around it by a slit too narrow to see,
//! which leaves one simple polygon, and a sliver of fill is laid over the slit so that its two
//! antialiased edges do not show as a seam. A contour that crosses itself, as an hourglass does,
//! is cut at its crossings into loops that do not. Two contours that cross each other are
//! refused: what they fill depends on where they cross, which nothing here works out.
//!
//! No polygon keeps a corner that lies in a line with the corners either side of it. Rune cuts
//! a polygon into triangles, and three corners in a line can come out as a triangle with no
//! area, which it paints as half the box around the three.

use crate::geometry::{Point, Polyline};

/// The area of one fill as polygons to fill one by one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fills {
    pub polygons: Vec<Vec<Point>>,
    /// Whether polygons overlap, so that a translucent colour would show twice.
    pub overlaps: bool,
}

/// An outline whose contours cross each other, or one that crosses itself too often to untangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crossing;

/// The most loops one contour is cut into where it crosses itself.
const MOST_LOOPS: usize = 64;

fn signed_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i][0] * ring[(i + 1) % n][1] - ring[(i + 1) % n][0] * ring[i][1]).sum::<f64>() / 2.0
}

fn cross(o: Point, a: Point, b: Point) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Whether segments `a` and `b` cross at a point inside both. Touching does not count.
fn crosses(a: [Point; 2], b: [Point; 2]) -> bool {
    let (d1, d2) = (cross(b[0], b[1], a[0]), cross(b[0], b[1], a[1]));
    let (d3, d4) = (cross(a[0], a[1], b[0]), cross(a[0], a[1], b[1]));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

fn edges(ring: &[Point]) -> impl Iterator<Item = [Point; 2]> + '_ {
    (0..ring.len()).map(|i| [ring[i], ring[(i + 1) % ring.len()]])
}

/// Whether the ring only ever turns one way, and once round: then it cannot cross itself.
fn convex(ring: &[Point]) -> bool {
    let n = ring.len();
    let mut sign = 0.0;
    let mut turned = 0.0;
    for i in 0..n {
        let (a, b, c) = (ring[i], ring[(i + 1) % n], ring[(i + 2) % n]);
        let turn = cross(a, b, c);
        if turn * sign < 0.0 {
            return false;
        }
        if turn != 0.0 {
            sign = turn;
        }
        let (d1, d2) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        turned += turn.atan2(d1[0] * d2[0] + d1[1] * d2[1]);
    }
    turned.abs() < 7.0
}

/// Cuts `ring` where it first crosses itself into the loop that runs on past the crossing and
/// the loop it leaves behind, and those again, until none crosses itself. The loops meet at the
/// crossings and nowhere else.
fn untangle(ring: Vec<Point>, loops: &mut Vec<Vec<Point>>) -> Result<(), Crossing> {
    if loops.len() >= MOST_LOOPS {
        return Err(Crossing);
    }
    let n = ring.len();
    let edge = |i: usize| [ring[i], ring[(i + 1) % n]];
    let crossing = (0..n).find_map(|i| (i + 2..n).find(|&j| crosses(edge(i), edge(j))).map(|j| (i, j)));
    let Some((i, j)) = crossing.filter(|_| !convex(&ring)) else {
        loops.push(ring);
        return Ok(());
    };
    let ([a, b], [c, d]) = (edge(i), edge(j));
    let along = cross(c, d, a) / (cross(c, d, a) - cross(c, d, b));
    let at = [a[0] + (b[0] - a[0]) * along, a[1] + (b[1] - a[1]) * along];
    let past: Vec<Point> = ring[..=i].iter().copied().chain([at]).chain(ring[j + 1..].iter().copied()).collect();
    let behind: Vec<Point> = [at].into_iter().chain(ring[i + 1..=j].iter().copied()).collect();
    untangle(past, loops)?;
    untangle(behind, loops)
}

/// How far a point may lie from a line and count as on it, for a sliver `gap` wide either side
/// of its slit: far more than the rounding of a point in Rune's 32 bits, and far less than
/// anything seen.
fn hair(gap: f64) -> f64 {
    gap / 100.0
}

/// `ring` without the corners that lie within `hair` of the line through the corners either
/// side of them: those along a straight side, and those in the middle of an S curve, which is
/// its own mirror image there. A run of such corners goes only while every one of them stays
/// within `hair` of the line that replaces them, so a gentle curve keeps its shape.
pub(crate) fn without_flat_corners(ring: &[Point], hair: f64) -> Vec<Point> {
    let n = ring.len();
    let off_line = |p: Point, a: Point, b: Point| {
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length == 0.0 { f64::INFINITY } else { cross(a, b, p).abs() / length }
    };
    // The walk starts from the sharpest corner, which stays.
    let turn = |i: usize| off_line(ring[i], ring[(i + n - 1) % n], ring[(i + 1) % n]);
    let Some(start) = (0..n).max_by(|&a, &b| turn(a).total_cmp(&turn(b))) else { return Vec::new() };
    let mut kept = vec![start];
    let mut left_out: Vec<usize> = Vec::new();
    for i in (1..n).map(|step| (start + step) % n) {
        let (before, after) = (ring[kept[kept.len() - 1]], ring[(i + 1) % n]);
        if left_out.iter().chain([&i]).all(|&corner| off_line(ring[corner], before, after) <= hair) {
            left_out.push(i);
        } else {
            kept.push(i);
            left_out.clear();
        }
    }
    kept.sort_unstable();
    kept.into_iter().map(|i| ring[i]).collect()
}

/// A point of `ring` that no loop cut from the same contour shares: the middle of its longest
/// edge. The loops meet only at their corners.
fn own_point(ring: &[Point]) -> Point {
    let longest = edges(ring)
        .max_by(|[a, b], [c, d]| (b[0] - a[0]).hypot(b[1] - a[1]).total_cmp(&(d[0] - c[0]).hypot(d[1] - c[1])));
    let [a, b] = longest.expect("a ring has edges");
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// Whether `p` is inside `ring`, by the crossings of a ray to its right.
fn inside(p: Point, ring: &[Point]) -> bool {
    edges(ring)
        .filter(|[a, b]| (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
        .count()
        % 2
        == 1
}

/// Where a ray from `from` to the right first crosses `ring`: the index of the point of `ring`
/// the crossing comes after, and the crossing. Nothing, where a corner of `ring` touches the
/// ray before that: both of the corner's edges meet the ray there, and a slit along the ray
/// would run through the corner.
fn first_right(ring: &[Point], from: Point) -> Option<(usize, Point)> {
    let mut hits: Vec<(f64, usize)> = edges(ring)
        .enumerate()
        .filter(|(_, [a, b])| (a[1] > from[1]) != (b[1] > from[1]))
        .map(|(i, [a, b])| (a[0] + (from[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]), i))
        .filter(|&(x, _)| x >= from[0])
        .collect();
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let &(x, edge) = hits.first()?;
    if hits.get(1).is_some_and(|next| next.0 == x) {
        return None;
    }
    let to = [x, from[1]];
    // A crossing at the end of its edge comes after that end.
    let after = if to == ring[(edge + 1) % ring.len()] { (edge + 1) % ring.len() } else { edge };
    Some((after, to))
}

/// `outer` with `hole` joined to it by a slit, and the sliver that covers the slit. `outer`
/// winds one way and `hole` the other.
///
/// The slit runs level from the hole's rightmost point to the nearest edge of `outer`, so
/// nothing lies across it. Its way back is a hair to one side: the side `outer` runs on to
/// from there, which is the side the hole's own edge comes in from. A slit that would run
/// along an edge or through a corner is not made.
fn join(outer: &[Point], hole: &[Point], gap: f64) -> Option<(Vec<Point>, Vec<Point>)> {
    let n = hole.len();
    let rightmost = (0..n).max_by(|&a, &b| hole[a][0].total_cmp(&hole[b][0]))?;
    // A hole whose right side stands upright is left from the middle of that side, clear of
    // corners that so often lie level with its own.
    let mut hole = hole.to_vec();
    let upright = [rightmost, (rightmost + n - 1) % n].into_iter().find(|&i| hole[i][0] == hole[(i + 1) % n][0]);
    let rightmost = match upright {
        Some(i) => {
            let (a, b) = (hole[i], hole[(i + 1) % n]);
            hole.insert(i + 1, [a[0], (a[1] + b[1]) / 2.0]);
            i + 1
        }
        None => rightmost,
    };
    let from = hole[rightmost];
    let (after, to) = first_right(outer, from)?;
    let onward = outer[(after + 1) % outer.len()];
    if onward[1] == to[1] {
        return None;
    }
    let hair = hair(gap);
    let side = if onward[1] > to[1] { hair } else { -hair };
    let share = (hair / (onward[1] - to[1]).abs()).min(0.5);
    let back = [to[0] + (onward[0] - to[0]) * share, to[1] + (onward[1] - to[1]) * share];
    let mut ring = outer[..=after].to_vec();
    ring.push(to);
    ring.extend((0..hole.len()).map(|step| hole[(rightmost + step) % hole.len()]));
    ring.extend([[from[0], from[1] + side], back]);
    ring.extend(&outer[after + 1..]);
    ring.dedup();
    let inset = gap.min((to[0] - from[0]) / 2.0);
    let (near, far) = (from[0] + inset, to[0] - inset);
    let (up, down) = (from[1] - gap, from[1] + gap);
    let sliver = vec![from, [near, up], [far, up], to, [far, down], [near, down]];
    Some((ring, sliver))
}

/// The polygons that cover what SVG's nonzero rule fills of `lines`. A subpath is closed as a
/// fill closes it. `gap` is half the width of the sliver laid over a slit: about a device pixel.
pub fn fill(lines: &[Polyline], gap: f64) -> Result<Fills, Crossing> {
    let mut loops: Vec<Vec<Point>> = Vec::new();
    for line in lines.iter().filter(|line| line.points.len() >= 3) {
        untangle(line.points.clone(), &mut loops)?;
    }
    let loops: Vec<Vec<Point>> = loops.iter().map(|ring| without_flat_corners(ring, hair(gap))).collect();
    let rings: Vec<&[Point]> = loops.iter().map(Vec::as_slice).filter(|ring| signed_area(ring) != 0.0).collect();
    if rings.len() <= 1 {
        return Ok(Fills { polygons: rings.iter().map(|ring| ring.to_vec()).collect(), overlaps: false });
    }
    for (i, a) in rings.iter().enumerate() {
        for b in &rings[i + 1..] {
            if edges(a).any(|ea| edges(b).any(|eb| crosses(ea, eb))) {
                return Err(Crossing);
            }
        }
    }
    let turns: Vec<i32> = rings.iter().map(|ring| if signed_area(ring) > 0.0 { 1 } else { -1 }).collect();
    // The contours around each one. They do not cross, so one point inside is all inside.
    let around: Vec<Vec<usize>> = (0..rings.len())
        .map(|i| (0..rings.len()).filter(|&j| j != i && inside(own_point(rings[i]), rings[j])).collect())
        .collect();
    let winding_around = |i: usize| around[i].iter().map(|&j| turns[j]).sum::<i32>();
    let filled_around = |i: usize| winding_around(i) != 0;
    let filled_inside = |i: usize| winding_around(i) + turns[i] != 0;
    let mut fills = Fills::default();
    for (i, ring) in rings.iter().enumerate() {
        if !filled_inside(i) || filled_around(i) {
            continue;
        }
        // The holes of this contour: unfilled inside, and no other contour that starts a fill
        // lies between.
        let mut holes: Vec<usize> = (0..rings.len())
            .filter(|&h| around[h].contains(&i) && !filled_inside(h) && filled_around(h))
            .filter(|&h| !around[h].iter().any(|&j| j != i && around[j].contains(&i) && !filled_around(j)))
            .collect();
        let right = |h: usize| rings[h].iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        holes.sort_by(|&a, &b| right(b).total_cmp(&right(a)));
        let mut polygon = ring.to_vec();
        let mut slivers = Vec::new();
        for hole in holes {
            let mut hole = rings[hole].to_vec();
            if (signed_area(&hole) > 0.0) == (signed_area(ring) > 0.0) {
                hole.reverse();
            }
            let (joined, sliver) = join(&polygon, &hole, gap).ok_or(Crossing)?;
            polygon = joined;
            slivers.push(sliver);
            fills.overlaps = true;
        }
        fills.polygons.push(polygon);
        fills.polygons.extend(slivers);
    }
    Ok(fills)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crosses_itself(ring: &[Point]) -> bool {
        if convex(ring) {
            return false;
        }
        let all: Vec<[Point; 2]> = edges(ring).collect();
        all.iter().enumerate().any(|(i, a)| all[i + 1..].iter().any(|b| crosses(*a, *b)))
    }

    fn ring(points: &[Point]) -> Polyline {
        Polyline::straight(points, true)
    }

    fn square(x: f64, y: f64, side: f64) -> Vec<Point> {
        vec![[x, y], [x + side, y], [x + side, y + side], [x, y + side]]
    }

    fn reversed(mut points: Vec<Point>) -> Vec<Point> {
        points.reverse();
        points
    }

    #[test]
    fn one_contour_is_filled_as_it_is_and_an_open_one_is_closed() {
        let l = [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [10.0, 10.0], [10.0, 30.0], [0.0, 30.0]];
        let open = Polyline::straight(&l, false);
        assert_eq!(fill(&[open], 0.5), Ok(Fills { polygons: vec![l.to_vec()], overlaps: false }));
    }

    #[test]
    fn a_line_and_a_point_fill_nothing() {
        let line = Polyline::straight(&[[0.0, 0.0], [5.0, 5.0]], false);
        let flat = ring(&[[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]]);
        assert_eq!(fill(&[line, flat], 0.5), Ok(Fills::default()));
    }

    #[test]
    fn a_corner_in_a_line_with_those_either_side_of_it_is_left_out() {
        // A square with a point along its top, and one a thousandth off its right side.
        let ring_of = |points: &[Point]| fill(&[ring(points)], 0.5).expect("no crossing").polygons;
        let dotted = [[0.0, 0.0], [4.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert_eq!(ring_of(&dotted), [square(0.0, 0.0, 10.0)]);
        let nearly = [[0.0, 0.0], [10.0, 0.0], [10.001, 5.0], [10.0, 10.0], [0.0, 10.0]];
        assert_eq!(ring_of(&nearly), [square(0.0, 0.0, 10.0)]);
        // The first and the last point are corners between the others as any is.
        let ends = [[5.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0], [2.0, 0.0]];
        assert_eq!(ring_of(&ends), [vec![[10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]]);
        // A corner that turns by more than a hair stays: a hundredth of the gap is 0.005.
        let bent = [[0.0, 0.0], [10.0, 0.0], [10.01, 5.0], [10.0, 10.0], [0.0, 10.0]];
        assert_eq!(ring_of(&bent), [bent.to_vec()]);
    }

    #[test]
    fn a_gentle_curve_keeps_its_shape_though_each_of_its_corners_is_nearly_flat() {
        // An arc that rises 0.4 over 40, cut so fine that each corner is a thousandth off the
        // line through the two beside it.
        let arc = (0..=40).map(f64::from).map(|x| [x, 0.001 * (x - 20.0) * (x - 20.0)]);
        let dish: Vec<Point> = arc.chain([[40.0, -10.0], [0.0, -10.0]]).collect();
        let [thinned] = &fill(&[ring(&dish)], 0.5).expect("no crossing").polygons[..] else { panic!("one polygon") };
        assert!(thinned.len() < dish.len() / 2, "most corners go: {} of {}", thinned.len(), dish.len());
        let from_edge = |p: Point, [a, b]: [Point; 2]| {
            let (along, length) = ([b[0] - a[0], b[1] - a[1]], (b[0] - a[0]).hypot(b[1] - a[1]));
            let t = (((p[0] - a[0]) * along[0] + (p[1] - a[1]) * along[1]) / (length * length)).clamp(0.0, 1.0);
            (p[0] - a[0] - t * along[0]).hypot(p[1] - a[1] - t * along[1])
        };
        for corner in &dish {
            let nearest = edges(thinned).map(|edge| from_edge(*corner, edge)).fold(f64::MAX, f64::min);
            assert!(nearest <= 0.005 + 1e-9, "the outline stays within a hair of {corner:?}: {nearest}");
        }
    }

    #[test]
    fn contours_apart_are_filled_one_by_one() {
        let (a, b) = (square(0.0, 0.0, 10.0), reversed(square(20.0, 0.0, 10.0)));
        let fills = fill(&[ring(&a), ring(&b)], 0.5).expect("no crossing");
        assert_eq!(fills, Fills { polygons: vec![a, b], overlaps: false });
    }

    #[test]
    fn a_hole_is_joined_to_its_contour_by_a_slit_under_a_sliver() {
        let (outer, hole) = (square(0.0, 0.0, 30.0), reversed(square(10.0, 10.0, 10.0)));
        let fills = fill(&[ring(&outer), ring(&hole)], 0.5).expect("no crossing");
        let [polygon, sliver] = &fills.polygons[..] else { panic!("a polygon and a sliver: {fills:?}") };
        assert!(fills.overlaps, "the sliver lies over the polygon");
        assert_eq!(polygon.len(), 4 + 5 + 3, "both contours, and the slit's way out and back");
        assert!((signed_area(polygon).abs() - 800.0).abs() < 0.2, "30 by 30 less 10 by 10: {}", signed_area(polygon));
        assert!(!crosses_itself(polygon), "the joined polygon is simple");
        assert!(!inside([15.0, 15.0], polygon), "the hole is outside");
        assert!(inside([5.0, 15.0], polygon) && inside([25.0, 5.0], polygon));
        assert_eq!(
            (sliver[0], sliver[3]),
            ([20.0, 15.0], [30.0, 15.0]),
            "the sliver runs from the hole to the contour"
        );
        let widest = sliver.iter().map(|p| (p[1] - 15.0).abs()).fold(0.0, f64::max);
        assert_eq!(widest, 0.5, "and is a gap wide each side of the slit");
    }

    #[test]
    fn a_contour_the_same_way_round_inside_another_adds_nothing() {
        let (outer, inner) = (square(0.0, 0.0, 30.0), square(10.0, 10.0, 10.0));
        let fills = fill(&[ring(&outer), ring(&inner)], 0.5).expect("no crossing");
        assert_eq!(fills, Fills { polygons: vec![outer], overlaps: false });
    }

    #[test]
    fn an_island_in_a_hole_is_filled_again() {
        let outer = square(0.0, 0.0, 50.0);
        let hole = reversed(square(10.0, 10.0, 30.0));
        let island = square(20.0, 20.0, 10.0);
        let fills = fill(&[ring(&outer), ring(&hole), ring(&island)], 0.5).expect("no crossing");
        let areas: Vec<f64> = fills.polygons.iter().map(|polygon| signed_area(polygon).abs()).collect();
        assert_eq!(fills.polygons.len(), 3, "the ring, its sliver and the island: {areas:?}");
        assert!(fills.overlaps);
        assert!((areas[0] - 1600.0).abs() < 0.5, "50 by 50 less 30 by 30: {areas:?}");
        assert!(fills.polygons.contains(&island));
    }

    #[test]
    fn two_holes_are_each_joined() {
        let outer = square(0.0, 0.0, 60.0);
        let (a, b) = (reversed(square(10.0, 10.0, 10.0)), reversed(square(35.0, 8.0, 10.0)));
        let fills = fill(&[ring(&outer), ring(&a), ring(&b)], 0.5).expect("no crossing");
        assert_eq!(fills.polygons.len(), 3, "the polygon and two slivers");
        let polygon = &fills.polygons[0];
        assert!((signed_area(polygon).abs() - 3400.0).abs() < 0.5);
        assert!(!crosses_itself(polygon));
        assert!(!inside([15.0, 15.0], polygon) && !inside([40.0, 10.0], polygon));
        assert!(inside([30.0, 12.0], polygon), "between the holes, clear of the slit");
        assert_eq!(fills.polygons[1][3], [60.0, 13.0], "the hole further right is joined to the contour");
        assert_eq!(fills.polygons[2][3], [35.0, 15.0], "and the other to that hole");
    }

    #[test]
    fn a_hole_is_joined_to_whatever_edge_is_nearest_to_its_right() {
        // A spike reaches up from the floor between the hole and the far wall.
        let outer =
            [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [65.0, 100.0], [60.0, 50.0], [55.0, 100.0], [0.0, 100.0]];
        let hole = reversed(square(10.0, 55.0, 10.0));
        let fills = fill(&[ring(&outer), ring(&hole)], 0.5).expect("no crossing");
        let [polygon, sliver] = &fills.polygons[..] else { panic!("a polygon and a sliver: {fills:?}") };
        assert_eq!(sliver[3], [59.0, 60.0], "the slit ends on the spike's near side");
        assert!(!crosses_itself(polygon), "{polygon:?}");
        assert!((signed_area(polygon).abs() - (signed_area(&outer).abs() - 100.0)).abs() < 0.5);
    }

    #[test]
    fn a_slit_that_would_run_through_a_corner_is_refused() {
        let outer =
            [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [65.0, 100.0], [60.0, 50.0], [55.0, 100.0], [0.0, 100.0]];
        // A diamond whose rightmost corner is level with the spike's tip.
        let hole = [[15.0, 45.0], [10.0, 50.0], [15.0, 55.0], [20.0, 50.0]];
        assert_eq!(fill(&[ring(&outer), ring(&hole)], 0.5), Err(Crossing));
    }

    #[test]
    fn a_contour_that_crosses_itself_is_filled_loop_by_loop() {
        // An hourglass: across the top, down to the far corner, across the bottom and back.
        let hourglass = ring(&[[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]]);
        let fills = fill(&[hourglass], 0.5).expect("two loops");
        assert_eq!(
            fills.polygons,
            [vec![[0.0, 0.0], [10.0, 0.0], [5.0, 5.0]], vec![[5.0, 5.0], [0.0, 10.0], [10.0, 10.0]]]
        );
        assert!(!fills.overlaps, "the loops meet at the crossing only");
        // A curl: round an L, then round a square inside it the same way. The square is inside
        // twice over, which the nonzero rule fills all the same, so the L is all there is.
        let curl = ring(&[
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 30.0],
            [10.0, 30.0],
            [10.0, 10.0],
            [20.0, 10.0],
            [20.0, 20.0],
            [0.0, 20.0],
        ]);
        let fills = fill(&[curl], 0.5).expect("loops");
        let l = vec![[0.0, 0.0], [30.0, 0.0], [30.0, 30.0], [10.0, 30.0], [10.0, 20.0], [0.0, 20.0]];
        assert_eq!(fills, Fills { polygons: vec![l], overlaps: false });
        // The same curl with the square the other way round: a hole.
        let pierced = ring(&[
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 30.0],
            [10.0, 30.0],
            [10.0, 20.0],
            [20.0, 20.0],
            [20.0, 10.0],
            [10.0, 10.0],
            [10.0, 20.0],
            [0.0, 20.0],
        ]);
        let fills = fill(&[pierced], 0.5);
        assert!(
            fills.is_ok_and(|fills| fills.polygons.len() == 1),
            "a contour that only touches itself is filled as it is"
        );
    }

    #[test]
    fn contours_that_cross_each_other_are_refused() {
        let (a, b) = (square(0.0, 0.0, 10.0), square(5.0, 5.0, 10.0));
        assert_eq!(fill(&[ring(&a), ring(&b)], 0.5), Err(Crossing));
    }

    #[test]
    fn a_circle_is_known_simple_without_comparing_its_edges() {
        let circle: Vec<Point> =
            (0..64).map(|i| f64::from(i) * std::f64::consts::TAU / 64.0).map(|a| [a.cos(), a.sin()]).collect();
        assert!(convex(&circle));
        let twice: Vec<Point> = circle.iter().chain(&circle).copied().collect();
        assert!(!convex(&twice), "a ring that goes round twice is not");
    }
}
