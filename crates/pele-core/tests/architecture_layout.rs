//! Differential test for the architecture grid on its own: for every source, the cell this port's
//! `placeOnGrid` gives each service, junction and group must be the cell Pele 1.13.0's gives it,
//! as the oracle's `architecture_grid` op answers.
//!
//! The inputs are `tests/fixtures/architecture/harvest.json` (written by
//! `scripts/harvest-architecture.mjs`), with Pele's worst cases at any length, and seeded random
//! diagrams that must reach each placement rule in [`RULES`].

use pele_core::architecture_probe::{GridProbe, grid};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/architecture/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("architecture_grid\t{source}")
}

/// The grid in the words of the oracle: Pele's `Grid`, each typed array as a list.
fn ours(grid: &GridProbe) -> Value {
    json!({
        "col": grid.col,
        "row": grid.row,
        "groupCol": grid.group_col,
        "groupRow": grid.group_row,
        "groupCols": grid.group_cols,
        "groupRows": grid.group_rows,
        "groupParent": grid.group_parent,
        "groupEmpty": grid.group_empty,
        "nodeParent": grid.node_parent,
    })
}

#[derive(Default)]
struct Tally {
    placed: usize,
    rejected: usize,
    other_types: usize,
    failures: Vec<String>,
}

impl Tally {
    fn finish(self, what: &str) -> Self {
        let total = self.placed + self.rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} placed as Pele places them, {} rejected by both, {} of another type, {} different",
            self.placed,
            self.rejected,
            self.other_types,
            self.failures.len()
        );
        assert!(
            self.failures.is_empty(),
            "{} of {total} {what} differ from Pele:\n{}",
            self.failures.len(),
            self.failures[..self.failures.len().min(8)].join("\n")
        );
        self
    }
}

fn run(name: &str, sources: &[String]) -> Tally {
    let requests: Vec<String> = sources.iter().map(|source| request(source)).collect();
    let mut tally = Tally::default();
    for (index, (source, theirs)) in sources.iter().zip(pele_oracle::ask(&requests)).enumerate() {
        let theirs: Value = serde_json::from_str(&theirs).expect("the oracle answers JSON");
        let problem = match std::panic::catch_unwind(|| grid(source)) {
            Err(_) => Some("this port panics".to_owned()),
            Ok(Ok(placed)) if ours(&placed) == theirs => {
                tally.placed += 1;
                None
            }
            Ok(Ok(placed)) => Some(format!("placed differently\n    ours: {}\n    pele: {theirs}", ours(&placed))),
            Ok(Err(_)) if theirs.get("error").is_some() => {
                tally.rejected += 1;
                None
            }
            Ok(Err(_)) if theirs.get("type").is_some() => {
                tally.other_types += 1;
                None
            }
            Ok(Err(error)) => Some(format!("ours rejects it ({error}), pele places it")),
        };
        if let Some(problem) = problem {
            let shown: String = source.chars().take(400).collect();
            tally.failures.push(format!("{name} {index}: {problem}\n    source: {shown:?}"));
        }
    }
    tally
}

#[test]
fn harvested_sources_place_as_pele_places_them() {
    let tally = run("harvested", &harvest("sources")).finish("harvested architecture sources");
    assert!(tally.placed > 1000);
    run("mutated", &harvest("mutated")).finish("mutated architecture sources");
    let worst = run("worst", &harvest("worst")).finish("Pele's worst-case architecture diagrams");
    assert!(worst.placed > 30, "the worst cases are laid out, not rejected");
}

/// xorshift64, so the diagrams are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

const SIDES: [&str; 4] = ["L", "R", "T", "B"];

/// A diagram the db accepts: groups nested in earlier ones, services and junctions in them, and
/// edges and alignments between the nodes. Some hold one long chain or a block of cells in a
/// group, so that blocks of more than 64 cells and parts that wrap into rows are placed.
fn random_diagram(rng: &mut Rng) -> String {
    let mut src = String::from("architecture-beta\n");
    let groups = rng.below(7);
    let nodes = if rng.chance(10) { 40 + rng.below(80) } else { 1 + rng.below(16) };
    let mut parent = Vec::new();
    for g in 0..groups {
        let p = (g > 0 && rng.chance(50)).then(|| rng.below(g));
        src += &format!("group g{g}{}\n", p.map_or(String::new(), |p| format!(" in g{p}")));
        parent.push(p);
    }
    let mut home = Vec::new();
    for i in 0..nodes {
        let g = (groups > 0 && rng.chance(65)).then(|| rng.below(groups));
        let kind = if rng.chance(15) { "junction" } else { "service" };
        src += &format!("{kind} n{i}{}\n", g.map_or(String::new(), |g| format!(" in g{g}")));
        home.push(g);
    }
    let chain = nodes > 30 && rng.chance(60);
    if chain {
        let (a, b) = if rng.chance(50) { ("R", "L") } else { ("B", "T") };
        for i in 1..nodes / 2 {
            src += &format!("n{}:{a} -- {b}:n{i}\n", i - 1);
        }
    }
    let edges = rng.below(nodes * 2 + 1);
    let like = rng.chance(20);
    for _ in 0..edges {
        let (a, b) = (rng.below(nodes), rng.below(nodes));
        let sa = SIDES[rng.below(4)];
        let sb = if like && rng.chance(50) { sa } else { SIDES[rng.below(4)] };
        let into = ["", "<", ">"][rng.below(3)];
        let across = home[a].is_some() && home[a] != home[b] && rng.chance(20);
        src += &format!("n{a}{}:{sa} {into}-- {sb}:n{b}\n", if across { "{group}" } else { "" });
    }
    if nodes > 2 && rng.chance(40) {
        for _ in 0..1 + rng.below(3) {
            let count = 2 + rng.below(3.min(nodes - 1));
            let mut members: Vec<usize> = Vec::new();
            while members.len() < count {
                let member = rng.below(nodes);
                if !members.contains(&member) {
                    members.push(member);
                }
            }
            let names: Vec<String> = members.iter().map(|m| format!("n{m}")).collect();
            src += &format!("align {} {}\n", if rng.chance(50) { "row" } else { "column" }, names.join(" "));
        }
    }
    src
}

type Rule = fn(&str, &GridProbe) -> bool;

/// The edges of a random diagram as their two ends and the side each leaves by.
fn edges(source: &str) -> Vec<(&str, char, char, &str)> {
    source
        .lines()
        .filter_map(|line| {
            let (a, rest) = line.split_once(':')?;
            let (sides, b) = rest.split_once(':')?;
            let first = sides.chars().next()?;
            let last = sides.chars().last()?;
            Some((a.trim_end_matches("{group}"), first, last, b))
        })
        .collect()
}

fn facing(a: char, b: char) -> bool {
    matches!((a, b), ('L', 'R') | ('R', 'L') | ('T', 'B') | ('B', 'T'))
}

/// Whether two edges with the same facing sides share the end `pick` names.
fn shared_facing(source: &str, pick: for<'a> fn(&'a (&'a str, char, char, &'a str)) -> &'a str) -> bool {
    let edges = edges(source);
    edges.iter().enumerate().any(|(i, x)| {
        facing(x.1, x.2)
            && edges[i + 1..].iter().any(|y| (y.1, y.2) == (x.1, x.2) && pick(x) == pick(y) && x.0 != x.3 && y.0 != y.3)
    })
}

/// The placement rules random diagrams must reach, each with what in the source and Pele's grid
/// shows it was used.
const RULES: &[(&str, Rule)] = &[
    ("an alignment followed ahead of the edges", |source, _| source.contains("\nalign ")),
    ("an edge between like sides", |source, _| edges(source).iter().any(|e| e.1 == e.2 && e.0 != e.3)),
    ("an edge that bends", |source, _| edges(source).iter().any(|e| e.1 != e.2 && !facing(e.1, e.2))),
    ("a cell wanted twice, so a node slides aside", |source, _| shared_facing(source, |e| e.0)),
    ("a node reached through the same sides from several", |source, _| shared_facing(source, |e| e.3)),
    ("nested groups", |_, grid| grid.group_parent.iter().any(|&p| p >= 0)),
    ("an empty group", |_, grid| grid.group_empty.contains(&1)),
    ("a group of more than 64 cells", |_, grid| grid.group_cols.iter().zip(&grid.group_rows).any(|(w, h)| w * h > 64)),
    ("parts wrapped into rows", |_, grid| {
        grid.group_parent.is_empty() && grid.row.iter().collect::<BTreeSet<_>>().len() > 1
    }),
];

#[test]
fn random_diagrams_place_as_pele_places_them() {
    let mut rng = Rng(0x0a2c_417e_c7d1_a7e5);
    let sources: Vec<String> = (0..3000).map(|_| random_diagram(&mut rng)).collect();
    let tally = run("random", &sources).finish("random architecture diagrams");
    assert_eq!(tally.placed, sources.len(), "every random diagram is one the db accepts");
    let mut reached = BTreeSet::new();
    for source in &sources {
        let placed = grid(source).expect("placed above");
        reached.extend(RULES.iter().filter(|(_, rule)| rule(source, &placed)).map(|(name, _)| *name));
    }
    let missed: Vec<&str> = RULES.iter().map(|(name, _)| *name).filter(|name| !reached.contains(name)).collect();
    println!("reach {} of {} named placement rules", reached.len(), RULES.len());
    assert!(missed.is_empty(), "no random diagram reaches {missed:?}");
}
