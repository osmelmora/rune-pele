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
