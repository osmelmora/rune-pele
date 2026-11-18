//! Pele's `ticks.ts`: the steps and the tick values of a value axis. A step below one is kept as
//! the negative of its inverse, a whole number, so that tick values are found by division and print
//! without rounding noise (0.3, not 0.30000000000000004).
//!
//! `tests/xychart_ticks.rs` holds the steps, the ticks and the value axis to Pele's bit for bit,
//! and `js::log10` to the whole part `tick_step` reads.

use crate::js;

/// A step of 1, 2 or 5 times a power of ten that divides `span` into about `count` parts.
pub fn tick_step(span: f64, count: f64) -> f64 {
    let raw = span / js::max(count, 1.0);
    let power = js::log10(raw).floor();
    let unit = raw / js::ten_to(power);
    let factor = if unit >= 7.07 {
        10.0
    } else if unit >= 3.16 {
        5.0
    } else if unit >= 1.41 {
        2.0
    } else {
        1.0
    };
    if power < 0.0 { -js::ten_to(-power) / factor } else { factor * js::ten_to(power) }
}

pub fn tick_index(step: f64, value: f64) -> f64 {
    if step > 0.0 { value / step } else { value * -step }
}

pub fn tick_value(step: f64, index: f64) -> f64 {
    if step > 0.0 { index * step } else { index / -step }
}

/// The multiples of a step from `lo` to `hi`, both included, at most 100 of them.
pub fn ticks_between(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let mut first = js::round(tick_index(step, lo));
    if tick_value(step, first) < lo {
        first += 1.0;
    }
    let mut last = js::round(tick_index(step, hi));
    if tick_value(step, last) > hi {
        last -= 1.0;
    }
    let mut out = Vec::new();
    let mut i = first;
    while i <= last && out.len() < 100 {
        out.push(tick_value(step, i));
        i += 1.0;
    }
    out
}

/// What the value axis of a chart gets from its range: Pele's `valueMarks` up to the labels. A
/// range from the data grows to whole steps; one the source sets exactly stays as it is. Gives
/// the range and the tick values.
pub fn value_axis(lo: f64, hi: f64, length: f64, spacing: f64, exact: bool) -> (f64, f64, Vec<f64>) {
    let step = tick_step((hi - lo).abs(), js::max(2.0, js::min(10.0, (length / spacing).floor())));
    let (mut lo, mut hi) = (lo, hi);
    if !exact {
        lo = tick_value(step, (tick_index(step, lo) + 1e-9).floor());
        hi = tick_value(step, (tick_index(step, hi) - 1e-9).ceil());
    }
    let ticks = ticks_between(js::min(lo, hi), js::max(lo, hi), step);
    (lo, hi, ticks)
}
