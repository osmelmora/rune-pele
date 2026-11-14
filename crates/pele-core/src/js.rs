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

/// `Math.sin`, from fdlibm.
///
/// The pinned Node on arm64 answers one ulp away on about 1 input in 80, because its build fuses
/// multiply-adds inside V8's copy of fdlibm. `num` hides that unless the true value sits within
/// an ulp of a rounding tie. `tests/oracle.rs` measures the gap.
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

/// `Math.cos`, with the same one-ulp gap as [`sin`].
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// `Math.pow`, which is also the `**` operator.
///
/// V8 calls the platform's `pow`, so no port matches every engine. Use it only where one ulp
/// cannot change the output, such as a threshold comparison.
pub fn pow(base: f64, exponent: f64) -> f64 {
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
}
