//! Seeded random git graphs for `git_front.rs` and `git_parity.rs`. A small model of Mermaid's git
//! database keeps most statements valid: commits of every type with ids, messages and tags,
//! branches with and without an order, checkouts, merges between branches that have diverged, and
//! cherry-picks of commits from other branches, merges among them. A few statements break a rule
//! on purpose, so each of Pele's errors is reached.

use serde_json::{Value, json};

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

const TEXTS: &[&str] = &[
    "fix",
    "a rather long commit id",
    "v1.0",
    "#35; and &amp;",
    "<b>bold</b>",
    "line<br>break",
    "\u{e9}t\u{e9}",
    "\u{4e2d}\u{6587}",
    "x",
    "spaces   inside",
    "**a**",
    "",
];

const NAMES: &[&str] = &["develop", "feature/x_y-z", "1.0.1", "hotfix", "a.b", "titles", "b", "release-2", "c"];

fn quoted(rng: &mut Rng, text: &str) -> String {
    if rng.chance(85) { format!("\"{text}\"") } else { format!("'{text}'") }
}

struct Commit {
    id: String,
    branch: usize,
    merge: bool,
    parents: Vec<String>,
}

/// One random git graph.
pub fn random_git(rng: &mut Rng) -> String {
    let mut out = String::new();
    let mut config = Vec::new();
    for (key, values) in [
        ("showBranches", &["true", "false"][..]),
        ("showCommitLabel", &["true", "false"]),
        ("rotateCommitLabel", &["true", "false"]),
        ("parallelCommits", &["true", "false"]),
        ("mainBranchName", &["trunk", "\"main line\"", "dev"]),
        ("mainBranchOrder", &["2", "0", "-1", "0.5"]),
    ] {
        if rng.chance(18) {
            config.push(format!("    {key}: {}", rng.pick(values)));
        }
    }
    let mut themed = Vec::new();
    if rng.chance(8) {
        for i in 0..8 {
            if rng.chance(50) {
                themed.push(format!(
                    "    git{i}: \"{}\"",
                    rng.pick(&["#ffeb3b", "navy", "rgb(10, 200, 30)", "notacolor", "#123"])
                ));
            }
        }
    }
    let main = config
        .iter()
        .find_map(|line| line.strip_prefix("    mainBranchName: "))
        .map_or("main".to_owned(), |name| name.trim_matches('"').to_owned());
    if !config.is_empty() || !themed.is_empty() || rng.chance(5) {
        out.push_str("---\n");
        if rng.chance(30) {
            out.push_str("title: Front title\n");
        }
        out.push_str("config:\n");
        if !config.is_empty() {
            out.push_str("  gitGraph:\n");
            for line in &config {
                out.push_str(line);
                out.push('\n');
            }
        }
        if !themed.is_empty() {
            out.push_str("  themeVariables:\n");
            for line in &themed {
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str("---\n");
    }
    out.push_str(rng.pick(&["gitGraph", "gitGraph:", "gitGraph LR:", "gitGraph TB:", "gitGraph BT:", "gitGraph TB :"]));
    out.push('\n');
    if rng.chance(25) {
        out.push_str(&format!(
            "  title {}\n",
            rng.pick(&["History", "A much longer title for a small graph", "#35; t"])
        ));
    }
    if rng.chance(10) {
        out.push_str("  accTitle: The history\n");
    }
    if rng.chance(10) {
        out.push_str(rng.pick(&["  accDescr: Two branches\n", "  accDescr {\n    multi\n    line\n  }\n"]));
    }

    let mut branches: Vec<(String, Option<String>)> = vec![(main, None)];
    let mut commits: Vec<Commit> = Vec::new();
    let mut current = 0;
    let mut serial = 0;
    let statements = 1 + rng.below(16);
    for _ in 0..statements {
        let roll = rng.below(100);
        if roll < 42 || (commits.is_empty() && rng.chance(80)) {
            let mut line = "  commit".to_owned();
            let id = if rng.chance(60) {
                let id = if rng.chance(6) && !commits.is_empty() {
                    commits[rng.below(commits.len())].id.clone()
                } else {
                    serial += 1;
                    format!("{}{serial}", rng.pick(&["c", "fix-", "a rather long commit id ", "#35;"]))
                };
                line.push_str(&format!(" id: \"{id}\""));
                Some(id)
            } else {
                None
            };
            if rng.chance(25) {
                let text = *rng.pick(TEXTS);
                let message = quoted(rng, text);
                line.push_str(&if rng.chance(70) { format!(" msg: {message}") } else { format!(" {message}") });
            }
            for _ in 0..[0, 0, 0, 1, 1, 2, 3][rng.below(7)] {
                let text = *rng.pick(TEXTS);
                line.push_str(&format!(" tag: {}", quoted(rng, text)));
            }
            if rng.chance(30) {
                line.push_str(&format!(" type: {}", rng.pick(&["NORMAL", "REVERSE", "HIGHLIGHT"])));
            }
            out.push_str(&line);
            out.push('\n');
            let id = id.unwrap_or_else(|| format!("#{}", commits.len()));
            let parents = branches[current].1.iter().cloned().collect();
            commits.push(Commit { id: id.clone(), branch: current, merge: false, parents });
            branches[current].1 = Some(id);
        } else if roll < 56 {
            let fresh = !rng.chance(3);
            let name = if fresh {
                format!("{}{}", rng.pick(NAMES), branches.len())
            } else {
                branches[rng.below(branches.len())].0.clone()
            };
            let written = if name.contains(' ') || rng.chance(10) { format!("\"{name}\"") } else { name.clone() };
            let order = if rng.chance(30) {
                format!(" order: {}", rng.pick(&["1", "2", "5", "0", "10"]))
            } else {
                String::new()
            };
            out.push_str(&format!("  branch {written}{order}\n"));
            if fresh {
                let head = branches[current].1.clone();
                branches.push((name, head));
                current = branches.len() - 1;
            }
        } else if roll < 70 {
            let target =
                if rng.chance(3) { "nowhere".to_owned() } else { branches[rng.below(branches.len())].0.clone() };
            let written = if target.contains(' ') { format!("\"{target}\"") } else { target.clone() };
            out.push_str(&format!("  {} {written}\n", rng.pick(&["checkout", "switch"])));
            if let Some(index) = branches.iter().position(|(name, _)| *name == target) {
                current = index;
            }
        } else if roll < 86 {
            // A merge needs a head on both branches, two heads, and a head here not made on the other.
            let here = branches[current].1.clone();
            let made_on = |id: &Option<String>| {
                id.as_ref()
                    .and_then(|id| commits.iter().rev().find(|commit| commit.id == *id))
                    .map(|commit| commit.branch)
            };
            let others: Vec<usize> = (0..branches.len())
                .filter(|&b| {
                    b != current && branches[b].1.is_some() && branches[b].1 != here && made_on(&here) != Some(b)
                })
                .collect();
            if here.is_none() || (others.is_empty() && !rng.chance(4)) {
                out.push_str("  commit\n");
                let id = format!("#{}", commits.len());
                commits.push(Commit {
                    id: id.clone(),
                    branch: current,
                    merge: false,
                    parents: here.into_iter().collect(),
                });
                branches[current].1 = Some(id);
                continue;
            }
            let other = if others.is_empty() || rng.chance(3) { rng.below(branches.len()) } else { *rng.pick(&others) };
            let name = &branches[other].0;
            let mut line = format!("  merge {}", if name.contains(' ') { format!("\"{name}\"") } else { name.clone() });
            let id = if rng.chance(40) {
                serial += 1;
                let id = format!("m{serial}");
                line.push_str(&format!(" id: \"{id}\""));
                Some(id)
            } else {
                None
            };
            if rng.chance(25) {
                line.push_str(&format!(" tag: {}", quoted(rng, "v2")));
            }
            if rng.chance(25) {
                line.push_str(&format!(" type: {}", rng.pick(&["NORMAL", "REVERSE", "HIGHLIGHT"])));
            }
            out.push_str(&line);
            out.push('\n');
            let id = id.unwrap_or_else(|| format!("#{}", commits.len()));
            let parents = branches[current].1.iter().cloned().chain(branches[other].1.iter().cloned()).collect();
            commits.push(Commit { id: id.clone(), branch: current, merge: true, parents });
            branches[current].1 = Some(id);
        } else {
            let named: Vec<&Commit> = commits.iter().filter(|commit| !commit.id.starts_with('#')).collect();
            let sources: Vec<&Commit> = named.iter().filter(|commit| commit.branch != current).copied().collect();
            let here = branches[current].1.clone();
            let pick = if rng.chance(4) {
                named.first().copied()
            } else {
                sources.get(rng.below(sources.len().max(1))).copied()
            };
            let Some(pick) = pick.filter(|_| here.is_some()) else {
                out.push_str("  commit\n");
                let id = format!("#{}", commits.len());
                commits.push(Commit {
                    id: id.clone(),
                    branch: current,
                    merge: false,
                    parents: here.into_iter().collect(),
                });
                branches[current].1 = Some(id);
                continue;
            };
            let mut line = format!("  cherry-pick id: \"{}\"", pick.id);
            if pick.merge || rng.chance(5) {
                let parent = pick.parents.get(rng.below(2)).or(pick.parents.first()).cloned().unwrap_or_default();
                line.push_str(&format!(" parent: \"{parent}\""));
            }
            if rng.chance(25) {
                let text = *rng.pick(TEXTS);
                line.push_str(&format!(" tag: {}", quoted(rng, text)));
            }
            out.push_str(&line);
            out.push('\n');
            let id = format!("#{}", commits.len());
            commits.push(Commit { id: id.clone(), branch: current, merge: false, parents: Vec::new() });
            branches[current].1 = Some(id);
        }
    }
    out
}

/// Render options a host may give: a width, which may turn a graph that runs across to run
/// down, and other sizes.
pub fn random_options(rng: &mut Rng) -> Value {
    let mut options = serde_json::Map::new();
    if rng.chance(45) {
        options.insert("maxWidth".to_owned(), json!(*rng.pick(&[120.0, 240.0, 320.0, 480.0, 639.0, 900.0])));
    }
    if rng.chance(10) {
        options.insert("fontSize".to_owned(), json!(*rng.pick(&[12.0, 13.5, 20.0])));
    }
    if rng.chance(10) {
        options.insert("padding".to_owned(), json!(*rng.pick(&[0.0, 20.5])));
    }
    if rng.chance(5) {
        options.insert("autoDirection".to_owned(), json!(false));
    }
    if rng.chance(5) {
        options.insert("directionBreakpoint".to_owned(), json!(*rng.pick(&[100.0, 2000.0])));
    }
    Value::Object(options)
}
