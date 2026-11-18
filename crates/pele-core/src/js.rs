//! JavaScript number semantics that Pele's output depends on.
//!
//! Pele prints every coordinate through `Math.round`, `Number.prototype.toString` and
//! `toFixed`. Rust's own `f64::round` breaks ties away from zero and `Display` never uses
//! exponents, so byte-exact SVG needs these exact copies instead.

use std::fmt::Write;

/// `Math.round`: the nearest integer, ties toward +∞, and `-0` for every input in `[-0.5, -0]`.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x < 0.0 { -0.0 } else { rounded }
}

/// `Math.sin`, as V8 computes it: its port of fdlibm's `sin`, `__ieee754_rem_pio2`,
/// `__kernel_rem_pio2`, `__kernel_sin` and `__kernel_cos`, in `src/base/ieee754.cc` of V8
/// 14.6.202.34, the V8 of the pinned Node 26.13.1, in plain double arithmetic.
///
/// It matches Node on x64 bit for bit. Node on arm64 is built to fuse multiply-adds inside the
/// same code, and answers one ulp away on a few inputs in a thousand. `num` hides the gap unless
/// the true value sits within an ulp of a rounding tie. `tests/oracle.rs` measures it, and the
/// decision log's "2026-11-18 V8's trig and log10" has both machines.
pub fn sin(x: f64) -> f64 {
    fdlibm::sin(x)
}

/// `Math.cos`, V8's fdlibm as [`sin`] is, with the same match on x64 and gap on arm64.
pub fn cos(x: f64) -> f64 {
    fdlibm::cos(x)
}

/// `Math.log10`, as V8 computes it: its `ieee754::log10` and `ieee754::log`, ported from fdlibm's
/// `e_log10.c` and `e_log.c`, in the same file as [`sin`]. The power of two and the logarithm of
/// what is left are each scaled to base ten. The `libm` crate's `log10` follows a later FreeBSD
/// version, which rounds differently.
///
/// It matches Node on x64 bit for bit. Node's build on arm64 rounds about one random input in
/// 900 an ulp apart. Within three ulps of every power of ten it rounds none apart, so a whole
/// part read with `floor` is Pele's on both.
pub fn log10(x: f64) -> f64 {
    fdlibm::log10(x)
}

/// `10 ** n`, as V8 works it out: the platform's `pow`, which is correctly rounded for a whole
/// `n`. Parsing `1e{n}` rounds correctly too, so for a whole `n` it matches Node on x64 and arm64
/// bit for bit, where [`pow`] does not. Any other `n` goes to [`pow`].
pub fn ten_to(n: f64) -> f64 {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 400.0 {
        format!("1e{n}").parse().expect("a power of ten parses")
    } else {
        pow(10.0, n)
    }
}

/// `Math.pow`, which is also the `**` operator.
///
/// V8 calls the platform's `pow`, so no port matches every engine. Use it only where one ulp
/// cannot change the output, such as a threshold comparison. Where ECMAScript and C part, it
/// answers as ECMAScript does: NaN for any base to a NaN power and for ±1 to ±∞, where C's
/// `pow` answers 1.
pub fn pow(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {
        return f64::NAN;
    }
    libm::pow(base, exponent)
}

/// `Math.max` of two numbers: NaN if either is, and `+0` over `-0`.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b || (a == b && a.is_sign_positive()) {
        a
    } else {
        b
    }
}

/// `Math.min` of two numbers: NaN if either is, and `-0` under `+0`.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b || (a == b && a.is_sign_negative()) {
        a
    } else {
        b
    }
}

/// `String(x)` for a number: shortest round-trip digits with JavaScript's exponent rules.
pub fn to_string(x: f64) -> String {
    ryu_js::Buffer::new().format(x).to_owned()
}

/// `x.toFixed(digits)` for `digits` in `0..=100`.
///
/// The spec rounds the exact binary value, breaking ties toward the larger magnitude, so this
/// reads the value's exact decimal expansion rather than a rounded one.
pub fn to_fixed(x: f64, digits: usize) -> String {
    assert!(digits <= 100, "toFixed digits must be in 0..=100");
    if x.is_nan() {
        return "NaN".to_owned();
    }
    if x.abs() >= 1e21 {
        return to_string(x);
    }
    let exact = format!("{:.1100}", x.abs());
    let (int, frac) = exact.split_once('.').expect("fixed format has a point");
    let mut kept: Vec<u8> = int.bytes().chain(frac.bytes().take(digits)).collect();
    if frac.as_bytes()[digits] >= b'5' {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let split = kept.len() - digits;
    let mut out = String::with_capacity(kept.len() + 2);
    if x < 0.0 {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&kept[..split]).expect("ascii digits"));
    if digits > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&kept[split..]).expect("ascii digits"));
    }
    out
}

/// `x.toExponential(digits)` for `digits` in `0..=100`: one digit, a point and `digits` more,
/// then the exponent, as in `1.25e+3`.
///
/// The spec keeps the `digits + 1` significant digits nearest the exact binary value, the larger
/// on a tie, as [`to_fixed`] rounds.
pub fn to_exponential(x: f64, digits: usize) -> String {
    assert!(digits <= 100, "toExponential digits must be in 0..=100");
    if !x.is_finite() {
        return to_string(x);
    }
    let mut out = String::with_capacity(digits + 8);
    if x < 0.0 {
        out.push('-');
    }
    let (kept, exponent) = significant(x.abs(), digits + 1);
    write_exponential(&mut out, &kept, exponent);
    out
}

/// `x.toPrecision(digits)` for `digits` in `1..=100`: that many significant digits, rounded as
/// [`to_exponential`] rounds, written with an exponent only when the first digit's is under -6
/// or at least `digits`.
pub fn to_precision(x: f64, digits: usize) -> String {
    assert!((1..=100).contains(&digits), "toPrecision digits must be in 1..=100");
    if !x.is_finite() {
        return to_string(x);
    }
    let mut out = String::with_capacity(digits + 8);
    if x < 0.0 {
        out.push('-');
    }
    let (kept, exponent) = significant(x.abs(), digits);
    if exponent < -6 || exponent >= digits as i32 {
        write_exponential(&mut out, &kept, exponent);
    } else if exponent < 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', exponent.unsigned_abs() as usize - 1));
        out.push_str(ascii(&kept));
    } else {
        let point = exponent as usize + 1;
        out.push_str(ascii(&kept[..point]));
        if point < digits {
            out.push('.');
            out.push_str(ascii(&kept[point..]));
        }
    }
    out
}

/// The first digit, a point and the rest if there are any, then the exponent: `1.25e+3`.
fn write_exponential(out: &mut String, kept: &[u8], exponent: i32) {
    out.push(char::from(kept[0]));
    if kept.len() > 1 {
        out.push('.');
        out.push_str(ascii(&kept[1..]));
    }
    let sign = if exponent < 0 { '-' } else { '+' };
    write!(out, "e{sign}{}", exponent.unsigned_abs()).expect("writing to a String cannot fail");
}

fn ascii(digits: &[u8]) -> &str {
    std::str::from_utf8(digits).expect("ascii digits")
}

/// The spec's `n` and `e` for `toExponential` and `toPrecision`: the `count` significant digits
/// nearest a finite `x ≥ 0`, and the decimal exponent of the first. Zero is `count` zeros with
/// exponent 0.
fn significant(x: f64, count: usize) -> (Vec<u8>, i32) {
    if x == 0.0 {
        return (vec![b'0'; count], 0);
    }
    // A double's exact decimal expansion ends within 1074 places of the point.
    let exact = format!("{x:.1100}");
    let (int, frac) = exact.split_once('.').expect("fixed format has a point");
    let digits: Vec<u8> = int.bytes().chain(frac.bytes()).collect();
    let first = digits.iter().position(|&digit| digit != b'0').expect("a positive number has a digit");
    let mut exponent = int.len() as i32 - 1 - first as i32;
    let mut kept = digits[first..first + count].to_vec();
    if round_significant(&mut kept, digits[first + count]) {
        exponent += 1;
    }
    (kept, exponent)
}

/// Rounds the ASCII digits `kept` to the nearest number of as many digits, given `next`, the digit
/// that follows them in the exact value. A tie goes to the larger, so a `next` of 5 or more rounds
/// up whatever comes after it. Returns whether the carry ran past the first digit, as `9.96` kept
/// to two digits gives `10`, which moves the exponent up by one.
fn round_significant(kept: &mut [u8], next: u8) -> bool {
    if next < b'5' {
        return false;
    }
    match kept.iter().rposition(|&digit| digit != b'9') {
        Some(at) => {
            kept[at] += 1;
            kept[at + 1..].fill(b'0');
            false
        }
        None => {
            kept.fill(b'0');
            kept[0] = b'1';
            true
        }
    }
}

/// Pele's `num()` (`src/svg/builder.ts`): a coordinate rounded to hundredths, trailing zeros dropped.
pub fn num(value: f64) -> String {
    let mut out = String::new();
    write_num(&mut out, value);
    out
}

/// [`num`] appended to `out`, for writers that build one long string.
pub fn write_num(out: &mut String, value: f64) {
    let n = round(value * 100.0);
    if n == 0.0 {
        out.push('0');
        return;
    }
    let Some(Hundredths { negative, whole, frac }) = split_hundredths(n) else {
        out.push_str(&to_string(n / 100.0));
        return;
    };
    if negative {
        out.push('-');
    }
    write!(out, "{whole}").expect("writing to a String cannot fail");
    match frac {
        0 => {}
        f if f % 10 == 0 => write!(out, ".{}", f / 10).expect("writing to a String cannot fail"),
        f => write!(out, ".{f:02}").expect("writing to a String cannot fail"),
    }
}

#[derive(Debug, PartialEq)]
struct Hundredths {
    negative: bool,
    whole: u64,
    frac: u64,
}

/// `num`'s short path for a nonzero integral `n` with `|n| < 1e11`; `None` sends it to `String(n / 100)`.
fn split_hundredths(n: f64) -> Option<Hundredths> {
    if n.is_nan() || n.abs() >= 1e11 {
        return None;
    }
    let hundredths = n.abs() as u64;
    Some(Hundredths { negative: n < 0.0, whole: hundredths / 100, frac: hundredths % 100 })
}

/// V8's fdlibm, `src/base/ieee754.cc` of V8 14.6.202.34, line for line. Each constant is written
/// as the bits fdlibm gives beside it, and the words of a double are read and set as its
/// `GET_HIGH_WORD` and `INSERT_WORDS` do.
mod fdlibm {
    fn high(x: f64) -> i32 {
        (x.to_bits() >> 32) as i32
    }

    fn low(x: f64) -> u32 {
        x.to_bits() as u32
    }

    fn words(high: i32, low: u32) -> f64 {
        f64::from_bits((u64::from(high as u32) << 32) | u64::from(low))
    }

    pub(super) fn sin(x: f64) -> f64 {
        let ix = high(x) & 0x7fff_ffff;
        if ix <= 0x3fe9_21fb {
            return kernel_sin(x, 0.0, false);
        }
        if ix >= 0x7ff0_0000 {
            return f64::NAN;
        }
        let (n, [y0, y1]) = rem_pio2(x);
        match n & 3 {
            0 => kernel_sin(y0, y1, true),
            1 => kernel_cos(y0, y1),
            2 => -kernel_sin(y0, y1, true),
            _ => -kernel_cos(y0, y1),
        }
    }

    pub(super) fn cos(x: f64) -> f64 {
        let ix = high(x) & 0x7fff_ffff;
        if ix <= 0x3fe9_21fb {
            return kernel_cos(x, 0.0);
        }
        if ix >= 0x7ff0_0000 {
            return f64::NAN;
        }
        let (n, [y0, y1]) = rem_pio2(x);
        match n & 3 {
            0 => kernel_cos(y0, y1),
            1 => -kernel_sin(y0, y1, true),
            2 => -kernel_cos(y0, y1),
            _ => kernel_sin(y0, y1, true),
        }
    }

    pub(super) fn log10(x: f64) -> f64 {
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
            if k == 0 {
                f - (hfsq - s * (hfsq + r))
            } else {
                dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
            }
        } else if k == 0 {
            f - s * (f - r)
        } else {
            dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
        }
    }
    /// `__kernel_sin`: sin(x + y) for |x| up to about π/4, from a polynomial of degree 13. `tail`
    /// is fdlibm's `iy`, whether `y` is there at all.
    fn kernel_sin(x: f64, y: f64, tail: bool) -> f64 {
        const S1: f64 = f64::from_bits(0xbfc5_5555_5555_5549);
        const S2: f64 = f64::from_bits(0x3f81_1111_1110_f8a6);
        const S3: f64 = f64::from_bits(0xbf2a_01a0_19c1_61d5);
        const S4: f64 = f64::from_bits(0x3ec7_1de3_57b1_fe7d);
        const S5: f64 = f64::from_bits(0xbe5a_e5e6_8a2b_9ceb);
        const S6: f64 = f64::from_bits(0x3de5_d93a_5acf_d57c);
        // fdlibm also tests `(int)x == 0` here, which always holds and only raises inexact.
        if high(x) & 0x7fff_ffff < 0x3e40_0000 {
            return x;
        }
        let z = x * x;
        let v = z * x;
        let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
        if tail { x - ((z * (0.5 * y - v * r) - y) - v * S1) } else { x + v * (S1 + z * r) }
    }

    /// `__kernel_cos`: cos(x + y) for |x| up to about π/4, from a polynomial of degree 14.
    fn kernel_cos(x: f64, y: f64) -> f64 {
        const C1: f64 = f64::from_bits(0x3fa5_5555_5555_554c);
        const C2: f64 = f64::from_bits(0xbf56_c16c_16c1_5177);
        const C3: f64 = f64::from_bits(0x3efa_01a0_19cb_1590);
        const C4: f64 = f64::from_bits(0xbe92_7e4f_809c_52ad);
        const C5: f64 = f64::from_bits(0x3e21_ee9e_bdb4_b1c4);
        const C6: f64 = f64::from_bits(0xbda8_fae9_be88_38d4);
        let ix = high(x) & 0x7fff_ffff;
        // As in `kernel_sin`, fdlibm's `(int)x == 0` always holds.
        if ix < 0x3e40_0000 {
            return 1.0;
        }
        let z = x * x;
        let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
        if ix < 0x3fd3_3333 {
            return 1.0 - (0.5 * z - (z * r - x * y));
        }
        let qx = if ix > 0x3fe9_0000 { 0.28125 } else { words(ix - 0x0020_0000, 0) };
        let iz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (iz - (z * r - x * y))
    }

    /// `__ieee754_rem_pio2`: x less the nearest multiple n of π/2, as a head and a tail, and n.
    /// Up to 2^19·π/2 it subtracts π/2 in 33-bit pieces; beyond, `kernel_rem_pio2` reduces it.
    pub(super) fn rem_pio2(x: f64) -> (i32, [f64; 2]) {
        const INVPIO2: f64 = f64::from_bits(0x3fe4_5f30_6dc9_c883);
        const PIO2_1: f64 = f64::from_bits(0x3ff9_21fb_5440_0000);
        const PIO2_1T: f64 = f64::from_bits(0x3dd0_b461_1a62_6331);
        const PIO2_2: f64 = f64::from_bits(0x3dd0_b461_1a60_0000);
        const PIO2_2T: f64 = f64::from_bits(0x3ba3_198a_2e03_7073);
        const PIO2_3: f64 = f64::from_bits(0x3ba3_198a_2e00_0000);
        const PIO2_3T: f64 = f64::from_bits(0x397b_839a_2520_49c1);
        const TWO24: f64 = f64::from_bits(0x4170_0000_0000_0000);
        const NPIO2_HW: [i32; 32] = [
            0x3ff9_21fb,
            0x4009_21fb,
            0x4012_d97c,
            0x4019_21fb,
            0x401f_6a7a,
            0x4022_d97c,
            0x4025_fdbb,
            0x4029_21fb,
            0x402c_463a,
            0x402f_6a7a,
            0x4031_475c,
            0x4032_d97c,
            0x4034_6b9c,
            0x4035_fdbb,
            0x4037_8fdb,
            0x4039_21fb,
            0x403a_b41b,
            0x403c_463a,
            0x403d_d85a,
            0x403f_6a7a,
            0x4040_7e4c,
            0x4041_475c,
            0x4042_106c,
            0x4042_d97c,
            0x4043_a28c,
            0x4044_6b9c,
            0x4045_34ac,
            0x4045_fdbb,
            0x4046_c6cb,
            0x4047_8fdb,
            0x4048_58eb,
            0x4049_21fb,
        ];
        let hx = high(x);
        let ix = hx & 0x7fff_ffff;
        if ix <= 0x3fe9_21fb {
            return (0, [x, 0.0]);
        }
        if ix < 0x4002_d97c {
            // |x| < 3π/4, so n is ±1. Near π/2 itself, 33 + 33 + 53 bits of π/2.
            let near = ix == 0x3ff9_21fb;
            return if hx > 0 {
                let z = x - PIO2_1;
                let (z, tail) = if near { (z - PIO2_2, PIO2_2T) } else { (z, PIO2_1T) };
                let y0 = z - tail;
                (1, [y0, (z - y0) - tail])
            } else {
                let z = x + PIO2_1;
                let (z, tail) = if near { (z + PIO2_2, PIO2_2T) } else { (z, PIO2_1T) };
                let y0 = z + tail;
                (-1, [y0, (z - y0) + tail])
            };
        }
        if ix <= 0x4139_21fb {
            let t = x.abs();
            let n = (t * INVPIO2 + 0.5) as i32;
            let fnum = f64::from(n);
            let mut r = t - fnum * PIO2_1;
            let mut w = fnum * PIO2_1T;
            let mut y0 = r - w;
            if n >= 32 || ix == NPIO2_HW[(n - 1) as usize] {
                // Cancellation: subtract the next 33 bits of π/2, and the next, while it lasts.
                let j = ix >> 20;
                if j - ((high(y0) >> 20) & 0x7ff) > 16 {
                    let t = r;
                    w = fnum * PIO2_2;
                    r = t - w;
                    w = fnum * PIO2_2T - ((t - r) - w);
                    y0 = r - w;
                    if j - ((high(y0) >> 20) & 0x7ff) > 49 {
                        let t = r;
                        w = fnum * PIO2_3;
                        r = t - w;
                        w = fnum * PIO2_3T - ((t - r) - w);
                        y0 = r - w;
                    }
                }
            }
            let y1 = (r - y0) - w;
            return if hx < 0 { (-n, [-y0, -y1]) } else { (n, [y0, y1]) };
        }
        if ix >= 0x7ff0_0000 {
            return (0, [f64::NAN, f64::NAN]);
        }
        // |x| scaled to [2^23, 2^24), cut into three 24-bit pieces.
        let e0 = (ix >> 20) - 1046;
        let mut z = words(ix - (e0 << 20), low(x));
        let mut tx = [0.0; 3];
        for piece in &mut tx[..2] {
            *piece = f64::from(z as i32);
            z = (z - *piece) * TWO24;
        }
        tx[2] = z;
        let mut nx = 3;
        while tx[nx - 1] == 0.0 {
            nx -= 1;
        }
        let (n, [y0, y1]) = kernel_rem_pio2(&tx[..nx], e0);
        if hx < 0 { (-n, [-y0, -y1]) } else { (n, [y0, y1]) }
    }

    /// 2/π in 24-bit pieces, the bits after the point: fdlibm's `two_over_pi`.
    const TWO_OVER_PI: [i32; 66] = [
        0xa2f983, 0x6e4e44, 0x1529fc, 0x2757d1, 0xf534dd, 0xc0db62, 0x95993c, 0x439041, 0xfe5163, 0xabdebb, 0xc561b7,
        0x246e3a, 0x424dd2, 0xe00649, 0x2eea09, 0xd1921c, 0xfe1deb, 0x1cb129, 0xa73ee8, 0x8235f5, 0x2ebb44, 0x84e99c,
        0x7026b4, 0x5f7e41, 0x3991d6, 0x398353, 0x39f49c, 0x845f8b, 0xbdf928, 0x3b1ff8, 0x97ffde, 0x05980f, 0xef2f11,
        0x8b5a0a, 0x6d1f6d, 0x367ecf, 0x27cb09, 0xb74f46, 0x3f669e, 0x5fea2d, 0x7527ba, 0xc7ebe5, 0xf17b3d, 0x0739f7,
        0x8a5292, 0xea6bfb, 0x5fb11f, 0x8d5d08, 0x560330, 0x46fc7b, 0x6babf0, 0xcfbc20, 0x9af436, 0x1da9e3, 0x91615e,
        0xe61b08, 0x659985, 0x5f14a0, 0x68408d, 0xffd880, 0x4d7327, 0x310606, 0x1556ca, 0x73a8c9, 0x60e27b, 0xc08c6b,
    ];

    /// `__kernel_rem_pio2` at fdlibm's precision 2, as `__ieee754_rem_pio2` calls it: `x`, given
    /// as 24-bit pieces of which the first is scaled by 2^e0, less the nearest multiple n of
    /// π/2, as a head and a tail, and n mod 8. It multiplies by only the bits of 2/π that can
    /// change n mod 8 and the fraction, taking more while the fraction comes out zero.
    fn kernel_rem_pio2(x: &[f64], e0: i32) -> (i32, [f64; 2]) {
        // `init_jk[2]`: five pieces of 2/π to begin with, and five of π/2 at the end.
        const JK: usize = 4;
        const JP: usize = JK;
        const PIO2: [f64; 8] = [
            f64::from_bits(0x3ff9_21fb_4000_0000),
            f64::from_bits(0x3e74_442d_0000_0000),
            f64::from_bits(0x3cf8_4698_8000_0000),
            f64::from_bits(0x3b78_cc51_6000_0000),
            f64::from_bits(0x39f0_1b83_8000_0000),
            f64::from_bits(0x387a_2520_4000_0000),
            f64::from_bits(0x36e3_8222_8000_0000),
            f64::from_bits(0x3569_f31d_0000_0000),
        ];
        const TWO24: f64 = f64::from_bits(0x4170_0000_0000_0000);
        const TWON24: f64 = f64::from_bits(0x3e70_0000_0000_0000);
        let scalbn = libm::scalbn;
        let jx = x.len() - 1;
        let jv = ((e0 - 3) / 24).max(0);
        let mut q0 = e0 - 24 * (jv + 1);
        let jv = jv as usize;
        let mut f = [0.0; 20];
        let mut q = [0.0; 20];
        let mut iq = [0i32; 20];
        let mut fq = [0.0; 20];
        let piece = |at: usize| f64::from(TWO_OVER_PI[at]);
        for (i, slot) in f[..=jx + JK].iter_mut().enumerate() {
            *slot = if jv + i < jx { 0.0 } else { piece(jv + i - jx) };
        }
        let product = |f: &[f64; 20], i: usize| {
            let mut fw = 0.0;
            for (j, xj) in x.iter().enumerate() {
                fw += xj * f[jx + i - j];
            }
            fw
        };
        for (i, slot) in q[..=JK].iter_mut().enumerate() {
            *slot = product(&f, i);
        }
        let mut jz = JK;
        let (n, ih, mut z) = loop {
            // q[] into 24-bit pieces, the last first.
            let mut z = q[jz];
            for (i, j) in (1..=jz).rev().enumerate() {
                let fw = f64::from((TWON24 * z) as i32);
                iq[i] = (z - TWO24 * fw) as i32;
                z = q[j - 1] + fw;
            }
            z = scalbn(z, q0);
            z -= 8.0 * (z * 0.125).floor();
            let mut n = z as i32;
            z -= f64::from(n);
            let mut ih = 0;
            if q0 > 0 {
                let i = iq[jz - 1] >> (24 - q0);
                n += i;
                iq[jz - 1] -= i << (24 - q0);
                ih = iq[jz - 1] >> (23 - q0);
            } else if q0 == 0 {
                ih = iq[jz - 1] >> 23;
            } else if z >= 0.5 {
                ih = 2;
            }
            if ih > 0 {
                // The fraction is at least a half: take 1 - fraction, and one more for n.
                n += 1;
                let mut carry = false;
                for digit in &mut iq[..jz] {
                    if carry {
                        *digit = 0xff_ffff - *digit;
                    } else if *digit != 0 {
                        carry = true;
                        *digit = 0x100_0000 - *digit;
                    }
                }
                match q0 {
                    1 => iq[jz - 1] &= 0x7f_ffff,
                    2 => iq[jz - 1] &= 0x3f_ffff,
                    _ => {}
                }
                if ih == 2 {
                    z = 1.0 - z;
                    if carry {
                        z -= scalbn(1.0, q0);
                    }
                }
            }
            if z == 0.0 && iq[JK..jz].iter().all(|&digit| digit == 0) {
                // The fraction cancelled to zero: take k more pieces of 2/π and start again.
                let mut k = 1;
                while k <= JK && iq[JK - k] == 0 {
                    k += 1;
                }
                for i in jz + 1..=jz + k {
                    f[jx + i] = piece(jv + i);
                    q[i] = product(&f, i);
                }
                jz += k;
                continue;
            }
            break (n, ih, z);
        };
        if z == 0.0 {
            jz -= 1;
            q0 -= 24;
            while iq[jz] == 0 {
                jz -= 1;
                q0 -= 24;
            }
        } else {
            z = scalbn(z, -q0);
            if z >= TWO24 {
                let fw = f64::from((TWON24 * z) as i32);
                iq[jz] = (z - TWO24 * fw) as i32;
                jz += 1;
                q0 += 24;
                iq[jz] = fw as i32;
            } else {
                iq[jz] = z as i32;
            }
        }
        let mut fw = scalbn(1.0, q0);
        for i in (0..=jz).rev() {
            q[i] = fw * f64::from(iq[i]);
            fw *= TWON24;
        }
        for i in (0..=jz).rev() {
            let mut fw = 0.0;
            for k in 0..=JP.min(jz - i) {
                fw += PIO2[k] * q[i + k];
            }
            fq[jz - i] = fw;
        }
        let mut fw = 0.0;
        for &part in fq[..=jz].iter().rev() {
            fw += part;
        }
        let y0 = if ih == 0 { fw } else { -fw };
        let mut fw = fq[0] - fw;
        for &part in &fq[1..=jz] {
            fw += part;
        }
        let y1 = if ih == 0 { fw } else { -fw };
        (n & 7, [y0, y1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_breaks_ties_toward_positive_infinity() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.49999999999999994), 0.0);
        assert_eq!(round(4503599627370497.0), 4503599627370497.0);
        assert!(round(-0.5).is_sign_negative() && round(-0.5) == 0.0);
    }

    #[test]
    fn max_and_min_follow_ecmascript() {
        assert!(max(f64::NAN, 1.0).is_nan() && max(1.0, f64::NAN).is_nan());
        assert!(min(f64::NAN, 1.0).is_nan() && min(1.0, f64::NAN).is_nan());
        assert!(max(-0.0, 0.0).is_sign_positive() && max(0.0, -0.0).is_sign_positive());
        assert!(min(-0.0, 0.0).is_sign_negative() && min(0.0, -0.0).is_sign_negative());
        assert_eq!(max(2.0, 3.0), 3.0);
        assert_eq!(min(2.0, 3.0), 2.0);
        assert_eq!(max(f64::NEG_INFINITY, -1.0), -1.0);
    }

    #[test]
    fn num_matches_pele_on_known_values() {
        assert_eq!(num(0.125), "0.13");
        assert_eq!(num(-0.125), "-0.12");
        assert_eq!(num(-0.004), "0");
        assert_eq!(num(10.1), "10.1");
        assert_eq!(num(1e12), "1000000000000");
        assert_eq!(num(f64::NAN), "NaN");
        assert_eq!(num(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn to_fixed_rounds_exact_ties_up() {
        assert_eq!(to_fixed(0.25, 1), "0.3");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(-0.4, 0), "-0");
        assert_eq!(to_fixed(-0.0, 0), "0");
        assert_eq!(to_fixed(99.5, 0), "100");
        assert_eq!(to_fixed(1e21, 2), "1e+21");
    }

    #[test]
    fn significant_digits_round_ties_up_and_place_the_point_as_node_does() {
        assert_eq!(to_exponential(1.25, 1), "1.3e+0");
        assert_eq!(to_exponential(-0.0, 2), "0.00e+0");
        assert_eq!(to_exponential(0.000123, 0), "1e-4");
        assert_eq!(to_exponential(9.96, 1), "1.0e+1");
        assert_eq!(to_exponential(-1e21, 3), "-1.000e+21");
        assert_eq!(to_precision(123.456, 4), "123.5");
        assert_eq!(to_precision(0.00000123, 2), "0.0000012");
        assert_eq!(to_precision(1.23e-7, 2), "1.2e-7");
        assert_eq!(to_precision(123456.0, 2), "1.2e+5");
        assert_eq!(to_precision(99.96, 3), "100");
        assert_eq!(to_precision(9.996, 3), "10.0");
        assert_eq!(to_precision(-0.0, 1), "0");
        assert_eq!(to_precision(1e21, 22), "1000000000000000000000");
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    /// ECMAScript `Math.round`, stated as "the closer of the two neighbouring integers, the
    /// larger on a tie, `-0` when that is zero and `x` is negative".
    fn spec_round(x: f64) -> f64 {
        if !x.is_finite() || x == 0.0 {
            return x;
        }
        let (lo, hi) = (x.floor(), x.ceil());
        let nearest = if x - lo < hi - x { lo } else { hi };
        if nearest == 0.0 && x < 0.0 { -0.0 } else { nearest }
    }

    #[kani::proof]
    fn round_is_math_round_for_every_double() {
        let x: f64 = kani::any();
        let (got, want) = (round(x), spec_round(x));
        assert!(got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()));
    }

    /// Pele's own split, transcribed from `num()` in `src/svg/builder.ts` with its double arithmetic.
    fn pele_split(n: f64) -> Option<(bool, f64, f64)> {
        if n > 0.0 {
            if n < 1e11 {
                let q = (n / 100.0).floor();
                return Some((false, q, n - q * 100.0));
            }
        } else if n > -1e11 {
            let m = -n;
            let q = (m / 100.0).floor();
            return Some((true, q, m - q * 100.0));
        }
        None
    }

    fn agrees(n: f64) -> bool {
        match (split_hundredths(n), pele_split(n)) {
            (Some(ours), Some((negative, q, frac))) => {
                ours.negative == negative && ours.whole as f64 == q && ours.frac as f64 == frac
            }
            (None, None) => true,
            _ => false,
        }
    }

    /// Off the short path both sides agree for every nonzero integral double and NaN.
    #[kani::proof]
    fn num_branch_matches_pele() {
        let n: f64 = kani::any();
        kani::assume(n.is_nan() || (n != 0.0 && n == n.floor() && n.abs() >= 1e11));
        assert!(agrees(n));
    }

    /// On the short path the digits agree. Every nonzero integral double below `1e11` in
    /// magnitude is `±(h as f64)` for some `h`, so quantifying over `h` covers the whole path.
    fn short_path_magnitude() -> f64 {
        let h: u64 = kani::any();
        kani::assume(h >= 1 && h < 100_000_000_000);
        h as f64
    }

    #[kani::proof]
    #[kani::solver(kissat)]
    fn num_split_matches_pele_for_positive_n() {
        assert!(agrees(short_path_magnitude()));
    }

    #[kani::proof]
    #[kani::solver(kissat)]
    fn num_split_matches_pele_for_negative_n() {
        assert!(agrees(-short_path_magnitude()));
    }

    /// The most digits treemap keeps: its precision is at most 20, and `toExponential(20)` keeps
    /// 21. It bounds every loop below. The longest runs over the result's `count + 1` digits, 22
    /// passes, so the unwind is 23.
    const MOST_KEPT: usize = 21;

    /// `truncated + one` in one more digit than `truncated`, by schoolbook addition from the right.
    fn add(truncated: &[u8], one: bool) -> [u8; MOST_KEPT + 1] {
        let mut sum = [b'0'; MOST_KEPT + 1];
        let mut carry = one;
        for at in (0..truncated.len()).rev() {
            let digit = truncated[at] + u8::from(carry);
            carry = digit > b'9';
            sum[at + 1] = if carry { b'0' } else { digit };
        }
        sum[0] = if carry { b'1' } else { b'0' };
        sum
    }

    /// For every run of 1 to 21 significant digits, every digit after them and whether anything
    /// nonzero follows that digit, `round_significant` gives the spec's `n`: of the two numbers
    /// with as many digits around the exact value, the nearer, and the larger on a tie.
    #[kani::proof]
    #[kani::unwind(23)]
    fn significant_digits_round_to_the_nearest_and_ties_to_the_larger() {
        let count: usize = kani::any();
        kani::assume((1..=MOST_KEPT).contains(&count));
        let mut kept: [u8; MOST_KEPT] = kani::any();
        kani::assume(kept.iter().all(u8::is_ascii_digit) && kept[0] != b'0');
        let next: u8 = kani::any();
        kani::assume(next.is_ascii_digit());
        let rest_nonzero: bool = kani::any();

        // The distances from the exact value down to the kept digits and up to one more in their
        // last place, in twentieths of that place. A nonzero rest adds some ε in (0, 1) to `next`,
        // and since `next` is whole, every such ε decides as ε = 1/2 does.
        let below = 2 * (next - b'0') + u8::from(rest_nonzero);
        let above = 20 - below;
        let want = add(&kept[..count], above <= below);

        let carried = round_significant(&mut kept[..count], next);
        // The result in `count + 1` digits: a carry past the first digit is a power of ten.
        for at in 0..=count {
            let got = match (carried, at) {
                (true, at) if at < count => kept[at],
                (true, _) | (false, 0) => b'0',
                (false, at) => kept[at - 1],
            };
            assert!(got == want[at]);
        }
        assert!(kept[0] != b'0' && kept.iter().all(u8::is_ascii_digit));
    }
}
