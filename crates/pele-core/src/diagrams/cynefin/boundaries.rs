//! Pele's `cynefin/boundaries.ts`: the geometry of Mermaid's Cynefin boundaries, seeded wavy
//! lines between the domains and the S-shaped cliff between Clear and Chaotic. A curve is a flat
//! list of numbers: a start point, then six numbers (two handles and an end) per cubic segment.

use crate::layout::num::to_int32;
use crate::scene::{Number, PathProgram};

/// mulberry32, one step, as Pele's `seededRandom` computes it on JavaScript numbers.
pub(super) fn seeded_random(seed: f64) -> f64 {
    let mut t = to_int32(seed + f64::from(0x6d2b_79f5));
    t = (t ^ ((t as u32) >> 15) as i32).wrapping_mul(t | 1);
    let mixed = (t ^ ((t as u32) >> 7) as i32).wrapping_mul(t | 61);
    t ^= t.wrapping_add(mixed);
    f64::from((t ^ ((t as u32) >> 14) as i32) as u32) / 4_294_967_296.0
}

/// Pele's `hashString`: the Java string hash, on UTF-16 code units.
pub(super) fn hash_string(text: &[u16]) -> f64 {
    let mut hash: i32 = 0;
    for &unit in text {
        hash = hash.wrapping_shl(5).wrapping_sub(hash).wrapping_add(i32::from(unit));
    }
    f64::from(hash)
}

/// Pele's `resolveSeed`: a configured seed other than zero wins; otherwise the seed comes from
/// the diagram's id.
pub(super) fn resolve_seed(configured: f64, id: &[u16]) -> f64 {
    if configured.is_finite() && configured != 0.0 { configured } else { hash_string(id) }
}

/// How a wave is drawn: its segments, its axis, the multipliers Mermaid uses to vary the seed per
/// point and per handle, and which ends stay on the centre line.
#[derive(Clone, Copy)]
pub(super) struct WaveOptions {
    pub(super) segments: usize,
    pub(super) vertical: bool,
    pub(super) point_step: f64,
    pub(super) control_step: f64,
    pub(super) control_shift: f64,
    pub(super) pin_start: bool,
    pub(super) pin_end: bool,
}

pub(super) const FOLD: WaveOptions = WaveOptions {
    segments: 7,
    vertical: true,
    point_step: 17.0,
    control_step: 31.0,
    control_shift: 7.0,
    pin_start: false,
    pin_end: false,
};

pub(super) const HORIZON: WaveOptions = WaveOptions {
    segments: 7,
    vertical: false,
    point_step: 23.0,
    control_step: 37.0,
    control_shift: 11.0,
    pin_start: false,
    pin_end: false,
};

/// A wavy line along one axis from `from` to `to`, centred on `center` in the other axis.
/// Points jitter by up to the amplitude; a pinned end stays on the centre line.
pub(super) fn wave(from: f64, to: f64, center: f64, seed: f64, amplitude: f64, o: WaveOptions) -> Vec<f64> {
    let segments = o.segments as f64;
    let step = (to - from) / segments;
    let mut along = Vec::with_capacity(o.segments + 1);
    let mut across = Vec::with_capacity(o.segments + 1);
    for i in 0..=o.segments {
        let pinned = (i == 0 && o.pin_start) || (i == o.segments && o.pin_end);
        let k = i as f64;
        along.push(from + k * step);
        across.push(if pinned {
            center
        } else {
            center + seeded_random(seed + k * o.point_step) * amplitude * 2.0 - amplitude
        });
    }
    let mut out = if o.vertical { vec![across[0], along[0]] } else { vec![along[0], across[0]] };
    for i in 0..o.segments {
        let mid = (along[i] + along[i + 1]) / 2.0;
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let offset = amplitude * 1.5 * sign * seeded_random(seed + i as f64 * o.control_step + o.control_shift);
        if o.vertical {
            out.extend([across[i] + offset, mid, across[i + 1] - offset, mid, across[i + 1], along[i + 1]]);
        } else {
            out.extend([mid, across[i] + offset, mid, across[i + 1] - offset, along[i + 1], across[i + 1]]);
        }
    }
    out
}

/// The cliff: a steep S from (cx, top) down to (cx, bottom).
pub(super) fn cliff(cx: f64, top: f64, bottom: f64, amplitude: f64) -> Vec<f64> {
    let h = bottom - top;
    vec![
        cx,
        top,
        cx + amplitude,
        top + h * 0.2,
        cx - amplitude * 1.5,
        top + h * 0.55,
        cx + amplitude * 0.5,
        top + h * 0.75,
        cx - amplitude,
        top + h * 0.85,
        cx + amplitude * 0.3,
        top + h * 0.95,
        cx,
        bottom,
    ]
}

fn at(curve: &[f64], i: usize) -> [Number; 2] {
    [Number::num(curve[i]), Number::num(curve[i + 1])]
}

/// The point a curve starts at.
pub(super) fn first(curve: &[f64]) -> [Number; 2] {
    at(curve, 0)
}

/// The point a curve ends at.
pub(super) fn last(curve: &[f64]) -> [Number; 2] {
    at(curve, curve.len() - 2)
}

/// Pele's `forward`: commands that continue from the curve's start point to its end.
pub(super) fn forward(mut path: PathProgram, curve: &[f64]) -> PathProgram {
    for i in (2..curve.len()).step_by(6) {
        path = path.c(at(curve, i), at(curve, i + 2), at(curve, i + 4));
    }
    path
}

/// Pele's `backward`: commands that continue from the curve's end point back to its start.
pub(super) fn backward(mut path: PathProgram, curve: &[f64]) -> PathProgram {
    for i in (2..=curve.len() - 6).rev().step_by(6) {
        path = path.c(at(curve, i + 2), at(curve, i), at(curve, i - 2));
    }
    path
}

/// Pele's `curvePath`.
pub(super) fn curve_path(curve: &[f64]) -> PathProgram {
    let [x, y] = first(curve);
    forward(PathProgram::default().m(x, y), curve)
}

/// Pele's `ellipsePath`: an ellipse as two half arcs.
pub(super) fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> PathProgram {
    let radii = [Number::num(rx), Number::num(ry)];
    let west = [Number::num(cx - rx), Number::num(cy)];
    let east = [Number::num(cx + rx), Number::num(cy)];
    PathProgram::default().m(west[0], west[1]).a_paired(radii, true, true, east).a_paired(radii, true, true, west).z()
}
