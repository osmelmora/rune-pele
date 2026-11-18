//! Holds the xychart tick functions to Pele's `ticks.ts` bit for bit: `Math.log10`, the powers of
//! ten `tickStep` takes, `tickStep`, `ticksBetween`, and the range and ticks `valueMarks` makes of
//! a value axis. Every number crosses to the oracle and back as the bits of its double.

use pele_core::js::{log10, ten_to};
use pele_core::xychart_ticks::{tick_step, ticks_between, value_axis};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// A double of either sign whose exponent is anywhere from `-span` to `span` decades.
    fn magnitude(&mut self, span: i32) -> f64 {
        let decades = self.below(2 * span as u64 + 1) as i32 - span;
        let mantissa = 1.0 + self.below(1 << 52) as f64 / (1u64 << 52) as f64 * 9.0;
        format!("{mantissa}e{decades}").parse().expect("a double")
    }
}

/// The bits of a double. JavaScript has one NaN, which a `DataView` writes as `7ff8…`.
fn hex(value: f64) -> String {
    if value.is_nan() { "7ff8000000000000".to_owned() } else { format!("{:016x}", value.to_bits()) }
}

/// Each double around `value`, `reach` steps of the last bit either way.
fn around(value: f64, reach: i64) -> impl Iterator<Item = f64> {
    (-reach..=reach).map(move |step| f64::from_bits((value.to_bits() as i64 + step) as u64))
}

fn compare(what: &str, requests: Vec<String>, ours: Vec<String>) {
    let theirs = pele_oracle::ask(&requests);
    let mut different = Vec::new();
    for ((request, ours), theirs) in requests.iter().zip(&ours).zip(&theirs) {
        if ours != theirs {
            different.push(format!("{request}\n    ours: {ours}\n    pele: {theirs}"));
        }
    }
    println!("{what}: {} inputs, {} different", requests.len(), different.len());
    assert!(
        different.is_empty(),
        "{} {what} differ:\n{}",
        different.len(),
        different[..different.len().min(10)].join("\n")
    );
}

/// `Math.log10` is fdlibm's in V8 and here, but Node's build for this machine rounds a few
/// results an ulp apart. `tickStep` reads only the whole part below it, which must agree for
/// every input, including each double within three of every power of ten.
#[test]
fn log10_matches_pele() {
    let mut inputs: Vec<f64> = vec![0.0, -0.0, -1.0, f64::INFINITY, f64::NAN, f64::MIN_POSITIVE, 5e-324, f64::MAX];
    for power in -323..=308 {
        inputs.extend(around(format!("1e{power}").parse().expect("a power of ten"), 3));
    }
    let mut rng = Rng(0x1234_5678_9abc_def1);
    inputs.extend((0..20000).map(|_| rng.magnitude(320).abs()));
    let requests: Vec<String> = inputs.iter().map(|&x| format!("xychart_log10\t{:016x}", x.to_bits())).collect();
    let theirs = pele_oracle::ask(&requests);
    let (mut exact, mut floors_apart) = (0, Vec::new());
    for ((&x, request), theirs) in inputs.iter().zip(&requests).zip(&theirs) {
        let ours = log10(x);
        let pele = f64::from_bits(u64::from_str_radix(theirs, 16).expect("hex bits"));
        if hex(ours) == hex(pele) {
            exact += 1;
        } else if ours.floor() != pele.floor() {
            floors_apart.push(format!("{request}\n    ours: {}\n    pele: {theirs}", hex(ours)));
        } else {
            println!("an ulp apart: {request}: ours {}, pele {theirs}", hex(ours));
        }
    }
    println!(
        "log10 inputs: {} inputs, {exact} bit for bit, {} with another whole part",
        inputs.len(),
        floors_apart.len()
    );
    assert!(floors_apart.is_empty(), "log10 has another whole part:\n{}", floors_apart.join("\n"));
    assert!(exact + 5 >= inputs.len(), "all but a few log10 results are bit for bit");
}

#[test]
fn powers_of_ten_match_pele() {
    let mut inputs: Vec<f64> = (-400..=400).map(f64::from).collect();
    inputs.extend([f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.5, -2.5]);
    let requests = inputs.iter().map(|&n| format!("xychart_pow10\t{}", hex(n))).collect();
    let ours = inputs.iter().map(|&n| hex(ten_to(n))).collect();
    compare("powers of ten", requests, ours);
}

#[test]
fn tick_steps_match_pele() {
    let mut inputs: Vec<(f64, f64)> = Vec::new();
    let counts = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
    for power in -320i32..=308 {
        for factor in [1.0, 1.41, 2.0, 3.16, 5.0, 7.07] {
            let edge: f64 = format!("{factor}e{power}").parse().expect("a double");
            for span in around(edge, 2) {
                inputs.push((span, counts[(power.unsigned_abs() as usize) % counts.len()]));
            }
        }
    }
    let mut rng = Rng(0x0dd_ba11_cafe_f00d);
    for _ in 0..20000 {
        let span = rng.magnitude(310).abs();
        inputs.push((span, counts[rng.below(counts.len() as u64) as usize]));
    }
    inputs.extend([(0.0, 5.0), (f64::INFINITY, 5.0), (f64::NAN, 3.0), (2e300, 10.0), (1.0, 0.5)]);
    let requests = inputs.iter().map(|&(span, count)| format!("xychart_step\t{}\t{}", hex(span), hex(count))).collect();
    let ours = inputs.iter().map(|&(span, count)| hex(tick_step(span, count))).collect();
    compare("tick steps", requests, ours);
}

#[test]
fn ticks_between_match_pele() {
    let mut rng = Rng(0xfeed_beef_1234_5678);
    let mut inputs: Vec<(f64, f64, f64)> = Vec::new();
    for _ in 0..12000 {
        let a = rng.magnitude(12);
        let b = if rng.below(4) == 0 { a + rng.magnitude(3) } else { rng.magnitude(12) };
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let step = tick_step(hi - lo, (2 + rng.below(9)) as f64);
        inputs.push((lo, hi, step));
    }
    inputs.extend([
        (0.0, 1.0, 0.1),
        (0.0, 0.3, -10.0),
        (-1e300, 1e300, 2e299),
        (1e20, 1e20 + 1e5, 1.0),
        (5.0, 1.0, 1.0),
    ]);
    let requests = inputs
        .iter()
        .map(|&(lo, hi, step)| format!("xychart_between\t{}\t{}\t{}", hex(lo), hex(hi), hex(step)))
        .collect();
    let ours = inputs
        .iter()
        .map(|&(lo, hi, step)| ticks_between(lo, hi, step).into_iter().map(hex).collect::<Vec<_>>().join(","))
        .collect();
    compare("ranges between ticks", requests, ours);
}

#[test]
fn value_axes_match_pele() {
    let mut rng = Rng(0x5151_aaaa_0f0f_7777);
    let mut inputs: Vec<(f64, f64, f64, f64, bool)> = Vec::new();
    for _ in 0..20000 {
        let lo = match rng.below(5) {
            0 => 0.0,
            1 => -(rng.below(100) as f64),
            _ => rng.magnitude(8),
        };
        let hi = match rng.below(4) {
            0 => lo + rng.magnitude(4).abs(),
            1 => (rng.below(1000) as f64) / 10.0,
            _ => rng.magnitude(8),
        };
        let length = 1.0 + rng.below(4000) as f64 * 0.37;
        let spacing = if rng.below(2) == 0 { 56.0 } else { 80.0 };
        inputs.push((lo, hi, length, spacing, rng.below(3) == 0));
    }
    inputs.extend([
        (-1e300, 1e300, 400.0, 56.0, false),
        (0.0, 1e-300, 400.0, 80.0, false),
        (3.0, 3.0000001, 50.0, 56.0, true),
    ]);
    let requests = inputs
        .iter()
        .map(|&(lo, hi, length, spacing, exact)| {
            format!("xychart_axis\t{}\t{}\t{}\t{}\t{exact}", hex(lo), hex(hi), hex(length), hex(spacing))
        })
        .collect();
    let ours = inputs
        .iter()
        .map(|&(lo, hi, length, spacing, exact)| {
            let (lo, hi, ticks) = value_axis(lo, hi, length, spacing, exact);
            [hex(lo), hex(hi)].into_iter().chain(ticks.into_iter().map(hex)).collect::<Vec<_>>().join(",")
        })
        .collect();
    compare("value axes", requests, ours);
}
