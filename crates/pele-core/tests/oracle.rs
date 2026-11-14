//! Differential tests: every answer must equal Pele 1.13.0 running in the pinned Node.

use pele_core::js;

fn oracle(requests: &[String]) -> Vec<String> {
    pele_oracle::ask(requests)
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn samples() -> Vec<f64> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut out = vec![
        0.0,
        -0.0,
        0.5,
        -0.5,
        0.49999999999999994,
        -0.49999999999999994,
        1.5,
        -1.5,
        2.5,
        -2.5,
        4503599627370496.0,
        4503599627370497.0,
        -4503599627370497.0,
        9007199254740993.0,
        1e11,
        1e9,
        999999999.995,
        -999999999.995,
        1e21,
        1e-7,
        123e-20,
        5e-324,
        f64::MAX,
        f64::MIN,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.005,
        -0.005,
        0.015,
        1.005,
        -1.005,
        0.125,
        -0.125,
    ];
    for k in -4000..=4000 {
        let tie = f64::from(k) / 200.0;
        out.extend([tie, tie.next_up(), tie.next_down(), f64::from(k) / 8.0]);
    }
    for _ in 0..60_000 {
        out.push(f64::from_bits(rng.next()));
        let coordinate = (rng.unit() - 0.5) * 10f64.powi((rng.next() % 12) as i32);
        out.push(coordinate);
        out.push((coordinate * 1000.0).round() / 1000.0);
    }
    out
}

fn hex(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

fn diff(op: &str, inputs: &[f64], extra: &str, ours: impl Fn(f64) -> String) {
    let requests: Vec<String> = inputs.iter().map(|&x| format!("{op}\t{}{extra}", hex(x))).collect();
    let theirs = oracle(&requests);
    let mismatches: Vec<String> = inputs
        .iter()
        .zip(&theirs)
        .filter_map(|(&x, want)| {
            let got = ours(x);
            (&got != want).then(|| format!("{op}({x:e}{extra}) bits {}: pele {want:?}, ours {got:?}", hex(x)))
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} of {} differ:\n{}",
        mismatches.len(),
        inputs.len(),
        mismatches[..mismatches.len().min(20)].join("\n")
    );
}

#[test]
fn round_matches_math_round() {
    diff("round", &samples(), "", |x| {
        let r = js::round(x);
        if r.is_nan() { "NaN".to_owned() } else { hex(r) }
    });
}

#[test]
fn num_matches_pele_num() {
    diff("num", &samples(), "", js::num);
}

#[test]
fn to_string_matches_string() {
    diff("string", &samples(), "", js::to_string);
}

#[test]
fn to_fixed_matches_to_fixed() {
    let inputs: Vec<f64> = samples().into_iter().step_by(7).collect();
    for digits in [0usize, 1, 2, 3, 20, 100] {
        diff("fixed", &inputs, &format!("\t{digits}"), |x| js::to_fixed(x, digits));
    }
}

/// The angles a chart feeds to `Math.sin` and `Math.cos`: turns, their neighbours and random spans.
fn angles() -> Vec<f64> {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut out = vec![0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e22, -1e22, 5e-324, 1e-10, 0.5, 1.0];
    for k in -720..=720 {
        let turn = f64::from(k) * std::f64::consts::PI / 180.0;
        out.extend([turn, turn.next_up(), turn.next_down()]);
    }
    for _ in 0..100_000 {
        out.push(rng.unit() * std::f64::consts::TAU);
        out.push((rng.unit() - 0.5) * 10f64.powi((rng.next() % 9) as i32));
        out.push(f64::from_bits(rng.next()));
    }
    out
}

fn double(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).expect("the oracle answers 16 hex digits"))
}

/// Whether `ours` is `theirs` or one of its two neighbouring doubles.
fn within_one_ulp(ours: f64, theirs: f64) -> bool {
    if ours.is_nan() || theirs.is_nan() {
        return ours.is_nan() && theirs.is_nan();
    }
    ours == theirs || ours == theirs.next_up() || ours == theirs.next_down()
}

/// The pinned Node does not compute these three the way `libm` does. Its `Math.sin` and
/// `Math.cos` are V8's fdlibm port compiled with fused multiply-adds on arm64, and its `Math.pow`
/// is the platform's `pow`. So the claim here is the neighbouring double, not the same bits.
fn diff_within_one_ulp(op: &str, inputs: &[f64], extra: &str, ours: impl Fn(f64) -> f64) {
    let requests: Vec<String> = inputs.iter().map(|&x| format!("{op}\t{}{extra}", hex(x))).collect();
    let theirs = oracle(&requests);
    let mut exact = 0;
    let far: Vec<String> = inputs
        .iter()
        .zip(&theirs)
        .filter_map(|(&x, want)| {
            let (got, want) = (ours(x), double(want));
            exact += usize::from(got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()));
            (!within_one_ulp(got, want)).then(|| format!("{op}({x:e}{extra}): node {want:e}, ours {got:e}"))
        })
        .collect();
    println!("{op}{extra}: {exact} of {} bit-equal to Node, the rest one ulp away", inputs.len());
    assert!(
        far.is_empty(),
        "{} of {} are further than one ulp:\n{}",
        far.len(),
        inputs.len(),
        far[..far.len().min(20)].join("\n")
    );
}

#[test]
fn sin_is_within_one_ulp_of_math_sin() {
    diff_within_one_ulp("sin", &angles(), "", js::sin);
}

#[test]
fn cos_is_within_one_ulp_of_math_cos() {
    diff_within_one_ulp("cos", &angles(), "", js::cos);
}

#[test]
fn pow_is_within_one_ulp_of_math_pow() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut bases: Vec<f64> = (0..=255).map(|c| (f64::from(c) / 255.0 + 0.055) / 1.055).collect();
    for _ in 0..50_000 {
        bases.push(rng.unit());
        bases.push(rng.unit() * 1e3);
    }
    for exponent in [2.4, 0.5, 2.0, -1.5] {
        diff_within_one_ulp("pow", &bases, &format!("\t{}", hex(exponent)), |x| js::pow(x, exponent));
    }
}
