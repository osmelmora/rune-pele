//! Differential test for drawing use case diagrams: every source must come out of this port as
//! it comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source
//! Pele rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/usecase/harvest.json` holds the use case sources of Pele's own test suite and
//! the inputs of its seeded mutation runs, written by `scripts/harvest-usecase.mjs`. The widths
//! put every source through the attempts of `turn_to_fit`.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/usecase/harvest.json");
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
            "padding" => layout.padding = Some(number(value)),
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

/// Whether a line of the source relates an element to itself.
fn self_relationship(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    words.len() >= 3 && words[0] == words[words.len() - 1] && words[1].contains("--")
}

fn runs_down(source: &str) -> bool {
    ["direction TB", "direction TD", "direction BT"].iter().any(|direction| source.contains(direction))
}

/// Whether a marker is drawn with no paint of its own, as only the cross is.
fn bare_marker(svg: &str) -> bool {
    svg.match_indices("<path class=\"pele-marker\" d=\"").any(|(at, opening)| {
        let rest = &svg[at + opening.len()..];
        rest.find('"').is_some_and(|end| rest[end..].starts_with("\"/>"))
    })
}

/// Whether the title of a boundary sits off the middle of its top edge, where `title_x` put it
/// to keep clear of an edge that enters there.
fn moved_title(svg: &str) -> bool {
    let number = |text: &str, name: &str| -> Option<f64> {
        let from = text.find(&format!(" {name}=\""))? + name.len() + 3;
        text[from..from + text[from..].find('"')?].parse().ok()
    };
    svg.match_indices("<g class=\"pele-cluster pele-boundary").any(|(at, _)| {
        let group = &svg[at..at + svg[at..].find("</g>").unwrap_or(0)];
        if group.contains("pele-boundary-tab") {
            return false;
        }
        let (Some(rect), Some(label)) = (group.find("<rect"), group.find("<text class=\"pele-cluster-label\"")) else {
            return false;
        };
        match (number(&group[rect..label], "x"), number(&group[rect..label], "width"), number(&group[label..], "x")) {
            (Some(x), Some(width), Some(title)) => (x + width / 2.0 - title).abs() > 0.02,
            _ => false,
        }
    })
}

/// What shows that a source went through one path of the draw step: something in the source
/// that was drawn, or something in Pele's SVG.
type Reached = fn(&str, &str) -> bool;

const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("an accessible title", |_, svg| svg.contains("aria-labelledby=")),
    ("an actor", |_, svg| svg.contains("<path d=\"M0,-12V10M-14,-3H14")),
    ("a hollow actor", |_, svg| svg.contains("r=\"8\" fill=\"var(--_bg)\"")),
    ("an awesome actor", |_, svg| svg.contains("<path d=\"M0,-30A11,11 0 1 1 0,-8")),
    ("an actor shown as an icon", |_, svg| svg.contains("class=\"pele-icon\"")),
    ("a business actor", |_, svg| svg.contains("class=\"pele-business\" d=\"M1.08,-12.09")),
    ("a hollow business actor", |_, svg| svg.contains("class=\"pele-business\" d=\"M0.96,-13.08")),
    ("a stereotype", |_, svg| svg.contains("class=\"pele-stereotype\"")),
    ("an oval use case", |_, svg| svg.contains("<ellipse rx=")),
    ("a business use case", |_, svg| svg.contains("usecase-ellipse usecase-business")),
    ("a rectangular use case", |_, svg| svg.contains("usecase-element usecase-rect")),
    ("a note", |_, svg| svg.contains("class=\"pele-node pele-note")),
    ("a note's connector", |_, svg| svg.contains("stroke-dasharray=\"2 3\"")),
    ("a JSON table", |_, svg| svg.contains("class=\"pele-json-lines\"")),
    ("a JSON table with no key column", |source, svg| source.contains("@{}") && svg.contains("pele-json-lines")),
    ("a JSON value on several lines", |source, _| source.contains("wraps and wraps")),
    ("a table heading", |_, svg| svg.contains("font-weight=\"var(--_hw)\"")),
    ("a boundary", |_, svg| svg.contains("class=\"pele-clusters\"")),
    ("a package boundary", |_, svg| svg.contains("class=\"pele-boundary-tab\"")),
    ("a boundary that sets its fill", |_, svg| svg.contains("fill=\"var(--_a)\" stroke=")),
    ("a boundary title moved clear of an edge", |_, svg| moved_title(svg)),
    ("a casing", |_, svg| svg.contains("class=\"pele-casings\"")),
    ("a casing wider than the default", |_, svg| svg.contains("<g stroke-width=\"")),
    ("a casing with a computed width", |_, svg| svg.contains("stroke-width:calc(")),
    ("a casing as faint as its edge", |_, svg| svg.contains("<g style=\"opacity:")),
    ("a hollow marker in a casing", |_, svg| svg.contains("fill=\"var(--_bg)\" stroke=\"var(--_bg)\" stroke-linejoin")),
    ("an edge in the ink of the fill under it", |_, svg| svg.contains("\" stroke=\"var(--pele-fg-on-")),
    ("a label in the ink of the fill under it", |_, svg| svg.contains(" fill=\"var(--pele-fg-on-")),
    ("a hollow marker showing the fill under it", |_, svg| {
        svg.contains("Z\" fill=\"#234\" stroke=\"var(--pele-fg-on-")
            || svg.contains("Z\" fill=\"#ffe\" stroke=\"var(--pele-fg-on-")
    }),
    ("an edge in a colour of its own", |_, svg| svg.contains("Z\" fill=\"red\" stroke=\"none\"")),
    ("an edge with a style", |_, svg| svg.contains("\" data-id=\"e1\" style=")),
    ("an edge with a class", |_, svg| svg.contains("class=\"pele-edge pele-association thick")),
    ("an arrowhead", |_, svg| svg.contains("stroke=\"none\"/>")),
    ("a circle marker", |_, svg| svg.contains("<circle class=\"pele-marker\"")),
    ("a cross marker", |_, svg| bare_marker(svg)),
    ("a generalization", |_, svg| svg.contains("fill=\"var(--_bg)\" stroke=\"var(--_l)\" stroke-linejoin=\"round\"")),
    ("an include or extend", |_, svg| svg.contains("stroke-dasharray=\"5 4\"")),
    ("an animated edge", |_, svg| svg.contains("stroke-dasharray=\"9 5\"")),
    ("an edge label", |_, svg| svg.contains("class=\"pele-edge-label\"")),
    ("a muted edge label", |_, svg| svg.contains("fill=\"var(--_bg)\"/><text fill=\"var(--_m)\"")),
    ("a self-relationship across", |source, _| !runs_down(source) && source.lines().any(self_relationship)),
    ("a self-relationship down", |source, _| runs_down(source) && source.lines().any(self_relationship)),
    ("a labelled self-relationship", |source, _| {
        source.lines().any(|line| self_relationship(line) && line.split_whitespace().count() > 3)
    }),
    ("an author's class", |_, svg| svg.contains("usecase-ellipse fillDark\"")),
    ("a label of several runs", |_, svg| svg.contains("xml:space=\"preserve\"")),
    ("an italic run", |_, svg| svg.contains("font-style=\"italic\"")),
    ("a bold label", |_, svg| svg.contains("font-weight=\"var(--_w)\"")),
    ("a style on a label", |_, svg| svg.contains("class=\"pele-label\" style=")),
    ("a rounded shape with a style", |_, svg| svg.contains("style=\"rx:var(--_r);")),
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
                    *if r#type == "usecase" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "usecase" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "usecase" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { error, .. }, Err(RenderError::Diagnostic(ours))) => {
                // A message can hold half of a surrogate pair, which only JavaScript's string can.
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
                Some(format!("pele rejects a use case diagram, ours reports it as not ported: {error}"))
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested use case sources");
    assert!(tally.identical > 1500, "the harvest holds use case diagrams Pele renders");
    assert!(tally.rejected > 1000, "the harvest holds use case diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested use case sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
        // Only under the breakpoint is a diagram that runs across turned to run down.
        assert_eq!(redrawn > 0, width < 640.0, "maxWidth {width} changes a drawing only under the breakpoint");
    }
}

/// How many of the cases Pele draws another way for the width they give than with room to spare:
/// running down instead of across.
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
    let tally = run("mutated", &plain(harvest("mutated"))).finish("mutated use case sources");
    assert!(tally.identical > 500, "a fair share of the mutated inputs is drawn");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "usecase-beta\nactor Customer\nactor Clerk@{ type: hollow }\nsystemBoundary Shop[The shop]\n  Browse(Browse the catalogue)\n  Order(Place an order)\n  Pay[Pay for it]\nend\nCustomer --> Browse\nCustomer --> Order\nOrder ..> : include Pay\nClerk -- handles --> Order\nnote for Pay \"Cards only\"";
    let filled = "usecase-beta\ndirection TB\nclassDef dark fill:#123,stroke:#9cf\nclassDef light fill:#ffd\nclassDef faint fill:#369,opacity:0.3\nsystemBoundary S:::dark\n  A(Alpha)\n  B(Beta)\n  C(Gamma)\nend\nsystemBoundary T:::light\n  D(Delta)\n  E(Epsilon)\nend\nsystemBoundary U:::faint\n  F(Phi)\n  G(Chi)\nend\nA --|> B\nB e1@--> C\nD --|> E\nD --o E\nF --|> G\nA --> D\nE --x F\nstyle e1 stroke:red,stroke-width:3px\nA g@--|> C\nstyle g stroke-width:calc(1px + 2px),opacity:0.5";
    let table = "usecase-beta\njson Payload@{\"id\": 7, \"name\": \"A value that wraps and wraps and wraps until it has to break onto another line\", \"tags\": [\"a\", \"b\"], \"nested\": {\"deep\": [1, {\"x\": null}], \"empty\": {}, \"none\": []}, \"1\": true}\njson Empty@{}\nactor A\nA --> Payload\nPayload -- Empty\nstyle Payload fill:#024,stroke:#fff\nclassDef t color:#fc0,font-size:20px\nclass Empty t";
    let loops = "usecase-beta\nactor A\nA --> A\nA -- again --> A\nU(Use) --o U\nU -- \"`**bold** loop`\" --x U\nB[Box] <-- back -- B\nA --> U\nU --> B";
    let actors = "usecase-beta\nactor \"Plain actor with a long name that wraps\" <<role>>\nactor H@{ type: hollow, business: true } <<worker>>\nactor W@{ type: awesome }\nactor I@{ icon: \"fa:user\" }\nactor N@{ business: true }\nactor M(\"`*markdown* **name**`\")\nH --|> N\nW --> I\nclassDef c fill:#fee,stroke:#900,color:#900,font-weight:bold\nclass H,W,I,N c";
    let titled = "---\ntitle: A title wider than the diagram it sits on top of, by some way\nconfig:\n  usecase:\n    nodeSpacing: 12\n    rankSpacing: 30\n---\nusecase-beta\naccTitle: Accessible\naccDescr: Described\nactor A\nA --> B(Do)\n";
    let package = "usecase-beta\ndirection TB\nactor A\nactor B\nsystemBoundary P[\"A package with a long title\"]@{ type: package }\n  U(Use)\n  V[Box] <<svc>>\n  W(Biz)@{ business: true }\nend\nsystemBoundary Q[Entered from above]\n  X(One)\n  Y(Two)\n  Z(Three)\nend\nA --> U\nA --> X\nA --> Y\nB --> Y\nB --> Z\nA --> Z\nU --> X";
    let mut cases = vec![
        (across.to_owned(), json!({})),
        (across.to_owned(), json!({ "maxWidth": 300 })),
        (across.to_owned(), json!({ "maxWidth": 300, "autoDirection": false })),
        (across.to_owned(), json!({ "maxWidth": 600, "directionBreakpoint": 700 })),
        (across.to_owned(), json!({ "maxWidth": 600, "directionBreakpoint": 500 })),
        (across.to_owned(), json!({ "maxWidth": 2000 })),
        (across.replace("usecase-beta\n", "usecase-beta\ndirection RL\n"), json!({ "maxWidth": 200 })),
        (across.to_owned(), json!({ "padding": 0 })),
        (across.to_owned(), json!({ "padding": 24.5, "fontSize": 13 })),
        (across.to_owned(), json!({ "fontSize": 22, "idPrefix": "dia<g>\"1" })),
        (across.to_owned(), json!({ "responsive": false })),
        (
            across.to_owned(),
            json!({ "config": { "usecase": { "useMaxWidth": false, "nodeSpacing": 80, "rankSpacing": 5000 } } }),
        ),
        (across.to_owned(), json!({ "config": { "usecase": { "nodeSpacing": -1, "rankSpacing": "40" } } })),
        (across.to_owned(), json!({ "config": { "usecase": 5 } })),
        (across.to_owned(), json!({ "outputLimit": 500 })),
        (across.to_owned(), json!({ "limit": 40 })),
        (filled.to_owned(), json!({})),
        (filled.replace("direction TB", "direction LR"), json!({})),
        (filled.replace("direction TB", "direction BT"), json!({ "maxWidth": 320 })),
        (table.to_owned(), json!({})),
        (table.to_owned(), json!({ "maxWidth": 250, "fontSize": 12 })),
        (loops.to_owned(), json!({})),
        (loops.replace("usecase-beta\n", "usecase-beta\ndirection TB\n"), json!({})),
        (loops.replace("usecase-beta\n", "usecase-beta\ndirection RL\n"), json!({ "maxWidth": 320 })),
        (actors.to_owned(), json!({})),
        (actors.to_owned(), json!({ "maxWidth": 320 })),
        (titled.to_owned(), json!({})),
        (titled.to_owned(), json!({ "maxWidth": 320 })),
        (package.to_owned(), json!({})),
        (package.replace("direction TB", "direction BT"), json!({})),
        (package.replace("direction TB", "direction LR"), json!({ "maxWidth": 400 })),
    ];
    for source in [
        "usecase-beta\n",
        "usecase-beta\nA",
        "usecase-beta\nsystemBoundary S\nend",
        "usecase-beta\nsystemBoundary \"\"\n  A\nend\nsystemBoundary Empty[\"\"]\nend",
        "usecase-beta\nactor A(\"\")\nB(\"\")\nC[\"\"]\nnote for B \"\"\nA -- \"\" --> B",
        "usecase-beta\nA(a &amp; <b>b</b> \\n c #35; &#60;) <<s & t>>\nnote for A \"<i>plain</i> & *text*\"\nA -- x<y --> B(\"`a **b** <br> c`\")",
        "usecase-beta\nA --> B\nA --> B\nA <-- B\nA --- B\nA ----> B\nB e@--> A\ne@{ animation: slow }\nA f@--> B\nf@{ animate: true }\nclass f,e hot\nclassDef hot stroke:#f00",
        "usecase-beta\nactor A\nactor B\nA --|> B\nB --|> A\nC --|> D\nC ..> : extend D\nD ..> : include C",
        "usecase-beta\nclassDef default fill:#030,stroke:#0f0,color:#0f0\nactor A\nA --> B(Go)\nsystemBoundary S\n C\nend\njson P@{\"a\":1}\nnote for A \"n\"",
        "usecase-beta\nA\nstyle A fill:url(x),stroke:a;b,color:red,font-size:300px,rx:9,opacity:0.5\nB\nstyle B fill:rgb(1 2 3 / 0.4),stroke-dasharray:4\\,2\nC\nstyle C fill:transparent\nA --> B\nB --> C",
        "usecase-beta\nsystemBoundary S\n  A\n  B\nend\nstyle S fill:none\nA --> B\nsystemBoundary T\n  C\n  D\nend\nstyle T fill:#000,fill-opacity:0\nC --|> D",
        "usecase-beta\nsystemBoundary S\n  A\n  B\nend\nstyle S fill:#fff\nA e@--|> B\nstyle e stroke:none\nA f@--> B\nstyle f stroke:transparent,stroke-width:4\nB g@--> A\nstyle g stroke-width:1px,stroke-opacity:0.5\nA h@--x S2\nstyle h stroke-width:2em",
        "usecase-beta\nactor \"😀 née Ünïcode 日本語のラベルは長いので折り返します\"\n\"x\\ty\"\n",
        "usecase-beta\nactor A@{ icon: \"\" }\nactor B@{ icon: \"#35;x\" }\nA --> B",
    ] {
        cases.push((source.to_owned(), json!({})));
        cases.push((source.to_owned(), json!({ "maxWidth": 320 })));
    }
    cases
}

#[test]
fn cases_match_pele() {
    let cases = cases();
    let tally = run("case", &cases).finish("hand-written use case sources");
    assert_eq!(tally.rejected, 4, "the two limits reject two cases each");
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the hand-written cases another way");
    assert!(redrawn >= 8, "the hand-written cases reach the turn to fit");
}

/// The shapes of Pele's own worst cases (`tests/usecase/stress.test.ts`), at sizes the default
/// limits let through: nesting, repetition and fan-out that a port could answer with a panic or
/// a long wait where Pele answers with a drawing.
#[test]
fn worst_cases_match_pele() {
    let u = "usecase-beta\n";
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let mut complete = u.to_owned();
    for i in 0..14 {
        for j in i + 1..14 {
            complete += &format!("n{i} --o n{j}\n");
        }
    }
    let sources = vec![
        format!("{u}json P@{}1{}", "{\"a\":".repeat(7000), "}".repeat(7000)),
        format!("{u}json P@{{\"a\":{}{}}}", "[".repeat(24_000), "]".repeat(24_000)),
        format!("{u}json P@{{{}\"z\":0}}", repeat(3000, &|i| format!("\"k{i}\":{i},"))),
        format!("{u}json P@{{{}\"z\":0}}", "\"k\":{\"a\":{}},".repeat(3000)),
        format!("{u}json P@{{\"a\":[{}1]}}", "1,".repeat(20_000)),
        format!("{u}A:::{}z\n", repeat(3000, &|i| format!("c{i},"))),
        format!(
            "{u}A\nclassDef {}z {}fill:red\nclass A z\n",
            repeat(2000, &|i| format!("c{i},")),
            repeat(2000, &|i| format!("x{i}:1,"))
        ),
        format!("{u}{}", "A(A) <<S>>:::c\n".repeat(3000)),
        format!("{u}{}", repeat(400, &|i| format!("systemBoundary s{i}\n u{i}\nend\n"))),
        format!("{u}A\n{}", "note for A \"n\"\n".repeat(300)),
        format!("{u}A(\"{}\")", "word ".repeat(9000)),
        format!("{u}actor A(\"{}\")", "a".repeat(40_000)),
        format!("{u}A(\"{}\")", "#35;&amp;<br>".repeat(3000)),
        format!("{u}A(\"`{}`\")", "*a _b ".repeat(6000)),
        format!("{u}A -- \"{}\" --> B", "word ".repeat(9000)),
        format!("{u}{}", repeat(500, &|i| format!("actor a{i}\n"))),
        format!("{u}{}", repeat(300, &|i| format!("n{i} --> n{}\n", i + 1))),
        format!("{u}{}", repeat(250, &|i| format!("actor a{i}\na{i} --> hub\n"))),
        format!("{u}{}{}", "A --> B\n".repeat(250), "A -- l --> A\n".repeat(150)),
        complete,
        format!("{u}A {}> B", "-".repeat(10_000)),
        format!(
            "{u}systemBoundary S\n U\n V\nend\n{}",
            repeat(150, &|i| format!("actor a{i}\na{i} --> U\nV --> a{i}\n"))
        ),
    ];
    let started = std::time::Instant::now();
    for source in &sources {
        let rendered = pele_core::render(source);
        assert!(!matches!(rendered, Err(RenderError::NotPorted)), "a use case diagram is ported");
    }
    let ours = started.elapsed();
    println!("this port draws the {} worst cases in {ours:?}", sources.len());
    let tally = run("worst case", &plain(sources)).finish("worst use case sources");
    assert!(tally.identical >= 20, "nearly every worst case is drawn");
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
    "Log in",
    "Place an order",
    "A much longer label that has to wrap onto more than one line to fit",
    "\"`**Bold** and *italic*`\"",
    "\"`*all italic*`\"",
    "\"a <b>plain</b> & label\"",
    "\"Quoted, with: punctuation!\"",
    "Ünïcode 日本語",
    "x",
];

const CLASS_DEFS: &[&str] = &[
    "classDef fillDark fill:#234,stroke:#9cf",
    "classDef fillLight fill:#ffe,color:#603",
    "classDef faint fill:#369,opacity:0.4",
    "classDef clear fill:none,stroke-dasharray:4 2",
    "classDef thick stroke-width:3px,stroke:red",
    "classDef vague stroke-width:var(--w),opacity:0.5",
    "classDef text color:#a00,font-size:18px,font-weight:bold,font-style:italic",
];

const ARROWS: &[&str] = &[
    "-->",
    "-->",
    "-->",
    "<--",
    "--",
    "--o",
    "o--",
    "--x",
    "x--",
    "--->",
    "<----",
    "---",
    "-- uses -->",
    "-- \"a longer label\" --",
    "<-- back --",
    "o-- l --",
    "x-- l --",
    "-- l --o",
    "-- l --x",
    "-- \"`**md**`\" ---->",
];

const JSONS: &[&str] = &[
    "{}",
    "{\"a\":1}",
    "{\"b\":{\"c\":[1,2]},\"a\":null}",
    "{\"id\":7,\"note\":\"A value that wraps and wraps and wraps until it has to break onto another line\"}",
    "{\"2\":[true,[],{}],\"1\":-1.5e3,\"\":\"x\",\"list\":[{\"k\":\"v\"},[1]]}",
];

/// The elements of a random diagram, by what may relate to what.
#[derive(Default)]
struct Cast {
    actors: Vec<String>,
    use_cases: Vec<String>,
    /// The use cases of each boundary, which relate to each other inside its fill.
    boundaries: Vec<(String, Vec<String>)>,
    jsons: Vec<String>,
    edges: usize,
}

fn suffix(rng: &mut Rng, classes: bool) -> String {
    let mut out = String::new();
    if rng.chance(6) {
        out += rng.pick(&[" <<role>>", " <<a longer stereotype>>"]);
    }
    if classes && rng.chance(4) {
        out += ":::";
        out += rng.pick(&["fillDark", "fillLight", "faint", "clear", "thick", "text", "fillDark,text"]);
    }
    out
}

fn use_case(rng: &mut Rng, cast: &mut Cast, classes: bool) -> (String, String) {
    let id = format!("u{}", cast.use_cases.len());
    cast.use_cases.push(id.clone());
    let rect = rng.chance(4);
    let label = rng.pick(LABELS);
    let mut text = match rng.below(5) {
        0 => id.clone(),
        _ if rect => format!("{id}[{label}]"),
        _ => format!("{id}({label})"),
    };
    if !rect && rng.chance(6) {
        text += "@{ business: true }";
    }
    (id, text + &suffix(rng, classes))
}

fn relate(rng: &mut Rng, cast: &mut Cast, from: &str, arrow: &str, to: &str) -> String {
    if rng.chance(5) {
        cast.edges += 1;
        format!("{from} e{}@{arrow} {to}", cast.edges)
    } else {
        format!("{from} {arrow} {to}")
    }
}

/// A use case diagram that Pele nearly always renders, with host options, to put many graphs
/// through the shapes, the routes, the fills under edges and the turn to fit.
fn random_usecase(rng: &mut Rng) -> (String, Value) {
    let mut source = String::new();
    let title = rng.chance(5);
    let config = rng.chance(6);
    if title || config {
        source += "---\n";
        if title {
            source += &format!(
                "title: {}\n",
                rng.pick(&["A title", "A much longer title than the diagram is wide, most likely"])
            );
        }
        if config {
            let (node, rank) = (5 + rng.below(90), 5 + rng.below(120));
            source += &format!("config:\n  usecase:\n    nodeSpacing: {node}\n    rankSpacing: {rank}\n");
        }
        source += "---\n";
    }
    source += "usecase-beta\n";
    let mut lines: Vec<String> = Vec::new();
    if rng.chance(2) {
        lines.push(format!("direction {}", rng.pick(&["TB", "TD", "BT", "LR", "RL"])));
    }
    if rng.chance(8) {
        lines.push("accTitle: The title".to_owned());
    }
    let classes = rng.below(3) > 0;
    if classes {
        lines.extend(CLASS_DEFS.iter().map(|&line| line.to_owned()));
        if rng.chance(10) {
            lines.push("classDef default stroke:#555,fill:#eef".to_owned());
        }
    }

    let mut cast = Cast::default();
    for i in 0..1 + rng.below(4) {
        let id = format!("a{i}");
        let label = if rng.chance(3) { format!("({})", rng.pick(LABELS)) } else { String::new() };
        let metadata = match rng.below(12) {
            0 => "@{ type: hollow }",
            1 => "@{ type: awesome }",
            2 => "@{ icon: \"fa:user\" }",
            3 => "@{ business: true }",
            4 => "@{ type: hollow, business: true }",
            _ => "",
        };
        lines.push(format!("actor {id}{label}{metadata}{}", suffix(rng, classes)));
        cast.actors.push(id);
    }
    for i in 0..rng.below(3) {
        let id = format!("s{i}");
        let mut opening = match rng.below(3) {
            0 => format!("systemBoundary {id}"),
            1 => format!("systemBoundary {id}[{}]", rng.pick(&["Shop", "\"A boundary with a long title\"", "\"\""])),
            _ => format!("systemBoundary \"Boundary {i}\""),
        };
        let id = if opening.ends_with(&format!("\"Boundary {i}\"")) { format!("Boundary_{i}") } else { id };
        if rng.chance(4) {
            opening += "@{ type: package }";
        }
        if classes && rng.chance(2) {
            opening += ":::";
            opening += rng.pick(&["fillDark", "fillLight", "faint", "clear", "fillDark", "fillLight"]);
        }
        lines.push(opening);
        let mut members = Vec::new();
        for _ in 0..1 + rng.below(4) {
            let (member, text) = use_case(rng, &mut cast, classes);
            lines.push(format!("  {text}"));
            members.push(member);
        }
        lines.push("end".to_owned());
        cast.boundaries.push((id, members));
    }
    for _ in 0..1 + rng.below(4) {
        let (_, text) = use_case(rng, &mut cast, classes);
        lines.push(text);
    }
    for i in 0..rng.below(3) / 2 + usize::from(rng.chance(4)) {
        let id = format!("j{i}");
        lines.push(format!("json {id}@{}", rng.pick(JSONS)));
        cast.jsons.push(id);
    }

    let pick = |rng: &mut Rng, list: &[String]| list[rng.below(list.len())].clone();
    let most = if rng.chance(6) { 24 } else { 9 };
    for _ in 0..1 + rng.below(most) {
        let line = match rng.below(14) {
            0..=4 => {
                let (from, to) = (pick(rng, &cast.actors), pick(rng, &cast.use_cases));
                let arrow = rng.pick(ARROWS);
                relate(rng, &mut cast, &from, arrow, &to)
            }
            5 => {
                let (from, to) = (pick(rng, &cast.use_cases), pick(rng, &cast.use_cases));
                let arrow = rng.pick(ARROWS);
                relate(rng, &mut cast, &from, arrow, &to)
            }
            6 => {
                let (from, to) = (pick(rng, &cast.use_cases), pick(rng, &cast.use_cases));
                format!("{from} ..> : {} {to}", rng.pick(&["include", "extend"]))
            }
            7 => {
                let list = if rng.chance(2) { &cast.actors } else { &cast.use_cases };
                let (from, to) = (pick(rng, list), pick(rng, list));
                relate(rng, &mut cast, &from, "--|>", &to)
            }
            8 | 9 if !cast.boundaries.is_empty() => {
                // Two use cases of one boundary, so that the edge may lie wholly on its fill.
                let members = cast.boundaries[rng.below(cast.boundaries.len())].1.clone();
                let (from, to) = (pick(rng, &members), pick(rng, &members));
                let arrow = rng.pick(&["--|>", "--|>", "-->", "--o", "--x", "-- in --", "..> : include"]);
                relate(rng, &mut cast, &from, arrow, &to)
            }
            10 => {
                let list = if rng.chance(2) { &cast.actors } else { &cast.use_cases };
                let node = pick(rng, list);
                let arrow =
                    rng.pick(&["-->", "--", "-- loop -->", "--o", "<-- back --", "-- \"a longer loop label\" --x"]);
                format!("{node} {arrow} {node}")
            }
            11 if !cast.jsons.is_empty() => {
                let json = pick(rng, &cast.jsons);
                let other = if rng.chance(2) { pick(rng, &cast.actors) } else { pick(rng, &cast.use_cases) };
                let arrow = rng.pick(&["-->", "<--", "--", "-- reads -->"]);
                if rng.chance(2) { format!("{other} {arrow} {json}") } else { format!("{json} {arrow} {other}") }
            }
            12 => {
                let target = if rng.chance(2) { pick(rng, &cast.actors) } else { pick(rng, &cast.use_cases) };
                format!(
                    "note for {target} {}",
                    rng.pick(&[
                        "\"A note\"",
                        "a note that is long enough to wrap around once or twice, with luck",
                        "\"`**md**`\""
                    ])
                )
            }
            _ => {
                let (from, to) = (pick(rng, &cast.actors), pick(rng, &cast.actors));
                let arrow = rng.pick(ARROWS);
                relate(rng, &mut cast, &from, arrow, &to)
            }
        };
        lines.push(line);
    }
    for edge in 1..=cast.edges {
        match rng.below(8) {
            0 => lines.push(format!("style e{edge} stroke:red,stroke-width:3px")),
            1 if classes => lines.push(format!("class e{edge} {}", rng.pick(&["thick", "vague", "text"]))),
            2 => lines.push(format!("e{edge}@{{ animate: true }}")),
            3 => lines.push(format!("e{edge}@{{ animation: {} }}", rng.pick(&["fast", "slow"]))),
            4 => lines.push(format!(
                "style e{edge} {}",
                rng.pick(&["stroke:none", "opacity:0.3", "color:#06c,font-size:11px"])
            )),
            _ => {}
        }
    }
    if rng.chance(3) {
        let target = match rng.below(4) {
            0 if !cast.boundaries.is_empty() => cast.boundaries[rng.below(cast.boundaries.len())].0.clone(),
            1 if !cast.jsons.is_empty() => pick(rng, &cast.jsons),
            2 => pick(rng, &cast.actors),
            _ => pick(rng, &cast.use_cases),
        };
        lines.push(format!(
            "style {target} {}",
            rng.pick(&[
                "fill:#abc,stroke:#333",
                "fill:#102,color:#fd0",
                "stroke-width:4px,font-weight:bold",
                "fill:#eee,opacity:0.6"
            ])
        ));
    }
    source += &lines.join("\n");
    source.push('\n');

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
fn random_usecases_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_usecase(&mut rng)).collect();
    let tally = run("random use case diagram", &cases).finish("random use case diagrams");
    assert!(tally.identical > 3200, "nearly every random use case diagram renders");
    tally.reaches_every_path("random use case diagrams");
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random use case diagrams another way");
    assert!(redrawn > 300, "the widths put many random use case diagrams through the turn to fit");
}
