//! Differential test for drawing git graphs: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/git/harvest.json` holds the git sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-git.mjs`. A graph that runs
//! across is drawn running down instead when that fits a narrow host better, so the widths matter.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "git/random.rs"]
mod random;

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use random::{Rng, random_git, random_options};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/git/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// This port's render with Pele's render options, which are what `render_with` gives the oracle.
fn render_with(source: &str, options: &Value) -> Result<Rendered, RenderError> {
    let mut parse = ParseOptions::default();
    let mut layout = LayoutOptions::default();
    let number = |value: &Value| value.as_f64().expect("a number");
    let count = |value: &Value| usize::try_from(value.as_u64().expect("a count")).expect("a count");
    let flag = |value: &Value| value.as_bool().expect("a boolean");
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = number(value),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(text(value)),
            "responsive" => layout.responsive = Some(flag(value)),
            "autoDirection" => layout.auto_direction = flag(value),
            "directionBreakpoint" => layout.direction_breakpoint = Some(number(value)),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

/// Whether Pele's SVG shows that a source went through one path of the draw step.
type Shows = fn(&str) -> bool;

fn contains(svg: &str, mark: &str) -> bool {
    svg.contains(mark)
}

/// The `d` of every connector between commits.
fn connectors(svg: &str) -> impl Iterator<Item = &str> {
    svg.split("<path class=\"pele-edge").skip(1).filter_map(|rest| rest.split("d=\"").nth(1)?.split('"').next())
}

/// The commands of a path, as its letters.
fn letters(d: &str) -> String {
    d.chars().filter(char::is_ascii_alphabetic).collect()
}

/// The groups of the commits, each up to the next.
fn commit_groups(svg: &str) -> impl Iterator<Item = &str> {
    svg.split("<g class=\"pele-commit ").skip(1)
}

/// The `d` of each lane's line.
fn lane_lines(svg: &str) -> impl Iterator<Item = &str> {
    svg.split("class=\"pele-lane\" d=\"").skip(1).filter_map(|rest| rest.split('"').next())
}

/// Whether a lane's line runs down the page rather than across it.
fn runs_down(d: &str) -> bool {
    let numbers: Vec<f64> = d.split(['M', 'L', ',']).filter_map(|part| part.parse().ok()).collect();
    numbers.len() == 4 && numbers[0] == numbers[2] && numbers[1] != numbers[3]
}

/// The drawing paths of `renderGit`, each with what in Pele's SVG shows it was taken.
const PATHS: &[(&str, Shows)] = &[
    ("a title", |svg| contains(svg, "class=\"pele-title\"")),
    ("an accessible title", |svg| contains(svg, "aria-labelledby=")),
    ("an accessible description", |svg| contains(svg, "aria-describedby=")),
    ("a graph with no commits", |svg| !contains(svg, "pele-commits")),
    ("a normal commit", |svg| contains(svg, "pele-commit-normal\"")),
    ("a reverse commit", |svg| contains(svg, "pele-commit-reverse\"")),
    ("a highlight commit", |svg| contains(svg, "pele-commit-highlight\"")),
    ("a merge", |svg| contains(svg, "pele-commit-merge\"")),
    ("a merge drawn as another type", |svg| {
        ["normal", "reverse", "highlight"]
            .iter()
            .any(|kind| svg.contains(&format!("pele-commit-{kind} pele-commit-merge")))
    }),
    ("a cherry-pick", |svg| contains(svg, "pele-commit-cherry-pick\"")),
    ("a commit's message as its tooltip", |svg| commit_groups(svg).any(|group| group.contains("><title>"))),
    ("a tag", |svg| contains(svg, "class=\"pele-tag\"")),
    ("several tags on one commit", |svg| {
        commit_groups(svg).any(|group| group.matches("class=\"pele-tag\"").count() > 1)
    }),
    ("a label turned back", |svg| contains(svg, "rotate(-45 ")),
    ("a label turned forward", |svg| contains(svg, "rotate(45 ")),
    ("a level label", |svg| contains(svg, "<g class=\"pele-commit-label\"><rect")),
    ("lanes with names", |svg| contains(svg, "class=\"pele-branch\"") && contains(svg, "style=\"rx:var(--_r)\"")),
    ("lanes without names", |svg| contains(svg, "pele-commit") && !contains(svg, "pele-branches")),
    ("a name in the palette's ink", |svg| contains(svg, "class=\"pele-label\" fill=\"var(--pele-fg-on")),
    ("a palette", |svg| contains(svg, "--pele-series-1:")),
    ("a connector along one lane", |svg| connectors(svg).any(|d| letters(d) == "ML")),
    ("a merge connector", |svg| contains(svg, "pele-edge pele-edge-merge")),
    ("a cherry-pick connector", |svg| contains(svg, "pele-edge pele-edge-cherry-pick")),
    ("a connector back along the history", |svg| connectors(svg).any(|d| letters(d) == "MC")),
    ("a connector that runs along the child's lane", |svg| connectors(svg).any(|d| letters(d) == "MCL")),
    ("a connector that turns in at the child", |svg| connectors(svg).any(|d| letters(d) == "MLC")),
    ("a connector that turns in before the child", |svg| connectors(svg).any(|d| letters(d) == "MLCL")),
    ("a connector between two lanes", |svg| connectors(svg).any(|d| letters(d) == "MCLC")),
    ("a connector between two lanes that turns in before the child", |svg| {
        connectors(svg).any(|d| letters(d) == "MCLCL")
    }),
    ("lanes that run down", |svg| lane_lines(svg).any(runs_down)),
    ("lanes that run up", |svg| lane_lines(svg).any(|d| runs_down(d) && d.contains(",-"))),
    ("two lanes", |svg| svg.matches("class=\"pele-lane\"").count() > 1),
];

/// Where two texts first differ, with what each holds there.
fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(80).min(bytes.len())..(at + 80).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
}

#[derive(Default)]
struct Tally {
    identical: usize,
    rejected: usize,
    other_types: usize,
    failures: Vec<String>,
    /// The drawing paths that the identical outputs went through, by name.
    reached: BTreeSet<&'static str>,
}

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<Rendered, RenderError>, theirs: &PeleRender) {
        let problem = match (theirs, ours) {
            (PeleRender::Svg { r#type, svg, width, height }, Ok(ours)) => {
                if ours.svg != *svg {
                    Some(first_difference(&ours.svg, svg))
                } else if ours.width != *width || ours.height != *height {
                    Some(format!("size {}x{}, pele {width}x{height}", ours.width, ours.height))
                } else {
                    if r#type == "gitGraph" {
                        self.identical += 1;
                        self.reached.extend(PATHS.iter().filter(|(_, shows)| shows(svg)).map(|(name, _)| *name));
                    } else {
                        self.other_types += 1;
                    }
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "gitGraph" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "gitGraph" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { error, .. }, Err(RenderError::Diagnostic(ours))) => {
                let ours = format!("PeleError: {}", ours.message);
                if ours == *error {
                    self.rejected += 1;
                    None
                } else {
                    Some(format!("rejected with another message\n    ours: {ours:?}\n    pele: {error:?}"))
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(error)) => Some(format!("pele renders a {type}, ours fails: {error}")),
            (PeleRender::Error { error, .. }, Ok(_)) => Some(format!("pele rejects it, ours renders: {error}")),
            (PeleRender::Error { error, .. }, Err(RenderError::NotPorted)) => {
                Some(format!("pele rejects a git graph, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {source:?}"));
        }
    }

    fn finish(self, what: &str) -> Self {
        let total = self.identical + self.rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {} rejected with Pele's message, {} of another type, {} different",
            self.identical,
            self.rejected,
            self.other_types,
            self.failures.len()
        );
        assert!(
            self.failures.is_empty(),
            "{} of {total} {what} differ from Pele:\n{}",
            self.failures.len(),
            self.failures[..self.failures.len().min(10)].join("\n")
        );
        self
    }
}

/// Renders each case through both engines. A panic in this port is a failure of that case.
fn run(name: &str, cases: &[(String, Value)]) -> Tally {
    let requests: Vec<(String, String)> =
        cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
    let mut tally = Tally::default();
    for (index, ((source, options), theirs)) in cases.iter().zip(pele_oracle::render_all_with(&requests)).enumerate() {
        let name = format!("{name} {index} with {options}");
        match std::panic::catch_unwind(|| render_with(source, options)) {
            Ok(ours) => tally.compare(&name, source, ours, &theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

/// How many of the cases Pele draws another way for the width they give than with no width.
fn redrawn(cases: &[(String, Value)]) -> usize {
    let svg = |render: &PeleRender| match render {
        PeleRender::Svg { svg, .. } => Some(svg.clone()),
        PeleRender::Error { .. } => None,
    };
    let with = |drop_width: bool| -> Vec<(String, String)> {
        cases
            .iter()
            .map(|(source, options)| {
                let mut options = options.clone();
                if drop_width {
                    options.as_object_mut().expect("options are an object").remove("maxWidth");
                }
                (source.clone(), options.to_string())
            })
            .collect()
    };
    let (bounded, unbounded) = (pele_oracle::render_all_with(&with(false)), pele_oracle::render_all_with(&with(true)));
    bounded.iter().zip(&unbounded).filter(|(bounded, unbounded)| svg(bounded) != svg(unbounded)).count()
}

const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested git sources");
    assert!(tally.identical > 500, "the harvest holds git graphs Pele renders");
    assert!(tally.rejected > 100, "the harvest holds git graphs Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested git sources at maxWidth {width}"));
        println!("maxWidth {width} makes Pele draw {} of the harvested sources another way", redrawn(&cases));
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated git sources");
}

const FLOW: &str = "\n  commit id: \"init\"\n  commit type: REVERSE tag: \"v0.1\"\n  branch develop\n  commit id: \"feat\" msg: \"A message\"\n  commit type: HIGHLIGHT\n  checkout main\n  commit tag: \"v0.2\" tag: \"stable\"\n  merge develop id: \"merged\" tag: \"v1.0\"\n  branch hotfix\n  commit id: \"fix\"\n  checkout develop\n  commit\n  cherry-pick id: \"fix\"\n  checkout main\n  merge hotfix\n";

/// Hand-written graphs for what the harvest reaches least, each with the host options that reach it.
fn cases() -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for header in ["gitGraph", "gitGraph TB:", "gitGraph BT:"] {
        for options in [
            json!({}),
            json!({ "maxWidth": 320 }),
            json!({ "maxWidth": 100, "padding": 0 }),
            json!({ "fontSize": 13.5, "padding": 20.5 }),
            json!({ "responsive": false, "idPrefix": "a\"b" }),
            json!({ "maxWidth": 320, "autoDirection": false }),
            json!({ "maxWidth": 320, "directionBreakpoint": 200 }),
            json!({ "config": { "gitGraph": { "parallelCommits": true, "rotateCommitLabel": false } } }),
            json!({ "config": { "gitGraph": { "showBranches": false, "showCommitLabel": false } } }),
            json!({ "config": { "gitGraph": { "useMaxWidth": false } } }),
            json!({ "config": { "themeVariables": { "git0": "#ffeb3b", "git1": "navy", "git2": "url(x)" } } }),
        ] {
            out.push((format!("{header}{FLOW}"), options));
        }
    }
    let more = [
        "gitGraph".to_owned(),
        "gitGraph\n title Only a title".to_owned(),
        "gitGraph\n title A title much wider than the graph it heads\n commit".to_owned(),
        "gitGraph\n branch a\n branch \"b c\"".to_owned(),
        "gitGraph\n commit id:\"add <Button> #35;1\" tag:\"<i>t</i>\"\n branch \"one<br>two\"\n commit id:\"**a**\"".to_owned(),
        "gitGraph\n commit id:\"a\" tag:\"\" tag:\"x\"\n branch b\n commit id:\"c\"\n checkout main\n commit\n cherry-pick id:\"c\" tag:\"\"".to_owned(),
        "gitGraph\n commit id:\"a\"\n commit id:\"a\"\n branch b\n commit id:\"a\"".to_owned(),
        "gitGraph\n commit id:\"x\"\n branch b\n commit id:\"x\"\n checkout main\n commit\n merge b".to_owned(),
        "gitGraph\n commit\n branch b\n commit\n checkout main\n commit\n merge b type: REVERSE\n merge b".to_owned(),
        "gitGraph\n commit\n branch b\n commit tag:\"t1\" tag:\"t2\" tag:\"a long tag\"\n checkout main\n commit\n checkout b\n commit\n checkout main\n merge b".to_owned(),
        format!("gitGraph\n commit\n{}", (0..6).map(|i| format!(" branch b{i}\n commit tag:\"v{i}\"\n")).collect::<String>()),
        format!(
            "gitGraph\n commit\n branch b\n commit\n{}",
            (0..8).map(|i| format!(" checkout {}\n commit\n merge {}\n", if i % 2 == 1 { "main" } else { "b" }, if i % 2 == 1 { "b" } else { "main" })).collect::<String>()
        ),
    ];
    for source in more {
        for options in
            [json!({}), json!({ "maxWidth": 320 }), json!({ "config": { "gitGraph": { "parallelCommits": true } } })]
        {
            out.push((source.clone(), options));
        }
    }
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("git cases");
    assert!(tally.identical > 60, "the cases render");
}

/// Pele's own worst cases (`tests/git/stress.test.ts`) that it renders, each with the source limit
/// raised.
fn worst_cases() -> Vec<(String, Value)> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let config = |yaml: &str| format!("---\nconfig:\n  gitGraph:\n{yaml}\n---\n");
    let zigzag = repeat(1200, &|i| {
        format!(
            "checkout {}\ncommit\nmerge {}\n",
            if i % 2 == 1 { "main" } else { "b" },
            if i % 2 == 1 { "b" } else { "main" }
        )
    });
    let picks = format!(
        "gitGraph\ncommit id:\"s\"\nbranch b\ncommit\n{}",
        repeat(1500, &|i| format!("checkout main\ncommit id:\"m{i}\"\ncheckout b\ncommit\ncherry-pick id:\"s\"\n"))
    );
    let big = json!({ "limit": 10_000_000 });
    [
        format!("gitGraph\n{}", "commit\n".repeat(7000)),
        format!("{}gitGraph TB:\n{}", config("    rotateCommitLabel: false"), "commit\n".repeat(7000)),
        format!("gitGraph\ncommit\n{}", repeat(3000, &|i| format!("branch b{i}\ncommit\n"))),
        format!(
            "{}gitGraph BT:\ncommit\n{}",
            config("    parallelCommits: true"),
            repeat(3000, &|i| format!("branch b{i}\ncommit\n"))
        ),
        format!("gitGraph\ncommit\n{}", repeat(1500, &|i| format!("branch b{i}\ncommit\ncheckout main\nmerge b{i}\n"))),
        format!("gitGraph\ncommit\nbranch b\ncommit\n{zigzag}"),
        format!("{}gitGraph\ncommit\nbranch b\ncommit\n{zigzag}", config("    parallelCommits: true")),
        picks.clone(),
        format!("{}{}", config("    parallelCommits: true"), picks.replace("gitGraph", "gitGraph TB:")),
        format!("gitGraph\ncommit{}\nbranch b\ncommit{}", " tag:\"t\"".repeat(4000), " tag:\"u\"".repeat(2000)),
        format!("gitGraph\n{}", "commit id:\"same\"\n".repeat(3000)),
        format!("gitGraph\ncommit id:\"{}\"", "word ".repeat(10000)),
        format!("gitGraph\ncommit\nbranch {}\ncommit", "x".repeat(50000)),
        format!("gitGraph\ncommit\nbranch b order: {}\ncommit", "9".repeat(50000)),
        format!("gitGraph\ntitle {}\ncommit", "x".repeat(50000)),
    ]
    .into_iter()
    .map(|source| (source, big.clone()))
    .collect()
}

#[test]
fn worst_cases_match_pele() {
    run("worst", &worst_cases()).finish("Pele's worst-case git graphs");
}

#[test]
fn random_graphs_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_git(&mut rng), random_options(&mut rng))).collect();
    let tally = run("random", &cases).finish("random git graphs");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random graph's output has {missed:?}");
}
