//! The arguments V8's fdlibm treats apart, in named groups. `tests/oracle.rs` holds `js::sin`,
//! `js::cos` and `js::log10` to the pinned Node on them, and `tests/intel_trig.rs` holds them to
//! Node on x64 bit for bit.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// The inputs of one group, by name.
pub type Groups = Vec<(&'static str, Vec<f64>)>;

/// `x` and its `n` neighbouring doubles on each side.
pub fn around(x: f64, n: usize) -> impl Iterator<Item = f64> {
    let (mut up, mut down) = (x, x);
    let mut out = vec![x];
    for _ in 0..n {
        up = up.next_up();
        down = down.next_down();
        out.extend([up, down]);
    }
    out.into_iter()
}

/// `x` and `-x`, each with its `n` neighbours a side.
fn both_signs(x: f64, n: usize) -> impl Iterator<Item = f64> {
    around(x, n).chain(around(-x, n))
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A double of either sign whose binary exponent is uniform in `lo..=hi`.
    fn magnitude(&mut self, lo: i32, hi: i32) -> f64 {
        let exponent = lo + (self.next() % u64::from((hi - lo).unsigned_abs() + 1)) as i32;
        let mantissa = f64::from_bits(0x3ff0_0000_0000_0000 | (self.next() >> 12));
        let x = libm::scalbn(mantissa, exponent);
        if self.next() & 1 == 0 { x } else { -x }
    }

    fn subnormal(&mut self) -> f64 {
        f64::from_bits((self.next() >> 12) | (self.next() & 0x8000_0000_0000_0000))
    }
}

fn bits(hex: &[u64]) -> Vec<f64> {
    hex.iter().map(|&b| f64::from_bits(b)).collect()
}

fn specials(rng: &mut Rng) -> Vec<f64> {
    let mut out = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        1e-10,
        1e22,
        -1e22,
    ];
    out.extend(both_signs(5e-324, 2));
    out.extend(both_signs(f64::MIN_POSITIVE.next_down(), 2));
    out.extend((0..1000).map(|_| rng.subnormal()));
    out
}

const SPECIAL: &str = "special values and subnormals";

/// The arguments of `Math.sin` and `Math.cos` where fdlibm changes course: special values; the
/// kernels' own thresholds; the bounds of each kind of reduction; whole multiples of π/2 near
/// which the medium reduction cancels; huge arguments that `__kernel_rem_pio2` reduces; and the
/// sets on which the decision log's Intel entry found libm or arm64 Node apart from x64 Node.
pub fn trig() -> Groups {
    let mut rng = Rng(0x7e1e_c0de_5eed_0001);
    let tiny = f64::from_bits(0x3e40_0000_0000_0000);
    let mut under = vec![tiny.next_down(), -tiny.next_down(), 1e-9, -1e-300];
    under.extend((28..=1074).map(|k| libm::scalbn(1.0, -k)));
    under.extend((0..1000).map(|_| rng.magnitude(-1074, -28)));
    let thresholds = [
        tiny,
        f64::from_bits(0x3fd3_3333_0000_0000),
        f64::from_bits(0x3fe9_0000_0000_0000),
        FRAC_PI_4,
        f64::from_bits(0x3fe9_21fb_ffff_ffff),
        f64::from_bits(0x3fe9_21fc_0000_0000),
        FRAC_PI_2,
        f64::from_bits(0x3ff9_21fb_0000_0000),
        f64::from_bits(0x3ff9_21fb_ffff_ffff),
        3.0 * FRAC_PI_4,
        f64::from_bits(0x4002_d97c_0000_0000),
        f64::from_bits(0x4139_21fb_ffff_ffff),
        f64::from_bits(0x4139_21fc_0000_0000),
    ];
    let multiples = (1..=64).flat_map(|n| both_signs(f64::from(n) * FRAC_PI_2, 4));
    let mut huge: Vec<f64> = (20..=1023).flat_map(|k| both_signs(libm::scalbn(1.0, k), 1)).collect();
    huge.extend(both_signs(libm::scalbn(6_381_956_970_095_103.0, 797), 2));
    huge.extend([1e100, 1e200, 1e300, -1e300]);
    huge.extend((19..=60).flat_map(|k| both_signs(libm::scalbn(1.0, k) * FRAC_PI_2, 1)));
    huge.extend((((1 << 19) - 2)..=((1 << 19) + 2)).flat_map(|n| both_signs(f64::from(n) * FRAC_PI_2, 2)));
    huge.extend((0..20000).map(|_| rng.magnitude(20, 1023)));
    let degrees = (-720..=720).flat_map(|k| around(f64::from(k) * PI / 180.0, 1));
    let quarters = (-4000..=4000).flat_map(|k| around(f64::from(k) * FRAC_PI_4, 1));
    let hundredths = (-4000..=4000).flat_map(|k| around(f64::from(k) / 200.0, 1).chain([f64::from(k) / 8.0]));
    vec![
        (SPECIAL, specials(&mut rng)),
        ("under 2^-27", under),
        (
            "2^-27, 0.3, 0.78125, π/4, π/2, 3π/4 and 2^19·π/2, the kernels' and reductions' bounds, ±3 ulps",
            thresholds.iter().flat_map(|&x| both_signs(x, 3)).collect(),
        ),
        ("n·π/2 for n to 64, ±4 ulps", multiples.collect()),
        ("huge arguments, which `__kernel_rem_pio2` reduces", huge),
        ("whole degrees to ±720°, ±1 ulp", degrees.collect()),
        ("multiples of π/4 to ±1000π, ±1 ulp", quarters.collect()),
        ("hundredths and eighths to ±20 and ±500, ±1 ulp", hundredths.collect()),
        (
            "random inputs libm or arm64 Node rounded apart from x64 Node",
            bits(&[
                0xd077_8aff_35c8_1a77,
                0xc0b2_17cf_b238_f79d,
                0x5aaf_c7cc_ff84_11b9,
                0x4001_b2e8_aa71_eaba,
                0xc17e_faac_6d74_1944,
                0xc00e_277b_9520_29d6,
                0x4016_9786_a89f_d032,
                0x4003_c4df_6c17_bc74,
                0xc044_bb28_1cff_596e,
                0x6462_810d_3e1e_dbce,
                0x404e_3ae1_caf5_0133,
                0xbfc2_b8d6_6be7_9780,
                0x3fe6_9696_4560_b92c,
                0xbfe2_41da_3cb3_8c1e,
                0x3fd8_76fe_261a_cc32,
                0x4145_9916_8bcd_d8aa,
                0xc182_6f24_8130_5b17,
                0x400f_7be7_2111_7bf0,
                0x4000_6202_d023_6519,
                0x3ff2_1c3d_6568_21ad,
            ]),
        ),
    ]
}

/// The arguments of `Math.log10` where fdlibm changes course: special values and subnormals,
/// which it scales by 2^54 first; powers of two, whose exponent it takes apart, and of ten;
/// values near 1, where `log` takes its short series, and near √2 and the two bounds of its
/// longer one; and the sets on which the Intel entry found libm or arm64 Node apart.
pub fn log10() -> Groups {
    let mut rng = Rng(0x7e1e_c0de_5eed_0002);
    let tens = (-323..=308).flat_map(|k| around(format!("1e{k}").parse().expect("a power of ten"), 3));
    let twos = (-1074..=1023).flat_map(|k| around(libm::scalbn(1.0, k), 3));
    let mut near_one: Vec<f64> = around(1.0, 64).collect();
    near_one.extend(around(1.0 + libm::scalbn(1.0, -20), 3).chain(around(1.0 - libm::scalbn(1.0, -20), 3)));
    let splits = [0x3ff6_147a_0000_0000_u64, 0x3ff6_b851_0000_0000, 0x3ff6_a09c_0000_0000, 0x3ff6_a09b_ffff_ffff];
    near_one.extend(splits.iter().flat_map(|&b| around(f64::from_bits(b), 3)));
    near_one.extend(splits.iter().flat_map(|&b| around(f64::from_bits(b) * 1024.0, 3)));
    vec![
        (SPECIAL, specials(&mut rng)),
        ("powers of ten, ±3 ulps", tens.collect()),
        ("powers of two, ±3 ulps", twos.collect()),
        ("near 1, √2 and the bounds of `log`'s series, ±3 ulps", near_one),
        ("whole numbers to 100000", (1..=100_000).map(f64::from).collect()),
        ("hundredths to 20", (1..=4000).map(|k| f64::from(k) / 200.0).collect()),
        (
            "random inputs libm or arm64 Node rounded apart from x64 Node",
            bits(&[
                0x3fff_2f26_8d18_8295,
                0x3fd0_09ff_ed9e_6bf8,
                0x3fed_0d89_ff5d_d7b1,
                0x3fef_ff41_e921_96f8,
                0x401b_f767_7269_b15d,
                0x3fec_23f1_5e43_8c3c,
                0x3fe7_e89e_db42_2acb,
                0x3fe9_a9e9_d932_5c97,
                0x40ba_47a6_c762_32d4,
                0x3fda_789d_f39c_deec,
            ]),
        ),
    ]
}
