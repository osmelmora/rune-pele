//! Differential test for drawing state diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/state/harvest.json` holds the state sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-state.mjs`. The widths put
//! every diagram that runs across through both attempts of `turn_to_fit`.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/state/harvest.json");
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
            "padding" => layout.padding = number(value),
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
    ("a composite", |_, svg| svg.contains("class=\"pele-clusters\"")),
    ("a composite with a fill of its own", |_, svg| svg.contains("rx=\"10\" fill=\"var(--_a)\" stroke=")),
    ("a composite in a composite", |source, _| source.contains("%% nested")),
    ("a composite that runs across inside one that runs down", |source, _| source.contains("%% across")),
    ("a transition from a composite into itself", |source, _| source.contains("%% anchor")),
    ("regions split by lines", |_, svg| svg.contains("class=\"pele-divider\" d=\"M")),
    ("regions in outlines", |_, svg| svg.contains("class=\"pele-divider\" rx=\"10\"")),
    ("a start state", |_, svg| svg.contains("class=\"pele-node pele-state-start")),
    ("an end state", |_, svg| svg.contains("class=\"pele-node pele-state-end")),
    ("a choice", |_, svg| svg.contains("class=\"pele-node pele-state-choice")),
    ("a fork", |_, svg| svg.contains("class=\"pele-node pele-state-fork")),
    ("a join", |_, svg| svg.contains("class=\"pele-node pele-state-join")),
    ("a note", |_, svg| svg.contains("class=\"pele-note")),
    ("a note to the left", |source, _| source.contains("note left of")),
    ("a note edge", |_, svg| svg.contains("pele-note-edge")),
    ("a state with a description", |_, svg| svg.contains("class=\"pele-state-description\"")),
    ("a self-transition", |source, _| source.contains("%% loop")),
    ("a self-transition with a label", |source, _| source.contains("%% labelled loop")),
    ("a self-transition on a composite", |source, _| source.contains("%% composite loop")),
    ("a casing", |_, svg| svg.contains("class=\"pele-casings\"")),
    ("a transition in the ink of the fill under it", |_, svg| {
        svg.contains("class=\"pele-edge pele-transition\" stroke=\"var(--pele-fg-on-")
    }),
    ("a transition label", |_, svg| svg.contains("class=\"pele-edge-label\"")),
    ("a link", |_, svg| svg.contains("<a href=")),
    ("a tooltip", |_, svg| svg.contains("<title>")),
    ("an author's class", |_, svg| svg.contains("class=\"pele-node pele-state hot")),
    ("a label in the ink of its fill", |_, svg| svg.contains("class=\"pele-label\" fill=\"var(--pele-fg-on-")),
    ("a state with a style", |_, svg| svg.contains("fill=\"var(--_s)\" stroke=\"var(--_b)\" style=")),
    ("a bold label", |_, svg| svg.contains("font-weight=\"var(--_w)\"")),
    ("a label of several runs", |_, svg| svg.contains("xml:space=\"preserve\"")),
    ("an accessible title", |_, svg| svg.contains("aria-labelledby=")),
    ("a diagram that runs across", |source, _| source.contains("direction LR") || source.contains("direction RL")),
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
                    *if r#type == "state" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "state" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "state" => {
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
                Some(format!("pele rejects a state diagram, ours reports it as not ported: {error}"))
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested state sources");
    assert!(tally.identical > 1500, "the harvest holds state diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds state diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested state sources at maxWidth {width}"));
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
    run("mutated", &plain(harvest("mutated"))).finish("mutated state sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "stateDiagram-v2\ndirection LR\n[*] --> Idle\nIdle --> Moving : start\nMoving --> Idle : stop\nMoving --> Crash\nCrash --> [*]\nstate Moving {\n  direction TB\n  Slow --> Fast\n  Fast --> Slow\n}";
    let linked = "stateDiagram-v2\na --> b\nstate c {\n x\n}\nclick a \"https://example.com/a?b=1&c=2\" \"A tip & more\"\nclick b href \"javascript:alert(1)\"\nclick c \"file:///etc/passwd\" \"#35; tip\"\nclick x \"\" \"\"\nb --> c";
    let filled = "stateDiagram-v2\nstate Dark {\n  a --> b\n  b --> b : again\n}\nstate Pale {\n  c --> d\n  note right of d : inside\n}\nstate Faint {\n  e --> f\n}\na --> c\nb --> e\nd --> f\nclassDef dark fill:#036,color:white\nclassDef pale fill:#ffd,opacity:0.9\nclassDef faint fill:#000,fill-opacity:0.2\nclass Dark dark\nclass Pale pale\nclass Faint faint";
    let anchored = "stateDiagram-v2\nstate Outer {\n  [*] --> Inner\n  state Inner {\n    a --> b\n  }\n  Inner --> c\n}\nOuter --> a\nb --> Outer\nc --> Outer\nOuter --> Outer\nx --> Outer\nOuter --> y";
    let regions = "stateDiagram-v2\nstate Active {\n  [*] --> NumLockOff\n  NumLockOff --> NumLockOn : EvNumLockPressed\n  --\n  [*] --> CapsLockOff\n  CapsLockOff --> CapsLockOn : EvCapsLockPressed\n  --\n  [*] --> ScrollLockOff\n}\nstate Across {\n  direction LR\n  a1 --> a2\n  --\n  b1 --> b2\n}\n[*] --> Active\nActive --> Across";
    let mut out = vec![
        (across.to_owned(), json!({ "maxWidth": 200 })),
        (across.to_owned(), json!({ "maxWidth": 200, "autoDirection": false })),
        (across.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (across.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (across.to_owned(), json!({ "maxWidth": 0 })),
        (across.to_owned(), json!({ "maxWidth": 320.5, "padding": 0, "fontSize": 13 })),
        (across.to_owned(), json!({ "responsive": false, "idPrefix": "a\"b", "padding": 20.5 })),
        (linked.to_owned(), json!({})),
        (linked.to_owned(), json!({ "links": false })),
        (linked.to_owned(), json!({ "linkRel": "" })),
        (linked.to_owned(), json!({ "linkRel": "nofollow \"x\"", "linkSchemes": ["file"] })),
        (filled.to_owned(), json!({})),
        (filled.to_owned(), json!({ "config": { "flowchart": { "curve": "linear" } } })),
        (anchored.to_owned(), json!({})),
        (anchored.to_owned(), json!({ "config": { "flowchart": { "curve": "step" } } })),
        (regions.to_owned(), json!({})),
        (regions.to_owned(), json!({ "config": { "flowchart": { "curve": "stepBefore" } } })),
        (
            regions.to_owned(),
            json!({ "config": { "flowchart": { "curve": "stepAfter" }, "state": { "nodeSpacing": 10, "rankSpacing": 5000, "wrappingWidth": 60 } } }),
        ),
        (
            regions.to_owned(),
            json!({ "config": { "state": { "useMaxWidth": false, "nodeSpacing": "x", "rankSpacing": -1 } } }),
        ),
    ];
    let more = [
        "stateDiagram-v2\n  accTitle: The <title> & more\n  accDescr: A \"description\"\na --> a\na --> a : twice\na --> a : a much longer label for the third loop\nb --> b",
        "---\ntitle: A title that is long enough to be wider than the diagram under it\n---\nstateDiagram-v2\na --> b",
        "stateDiagram-v2\ndirection LR\na --> a : loop\nstate G {\n x --> y\n}\nG --> G : around\nG --> G",
        "stateDiagram-v2\nstate f <<fork>>\nstate j <<join>>\n[*] --> f\nf --> a\nf --> b\nf --> c\nf --> d\na --> j\nb --> j\nc --> j\nd --> j\nj --> [*]",
        "stateDiagram-v2\ndirection LR\nstate f <<fork>>\n[*] --> f\nf --> a\nf --> b\nstate c <<choice>>\na --> c\nc --> x : yes\nc --> y : no",
        "stateDiagram-v2\nnote left of a : on the left\nnote right of a\n  several\n  lines of **note**\nend note\na --> b\nnote right of b : a note long enough to wrap around in its box when it gets past the width\nnote \"floating\" as N",
        "stateDiagram-v2\na : The title\na : first line\na :\na : <b>bold</b> line\nb : one\nb : two\na --> b",
        "stateDiagram-v2\nstate \"A long description of a state\" as s1\nstate \"x\" as s2:extra\ns1 --> s2 : <i>slanted</i> label\nstyle s1 fill:#f9f,stroke:#333,stroke-width:4px,color:#fff,font-weight:bold,font-size:20px,font-style:italic\nstyle s2 stroke:a&fill:navy,opacity:50%",
        "stateDiagram-v2\nclassDef hot fill:#f00,color:white,font-weight:bold,stroke-width:2px,stroke:yellow\nclassDef cold fill:#00f\na:::hot --> b:::cold\nclass a,b , c hot\nstate g {\n x\n}\nclass g cold\n[*]:::hot --> a",
        "stateDiagram-v2\nstate a0 {\n state a1 {\n  z0\n }\n}\nstate a1 {\n state a0 {\n  z1\n }\n}\no --> z0\nz1 --> o",
        "stateDiagram-v2\nstate root {\n a --> b\n}\nroot --> root\n[*] --> [*]",
        "stateDiagram-v2\nstate A {\n --\n x\n --\n y\n --\n}\nstate B {\n direction LR\n p\n --\n q --> r\n}",
        "stateDiagram-v2\n[*] --> s\ns --> [*]\nstate s {\n [*] --> t\n t --> [*]\n}",
        "stateDiagram-v2\n\u{65e5}\u{672c}\u{8a9e} --> b : emoji \u{1f600}\nb : #quot;quoted#quot; #amp; #lt;tag#gt; #35;",
    ];
    out.extend(more.iter().flat_map(|source| {
        [json!({}), json!({ "maxWidth": 320 }), json!({ "maxWidth": 120 })]
            .map(|options| ((*source).to_owned(), options))
    }));
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("hand-written state diagrams");
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
    "go",
    "a b",
    "**bold**",
    "#35; x",
    "&amp; y",
    "<i>tag</i>",
    "a label long enough to wrap around once it passes the wrapping width of two hundred",
    "1",
    "line<br>break",
];
const STYLES: &[&str] = &[
    "fill:#f00,color:white",
    "fill:#036",
    "stroke:blue,stroke-width:2px",
    "font-weight:bold",
    "fill:#ffd,opacity:0.9",
];

/// A state name: one of a few, so that transitions meet, or a start or end.
fn id(rng: &mut Rng, ids: &[String]) -> String {
    if rng.chance(7) {
        return "[*]".to_owned();
    }
    ids[rng.below(ids.len())].clone()
}

/// One statement in a document whose states are `ids`. `depth` counts the composites around it.
fn statement(rng: &mut Rng, depth: usize, ids: &[String], groups: &mut usize) -> String {
    match rng.below(20) {
        0..=6 => {
            let (a, b) = (id(rng, ids), id(rng, ids));
            let mut out = format!("{a} --> {b}");
            if rng.chance(3) {
                out += &format!(" : {}", rng.pick(LABELS));
            }
            out
        }
        7 => {
            let a = ids[rng.below(ids.len())].clone();
            if rng.chance(2) {
                format!("{a} --> {a} : {} %% labelled loop", rng.pick(LABELS))
            } else {
                format!("{a} --> {a} %% loop")
            }
        }
        8 | 9 if depth < 3 => {
            *groups += 1;
            let name = format!("G{groups}");
            let inner: Vec<String> = (0..3).map(|k| format!("{name}x{k}")).collect();
            let mut out = if rng.chance(3) {
                format!("state \"{}\" as {name} {{", rng.pick(LABELS))
            } else {
                format!("state {name} {{")
            };
            if depth > 0 {
                out += " %% nested";
            }
            out += "\n";
            if rng.chance(3) {
                out += &format!("direction {} %% across\n", rng.pick(&["LR", "RL", "BT", "TB"]));
            }
            for _ in 0..1 + rng.below(4) {
                out += &statement(rng, depth + 1, &inner, groups);
                out += "\n";
                if rng.chance(5) {
                    out += "--\n";
                }
            }
            out += "}";
            match rng.below(5) {
                0 => out += &format!("\n{name} --> {} %% anchor", inner[0]),
                1 => out += &format!("\n{} --> {name} %% anchor", inner[1]),
                2 => out += &format!("\n{name} --> {name} %% composite loop"),
                _ => {}
            }
            if rng.chance(3) {
                out += &format!("\n{} --> {name}", ids[rng.below(ids.len())]);
            }
            // Only the root document's class statements reach a composite.
            if depth == 0 && rng.chance(2) {
                out += &format!("\nclass {name} {}", rng.pick(&["hot", "cold"]));
                if rng.chance(2) {
                    out += &format!("\n{} --> {}", ids[rng.below(ids.len())], inner[rng.below(inner.len())]);
                }
            }
            out
        }
        10 => {
            let name = format!("{}{}", rng.pick(&["f", "j", "c"]), depth);
            let kind = match &name[..1] {
                "f" => rng.pick(&["<<fork>>", "[[fork]]"]),
                "j" => "<<join>>",
                _ => rng.pick(&["<<choice>>", "[[choice]]"]),
            };
            let mut out = format!("state {name} {kind}");
            for _ in 0..1 + rng.below(4) {
                if rng.chance(2) {
                    out += &format!("\n{name} --> {}", id(rng, ids));
                } else {
                    out += &format!("\n{} --> {name}", id(rng, ids));
                }
            }
            out
        }
        11 => match rng.below(3) {
            0 => {
                format!("note {} of {} : {}", rng.pick(&["left", "right"]), ids[rng.below(ids.len())], rng.pick(LABELS))
            }
            1 => format!(
                "note {} of {}\n  {}\n  {}\nend note",
                rng.pick(&["left", "right"]),
                ids[rng.below(ids.len())],
                rng.pick(LABELS),
                rng.pick(LABELS)
            ),
            _ => format!("note \"{}\" as N{}", rng.pick(LABELS), rng.below(3)),
        },
        12 => format!("{} : {}", ids[rng.below(ids.len())], rng.pick(LABELS)),
        13 => format!("state \"{}\" as {}", rng.pick(LABELS), ids[rng.below(ids.len())]),
        14 => format!("{}:::{} --> {}", ids[rng.below(ids.len())], rng.pick(&["hot", "cold"]), id(rng, ids)),
        15 => format!("class {} {}", ids[rng.below(ids.len())], rng.pick(&["hot", "cold"])),
        16 => format!("style {} {}", ids[rng.below(ids.len())], rng.pick(STYLES)),
        17 => match rng.below(2) {
            0 => format!("click {} \"https://example.com\" \"{}\"", ids[rng.below(ids.len())], rng.pick(LABELS)),
            _ => format!("click {} href \"{}\"", ids[rng.below(ids.len())], rng.pick(&["/a", "javascript:x"])),
        },
        _ => rng.pick(&["%% a comment", "", "accTitle: The title", "hide empty description"]).to_owned(),
    }
}

fn random_state(rng: &mut Rng) -> (String, Value) {
    let mut out = String::new();
    if rng.chance(8) {
        out += "---\ntitle: A title\n---\n";
    }
    out += rng.pick(&["stateDiagram-v2\n", "stateDiagram\n"]);
    if rng.chance(4) {
        out += &format!("direction {}\n", rng.pick(&["LR", "RL", "BT", "TB"]));
    }
    out += &format!("classDef hot {}\nclassDef cold {}\n", rng.pick(STYLES), rng.pick(STYLES));
    let ids: Vec<String> = ["A", "B", "C", "D", "Idle", "Moving"].iter().map(|&id| id.to_owned()).collect();
    let mut groups = 0;
    for _ in 0..2 + rng.below(9) {
        out += &statement(rng, 0, &ids, &mut groups);
        out += "\n";
    }
    let options = match rng.below(8) {
        0 => json!({ "maxWidth": WIDTHS[rng.below(WIDTHS.len())] }),
        1 => {
            json!({ "config": { "flowchart": { "curve": rng.pick(&["linear", "step", "stepBefore", "stepAfter", "basis"]) } } })
        }
        _ => json!({}),
    };
    (out, options)
}

#[test]
fn random_state_diagrams_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_state(&mut rng)).collect();
    let tally = run("random", &cases).finish("random state diagrams");
    assert!(tally.identical > 3000, "most random state diagrams render");
    tally.reaches_every_path("random state diagrams");
}

/// Pele's own worst cases for state diagrams (`tests/state/stress.test.ts`) that render, each
/// with the source limit lifted as that test lifts it.
fn stress() -> Vec<String> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let head = "stateDiagram-v2\n";
    let n = 50_000;
    vec![
        format!(
            "{head}classDef hot fill:#f00,color:white,font-weight:bold,stroke-width:2px,stroke:yellow\n{}",
            repeat(1500, &|_| "A --> B\nB --> A\nclass A hot\n".to_owned())
        ),
        format!("{head}{}a-->b\n{}", repeat(3000, &|i| format!("state s{i}{{\n")), "}\n".repeat(3000)),
        format!("{head}{}a\n{}", repeat(2500, &|i| format!("state s{i}{{\nx{i}\n--\n")), "}\n".repeat(2500)),
        format!("{head}{}", repeat(5000, &|i| format!("state s{i}{{\n}}\n"))),
        format!("{head}state s{{\n{}z\n}}\n", repeat(8000, &|i| format!("a{i}\n--\n"))),
        format!("{head}{}", repeat(5000, &|i| format!("n{i}-->n{}\n", i + 1))),
        format!("{head}{}", repeat(5000, &|i| format!("c-->n{i}\n"))),
        format!("{head}state f <<fork>>\n{}", repeat(5000, &|i| format!("f-->n{i}\n"))),
        format!(
            "{head}{}",
            (0..60).flat_map(|i| (i + 1..60).map(move |j| format!("n{i}-->n{j}\n"))).collect::<String>()
        ),
        format!("{head}{}{}", "a-->b\n".repeat(3000), "a-->a\n".repeat(3000)),
        format!("{head}{}", "[*]-->[*]\n".repeat(n / 12)),
        format!("{head}{}", repeat(2500, &|i| format!("note {} of a:n\n", if i % 2 == 1 { "left" } else { "right" }))),
        format!("{head}{}", repeat(2000, &|i| format!("note right of s{i}:n\n"))),
        format!("{head}{}", "a:x\n".repeat(n / 4)),
        format!("{head}a : {}", "word ".repeat(n / 5)),
        format!("{head}note right of a\n{}end note\n", "word *b _c\n".repeat(n / 11)),
        format!("{head}a --> b : {}", "#35;<br>x ".repeat(n / 10)),
        format!("{head}state \"{}\" as s {{\na\n}}\n", "w ".repeat(n / 2)),
        format!("{head}a\n{}", repeat(5000, &|i| format!("class a c{i}\n"))),
        format!("{head}class {}z c\n", repeat(8000, &|i| format!("s{i},"))),
        format!("{head}a\nclassDef c {}x\nclass a c\n", "fill:red,".repeat(n / 9)),
        format!("{head}a-->b\n{}", "style a,b fill:red\n".repeat(3000)),
        format!("{head}a-->b\n{}", "click a \"u\" \"t\"\n".repeat(3000)),
        format!("{head}a\nclick a \"&{}x\" \"t\"\n", "#38;".repeat(n / 4)),
        format!(
            "{head}{}z\n{}{}",
            repeat(1500, &|i| format!("state s{i}{{\n")),
            "}\n".repeat(1500),
            repeat(1500, &|i| format!("s{i}-->z\n"))
        ),
        format!(
            "{head}{}z\n{}{}",
            repeat(1500, &|i| format!("state s{i}{{\n")),
            "}\n".repeat(1500),
            repeat(3000, &|i| format!("o{i}-->z\n"))
        ),
        format!("{head}{}", repeat(2000, &|i| format!("state a{i}{{\nstate a{}{{\n}}\n}}\n", (i + 1) % 2000))),
        format!(
            "{head}{}{}",
            repeat(1200, &|i| format!("state a{i}{{\nstate a{}{{\nz{i}\n}}\n}}\n", (i + 1) % 1200)),
            repeat(3000, &|i| format!("o{i}-->z{}\n", i % 1200))
        ),
        format!("{head}state root {{\n a --> b\n}}\nroot --> root\n"),
    ]
}

#[test]
fn stress_sources_match_pele() {
    let options = json!({ "limit": 1_000_000_000u64, "outputLimit": 1_000_000_000u64 });
    let cases: Vec<(String, Value)> = stress().into_iter().map(|source| (source, options.clone())).collect();
    run("stress", &cases).finish("Pele's worst-case state diagrams");
}
