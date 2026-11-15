//! Block arrows, ported from Pele's `src/diagrams/block/arrows.ts`: the directions an arrow
//! points, the size it needs for its label, and its outline as one polygon.

use crate::input::{JsText, Units};
use crate::js;

pub(super) const RIGHT: u8 = 1;
pub(super) const LEFT: u8 = 2;
pub(super) const UP: u8 = 4;
pub(super) const DOWN: u8 = 8;

/// How far a head reaches past each side of its shaft.
pub(super) const OVERHANG: f64 = 7.0;
const MAX_DEPTH: f64 = 20.0;
const NUB: f64 = 12.0;

pub(super) struct ArrowSize {
    pub(super) w: f64,
    pub(super) h: f64,
    /// Shift of the label from the centre, away from the heads.
    pub(super) dx: f64,
    pub(super) dy: f64,
}

/// The axis an arrow runs along.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    /// A shaft with heads wider than it.
    X,
    Y,
    /// A box with a small point on each side it names, for an arrow that points along both
    /// axes or along neither.
    Box,
}

/// Pele's `arrowMask`. Mermaid compares directions exactly, so `( up )` draws no arrow there.
/// Here spaces are ignored, as in Pele.
pub(super) fn arrow_mask(directions: Option<&[JsText]>) -> u8 {
    let mut mask = 0;
    for direction in directions.unwrap_or_default() {
        let direction = String::from_utf16_lossy(direction.trim_js());
        mask |= match direction.as_str() {
            "right" => RIGHT,
            "left" => LEFT,
            "up" => UP,
            "down" => DOWN,
            "x" => RIGHT | LEFT,
            "y" => UP | DOWN,
            _ => 0,
        };
    }
    mask
}

fn bit(mask: u8, flag: u8) -> f64 {
    if mask & flag != 0 { 1.0 } else { 0.0 }
}

pub(super) fn arrow_axis(mask: u8) -> Axis {
    if mask != 0 && mask & (UP | DOWN) == 0 {
        Axis::X
    } else if mask != 0 && mask & (RIGHT | LEFT) == 0 {
        Axis::Y
    } else {
        Axis::Box
    }
}

/// Pele's `arrowSize`: the size of an arrow around a label `tw` by `th`.
pub(super) fn arrow_size(mask: u8, tw: f64, th: f64) -> ArrowSize {
    let across = bit(mask, LEFT) - bit(mask, RIGHT);
    let along = bit(mask, UP) - bit(mask, DOWN);
    match arrow_axis(mask) {
        Axis::X => {
            let h = js::max(th + 8.0, 14.0) + 2.0 * OVERHANG;
            let depth = js::min(h / 2.0, MAX_DEPTH);
            let heads = if mask == RIGHT | LEFT { 2.0 } else { 1.0 };
            ArrowSize { w: js::max(tw + 12.0, 16.0) + depth * heads, h, dx: (across * depth) / 2.0, dy: 0.0 }
        }
        Axis::Y => {
            let w = js::max(tw + 16.0, 14.0) + 2.0 * OVERHANG;
            let depth = js::min(w / 2.0, MAX_DEPTH);
            let heads = if mask == UP | DOWN { 2.0 } else { 1.0 };
            ArrowSize { w, h: js::max(th + 8.0, 16.0) + depth * heads, dx: 0.0, dy: (along * depth) / 2.0 }
        }
        Axis::Box => ArrowSize {
            w: js::max(tw + 16.0, 24.0) + NUB * (bit(mask, LEFT) + bit(mask, RIGHT)),
            h: js::max(th + 8.0, 24.0) + NUB * (bit(mask, UP) + bit(mask, DOWN)),
            dx: (across * NUB) / 2.0,
            dy: (along * NUB) / 2.0,
        },
    }
}

/// The corners of Pele's `drawArrow` polygon for an arrow `w` by `h` centred on the origin, as
/// x, y pairs.
pub(super) fn arrow_points(mask: u8, w: f64, h: f64) -> Vec<f64> {
    let x = w / 2.0;
    let y = h / 2.0;
    let mut p: Vec<f64>;
    let has = |flag: u8| mask & flag != 0;
    match arrow_axis(mask) {
        Axis::X => {
            let depth = js::min(y, MAX_DEPTH);
            let s = y - OVERHANG;
            let x0 = if has(LEFT) { -x + depth } else { -x };
            let x1 = if has(RIGHT) { x - depth } else { x };
            p = vec![x0, -s, x1, -s];
            if has(RIGHT) {
                p.extend([x1, -y, x, 0.0, x1, y]);
            }
            p.extend([x1, s, x0, s]);
            if has(LEFT) {
                p.extend([x0, y, -x, 0.0, x0, -y]);
            }
        }
        Axis::Y => {
            let depth = js::min(x, MAX_DEPTH);
            let s = x - OVERHANG;
            let y0 = if has(UP) { -y + depth } else { -y };
            let y1 = if has(DOWN) { y - depth } else { y };
            p = vec![s, y0, s, y1];
            if has(DOWN) {
                p.extend([x, y1, 0.0, y, -x, y1]);
            }
            p.extend([-s, y1, -s, y0]);
            if has(UP) {
                p.extend([-x, y0, 0.0, -y, x, y0]);
            }
        }
        Axis::Box => {
            let x0 = if has(LEFT) { -x + NUB } else { -x };
            let x1 = if has(RIGHT) { x - NUB } else { x };
            let y0 = if has(UP) { -y + NUB } else { -y };
            let y1 = if has(DOWN) { y - NUB } else { y };
            let cx = (x0 + x1) / 2.0;
            let cy = (y0 + y1) / 2.0;
            let kx = js::min(10.0, (x1 - x0) / 2.0);
            let ky = js::min(10.0, (y1 - y0) / 2.0);
            p = vec![x0, y0];
            if has(UP) {
                p.extend([cx - kx, y0, cx, -y, cx + kx, y0]);
            }
            p.extend([x1, y0]);
            if has(RIGHT) {
                p.extend([x1, cy - ky, x, cy, x1, cy + ky]);
            }
            p.extend([x1, y1]);
            if has(DOWN) {
                p.extend([cx + kx, y1, cx, y, cx - kx, y1]);
            }
            p.extend([x0, y1]);
            if has(LEFT) {
                p.extend([x0, cy + ky, -x, cy, x0, cy - ky]);
            }
        }
    }
    p
}
