//! Where the circles of a venn diagram go and where the text of each region goes, ported from
//! Pele's `src/diagrams/venn/layout.ts`.
//!
//! Beside `Math.sin` and `Math.cos` this uses `Math.acos` and `Math.atan2`. All four come in
//! through [`Trig`], so that `tests/venn_layout.rs` can measure what the port's answers do to
//! the layout against the pinned Node's.

use super::VennSubset;
use crate::input::JsText;
use crate::js;
use crate::layout::num::{by_difference, hypot};
use indexmap::IndexMap;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

const TAU: f64 = PI * 2.0;

/// The `Math` functions the layout calls that the port does not answer as V8 does on every machine.
pub(crate) trait Trig {
    fn sin(&self, x: f64) -> f64;
    fn cos(&self, x: f64) -> f64;
    fn acos(&self, x: f64) -> f64;
    fn atan2(&self, y: f64, x: f64) -> f64;
}

/// `js::sin` and `js::cos` (V8's fdlibm), and libm's `acos` and `atan2`, as every other type computes them.
pub(crate) struct Ported;

impl Trig for Ported {
    fn sin(&self, x: f64) -> f64 {
        js::sin(x)
    }

    fn cos(&self, x: f64) -> f64 {
        js::cos(x)
    }

    fn acos(&self, x: f64) -> f64 {
        libm::acos(x)
    }

    fn atan2(&self, y: f64, x: f64) -> f64 {
        libm::atan2(y, x)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Circle {
    pub(crate) id: JsText,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) r: f64,
}

/// Where the content of a region goes: the point inside all of `inside` and outside every other
/// circle that is farthest from their outlines, and how far that is. A margin of 0 or less means
/// the circles as placed leave the region empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spot {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) margin: f64,
}

/// One step of a region's outline. Every arc turns the positive way and is unrotated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum OutlineStep {
    Move([f64; 2]),
    Line([f64; 2]),
    Arc { r: f64, large: bool, end: [f64; 2] },
    Close,
}

/// For every overlap of three or more sets, the pairwise overlaps it implies and the diagram
/// does not state, a quarter of the smaller set each, after the subsets given. Only the sets and
/// sizes are kept.
pub(crate) fn ensure_pairwise_subsets(subsets: &[VennSubset]) -> Vec<(Vec<JsText>, f64)> {
    let sorted = |sets: &[JsText]| {
        let mut sets = sets.to_vec();
        sets.sort_by(|a, b| a[..].cmp(&b[..]));
        sets
    };
    let mut present: HashSet<Vec<JsText>> = HashSet::new();
    let mut sizes: HashMap<&JsText, f64> = HashMap::new();
    for subset in subsets {
        present.insert(sorted(&subset.sets));
        if subset.sets.len() == 1 {
            sizes.insert(&subset.sets[0], subset.size);
        }
    }
    let mut all: Vec<(Vec<JsText>, f64)> = subsets.iter().map(|subset| (subset.sets.clone(), subset.size)).collect();
    for subset in subsets {
        if subset.sets.len() < 3 {
            continue;
        }
        let members = sorted(&subset.sets);
        for i in 0..members.len() - 1 {
            for j in i + 1..members.len() {
                let pair = vec![members[i].clone(), members[j].clone()];
                if !present.insert(pair.clone()) {
                    continue;
                }
                let size = match (sizes.get(&members[i]), sizes.get(&members[j])) {
                    (Some(&a), Some(&b)) => js::min(a, b) / 4.0,
                    _ => 2.5,
                };
                all.push((pair, size));
            }
        }
    }
    all
}

/// The area two circles share when their centres are `d` apart.
pub(crate) fn lens_area(r1: f64, r2: f64, d: f64, trig: &dyn Trig) -> f64 {
    if d >= r1 + r2 {
        return 0.0;
    }
    let small = js::min(r1, r2);
    if d <= (r1 - r2).abs() {
        return PI * small * small;
    }
    let a1 = r1 * r1 * trig.acos((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1));
    let a2 = r2 * r2 * trig.acos((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2));
    let a3 = 0.5 * js::max((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2), 0.0).sqrt();
    a1 + a2 - a3
}

/// How far apart two circles sit for their overlap to have the given area. The area falls as the
/// distance grows, so bisection finds it.
pub(crate) fn distance_for_overlap(r1: f64, r2: f64, area: f64, trig: &dyn Trig) -> f64 {
    let mut low = (r1 - r2).abs();
    let mut high = r1 + r2;
    if area.partial_cmp(&0.0).is_none_or(Ordering::is_le) {
        return high;
    }
    if area >= PI * js::pow(js::min(r1, r2), 2.0) {
        return low;
    }
    for _ in 0..48 {
        let middle = (low + high) / 2.0;
        if lens_area(r1, r2, middle, trig) > area {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) / 2.0
}

/// How far a point is from the nearest outline it must stay within or without. Gives up, with
/// some value below `floor`, as soon as the point cannot reach `floor`.
fn margin(circles: &[Circle], inside: &[usize], outside: &[usize], x: f64, y: f64, floor: f64) -> f64 {
    let mut best = f64::INFINITY;
    for &i in inside {
        let c = &circles[i];
        let (dx, dy) = (x - c.x, y - c.y);
        let room = c.r - (dx * dx + dy * dy).sqrt();
        if room < best {
            if room < floor {
                return room;
            }
            best = room;
        }
    }
    for &i in outside {
        let c = &circles[i];
        let (dx, dy) = (x - c.x, y - c.y);
        let room = (dx * dx + dy * dy).sqrt() - c.r;
        if room < best {
            if room < floor {
                return room;
            }
            best = room;
        }
    }
    best
}

/// The search of `findSpot`: the best point so far, how roomy it is and how far from the middle.
struct Search<'a> {
    circles: &'a [Circle],
    inside: &'a [usize],
    outside: &'a [usize],
    middle: [f64; 2],
    r: f64,
    x: f64,
    y: f64,
    best: f64,
    off: f64,
}

impl Search<'_> {
    fn consider(&mut self, px: f64, py: f64) -> bool {
        let tolerance = self.r * 1e-9;
        let m = margin(self.circles, self.inside, self.outside, px, py, self.best - tolerance);
        if m.partial_cmp(&(self.best - tolerance)).is_none_or(Ordering::is_le) {
            return false;
        }
        let distance = hypot(px - self.middle[0], py - self.middle[1]);
        if m > self.best + tolerance || distance < self.off - tolerance {
            self.best = m;
            self.off = distance;
            self.x = px;
            self.y = py;
            return true;
        }
        false
    }
}

/// Finds the roomiest point of a region by sampling the smallest of its circles on a grid and
/// then walking uphill from the best sample. Ties go to the point nearest the middle of the
/// region, so a symmetric arrangement gets symmetric labels.
pub(crate) fn find_spot(circles: &[Circle], inside: &[usize], outside: &[usize], trig: &dyn Trig) -> Spot {
    let (mut cx, mut cy) = (0.0, 0.0);
    let mut smallest = &circles[inside[0]];
    let count = inside.len() as f64;
    for &i in inside {
        let c = &circles[i];
        cx += c.x / count;
        cy += c.y / count;
        if c.r < smallest.r {
            smallest = c;
        }
    }
    let r = smallest.r;
    let best = margin(circles, inside, outside, cx, cy, f64::NEG_INFINITY);
    let mut search = Search { circles, inside, outside, middle: [cx, cy], r, x: cx, y: cy, best, off: 0.0 };
    const GRID: i32 = 10;
    for gy in -GRID..=GRID {
        for gx in -GRID..=GRID {
            if gx * gx + gy * gy <= GRID * GRID {
                let grid = f64::from(GRID);
                search.consider(smallest.x + (f64::from(gx) * r) / grid, smallest.y + (f64::from(gy) * r) / grid);
            }
        }
    }
    let mut step = r / f64::from(GRID);
    let mut round = 0;
    while round < 40 && step > r * 1e-6 {
        let mut moved = false;
        for k in 0..8 {
            let angle = (f64::from(k) * TAU) / 8.0;
            let (px, py) = (search.x + trig.cos(angle) * step, search.y + trig.sin(angle) * step);
            if search.consider(px, py) {
                moved = true;
            }
        }
        if !moved {
            step /= 2.0;
        }
        round += 1;
    }
    Spot { x: search.x, y: search.y, margin: search.best }
}

/// Places one circle per set, with areas in proportion to the sizes of the sets and each pair as
/// far apart as its overlap asks: one, two and three sets exactly, more by relaxing a ring of
/// circles toward those distances. Lengths are in units of the square root of a size.
pub(crate) fn layout_circles(subsets: &[VennSubset], trig: &dyn Trig) -> Vec<Circle> {
    let mut sizes: IndexMap<&JsText, f64> = IndexMap::new();
    for subset in subsets {
        for id in &subset.sets {
            sizes.entry(id).or_insert(10.0);
        }
    }
    for subset in subsets {
        if subset.sets.len() == 1 {
            sizes.insert(&subset.sets[0], subset.size);
        }
    }

    let mut largest = 0.0;
    for &size in sizes.values() {
        if size.is_finite() && size > largest {
            largest = size;
        }
    }
    if largest == 0.0 {
        largest = 10.0;
    }
    let widest = (largest / PI).sqrt();
    let mut index: HashMap<&JsText, usize> = HashMap::new();
    let mut circles: Vec<Circle> = Vec::with_capacity(sizes.len());
    for (&id, &size) in &sizes {
        index.insert(id, circles.len());
        // A set too small to hold its name, or with no size at all, still gets a circle that shows.
        let r = if size.is_finite() && size > 0.0 { (size / PI).sqrt() } else { 0.0 };
        circles.push(Circle { id: id.clone(), x: 0.0, y: 0.0, r: js::max(r, widest * 0.3) });
    }
    let n = circles.len();
    if n < 2 {
        return circles;
    }

    // The overlap each pair was given, by the indexes of its circles.
    let mut overlaps: HashMap<usize, f64> = HashMap::new();
    for (sets, size) in ensure_pairwise_subsets(subsets) {
        if sets.len() != 2 {
            continue;
        }
        let (a, b) = (index[&sets[0]], index[&sets[1]]);
        if a != b {
            overlaps.insert(a.min(b) * n + a.max(b), size);
        }
    }
    // How far apart each overlapping pair should be. Pairs that do not overlap only have to keep a gap.
    let wanted: HashMap<usize, f64> = overlaps
        .iter()
        .map(|(&key, &area)| (key, distance_for_overlap(circles[key / n].r, circles[key % n].r, area, trig)))
        .collect();
    let gap = widest * 0.15;
    let distance = |circles: &[Circle], a: usize, b: usize| {
        wanted.get(&(a * n + b)).copied().unwrap_or(circles[a].r + circles[b].r + gap)
    };

    if n == 2 {
        circles[1].x = distance(&circles, 0, 1);
        return circles;
    }

    if n == 3 {
        // Two sets side by side and the third below, the corners of a triangle whose sides are
        // the three distances. An overlap of all three has to exist, so the triangle shrinks
        // until it does.
        let mut ab = distance(&circles, 0, 1);
        let mut ac = distance(&circles, 0, 2);
        let mut bc = distance(&circles, 1, 2);
        let triple = subsets.iter().any(|subset| subset.sets.iter().collect::<HashSet<_>>().len() == 3);
        for _ in 0..40 {
            let x = if ab > 0.0 { (ab * ab + ac * ac - bc * bc) / (2.0 * ab) } else { 0.0 };
            circles[1].x = ab;
            circles[2].x = js::max(-ac, js::min(ac, x));
            circles[2].y = js::max(ac * ac - x * x, 0.0).sqrt();
            if !triple || find_spot(&circles, &[0, 1, 2], &[], trig).margin > widest * 0.12 {
                break;
            }
            ab *= 0.94;
            ac *= 0.94;
            bc *= 0.94;
        }
        return circles;
    }

    let mut total = 0.0;
    for c in &circles {
        total += 2.0 * c.r + gap;
    }
    let rounds = js::min(300.0, (3e6 / (n * n) as f64).floor()) as usize;
    if n <= 64 {
        let ring = total / TAU;
        for (i, c) in circles.iter_mut().enumerate() {
            let angle = (i as f64 * TAU) / n as f64 - PI / 2.0;
            c.x = trig.cos(angle) * ring;
            c.y = trig.sin(angle) * ring;
        }
    } else {
        let columns = (n as f64).sqrt().ceil() as usize;
        let cell = 2.0 * widest + gap;
        for (i, c) in circles.iter_mut().enumerate() {
            c.x = (i % columns) as f64 * cell;
            c.y = (i / columns) as f64 * cell;
        }
    }
    // Each pair's wanted distance by `a * n + b`. Rounds run only while n² is at most 3,000,000.
    let mut pair_wanted: Vec<Option<f64>> = vec![None; if rounds > 0 { n * n } else { 0 }];
    if rounds > 0 {
        for (&key, &apart) in &wanted {
            pair_wanted[key] = Some(apart);
        }
    }
    let mut dx = vec![0.0; n];
    let mut dy = vec![0.0; n];
    let mut pulls = vec![0u32; n];
    for _ in 0..rounds {
        dx.fill(0.0);
        dy.fill(0.0);
        pulls.fill(0);
        for a in 0..n {
            for b in a + 1..n {
                let overlap = pair_wanted[a * n + b];
                let apart = circles[a].r + circles[b].r + gap;
                let mut ux = circles[b].x - circles[a].x;
                let mut uy = circles[b].y - circles[a].y;
                let mut length = hypot(ux, uy);
                if overlap.is_none() && length >= apart {
                    continue;
                }
                if length < 1e-9 {
                    ux = trig.cos((a + b) as f64);
                    uy = trig.sin((a + b) as f64);
                    length = 1.0;
                }
                let shift = (length - overlap.unwrap_or(apart)) / 2.0 / length;
                dx[a] += ux * shift;
                dy[a] += uy * shift;
                dx[b] -= ux * shift;
                dy[b] -= uy * shift;
                pulls[a] += 1;
                pulls[b] += 1;
            }
        }
        for i in 0..n {
            if pulls[i] == 0 {
                continue;
            }
            circles[i].x += dx[i] / f64::from(pulls[i]);
            circles[i].y += dy[i] / f64::from(pulls[i]);
        }
    }
    circles
}

fn whole(c: &Circle) -> Vec<OutlineStep> {
    let arc = |x: f64| OutlineStep::Arc { r: c.r, large: true, end: [x, c.y] };
    vec![OutlineStep::Move([c.x - c.r, c.y]), arc(c.x + c.r), arc(c.x - c.r), OutlineStep::Close]
}

/// Where two outlines cross, with the two circles, by their place in the list.
struct Corner {
    x: f64,
    y: f64,
    a: usize,
    b: usize,
}

/// The outline of the region all the given circles share, or `None` if they share none.
pub(crate) fn region_path(circles: &[Circle], trig: &dyn Trig) -> Option<Vec<OutlineStep>> {
    if circles.len() == 1 {
        return Some(whole(&circles[0]));
    }
    let within = |x: f64, y: f64, skip: usize, other: usize| {
        circles.iter().enumerate().all(|(k, c)| {
            k == skip
                || k == other
                || hypot(x - c.x, y - c.y).partial_cmp(&(c.r * (1.0 + 1e-9))).is_none_or(Ordering::is_le)
        })
    };
    // The corners of the region: where two outlines cross inside all the other circles.
    let mut corners: Vec<Corner> = Vec::new();
    for i in 0..circles.len() {
        for j in i + 1..circles.len() {
            let (a, b) = (&circles[i], &circles[j]);
            let d = hypot(b.x - a.x, b.y - a.y);
            if d >= a.r + b.r || d <= (a.r - b.r).abs() || d == 0.0 {
                continue;
            }
            let along = (a.r * a.r - b.r * b.r + d * d) / (2.0 * d);
            let up = js::max(a.r * a.r - along * along, 0.0).sqrt();
            let mx = a.x + ((b.x - a.x) * along) / d;
            let my = a.y + ((b.y - a.y) * along) / d;
            for sign in [1.0, -1.0] {
                let x = mx + (sign * up * (b.y - a.y)) / d;
                let y = my - (sign * up * (b.x - a.x)) / d;
                if within(x, y, i, j) {
                    corners.push(Corner { x, y, a: i, b: j });
                }
            }
        }
    }
    if corners.len() < 2 {
        // No corners: either one circle lies inside all the others, or there is nothing shared.
        let mut smallest = 0;
        for (k, c) in circles.iter().enumerate() {
            if c.r < circles[smallest].r {
                smallest = k;
            }
        }
        let s = &circles[smallest];
        for (k, c) in circles.iter().enumerate() {
            if k != smallest && hypot(c.x - s.x, c.y - s.y) + s.r > c.r * (1.0 + 1e-9) {
                return None;
            }
        }
        return Some(whole(s));
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    let count = corners.len() as f64;
    for p in &corners {
        cx += p.x / count;
        cy += p.y / count;
    }
    corners.sort_by(|p, q| by_difference(trig.atan2(p.y - cy, p.x - cx) - trig.atan2(q.y - cy, q.x - cx)));
    let mut steps = vec![OutlineStep::Move([corners[0].x, corners[0].y])];
    for i in 0..corners.len() {
        let from = &corners[i];
        let to = &corners[(i + 1) % corners.len()];
        // The side between two corners is an arc of a circle that passes through both; of those,
        // the one whose arc stays inside the others.
        let mut arc: Option<usize> = None;
        let mut large = false;
        let mut narrowest = f64::INFINITY;
        for k in [from.a, from.b] {
            if k != to.a && k != to.b {
                continue;
            }
            let c = &circles[k];
            let start = trig.atan2(from.y - c.y, from.x - c.x);
            let mut sweep = trig.atan2(to.y - c.y, to.x - c.x) - start;
            if sweep <= 0.0 {
                sweep += TAU;
            }
            let mx = c.x + c.r * trig.cos(start + sweep / 2.0);
            let my = c.y + c.r * trig.sin(start + sweep / 2.0);
            let width = hypot(mx - (from.x + to.x) / 2.0, my - (from.y + to.y) / 2.0);
            if width < narrowest {
                narrowest = width;
                arc = Some(k);
                large = sweep > PI;
            }
        }
        steps.push(match arc {
            None => OutlineStep::Line([to.x, to.y]),
            Some(k) => OutlineStep::Arc { r: circles[k].r, large, end: [to.x, to.y] },
        });
    }
    steps.push(OutlineStep::Close);
    Some(steps)
}
