//! Differential test for drawing agentflows: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/agentflow/harvest.json` holds the agentflow sources of Pele's own test suite,
//! the sources of its reference corpus and the inputs of its seeded mutation runs, written by
//! `scripts/harvest-agentflow.mjs`. The widths put every diagram that runs across through both
//! attempts of `turn_to_fit`.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/agentflow/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// This port's render with Pele's render options, which are what `render_with` gives the oracle.
fn render_with(source: &str, options: &Value) -> Result<Rendered, RenderError> {
    let mut parse = ParseOptions::default();
    let mut layout = LayoutOptions::default();
    let number = |value: &Value| value.as_f64().expect("a number");
    let flag = |value: &Value| value.as_bool().expect("a boolean");
    let count = |value: &Value| usize::try_from(value.as_u64().expect("a count")).expect("a count");
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    let list = |value: &Value| value.as_array().expect("a list").iter().map(text).collect::<Vec<String>>();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = Some(number(value)),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(text(value)),
            "responsive" => layout.responsive = Some(flag(value)),
            "autoDirection" => layout.auto_direction = flag(value),
            "directionBreakpoint" => layout.direction_breakpoint = Some(number(value)),
            "links" => layout.resources.links = flag(value),
            "linkSchemes" => layout.resources.link_schemes = list(value),
            "linkRel" => layout.resources.link_rel = text(value),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

/// A drawing path, and how an identical output shows that it went through it: by a mark in
/// Pele's SVG, or, where the SVG has none of its own, by the `%% tag` the random generator writes
/// beside the statements that take it.
type Reached = fn(&str, &str) -> bool;

const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("a flow", |_, svg| svg.contains("class=\"pele-cluster pele-flow")),
    ("a flow with a title", |_, svg| svg.contains("class=\"pele-cluster-label\"")),
    ("a flow in a flow", |source, _| source.contains("%% nested")),
    ("a flow that runs its own way", |source, _| source.contains("%% across")),
    ("a collapsed flow", |_, svg| svg.contains("pele-shape-collapsed")),
    ("an edge that ends at a collapsed flow", |source, _| source.contains("%% into collapsed")),
    ("a fill of its own in place of the wash", |_, svg| svg.contains(")\" stroke=\"var(--pele-series-")),
    ("a rounded node", |_, svg| svg.contains("pele-shape-rounded")),
    ("a framed node", |_, svg| svg.contains("pele-shape-fr-rect")),
    ("a leaning node", |_, svg| svg.contains("pele-shape-lean-r")),
    ("a diamond", |_, svg| svg.contains("pele-shape-diam")),
    ("a lined document", |_, svg| svg.contains("pele-shape-lin-doc")),
    ("a hexagon", |_, svg| svg.contains("pele-shape-hex")),
    ("a task", |_, svg| svg.contains("pele-kind-task")),
    ("a tool", |_, svg| svg.contains("pele-kind-tool")),
    ("a decision", |_, svg| svg.contains("pele-kind-decision")),
    ("an input", |_, svg| svg.contains("pele-kind-input")),
    ("a reference document", |_, svg| svg.contains("pele-kind-refdoc")),
    ("an action", |_, svg| svg.contains("pele-kind-action")),
    ("a connector", |_, svg| svg.contains("pele-kind-connector")),
    ("a sequence edge", |_, svg| svg.contains("pele-edge-sequence")),
    ("a reference edge", |_, svg| svg.contains("pele-edge-reference")),
    ("a failure edge", |_, svg| svg.contains("pele-edge-failure")),
    ("a self-loop", |source, _| source.contains("%% loop")),
    ("a self-loop with a label", |source, _| source.contains("%% labelled loop")),
    ("an edge out of a node to the flow around it", |source, _| source.contains("%% held out")),
    ("an edge into a node from the flow around it", |source, _| source.contains("%% held in")),
    ("a casing", |_, svg| svg.contains("class=\"pele-casings\"")),
    ("an edge in the ink of the fill under it", |_, svg| svg.contains("\" stroke=\"var(--pele-fg-on-")),
    ("an edge label", |_, svg| svg.contains("class=\"pele-edge-label\"")),
    ("an edge with a style", |source, _| source.contains("%% edge style")),
    ("a stepped edge", |source, _| source.contains("%% step")),
    ("a straight edge", |source, _| source.contains("%% linear")),
    ("a link", |_, svg| svg.contains("<a href=")),
    ("a link inside the host", |_, svg| svg.contains("<a class=\"internal-link\"")),
    ("a tooltip", |_, svg| svg.contains("<title>")),
    ("an author's class", |_, svg| svg.contains(" hot\" data-id=")),
    ("a label in the ink of its fill", |_, svg| svg.contains("class=\"pele-label\" fill=\"var(--pele-fg-on-")),
    ("a bold label", |_, svg| svg.contains("font-weight=\"var(--_w)\"")),
    ("a label of several runs", |_, svg| svg.contains("xml:space=\"preserve\"")),
    ("an accessible title", |_, svg| svg.contains("aria-labelledby=")),
    ("a diagram that runs across", |source, _| {
        source.contains("agentflow-beta LR") || source.contains("agentflow-beta RL")
    }),
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
                    *if r#type == "agentflow" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "agentflow" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "agentflow" => {
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
                Some(format!("pele rejects an agentflow, ours reports it as not ported: {error}"))
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
        println!("{what} reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
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

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

/// The widths a host may have. 320 and 480 are under the breakpoint at which a diagram that
/// runs across is turned to run down, 640 is on it and 900 over it.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested agentflow sources");
    assert!(tally.identical > 2000, "the harvest holds agentflows Pele renders");
    assert!(tally.rejected > 1000, "the harvest holds agentflows Pele rejects");
}

#[test]
fn reference_sources_match_pele() {
    let tally = run("reference", &plain(harvest("reference"))).finish("agentflow reference sources");
    assert!(tally.identical > 150, "the reference holds agentflows Pele renders");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested agentflow sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
        if width < 640.0 {
            assert!(redrawn > 0, "maxWidth {width} turns some diagram");
        }
    }
}

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

#[test]
fn mutated_sources_match_pele() {
    let tally = run("mutated", &plain(harvest("mutated"))).finish("mutated agentflow sources");
    assert!(tally.identical > 1000, "mutation leaves agentflows Pele renders");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "agentflow-beta LR\n  start@{ shape: input } --> plan[\"Plan the work\"] --> pick{\"Tool?\"}\n  pick -- yes --> run[[\"Run tool\"]]\n  pick -- \"no\" --x fail@{ shape: action, label: \"Give up\" }\n  run -.- notes@{ shape: refdoc }\n  flow inner[\"Inner\"]\n    direction TB\n    run --> check --> run\n  end\n";
    let linked = "agentflow-beta TB\n  a[\"A & B\"] --> b --> c:::internal-link\n  d[\"Note #35;1\"]:::internal-link --> a\n  click a \"https://example.com/a?b=1&c=2\" \"A tip & more\" _blank\n  click b href \"javascript:alert(1)\"\n  click c \"file:///etc/passwd\" \"#35; tip\"\n  click d call cb() \"called\"\n";
    let filled = "agentflow-beta TB\n  flow dark[\"Dark\"]\n    a --> b\n    b --> b\n    a --> dark\n  end\n  flow pale[\"Pale\"]\n    c --> d\n    pale --> d\n  end\n  flow faint[\"Faint\"]\n    e --> f\n  end\n  a --> c\n  b --x e\n  d -.- f\n  classDef dark fill:#036,color:white\n  classDef pale fill:#ffd,opacity:0.9\n  classDef faint fill:#000,fill-opacity:0.2\n  class dark dark\n  class pale pale\n  class faint faint\n  class a,d dark\n  linkStyle 0 stroke:red,stroke-width:3px\n  linkStyle 1 stroke:none\n  linkStyle 5 stroke-width:2em,opacity:0.5\n";
    let held = "agentflow-beta TB\n  flow outer[\"Outer\"]\n    flow inner[\"Inner\"]\n      a --> b\n      a --> inner\n      inner --> b\n      b --> outer\n    end\n    c --> outer\n    outer -- \"labelled\" --> c\n    inner --> outer\n    outer --x inner\n  end\n  flow side\n    direction LR\n    x --> y --> z\n    x --> side\n    side --> z\n    y --> side\n  end\n";
    let collapsed = "agentflow-beta TB\n  flow f[\"Folded\"]@{ view: collapsed }\n    a --> b\n    flow g\n      c\n    end\n  end\n  flow h[\"`**Bold** and _slanted_`\"]\n    x --> a\n    c --> y\n    f --> f\n  end\n  y --> z@{ shape: tool, label: \"A tool\" }\n  style f fill:#246,stroke-dasharray:2 2\n  classDef hot fill:#f00,color:#fff,font-weight:bold\n  class z,h hot\n";
    let curves = "agentflow-beta LR\n  a e1@--> b\n  a e2@--> c\n  b e3@--> d\n  c --> d\n  d --> a\n  e1@{ curve: linear }\n  e2@{ curve: step }\n  e3@{ curve: stepBefore }\n  linkStyle 3 interpolate stepAfter\n  linkStyle 4 interpolate basis stroke:#06c\n";
    let config = |agentflow: Value| json!({ "config": { "agentflow": agentflow } });
    let mut out = vec![
        (across.to_owned(), json!({ "maxWidth": 200 })),
        (across.to_owned(), json!({ "maxWidth": 200, "autoDirection": false })),
        (across.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (across.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (across.to_owned(), json!({ "maxWidth": 0 })),
        (across.to_owned(), json!({ "maxWidth": 320.5, "padding": 0, "fontSize": 13 })),
        (across.to_owned(), json!({ "responsive": false, "idPrefix": "a\"b", "padding": 20.5 })),
        (across.to_owned(), config(json!({ "nodeSpacing": 10, "rankSpacing": 5000, "wrappingWidth": 60 }))),
        (across.to_owned(), config(json!({ "nodeSpacing": "x", "rankSpacing": -1, "minNodeWidth": 20 }))),
        (across.to_owned(), config(json!({ "useMaxWidth": false, "minNodeWidth": 3000, "wrappingWidth": 0.5 }))),
        (across.to_owned(), json!({ "config": { "flowchart": { "curve": "step", "inheritDir": true } } })),
        (linked.to_owned(), json!({})),
        (linked.to_owned(), json!({ "links": false })),
        (linked.to_owned(), json!({ "linkRel": "" })),
        (linked.to_owned(), json!({ "linkRel": "nofollow \"x\"", "linkSchemes": ["file"] })),
        (filled.to_owned(), json!({})),
        (filled.to_owned(), json!({ "config": { "flowchart": { "curve": "linear" } } })),
        (held.to_owned(), json!({})),
        (held.to_owned(), json!({ "config": { "flowchart": { "curve": "stepBefore" } } })),
        (collapsed.to_owned(), json!({})),
        (curves.to_owned(), json!({})),
        (curves.to_owned(), json!({ "config": { "flowchart": { "curve": "stepAfter" } } })),
    ];
    let more = [
        "agentflow-beta TB\n  accTitle: The <title> & more\n  accDescr: A \"description\"\n  a --> a\n  a -- twice --> a\n  a -- \"a much longer label for the third loop\" --> a\n  b --x b\n",
        "---\ntitle: A title that is long enough to be wider than the diagram under it\n---\nagentflow-beta TB\n  a --> b\n",
        "agentflow-beta LR\n  a --> a\n  a -- loop --> a\n  flow g\n    x --> y\n    x --> x\n  end\n  g --> g\n",
        "agentflow-beta TB\n  a@{ shape: tool } --> b@{ shape: decision } --> c@{ shape: input } --> d@{ shape: refdoc } --> e@{ shape: action } --> f@{ shape: task }\n  g@{ shape: subproc } --> h@{ shape: hex } --> i@{ shape: lined-document } --> j@{ shape: stadium } --> k@{ shape: tri }\n  connector gh[\"GitHub\"]@{ protocol: mcp }\n  k --> gh\n",
        "agentflow-beta TB\n  a(round) --> b((circle)) --> c([stadium]) --> d[(db)] --> e>odd] --> f{{hex}} --> g(((dbl)))\n  h[/in/] --> i[\\out\\] --> j[/trap\\] --> k[\\inv/] --> l[[tool]] --> m{dec} --> n(-ell-)\n",
        "agentflow-beta TB\n  a[\"`**bold** and _slanted_ text that is long enough to wrap in its node`\"] --> b[\"<b>tag</b> line<br>break\"]\n  b -- \"`a **bold** edge`\" --> c\n  style a fill:#f9f,stroke:#333,stroke-width:4px,color:#fff,font-weight:bold,font-size:20px,font-style:italic\n  style b stroke:a&fill:navy,opacity:50%\n",
        "agentflow-beta TB\n  flow a\n    b\n  end\n  flow b\n    a\n    c\n  end\n  c --> a\n  b --> c\n",
        "agentflow-beta TB\n  global\n    shared\n  end\n  flow f\n    a --> shared\n  end\n  flow g\n    shared --> b\n  end\n",
        "agentflow-beta BT\n  flow f[\"A title wide enough to set the width of the flow it names\"]\n    a\n  end\n  x --> a\n  y --> a\n  z --> a\n",
        "agentflow-beta RL\n  flow f[\"T\"]\n    a --> b\n  end\n  a --> f\n  f --> b\n  c --> f\n",
        "agentflow-beta TB\n  \u{65e5}\u{672c}\u{8a9e} -- \"emoji \u{1f600}\" --> b[\"#quot;quoted#quot; #amp; #lt;tag#gt; #35;\"]\n",
        "agentflow-beta TB\n  flow empty\n  end\n  flow f@{ view: collapsed }\n  end\n  a\n",
    ];
    out.extend(more.iter().flat_map(|source| {
        [json!({}), json!({ "maxWidth": 320 }), json!({ "maxWidth": 120 })]
            .map(|options| ((*source).to_owned(), options))
    }));
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("hand-written agentflows");
    assert_eq!(tally.rejected, 0, "every hand-written agentflow is one Pele renders");
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

const NODES: &[&str] = &["a", "b", "c", "d", "e", "n1", "n2", "long_id", "é"];
const TEXTS: &[&str] = &[
    "Run",
    "Plan the work",
    "`**bold** text`",
    "#35; x &amp; y",
    "a label long enough to wrap around inside the node it sits in",
    "x<br>y",
    "é ö 日本",
];
const SHAPES: &[&str] =
    &["tool", "task", "input", "decision", "refdoc", "action", "hex", "subproc", "lined-document", "stadium", "tri"];

/// A random agentflow that Pele renders, with a `%% tag` beside each statement whose drawing
/// path the SVG does not name.
struct Generator {
    rng: Rng,
    out: String,
    edges: usize,
    flows: usize,
}

/// Appends one statement, indented for its depth.
fn line(out: &mut String, depth: usize, text: &str) {
    *out += &"  ".repeat(depth + 1);
    *out += text;
    *out += "\n";
}

impl Generator {
    fn node(&mut self) -> String {
        let id = self.rng.pick(NODES);
        let text = self.rng.pick(TEXTS);
        match self.rng.below(14) {
            0..=5 => id.to_owned(),
            6 => format!("{id}[\"{text}\"]"),
            7 => format!("{id}{{\"{text}\"}}"),
            8 => format!("{id}[[\"{text}\"]]"),
            9 => format!("{id}{{{{\"{text}\"}}}}"),
            10 => format!("{id}@{{ shape: {} }}", self.rng.pick(SHAPES)),
            11 => format!("{id}@{{ shape: {}, label: \"{text}\" }}", self.rng.pick(SHAPES)),
            12 => format!("{id}:::{}", self.rng.pick(&["hot", "pale", "internal-link"])),
            _ => format!("{id}{}", self.rng.pick(&["(round)", "((circle))", "[/in/]", "[plain]"])),
        }
    }

    fn link(&mut self) -> String {
        let text = self.rng.pick(TEXTS);
        match self.rng.below(12) {
            0..=4 => "-->".to_owned(),
            5 => "--x".to_owned(),
            6 => "-.-".to_owned(),
            7 => self.rng.pick(&["--->", "---x", "-..-"]).to_owned(),
            8 | 9 => format!("-- \"{text}\" -->"),
            10 => format!("-- \"{text}\" --x"),
            _ => format!("-->|\"{text}\"|"),
        }
    }

    fn edge(&mut self, depth: usize) {
        let (from, link, to) = (self.node(), self.link(), self.node());
        line(&mut self.out, depth, &format!("{from} {link} {to}"));
        self.edges += 1;
    }

    fn flow(&mut self, depth: usize) {
        let id = format!("f{}", self.flows);
        self.flows += 1;
        let collapsed = self.rng.chance(5);
        let mut header = format!("flow {id}");
        if self.rng.chance(2) {
            header += &format!("[\"{}\"]", self.rng.pick(TEXTS));
        }
        if collapsed {
            header += "@{ view: collapsed }";
        }
        if depth > 0 {
            header += " %% nested";
        }
        line(&mut self.out, depth, &header);
        if self.rng.chance(3) {
            line(
                &mut self.out,
                depth + 1,
                &format!("direction {} %% across", self.rng.pick(&["TB", "LR", "RL", "BT"])),
            );
        }
        for _ in 0..1 + self.rng.below(3) {
            match self.rng.below(10) {
                0 | 1 if depth < 2 => self.flow(depth + 1),
                2 if !collapsed => {
                    let node = self.rng.pick(NODES);
                    let link = self.link();
                    line(&mut self.out, depth + 1, &format!("{node} {link} {id} %% held out"));
                    self.edges += 1;
                }
                3 if !collapsed => {
                    let node = self.rng.pick(NODES);
                    let link = self.link();
                    line(&mut self.out, depth + 1, &format!("{id} {link} {node} %% held in"));
                    self.edges += 1;
                }
                _ => self.edge(depth + 1),
            }
        }
        line(&mut self.out, depth, "end");
        if collapsed && self.rng.chance(2) {
            let node = self.rng.pick(&["out1", "out2"]);
            line(&mut self.out, depth, &format!("{node} --> {id} %% into collapsed"));
            self.edges += 1;
        }
        if self.rng.chance(4) {
            line(&mut self.out, depth, &format!("class {id} {}", self.rng.pick(&["hot", "pale", "faint"])));
        }
    }

    fn statement(&mut self) {
        match self.rng.below(30) {
            0..=9 => self.edge(0),
            10..=13 => self.flow(0),
            14 => {
                let node = self.rng.pick(NODES);
                line(&mut self.out, 0, &format!("{node} --> {node} %% loop"));
                self.edges += 1;
            }
            15 => {
                let node = self.rng.pick(NODES);
                line(&mut self.out, 0, &format!("{node} -- \"{}\" --> {node} %% labelled loop", self.rng.pick(TEXTS)));
                self.edges += 1;
            }
            16 => {
                let title = self.rng.pick(TEXTS);
                line(&mut self.out, 0, &format!("connector {}[\"{title}\"]", self.rng.pick(&["gh", "db"])));
                line(&mut self.out, 0, &format!("{} --> {}", self.rng.pick(NODES), self.rng.pick(&["gh", "db"])));
                self.edges += 1;
            }
            17 | 18 => {
                let node = self.rng.pick(NODES);
                line(&mut self.out, 0, &format!("class {node} {}", self.rng.pick(&["hot", "pale", "faint"])));
            }
            19 => {
                let styles = self.rng.pick(&[
                    "fill:#bbf",
                    "stroke:red,stroke-dasharray:3 3",
                    "color:green",
                    "fill:#123,color:#fff",
                ]);
                line(&mut self.out, 0, &format!("style {} {styles}", self.rng.pick(NODES)));
            }
            20 | 21 if self.edges > 0 => {
                let at = self.rng.below(self.edges);
                let (rest, tag) = *[
                    ("stroke:red,stroke-width:3px", "edge style"),
                    ("stroke:none", "edge style"),
                    ("interpolate step", "step"),
                    ("interpolate stepAfter stroke:#06c", "step"),
                    ("interpolate linear", "linear"),
                    ("interpolate basis", "edge style"),
                ]
                .get(self.rng.below(6))
                .expect("one of six");
                line(&mut self.out, 0, &format!("linkStyle {at} {rest} %% {tag}"));
            }
            22 => {
                let action = self.rng.pick(&[
                    "href \"https://example.com/a?b=1&c=2\" \"A tip\" _blank",
                    "\"https://example.com\"",
                    "call cb() \"called\"",
                    "\"javascript:alert(1)\" \"blocked\"",
                ]);
                line(&mut self.out, 0, &format!("click {} {action}", self.rng.pick(NODES)));
            }
            23 => {
                let (from, to) = (self.rng.pick(NODES), self.rng.pick(NODES));
                let id = format!("e{}", self.edges);
                line(&mut self.out, 0, &format!("{from} {id}@--> {to}"));
                self.edges += 1;
                let (curve, tag) = *[("linear", "linear"), ("step", "step"), ("stepBefore", "step")]
                    .get(self.rng.below(3))
                    .expect("one");
                line(&mut self.out, 0, &format!("{id}@{{ curve: {curve} }} %% {tag}"));
            }
            24 => line(&mut self.out, 0, "%% a comment between statements"),
            _ => self.edge(0),
        }
    }

    fn agentflow(rng: Rng) -> (String, Rng) {
        let mut g = Generator { rng, out: String::new(), edges: 0, flows: 0 };
        if g.rng.chance(6) {
            g.out += &format!("---\ntitle: {}\n---\n", g.rng.pick(&["Front", "A longer title for the diagram"]));
        }
        g.out +=
            g.rng.pick(&["agentflow-beta TB\n", "agentflow-beta LR\n", "agentflow-beta RL\n", "agentflow-beta BT\n"]);
        if g.rng.chance(8) {
            g.out += "  accTitle: A title\n  accDescr: A description\n";
        }
        g.out += "  classDef hot fill:#f00,color:#fff,font-weight:bold\n  classDef pale fill:#ffd,opacity:0.9\n  classDef faint fill:#000,fill-opacity:0.2\n";
        for _ in 0..2 + g.rng.below(9) {
            g.statement();
        }
        (g.out, g.rng)
    }
}

#[test]
fn random_agentflows_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut cases: Vec<(String, Value)> = Vec::new();
    for _ in 0..4000 {
        let (source, next) = Generator::agentflow(rng);
        rng = next;
        let options = match rng.below(10) {
            0 => json!({ "maxWidth": 320 }),
            1 => json!({ "maxWidth": 480 }),
            2 => json!({ "maxWidth": 640 }),
            3 => json!({ "maxWidth": 900 }),
            4 => json!({ "config": { "flowchart": { "curve": rng.pick(&["linear", "step", "basis"]) } } }),
            5 => json!({ "config": { "agentflow": { "nodeSpacing": 12, "rankSpacing": 90, "minNodeWidth": 40 } } }),
            _ => json!({}),
        };
        cases.push((source, options));
    }
    let tally = run("random", &cases).finish("random agentflows");
    assert!(tally.identical > 3600, "nearly every random agentflow is one Pele renders");
    tally.reaches_every_path("random agentflows");
}

/// The sources of Pele's `tests/agentflow/stress.test.ts` that it renders: its worst cases
/// for the layout and the drawing.
fn stress() -> Vec<String> {
    let head = "agentflow-beta TB\n";
    let n = 50_000;
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    vec![
        format!("{head}A\nstyle A {}fill:red\n", repeat(10000, &|i| format!("x{i}:1,"))),
        format!("{head}A:::{}z\n", repeat(20000, &|i| format!("c{i},"))),
        format!("{head}a --> b\nlinkStyle {}0 stroke:red\n", "0,".repeat(10000)),
        format!("{head}{}A --> B\n{}", repeat(3000, &|i| format!("flow s{i}\n")), "end\n".repeat(3000)),
        format!(
            "{head}{}A --> B\n{}",
            repeat(3000, &|i| format!("flow s{i}@{{ view: collapsed }}\n")),
            "end\n".repeat(3000)
        ),
        format!(
            "{head}{}A\n{}{}",
            repeat(1500, &|i| format!("flow s{i}\n")),
            "end\n".repeat(1500),
            "A --> B\n".repeat(1000)
        ),
        format!("{head}{}", repeat(5000, &|i| format!("flow s{i}\nn{i}\nend\n"))),
        format!("{head}{}", "flow s\nA\nB\nC\nend\n".repeat(5000)),
        format!("{head}{}", repeat(3000, &|i| format!("flow f{i}\n a{i} --> f{}\nend\n", i + 1))),
        format!(
            "{head}{}",
            repeat(2000, &|i| format!("flow f{i}\n a{i} --> f{}\nend\n", if i == 0 { 1999 } else { i - 1 }))
        ),
        format!(
            "{head}flow f\n{}end\n{}",
            repeat(3000, &|i| format!("n{i}\n")),
            repeat(3000, &|i| format!("global\nn{i}\nend\n"))
        ),
        format!("{head}{}", repeat(5000, &|i| format!("connector c{i}[\"C\"]@{{ protocol: \"mcp\" }}\n"))),
        format!("{head}a\n{}", repeat(5000, &|i| format!("a@{{ k{i}: {i} }}\n"))),
        format!("{head}a e1@--> b\n{}", repeat(5000, &|i| format!("e1@{{ k{i}: {i} }}\n"))),
        format!("{head}a@{{\n{}}}\n", repeat(5000, &|i| format!("  k{i}: \"v\",\n"))),
        format!("{head}a@{{ k: {} }}\n", "[".repeat(n / 2)),
        format!("{head}a@{{\n{}}}\n", "  k: 'a,\n".repeat(n / 9)),
        format!("{head}{}", repeat(4000, &|i| format!("n{i}((x))\n"))),
        format!("{head}{}", repeat(3000, &|i| format!("n{i}@{{ shape: tri }}\n"))),
        format!("{head}a[\"{}\"]", "word ".repeat(n / 5)),
        format!("{head}a[\"`{}`\"]", "*a _b ".repeat(n / 6)),
        format!("{head}a[\"{}\"]", "#35;&amp;<br>".repeat(n / 13)),
        format!("{head}a -- \"{}\" --> b", "word ".repeat(n / 5)),
        format!("{head}{} --> b", "a".repeat(n)),
        format!("{head}{}", repeat(5000, &|i| format!("n{i}\n"))),
        format!("{head}{}", repeat(5000, &|i| format!("n{i} --> n{}\n", i + 1))),
        format!("{head}n{}\n", repeat(5000, &|i| format!(" --> n{i}"))),
        format!("{head}{}", repeat(5000, &|i| format!("c --> n{i}\n"))),
        format!("{head}{}a --> b{}\n", repeat(150, &|i| format!("a{i} & ")), repeat(150, &|i| format!(" & b{i}"))),
        format!("{head}{}{}", "A --> B\n".repeat(2000), "A --> A\n".repeat(2000)),
        format!(
            "{head}{}",
            (0..50).flat_map(|i| (i + 1..50).map(move |j| format!("n{i} --> n{j}\n"))).collect::<String>()
        ),
        format!("{head}flow f\n{}end\n", repeat(1200, &|i| format!("n{i} --> f\n"))),
        format!("{head}flow f\na --> b\n{}end\n", "a --> f\nf --> b\n".repeat(2500)),
        format!("{head}flow a\nA\nend\nflow b\nB\nend\n{}", "a --> b\nA -.- b\n".repeat(1000)),
    ]
}

#[test]
fn stress_sources_match_pele() {
    let options = json!({ "limit": 1_000_000_000u64, "outputLimit": 1_000_000_000u64 });
    let cases: Vec<(String, Value)> = stress().into_iter().map(|source| (source, options.clone())).collect();
    run("stress", &cases).finish("Pele's worst-case agentflows");
}
