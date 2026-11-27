//! Differential tests for the two parts of treemap that are numbers alone, before anything is
//! drawn: Pele's squarified layout, every box's place and size as the bits of its doubles, through
//! the oracle's `treemap_layout` op; and Pele's value format, byte for byte, through
//! `treemap_format`.

#![expect(clippy::disallowed_methods, reason = "the test's own arithmetic, which no SVG reads")]

#[path = "differential/harness.rs"]
mod harness;

use harness::{Rng, bits};
use pele_core::treemap_probe::{Node, Placed, format, layout};
use serde_json::{Value, json};

fn node_json(node: &Node) -> Value {
    match node {
        Node::Section { name, children } => {
            json!({ "name": name, "children": children.iter().map(node_json).collect::<Vec<_>>() })
        }
        Node::Leaf { name, value } => json!({ "name": name, "value": bits(*value) }),
    }
}

fn placed_json(placed: &Placed) -> Value {
    json!({
        "name": placed.name,
        "parent": placed.parent,
        "series": placed.series,
        "value": bits(placed.value),
        "headed": placed.headed,
        "header": bits(placed.header),
        "x": bits(placed.x),
        "y": bits(placed.y),
        "w": bits(placed.w),
        "h": bits(placed.h),
    })
}

/// A tree and the rectangle and insets it is laid out with.
struct Call {
    roots: Vec<Node>,
    width: f64,
    height: f64,
    insets: [f64; 4],
}

/// Lays out each call through both engines and counts the boxes compared.
fn compare(what: &str, calls: &[Call]) -> usize {
    let requests: Vec<String> = calls
        .iter()
        .map(|call| {
            let [gap, header, short, side] = call.insets;
            let input = json!({
                "roots": call.roots.iter().map(node_json).collect::<Vec<_>>(),
                "width": bits(call.width),
                "height": bits(call.height),
                "gap": bits(gap),
                "header": bits(header),
                "short": bits(short),
                "side": bits(side),
            });
            format!("treemap_layout\t{input}")
        })
        .collect();
    let answers = pele_oracle::ask(&requests);
    let mut boxes = 0;
    let mut failures = Vec::new();
    for (index, (call, theirs)) in calls.iter().zip(&answers).enumerate() {
        let ours =
            Value::Array(layout(&call.roots, call.width, call.height, call.insets).iter().map(placed_json).collect());
        let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
        boxes += theirs.as_array().map_or(0, Vec::len);
        if ours != theirs {
            let pairs = ours.as_array().into_iter().flatten().zip(theirs.as_array().into_iter().flatten());
            let first = pairs.enumerate().find(|(_, (a, b))| a != b);
            failures.push(format!(
                "call {index} ({} by {} with {:?}): {} boxes, pele {}; first different box {first:?}",
                call.width,
                call.height,
                call.insets,
                ours.as_array().map_or(0, Vec::len),
                theirs.as_array().map_or(0, Vec::len)
            ));
        }
    }
    println!("{what}: {} calls, {boxes} boxes, {} different", calls.len(), failures.len());
    assert!(
        failures.is_empty(),
        "{} of {} {what} differ from Pele:\n{}",
        failures.len(),
        calls.len(),
        failures[..failures.len().min(8)].join("\n")
    );
    boxes
}

fn leaf(name: &str, value: f64) -> Node {
    Node::Leaf { name: name.to_owned(), value }
}

fn section(name: &str, children: Vec<Node>) -> Node {
    Node::Section { name: name.to_owned(), children }
}

const VALUES: &[f64] =
    &[1.0, 2.0, 3.0, 5.0, 10.0, 100.0, 0.5, 0.25, 1.5, 7.3, 0.0001, 99_999_999.0, 0.0, -1.0, 1e-9, 12_345.678, 1e300];

fn value(rng: &mut Rng) -> f64 {
    match rng.below(20) {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => rng.unit() * 1e6,
        3 => (rng.unit() * 1e4).floor(),
        _ => VALUES[rng.below(VALUES.len())],
    }
}

fn tree(rng: &mut Rng, depth: usize) -> Node {
    if depth == 0 || rng.chance(45) {
        return leaf(["a", "b", "c", "leaf"][rng.below(4)], value(rng));
    }
    let count = if rng.chance(4) { 30 + rng.below(40) } else { rng.below(8) };
    section(["s", "t", "Section"][rng.below(3)], (0..count).map(|_| tree(rng, depth - 1)).collect())
}

fn random_call(rng: &mut Rng) -> Call {
    let count = rng.below(10);
    let roots = (0..count)
        .map(|_| {
            let depth = 1 + rng.below(4);
            tree(rng, depth)
        })
        .collect();
    let pick = |rng: &mut Rng, list: &[f64]| list[rng.below(list.len())];
    let width = pick(rng, &[640.0, 200.0, 320.0, 10.0, 4000.0, 401.0, 333.5]);
    let height = pick(rng, &[400.0, 10.0, 30.0, 4000.0, 123.0, 0.0]);
    let gap = pick(rng, &[2.0, 3.0, 4.0, 0.0, 1.0, 6.0, 20.0, 2.5, 200.0]);
    let side = pick(rng, &[3.0, 4.0, 5.0, 6.0, 0.0, 30.0]);
    let header = if rng.chance(80) { 28.0 } else { pick(rng, &[0.0, 10.0, 100.0]) };
    let short = if rng.chance(80) { 18.0 } else { pick(rng, &[0.0, 5.0, 40.0]) };
    Call { roots, width, height, insets: [gap, header, short, side] }
}

#[test]
fn random_trees_lay_out_as_pele_lays_them_out() {
    let mut rng = Rng(0x7ee3_1a70_0000_0003);
    let calls: Vec<Call> = (0..3000).map(|_| random_call(&mut rng)).collect();
    let boxes = compare("random treemap layouts", &calls);
    assert!(boxes > 100_000, "the random trees place boxes");
}

/// The shapes of Pele's worst cases and of the trees that reach each branch of the layout.
#[test]
fn hard_trees_lay_out_as_pele_lays_them_out() {
    let defaults = [2.0, 28.0, 18.0, 3.0];
    let call = |roots: Vec<Node>| Call { roots, width: 640.0, height: 400.0, insets: defaults };
    let mut deep = leaf("leaf", 1.0);
    for i in (0..300).rev() {
        deep = section(&format!("s{i}"), vec![deep]);
    }
    let calls = vec![
        call((1..=5000).map(|i| leaf("l", f64::from(i))).collect()),
        call(vec![section("s", (0..5000).map(|_| leaf("l", 1.0)).collect())]),
        call((0..5000).map(|i| leaf("l", if i % 2 == 1 { 0.0001 } else { 99_999_999.0 })).collect()),
        call((0..3000).map(|i| section(&format!("s{i}"), vec![leaf("l", f64::from(i + 1))])).collect()),
        call(vec![deep]),
        call(vec![leaf("a", 1e308), leaf("b", 1e308)]),
        call(vec![section("s", vec![leaf("a", 1e308), leaf("b", 1e308)]), leaf("c", 1.0)]),
        call(vec![leaf("a", 1.0), leaf("b", 1.0), leaf("c", 1.0)]),
        call(vec![section("empty", Vec::new()), section("zero", vec![leaf("z", 0.0)]), leaf("x", 2.0)]),
        call(vec![leaf("big", 1000.0), leaf("small", 4.0)]),
        call(vec![section("A", vec![section("B", vec![leaf("c", 1.0), leaf("d", 2.0)]), leaf("e", 3.0)])]),
        Call { roots: vec![leaf("a", 1.0)], width: 0.0, height: 400.0, insets: defaults },
        Call {
            roots: vec![section("s", vec![leaf("a", 1.0), leaf("b", 2.0)])],
            width: 40.0,
            height: 30.0,
            insets: defaults,
        },
    ];
    compare("hard treemap layouts", &calls);
}

/// Formats each value with each specifier through both engines.
fn compare_formats(what: &str, cases: &[(String, f64)]) {
    let requests: Vec<String> = cases
        .iter()
        .map(|(spec, value)| format!("treemap_format\t{}\t{}", harness::json_line(spec), bits(*value)))
        .collect();
    let answers = pele_oracle::ask(&requests);
    let failures: Vec<String> = cases
        .iter()
        .zip(&answers)
        .filter_map(|((spec, value), theirs)| {
            let ours = format(spec, *value);
            (ours != *theirs)
                .then(|| format!("{spec:?} of {value:e} ({}): ours {ours:?}, pele {theirs:?}", bits(*value)))
        })
        .collect();
    println!("{what}: {} inputs, {} different", cases.len(), failures.len());
    assert!(
        failures.is_empty(),
        "{} of {} {what} differ from Pele:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(12)].join("\n")
    );
}

const SPECS: &[&str] = &[
    ",",
    "",
    "$0,0",
    "$,",
    "$,.2f",
    ",.2f",
    ".0f",
    ".1f",
    ".20f",
    ".25f",
    ",d",
    "d",
    "$d",
    "+d",
    "+,.3f",
    "-,",
    "(,",
    " ,",
    ".1%",
    "%",
    ",.0%",
    ".3e",
    "e",
    ".0e",
    ",e",
    ".20e",
    "~",
    ",.2~f",
    ".3~e",
    ".6~",
    "~%",
    ".2",
    ".0",
    ".21",
    ".1g",
    "s",
    "x",
    "F",
    ".3s",
    "<,",
    ">10,.2f",
    "*^10.1f",
    "^",
    "0,",
    "010d",
    "x<,",
    "\n<,",
    "<<",
    "<1<",
    "1<",
    ".,",
    ",.",
    "$$",
    "<b>",
    "0000000000000000000000005f",
    ".0000000000000000000000005f",
    ".99999999999999999999999f",
    "%%",
    "+$,.1%",
];

const FORMAT_VALUES: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    1.5,
    2.5,
    -2.5,
    0.125,
    0.375,
    1.005,
    1234.0,
    1234.5678,
    999_999.5,
    1e21,
    1e-7,
    1.2e-7,
    123_456_789.125,
    9.995,
    99.5,
    0.000_001,
    1e300,
    -1e300,
    5e-324,
    2.2250738585072014e-308,
    0.1,
    0.2,
    0.3,
    1.0 / 3.0,
    2.0 / 3.0,
    700_000.0,
    200_000.0,
    12_345_678_901_234_567_000.0,
    4.35,
    8.675,
    0.045,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::MAX,
    f64::MIN_POSITIVE,
    1e15,
    1e16,
    123e18,
    0.000_005,
];

#[test]
fn value_formats_match_pele() {
    let cases: Vec<(String, f64)> =
        SPECS.iter().flat_map(|spec| FORMAT_VALUES.iter().map(|&value| ((*spec).to_owned(), value))).collect();
    compare_formats("treemap value formats", &cases);
}

#[test]
fn random_value_formats_match_pele() {
    let mut rng = Rng(0xf0a7_0000_7ee3_0005);
    let parts: [&[&str]; 9] = [
        &["", "", "", "<", ">", "^", "=", "x<", "*>", "0="],
        &["", "", "", "+", "-", "(", " "],
        &["", "", "$"],
        &["", "", "0"],
        &["", "", "", "8", "12", "1"],
        &["", ",", ","],
        &["", "", ".0", ".1", ".2", ".3", ".6", ".12", ".17", ".20", ".22"],
        &["", "", "~"],
        &["", "", "f", "%", "d", "e", "g", "s", "r", "%"],
    ];
    let mut cases = Vec::new();
    for _ in 0..20_000 {
        let spec: String = parts.iter().map(|part| rng.pick(part)).collect();
        let exponent = rng.below(60) as i32 - 30;
        let mantissa = rng.unit() * 10.0;
        let value = match rng.below(10) {
            0 => FORMAT_VALUES[rng.below(FORMAT_VALUES.len())],
            1 => (mantissa * 1000.0).round() / 8.0,
            2 => -(mantissa * 10f64.powi(exponent)),
            3 => f64::from_bits(rng.next()),
            _ => mantissa * 10f64.powi(exponent),
        };
        cases.push((spec, value));
    }
    compare_formats("random treemap value formats", &cases);
}
