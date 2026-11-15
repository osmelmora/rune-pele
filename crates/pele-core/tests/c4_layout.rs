//! Differential tests for C4 layout and routing on their own, before anything is drawn: Pele's
//! `layoutC4` and `route` (`src/diagrams/c4/layout.ts`, `route.ts`) through the oracle's
//! `c4_layout` and `c4_route` ops, against `pele_core::c4_probe`. Every number crosses as the
//! bits of its double, so equal means equal to the last bit.
//!
//! The layouts are those of the C4 sources of `tests/fixtures/c4/harvest.json` and of seeded
//! random documents, loose and at the rooms and gaps a tight drawing tries. The routes run
//! between the boxes of those layouts and of random grids, nested boxes and rows, each sequence
//! through one set of lanes as a drawing's relations are.

use pele_core::c4_probe::{self, GroupProbe, ProbeBox, RouteAnswer, RouteCall};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/c4/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn line(op: &str, args: &[String]) -> String {
    let mut out = op.to_owned();
    for arg in args {
        out.push('\t');
        out.push_str(&arg.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"));
    }
    out
}

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
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    /// A number from `low` to `high`, often with a fraction that fills the mantissa.
    fn number(&mut self, low: f64, high: f64) -> f64 {
        let whole = low + self.below((high - low) as usize + 1) as f64;
        match self.below(3) {
            0 => whole,
            1 => whole + self.below(8) as f64 / 8.0,
            _ => whole + (self.next() % 1_000_000) as f64 / 999_983.0,
        }
    }
}

fn group_json(group: &GroupProbe) -> Value {
    let sizes = |labels: &[[f64; 2]]| labels.iter().map(|[w, h]| json!([bits(*w), bits(*h)])).collect::<Vec<_>>();
    json!({
        "boundary": group.boundary,
        "frame": group.frame.map(bits),
        "head": bits(group.head),
        "headW": bits(group.head_w),
        "headX": bits(group.head_x),
        "kids": group.kids,
        "labels": sizes(&group.labels),
        "nodes": group.nodes.iter().map(|node| json!({
            "shape": node.shape,
            "form": node.form,
            "frame": node.frame.map(bits),
            "textH": bits(node.text_h),
            "labels": sizes(&node.labels),
        })).collect::<Vec<_>>(),
    })
}

/// A source with the font size, room and gap of one layout.
#[derive(Clone)]
struct LayoutCase {
    source: String,
    size: f64,
    room: Option<f64>,
    gap: f64,
}

/// The rooms and gaps a tight drawing at `maxWidth` 320, 480, 640 and 900 tries, and none.
fn variants(source: &str, size: f64) -> Vec<LayoutCase> {
    let mut cases = vec![LayoutCase { source: source.to_owned(), size, room: None, gap: 36.0 }];
    for width in [320.0, 480.0, 640.0, 900.0] {
        for gap in [36.0, 60.0, 84.0, 108.0] {
            cases.push(LayoutCase { source: source.to_owned(), size, room: Some(f64::max(80.0, width - 16.0)), gap });
        }
    }
    cases
}

/// Lays out each case through both, and answers how many matched and the layouts that did.
fn compare_layouts(what: &str, cases: &[LayoutCase]) -> Vec<Vec<GroupProbe>> {
    let requests: Vec<String> = cases
        .iter()
        .map(|case| {
            let room = case.room.map_or("null".to_owned(), bits);
            line("c4_layout", &[serde_json::to_string(&case.source).unwrap(), bits(case.size), room, bits(case.gap)])
        })
        .collect();
    let answers = pele_oracle::ask(&requests);
    let mut failures = Vec::new();
    let mut compared = 0;
    let mut laid = Vec::new();
    for (case, theirs) in cases.iter().zip(&answers) {
        let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
        if theirs.is_object() {
            continue;
        }
        let ours = c4_probe::layout(&case.source, case.size, case.room, case.gap)
            .unwrap_or_else(|error| panic!("pele lays out {:?}, this port rejects it: {error}", case.source));
        compared += 1;
        let json = Value::Array(ours.iter().map(group_json).collect());
        if json != theirs {
            failures.push(format!(
                "{:?} at size {} room {:?} gap {}\n    ours: {json}\n    pele: {theirs}",
                case.source, case.size, case.room, case.gap
            ));
        }
        laid.push(ours);
    }
    println!("{what}: {} layouts, {compared} compared, {} different", cases.len(), failures.len());
    assert!(
        failures.is_empty(),
        "{} layouts differ:\n{}",
        failures.len(),
        failures[..failures.len().min(5)].join("\n")
    );
    laid
}

#[test]
fn harvested_layouts_match_pele() {
    let mut cases = Vec::new();
    for source in harvest("sources").iter().chain(&harvest("mutated")) {
        cases.extend(variants(source, 16.0));
    }
    for source in harvest("sources").iter().step_by(7) {
        for size in [12.0, 14.5, 20.0] {
            cases.push(LayoutCase { source: source.clone(), size, room: None, gap: 36.0 });
            cases.push(LayoutCase { source: source.clone(), size, room: Some(304.0), gap: 60.0 });
        }
    }
    let laid = compare_layouts("harvested C4 sources", &cases);
    assert!(laid.len() > 20_000, "most of the harvest lays out");
}

const SHAPES: &[&str] = &[
    "Person",
    "Person_Ext",
    "System",
    "SystemDb",
    "SystemQueue_Ext",
    "Container",
    "ContainerDb_Ext",
    "ContainerQueue",
    "Component",
    "ComponentDb",
    "ComponentQueue_Ext",
];
const BOUNDARIES: &[&str] = &["Boundary", "Enterprise_Boundary", "System_Boundary", "Container_Boundary", "Node_L"];
const LABELS: &[&str] = &[
    "A",
    "Banking system",
    "A rather long label that has to wrap over a few lines in the box",
    "Wort<br>umbruch",
    "x",
    "",
    "unbreakableunbreakableunbreakableunbreakable",
];

/// A random C4 document: shapes of every kind with and without technology and description,
/// boundaries nested up to four deep, an alias used twice, forms set by shape, sprite or tag,
/// and the grid's counts changed.
fn random_document(rng: &mut Rng) -> String {
    let mut src = format!("{}\n", rng.pick(&["C4Context", "C4Container", "C4Deployment"]));
    if rng.below(3) == 0 {
        src.push_str(&format!("UpdateLayoutConfig(\"{}\", \"{}\")\n", 1 + rng.below(6), 1 + rng.below(4)));
    }
    let mut depth = 0;
    let mut alias = 0;
    for _ in 0..1 + rng.below(16) {
        match rng.below(10) {
            0..=1 if depth < 4 => {
                depth += 1;
                alias += 1;
                let descr = if rng.below(2) == 0 { format!(", \"{}\"", rng.pick(LABELS)) } else { String::new() };
                let kind = rng.pick(BOUNDARIES);
                src.push_str(&format!(
                    "{kind}(b{alias}, \"{}\", \"{}\"{descr}) {{\n",
                    rng.pick(LABELS),
                    rng.pick(LABELS)
                ));
                src.push_str(&format!("System(s{alias}, \"{}\")\n", rng.pick(LABELS)));
            }
            2 if depth > 0 => {
                depth -= 1;
                src.push_str("}\n");
            }
            _ => {
                alias += 1;
                let name = if rng.below(12) == 0 { format!("s{}", rng.below(alias)) } else { format!("s{alias}") };
                let kind = rng.pick(SHAPES);
                let mut args = format!("{name}, \"{}\"", rng.pick(LABELS));
                for _ in 0..rng.below(3) {
                    args.push_str(&format!(", \"{}\"", rng.pick(LABELS)));
                }
                match rng.below(8) {
                    0 => args.push_str(&format!(", $shape=\"{}\"", rng.pick(&["db", "Queue", "pipe", "box", "x"]))),
                    1 => args.push_str(&format!(", $tags=\"a, {}\"", rng.pick(&["database", "person", "PIPE"]))),
                    2 => args.push_str(&format!(", $sprite=\"{}\"", rng.pick(&["cylinder", "rounded"]))),
                    _ => {}
                }
                src.push_str(&format!("{kind}({args})\n"));
            }
        }
    }
    for _ in 0..depth {
        src.push_str("}\n");
    }
    src
}

#[test]
fn random_layouts_match_pele() {
    let mut rng = Rng(0x0c4_1a70_0715);
    let mut cases = Vec::new();
    for _ in 0..1200 {
        let source = random_document(&mut rng);
        cases.extend(variants(&source, *rng.pick(&[16.0, 16.0, 13.0, 18.0])));
    }
    compare_layouts("random C4 documents", &cases);
}

/// One sequence of route calls through one set of lanes.
struct Routing {
    boxes: Vec<ProbeBox>,
    frames: Vec<usize>,
    calls: Vec<RouteCall>,
}

fn route_request(routing: &Routing) -> String {
    let request = json!({
        "boxes": routing.boxes.iter().map(|b| json!([bits(b.x), bits(b.y), bits(b.w), bits(b.h), b.form])).collect::<Vec<_>>(),
        "frames": routing.frames,
        "calls": routing.calls.iter().map(|call| json!({
            "a": call.a,
            "b": call.b,
            "shift": bits(call.shift),
            "others": call.others,
            "startTrim": bits(call.start_trim),
            "endTrim": bits(call.end_trim),
        })).collect::<Vec<_>>(),
    });
    line("c4_route", &[request.to_string()])
}

fn route_json(answers: &[RouteAnswer], squeezed: usize) -> Value {
    let routes: Vec<Value> = answers
        .iter()
        .map(|r| {
            json!({
                "d": r.d,
                "start": r.start.map(bits),
                "end": r.end.map(bits),
                "stretch": r.stretch.map(bits),
                "nx": bits(r.nx),
                "ny": bits(r.ny),
                "lean": bits(r.lean),
                "room": bits(r.room),
                "points": r.points.iter().map(|&p| bits(p)).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "routes": routes, "squeezed": squeezed })
}

/// What kind of route an answer is, for the coverage the random routes must reach.
fn kind_of(r: &RouteAnswer) -> &'static str {
    if r.d.contains('C') {
        "a relation to itself"
    } else if r.points.len() == 4 && r.nx != 0.0 {
        "a straight line beside a reversed pair"
    } else if r.points.len() == 4 && r.stretch[0] == r.stretch[2] {
        "an upright straight line"
    } else if r.points.len() == 4 && r.stretch[1] == r.stretch[3] {
        "a level straight line"
    } else if r.points.len() == 4 {
        "a slanted straight line"
    } else if r.lean != 0.0 && r.nx > 0.0 {
        "an upright detour to the right"
    } else if r.lean != 0.0 {
        "an upright detour to the left"
    } else if r.points[1] == r.points[3] {
        "an elbow across a column gap"
    } else if r.points[0] == r.points[2] && r.room == 160.0 {
        "an elbow or level detour across a row gap"
    } else {
        "another bent route"
    }
}

const ROUTE_KINDS: &[&str] = &[
    "a relation to itself",
    "a straight line beside a reversed pair",
    "an upright straight line",
    "a level straight line",
    "a slanted straight line",
    "an upright detour to the right",
    "an upright detour to the left",
    "an elbow across a column gap",
    "an elbow or level detour across a row gap",
];

fn compare_routes(what: &str, routings: &[Routing]) -> (BTreeSet<&'static str>, usize) {
    let requests: Vec<String> = routings.iter().map(route_request).collect();
    let answers = pele_oracle::ask(&requests);
    let mut failures = Vec::new();
    let mut reached = BTreeSet::new();
    let mut squeezed = 0;
    let mut calls = 0;
    for (routing, theirs) in routings.iter().zip(&answers) {
        let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
        let (ours, ours_squeezed) = c4_probe::route(&routing.boxes, &routing.frames, &routing.calls);
        calls += routing.calls.len();
        let json = route_json(&ours, ours_squeezed);
        if json != theirs {
            let at = ours
                .iter()
                .zip(theirs["routes"].as_array().unwrap())
                .position(|(o, t)| route_json(std::slice::from_ref(o), 0)["routes"][0] != *t);
            failures.push(format!("call {at:?} of {}\n    ours: {json}\n    pele: {theirs}", routing.calls.len()));
            continue;
        }
        squeezed += ours_squeezed;
        reached.extend(ours.iter().map(kind_of));
    }
    println!(
        "{what}: {} sequences of {calls} routes, {} different, {squeezed} squeezed; reached {:?}",
        routings.len(),
        failures.len(),
        reached
    );
    assert!(
        failures.is_empty(),
        "{} sequences differ:\n{}",
        failures.len(),
        failures[..failures.len().min(3)].join("\n")
    );
    (reached, squeezed)
}

/// Random calls among `boxes`, with the trims of an arrowhead or none, the shift of a reversed
/// pair, and the shapes to keep clear of or none.
fn calls(rng: &mut Rng, boxes: usize, shapes: &[usize], count: usize) -> Vec<RouteCall> {
    (0..count)
        .map(|_| {
            let a = rng.below(boxes);
            let b = if rng.below(12) == 0 { a } else { rng.below(boxes) };
            RouteCall {
                a,
                b,
                shift: if rng.below(4) == 0 { 7.0 } else { 0.0 },
                others: (rng.below(10) > 0).then(|| shapes.to_vec()),
                start_trim: *rng.pick(&[0.0, 7.0]),
                end_trim: *rng.pick(&[7.0, 7.0, 0.0]),
            }
        })
        .collect()
}

/// The boxes of a layout: its shapes, then its boundaries, each boundary framed.
fn routing_of(layout: &[GroupProbe], rng: &mut Rng) -> Option<Routing> {
    let mut boxes = Vec::new();
    for group in layout {
        for node in &group.nodes {
            let [x, y, w, h] = node.frame;
            boxes.push(ProbeBox { x, y, w, h, form: node.form });
        }
    }
    let shapes: Vec<usize> = (0..boxes.len()).collect();
    let mut frames = Vec::new();
    for group in &layout[1..] {
        let [x, y, w, h] = group.frame;
        frames.push(boxes.len());
        boxes.push(ProbeBox { x, y, w, h, form: "rect" });
    }
    if boxes.len() < 2 {
        return None;
    }
    let count = 1 + rng.below(3 * boxes.len());
    Some(Routing { calls: calls(rng, boxes.len(), &shapes, count), boxes, frames })
}

#[test]
fn routes_between_laid_out_shapes_match_pele() {
    let mut rng = Rng(0x0c4_2007e);
    let sources: Vec<String> = harvest("sources").into_iter().chain(harvest("mutated")).collect();
    let mut layouts = Vec::new();
    for source in sources.iter().step_by(3) {
        if let Ok(layout) = c4_probe::layout(source, 16.0, None, 36.0) {
            layouts.push(layout);
        }
    }
    for _ in 0..600 {
        let source = random_document(&mut rng);
        let room = rng.pick(&[None, Some(304.0), Some(624.0)]).to_owned();
        layouts
            .push(c4_probe::layout(&source, 16.0, room, *rng.pick(&[36.0, 60.0])).expect("a random document parses"));
    }
    let routings: Vec<Routing> = layouts.iter().filter_map(|layout| routing_of(layout, &mut rng)).collect();
    compare_routes("routes between laid out shapes", &routings);
}

/// Boxes in a grid of rows and columns, with gaps from tight to wide, a few of them nested in
/// a frame or dropped out of line, so that every way of routing is tried: straight, shifted,
/// round the shapes in the way, along a lane, and squeezed.
fn random_grid(rng: &mut Rng) -> Routing {
    let forms = ["rect", "rect", "person", "cyl", "h-cyl"];
    let columns = 1 + rng.below(5);
    let rows = 1 + rng.below(4);
    let gap_x = rng.number(8.0, 140.0);
    let gap_y = rng.number(8.0, 100.0);
    let mut boxes = Vec::new();
    let mut y = rng.number(0.0, 40.0);
    for _ in 0..rows {
        let mut x = rng.number(0.0, 40.0);
        let h = rng.number(40.0, 140.0);
        for _ in 0..columns {
            let w = rng.number(60.0, 260.0);
            let dy = if rng.below(6) == 0 { rng.number(0.0, 60.0) } else { 0.0 };
            boxes.push(ProbeBox { x, y: y + dy, w, h, form: rng.pick(&forms) });
            x += w + gap_x;
        }
        y += h + gap_y;
    }
    let shapes: Vec<usize> = (0..boxes.len()).collect();
    let mut frames = Vec::new();
    // A frame round some of the boxes, which a relation can start or end at.
    for _ in 0..rng.below(3) {
        let first = rng.below(boxes.len());
        let last = first + rng.below(boxes.len() - first);
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for b in &boxes[first..=last] {
            x0 = x0.min(b.x);
            y0 = y0.min(b.y);
            x1 = x1.max(b.x + b.w);
            y1 = y1.max(b.y + b.h);
        }
        let pad = rng.number(4.0, 40.0);
        frames.push(boxes.len());
        boxes.push(ProbeBox { x: x0 - pad, y: y0 - pad, w: x1 - x0 + 2.0 * pad, h: y1 - y0 + 2.0 * pad, form: "rect" });
    }
    let count = 1 + rng.below(4 * boxes.len());
    Routing { calls: calls(rng, boxes.len(), &shapes, count), boxes, frames }
}

#[test]
fn routes_on_random_grids_match_pele() {
    let mut rng = Rng(0x0c4_9a1d);
    let routings: Vec<Routing> = (0..4000).map(|_| random_grid(&mut rng)).collect();
    let (reached, squeezed) = compare_routes("routes on random grids", &routings);
    let missing: Vec<&&str> = ROUTE_KINDS.iter().filter(|kind| !reached.contains(**kind)).collect();
    assert!(missing.is_empty(), "no random route is {missing:?}");
    assert!(squeezed > 0, "some random route finds no lane of its own");
}
