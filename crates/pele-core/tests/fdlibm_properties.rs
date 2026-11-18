//! What `js::sin`, `js::cos`, `js::log10` and `js::ten_to` promise for every argument, where the
//! Kani proofs in `js.rs` cannot decide it in reasonable time: checked on fdlibm's edges and on
//! random doubles of every size, so sampled, not proved. `js.rs`'s proofs cover NaN and the
//! infinities, arguments under 2^-27, and the range up to π/4.

use pele_core::js;

#[path = "js/fdlibm_edges.rs"]
#[allow(dead_code, reason = "these properties read only the trig edges")]
mod fdlibm_edges;

/// fdlibm's edges and a million doubles of any bits, NaN and huge ones among them.
fn arguments() -> Vec<f64> {
    let mut out: Vec<f64> = fdlibm_edges::trig().into_iter().flat_map(|(_, inputs)| inputs).collect();
    let mut state = 0x0dd5_eed5_c0de_0001_u64;
    out.extend((0..1_000_000).map(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        f64::from_bits(state)
    }));
    out
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

#[test]
fn sin_is_odd_and_cos_is_even() {
    let apart: Vec<String> = arguments()
        .into_iter()
        .filter(|&x| !same(js::sin(-x), -js::sin(x)) || !same(js::cos(-x), js::cos(x)))
        .map(|x| format!("{x:e} [{:016x}]", x.to_bits()))
        .collect();
    assert!(apart.is_empty(), "{} arguments where sin is not odd or cos not even:\n{}", apart.len(), apart.join("\n"));
}

#[test]
fn sin_and_cos_of_a_finite_argument_lie_in_minus_one_to_one() {
    let outside: Vec<String> = arguments()
        .into_iter()
        .filter(|x| x.is_finite())
        .filter(|&x| !(-1.0..=1.0).contains(&js::sin(x)) || !(-1.0..=1.0).contains(&js::cos(x)))
        .map(|x| format!("{x:e} [{:016x}]", x.to_bits()))
        .collect();
    assert!(outside.is_empty(), "{} arguments outside [-1, 1]:\n{}", outside.len(), outside.join("\n"));
}

/// Gantt and xychart floor the `log10` of a step and divide by `ten_to` of that, so a power of ten
/// must give its own exponent back, for every power whose double is normal, 1e-307 to 1e308.
#[test]
fn the_log10_of_every_normal_power_of_ten_is_its_exponent() {
    let wrong: Vec<String> = (-307..=308)
        .map(f64::from)
        .filter(|&k| js::log10(js::ten_to(k)).to_bits() != k.to_bits())
        .map(|k| format!("log10(1e{k}) = {:e}", js::log10(js::ten_to(k))))
        .collect();
    assert!(wrong.is_empty(), "{} powers of ten give another exponent:\n{}", wrong.len(), wrong.join("\n"));
}
