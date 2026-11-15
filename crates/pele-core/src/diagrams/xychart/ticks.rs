//! Pele's `ticks.ts`: the steps and the tick values of a value axis. A step below one is kept as
//! the negative of its inverse, a whole number, so that tick values are found by division and print
//! without rounding noise (0.3, not 0.30000000000000004).
//!
//! `tests/xychart_ticks.rs` holds the steps, the ticks and the value axis to Pele's bit for bit,
//! and `log10` to the whole part `tick_step` reads.

use crate::js;

/// `Math.log10`, as V8's `ieee754::log10` works it out from fdlibm's `e_log10.c`: the power of two
/// and the logarithm of what is left, each scaled to base ten. The `libm` crate's `log10` follows
/// a later FreeBSD version, which rounds differently. Each constant is written as the bits fdlibm
/// gives beside it. Node's build on darwin arm64 still rounds about one result in 25,000 an ulp
/// apart, which leaves the whole part alone.
pub fn log10(x: f64) -> f64 {
    const TWO54: f64 = f64::from_bits(0x4350_0000_0000_0000);
    const IVLN10: f64 = f64::from_bits(0x3fdb_cb7b_1526_e50e);
    const LOG10_2HI: f64 = f64::from_bits(0x3fd3_4413_509f_6000);
    const LOG10_2LO: f64 = f64::from_bits(0x3d59_fef3_11f1_2b36);
    let bits = x.to_bits();
    let (mut hx, lx) = ((bits >> 32) as i32, bits as u32);
    let mut x = x;
    let mut k: i32 = 0;
    if hx < 0x0010_0000 {
        if (hx & 0x7fff_ffff) as u32 | lx == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = (x.to_bits() >> 32) as i32;
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    let lx = x.to_bits() as u32;
    if hx == 0x3ff0_0000 && lx == 0 {
        return 0.0;
    }
    k += (hx >> 20) - 1023;
    let i = ((k as u32 & 0x8000_0000) >> 31) as i32;
    hx = (hx & 0x000f_ffff) | ((0x3ff - i) << 20);
    let y = f64::from(k + i);
    let x = f64::from_bits(((hx as u32 as u64) << 32) | u64::from(lx));
    let z = y * LOG10_2LO + IVLN10 * log(x);
    z + y * LOG10_2HI
}

/// `ieee754::log` of V8, fdlibm's `e_log.c`: `log(1+f)` from a series in `s = f/(2+f)`, with the
/// power of two added in two parts.
fn log(x: f64) -> f64 {
    const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
    const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
    const TWO54: f64 = f64::from_bits(0x4350_0000_0000_0000);
    const LG1: f64 = f64::from_bits(0x3fe5_5555_5555_5593);
    const LG2: f64 = f64::from_bits(0x3fd9_9999_9997_fa04);
    const LG3: f64 = f64::from_bits(0x3fd2_4924_9422_9359);
    const LG4: f64 = f64::from_bits(0x3fcc_71c5_1d8e_78af);
    const LG5: f64 = f64::from_bits(0x3fc7_4664_96cb_03de);
    const LG6: f64 = f64::from_bits(0x3fc3_9a09_d078_c69f);
    const LG7: f64 = f64::from_bits(0x3fc2_f112_df3e_5244);
    // fdlibm's `0.33333333333333333`.
    const THIRD: f64 = f64::from_bits(0x3fd5_5555_5555_5555);
    let mut x = x;
    let bits = x.to_bits();
    let (mut hx, lx) = ((bits >> 32) as i32, bits as u32);
    let mut k: i32 = 0;
    if hx < 0x0010_0000 {
        if (hx & 0x7fff_ffff) as u32 | lx == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = (x.to_bits() >> 32) as i32;
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000f_ffff;
    let i = (hx + 0x95f64) & 0x10_0000;
    let high = (hx | (i ^ 0x3ff0_0000)) as u32;
    x = f64::from_bits((u64::from(high) << 32) | (x.to_bits() & 0xffff_ffff));
    k += i >> 20;
    let f = x - 1.0;
    if (0x000f_ffff & (2 + hx)) < 3 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = f64::from(k);
            return dk * LN2_HI + dk * LN2_LO;
        }
        let r = f * f * (0.5 - THIRD * f);
        if k == 0 {
            return f - r;
        }
        let dk = f64::from(k);
        return dk * LN2_HI - ((r - dk * LN2_LO) - f);
    }
    let s = f / (2.0 + f);
    let dk = f64::from(k);
    let z = s * s;
    let mut i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 { f - (hfsq - s * (hfsq + r)) } else { dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f) }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

/// `10 ** n` for a whole `n`, as V8 works it out: the platform's `pow`, which is correctly
/// rounded for a power of ten. Parsing `1e{n}` rounds correctly too.
pub fn ten_to(n: f64) -> f64 {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 400.0 {
        format!("1e{n}").parse().expect("a power of ten parses")
    } else {
        js::pow(10.0, n)
    }
}

/// A step of 1, 2 or 5 times a power of ten that divides `span` into about `count` parts.
pub fn tick_step(span: f64, count: f64) -> f64 {
    let raw = span / js::max(count, 1.0);
    let power = log10(raw).floor();
    let unit = raw / ten_to(power);
    let factor = if unit >= 7.07 {
        10.0
    } else if unit >= 3.16 {
        5.0
    } else if unit >= 1.41 {
        2.0
    } else {
        1.0
    };
    if power < 0.0 { -ten_to(-power) / factor } else { factor * ten_to(power) }
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
