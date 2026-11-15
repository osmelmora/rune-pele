//! Differential test for drawing sequence diagrams: every source must come out of this port as
//! it comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source
//! Pele rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/sequence/harvest.json` holds the sequence sources of Pele's own test suite,
//! the inputs of its seeded mutation and generation runs and the shapes of its worst cases,
//! written by `scripts/harvest-sequence.mjs`. The widths put every source that is wider than
//! the host through the tight draw, and through the widening of gaps that follows it.

use pele_core::{HostConfig, LayoutOptions, MonospacePart, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/sequence/harvest.json");
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
            "monospace" => {
                layout.monospace = list(value)
                    .iter()
                    .map(|part| match part.as_str() {
                        "members" => MonospacePart::Members,
                        "attributes" => MonospacePart::Attributes,
                        "files" => MonospacePart::Files,
                        "messages" => MonospacePart::Messages,
                        other => panic!("{other} is not a monospace part"),
                    })
                    .collect();
            }
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

type Reached = fn(&str) -> bool;

/// The drawing paths a source can go through, each with what in Pele's SVG shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a title", |svg| svg.contains("class=\"pele-title\"")),
    ("an accessible title", |svg| svg.contains("aria-labelledby=")),
    ("a participant", |svg| svg.contains("pele-actor-participant")),
    ("an actor", |svg| svg.contains("pele-actor-actor")),
    ("a boundary", |svg| svg.contains("pele-actor-boundary")),
    ("a control", |svg| svg.contains("pele-actor-control")),
    ("an entity", |svg| svg.contains("pele-actor-entity")),
    ("a database", |svg| svg.contains("pele-actor-database")),
    ("collections", |svg| svg.contains("pele-actor-collections")),
    ("a queue", |svg| svg.contains("pele-actor-queue")),
    ("participants mirrored under the lifelines", |svg| svg.contains("pele-actor-footer")),
    ("participants not mirrored", |svg| svg.contains("pele-actor-") && !svg.contains("pele-actor-footer")),
    ("an author's class on a participant", |svg| tag_has(svg, "<g class=\"pele-node pele-actor ", " k m\"")),
    ("a figure's label in a box's ink", |svg| svg.contains("class=\"pele-label\" fill=\"var(--pele-fg-on-")),
    ("a participant's label on several lines", |svg| {
        svg.split("<text class=\"pele-label\" text-anchor")
            .skip(1)
            .any(|rest| text_of(rest).matches("<tspan x=").count() > 1)
    }),
    ("a lifeline", |svg| svg.contains("class=\"pele-lifeline\"")),
    ("a participant created by a message", created_later),
    ("a destroyed participant", |svg| svg.contains("class=\"pele-destroy\"")),
    ("an activation", |svg| svg.contains("class=\"pele-activation\"")),
    ("an activation inside another", nested_activation),
    ("an arrow head", |svg| has_marker(svg, "MLLZ")),
    ("a cross head", |svg| has_marker(svg, "MLML")),
    ("an open head", |svg| svg.contains("\" stroke-linejoin=\"round\"/>")),
    ("a filled half head", |svg| has_marker(svg, "MLVZ")),
    ("a half head of one stroke", |svg| has_marker(svg, "ML")),
    ("a message with a head at each end", |svg| {
        messages(svg).iter().any(|m| m.matches("class=\"pele-marker\"").count() == 2)
    }),
    ("a message with no head", |svg| messages(svg).iter().any(|m| !m.contains("class=\"pele-marker\""))),
    ("a dotted message", |svg| messages(svg).iter().any(|m| line_of(m).contains("stroke-dasharray=\"4 4\""))),
    ("a message from right to left", |svg| {
        messages(svg).iter().any(|m| {
            let d = path_of(m);
            let ends = |at: &str| d.split_once(at).and_then(|(_, rest)| number_at(rest));
            matches!((ends("M"), ends("H")), (Some(from), Some(to)) if to < from && !d.contains('V'))
        })
    }),
    ("a message to its own sender, rounded", |svg| messages(svg).iter().any(|m| path_of(m).contains("A6,6 0 0 1"))),
    ("a message to its own sender, square", |svg| {
        messages(svg).iter().any(|m| path_of(m).contains('V') && !path_of(m).contains('A'))
    }),
    ("a central connection", |svg| svg.contains("class=\"pele-central\"")),
    ("a message number", |svg| svg.contains("class=\"pele-sequence-number\"")),
    ("a message number that is no whole number", |svg| {
        svg.split("<g class=\"pele-sequence-number\">").skip(1).any(|rest| text_of(rest).contains('.'))
    }),
    ("a message in a fill's ink", |svg| {
        tag_has(svg, "<g class=\"pele-edge pele-message\"", "stroke=\"var(--pele-fg-on-")
    }),
    ("a message label", |svg| svg.contains("class=\"pele-edge-label\"")),
    ("a message label on a plate", |svg| {
        svg.split("<g class=\"pele-edge-label\"")
            .skip(1)
            .any(|rest| rest[rest.find('>').unwrap_or(0)..].starts_with("><rect"))
    }),
    ("a message label in a fill's ink", |svg| {
        svg.split("<g class=\"pele-edge-label\"")
            .skip(1)
            .any(|rest| rest[rest.find('>').unwrap_or(0)..].starts_with("><text fill=\"var(--pele-fg-on-"))
    }),
    ("a message label on several lines", |svg| {
        svg.split("<g class=\"pele-edge-label\"").skip(1).any(|rest| text_of(rest).matches("<tspan x=").count() > 1)
    }),
    ("a label with emphasis", |svg| {
        svg.contains("<tspan font-weight=\"var(--_w)\"") || svg.contains(" font-style=\"italic\"")
    }),
    ("messages in the monospace font", |svg| {
        tag_has(svg, "<g class=\"pele-edge-labels\"", "font-family=\"var(--_fm)\"")
    }),
    ("a note", |svg| svg.contains("class=\"pele-note\"")),
    ("a loop", |svg| svg.contains("pele-frame-loop")),
    ("an alt", |svg| svg.contains("pele-frame-alt")),
    ("an opt", |svg| svg.contains("pele-frame-opt")),
    ("a par", |svg| svg.contains("pele-frame-par")),
    ("a critical", |svg| svg.contains("pele-frame-critical")),
    ("a break", |svg| svg.contains("pele-frame-break")),
    ("a block with a title", |svg| svg.contains("class=\"pele-frame-label\"")),
    ("a block divided into sections", |svg| {
        svg.split("<g class=\"pele-frame pele-frame-")
            .skip(1)
            .any(|rest| rest[..rest.find("</g>").unwrap_or(rest.len())].contains("stroke-dasharray"))
    }),
    ("a block's title on a plate", |svg| svg.contains("stroke=\"none\"/><text class=\"pele-frame-label\"")),
    ("a block's title in a fill's ink", |svg| svg.contains("class=\"pele-frame-label\" fill=\"var(--pele-fg-on-")),
    ("a block inside a block", |svg| {
        let xs: BTreeSet<&str> = svg
            .split("<g class=\"pele-frame pele-frame-")
            .skip(1)
            .filter_map(|rest| rest.split_once("<rect x=\"").map(|(_, x)| &x[..x.find('"').unwrap_or(0)]))
            .collect();
        xs.len() > 1
    }),
    ("a shaded block in a colour", |svg| tag_has(svg, "<rect class=\"pele-rect\"", "style=\"rx:var(--_r);fill:")),
    ("a shaded block in the theme's colour", |svg| tag_has(svg, "<rect class=\"pele-rect\"", "fill-opacity=\"0.5\"")),
    ("a box", |svg| svg.contains("class=\"pele-cluster pele-box\"")),
    ("a box with a name", |svg| svg.contains("class=\"pele-cluster pele-box\" data-id=")),
    ("a box in a colour", |svg| svg.contains("fill=\"var(--_a)\" stroke=\"var(--_b)\" style=\"rx:var(--_r);fill:")),
    ("a box in the theme's colour", |svg| svg.contains("stroke=\"var(--_b)\" fill-opacity=\"0.5\"")),
    ("a box title in a fill's ink", |svg| svg.contains("class=\"pele-cluster-label\" fill=\"var(--pele-fg-on-")),
];

/// Whether a tag that starts with `start` has `needle` among its attributes.
fn tag_has(svg: &str, start: &str, needle: &str) -> bool {
    svg.split(start).skip(1).any(|rest| rest[..rest.find('>').unwrap_or(rest.len())].contains(needle))
}

/// The text element a piece of markup starts with or holds first, up to its end.
fn text_of(rest: &str) -> &str {
    &rest[..rest.find("</text>").unwrap_or(rest.len())]
}

/// Whether a marker is drawn as a path whose commands are `shape`, such as `MLLZ` for an arrow head.
fn has_marker(svg: &str, shape: &str) -> bool {
    svg.split("<path class=\"pele-marker\" d=\"").skip(1).any(|piece| {
        let d = &piece[..piece.find('"').unwrap_or(0)];
        d.chars().filter(char::is_ascii_uppercase).eq(shape.chars())
    })
}

/// The markup of each message, from its group's attributes to the next message.
fn messages(svg: &str) -> Vec<&str> {
    let edges = svg.split_once("<g class=\"pele-edges\"").map_or("", |(_, rest)| rest);
    let edges = &edges[..edges
        .find("<g class=\"pele-destroys\"")
        .or(edges.find("<g class=\"pele-edge-labels\""))
        .unwrap_or(edges.len())];
    edges.split("<g class=\"pele-edge pele-message\"").skip(1).collect()
}

/// A message's line: the first path of its markup.
fn line_of(message: &str) -> &str {
    let line = message.split_once("<path d=\"").map_or("", |(_, rest)| rest);
    &line[..line.find("/>").unwrap_or(line.len())]
}

fn path_of(message: &str) -> &str {
    let line = line_of(message);
    &line[..line.find('"').unwrap_or(line.len())]
}

fn number_at(text: &str) -> Option<f64> {
    let end = text.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(text.len());
    text[..end].parse().ok()
}

/// Whether a lifeline starts lower than another, as that of a participant a message creates does.
fn created_later(svg: &str) -> bool {
    let tops: BTreeSet<&str> = svg
        .split("<path class=\"pele-lifeline\"")
        .skip(1)
        .filter_map(|rest| {
            let d = rest.split_once(" d=\"M")?.1;
            let top = d.split_once(',')?.1;
            Some(&top[..top.find('V')?])
        })
        .collect();
    tops.len() > 1
}

/// Whether two activations of one participant stand at different places across.
fn nested_activation(svg: &str) -> bool {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for rest in svg.split("<rect class=\"pele-activation\" data-id=\"").skip(1) {
        let Some((id, rest)) = rest.split_once("\" x=\"") else {
            continue;
        };
        let x = &rest[..rest.find('"').unwrap_or(0)];
        if seen.iter().any(|&(other, at)| other == id && at != x) {
            return true;
        }
        seen.push((id, x));
    }
    false
}

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
                    *if r#type == "sequence" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached.extend(PATHS.iter().filter(|(_, reached)| reached(svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "sequence" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "sequence" => {
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
                Some(format!("pele rejects a sequence diagram, ours reports it as not ported: {error}"))
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

/// The widths a host may have. A sequence diagram wider than the host is drawn tightly.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

/// How many of the cases Pele draws another way for the width they give than with room to
/// spare, which for a sequence diagram is the tight draw.
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
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested sequence sources");
    assert!(tally.identical > 2000, "the harvest holds sequence diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds sequence diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested sequence sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated sequence sources");
}

/// The shapes of Pele's own worst cases (`tests/sequence/stress.test.ts`), at sizes the default
/// limits allow, as they are and at a width that draws them tightly.
#[test]
fn worst_cases_match_pele() {
    let sources = harvest("worst");
    let started = std::time::Instant::now();
    for source in &sources {
        let _ = pele_core::render(source);
    }
    println!("this port draws the {} worst cases in {:?}", sources.len(), started.elapsed());
    let mut cases = plain(sources.clone());
    cases.extend(sources.into_iter().map(|source| (source, json!({ "maxWidth": 480 }))));
    let tally = run("worst case", &cases).finish("worst sequence sources");
    assert!(tally.identical >= 40, "nearly every worst case is drawn");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let chat = "sequenceDiagram\nautonumber\nparticipant A as Alice in a long name\nactor B\nA->>+B: a message with a text long enough to wrap when the diagram is tight\nB-->>-A: and a reply that is also rather long for its gap\nNote over A,B: a note over both that says quite a lot about them\nloop every now and then, for as long as it takes\nA->>A: think it over\nend";
    let wide = "sequenceDiagram\nparticipant A\nparticipant B\nparticipant C\nparticipant D\nA->>D: a message across every lifeline that needs more room than they give\nB->>C: short\nA->>B: one more message with a long text between two neighbours\nC-->>D: and another long message text between the last two";
    let kinds = "sequenceDiagram\nparticipant P\nactor A\nparticipant B@{ \"type\": \"boundary\" }\nparticipant C@{ \"type\": \"control\" }\nparticipant E@{ \"type\": \"entity\" }\nparticipant D@{ \"type\": \"database\" }\nparticipant S@{ \"type\": \"collections\" }\nparticipant Q@{ \"type\": \"queue\" } as A queue<br/>on two lines\nparticipant X@{ \"type\": \"nothing\" }\nP->>Q: x";
    let boxed = "sequenceDiagram\nbox navy Dark\nactor A\nparticipant B\nend\nbox rgb(250, 250, 200) A light box with a title wider than what it holds\nparticipant C@{ \"type\": \"database\" }\nend\nbox\nactor D\nend\nA->>B: inside\nB->>C: across\nC->>C: own\nrect rgb(20, 20, 60)\nA->>B: shaded\nNote over A: n\nalt first\nB->>A: back\nelse second\nA->>A: again\nend\nend\nrect\nC->>D: plain\nend";
    let life = "sequenceDiagram\nA->>B: start\ncreate participant C\nB->>C: make\ncreate actor D as Dee\nC->>D: make\nD-->>C: back\ndestroy C\nB-xC: stop\ndestroy D\nD->>A: leave\nA->>B: end";
    let arrows = "sequenceDiagram\nA->>B: 1\nA-->>B: 2\nB->A: 3\nB-->A: 4\nA-xB: 5\nA--xB: 6\nA-)B: 7\nA--)B: 8\nA<<->>B: 9\nA<<-->>B: 10\nA-|\\B: 11\nA-|/B: 12\nA-\\\\B: 13\nA-//B: 14\nA--|\\B: 15\nA--|/B: 16\nA--\\\\B: 17\nA--//B: 18\nA/|-B: 19\nA\\|-B: 20\nA//-B: 21\nA\\\\-B: 22\nB/|--A: 23\nB\\|--A: 24\nB//--A: 25\nB\\\\--A: 26\nA-|\\A: 27\nA/|-A: 28\nA\\\\--A: 29\nA<<->>A: 30";
    let central = "sequenceDiagram\nautonumber\nA()->>()B: both\nA()->>B: from\nA->>()B: to\nB()-->>()A: back\nA()->>()A: own\nA/|-()B: tail\nA()//--B: tail";
    let nested = "sequenceDiagram\nA->>+B: 1\nA->>+B: 2\nA->>+B: 3\nB-->>-A: a\nB-->>-A: b\nactivate A\nactivate A\nNote right of A: beside the activations\nNote left of B: left of B\ndeactivate A\nA->>A: with one open\nB->>+A: onto it\nNote over A: open at the end";
    let mut out = vec![
        (chat.to_owned(), json!({})),
        (chat.to_owned(), json!({ "maxWidth": 200 })),
        (chat.to_owned(), json!({ "maxWidth": 260.5 })),
        (chat.to_owned(), json!({ "maxWidth": 0 })),
        (chat.to_owned(), json!({ "maxWidth": 300, "monospace": ["messages"] })),
        (chat.to_owned(), json!({ "monospace": ["messages", "members"] })),
        (chat.to_owned(), json!({ "fontSize": 12, "padding": 0 })),
        (chat.to_owned(), json!({ "fontSize": 20.5, "padding": 24.5, "maxWidth": 400 })),
        (chat.to_owned(), json!({ "idPrefix": "my \"id\"", "responsive": false })),
        (format!("{chat}\naccTitle: A title\naccDescr: A description"), json!({ "idPrefix": "x" })),
        (chat.to_owned(), json!({ "config": { "sequence": { "mirrorActors": false, "showSequenceNumbers": true } } })),
        (chat.to_owned(), json!({ "config": { "sequence": { "useMaxWidth": false, "rightAngles": true } } })),
        (chat.to_owned(), json!({ "config": { "sequence": { "wrap": true, "width": 90, "height": 80 } } })),
        (chat.to_owned(), json!({ "config": { "wrap": true }, "maxWidth": 280 })),
        (chat.to_owned(), json!({ "config": { "sequence": "not a section" } })),
        (chat.to_owned(), json!({ "config": { "sequence": { "width": 2001, "height": -1, "actorMargin": "40" } } })),
        (
            chat.to_owned(),
            json!({ "config": { "sequence": { "actorMargin": 0, "messageMargin": 0, "noteMargin": 0 } } }),
        ),
        (chat.to_owned(), json!({ "config": { "sequence": { "actorMargin": 2000, "messageMargin": 2000 } } })),
    ];
    for align in ["left", "right", "center", "LEFT"] {
        let config = json!({ "sequence": { "messageAlign": align } });
        out.push((wide.to_owned(), json!({ "config": config })));
        out.push((wide.to_owned(), json!({ "config": config, "maxWidth": 320 })));
    }
    for width in [150, 240, 320, 400, 480, 560] {
        for source in [wide, kinds, boxed, life, nested] {
            out.push((source.to_owned(), json!({ "maxWidth": width })));
        }
    }
    for source in [kinds, boxed, life, arrows, central, nested] {
        out.push((source.to_owned(), json!({})));
        out.push((
            source.to_owned(),
            json!({ "config": { "sequence": { "rightAngles": true, "mirrorActors": false } } }),
        ));
        out.push((
            source.to_owned(),
            json!({ "config": { "sequence": { "showSequenceNumbers": true, "boxMargin": 30 } } }),
        ));
    }
    out.extend(plain(
        [
            "sequenceDiagram",
            "sequenceDiagram\ntitle Only a title",
            "sequenceDiagram\ntitle A title much wider than the one participant under it\nparticipant A",
            "sequenceDiagram\nloop nothing inside\nend\nrect rgb(1,2,3)\nend",
            "sequenceDiagram\nparticipant A\nparticipant B\nA->>C: only C and A are used\nNote over B: a note counts",
            "sequenceDiagram\nparticipant A\nproperties A: {\"class\": \"k m\"}\nparticipant B\nproperties B: {\"class\": \"<script>\"}\nparticipant C\nproperties C: {\"class\": 5}\nA->>B: x",
            "sequenceDiagram\nautonumber 9.99 0.01\nA->>B: x\nB->>A: y\nautonumber 1000000 1000000\nA->>B: z\nB->>A: w",
            "sequenceDiagram\nbox Aqua one\nparticipant A\nend\nparticipant B\nbox Aqua one\nparticipant C\nend\nA->>C: x",
            "sequenceDiagram\nA->>B: x\nbox Transparent t\nparticipant A\nparticipant C\nend\nbox transparent\nparticipant B\nend",
            "sequenceDiagram\nbox calc(1px) t\nparticipant A\nend\nrect var(--x)\nA->>A: x\nend\nrect url(x)\nA->>A: y\nend\nrect rgba(0,0,0,0.2)\nA->>A: z\nend",
            "sequenceDiagram\ncreate participant A\nB->>A: make\ncreate participant C\nA->>C: make\nNote over A,C: n",
            "sequenceDiagram\ncreate actor A\nB->>A: a text that is tall<br/>because it<br/>has four<br/>lines\ndestroy A\nA-xB: x\ndestroy B\nA->>B: both ends",
            "sequenceDiagram\nNote left of A: at the left edge\nNote right of A: wrap: a wrapped note at the right edge that goes on\nNote over A: <b>bold</b> and <i>italic</i><br/>and plain",
            "sequenceDiagram\nA->>B: <b>all bold</b>\nB->>A: <i>all italic</i>\nA->>B: a<sub>1</sub> and x<sup>2</sup>\nB->>A: #9829; &lt;tag&gt; &amp;",
            "sequenceDiagram\nalt\nA->>B: x\nelse\nB->>A: y\nelse a third section with a long title that widens the block\nA->>A: z\nend",
            "sequenceDiagram\npar one\nA->>B: x\nand two\npar_over inner\nB->>A: y\nend\nend\ncritical c\nA->>B: z\noption o\nbreak b\nB->>A: w\nend\nend",
            "sequenceDiagram\nparticipant \u{65e5}\u{672c}\u{8a9e} as \u{6771}\u{4eac}\n\u{65e5}\u{672c}\u{8a9e}->>\u{e9}: \u{4f60}\u{597d} \u{1f600}",
            "---\ntitle: A front matter title\nconfig:\n  sequence:\n    mirrorActors: false\n---\nsequenceDiagram\nA->>B: x",
            "%%{init: {\"sequence\": {\"hideUnusedParticipants\": true}}}%%\nsequenceDiagram\nparticipant A\nparticipant B\nparticipant C\nA->>C: x",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    out
}

#[test]
fn cases_match_pele() {
    let cases = cases();
    let tally = run("case", &cases).finish("sequence cases");
    assert!(tally.identical > 85, "nearly every case renders");
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the cases another way");
    assert!(redrawn > 20, "the widths put many cases through the tight draw");
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

const NAMES: &[&str] = &["A", "B", "C", "D", "E", "Alice", "Bob", "a b", "\u{e9}", "Svc-1"];
const KINDS: &[&str] = &["boundary", "control", "entity", "database", "collections", "queue", "actor", "participant"];
const ARROWS: &[&str] = &[
    "->>", "->>", "->>", "-->>", "-->>", "->", "-->", "<<->>", "<<-->>", "-x", "--x", "-)", "--)", "-|\\", "-|/",
    "-\\\\", "-//", "--|\\", "--|/", "--\\\\", "--//", "/|-", "\\|-", "//-", "\\\\-", "/|--", "\\|--", "//--",
    "\\\\--",
];
const TEXTS: &[&str] = &[
    "ok",
    "",
    "x",
    "a reply",
    "a message with a text long enough to wrap when the diagram is tight",
    "wrap: wrapped text that goes on and on for a while, well past its room",
    "two<br/>lines",
    "<b>bold</b> and <i>italic</i>",
    "<b>all bold</b>",
    "#35; and #9829; #lt;tag#gt;",
    "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}",
    "nowrap: kept on one line however long it gets to be",
];
const COLOURS: &[&str] =
    &["Aqua", "navy", "rgb(250, 250, 200)", "rgba(0, 0, 0, 0.8)", "hsl(200, 60%, 30%)", "transparent", "", "#333"];

/// A sequence diagram being written, with what its statements have left open.
struct Writer<'r> {
    rng: &'r mut Rng,
    out: String,
    actors: Vec<String>,
    /// How many activations each participant has open.
    active: Vec<usize>,
    made: usize,
}

impl Writer<'_> {
    fn line(&mut self, text: &str) {
        self.out += self.rng.pick(&["", "  ", "    "]);
        self.out += text;
        self.out += "\n";
    }

    fn actor(&mut self) -> usize {
        self.rng.below(self.actors.len())
    }

    fn declare(&mut self, name: &str) {
        let label = match self.rng.below(6) {
            0 => " as A long name for it",
            1 => " as Two<br/>lines",
            2 => " as wrap: A name that wraps because it is asked to and is long",
            _ => "",
        };
        let line = match self.rng.below(4) {
            0 => format!("actor {name}{label}"),
            1 => format!("participant {name}@{{ \"type\": \"{}\" }}{label}", self.rng.pick(KINDS)),
            _ => format!("participant {name}{label}"),
        };
        self.line(&line);
        self.actors.push(name.to_owned());
        self.active.push(0);
    }

    fn message(&mut self) {
        let from = self.actor();
        let to = if self.rng.chance(6) { from } else { self.actor() };
        let mut arrow = self.rng.pick(ARROWS).to_owned();
        match self.rng.below(10) {
            0 | 1 => {
                arrow += "+";
                self.active[to] += 1;
            }
            2 | 3 if self.active[from] > 0 => {
                arrow += "-";
                self.active[from] -= 1;
            }
            4 => arrow = format!("(){arrow}()"),
            5 => arrow = format!("(){arrow}"),
            6 => arrow += "()",
            _ => {}
        }
        let line = format!("{}{arrow}{}: {}", self.actors[from], self.actors[to], self.rng.pick(TEXTS));
        self.line(&line);
    }

    fn statement(&mut self, depth: usize) {
        match self.rng.below(30) {
            0..=2 => {
                let (a, b) = (self.actor(), self.actor());
                let text = self.rng.pick(TEXTS);
                let line = match self.rng.below(4) {
                    0 => format!("Note over {},{}: {text}", self.actors[a], self.actors[b]),
                    1 => format!("Note over {}: {text}", self.actors[a]),
                    2 => format!("Note left of {}: {text}", self.actors[a]),
                    _ => format!("Note right of {}: {text}", self.actors[a]),
                };
                self.line(&line);
            }
            3..=5 if depth < 3 => {
                let (open, divider) = match self.rng.below(9) {
                    0 | 1 => ("loop", ""),
                    2 => ("opt", ""),
                    3 => ("break", ""),
                    4 => ("alt", "else"),
                    5 => ("par", "and"),
                    6 => ("par_over", "and"),
                    7 => ("critical", "option"),
                    _ => ("rect", ""),
                };
                let title = if open == "rect" { self.rng.pick(COLOURS) } else { self.rng.pick(TEXTS) };
                self.line(&format!("{open} {title}"));
                let sections = if divider.is_empty() { 1 } else { 1 + self.rng.below(3) };
                for section in 0..sections {
                    if section > 0 {
                        let title = self.rng.pick(TEXTS);
                        self.line(&format!("{divider} {title}"));
                    }
                    for _ in 0..self.rng.below(4) {
                        self.statement(depth + 1);
                    }
                }
                self.line("end");
            }
            6 => {
                let line = self.rng.pick(&[
                    "autonumber",
                    "autonumber 10",
                    "autonumber 2 .5",
                    "autonumber off",
                    "autonumber 100 25",
                    "autonumber 1.25 0.25",
                ]);
                self.line(line);
            }
            7 => {
                let a = self.actor();
                self.active[a] += 1;
                self.line(&format!("activate {}", self.actors[a]));
            }
            8 => {
                if let Some(a) = (0..self.actors.len()).find(|&a| self.active[a] > 0) {
                    self.active[a] -= 1;
                    self.line(&format!("deactivate {}", self.actors[a]));
                }
            }
            9 => {
                self.made += 1;
                let name = format!("N{}", self.made);
                let from = self.actor();
                let kind = self.rng.pick(&["participant", "actor"]);
                self.line(&format!("create {kind} {name}"));
                let text = self.rng.pick(TEXTS);
                let arrow = self.rng.pick(&["->>", "-->>", "-)", "->>+"]);
                self.line(&format!("{}{arrow}{name}: {text}", self.actors[from]));
                self.actors.push(name);
                self.active.push(usize::from(arrow.ends_with('+')));
            }
            10 => {
                let (gone, other) = (self.actor(), self.actor());
                self.line(&format!("destroy {}", self.actors[gone]));
                let text = self.rng.pick(TEXTS);
                let line = if self.rng.chance(2) {
                    format!("{}-x{}: {text}", self.actors[other], self.actors[gone])
                } else {
                    format!("{}->>{}: {text}", self.actors[gone], self.actors[other])
                };
                self.line(&line);
            }
            _ => self.message(),
        }
    }
}

/// A sequence diagram that Pele most likely renders, and the host options to render it with.
fn random_sequence(rng: &mut Rng) -> (String, Value) {
    let mut config = String::new();
    let settings = [
        (6, "mirrorActors: false".to_owned()),
        (4, "rightAngles: true".to_owned()),
        (6, "showSequenceNumbers: true".to_owned()),
        (10, "hideUnusedParticipants: true".to_owned()),
        (6, format!("messageAlign: {}", if rng.chance(2) { "left" } else { "right" })),
        (8, format!("actorMargin: {}", 5 + rng.below(90))),
        (8, format!("width: {}", 40 + rng.below(200))),
        (8, format!("height: {}", 20 + rng.below(80))),
        (8, format!("messageMargin: {}", rng.below(80))),
        (8, format!("noteMargin: {}", rng.below(30))),
        (8, format!("boxMargin: {}", rng.below(30))),
        (8, "wrap: true".to_owned()),
    ];
    for (one_in, line) in settings {
        if rng.chance(one_in) {
            config += &format!("    {line}\n");
        }
    }
    let title = rng.chance(6);
    let mut source = String::new();
    if !config.is_empty() || title {
        source += "---\n";
        if title {
            let text = rng.pick(&["A title", "A much longer title than the diagram is wide, most likely, by some way"]);
            source += &format!("title: {text}\n");
        }
        if !config.is_empty() {
            source += &format!("config:\n  sequence:\n{config}");
        }
        source += "---\n";
    }
    source += "sequenceDiagram\n";
    if rng.chance(10) {
        source += "accTitle: An accessible title\naccDescr: And a description\n";
    }
    let mut writer = Writer { rng, out: source, actors: Vec::new(), active: Vec::new(), made: 0 };
    let count = 1 + writer.rng.below(6);
    let mut boxed = 0;
    for &name in &NAMES[..count] {
        if boxed == 0 && writer.rng.chance(4) {
            let colour = writer.rng.pick(COLOURS);
            let title =
                writer.rng.pick(&["", "Group", "A box title that is wider than what it holds", "wrap: Wrapped"]);
            writer.line(&format!("box {colour} {title}"));
            boxed = 1 + writer.rng.below(3);
        }
        writer.declare(name);
        if boxed > 0 {
            boxed -= 1;
            if boxed == 0 {
                writer.line("end");
            }
        }
    }
    if boxed > 0 {
        writer.line("end");
    }
    if writer.rng.chance(8) {
        let a = writer.actor();
        let line = format!("properties {}: {{\"class\": \"k m\"}}", writer.actors[a]);
        writer.line(&line);
    }
    let many = writer.rng.chance(6);
    for _ in 0..1 + writer.rng.below(if many { 30 } else { 10 }) {
        writer.statement(0);
    }
    let source = writer.out;

    let mut options = serde_json::Map::new();
    if rng.below(4) > 0 {
        let width = match rng.below(6) {
            0 | 1 => 100.0 + rng.below(700) as f64 + 0.5,
            width => WIDTHS[width - 2],
        };
        options.insert("maxWidth".to_owned(), width.into());
    }
    if rng.chance(8) {
        options.insert("padding".to_owned(), (rng.below(41) as f64 / 2.0).into());
    }
    if rng.chance(8) {
        options.insert("fontSize".to_owned(), [12.0, 13.0, 14.0, 18.5, 20.0][rng.below(5)].into());
    }
    if rng.chance(5) {
        options.insert("monospace".to_owned(), json!(["messages"]));
    }
    (source, Value::Object(options))
}

#[test]
fn random_sequence_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_sequence(&mut rng)).collect();
    let tally = run("random sequence diagram", &cases).finish("random sequence diagrams");
    assert!(tally.identical > 3000, "nearly every random sequence diagram renders");
    tally.reaches_every_path("random sequence diagrams");
    println!("the random sequence diagrams reach the {} drawing paths: {:?}", PATHS.len(), tally.reached);
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random sequence diagrams another way");
    assert!(redrawn > 1000, "the widths put many random sequence diagrams through the tight draw");
}
