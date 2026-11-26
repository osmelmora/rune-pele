//! Holds the layout port to Pele 1.13.0, double for double.
//!
//! `tests/fixtures/layout/recorded.json` holds the layout calls Pele made while it rendered its
//! own corpus and test sources, written by `scripts/record-layout.mjs`. Each call's input goes
//! through the Rust port, and what the port leaves behind must have the same bits as what Pele
//! left behind. The other tests ask the oracle directly, for graphs the corpus does not hold.
//!
//! A call travels as `{ fn, in, out }`, in the shape `harness/oracle.ts` gives it under
//! "compound layout": doubles packed as 16 hex digits each.

#![expect(clippy::disallowed_methods, reason = "the test's own arithmetic, which no SVG reads")]

use pele_core::layout::layered::{Groups, Kind, LEdge, LNode, layered};
use pele_core::layout::num::hypot;
use pele_core::layout::{
    Axis, CEdge, CNode, Dir, EdgeStyle, LayeredOptions, RoutePoint, Size, compound_layout, inherit_crossed_directions,
    shift_layout, wrap_components,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const NODE: [&str; 10] = ["w", "h", "padX", "padTop", "padBottom", "minW", "seq", "span", "x", "y"];
const EDGE: [&str; 5] = ["minlen", "labelW", "labelH", "labelX", "labelY"];
const LNODE: [&str; 9] = ["w", "h", "pin", "hint", "key", "span", "x", "y", "rank"];
const LEDGE: [&str; 7] = ["minlen", "labelW", "labelH", "tailDx", "headDx", "labelX", "labelY"];

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
            CNode {
                w: n[0],
                h: n[1],
                parent: index(&c["parent"]),
                is_group: c["group"].as_bool().expect("group"),
                dir: dir(&c["dir"]),
                pad_x: n[2],
                pad_top: n[3],
                pad_bottom: n[4],
                min_w: n[5],
                seq: n[6],
                span: n[7],
                x: n[8],
                y: n[9],
            }
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
            let route = axes
                .bytes()
                .enumerate()
                .map(|(i, axis)| RoutePoint {
                    x: points[2 * i],
                    y: points[2 * i + 1],
                    axis: if axis == b'0' { Axis::Vertical } else { Axis::Horizontal },
                })
                .collect();
            CEdge {
                src: index(&e["src"]).expect("src"),
                dst: index(&e["dst"]).expect("dst"),
                minlen: n[0],
                label_w: n[1],
                label_h: n[2],
                route,
                label_x: n[3],
                label_y: n[4],
            }
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

fn with(mut object: Value, key: &str, value: Value) -> Value {
    object[key] = value;
    object
}

fn size_out(size: Option<Size>) -> Value {
    size.map_or(Value::Null, |size| Value::String(pack(&[size.width, size.height])))
}

fn options(opt: &Value) -> LayeredOptions {
    let n = unpack(&opt["n"]);
    let flag = |name: &str| opt[name].as_bool().expect("a flag");
    LayeredOptions {
        node_sep: n[0],
        edge_sep: n[1],
        rank_sep: n[2],
        port_sep: n[3],
        style: if flag("tracks") {
            EdgeStyle::Stepped
        } else if flag("straight") {
            EdgeStyle::Linear
        } else {
            EdgeStyle::Curved
        },
        apart: flag("apart"),
        head_room: if opt["headRoom"].is_null() { 0.0 } else { unpack(&opt["headRoom"])[0] },
    }
}

fn options_out(opt: &LayeredOptions) -> Value {
    json!({
        "n": pack(&[opt.node_sep, opt.edge_sep, opt.rank_sep, opt.port_sep]),
        "headRoom": pack(&[opt.head_room]),
        "tracks": opt.style == EdgeStyle::Stepped,
        "straight": opt.style == EdgeStyle::Linear,
        "apart": opt.apart,
        "orderOnly": false,
        "groups": null,
    })
}

/// What the Rust port leaves behind for the input of one call.
fn replay(name: &str, input: &Value) -> Value {
    let (mut nodes, mut edges) =
        graph_in(&json!({ "nodes": input["nodes"], "edges": input.get("edges").cloned().unwrap_or(json!([])) }));
    let by = input.get("by").map(unpack).unwrap_or_default();
    match name {
        "compoundLayout" => {
            let size = compound_layout(
                &mut nodes,
                &mut edges,
                dir(&input["dir"]).expect("a root direction"),
                &options(&input["opt"]),
            );
            with(graph_out(&nodes, &edges), "size", size_out(Some(size)))
        }
        "shiftLayout" => {
            shift_layout(&mut nodes, &mut edges, by[0], by[1]);
            graph_out(&nodes, &edges)
        }
        "inheritCrossedDirections" => {
            let ends: Vec<_> =
                input["ends"].as_array().expect("ends").iter().map(|pair| (index(&pair[0]), index(&pair[1]))).collect();
            inherit_crossed_directions(&mut nodes, ends);
            graph_out(&nodes, &[])
        }
        "wrapComponents" => {
            let size = wrap_components(&mut nodes, &mut edges, by[0], by[1]);
            with(graph_out(&nodes, &edges), "size", size_out(size))
        }
        other => panic!("{other} is not a layout function this test replays"),
    }
}

fn kind_of(code: &Value) -> Kind {
    match code.as_u64() {
        Some(0) => Kind::Node,
        Some(1) => Kind::Dummy,
        Some(2) => Kind::Label,
        Some(3) => Kind::StartPort,
        Some(4) => Kind::EndPort,
        _ => panic!("{code} is not a node kind"),
    }
}

/// What the Rust `layered` leaves behind for the input of one of Pele's `layered` calls.
fn replay_layered(input: &Value) -> Value {
    let mut nodes: Vec<LNode> = input["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|v| {
            let n = unpack(&v["n"]);
            LNode {
                w: n[0],
                h: n[1],
                kind: kind_of(&v["kind"]),
                pin: n[2] as i32,
                hint: n[3],
                key: n[4],
                span: n[5],
                x: n[6],
                y: n[7],
                rank: n[8] as i32,
            }
        })
        .collect();
    let mut edges: Vec<LEdge> = input["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|e| {
            let n = unpack(&e["n"]);
            let flag = |name: &str| e[name].as_bool().expect("a flag");
            LEdge {
                tail: index(&e["tail"]).expect("tail"),
                head: index(&e["head"]).expect("head"),
                minlen: n[0],
                label_w: n[1],
                label_h: n[2],
                tail_dx: n[3],
                head_dx: n[4],
                tail_fixed: flag("tailFixed"),
                head_fixed: flag("headFixed"),
                reversed: flag("reversed"),
                points: unpack(&e["points"]).chunks(2).map(|pair| [pair[0], pair[1]]).collect(),
                label_x: n[5],
                label_y: n[6],
            }
        })
        .collect();
    let opt = &input["opt"];
    let ints = |list: &Value| {
        list.as_array().expect("a list").iter().map(|item| item.as_i64().expect("an integer") as i32).collect()
    };
    let groups = (!opt["groups"].is_null())
        .then(|| Groups { of: ints(&opt["groups"]["of"]), parent: ints(&opt["groups"]["parent"]) });
    let result =
        layered(&mut nodes, &mut edges, &options(opt), groups.as_ref(), opt["orderOnly"].as_bool().expect("orderOnly"));
    json!({
        "nodes": nodes.iter().map(|v| json!({
            "kind": v.kind as u8,
            "n": pack(&[v.w, v.h, f64::from(v.pin), v.hint, v.key, v.span, v.x, v.y, f64::from(v.rank)]),
        })).collect::<Vec<_>>(),
        "edges": edges.iter().map(|e| json!({
            "tail": e.tail,
            "head": e.head,
            "reversed": e.reversed,
            "tailFixed": e.tail_fixed,
            "headFixed": e.head_fixed,
            "n": pack(&[e.minlen, e.label_w, e.label_h, e.tail_dx, e.head_dx, e.label_x, e.label_y]),
            "points": pack(&e.points.iter().flatten().copied().collect::<Vec<_>>()),
        })).collect::<Vec<_>>(),
        "size": pack(&[result.width, result.height]),
        "crossings": result.crossings.map(|(crossings, _)| crossings),
        "declared": result.crossings.map(|(_, declared)| declared),
    })
}

/// The name of the `at`th double packed under `key`.
fn packed_name(path: &str, key: &str, at: usize) -> String {
    let names: &[&str] = match key {
        "n" if path.contains("layered") && path.contains("nodes") => &LNODE,
        "n" if path.contains("layered") => &LEDGE,
        "n" if path.contains("nodes") => &NODE,
        "n" => &EDGE,
        "size" => &["width", "height"],
        "route" | "points" => return format!("{key}[{}].{}", at / 2, if at.is_multiple_of(2) { "x" } else { "y" }),
        _ => return format!("{key}[{at}]"),
    };
    names[at].to_owned()
}

/// Where two answers first differ, with what each holds there, or none if they are the same.
fn first_difference(path: &str, ours: &Value, pele: &Value) -> Option<String> {
    match (ours, pele) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, theirs) in b {
                let Some(mine) = a.get(key) else { return Some(format!("{path}.{key}: ours has none, pele {theirs}")) };
                if let (Some(mine), Some(theirs)) = (mine.as_str(), theirs.as_str())
                    && key != "axes"
                {
                    if mine == theirs {
                        continue;
                    }
                    let (mine, theirs) = (unpack(&json!(mine)), unpack(&json!(theirs)));
                    if mine.len() != theirs.len() {
                        return Some(format!("{path}.{key}: ours holds {} numbers, pele {}", mine.len(), theirs.len()));
                    }
                    let at = (0..mine.len())
                        .find(|&i| mine[i].to_bits() != theirs[i].to_bits())
                        .expect("the strings differ");
                    return Some(format!(
                        "{path}.{}: ours {:?} ({:016x}), pele {:?} ({:016x})",
                        packed_name(path, key, at),
                        mine[at],
                        mine[at].to_bits(),
                        theirs[at],
                        theirs[at].to_bits()
                    ));
                }
                if let Some(found) = first_difference(&format!("{path}.{key}"), mine, theirs) {
                    return Some(found);
                }
            }
            None
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!("{path}: ours holds {} items, pele {}", a.len(), b.len()));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .find_map(|(i, (mine, theirs))| first_difference(&format!("{path}[{i}]"), mine, theirs))
        }
        _ => (ours != pele).then(|| format!("{path}: ours {ours}, pele {pele}")),
    }
}

/// One oracle request per call: `<op>\t<json>`, with the line ends JSON leaves bare escaped.
fn request(op: &str, call: &Value) -> String {
    format!("{op}\t{}", call.to_string().replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"))
}

fn ask(requests: &[String]) -> Vec<Value> {
    pele_oracle::ask(requests).iter().map(|line| serde_json::from_str(line).expect("the oracle answers JSON")).collect()
}

/// For a `compoundLayout` call that differs, the first `layered` call inside it whose result
/// differs from Pele's on the same input, which is where to look.
fn first_layered_difference(input: &Value) -> String {
    let answer = &ask(&[request("layout_run", &json!({ "fn": "compoundLayout", "in": input, "trace": true }))])[0];
    for (i, call) in answer["layered"].as_array().expect("a trace").iter().enumerate() {
        if let Some(found) = first_difference(&format!("layered[{i}]"), &replay_layered(&call["in"]), &call["out"]) {
            return found;
        }
    }
    "every layered call matches on Pele's own inputs, so the difference is in the compound layer".to_owned()
}

#[derive(Default)]
struct Tally {
    calls: BTreeMap<String, (usize, usize)>,
    findings: Vec<String>,
}

impl Tally {
    fn check(&mut self, label: &str, name: &str, input: &Value, pele: &Value) {
        let entry = self.calls.entry(format!("{label} {name}")).or_default();
        entry.0 += 1;
        match first_difference("out", &replay(name, input), pele) {
            None => entry.1 += 1,
            Some(found) => self.findings.push(format!("{label} {name}: {found}")),
        }
    }

    fn report(&self) -> String {
        let mut out = String::from(" calls  identical  type and function\n");
        for (key, (calls, identical)) in &self.calls {
            out.push_str(&format!("{calls:6}  {identical:9}  {key}\n"));
        }
        let (calls, identical) = self.calls.values().fold((0, 0), |sum, entry| (sum.0 + entry.0, sum.1 + entry.1));
        out.push_str(&format!("{calls:6}  {identical:9}  total\n"));
        out
    }

    fn finish(self) {
        println!("{}", self.report());
        assert!(
            self.findings.is_empty(),
            "{} calls differ from Pele:\n{}\n{}",
            self.findings.len(),
            self.findings[..self.findings.len().min(20)].join("\n"),
            self.report()
        );
    }
}

fn recorded() -> Vec<Value> {
    let path = pele_oracle::root().join("tests/fixtures/layout/recorded.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let fixture: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let pin = std::fs::read_to_string(pele_oracle::root().join("scripts/pele-pin.txt")).expect("the Pele pin");
    assert_eq!(
        fixture["pele"].as_str(),
        Some(pin.trim()),
        "recorded at another Pele pin; rerun scripts/record-layout.mjs"
    );
    fixture["calls"].as_array().expect("calls").clone()
}

#[test]
fn recorded_calls_replay_bit_identically() {
    let mut tally = Tally::default();
    for call in recorded() {
        let name = call["fn"].as_str().expect("fn");
        tally.check(call["type"].as_str().expect("type"), name, &call["in"], &call["out"]);
    }
    assert!(tally.calls.len() >= 7, "the fixture holds every type's calls");
    tally.finish();
}

struct Rng(u32);

impl Rng {
    /// Pele's `random` from `tests/support/corpus.ts`.
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        f64::from(self.0) / 4294967296.0
    }
    fn int(&mut self, n: usize) -> usize {
        (self.unit() * n as f64).floor() as usize
    }
    /// A size between `from` and `from + spread` that fills its mantissa, as text metrics do.
    /// A value with a short mantissa adds without rounding, and then the order of a sum cannot show.
    fn size(&mut self, from: f64, spread: f64) -> f64 {
        from + (self.unit() + self.unit() / 4294967296.0) * spread
    }
}

/// A graph after Pele's `randomGraph` in `tests/layout/compound.test.ts`: up to four groups,
/// some nested and some with a direction of their own, leaves in and out of them, and edges
/// between any two nodes, groups included. Unlike Pele's, sizes are not whole numbers.
fn random_graph(seed: u32, scale: usize) -> (Vec<CNode>, Vec<CEdge>, Dir) {
    const DIRS: [Dir; 4] = [Dir::TB, Dir::BT, Dir::LR, Dir::RL];
    let mut rnd = Rng(seed);
    let mut nodes: Vec<CNode> = Vec::new();
    let mut groups: Vec<usize> = Vec::new();
    let group_count = rnd.int(5);
    for g in 0..group_count {
        let mut node = CNode::new(0.0, 0.0);
        node.parent = if !groups.is_empty() && rnd.unit() < 0.4 { Some(groups[rnd.int(groups.len())]) } else { None };
        node.is_group = true;
        node.pad_x = 20.0;
        node.pad_top = rnd.size(16.0, 30.0);
        node.pad_bottom = 20.0;
        node.min_w = rnd.size(40.0, 90.0);
        node.dir = if rnd.unit() < 0.3 { Some(DIRS[rnd.int(4)]) } else { None };
        node.seq = (1000 + g) as f64;
        if seed.is_multiple_of(2) {
            node.span = f64::INFINITY;
        }
        groups.push(nodes.len());
        nodes.push(node);
    }
    let leaves = (3 + rnd.int(30)) * scale;
    for i in 0..leaves {
        let parent = if !groups.is_empty() && rnd.unit() < 0.5 { Some(groups[rnd.int(groups.len())]) } else { None };
        let mut node = CNode::new(rnd.size(30.0, 120.0), rnd.size(30.0, 50.0));
        node.parent = parent;
        node.seq = i as f64;
        if seed.is_multiple_of(2) {
            node.span = node.w.min(node.h) - 24.0;
        }
        nodes.push(node);
    }
    let mut edges: Vec<CEdge> = Vec::new();
    let count = rnd.int(leaves * 2);
    for _ in 0..count {
        let labelled = rnd.unit() < 0.3;
        let mut edge = CEdge::new(rnd.int(nodes.len()), rnd.int(nodes.len()));
        edge.label_w = if labelled { rnd.size(20.0, 60.0) } else { 0.0 };
        edge.label_h = if labelled { rnd.size(19.2, 20.0) } else { 0.0 };
        if rnd.unit() < 0.1 {
            edge.minlen = (1 + rnd.int(3)) as f64;
        }
        edges.push(edge);
    }
    (nodes, edges, DIRS[rnd.int(4)])
}

/// The option sets the seven renderers pass, and the routing styles a flowchart can ask for.
fn option_sets() -> Vec<(&'static str, LayeredOptions)> {
    let base = LayeredOptions {
        node_sep: 40.0,
        edge_sep: 16.0,
        rank_sep: 48.0,
        port_sep: 20.0,
        style: EdgeStyle::Curved,
        apart: false,
        head_room: 0.0,
    };
    let tall_cards = 11.199999999999996;
    vec![
        ("plain", base.clone()),
        ("head room", LayeredOptions { head_room: 7.0, ..base.clone() }),
        ("stepped", LayeredOptions { style: EdgeStyle::Stepped, head_room: 7.0, ..base.clone() }),
        ("linear", LayeredOptions { style: EdgeStyle::Linear, head_room: 7.0, ..base.clone() }),
        ("apart", LayeredOptions { apart: true, rank_sep: 56.0 + tall_cards, ..base.clone() }),
        ("tight", LayeredOptions { node_sep: 20.0, rank_sep: 36.0, head_room: 7.0, ..base }),
    ]
}

/// One graph to lay out, and the label its result is tallied under.
struct Case {
    label: String,
    nodes: Vec<CNode>,
    edges: Vec<CEdge>,
    dir: Dir,
    opt: LayeredOptions,
}

fn case(label: &str, nodes: Vec<CNode>, edges: Vec<CEdge>, dir: Dir, opt: &LayeredOptions) -> Case {
    Case { label: label.to_owned(), nodes, edges, dir, opt: opt.clone() }
}

/// Lays `cases` out through Pele and through the port, and tallies them under their labels.
fn against_pele(cases: Vec<Case>) -> Tally {
    let inputs: Vec<Value> = cases
        .iter()
        .map(|case| {
            with(
                with(graph_out(&case.nodes, &case.edges), "dir", json!(dir_name(case.dir))),
                "opt",
                options_out(&case.opt),
            )
        })
        .collect();
    let requests: Vec<String> =
        inputs.iter().map(|input| request("layout_run", &json!({ "fn": "compoundLayout", "in": input }))).collect();
    let mut tally = Tally::default();
    for ((case, input), answer) in cases.iter().zip(&inputs).zip(ask(&requests)) {
        let before = tally.findings.len();
        tally.check(&case.label, "compoundLayout", input, &answer["out"]);
        if tally.findings.len() > before && before == 0 {
            tally.findings.push(format!("  first layered call that differs: {}", first_layered_difference(input)));
        }
    }
    tally
}

/// `PELE_LAYOUT_SEEDS` sets how many random graphs each option set gets, for a longer run.
#[test]
fn random_graphs_match_pele() {
    let seeds: u32 =
        std::env::var("PELE_LAYOUT_SEEDS").map_or(300, |count| count.parse().expect("PELE_LAYOUT_SEEDS is a count"));
    let mut graphs = Vec::new();
    for (name, opt) in option_sets() {
        for seed in 1..=seeds {
            let (nodes, edges, dir) = random_graph(seed, 1);
            graphs.push(case(&format!("random, {name}"), nodes, edges, dir, &opt));
        }
        for seed in 1..=seeds / 25 {
            let (nodes, edges, dir) = random_graph(1_000_000 + seed, 8);
            graphs.push(case(&format!("random and large, {name}"), nodes, edges, dir, &opt));
        }
    }
    against_pele(graphs).finish();
}

/// Graphs past the sizes at which Pele changes method (more than 600 nodes with their bends, 400
/// edges across one gap, 20,000 bends, 500 edges, 40,000 group borders crossed), and graphs
/// whose groups are not a tree.
#[test]
fn graphs_at_the_edges_match_pele() {
    let leaf = |i: usize| {
        let mut node = CNode::new(40.0 + (i % 7) as f64 * 9.0, 30.0 + (i % 3) as f64 * 8.0);
        node.seq = i as f64;
        node.span = 20.0;
        node
    };
    let plain = option_sets().remove(1).1;
    let mut graphs = Vec::new();

    let nodes: Vec<CNode> = (0..700).map(leaf).collect();
    let edges: Vec<CEdge> = (0..699).map(|i| CEdge::new(i / 3, i + 1)).collect();
    graphs.push(case("a tree of 700", nodes, edges, Dir::TB, &plain));

    let nodes: Vec<CNode> = (0..42).map(leaf).collect();
    let mut edges = Vec::new();
    for i in 0..21 {
        for j in 21..42 {
            edges.push(CEdge::new(i, j));
        }
    }
    for style in [EdgeStyle::Curved, EdgeStyle::Stepped] {
        let opt = LayeredOptions { style, ..plain.clone() };
        graphs.push(case("441 edges across one gap", nodes.clone(), edges.clone(), Dir::LR, &opt));
    }

    let nodes: Vec<CNode> = (0..260).map(leaf).collect();
    let mut edges: Vec<CEdge> = (0..259).map(|i| CEdge::new(i, i + 1)).collect();
    for i in 0..90 {
        let mut edge = CEdge::new(i % 5, 255 - i % 4);
        edge.label_w = 30.5 + i as f64;
        edge.label_h = 19.2;
        edges.push(edge);
    }
    graphs.push(case("more than 20,000 bends", nodes, edges, Dir::TB, &plain));

    let mut nodes: Vec<CNode> = (0..60).map(leaf).collect();
    for g in 0..6 {
        let mut group = CNode::new(0.0, 0.0);
        group.is_group = true;
        group.pad_x = 20.0;
        group.pad_top = 40.0;
        group.pad_bottom = 20.0;
        group.seq = 1000.0 + g as f64;
        group.span = f64::INFINITY;
        nodes.push(group);
    }
    for (i, node) in nodes.iter_mut().take(60).enumerate() {
        node.parent = Some(60 + i % 6);
    }
    let edges: Vec<CEdge> = (0..520).map(|i| CEdge::new(i % 60, (i * 7 + 13) % 60)).collect();
    graphs.push(case("520 edges between groups", nodes, edges, Dir::TB, &plain));

    let group = |seq: f64| {
        let mut group = CNode::new(0.0, 0.0);
        group.is_group = true;
        group.pad_x = 20.0;
        group.pad_top = 40.0;
        group.pad_bottom = 20.0;
        group.seq = seq;
        group.span = f64::INFINITY;
        group
    };
    let mut nodes: Vec<CNode> = (0..120).map(|g| group(1000.0 + g as f64)).collect();
    for (g, node) in nodes.iter_mut().enumerate().skip(1) {
        node.parent = Some(g - 1);
    }
    nodes.extend((0..31).map(|i| leaf(200 + i)));
    nodes[120].parent = Some(119);
    let edges: Vec<CEdge> = (0..400).map(|i| CEdge::new(121 + i % 30, 120)).collect();
    graphs.push(case("48,000 group borders crossed", nodes, edges, Dir::TB, &plain));

    let mut nodes = vec![group(10.0), group(11.0), group(12.0), leaf(0), leaf(1), leaf(2), leaf(3), leaf(4)];
    nodes[0].parent = Some(1);
    nodes[1].parent = Some(0);
    nodes[2].parent = Some(2);
    nodes[3].parent = Some(0);
    nodes[4].parent = Some(1);
    nodes[5].parent = Some(7);
    nodes[6].parent = Some(900);
    let edges = vec![
        CEdge::new(3, 4),
        CEdge::new(4, 5),
        CEdge::new(5, 6),
        CEdge::new(0, 3),
        CEdge::new(6, 6),
        CEdge::new(7, 2),
    ];
    for dir in [Dir::TB, Dir::BT, Dir::LR, Dir::RL] {
        graphs.push(case(
            "groups that hold each other, and parents that are no group",
            nodes.clone(),
            edges.clone(),
            dir,
            &plain,
        ));
    }
    graphs.push(case("no nodes", Vec::new(), Vec::new(), Dir::TB, &plain));
    graphs.push(case("one node", vec![leaf(0)], vec![CEdge::new(0, 0)], Dir::LR, &plain));

    against_pele(graphs).finish();
}

/// Random graphs once laid out, then wrapped into rows at a third and at twice their width.
#[test]
fn wrapped_random_graphs_match_pele() {
    let opt = option_sets().remove(0).1;
    let mut inputs: Vec<Value> = Vec::new();
    for seed in 1..=200 {
        let (mut nodes, mut edges, dir) = random_graph(2_000_000 + seed, 1);
        let size = compound_layout(&mut nodes, &mut edges, dir, &opt);
        for share in [1.0 / 3.0, 2.0] {
            inputs.push(with(graph_out(&nodes, &edges), "by", json!(pack(&[size.width * share, opt.node_sep]))));
        }
    }
    let requests: Vec<String> =
        inputs.iter().map(|input| request("layout_run", &json!({ "fn": "wrapComponents", "in": input }))).collect();
    let mut tally = Tally::default();
    for (input, answer) in inputs.iter().zip(ask(&requests)) {
        let label = if answer["out"]["size"].is_null() { "random, left as it was" } else { "random, wrapped" };
        tally.check(label, "wrapComponents", input, &answer["out"]);
    }
    assert_eq!(tally.calls.len(), 2, "some graphs wrap and some do not");
    tally.finish();
}

#[test]
fn hypot_matches_math_hypot() {
    let mut rng = Rng(0x9e37_79b9);
    let mut pairs = vec![
        (0.0, 0.0),
        (-0.0, 0.0),
        (f64::NAN, 1.0),
        (f64::INFINITY, f64::NAN),
        (3.0, 4.0),
        (1e-320, 1e-320),
        (1e300, 1e300),
    ];
    for _ in 0..40_000 {
        let scale = 10f64.powi(rng.int(9) as i32 - 6);
        pairs.push(((rng.unit() - 0.5) * scale, (rng.unit() - 0.5) * scale));
        pairs.push(((rng.unit() - 0.5) * 30.0, (rng.unit() - 0.5) * 1e-3));
    }
    let requests: Vec<String> =
        pairs.iter().map(|(x, y)| format!("hypot\t{:016x}\t{:016x}", x.to_bits(), y.to_bits())).collect();
    let differ: Vec<String> = pairs
        .iter()
        .zip(pele_oracle::ask(&requests))
        .filter(|((x, y), pele)| {
            format!("{:016x}", hypot(*x, *y).to_bits()) != **pele
                && !(hypot(*x, *y).is_nan() && f64::from_bits(u64::from_str_radix(pele, 16).expect("hex")).is_nan())
        })
        .map(|((x, y), pele)| format!("hypot({x:e}, {y:e}): ours {:016x}, node {pele}", hypot(*x, *y).to_bits()))
        .collect();
    assert!(
        differ.is_empty(),
        "{} of {} differ:\n{}",
        differ.len(),
        pairs.len(),
        differ[..differ.len().min(10)].join("\n")
    );
}

/// The largest recorded graph, laid out by the port and by Pele in Node. Run with
/// `cargo test --release -p pele-core --test layout_replay -- --ignored --nocapture timing`.
#[test]
#[ignore = "prints a timing; it asserts nothing"]
fn timing_of_the_largest_recorded_graph() {
    let calls = recorded();
    let size = |call: &Value| {
        call["in"]["nodes"].as_array().map_or(0, Vec::len) + call["in"]["edges"].as_array().map_or(0, Vec::len)
    };
    let largest = calls
        .iter()
        .filter(|call| call["fn"] == "compoundLayout")
        .max_by_key(|call| size(call))
        .expect("a recorded layout");
    let (nodes, edges, dir, opt) = {
        let (nodes, edges) = graph_in(&largest["in"]);
        (nodes, edges, dir(&largest["in"]["dir"]).expect("a direction"), options(&largest["in"]["opt"]))
    };
    let (big_nodes, big_edges, big_dir) = random_graph(1003, 8);
    let big = with(with(graph_out(&big_nodes, &big_edges), "dir", json!(dir_name(big_dir))), "opt", options_out(&opt));
    const RUNS: usize = 2000;
    let time = |nodes: &[CNode], edges: &[CEdge], dir: Dir| -> u128 {
        let mut ns: Vec<u128> = (0..RUNS)
            .map(|_| {
                let (mut nodes, mut edges) = (nodes.to_vec(), edges.to_vec());
                let start = std::time::Instant::now();
                compound_layout(&mut nodes, &mut edges, dir, &opt);
                start.elapsed().as_nanos()
            })
            .collect();
        ns.sort_unstable();
        ns[RUNS / 2]
    };
    let node = ask(&[
        format!("{}\t{RUNS}", request("layout_time", &json!({ "fn": "compoundLayout", "in": largest["in"] }))),
        format!("{}\t{RUNS}", request("layout_time", &json!({ "fn": "compoundLayout", "in": big }))),
    ]);
    let median = |answer: &Value| {
        let mut ns: Vec<u64> =
            answer["ns"].as_array().expect("ns").iter().map(|n| n.as_u64().expect("nanoseconds")).collect();
        ns.sort_unstable();
        ns[ns.len() / 2]
    };
    println!(
        "largest recorded graph ({} {} nodes, {} edges): rust {} ns, node {} ns, median of {RUNS} runs",
        largest["type"].as_str().expect("type"),
        nodes.len(),
        edges.len(),
        time(&nodes, &edges, dir),
        median(&node[0])
    );
    println!(
        "random graph ({} nodes, {} edges): rust {} ns, node {} ns, median of {RUNS} runs",
        big_nodes.len(),
        big_edges.len(),
        time(&big_nodes, &big_edges, big_dir),
        median(&node[1])
    );
}
