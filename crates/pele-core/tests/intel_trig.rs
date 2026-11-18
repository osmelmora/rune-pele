//! Pele on an Intel Mac against Pele on Apple Silicon and against this port. One test compares
//! the maths functions Pele calls bit for bit, the other Pele's own SVGs over the corpus and the
//! fixtures. Both are ignored: `mise run intel-trig` runs them beside an x86-64 Node of the pinned
//! version, and they write their tables to `out/verify/intel-trig/`.

use pele_core::{js, layout::num::hypot};
use pele_oracle::PeleRender;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A Node this comparison runs, checked to be the pinned version built for `arch`.
struct Node {
    arch: &'static str,
    path: PathBuf,
}

impl Node {
    fn named(arch: &'static str, var: &str, otherwise: Option<PathBuf>) -> Node {
        let path = std::env::var_os(var)
            .map(PathBuf::from)
            .or(otherwise)
            .unwrap_or_else(|| panic!("set {var} to a Node built for {arch}; `mise run intel-trig` does"));
        let said = Command::new(&path)
            .args(["-p", "process.version + ' ' + process.arch"])
            .output()
            .unwrap_or_else(|e| panic!("cannot run {}: {e}", path.display()));
        let said = String::from_utf8_lossy(&said.stdout).trim().to_owned();
        let pinned = &pele_oracle::node().expect("the pinned Node").version;
        assert_eq!(said, format!("{pinned} {arch}"), "{} must be Node {pinned} built for {arch}", path.display());
        Node { arch, path }
    }

    /// Runs `script` with `args` under this Node, sends it `input` and returns all it printed.
    fn run(&self, flags: &[&str], script: &Path, args: &[&str], input: impl FnOnce(&mut dyn Write) + Send) -> Vec<u8> {
        let mut child = Command::new(&self.path)
            .args(flags)
            .arg(script)
            .args(args)
            .env("TZ", "UTC")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {}: {e}", self.path.display()));
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut out = Vec::new();
        std::thread::scope(|s| {
            s.spawn(move || input(&mut stdin));
            stdout.read_to_end(&mut out).expect("node's answers");
        });
        let status = child.wait().expect("node finishes");
        assert!(status.success(), "{} {} {args:?} failed: {status}", self.path.display(), script.display());
        out
    }
}

/// The two Nodes: `INTEL_TRIG_X64_NODE`, and `INTEL_TRIG_ARM64_NODE` or else the oracle's own.
fn nodes() -> [Node; 2] {
    let pinned = pele_oracle::node().ok().map(|node| node.path.clone());
    [Node::named("x64", "INTEL_TRIG_X64_NODE", None), Node::named("arm64", "INTEL_TRIG_ARM64_NODE", pinned)]
}

fn out_dir() -> PathBuf {
    let dir = std::env::var_os("INTEL_TRIG_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| pele_oracle::root().join("out/verify/intel-trig"));
    std::fs::create_dir_all(&dir).expect("the output folder");
    dir
}

/// SplitMix64, so that one seed names the whole sample.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// Any double, NaN, infinities and subnormals included.
    fn bits(&mut self) -> f64 {
        f64::from_bits(self.next())
    }

    /// A double of either sign whose binary exponent is uniform in `lo..=hi`.
    fn magnitude(&mut self, lo: i32, hi: i32) -> f64 {
        let exponent = lo + i32::try_from(self.below(u64::from((hi - lo).unsigned_abs()) + 1)).expect("small");
        let mantissa = f64::from_bits(0x3ff0_0000_0000_0000 | (self.next() >> 12));
        let x = libm::scalbn(mantissa, exponent);
        if self.next() & 1 == 0 { x } else { -x }
    }

    /// A positive double of any finite magnitude.
    fn positive(&mut self) -> f64 {
        self.magnitude(-1074, 1023).abs()
    }

    /// A coordinate as a chart has them: uniform in ±10^k / 2, for k below `decades`.
    fn coordinate(&mut self, decades: u64) -> f64 {
        (self.unit() - 0.5) * 10f64.powi(i32::try_from(self.below(decades)).expect("small"))
    }
}

/// `x` and its `n` neighbouring doubles on each side.
fn around(x: f64, n: usize) -> impl Iterator<Item = f64> {
    let (mut up, mut down) = (x, x);
    let mut out = vec![x];
    for _ in 0..n {
        up = up.next_up();
        down = down.next_down();
        out.extend([up, down]);
    }
    out.into_iter()
}

const SPECIAL: [f64; 14] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    -0.5,
    2.0,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    5e-324,
    f64::MIN_POSITIVE,
    f64::MAX,
    1e22,
];

fn unary(xs: impl IntoIterator<Item = f64>) -> Vec<[f64; 2]> {
    xs.into_iter().map(|x| [x, 0.0]).collect()
}

/// The inputs Pele's code is known to feed a function, in named groups.
type Edges = Vec<(&'static str, Vec<[f64; 2]>)>;

fn pairs(xs: &[f64], ys: &[f64]) -> Vec<[f64; 2]> {
    xs.iter().flat_map(|&x| ys.iter().map(move |&y| [x, y])).collect()
}

/// The angles of `tests/oracle.rs` and the hundredths and eighths it samples, each with its
/// neighbours: where the pinned Node on arm64 was seen one ulp away from libm.
fn angle_edges() -> Edges {
    let degrees = (-720..=720).flat_map(|k| around(f64::from(k) * std::f64::consts::PI / 180.0, 1));
    let quarters = (-4000..=4000).flat_map(|k| around(f64::from(k) * std::f64::consts::FRAC_PI_4, 1));
    let hundredths = (-4000..=4000).flat_map(|k| around(f64::from(k) / 200.0, 1).chain([f64::from(k) / 8.0]));
    vec![
        ("special values", unary(SPECIAL.into_iter().chain([-1e22, 1e-10]))),
        ("whole degrees to ±720°, ±1 ulp", unary(degrees)),
        ("multiples of π/4 to ±1000π, ±1 ulp", unary(quarters)),
        ("hundredths and eighths to ±20 and ±500, ±1 ulp", unary(hundredths)),
    ]
}

/// Cosines and their neighbours, as venn's overlap and block's arrow heads take them.
fn unit_edges() -> Edges {
    let thousandths = (-1000..=1000).flat_map(|k| around(f64::from(k) / 1000.0, 1));
    let near_one = (1..=53).flat_map(|k| {
        let near = 1.0 - libm::scalbn(1.0, -k);
        [near, -near]
    });
    let cosines = (-720..=720).map(|k| libm::cos(f64::from(k) * std::f64::consts::PI / 180.0));
    vec![
        ("special values", unary(SPECIAL)),
        ("thousandths of -1 to 1, ±1 ulp", unary(thousandths)),
        ("±(1 - 2^-k)", unary(near_one)),
        ("cosines of whole degrees", unary(cosines)),
    ]
}

/// Every pair of whole numbers from -40 to 40, sides of right triangles, and special values.
fn grid_edges() -> Edges {
    let whole: Vec<f64> = (-40..=40).map(f64::from).collect();
    vec![
        ("special values, in pairs", pairs(&SPECIAL, &SPECIAL)),
        ("whole numbers -40 to 40, in pairs", pairs(&whole, &whole)),
        ("3k and 4k", (1..=1000).map(|k| [3.0 * f64::from(k), 4.0 * f64::from(k)]).collect()),
    ]
}

/// The powers Pele takes: sRGB channels to 2.4 (`ink`), 0.99 to a sankey's passes, ten to an
/// axis's power, two to a packet's, a half to a cluster's depth, and squares in cynefin.
fn pow_edges() -> Edges {
    let channels: Vec<f64> = (0..=255).map(|c| (f64::from(c) / 255.0 + 0.055) / 1.055).collect();
    vec![
        ("special values, in pairs", pairs(&SPECIAL, &SPECIAL)),
        ("sRGB channels ^ 2.4", pairs(&channels, &[2.4])),
        ("sRGB channels ^ 1/2.4, 0.5, 2 and -1.5", pairs(&channels, &[1.0 / 2.4, 0.5, 2.0, -1.5])),
        ("0.99 ^ 0..1000", (0..=1000).map(|pass| [0.99, f64::from(pass)]).collect()),
        ("10 ^ -330..330", (-330..=330).map(|power| [10.0, f64::from(power)]).collect()),
        ("2 ^ -1100..1100", (-1100..=1100).map(|power| [2.0, f64::from(power)]).collect()),
        ("0.5 ^ 0..64", (0..=64).map(|depth| [0.5, f64::from(depth)]).collect()),
        ("hundredths to ±20, squared", (-4000..=4000).map(|k| [f64::from(k) / 200.0, 2.0]).collect()),
    ]
}

/// Each power of ten as JavaScript parses it, with three neighbours a side, whole numbers and
/// hundredths: an axis's step and a treemap's crowding.
fn log10_edges() -> Edges {
    let tens = (-323..=308).flat_map(|k| around(format!("1e{k}").parse().expect("a power of ten"), 3));
    vec![
        ("special values", unary(SPECIAL)),
        ("powers of ten, ±3 ulps", unary(tens)),
        ("whole numbers to 100000", unary((1..=100_000).map(f64::from))),
        ("hundredths to 20", unary((1..=4000).map(|k| f64::from(k) / 200.0))),
    ]
}

/// Each power of two with three neighbours a side, and whole numbers: a packet's bit count.
fn log2_edges() -> Edges {
    vec![
        ("special values", unary(SPECIAL)),
        ("powers of two, ±3 ulps", unary((-1074..=1023).flat_map(|k| around(libm::scalbn(1.0, k), 3)))),
        ("whole numbers to 100000", unary((1..=100_000).map(f64::from))),
    ]
}

fn sqrt_edges() -> Edges {
    vec![
        ("special values", unary(SPECIAL)),
        ("whole numbers to 100000", unary((0..=100_000).map(f64::from))),
        ("hundredths to 20, ±1 ulp", unary((1..=4000).flat_map(|k| around(f64::from(k) / 200.0, 1)))),
    ]
}

/// A port's stand-in for a JavaScript function, as the port calls it.
struct Port {
    name: &'static str,
    f: fn(f64, f64) -> f64,
    /// Measured bit-equal to x64 Node on every input, outside a split's other part. The run fails
    /// when that stops being true, because the decision log says so.
    same_on_x64: bool,
}

/// A `Math` function Pele calls: how many arguments it takes, the port's stand-ins, the inputs
/// Pele's code is known to feed it, a random input of the sort it gets, and where a random
/// sample is reported in two parts, what parts them.
struct Function {
    js: &'static str,
    arity: usize,
    ports: &'static [Port],
    edges: fn() -> Edges,
    random: fn(&mut Rng) -> [f64; 2],
    split: Option<Split>,
}

impl Function {
    /// How JavaScript writes the call: `Math.sin`, or the `**` operator.
    fn label(&self) -> String {
        if self.js == "power" { "`**`".to_owned() } else { format!("`Math.{}`", self.js) }
    }
}

/// Two parts of a random sample: those `inside` holds, and the rest.
struct Split {
    inside: &'static str,
    outside: &'static str,
    holds: fn([f64; 2]) -> bool,
}

/// A third of each random sample is any double at all; the rest is the function's own ground.
fn mixed(rng: &mut Rng, arity: usize, own: impl Fn(&mut Rng) -> [f64; 2]) -> [f64; 2] {
    if rng.below(3) == 0 { [rng.bits(), if arity == 2 { rng.bits() } else { 0.0 }] } else { own(rng) }
}

fn random_angle(rng: &mut Rng) -> [f64; 2] {
    mixed(rng, 1, |r| [if r.next() & 1 == 0 { r.coordinate(9) } else { r.unit() * std::f64::consts::TAU }, 0.0])
}

fn random_cosine(rng: &mut Rng) -> [f64; 2] {
    mixed(rng, 1, |r| [if r.next() & 1 == 0 { 2.0 * r.unit() - 1.0 } else { r.magnitude(-60, -1) }, 0.0])
}

fn random_point(rng: &mut Rng) -> [f64; 2] {
    mixed(rng, 2, |r| {
        if r.next() & 1 == 0 {
            [r.coordinate(7), r.coordinate(7)]
        } else {
            [r.magnitude(-60, 60), r.magnitude(-60, 60)]
        }
    })
}

fn random_power(rng: &mut Rng) -> [f64; 2] {
    const EXPONENTS: [f64; 5] = [2.4, 1.0 / 2.4, 0.5, 2.0, -1.5];
    mixed(rng, 2, |r| match r.below(3) {
        0 => [r.unit(), EXPONENTS[usize::try_from(r.below(5)).expect("small")]],
        1 => [r.magnitude(-30, 30).abs(), (r.unit() - 0.5) * 20.0],
        _ => [r.magnitude(-30, 30), f64::from(i32::try_from(r.below(129)).expect("small") - 64)],
    })
}

fn random_positive(rng: &mut Rng) -> [f64; 2] {
    mixed(rng, 1, |r| [if r.next() & 1 == 0 { r.positive() } else { r.unit() * 10f64.powi(r.below(10) as i32) }, 0.0])
}

const FUNCTIONS: &[Function] = &[
    Function {
        js: "sin",
        arity: 1,
        ports: &[
            Port { name: "js::sin (V8's fdlibm)", f: |x, _| js::sin(x), same_on_x64: true },
            Port { name: "libm::sin (the port before)", f: |x, _| libm::sin(x), same_on_x64: false },
        ],
        edges: angle_edges,
        random: random_angle,
        split: None,
    },
    Function {
        js: "cos",
        arity: 1,
        ports: &[
            Port { name: "js::cos (V8's fdlibm)", f: |x, _| js::cos(x), same_on_x64: true },
            Port { name: "libm::cos (the port before)", f: |x, _| libm::cos(x), same_on_x64: false },
        ],
        edges: angle_edges,
        random: random_angle,
        split: None,
    },
    Function {
        js: "acos",
        arity: 1,
        ports: &[Port { name: "libm::acos (venn)", f: |x, _| libm::acos(x), same_on_x64: true }],
        edges: unit_edges,
        random: random_cosine,
        split: None,
    },
    Function {
        js: "asin",
        arity: 1,
        ports: &[Port { name: "libm::asin (block)", f: |x, _| libm::asin(x), same_on_x64: true }],
        edges: unit_edges,
        random: random_cosine,
        split: None,
    },
    Function {
        js: "atan2",
        arity: 2,
        ports: &[Port { name: "libm::atan2 (venn, block, edges)", f: libm::atan2, same_on_x64: true }],
        edges: grid_edges,
        random: random_point,
        split: Some(Split {
            inside: "y/x within ±2^60",
            outside: "y/x beyond ±2^60, where fdlibm and FreeBSD's msun part ways",
            holds: |[y, x]| (y / x).abs() <= libm::scalbn(1.0, 60),
        }),
    },
    Function {
        js: "pow",
        arity: 2,
        ports: &[
            Port { name: "js::pow (libm)", f: js::pow, same_on_x64: false },
            Port { name: "js::ten_to (gantt), for base 10", f: ten_or_pow, same_on_x64: false },
        ],
        edges: pow_edges,
        random: random_power,
        split: None,
    },
    Function {
        js: "power",
        arity: 2,
        ports: &[
            Port { name: "js::pow (libm)", f: js::pow, same_on_x64: false },
            Port { name: "f64::powf (palette's ink)", f: f64::powf, same_on_x64: false },
            Port { name: "js::ten_to (xychart), for base 10", f: ten_or_pow, same_on_x64: false },
        ],
        edges: pow_edges,
        random: random_power,
        split: None,
    },
    Function {
        js: "log10",
        arity: 1,
        ports: &[
            Port { name: "js::log10 (xychart, gantt, treemap)", f: |x, _| js::log10(x), same_on_x64: true },
            Port { name: "libm::log10 (gantt and treemap before)", f: |x, _| libm::log10(x), same_on_x64: false },
        ],
        edges: log10_edges,
        random: random_positive,
        split: None,
    },
    Function {
        js: "log2",
        arity: 1,
        ports: &[Port { name: "libm::log2 (packet)", f: |x, _| libm::log2(x), same_on_x64: true }],
        edges: log2_edges,
        random: random_positive,
        split: None,
    },
    Function {
        js: "hypot",
        arity: 2,
        ports: &[Port { name: "layout::num::hypot (V8's)", f: hypot, same_on_x64: true }],
        edges: grid_edges,
        random: random_point,
        split: None,
    },
    Function {
        js: "sqrt",
        arity: 1,
        ports: &[Port { name: "f64::sqrt", f: |x, _| x.sqrt(), same_on_x64: true }],
        edges: sqrt_edges,
        random: random_positive,
        split: None,
    },
];

/// `js::ten_to` for a power of ten, as xychart and gantt take one, and `js::pow` for the rest.
fn ten_or_pow(x: f64, y: f64) -> f64 {
    if x == 10.0 { js::ten_to(y) } else { js::pow(x, y) }
}

/// Equal bits, with every NaN one value: V8 may quiet or canonicalise a NaN's payload.
fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// How many doubles apart two answers are, or `None` when only one is NaN.
fn ulps(a: f64, b: f64) -> Option<u64> {
    let ordered = |x: f64| {
        let bits = x.to_bits() as i64;
        if bits < 0 { i64::MIN.wrapping_sub(bits) } else { bits }
    };
    (a.is_nan() == b.is_nan()).then(|| ordered(a).abs_diff(ordered(b)))
}

/// How often two sources of answers disagree: how many by a number of ulps, at most how many,
/// and how many with NaN against a number, with the first `keep` as examples.
#[derive(Default)]
struct Tally {
    apart: usize,
    furthest: u64,
    nan: usize,
    keep: usize,
    examples: Vec<String>,
}

impl Tally {
    fn keeping(keep: usize) -> Tally {
        Tally { keep, ..Tally::default() }
    }

    fn count(&mut self, a: f64, b: f64, example: impl FnOnce() -> String) {
        if same(a, b) {
            return;
        }
        match ulps(a, b) {
            Some(far) => {
                self.apart += 1;
                self.furthest = self.furthest.max(far);
            }
            None => self.nan += 1,
        }
        if self.examples.len() < self.keep {
            self.examples.push(example());
        }
    }

    fn differ(&self) -> usize {
        self.apart + self.nan
    }

    fn cell(&self) -> String {
        let apart = (self.apart > 0).then(|| format!("{} (≤ {} ulp)", self.apart, self.furthest));
        let nan = (self.nan > 0).then(|| format!("{} NaN against a number", self.nan));
        match (apart, nan) {
            (None, None) => "0".to_owned(),
            (Some(cell), None) | (None, Some(cell)) => cell,
            (Some(apart), Some(nan)) => format!("{apart}; {nan}"),
        }
    }
}

fn hex(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

fn seed() -> u64 {
    let given = std::env::var("INTEL_TRIG_SEED").unwrap_or_else(|_| "0x1e7e1".to_owned());
    let parsed = match given.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => given.parse(),
    };
    parsed.unwrap_or_else(|e| panic!("INTEL_TRIG_SEED={given} is not a number: {e}"))
}

fn samples() -> usize {
    let given = std::env::var("INTEL_TRIG_SAMPLES").unwrap_or_else(|_| "10000000".to_owned());
    given.parse().unwrap_or_else(|e| panic!("INTEL_TRIG_SAMPLES={given} is not a count: {e}"))
}

/// The function's answers for every input, as the Node computes them.
fn node_answers(node: &Node, flags: &[&str], function: &Function, inputs: &[[f64; 2]]) -> Vec<f64> {
    let script = pele_oracle::root().join("scripts/intel-trig.mjs");
    let bytes = node.run(flags, &script, &[function.js], |stdin| {
        let mut buffer = Vec::with_capacity(1 << 20);
        for chunk in inputs.chunks(1 << 16) {
            buffer.clear();
            for input in chunk {
                for x in &input[..function.arity] {
                    buffer.extend_from_slice(&x.to_le_bytes());
                }
            }
            stdin.write_all(&buffer).expect("node reads its inputs");
        }
    });
    assert_eq!(bytes.len(), inputs.len() * 8, "{} {} answered every input", node.arch, function.js);
    bytes.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect()
}

#[test]
#[ignore = "needs an x86-64 Node: run `mise run intel-trig`"]
fn the_maths_pele_calls_on_x64_and_arm64_node_and_in_the_port() {
    let [x64, arm64] = nodes();
    let (seed, count) = (seed(), samples());
    let mut table = String::new();
    let mut jitless = String::new();
    let mut examples = String::new();
    let mut broken = Vec::new();
    writeln!(
        table,
        "| function | port | inputs | x64 vs port | arm64 vs port | x64 vs arm64 |\n| --- | --- | --- | --- | --- | --- |"
    )
    .unwrap();
    writeln!(
        jitless,
        "| function | edge inputs | x64 jitless vs JIT | arm64 jitless vs JIT |\n| --- | --- | --- | --- |"
    )
    .unwrap();
    for (i, function) in FUNCTIONS.iter().enumerate() {
        let mut rng = Rng(seed ^ (i as u64).wrapping_mul(0xd6e8_feb8_6659_fd93));
        let groups = (function.edges)();
        let edges: Vec<[f64; 2]> = groups.iter().flat_map(|(_, inputs)| inputs.iter().copied()).collect();
        let mut cells = Vec::new();
        for node in [&x64, &arm64] {
            let jit = node_answers(node, &[], function, &edges);
            let interpreted = node_answers(node, &["--jitless"], function, &edges);
            let mut tally = Tally::keeping(5);
            for (k, (&a, &b)) in interpreted.iter().zip(&jit).enumerate() {
                tally.count(a, b, || {
                    format!("{} jitless {}: {} against {}", node.arch, hex(edges[k][0]), hex(a), hex(b))
                });
            }
            cells.push(tally.cell());
        }
        writeln!(jitless, "| {} | {} | {} | {} |", function.label(), edges.len(), cells[0], cells[1]).unwrap();
        let random: Vec<[f64; 2]> = (0..count).map(|_| (function.random)(&mut rng)).collect();
        let sampled = format!("{count} random, seed {seed:#x}");
        let random = match &function.split {
            None => vec![(sampled, random, true)],
            Some(split) => {
                let (inside, outside): (Vec<_>, Vec<_>) = random.into_iter().partition(|&input| (split.holds)(input));
                vec![
                    (format!("{sampled}: the {} with {}", inside.len(), split.inside), inside, true),
                    (format!("{sampled}: the {} with {}", outside.len(), split.outside), outside, false),
                ]
            }
        };
        let sets =
            groups.into_iter().map(|(name, inputs)| (format!("{name} ({})", inputs.len()), inputs, true)).chain(random);
        for (set, inputs, claimed) in &sets.collect::<Vec<_>>() {
            let [on_x64, on_arm64] = std::thread::scope(|s| {
                let x = s.spawn(|| node_answers(&x64, &[], function, inputs));
                let a = s.spawn(|| node_answers(&arm64, &[], function, inputs));
                [x.join().expect("x64 answers"), a.join().expect("arm64 answers")]
            });
            let keep = if set.contains("random") { 5 } else { 50 };
            let mut across = Tally::keeping(keep);
            let mut ports: Vec<[Tally; 2]> =
                function.ports.iter().map(|_| [Tally::keeping(keep), Tally::keeping(keep)]).collect();
            for (k, input) in inputs.iter().enumerate() {
                let [x, y] = *input;
                let (a, b) = (on_x64[k], on_arm64[k]);
                let args = || match function.arity {
                    1 => format!("{x:?} [{}]", hex(x)),
                    _ => format!("{x:?}, {y:?} [{}, {}]", hex(x), hex(y)),
                };
                across.count(a, b, || format!("{}({}): x64 {}, arm64 {}", function.js, args(), hex(a), hex(b)));
                for (port, [x64_tally, arm64_tally]) in function.ports.iter().zip(&mut ports) {
                    let ours = (port.f)(x, y);
                    let line = |node: &str, theirs: f64| {
                        format!("{}({}): {node} {}, {} {}", function.js, args(), hex(theirs), port.name, hex(ours))
                    };
                    x64_tally.count(a, ours, || line("x64", a));
                    arm64_tally.count(b, ours, || line("arm64", b));
                }
            }
            for (port, [x64_tally, arm64_tally]) in function.ports.iter().zip(&ports) {
                if port.same_on_x64 && *claimed && x64_tally.differ() > 0 {
                    broken.push(format!("{} {} on {set}: {}", function.label(), port.name, x64_tally.cell()));
                }
                writeln!(
                    table,
                    "| {} | {} | {set} | {} | {} | {} |",
                    function.label(),
                    port.name,
                    x64_tally.cell(),
                    arm64_tally.cell(),
                    across.cell()
                )
                .unwrap();
                for (pair, tally) in [("x64 vs port", x64_tally), ("arm64 vs port", arm64_tally)] {
                    for line in &tally.examples {
                        writeln!(examples, "{set} {pair}: {line}").unwrap();
                    }
                }
            }
            for line in &across.examples {
                writeln!(examples, "{set} x64 vs arm64: {line}").unwrap();
            }
            println!("{} {set}: {} inputs compared", function.js, inputs.len());
        }
    }
    let (ink, flipped) = ink_on_every_hex_colour(&x64, &arm64);
    if flipped > 0 {
        broken.push(format!("the port picks another ink than Pele for {flipped} #rrggbb colours"));
    }
    let out = out_dir();
    let header = format!(
        "# Pele's maths on x64 and arm64 Node {} and in the port\n\nx64: {}\narm64: {}\nseed {seed:#x}, {count} random inputs a function. Each cell counts the inputs whose answers differ in any bit (every NaN is one value), and how many doubles apart they are at most.\n\n",
        pele_oracle::node().expect("the pinned Node").version,
        x64.path.display(),
        arm64.path.display(),
    );
    let report = format!(
        "{header}{table}\n## The interpreter against the compilers\n\nEach Node again with `--jitless` over the edges, against its answers above.\n\n{jitless}\n{ink}"
    );
    std::fs::write(out.join("math.md"), &report).expect("math.md");
    std::fs::write(out.join("math-examples.txt"), &examples).expect("math-examples.txt");
    println!("{report}\nexamples of each difference in {}", out.join("math-examples.txt").display());
    assert!(broken.is_empty(), "what the decision log measured no longer holds:\n{}", broken.join("\n"));
}

/// Pele's `ink` picks the text colour on a fill by its luminance, from each channel raised to 2.4,
/// against the luminance where #1f1f1f and #fff contrast alike. The port raises them with
/// `js::pow` (`scene.rs`). A `#rrggbb` colour has 256 channel values, so every one of the
/// 16,777,216 colours is checked: does the port's luminance pick the ink each Node's does?
fn ink_on_every_hex_colour(x64: &Node, arm64: &Node) -> (String, usize) {
    let power = FUNCTIONS.iter().find(|function| function.js == "power").expect("`**` is compared");
    let raised: Vec<[f64; 2]> = (0..=255).map(|c| [(f64::from(c) / 255.0 + 0.055) / 1.055, 2.4]).collect();
    let linear = |pows: &[f64]| -> Vec<f64> {
        (0..=255)
            .map(|c| {
                let c = f64::from(c) / 255.0;
                if c <= 0.04045 { c / 12.92 } else { pows[(c * 255.0).round() as usize] }
            })
            .collect()
    };
    let ours = linear(&raised.iter().map(|&[x, y]| js::pow(x, y)).collect::<Vec<_>>());
    let luminance =
        |linear: &[f64], [r, g, b]: [usize; 3]| 0.2126 * linear[r] + 0.7152 * linear[g] + 0.0722 * linear[b];
    let even = |linear: &[f64]| {
        (1.05 * (0.2126 * linear[31] + 0.7152 * linear[31] + 0.0722 * linear[31] + 0.05)).sqrt() - 0.05
    };
    let mut total = 0;
    let mut section = "## Pele's ink on every #rrggbb\n\n| Node | channels raised apart from `js::pow` | colours whose ink the port picks otherwise | closest luminance to the turn |\n| --- | --- | --- | --- |\n".to_owned();
    for node in [x64, arm64] {
        let theirs = linear(&node_answers(node, &[], power, &raised));
        let apart = theirs.iter().zip(&ours).filter(|(a, b)| !same(**a, **b)).count();
        let (even_theirs, even_ours) = (even(&theirs), even(&ours));
        let (mut flipped, mut closest) = (0usize, f64::INFINITY);
        for r in 0..256 {
            for g in 0..256 {
                for b in 0..256 {
                    let theirs = luminance(&theirs, [r, g, b]);
                    flipped += usize::from((theirs > even_theirs) != (luminance(&ours, [r, g, b]) > even_ours));
                    closest = closest.min((theirs - even_theirs).abs());
                }
            }
        }
        writeln!(section, "| {} | {apart} of 256 | {flipped} of 16777216 | {closest:e} away |", node.arch).unwrap();
        total += flipped;
    }
    (section, total)
}

/// Every Mermaid source in `tests/fixtures`: each `.mmd`, and each string or `source` in a JSON
/// fixture's arrays. Layout recordings, which hold calls and no sources, are left out.
fn fixtures() -> Vec<(String, Vec<String>)> {
    let root = pele_oracle::root();
    let mut files = Vec::new();
    let mut folders = vec![root.join("tests/fixtures")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).expect("a fixture folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                folders.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut mmd = Vec::new();
    let mut sets = Vec::new();
    for path in files {
        let name = path.strip_prefix(&root).expect("under the root").display().to_string();
        match path.extension().and_then(|e| e.to_str()) {
            Some("mmd") => mmd.push(std::fs::read_to_string(&path).expect("a source")),
            Some("json") => {
                let json: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).expect("a fixture")).expect("JSON");
                let arrays: Vec<&serde_json::Value> = match &json {
                    serde_json::Value::Object(fields) => fields
                        .iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "pele" | "types"))
                        .map(|(_, v)| v)
                        .collect(),
                    array => vec![array],
                };
                let sources: Vec<String> = arrays
                    .into_iter()
                    .filter_map(serde_json::Value::as_array)
                    .flatten()
                    .filter_map(|item| item.as_str().or_else(|| item.get("source")?.as_str()))
                    .map(str::to_owned)
                    .collect();
                if !sources.is_empty() {
                    sets.push((name, sources));
                }
            }
            _ => {}
        }
    }
    sets.push(("tests/fixtures/**/*.mmd".to_owned(), mmd));
    sets
}

/// The venn source whose geometry libm and the pinned arm64 Node were seen to round apart
/// (`venn_layout`'s `the_unsettled_relaxation_differs_only_in_libms_last_bit`).
fn unsettled_venn() -> String {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    format!(
        "venn-beta\n{}{}",
        repeat(60, &|i| format!("set s{i}\n")),
        repeat(40, &|_| format!("union {}s59\n", repeat(59, &|i| format!("s{i},"))))
    )
}

/// One request line for the oracle; U+2028 and U+2029 end a line for Node, so they are escaped.
fn render_request(source: &str) -> String {
    let json =
        serde_json::to_string(source).expect("a string").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("render\t{json}")
}

fn pele_answers(node: &Node, requests: &[String]) -> Vec<String> {
    let script = pele_oracle::root().join("harness/dist/oracle.mjs");
    let body = requests.join("\n") + "\n";
    let out = node.run(&[], &script, &[], move |stdin| stdin.write_all(body.as_bytes()).expect("the oracle reads"));
    let answers: Vec<String> = String::from_utf8(out).expect("UTF-8").lines().map(str::to_owned).collect();
    assert_eq!(answers.len(), requests.len(), "{}'s oracle answered every request", node.arch);
    answers
}

#[test]
#[ignore = "needs an x86-64 Node: run `mise run intel-trig`"]
fn pele_draws_the_corpus_and_the_fixtures_on_x64_and_arm64_node() {
    let [x64, arm64] = nodes();
    let mut sets: Vec<(String, Vec<String>)> =
        pele_oracle::corpus().into_iter().map(|(name, sources)| (format!("corpus {name}"), sources)).collect();
    sets.extend(fixtures());
    let unsettled_name = "venn_layout's unsettled relaxation";
    sets.push((unsettled_name.to_owned(), vec![unsettled_venn()]));
    let mut rows = Vec::new();
    let mut differences = String::new();
    for (name, sources) in &sets {
        let requests: Vec<String> = sources.iter().map(|s| render_request(s)).collect();
        let [on_x64, on_arm64] = std::thread::scope(|s| {
            let x = s.spawn(|| pele_answers(&x64, &requests));
            let a = s.spawn(|| pele_answers(&arm64, &requests));
            [x.join().expect("x64 answers"), a.join().expect("arm64 answers")]
        });
        let mut row = [sources.len(), 0, 0, 0, 0, 0];
        for (i, source) in sources.iter().enumerate() {
            if on_x64[i] == on_arm64[i] {
                row[1] += 1;
                continue;
            }
            row[2] += 1;
            let svg = |answer: &str| match serde_json::from_str(answer) {
                Ok(PeleRender::Svg { svg, .. }) => Some(svg),
                _ => None,
            };
            let ours = pele_core::render(source).ok().map(|rendered| rendered.svg);
            let column = if ours.is_some() && ours == svg(&on_x64[i]) {
                3
            } else if ours.is_some() && ours == svg(&on_arm64[i]) {
                4
            } else {
                5
            };
            row[column] += 1;
            let start: String = source.chars().take(80).collect();
            writeln!(
                differences,
                "{name} #{i} ({}): {start:?}",
                ["", "", "", "port = x64", "port = arm64", "port = neither"][column]
            )
            .unwrap();
        }
        println!("{name}: {} sources, {} differ", row[0], row[2]);
        rows.push((name.clone(), row));
    }
    let sum = |rows: &mut dyn Iterator<Item = &[usize; 6]>| {
        rows.fold([0usize; 6], |mut total, row| {
            total.iter_mut().zip(row).for_each(|(t, n)| *t += n);
            total
        })
    };
    let types = rows.iter().filter(|(name, _)| name.starts_with("corpus")).count();
    let corpus = sum(&mut rows.iter().filter(|(name, _)| name.starts_with("corpus")).map(|(_, row)| row));
    let total = sum(&mut rows.iter().map(|(_, row)| row));
    let unsettled = sum(&mut rows.iter().filter(|(name, _)| name == unsettled_name).map(|(_, row)| row));
    let mut table = "| sources | inputs | Pele's answers alike | differ | the port draws as x64 | as arm64 | as neither |\n| --- | --- | --- | --- | --- | --- | --- |\n".to_owned();
    let mut line = |name: &str, row: &[usize; 6]| {
        writeln!(table, "| {name} | {} | {} | {} | {} | {} | {} |", row[0], row[1], row[2], row[3], row[4], row[5])
            .unwrap();
    };
    line(&format!("Pele's corpus, all {types} types"), &corpus);
    for (name, row) in &rows {
        if row[2] > 0 || !name.starts_with("corpus") {
            line(name, row);
        }
    }
    line("total", &total);
    let report = format!(
        "# Pele's answers on x64 and arm64 Node\n\nx64: {}\narm64: {}\nEvery answer of the oracle's `render`, SVG or error, compared byte for byte. A corpus type is listed on its own only where it differs.\n\n{table}",
        x64.path.display(),
        arm64.path.display()
    );
    let out = out_dir();
    std::fs::write(out.join("pele-svgs.md"), &report).expect("pele-svgs.md");
    std::fs::write(out.join("pele-svgs-differ.txt"), &differences).expect("pele-svgs-differ.txt");
    println!("{report}");
    assert_eq!(
        total[2] - unsettled[2],
        0,
        "Pele draws corpus or fixture sources apart on x64 and arm64 Node:\n{differences}"
    );
}
