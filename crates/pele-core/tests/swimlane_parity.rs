//! Differential test for drawing swimlanes: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele rejects
//! must be rejected here with Pele's message.
//!
//! `tests/fixtures/swimlane/harvest.json` holds the swimlane sources of Pele's own test suite,
//! the flowchart harvest with swimlane headers, and seeded mutations, written by
//! `scripts/harvest-swimlane.mjs`. The widths put every source through the attempts of
//! `turn_to_fit` and `tighten`, where lanes are set closer and labels wrap narrower.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/swimlane/harvest.json");
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
            "maxEdges" => parse.limits.max_edges = count(value),
            "links" => layout.resources.links = flag(value),
            "images" => layout.resources.images = flag(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
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

/// What in Pele's SVG shows that a source went through one path of the draw step.
const PATHS: &[(&str, &str)] = &[
    ("a title", "class=\"pele-title\""),
    ("an accessible title", "aria-labelledby="),
    ("a lane", "class=\"pele-cluster pele-lane"),
    ("a lane with an author's class", "class=\"pele-cluster pele-lane c1\""),
    ("a lane title turned to run across", "<g transform=\"rotate(-90 "),
    ("the lane of loose nodes", "data-id=\"__swimlane_default__\""),
    ("a lane with a style", "fill=\"var(--_a)\" stroke=\"var(--_b)\" style=\""),
    ("a lane title in the ink of its fill", "class=\"pele-cluster-label\" fill=\"var(--pele-fg-on-"),
    ("a group inside a lane", "<g class=\"pele-cluster\" data-id="),
    ("a group title struck by an edge", "class=\"pele-cluster-titles\""),
    ("a casing", "class=\"pele-casings\""),
    ("an edge in the ink of a fill", "\" stroke=\"var(--pele-fg-on-"),
    ("an edge to a lane", "_Lane0_"),
    ("a self-loop", "data-id=\"L_Self_Self_"),
    ("an edge label", "class=\"pele-edge-label\""),
    ("a dotted edge", "stroke-dasharray=\"3 4\""),
    ("a thick edge", "stroke-width=\"2.5\""),
    ("a marker", "class=\"pele-marker\""),
    ("a label of several lines", "xml:space=\"preserve\""),
    ("a rounded shape", "rx:var(--_r)"),
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

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<Rendered, RenderError>, theirs: &PeleRender) {
        let problem = match (theirs, ours) {
            (PeleRender::Svg { r#type, svg, width, height }, Ok(ours)) => {
                if ours.svg != *svg {
                    Some(first_difference(&ours.svg, svg))
                } else if ours.width != *width || ours.height != *height {
                    Some(format!("size {}x{}, pele {width}x{height}", ours.width, ours.height))
                } else {
                    *if r#type == "swimlane" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached.extend(PATHS.iter().filter(|(_, mark)| svg.contains(mark)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "swimlane" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "swimlane" => {
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
                Some(format!("pele rejects a swimlane, ours reports it as not ported: {error}"))
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
            self.failures[..self.failures.len().min(12)].join("\n")
        );
        self
    }

    /// Fails unless the identical outputs went through every path named in [`PATHS`].
    fn reaches_every_path(&self, what: &str) {
        let missed: Vec<&str> =
            PATHS.iter().map(|(name, _)| *name).filter(|name| !self.reached.contains(name)).collect();
        assert!(missed.is_empty(), "no output of the {what} has {missed:?}");
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

/// The widths a host may have. 320 and 480 are under the breakpoint at which a swimlane that
/// runs across is turned to run down, 640 is on it and 900 over it.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

/// How many of the cases Pele draws another way for the width they give than with room to spare.
fn redrawn_to_fit(cases: &[(String, Value)]) -> usize {
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

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested swimlane sources");
    assert!(tally.identical > 100, "the harvest holds swimlanes Pele renders");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested swimlane sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn converted_flowcharts_match_pele() {
    let tally = run("converted", &plain(harvest("converted"))).finish("flowcharts with swimlane headers");
    assert!(tally.identical > 1000, "most converted flowcharts render");
}

#[test]
fn converted_flowcharts_match_pele_at_each_width() {
    let sources = harvest("converted");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("converted", &cases).finish(&format!("flowcharts with swimlane headers at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the converted flowcharts another way");
        assert!(width >= 640.0 || redrawn > 10, "maxWidth {width} changes some drawing");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated swimlane sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "swimlane-beta LR\n  subgraph Customer\n    browse[Browse the catalogue] --> pay[Pay]\n  end\n  subgraph Warehouse [Warehouse team]\n    pick[Pick item] --> ship[Ship it]\n  end\n  browse --> pick\n  pay -->|confirmed| pick\n  ship -.->|tracking number| browse\n  ship ==> pay";
    let groups = "swimlane-beta TB\n  subgraph Lane0 [Sales]\n    subgraph inner [A group in the lane]\n      direction LR\n      a --> b\n    end\n    c\n  end\n  subgraph Lane1\n    d --> e\n  end\n  loose --> a\n  Lane0 --> Lane1\n  e --> Lane0\n  inner --> d\n  x --> inner";
    let filled = "swimlane-beta LR\n  subgraph Dark [Dark lane]\n    a --> b\n    b --> b\n  end\n  subgraph Pale\n    c --> d\n  end\n  a --> c\n  d --> a\n  style Dark fill:#036,color:#fff\n  style Pale fill:#ffd,stroke:#f00\n  classDef c1 fill:#ddf\n  class Pale c1";
    let mut out = vec![];
    for source in [across, groups, filled] {
        for options in [
            json!({}),
            json!({ "maxWidth": 200 }),
            json!({ "maxWidth": 200, "autoDirection": false }),
            json!({ "maxWidth": 300, "directionBreakpoint": 250 }),
            json!({ "maxWidth": 320.5, "padding": 0, "fontSize": 13 }),
            json!({ "responsive": false, "idPrefix": "a\"b", "padding": 20.5 }),
            json!({ "config": { "swimlane": { "curve": "step", "nodeSpacing": 10, "rankSpacing": 90, "useMaxWidth": false } } }),
            json!({ "config": { "flowchart": { "curve": "linear", "wrappingWidth": 60 } } }),
            json!({ "config": { "swimlane": "not a section", "flowchart": { "curve": "stepAfter" } } }),
        ] {
            out.push((source.to_owned(), options));
        }
        for direction in ["BT", "RL", "TD"] {
            let turned = source.replacen(" LR\n", &format!(" {direction}\n"), 1).replacen(
                " TB\n",
                &format!(" {direction}\n"),
                1,
            );
            out.push((turned.clone(), json!({})));
            out.push((turned, json!({ "maxWidth": 320 })));
        }
    }
    out.push(("swimlane-beta\n".to_owned() + &"subgraph L\nn\nend\n".repeat(3), json!({})));
    out.push(("swimlane-beta TD\n  a --> b --> c\n  c --> a".to_owned(), json!({ "maxWidth": 320 })));
    out.push(("---\ntitle: A title that is long enough to be wider than the lanes under it\n---\nswimlane-beta LR\n  subgraph A\n    x\n  end".to_owned(), json!({ "maxWidth": 300 })));
    out.push(("swimlane-beta LR\n  accTitle: The <title> & more\n  accDescr: A \"description\"\n  subgraph A\n    x[\"`**Bold** and _italic_\n  and a second line that wraps`\"]\n  end".to_owned(), json!({ "maxWidth": 480 })));
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("swimlane cases");
    assert!(tally.identical > 40, "the cases render");
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

    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

const LABELS: &[&str] = &[
    "Start",
    "A longer label with several words in it",
    "\"`**Bold** and _italic_ words`\"",
    "one<br>two",
    "#35; and #amp;",
    "x",
    "Supercalifragilisticexpialidocious",
];
const BRACKETS: &[(&str, &str)] =
    &[("[", "]"), ("(", ")"), ("([", "])"), ("{", "}"), ("((", "))"), ("[[", "]]"), ("[(", ")]")];
const LINKS: &[&str] = &[
    "-->",
    "-->",
    "-->",
    "---",
    "-.->",
    "==>",
    "--x",
    "--o",
    "<-->",
    "---->",
    "-- yes -->",
    "-->|a label|",
    "-->|a much longer label that wraps onto more than one line of text|",
];
const DECLARATIONS: &[&str] = &[
    "fill:#f9f",
    "fill:#036",
    "fill:#036,color:#fff",
    "fill:#000,fill-opacity:0.3",
    "stroke:#333,stroke-width:4px",
    "color:red",
    "font-weight:bold",
    "fill:none",
];
const CURVES: &[&str] = &["basis", "linear", "step", "stepBefore", "stepAfter"];

fn node(rng: &mut Rng, most: usize) -> String {
    let id = format!("N{}", rng.below(most));
    if rng.chance(3) {
        let (open, close) = BRACKETS[rng.below(BRACKETS.len())];
        format!("{id}{open}{}{close}", rng.pick(LABELS))
    } else {
        id
    }
}

/// A swimlane that Pele nearly always renders, with host options, to put many graphs through
/// the lanes, the handoffs between them, the fills and the width attempts.
fn random_swimlane(rng: &mut Rng) -> (String, Value) {
    let mut source = String::new();
    let mut config = String::new();
    if rng.chance(4) {
        config += &format!("    curve: {}\n", rng.pick(CURVES));
    }
    if rng.chance(6) {
        config += &format!("    nodeSpacing: {}\n    rankSpacing: {}\n", 5 + rng.below(90), 5 + rng.below(90));
    }
    if rng.chance(6) {
        config += &format!("    wrappingWidth: {}\n", 40 + rng.below(300));
    }
    let title = rng.chance(6);
    if !config.is_empty() || title {
        source += "---\n";
        if title {
            source += &format!(
                "title: {}\n",
                rng.pick(&["A title", "A much longer title than the lanes are wide, most likely"])
            );
        }
        if !config.is_empty() {
            source += &format!("config:\n  {}:\n{config}", rng.pick(&["swimlane", "flowchart"]));
        }
        source += "---\n";
    }
    source += &format!("swimlane-beta {}\n", rng.pick(&["LR", "LR", "TB", "TD", "RL", "BT", ""]));
    let lanes = 1 + rng.below(5);
    let many = rng.chance(5);
    let most = 3 + rng.below(if many { 30 } else { 10 });
    let mut groups = Vec::new();
    for lane in 0..lanes {
        let title = match rng.below(3) {
            0 => String::new(),
            1 => format!(" [{}]", rng.pick(LABELS)),
            _ => " [\"A lane title that is rather long\"]".to_owned(),
        };
        source += &format!("subgraph Lane{lane}{title}\n");
        if rng.chance(4) {
            let name = format!("G{lane}");
            source += &format!("subgraph {name} [\"A group title that is rather long, to be crossed\"]\n");
            for _ in 0..1 + rng.below(3) {
                source += &format!("{}\n", node(rng, most));
            }
            source += "end\n";
            groups.push(name);
        }
        for _ in 0..1 + rng.below(4) {
            source += &format!("{}\n", node(rng, most));
        }
        source += "end\n";
    }
    let mut edges = 0;
    for _ in 0..rng.below(2 * most) {
        let from = if rng.chance(15) { "Lane0".to_owned() } else { node(rng, most + 2) };
        let to = if rng.chance(15) {
            groups.first().cloned().unwrap_or_else(|| "Lane0".to_owned())
        } else {
            node(rng, most + 2)
        };
        source += &format!("{from} {} {to}\n", rng.pick(LINKS));
        edges += 1;
    }
    if rng.chance(5) {
        source += "Self --> Self\nSelf -->|again| Self\n";
        edges += 2;
    }
    for _ in 0..rng.below(3) {
        source += &match rng.below(5) {
            0 => format!("style Lane{} {}\n", rng.below(lanes), rng.pick(DECLARATIONS)),
            4 if !groups.is_empty() => {
                format!("style {} {}\n", groups[rng.below(groups.len())], rng.pick(DECLARATIONS))
            }
            1 => format!("style N{} {}\n", rng.below(most), rng.pick(DECLARATIONS)),
            2 => format!("classDef c1 {}\nclass Lane{} c1\n", rng.pick(DECLARATIONS), rng.below(lanes)),
            _ if edges > 0 => format!("linkStyle {} {}\n", rng.below(edges), rng.pick(DECLARATIONS)),
            _ => "%% nothing\n".to_owned(),
        };
    }
    if rng.chance(8) {
        source += "accTitle: The title\naccDescr: A description\n";
    }

    let mut options = serde_json::Map::new();
    if rng.below(3) > 0 {
        let width = match rng.below(5) {
            0 => 100.0 + rng.below(900) as f64 + 0.5,
            width => WIDTHS[width - 1],
        };
        options.insert("maxWidth".to_owned(), width.into());
    }
    if rng.chance(8) {
        options.insert("autoDirection".to_owned(), false.into());
    }
    if rng.chance(8) {
        options.insert("directionBreakpoint".to_owned(), (200.0 + rng.below(800) as f64).into());
    }
    if rng.chance(8) {
        options.insert("padding".to_owned(), (rng.below(41) as f64 / 2.0).into());
    }
    if rng.chance(8) {
        options.insert("fontSize".to_owned(), [12.0, 13.0, 14.0, 18.5, 20.0][rng.below(5)].into());
    }
    (source, Value::Object(options))
}

#[test]
fn random_swimlanes_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..3000).map(|_| random_swimlane(&mut rng)).collect();
    let tally = run("random swimlane", &cases).finish("random swimlanes");
    assert!(tally.identical > 2700, "nearly every random swimlane renders");
    tally.reaches_every_path("random swimlanes");
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random swimlanes another way");
    assert!(redrawn > 500, "the widths put many random swimlanes through the attempts to fit");
}
