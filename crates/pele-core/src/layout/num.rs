//! JavaScript number rules the layout leans on, beside those in `crate::js`.

use std::cmp::Ordering;

/// What `Array.prototype.sort` makes of a comparator's result. A negative number sorts the first
/// item first and a positive one the second. Zero and NaN leave the pair to the next rule, which
/// is how `a - b || next` reads in Pele.
pub(crate) fn by_difference(difference: f64) -> Ordering {
    if difference < 0.0 {
        Ordering::Less
    } else if difference > 0.0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// `Math.hypot` of two numbers, as V8 computes it: both scaled by the larger, and their squares
/// added with Kahan summation. `libm::hypot` rounds differently.
pub fn hypot(x: f64, y: f64) -> f64 {
    let mut any_nan = false;
    let mut max = 0.0;
    for value in [x, y] {
        if value.is_nan() {
            any_nan = true;
        } else if value.abs() > max {
            max = value.abs();
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if any_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for value in [x, y] {
        let n = value.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// What storing a number in an `Int32Array` keeps of it.
pub(crate) fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let wrapped = x.trunc().rem_euclid(4_294_967_296.0);
    (wrapped as u32) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_int32_wraps_and_drops_fractions() {
        assert_eq!(to_int32(2.9), 2);
        assert_eq!(to_int32(-2.9), -2);
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(f64::INFINITY), 0);
        assert_eq!(to_int32(2_147_483_648.0), i32::MIN);
        assert_eq!(to_int32(-1.0), -1);
    }

    #[test]
    fn a_nan_difference_leaves_the_pair_to_the_next_rule() {
        assert_eq!(by_difference(f64::NAN), Ordering::Equal);
        assert_eq!(by_difference(-0.0), Ordering::Equal);
        assert_eq!(by_difference(-1e-300), Ordering::Less);
        assert_eq!(by_difference(f64::INFINITY), Ordering::Greater);
    }
}
