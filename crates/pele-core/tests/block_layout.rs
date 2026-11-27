//! Holds the block diagram's grid layout to Pele 1.13.0's `layout`, double for double.
//!
//! The oracle's `block_layout_record` op renders a source and answers every layout call the render
//! made, with the tree of cells it was given and the tree it left behind. Each call's input goes
//! through the Rust port, and what the port leaves behind must have the same bits as what Pele
//! left behind. The other test asks the oracle's `block_layout_run` op directly, for trees no
//! source makes.

#![expect(clippy::disallowed_methods, reason = "the test's own arithmetic, which no SVG reads")]

use pele_core::block_layout::{Cell, cell, layout};
use serde_json::{Value, json};
use std::collections::HashSet;

/// The doubles of a cell, in the order the oracle packs them.
const NUMBERS: usize = 12;

fn pack(values: &[f64]) -> String {
    values.iter().map(|value| format!("{:016x}", value.to_bits())).collect()
}

fn unpack(hex: &Value) -> Vec<f64> {
    let hex = hex.as_str().expect("packed doubles are a string");
    assert_eq!(hex.len() % 16, 0, "packed doubles come 16 hex digits each");
    (0..hex.len() / 16)
        .map(|i| f64::from_bits(u64::from_str_radix(&hex[16 * i..16 * i + 16], 16).expect("hex digits")))
        .collect()
}

fn tree_in(list: &Value) -> Vec<Cell> {
    list.as_array()
        .expect("a tree is a list of cells")
        .iter()
        .map(|item| {
            let n = unpack(&item["n"]);
            assert_eq!(n.len(), NUMBERS);
            Cell {
                children: item["children"]
                    .as_array()
                    .expect("children")
                    .iter()
                    .map(|child| usize::try_from(child.as_u64().expect("an index")).expect("an index"))
                    .collect(),
                space: item["space"].as_bool().expect("space"),
                columns: n[0],
                span: n[1],
                head: n[2],
                nw: n[3],
                nh: n[4],
                col: n[5],
                row: n[6],
                cols: n[7],
                x: n[8],
                y: n[9],
                w: n[10],
                h: n[11],
                heights: unpack(&item["heights"]),
                row_gaps: unpack(&item["rowGaps"]),
                col_gaps: unpack(&item["colGaps"]),
            }
        })
        .collect()
}

/// The tree as the oracle writes it. The port's arena is in breadth-first order already.
fn tree_out(cells: &[Cell]) -> Value {
    Value::Array(
        cells
            .iter()
            .map(|c| {
                let n = [c.columns, c.span, c.head, c.nw, c.nh, c.col, c.row, c.cols, c.x, c.y, c.w, c.h];
                json!({
                    "children": c.children,
                    "space": c.space,
                    "n": pack(&n),
                    "heights": pack(&c.heights),
                    "rowGaps": pack(&c.row_gaps),
                    "colGaps": pack(&c.col_gaps),
                })
            })
            .collect(),
    )
}

/// What the port makes of a recorded call: the same JSON as the oracle's.
fn replay(call: &Value) -> Value {
    let mut cells = tree_in(&call["in"]);
    let gap = unpack(&call["gap"])[0];
    let mut out = json!({ "in": call["in"].clone(), "gap": call["gap"].clone() });
    match layout(&mut cells, gap) {
        Ok((width, height)) => {
            out["out"] = tree_out(&cells);
            out["size"] = pack(&[width, height]).into();
        }
        Err(error) => out["error"] = format!("PeleError: {error}").into(),
    }
    out
}

fn compare(what: &str, calls: &[Value]) {
    let mut failures = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        let ours = std::panic::catch_unwind(|| replay(call));
        match ours {
            Ok(ours) if ours == *call => {}
            Ok(ours) => failures.push(format!("{what} {index}:\n    ours: {ours}\n    pele: {call}")),
            Err(_) => failures.push(format!("{what} {index}: this port panics on {}", call["in"])),
        }
    }
    let errors = calls.iter().filter(|call| call.get("error").is_some()).count();
    println!("{what}: {} calls, {} of them errors, {} different", calls.len(), errors, failures.len());
    assert!(
        failures.is_empty(),
        "{} of {} {what} differ:\n{}",
        failures.len(),
        calls.len(),
        failures[..failures.len().min(5)].join("\n")
    );
}

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/block/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// Sources whose edge labels crowd each other, so that the layout runs again with wider gaps.
const CROWDED: &[&str] = &[
    "block\n A space B\n A -- \"label one\" --> B\n A -- \"label two\" --> B\n A -- \"three\" --> B",
    "block\n columns 1\n A\n B\n A -- \"x\" --> B\n A -- \"y\" --> B\n B -- \"z\" --> A",
    "block\n columns 3\n A B C\n D E F\n A -- \"first label\" --> F\n A -- \"second label\" --> F\n C -- \"up\" --> D\n C -- \"down\" --> D",
    "block\n block:g\n  columns 2\n  A space B\n end\n A -- \"a long label\" --> B\n A -- \"another one\" --> B\n B -- \"back\" --> A",
    "block\n columns 2\n block:g\n  A\n end\n block:h\n  B\n end\n C D\n A -- \"one\" --> D\n A -- \"two\" --> D\n A -- \"three\" --> D",
    "block\n columns 1\n block:g\n  A B\n end\n block:h\n  C\n end\n A -- \"x\" --> C\n A -- \"y\" --> C\n A -- \"z\" --> C\n A -- \"w\" --> C",
    "block\n A B\n A -- \"x\" --> B\n A -- \"y\" --> B\n A -- \"z\" --> B",
    "block\n columns 2\n A space\n space B\n A -- \"diagonal one\" --> B\n A -- \"diagonal two\" --> B",
];

/// Every layout call Pele makes while it renders the harvest, its mutations and the crowded
/// sources, with room to spare and at 320 pixels, where a diagram that does not fit is laid out
/// again more tightly.
#[test]
fn recorded_calls_replay_bit_for_bit() {
    let mut requests = Vec::new();
    let crowded = CROWDED.iter().map(|&source| source.to_owned());
    for source in harvest("sources").into_iter().chain(harvest("mutated")).chain(crowded) {
        let source = serde_json::to_string(&source).expect("a string encodes");
        let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
        for options in ["{}", "{\"maxWidth\":320}"] {
            requests.push(format!("block_layout_record\t{source}\t{options}"));
        }
    }
    let mut seen = HashSet::new();
    let mut calls = Vec::new();
    for answer in pele_oracle::ask(&requests) {
        let answer: Value = serde_json::from_str(&answer).expect("the oracle answers JSON");
        for call in answer["calls"].as_array().expect("calls") {
            if seen.insert(call.to_string()) {
                calls.push(call.clone());
            }
        }
    }
    println!("{} distinct layout calls recorded", calls.len());
    assert!(calls.len() > 1000, "the harvest makes many layout calls");
    let gapped = calls
        .iter()
        .filter(|call| {
            call["in"].as_array().expect("cells").iter().any(|cell| {
                !cell["rowGaps"].as_str().unwrap_or("").is_empty() || !cell["colGaps"].as_str().unwrap_or("").is_empty()
            })
        })
        .count();
    println!("{gapped} of the recorded calls lay out a tree with gaps widened for edge labels");
    assert!(gapped > 10, "the harvest widens gaps for edge labels");
    compare("recorded layout calls", &calls);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    /// A size whose mantissa is full, so that a sum taken in another order rounds differently.
    fn size(&mut self, most: f64) -> f64 {
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        if self.chance(6) { (unit * most).round() } else { unit * most + 1e-9 }
    }

    fn gaps(&mut self) -> Vec<f64> {
        if !self.chance(3) {
            return Vec::new();
        }
        (0..1 + self.below(5)).map(|_| if self.chance(3) { 0.0 } else { (self.size(60.0)).ceil() }).collect()
    }
}

/// A random tree of cells in breadth-first order, as a render builds it.
fn random_tree(rng: &mut Rng) -> Vec<Cell> {
    let mut cells = vec![cell(0.0, 0.0, 1.0)];
    let mut queue = vec![(0, 0)];
    let budget = 2 + rng.below(40);
    while let Some((at, depth)) = (!queue.is_empty()).then(|| queue.remove(0)) {
        let container = at == 0 || (depth < 4 && rng.chance(3));
        if !container {
            continue;
        }
        cells[at].columns = match rng.below(8) {
            0 if rng.chance(20) => 0.0,
            0..=3 => -1.0,
            _ => (1 + rng.below(5)) as f64,
        };
        if at > 0 {
            cells[at].head = if rng.chance(2) { rng.size(30.0) + 4.0 } else { 0.0 };
            cells[at].nw = rng.size(200.0);
        }
        cells[at].row_gaps = rng.gaps();
        cells[at].col_gaps = rng.gaps();
        let count = if cells.len() >= budget { 0 } else { rng.below(7) };
        for _ in 0..count {
            let span = match rng.below(10) {
                0 => 0.0,
                1 | 2 => (2 + rng.below(4)) as f64,
                3 if rng.chance(10) => 5000.0,
                _ => 1.0,
            };
            let mut child = cell(rng.size(300.0), rng.size(120.0), span);
            child.space = rng.chance(6);
            let index = cells.len();
            cells.push(child);
            cells[at].children.push(index);
            queue.push((index, depth + 1));
        }
    }
    cells
}

#[test]
fn random_trees_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut inputs = Vec::new();
    for _ in 0..3000 {
        let cells = random_tree(&mut rng);
        let gap = match rng.below(4) {
            0 => 0.0,
            1 => 6.0,
            2 => 12.0,
            _ => rng.size(40.0),
        };
        inputs.push(json!({ "in": tree_out(&cells), "gap": pack(&[gap]) }));
    }
    let requests: Vec<String> = inputs.iter().map(|input| format!("block_layout_run\t{input}")).collect();
    let calls: Vec<Value> =
        pele_oracle::ask(&requests).iter().map(|answer| serde_json::from_str(answer).expect("JSON")).collect();
    assert!(calls.iter().any(|call| call.get("error").is_some()), "some trees have a column count of 0");
    compare("random layout trees", &calls);
}
