//! A fill cut into upright bands, for what one polygon in one colour cannot draw.
//!
//! Two fills are drawn in bands. A fill along a gradient is a band for each step of colour. And
//! a fill with a curved edge and no stroke over it is in bands for the sake of that edge: Rune
//! cuts a polygon into triangles and softens each along the sides of the polygon it has, and a
//! triangle with two sides that meet at a corner which hardly turns drops the soft edge of each
//! beyond the line of the other. On a smooth curve that is a stretch drawn hard-edged, a
//! quarter of a pixel bolder or lighter than it is.
//!
//! The lines the bands are cut at stand on device pixels, where a hard edge is exact, so two
//! bands meet with no seam. Each band is handed over so that Rune fans it from the middle of
//! such a cut: every triangle then has one side of the curve, or two that meet at the sharp
//! corner the cut makes.

use crate::geometry::Point;

/// One piece of a fill between two upright lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// Half way between the two lines, or between a line and the fill's own end.
    pub middle: f64,
    pub outline: Vec<Point>,
}

fn signed_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i][0] * ring[(i + 1) % n][1] - ring[(i + 1) % n][0] * ring[i][1]).sum::<f64>() / 2.0
}

/// The parts of `ring` left of the upright line at `x`, and those right of it. A point on the
/// line is right of it.
fn split(ring: &[Point], x: f64) -> [Vec<Vec<Point>>; 2] {
    let left = |p: Point| p[0] < x;
    // The ring with a point wherever a side crosses the line, and which points those are.
    let mut points: Vec<Point> = Vec::with_capacity(ring.len() + 4);
    let mut crossings: Vec<usize> = Vec::new();
    for (i, &p) in ring.iter().enumerate() {
        let q = ring[(i + 1) % ring.len()];
        points.push(p);
        if left(p) != left(q) {
            let y = match (p[0] == x, q[0] == x) {
                (true, _) => p[1],
                (_, true) => q[1],
                _ => p[1] + (x - p[0]) / (q[0] - p[0]) * (q[1] - p[1]),
            };
            crossings.push(points.len());
            points.push([x, y]);
        }
    }
    if crossings.is_empty() {
        let whole = vec![ring.to_vec()];
        return if left(ring[0]) { [whole, Vec::new()] } else { [Vec::new(), whole] };
    }
    // Along the line, the inside of a ring that does not cross itself lies between one
    // crossing and the next. A part runs round the ring from a crossing to the next one, along
    // the line to the crossing that one is paired with, and on from there.
    let mut along = crossings.clone();
    along.sort_by(|&a, &b| points[a][1].total_cmp(&points[b][1]));
    let mut pair: Vec<Option<usize>> = vec![None; points.len()];
    for &[from, to] in along.as_chunks::<2>().0 {
        (pair[from], pair[to]) = (Some(to), Some(from));
    }
    let mut begun = vec![false; points.len()];
    let mut sides = [Vec::new(), Vec::new()];
    for &start in &crossings {
        if begun[start] {
            continue;
        }
        let mut part = Vec::new();
        let mut at = start;
        loop {
            begun[at] = true;
            part.push(points[at]);
            let mut next = (at + 1) % points.len();
            let across = loop {
                part.push(points[next]);
                match pair[next] {
                    Some(across) => break across,
                    None => next = (next + 1) % points.len(),
                }
            };
            at = across;
            if at == start {
                break;
            }
        }
        part.dedup();
        if part.len() > 1 && part.first() == part.last() {
            part.pop();
        }
        if signed_area(&part) != 0.0 {
            sides[usize::from(!part.iter().any(|&p| left(p)))].push(part);
        }
    }
    sides
}

/// `part` as Rune fans it whole: ending at the middle of its longest side along the line at
/// `x`. Rune's first triangles all share a polygon's last point, for as far as that point sees.
fn fanned(part: &[Point], x: f64) -> Option<Vec<Point>> {
    let n = part.len();
    let on_line = |i: &usize| part[*i][0] == x && part[(*i + 1) % n][0] == x;
    let length = |i: usize| (part[(i + 1) % n][1] - part[i][1]).abs();
    let side = (0..n).filter(on_line).max_by(|&a, &b| length(a).total_cmp(&length(b)))?;
    let (from, to) = (part[side], part[(side + 1) % n]);
    let round = (1..=n).map(|step| part[(side + step) % n]);
    Some(round.chain([[x, (from[1] + to[1]) / 2.0]]).collect())
}

/// `polygon` cut at each of `lines`, upright lines at rising x, into bands that Rune fans from
/// their cuts. Nothing, where the bands do not add up to the polygon: one that crosses itself
/// is not cut right.
pub fn bands(polygon: &[Point], lines: &[f64]) -> Option<Vec<Band>> {
    let ends = |pick: fn(f64, f64) -> f64, from: f64| polygon.iter().map(|p| p[0]).fold(from, pick);
    let (least, most) = (ends(f64::min, f64::MAX), ends(f64::max, f64::MIN));
    let mut bands = Vec::new();
    let mut rest = vec![polygon.to_vec()];
    let mut before: Option<f64> = None;
    for line in lines.iter().copied().map(Some).chain([None]) {
        let (parts, beyond) = match line {
            Some(x) => {
                let mut sides = [Vec::new(), Vec::new()];
                for ring in &rest {
                    let [left, right] = split(ring, x);
                    sides[0].extend(left);
                    sides[1].extend(right);
                }
                let [left, right] = sides;
                (left, right)
            }
            None => (std::mem::take(&mut rest), Vec::new()),
        };
        let middle = (before.unwrap_or(least).max(least) + line.unwrap_or(most).min(most)) / 2.0;
        for part in parts {
            let cuts = [before, line].into_iter().flatten();
            let outline = cuts.filter_map(|x| fanned(&part, x)).next().unwrap_or(part);
            bands.push(Band { middle, outline });
        }
        (rest, before) = (beyond, line);
    }
    let whole = signed_area(polygon).abs();
    let cut: f64 = bands.iter().map(|band| signed_area(&band.outline).abs()).sum();
    ((cut - whole).abs() <= 1e-6 * whole.max(1.0)).then_some(bands)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(bands: &[Band]) -> f64 {
        bands.iter().map(|band| signed_area(&band.outline).abs()).sum()
    }

    #[test]
    fn a_rectangle_is_cut_at_each_line_and_fanned_from_the_middle_of_a_cut() {
        let plate = [[1.0, 0.0], [9.0, 0.0], [9.0, 4.0], [1.0, 4.0]];
        let bands = bands(&plate, &[4.0, 6.0]).expect("bands");
        let middles: Vec<f64> = bands.iter().map(|band| band.middle).collect();
        assert_eq!(middles, [2.5, 5.0, 7.5], "half way between the lines, and between a line and an end");
        // The first band has only the cut on its right. The others end at their left cut's middle.
        let lasts: Vec<Point> = bands.iter().map(|band| *band.outline.last().expect("a point")).collect();
        assert_eq!(lasts, [[4.0, 2.0], [4.0, 2.0], [6.0, 2.0]]);
        let areas: Vec<f64> = bands.iter().map(|band| signed_area(&band.outline).abs()).collect();
        assert_eq!(areas, [12.0, 8.0, 12.0]);
        for band in &bands {
            let (first, last, before) =
                (band.outline[0], band.outline[band.outline.len() - 1], band.outline[band.outline.len() - 2]);
            assert_eq!((first[0], before[0]), (last[0], last[0]), "the last point lies between two on its cut");
        }
    }

    #[test]
    fn a_line_that_misses_the_polygon_cuts_nothing() {
        let plate = [[1.0, 0.0], [9.0, 0.0], [9.0, 4.0], [1.0, 4.0]];
        let missed = bands(&plate, &[0.5, 20.0]).expect("bands");
        assert_eq!(missed, [Band { middle: 5.0, outline: plate.to_vec() }]);
        // A line along a side cuts nothing off either, and the side is a cut to fan from.
        let along = bands(&plate, &[9.0]).expect("bands");
        assert_eq!(along.len(), 1);
        assert_eq!((area(&along), along[0].outline.last()), (32.0, Some(&[9.0, 2.0])));
    }

    #[test]
    fn a_line_through_a_dent_leaves_a_part_either_side_of_it() {
        // A C, open to the right: the line at 6 crosses its two arms.
        let c = [[0.0, 0.0], [10.0, 0.0], [10.0, 2.0], [3.0, 2.0], [3.0, 8.0], [10.0, 8.0], [10.0, 10.0], [0.0, 10.0]];
        let bands = bands(&c, &[6.0]).expect("bands");
        let rights: Vec<&Band> = bands.iter().filter(|band| band.middle == 8.0).collect();
        assert_eq!(bands.len(), 3, "the back of the C and its two arms' ends");
        assert_eq!(rights.len(), 2);
        assert!(rights.iter().all(|band| signed_area(&band.outline).abs() == 8.0), "each arm's end is 4 by 2");
        assert_eq!(area(&bands), 58.0, "the bands add up to the C");
    }

    #[test]
    fn a_corner_on_a_line_is_in_the_band_to_its_right() {
        // A diamond with its top and bottom corners on the line.
        let diamond = [[5.0, 0.0], [10.0, 5.0], [5.0, 10.0], [0.0, 5.0]];
        let bands = bands(&diamond, &[5.0]).expect("bands");
        assert_eq!(bands.len(), 2);
        assert_eq!(area(&bands), 50.0);
        assert!(bands[0].outline.iter().all(|p| p[0] <= 5.0) && bands[1].outline.iter().all(|p| p[0] >= 5.0));
        // And a corner that only touches the line cuts nothing off.
        let touching = [[0.0, 0.0], [5.0, 3.0], [0.0, 6.0]];
        assert_eq!(super::bands(&touching, &[5.0]).expect("bands").len(), 1);
    }

    #[test]
    fn a_polygon_that_crosses_itself_is_not_cut() {
        // An hourglass on its side, cut through its left half.
        let hourglass = [[0.0, 0.0], [10.0, 6.0], [10.0, 0.0], [0.0, 6.0]];
        assert_eq!(bands(&hourglass, &[2.0]), None, "its two halves wind opposite ways, and add up to nothing");
    }
}
