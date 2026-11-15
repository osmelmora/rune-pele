//! Differential test for drawing requirement diagrams: every source must come out of this port
//! as it comes out of Pele 1.13.0, byte for byte and with the same width and height, and a
//! source Pele rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/requirement/harvest.json` holds the requirement sources of Pele's own test
//! suite and the inputs of its seeded mutation runs, written by `scripts/harvest-requirement.mjs`.
//! The widths put every diagram that runs across through `turn_to_fit`.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/requirement/harvest.json");
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
    ("a requirement", "class=\"pele-node pele-requirement"),
    ("an element", "class=\"pele-node pele-element"),
    ("an author's class", "class=\"pele-node pele-requirement c1"),
    ("a table of fields", "class=\"pele-fields\""),
    ("a divider", "class=\"pele-divider\""),
    ("a divider with a line style", "stroke=\"var(--_b)\" style=\"stroke"),
    ("a contains relationship", "pele-relationship-contains"),
    ("an arrow relationship", "pele-relationship-arrow"),
    ("a self-loop", "data-id=\"loop-loop-"),
    ("a relationship label", "class=\"pele-edge-label\""),
    ("a rounded box with a style", "style=\"rx:var(--_r);"),
    ("a label in the ink of the fill", "fill=\"var(--pele-fg-on-"),
    ("a style on the text", "class=\"pele-value\" style="),
    (
        "a bold field from the style",
        "class=\"pele-field\" fill=\"var(--_m)\" style=\"font-weight:bold;\" font-weight=\"var(--_w)\"",
    ),
    ("an italic run", "font-style=\"italic\""),
    ("a font size from the style", "font-size=\"20\""),
    ("a value on several lines", "</tspan><tspan x="),
    ("a heading weight", "font-weight=\"var(--_hw)\""),
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
                    *if r#type == "requirement" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached.extend(PATHS.iter().filter(|(_, mark)| svg.contains(mark)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "requirement" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "requirement" => {
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
                Some(format!("pele rejects a requirement diagram, ours reports it as not ported: {error}"))
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

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

/// The widths a host may have. 320 and 480 are under the breakpoint at which a diagram that runs
/// across is turned to run down, 640 is on it and 900 over it.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested requirement sources");
    assert!(tally.identical > 900, "the harvest holds requirement diagrams Pele renders");
    assert!(tally.rejected > 1000, "the harvest holds requirement diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested requirement sources at maxWidth {width}"));
        let turned = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {turned} of the harvested sources another way");
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
    run("mutated", &plain(harvest("mutated"))).finish("mutated requirement sources");
}

const HEAD: &str = "requirementDiagram\n";

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "requirementDiagram\ndirection LR\nrequirement a {\nid: 1\ntext: the first requirement\nrisk: high\nverifymethod: test\n}\nelement b {\ntype: simulation\ndocref: reqs/test.md\n}\nelement c {\n}\nb - satisfies -> a\nc - verifies -> a\na - contains -> c\nb - copies -> c";
    let loops = "requirementDiagram\nrequirement loop {\nid: 1\n}\nloop - contains -> loop\nloop - traces -> loop\nloop - copies -> loop\nelement e {\n}\ne - derives -> loop";
    let star = format!(
        "{HEAD}requirement hub {{\ntext: the middle\n}}\n{}",
        (0..9)
            .map(|i| format!(
                "element e{i} {{\ntype: t{i}\n}}\ne{i} - {} -> hub\n",
                ["satisfies", "contains", "verifies"][i % 3]
            ))
            .collect::<String>()
    );
    let styled = "requirementDiagram\nrequirement a {\nid: 1\ntext: \"**bold** and _italic_ words\"\n}\nelement b {\ntype: \"a\nb\"\n}\nclassDef c1 fill:#036,color:#fff,font-weight:bold\nclassDef c2 stroke:#f00,stroke-width:3px,font-style:italic,font-size:20px\nclass a c1\nb:::c2\nstyle a stroke-dasharray:5 5\na - refines -> b";
    let mut out = vec![
        (across.to_owned(), json!({})),
        (across.to_owned(), json!({ "maxWidth": 200 })),
        (across.to_owned(), json!({ "maxWidth": 200, "autoDirection": false })),
        (across.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (across.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (across.replace("LR", "RL"), json!({ "maxWidth": 320 })),
        (across.replace("LR", "BT"), json!({ "maxWidth": 320 })),
        (across.to_owned(), json!({ "maxWidth": 320.5, "padding": 0, "fontSize": 13 })),
        (across.to_owned(), json!({ "responsive": false, "idPrefix": "a\"b", "padding": 20.5 })),
        (across.to_owned(), json!({ "config": { "requirement": { "useMaxWidth": false } } })),
        (loops.to_owned(), json!({})),
        (loops.replace("requirementDiagram\n", "requirementDiagram\ndirection LR\n"), json!({})),
        (star.clone(), json!({})),
        (star.replace(HEAD, "requirementDiagram\ndirection RL\n"), json!({})),
        (styled.to_owned(), json!({})),
        (styled.to_owned(), json!({ "fontSize": 20 })),
        (format!("{HEAD}{}", "element a {\n}\n".repeat(5000)), json!({})),
        (format!("{HEAD}requirement a {{\ntext: \"{}\"\n}}\n", "word ".repeat(2000)), json!({ "outputLimit": 10000 })),
    ];
    let more = [
        "---\ntitle: A title that is long enough to be wider than the diagram under it\n---\nrequirementDiagram\nrequirement a {\n}\n",
        "requirementDiagram\naccTitle: The <title> & more\naccDescr: A \"description\"\nrequirement a {\n}\n",
        "requirementDiagram\nrequirement x {\nid: 1\n}\nelement x {\ntype: t\n}\nx - contains -> x\ny - traces -> x\n",
        "requirementDiagram\nrequirement a {\ntext: \"#35; and &amp; and &lt;b&gt;\"\n}\nelement \"<script>\" {\ntype: \"<img src=x>\"\n}\n\"<script>\" - copies -> a",
        "requirementDiagram\nrequirement a {\ntext: \"\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{9577}\u{3044}\u{30c6}\u{30ad}\u{30b9}\u{30c8}\u{3067}\u{3059}\u{3002}\u{6298}\u{308a}\u{8fd4}\u{3057}\u{307e}\u{3059}\u{3002}\u{3082}\u{3063}\u{3068}\u{9577}\u{304f}\"\n}\n",
        "requirementDiagram\nrequirement \"A very long requirement name that has to wrap onto more than one line\" {\nid: REQ-0001\n}\n",
        "requirementDiagram\nrequirement a {\ntext: \"one<br>two<br/><br>four\"\n}\n",
        "requirementDiagram\nrequirement a {\ntext: \"<b>bold</b> <i>it</i> <code>mono</code> x<sub>2</sub> y<sup>3</sup> <u>u</u>\"\n}\n",
        "requirementDiagram\nelement a {\n}\nstyle a fill:none,stroke:none\nelement b {\ntype: x\n}\nstyle b fill:#0003,color:red\n",
        "requirementDiagram\nrequirement a {\n}\nrequirement b {\n}\na - contains -> b\na - copies -> b\nb - derives -> a\na - satisfies -> b\n",
    ];
    out.extend(more.iter().flat_map(|source| {
        [json!({}), json!({ "maxWidth": 320 }), json!({ "fontSize": 13.5 })]
            .map(|options| ((*source).to_owned(), options))
    }));
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("requirement cases");
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

const KINDS: &[&str] = &[
    "requirement",
    "functionalRequirement",
    "interfaceRequirement",
    "performanceRequirement",
    "physicalRequirement",
    "designConstraint",
];
const RELATIONS: &[&str] = &["contains", "copies", "derives", "satisfies", "verifies", "refines", "traces"];
const TEXTS: &[&str] = &[
    "x",
    "1",
    "\"REQ-0042\"",
    "the test text.",
    "\"quoted, with a comma\"",
    "\"**bold** and _italic_\"",
    "\"#35; &amp; &lt;x&gt;\"",
    "\"a long value that wraps across more than one line of the field table, as values do\"",
    "\"one<br>two\"",
    "Supercalifragilisticexpialidocious",
    "\"<b>all bold</b>\"",
    "\"\u{65e5}\u{672c}\u{8a9e}\"",
];
const DECLARATIONS: &[&str] = &[
    "fill:#f9f",
    "fill:#036",
    "fill:#036,color:#fff",
    "fill:#000,fill-opacity:0.3",
    "stroke:#333,stroke-width:4px",
    "stroke:red,stroke-dasharray:5 5",
    "color:red",
    "font-weight:bold",
    "font-style:italic",
    "font-size:20px",
    "fill:none",
    "rx:10",
];

fn name(rng: &mut Rng, most: usize) -> String {
    let index = rng.below(most);
    match index % 5 {
        0 => format!("r{index}"),
        1 => format!("\"Name {index}\""),
        2 => format!("e{index}"),
        3 => format!("\"A longer name for item {index}, which wraps\""),
        _ => format!("n{index}"),
    }
}

fn block(rng: &mut Rng, most: usize, names: &mut Vec<String>) -> String {
    let requirement = rng.below(3) > 0;
    let named = if rng.chance(12) { "loop".to_owned() } else { name(rng, most) };
    names.push(named.clone());
    let head = if requirement { rng.pick(KINDS) } else { "element" };
    let mut out = format!("{head} {named}");
    if rng.chance(4) {
        out += &format!(":::{}", rng.pick(&["c1", "c2", "c1,c2"]));
    }
    out += " {\n";
    for _ in 0..rng.below(5) {
        out += &if requirement {
            match rng.below(4) {
                0 => format!("id: {}", rng.pick(TEXTS)),
                1 => format!("text: {}", rng.pick(TEXTS)),
                2 => format!("risk: {}", rng.pick(&["low", "medium", "high"])),
                _ => format!("verifymethod: {}", rng.pick(&["analysis", "demonstration", "inspection", "test"])),
            }
        } else if rng.chance(2) {
            format!("type: {}", rng.pick(TEXTS))
        } else {
            format!("docref: {}", rng.pick(TEXTS))
        };
        out += "\n";
    }
    out + "}"
}

fn random_requirement(rng: &mut Rng) -> (String, Value) {
    let mut source = String::new();
    if rng.chance(5) {
        let title = rng.pick(&["A title", "A much longer title than the diagram is wide, most likely"]);
        source += &format!("---\ntitle: {title}\n---\n");
    }
    source += HEAD;
    if rng.below(3) > 0 {
        source += &format!("direction {}\n", rng.pick(&["TB", "BT", "LR", "RL", "LR", "RL"]));
    }
    if rng.chance(6) {
        source += "accTitle: The title\naccDescr: A description\n";
    }
    let many = rng.chance(5);
    let most = 2 + rng.below(if many { 20 } else { 7 });
    let mut names = Vec::new();
    for _ in 0..1 + rng.below(most) {
        source += &block(rng, most, &mut names);
        source += "\n";
    }
    for _ in 0..rng.below(2 * most) {
        let a = if rng.chance(8) { name(rng, most) } else { names[rng.below(names.len())].clone() };
        let b = if rng.chance(10) { a.clone() } else { names[rng.below(names.len())].clone() };
        let kind = rng.pick(RELATIONS);
        source += &if rng.chance(3) { format!("{b} <- {kind} - {a}\n") } else { format!("{a} - {kind} -> {b}\n") };
    }
    for _ in 0..rng.below(4) {
        source += &match rng.below(4) {
            0 => format!("classDef {} {}\n", rng.pick(&["c1", "c2"]), rng.pick(DECLARATIONS)),
            1 => format!("class {} {}\n", names[rng.below(names.len())], rng.pick(&["c1", "c2"])),
            2 => format!("style {} {}\n", names[rng.below(names.len())], rng.pick(DECLARATIONS)),
            _ => format!("{}:::{}\n", names[rng.below(names.len())], rng.pick(&["c1", "c2"])),
        };
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
fn random_requirements_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..3000).map(|_| random_requirement(&mut rng)).collect();
    let tally = run("random requirement", &cases).finish("random requirement diagrams");
    assert!(tally.identical > 2700, "nearly every random requirement diagram renders");
    tally.reaches_every_path("random requirement diagrams");
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random requirement diagrams another way");
    assert!(redrawn > 50, "the widths turn many random requirement diagrams");
}
