//! Holds the swimlane layout port to Pele 1.13.0's `laneLayout`, double for double.
//!
//! `tests/fixtures/swimlane/lanes.json` holds the lane layout calls Pele made while it rendered the
//! swimlane harvest, written by `scripts/harvest-swimlane.mjs`. Each call's input goes through the
//! Rust port, and what the port leaves behind must have the same bits as what Pele left behind.
//! The other tests ask the oracle's `lane_run` op directly, for graphs no source holds.

use pele_core::layout::lanes::LANE_PAD;
use pele_core::layout::{Axis, CEdge, CNode, Dir, EdgeStyle, LayeredOptions, RoutePoint, lane_layout};
use serde_json::{Value, json};

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

fn index(value: &Value) -> Option<usize> {
    usize::try_from(value.as_i64().expect("an index or -1")).ok()
}

fn dir(name: &Value) -> Option<Dir> {
    match name.as_str() {
        None => None,
        Some("TB") => Some(Dir::TB),
        Some("BT") => Some(Dir::BT),
        Some("LR") => Some(Dir::LR),
        Some("RL") => Some(Dir::RL),
        Some(other) => panic!("{other} is not a direction"),
    }
}

fn dir_name(dir: Dir) -> &'static str {
    match dir {
        Dir::TB => "TB",
        Dir::BT => "BT",
        Dir::LR => "LR",
        Dir::RL => "RL",
    }
}

fn graph_in(graph: &Value) -> (Vec<CNode>, Vec<CEdge>) {
    let nodes = graph["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|c| {
            let n = unpack(&c["n"]);
            let mut node = CNode::new(n[0], n[1]);
            node.parent = index(&c["parent"]);
            node.is_group = c["group"].as_bool().expect("group");
            node.dir = dir(&c["dir"]);
            (node.pad_x, node.pad_top, node.pad_bottom, node.min_w) = (n[2], n[3], n[4], n[5]);
            (node.seq, node.span, node.x, node.y) = (n[6], n[7], n[8], n[9]);
            node
        })
        .collect();
    let edges = graph["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|e| {
            let n = unpack(&e["n"]);
            let points = unpack(&e["route"]);
            let axes = e["axes"].as_str().expect("axes");
            let mut edge = CEdge::new(index(&e["src"]).expect("src"), index(&e["dst"]).expect("dst"));
            (edge.minlen, edge.label_w, edge.label_h, edge.label_x, edge.label_y) = (n[0], n[1], n[2], n[3], n[4]);
            edge.route = axes
                .bytes()
                .enumerate()
                .map(|(i, axis)| RoutePoint {
                    x: points[2 * i],
                    y: points[2 * i + 1],
                    axis: if axis == b'0' { Axis::Vertical } else { Axis::Horizontal },
                })
                .collect();
            edge
        })
        .collect();
    (nodes, edges)
}

fn graph_out(nodes: &[CNode], edges: &[CEdge]) -> Value {
    json!({
        "nodes": nodes.iter().map(|c| json!({
            "parent": c.parent.map_or(-1, |p| p as i64),
            "group": c.is_group,
            "dir": c.dir.map(dir_name),
            "n": pack(&[c.w, c.h, c.pad_x, c.pad_top, c.pad_bottom, c.min_w, c.seq, c.span, c.x, c.y]),
        })).collect::<Vec<_>>(),
        "edges": edges.iter().map(|e| json!({
            "src": e.src,
            "dst": e.dst,
            "n": pack(&[e.minlen, e.label_w, e.label_h, e.label_x, e.label_y]),
            "route": pack(&e.route.iter().flat_map(|p| [p.x, p.y]).collect::<Vec<_>>()),
            "axes": e.route.iter().map(|p| if p.axis == Axis::Vertical { '0' } else { '1' }).collect::<String>(),
        })).collect::<Vec<_>>(),
    })
}

/// What the Rust port leaves behind for the input of one call.
fn replay(input: &Value) -> Value {
    let (mut nodes, mut edges) = graph_in(input);
    let n = unpack(&input["opt"]);
    let options = LayeredOptions {
        node_sep: n[0],
        edge_sep: n[1],
        rank_sep: n[2],
        port_sep: n[3],
        style: EdgeStyle::Curved,
        apart: false,
        head_room: 0.0,
    };
    let lane_pad = if input["lanePad"].is_null() { LANE_PAD } else { unpack(&input["lanePad"])[0] };
    let size = lane_layout(&mut nodes, &mut edges, dir(&input["dir"]).expect("a direction"), &options, lane_pad);
    let mut out = graph_out(&nodes, &edges);
    out["size"] = Value::String(pack(&[size.width, size.height]));
    out
}

/// The first place two outputs differ, by path, with both values there.
fn difference(path: &str, ours: &Value, theirs: &Value) -> Option<String> {
    match (ours, theirs) {
        (Value::Object(a), Value::Object(b)) => {
            b.iter().find_map(|(key, value)| difference(&format!("{path}.{key}"), &a[key], value))
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            a.iter().zip(b).enumerate().find_map(|(i, (a, b))| difference(&format!("{path}[{i}]"), a, b))
        }
        _ if ours == theirs => None,
        _ => Some(format!("{path}: ours {ours}, pele {theirs}")),
    }
}

/// Replays each call and lists those whose output differs from `out`.
fn check(calls: &[(String, Value, Value)]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|(name, input, theirs)| {
            let ours = std::panic::catch_unwind(|| replay(input));
            match ours {
                Ok(ours) => difference("out", &ours, theirs).map(|problem| format!("{name}: {problem}")),
                Err(_) => Some(format!("{name}: this port panics")),
            }
        })
        .collect()
}

#[test]
fn recorded_lane_calls_replay() {
    let path = pele_oracle::root().join("tests/fixtures/swimlane/lanes.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let recorded: Value = serde_json::from_str(&text).expect("the recording is JSON");
    let calls: Vec<(String, Value, Value)> = recorded["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .enumerate()
        .map(|(i, call)| (format!("recorded call {i}"), call["in"].clone(), call["out"].clone()))
        .collect();
    let failures = check(&calls);
    println!("recorded lane layout calls: {}, {} different", calls.len(), failures.len());
    assert!(calls.len() > 1000, "the recording holds the harvest's calls");
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        calls.len(),
        failures[..failures.len().min(10)].join("\n")
    );
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

    /// A size from `least` to `least + spread` whose mantissa is full, so that a sum taken in
    /// another order rounds differently.
    fn size(&mut self, least: f64, spread: usize) -> f64 {
        least + self.below(spread) as f64 + (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn node_value(c: &CNode) -> Value {
    json!({
        "parent": c.parent.map_or(-1, |p| p as i64),
        "group": c.is_group,
        "dir": Value::Null,
        "n": pack(&[c.w, c.h, c.pad_x, c.pad_top, c.pad_bottom, c.min_w, c.seq, c.span, c.x, c.y]),
    })
}

/// A lane layout input as the flowchart render builds one: lanes and groups with their padding,
/// nodes in them or loose, and edges between any two, with labels, lengths and self-loops.
fn random_graph(rng: &mut Rng, most: usize, most_edges: usize) -> Value {
    let mut nodes: Vec<CNode> = Vec::new();
    let group = |rng: &mut Rng, parent: Option<usize>| {
        let mut c = CNode::new(0.0, 0.0);
        c.is_group = true;
        c.parent = parent;
        let pad = if rng.chance(3) { 12.0 } else { 20.0 };
        c.pad_x = pad;
        c.pad_top = if rng.chance(3) { 20.0 } else { rng.size(30.0, 30) };
        c.pad_bottom = pad;
        c.min_w = rng.size(20.0, 200);
        c.span = f64::INFINITY;
        c
    };
    let lanes = rng.below(6);
    for _ in 0..lanes {
        let lane = group(rng, None);
        nodes.push(lane);
    }
    let groups = rng.below(4);
    for _ in 0..groups {
        let parents: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].is_group).collect();
        let parent = if parents.is_empty() || rng.chance(5) { None } else { Some(parents[rng.below(parents.len())]) };
        let inner = group(rng, parent);
        nodes.push(inner);
    }
    let groups_now: Vec<usize> = (0..nodes.len()).collect();
    for _ in 0..1 + rng.below(most) {
        let mut c = CNode::new(rng.size(20.0, 150), rng.size(20.0, 80));
        c.parent =
            if groups_now.is_empty() || rng.chance(8) { None } else { Some(groups_now[rng.below(groups_now.len())]) };
        c.span = c.w;
        nodes.push(c);
    }
    // Pele lists groups last-declared first, after the order they were made in here.
    for (i, c) in nodes.iter_mut().enumerate() {
        c.seq = i as f64;
    }
    let count = nodes.len();
    let mut edges = Vec::new();
    for _ in 0..rng.below(most_edges + 1) {
        let src = rng.below(count);
        let dst = if rng.chance(12) { src } else { rng.below(count) };
        let mut e = CEdge::new(src, dst);
        e.minlen = [1.0, 1.0, 1.0, 2.0, 3.0][rng.below(5)];
        if rng.chance(3) {
            e.label_w = rng.size(20.0, 120);
            e.label_h = rng.size(20.0, 40);
        }
        edges.push(e);
    }
    let dir = [Dir::TB, Dir::BT, Dir::LR, Dir::RL][rng.below(4)];
    let tight = rng.chance(3);
    json!({
        "nodes": nodes.iter().map(node_value).collect::<Vec<_>>(),
        "edges": graph_out(&[], &edges)["edges"],
        "dir": dir_name(dir),
        "opt": pack(&[if tight { 20.0 } else { 40.0 }, 16.0, if tight { 36.0 } else { 48.0 }, 20.0]),
        "lanePad": if tight { Value::String(pack(&[12.0])) } else { Value::Null },
    })
}

/// Asks the oracle what Pele's `laneLayout` leaves behind for each input, and replays each.
fn run_live(name: &str, inputs: Vec<Value>) -> usize {
    let requests: Vec<String> = inputs.iter().map(|input| format!("lane_run\t{}", json!({ "in": input }))).collect();
    let answers = pele_oracle::ask(&requests);
    let calls: Vec<(String, Value, Value)> = inputs
        .into_iter()
        .zip(&answers)
        .enumerate()
        .map(|(i, (input, answer))| {
            let theirs: Value = serde_json::from_str(answer).expect("the oracle answers JSON");
            (format!("{name} {i}"), input, theirs["out"].clone())
        })
        .collect();
    let failures = check(&calls);
    println!("{name}: {}, {} different", calls.len(), failures.len());
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        calls.len(),
        failures[..failures.len().min(10)].join("\n")
    );
    calls.len()
}

#[test]
fn random_lane_graphs_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let inputs: Vec<Value> = (0..3000).map(|_| random_graph(&mut rng, 14, 24)).collect();
    run_live("random lane graphs", inputs);
}

#[test]
fn large_lane_graphs_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let inputs: Vec<Value> = (0..6).map(|_| random_graph(&mut rng, 400, 900)).collect();
    run_live("large lane graphs", inputs);
}

/// Graphs at the edges: none, nothing but lanes, no lanes, a lane edge, and enough long edges
/// that Pele gives up on dummies.
#[test]
fn edge_case_lane_graphs_match_pele() {
    let lane = |min_w: f64| {
        let mut c = CNode::new(0.0, 0.0);
        c.is_group = true;
        (c.pad_x, c.pad_top, c.pad_bottom, c.min_w, c.span) = (20.0, 36.5, 20.0, min_w, f64::INFINITY);
        c
    };
    let node = |parent: Option<usize>| {
        let mut c = CNode::new(70.25, 42.5);
        c.parent = parent;
        c
    };
    let input = |nodes: Vec<CNode>, edges: Vec<CEdge>, dir: Dir| {
        json!({
            "nodes": nodes.iter().map(node_value).collect::<Vec<_>>(),
            "edges": graph_out(&[], &edges)["edges"],
            "dir": dir_name(dir),
            "opt": pack(&[40.0, 16.0, 48.0, 20.0]),
            "lanePad": Value::Null,
        })
    };
    let mut inputs = vec![
        input(vec![], vec![], Dir::TB),
        input(vec![lane(10.0), lane(300.0)], vec![CEdge::new(0, 1)], Dir::LR),
        input(vec![node(None), node(None)], vec![CEdge::new(0, 1), CEdge::new(1, 0), CEdge::new(0, 0)], Dir::BT),
        input(vec![lane(0.0), node(Some(0)), node(Some(0))], vec![CEdge::new(1, 0), CEdge::new(0, 2)], Dir::RL),
    ];
    let mut chain = vec![lane(0.0), lane(0.0)];
    chain.extend((0..1501).map(|_| node(Some(0))));
    chain.push(node(Some(1)));
    let mut edges: Vec<CEdge> = (0..1500).map(|i| CEdge::new(2 + i, 3 + i)).collect();
    for i in 0..40 {
        edges.push(CEdge::new(2, 2 + 1500 - i));
        edges.push(CEdge::new(1503, 2 + 1400 - i));
    }
    inputs.push(input(chain, edges, Dir::TB));
    run_live("edge case lane graphs", inputs);
}
