//! The venn geometry on its own, before anything is scaled or drawn: the circles Pele places,
//! which circles touch, the spot each region's text goes to and each overlap's outline must come
//! out bit for bit as the oracle's `venn_geometry` and `venn_region` ops answer, against
//! `pele_core::venn_probe`. The layout also calls `Math.acos` and `Math.atan2`, which come from
//! libm here, so their gap to the pinned Node is measured too.

use pele_core::venn_probe::{self, TrigCall};
use pele_oracle::PeleRender;
use serde_json::Value;
use std::collections::HashMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/venn/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn double(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).expect("the oracle answers 16 hex digits"))
}

fn request(op: &str, source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    format!("{op}\t{}", source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"))
}

/// Node's answer to each call, asked of the oracle's `sin`, `cos`, `acos` and `atan2` ops.
fn ask_node(calls: &[TrigCall], answers: &mut HashMap<TrigCall, f64>) {
    let requests: Vec<String> = calls
        .iter()
        .map(|call| {
            format!("{}\t{}", call.op, call.args.iter().map(|&x| format!("{x:016x}")).collect::<Vec<_>>().join("\t"))
        })
        .collect();
    for (call, answer) in calls.iter().zip(pele_oracle::ask(&requests)) {
        answers.insert(call.clone(), double(&answer));
    }
}

/// Each source's geometry with every `Math` call the layout makes answered by Node. A call
/// that is only made once an earlier one has Node's answer is asked in the next round.
fn geometries_with_node_trig(sources: &[&String]) -> Vec<Option<String>> {
    let mut answers = HashMap::new();
    loop {
        let mut missed: Vec<TrigCall> = Vec::new();
        let geometries: Vec<Option<String>> = sources
            .iter()
            .map(|source| {
                let (geometry, calls) = venn_probe::geometry_answered(source, &answers);
                missed.extend(calls);
                geometry
            })
            .collect();
        if missed.is_empty() {
            return geometries;
        }
        missed.sort_by(|a, b| (a.op, &a.args).cmp(&(b.op, &b.args)));
        missed.dedup();
        ask_node(&missed, &mut answers);
    }
}

/// Puts each source through `venn_geometry` on both sides. A source that is no venn diagram on
/// one side must be none on the other. A geometry that differs must come out bit for bit as
/// Pele's once the layout is given Node's own `Math` answers: the port's arithmetic is then
/// Pele's, and only libm's last bit tells them apart. Answers how many were bit-equal as they
/// are, and how many only with Node's answers.
fn geometries(what: &str, sources: &[String]) -> (usize, usize) {
    let requests: Vec<String> = sources.iter().map(|source| request("venn_geometry", source)).collect();
    let answers = pele_oracle::ask(&requests);
    let (mut equal, mut failures) = (0, Vec::new());
    let mut differing: Vec<(usize, &String, &String)> = Vec::new();
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let drawn = theirs.starts_with("{\"circles\":");
        match venn_probe::geometry(source) {
            Some(ours) if ours == *theirs => equal += 1,
            None if !drawn => {}
            Some(_) => differing.push((index, source, theirs)),
            None => failures.push(format!("{what} {index}: this port has no geometry\n    source: {source:?}")),
        }
    }
    let replayed = geometries_with_node_trig(&differing.iter().map(|(_, source, _)| *source).collect::<Vec<_>>());
    for ((index, source, theirs), ours) in differing.iter().zip(&replayed) {
        let ours = ours.as_deref().unwrap_or("");
        if ours != theirs.as_str() {
            let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(0);
            let around = |text: &str| text[at.saturating_sub(60).min(text.len())..(at + 60).min(text.len())].to_owned();
            failures.push(format!(
                "{what} {index}: byte {at}, with Node's Math answers\n    ours: {}\n    pele: {}\n    source: {source:?}",
                around(ours),
                around(theirs)
            ));
        }
    }
    println!(
        "{what}: {} inputs, {equal} geometries bit-equal to Pele, {} bit-equal only with Node's Math answers, {} different",
        sources.len(),
        differing.len() - failures.len().min(differing.len()),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} {what} differ from Pele:\n{}",
        failures.len(),
        sources.len(),
        failures[..failures.len().min(10)].join("\n")
    );
    (equal, differing.len())
}

#[test]
fn harvested_geometry_matches_pele() {
    let (equal, trig) = geometries("harvested venn sources", &harvest("sources"));
    assert!(equal > 500, "the harvest holds venn diagrams Pele lays out");
    assert_eq!(trig, 0, "libm's last bit changes no harvested geometry");
    let (_, trig) = geometries("mutated venn sources", &harvest("mutated"));
    assert_eq!(trig, 0, "libm's last bit changes no mutated geometry");
}

/// A small seeded generator, so a failure names an input that can be made again.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A size as a source writes it: whole, short, or with every digit a double holds.
fn size(rng: &mut Rng) -> String {
    match rng.below(6) {
        0 => format!("{}", 1 + rng.below(40)),
        1 => format!("{}.{}", rng.below(30), rng.below(100)),
        2 => format!("{:.17}", rng.unit() * 50.0),
        3 => format!("{:.17}", rng.unit() * rng.unit() * 3.0),
        4 => ["0", "-1", "0.0001", "100000", "2.5"][rng.below(5)].to_owned(),
        _ => format!("{:.12}", 5.0 + rng.unit()),
    }
}

/// A random diagram of sets and the overlaps between them, from one set to a crowd, so that
/// each way of placing circles is taken: one, two and three sets exactly, a ring that relaxes,
/// and a grid past 64 sets.
fn random_diagram(rng: &mut Rng) -> String {
    let n = match rng.below(20) {
        0 => 1,
        1..=3 => 2,
        4..=8 => 3,
        9..=14 => 4 + rng.below(5),
        15..=18 => 9 + rng.below(20),
        _ => 60 + rng.below(12),
    };
    let mut src = String::from("venn-beta\n");
    for i in 0..n {
        let sized = rng.below(3) > 0;
        src.push_str(&format!("set S{i}{}\n", if sized { format!(":{}", size(rng)) } else { String::new() }));
    }
    let unions = if n > 20 { rng.below(n) } else { rng.below(2 * n + 2) };
    for _ in 0..unions {
        if n < 2 {
            break;
        }
        let count = if n >= 3 && rng.below(4) == 0 { 3 + rng.below(n.min(5) - 2) } else { 2 };
        let mut members: Vec<usize> = Vec::new();
        while members.len() < count {
            let pick = rng.below(n);
            if !members.contains(&pick) {
                members.push(pick);
            }
        }
        let ids: Vec<String> = members.iter().map(|i| format!("S{i}")).collect();
        let sized = rng.below(3) > 0;
        src.push_str(&format!(
            "union {}{}\n",
            ids.join(","),
            if sized { format!(":{}", size(rng)) } else { String::new() }
        ));
    }
    src
}

#[test]
fn random_geometry_matches_pele() {
    let mut rng = Rng(0x0dd5_eed5_9e77_1a5e);
    let sources: Vec<String> = (0..2000).map(|_| random_diagram(&mut rng)).collect();
    let (equal, trig) = geometries("random venn diagrams", &sources);
    assert!(equal + trig > 1950, "random diagrams parse");
}

/// Circles that cross, touch, nest and coincide, so that every way an outline is closed is taken:
/// corners found and joined by arcs or lines, one circle inside the others, and nothing shared.
fn random_circles(rng: &mut Rng) -> Vec<[f64; 3]> {
    let count = 2 + rng.below(5);
    let spread = [0.5, 1.0, 2.0, 4.0][rng.below(4)];
    (0..count)
        .map(|_| match rng.below(8) {
            0 => [0.0, 0.0, 1.0],
            1 => [rng.unit() * 0.1, 0.0, 1.0 + rng.unit()],
            _ => [rng.unit() * spread - spread / 2.0, rng.unit() * spread - spread / 2.0, 0.3 + rng.unit() * 2.0],
        })
        .collect()
}

#[test]
fn random_outlines_match_pele() {
    let mut rng = Rng(0x0e71_15e5_0fc1_c1c1);
    let cases: Vec<Vec<[f64; 3]>> = (0..20000).map(|_| random_circles(&mut rng)).collect();
    let requests: Vec<String> = cases
        .iter()
        .map(|circles| {
            let list: Vec<String> = circles
                .iter()
                .map(|[x, y, r]| format!("[\"{}\",\"{}\",\"{}\"]", bits(*x), bits(*y), bits(*r)))
                .collect();
            format!("venn_region\t[{}]", list.join(","))
        })
        .collect();
    let answers = pele_oracle::ask(&requests);
    let (mut empty, mut lines, mut failures) = (0, 0, Vec::new());
    for (circles, theirs) in cases.iter().zip(&answers) {
        let ours = venn_probe::region(circles);
        empty += usize::from(theirs.is_empty());
        lines += usize::from(theirs.contains('L'));
        if ours != *theirs {
            failures.push(format!("{circles:?}\n    ours: {ours}\n    pele: {theirs}"));
        }
    }
    println!(
        "random circles: {} outlines, {empty} with nothing shared, {lines} with a straight side, {} different",
        cases.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} outlines of random circles differ from Pele:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
    assert!(empty > 100, "some circles share nothing");
}

/// How many of `ours` are bit-equal to what Node answers, and which are further than one ulp.
/// The pinned Node computes `Math.acos` and `Math.atan2` with V8's fdlibm port, built with fused
/// multiply-adds on arm64, so libm's answer is the neighbouring double on a few inputs.
fn measure(op: &str, inputs: &[Vec<f64>], ours: impl Fn(&[f64]) -> f64) {
    let requests: Vec<String> = inputs
        .iter()
        .map(|args| format!("{op}\t{}", args.iter().map(|&x| bits(x)).collect::<Vec<_>>().join("\t")))
        .collect();
    let answers = pele_oracle::ask(&requests);
    let (mut exact, mut far) = (0, Vec::new());
    for (args, theirs) in inputs.iter().zip(&answers) {
        let (got, want) = (ours(args), double(theirs));
        if got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()) {
            exact += 1;
        } else if !(got == want.next_up() || got == want.next_down()) {
            far.push(format!("{op}{args:?}: node {want:e}, ours {got:e}"));
        }
    }
    println!("{op}: {exact} of {} bit-equal to Node, {} further than one ulp", inputs.len(), far.len());
    assert!(far.is_empty(), "{}", far[..far.len().min(20)].join("\n"));
}

#[test]
fn acos_matches_math_acos() {
    let mut rng = Rng(0x00ac_05ac_05ac_05ac);
    let mut inputs: Vec<Vec<f64>> =
        [-1.0, 1.0, 0.0, -0.0, 0.5, -0.5, 1.0 + f64::EPSILON, f64::NAN, 1e-300].iter().map(|&x| vec![x]).collect();
    for _ in 0..200_000 {
        let x = rng.unit() * 2.0 - 1.0;
        inputs.push(vec![x]);
        inputs.push(vec![1.0 - rng.unit() * 1e-6]);
        inputs.push(vec![(rng.unit() - 0.5) * 1e-3]);
    }
    measure("acos", &inputs, |args| venn_probe::acos(args[0]));
}

#[test]
fn atan2_matches_math_atan2() {
    let mut rng = Rng(0x0a7a_2a7a_2a7a_2a7a);
    let mut inputs: Vec<Vec<f64>> = Vec::new();
    for y in [0.0, -0.0, 1.0, -1.0] {
        for x in [0.0, -0.0, 1.0, -1.0, f64::INFINITY] {
            inputs.push(vec![y, x]);
        }
    }
    for _ in 0..200_000 {
        inputs.push(vec![rng.unit() * 4.0 - 2.0, rng.unit() * 4.0 - 2.0]);
        inputs.push(vec![(rng.unit() - 0.5) * 300.0, (rng.unit() - 0.5) * 300.0]);
        inputs.push(vec![(rng.unit() - 0.5) * 1e-9, rng.unit() - 0.5]);
    }
    measure("atan2", &inputs, |args| venn_probe::atan2(args[0], args[1]));
}

/// The one source known to draw differently: 40 unions of the same 60 sets, the shape of one of
/// Pele's worst cases (`tests/venn/stress.test.ts`). Every pair of circles is asked to overlap,
/// which no placement can give them, so 300 rounds of relaxation never settle and carry libm's
/// last bit into what is drawn. Its geometry is Pele's bit for bit once the layout is given
/// Node's own `Math` answers. When libm's answers are made Node's, this test fails and the source
/// moves to the worst cases of `venn_parity.rs`.
#[test]
fn the_unsettled_relaxation_differs_only_in_libms_last_bit() {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let source = format!(
        "venn-beta\n{}{}",
        repeat(60, &|i| format!("set s{i}\n")),
        repeat(40, &|_| format!("union {}s59\n", repeat(59, &|i| format!("s{i},"))))
    );
    let theirs = pele_oracle::ask(&[request("venn_geometry", &source)]).remove(0);
    let ours = venn_probe::geometry(&source).expect("a venn diagram");
    assert_ne!(ours, theirs, "libm's last bit changes this geometry");
    let replayed = geometries_with_node_trig(&[&source]).remove(0);
    assert_eq!(replayed.as_deref(), Some(theirs.as_str()), "with Node's Math answers the geometry is Pele's");
    let ours = pele_core::render(&source).expect("a venn diagram renders");
    let PeleRender::Svg { svg, .. } = &pele_oracle::render_all(&[source]).remove(0) else { panic!("Pele renders it") };
    assert_ne!(&ours.svg, svg, "the difference reaches the drawing");
    println!(
        "40 unions of 60 sets: geometry and drawing differ with libm, geometry is Pele's with Node's Math answers"
    );
}
