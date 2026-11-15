//! Differential test for drawing C4 diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/c4/harvest.json` holds the C4 sources of Pele's own test suite and the inputs
//! of its generated and mutated runs, written by `scripts/harvest-c4.mjs`. The widths put every
//! diagram wider than the host through Pele's tight drawing, with its wider gaps. The random
//! diagrams carry the proof for what the six corpus sources never draw, and must reach every
//! drawing path in [`PATHS`].

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/c4/harvest.json");
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

type Reached = fn(&str, &str) -> bool;

/// Whether a tag that starts with `start` has `needle` among its attributes.
fn tag_has(svg: &str, start: &str, needle: &str) -> bool {
    svg.split(start).skip(1).any(|rest| rest[..rest.find('>').unwrap_or(rest.len())].contains(needle))
}

/// The part of the drawing inside the layer that opens with `start`.
fn layer<'a>(svg: &'a str, start: &str) -> &'a str {
    svg.split_once(start).map_or("", |(_, rest)| rest)
}

/// The nodes as their kind and the form they are drawn in.
fn node_forms(svg: &str) -> Vec<(&str, &str)> {
    svg.split("<g class=\"pele-node pele-shape-")
        .skip(1)
        .filter_map(|rest| {
            let (form, rest) = rest.split_once(" pele-c4-")?;
            let kind = &rest[..rest.find([' ', '"']).unwrap_or(0)];
            Some((form, kind))
        })
        .collect()
}

/// The form a shape of `kind` takes when nothing names another.
fn default_form(kind: &str) -> &'static str {
    if kind.ends_with("person") {
        "person"
    } else if kind.ends_with("_db") {
        "cyl"
    } else if kind.ends_with("_queue") {
        "h-cyl"
    } else {
        "rect"
    }
}

/// The frames of the boundaries as x, y, w, h, and the x of their heading's first line.
fn boundaries(svg: &str) -> Vec<([f64; 4], Option<f64>)> {
    let attribute = |text: &str, name: &str| -> Option<f64> {
        let (_, rest) = text.split_once(&format!(" {name}=\""))?;
        rest[..rest.find('"')?].parse().ok()
    };
    svg.split("<g class=\"pele-cluster pele-c4-")
        .skip(1)
        .filter_map(|rest| {
            let rect = &rest[rest.find("<rect")?..];
            let rect = &rect[..rect.find('>')?];
            let frame =
                [attribute(rect, "x")?, attribute(rect, "y")?, attribute(rect, "width")?, attribute(rect, "height")?];
            let end = rest.find("</g>").unwrap_or(rest.len());
            let heading =
                rest[..end].split_once("class=\"pele-cluster-label\"").and_then(|(_, label)| attribute(label, "x"));
            Some((frame, heading))
        })
        .collect()
}

fn has_kind(svg: &str, kind: &str) -> bool {
    svg.contains(&format!(" pele-c4-{kind}\"")) || svg.contains(&format!(" pele-c4-{kind} "))
}

/// Whether the source uses a statement and Pele drew a relation from it.
fn uses(source: &str, svg: &str, keyword: &str) -> bool {
    source.lines().any(|line| line.trim_start().starts_with(keyword)) && svg.contains("<g class=\"pele-edge\"")
}

/// The `d` of each relation's line.
fn edge_paths(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-edge\"")
        .skip(1)
        .filter_map(|rest| rest.split_once("<path d=\"").map(|(_, d)| &d[..d.find('"').unwrap_or(0)]))
        .collect()
}

/// The drawing paths a source can go through, each with what in the source and Pele's SVG shows
/// it did.
const PATHS: &[(&str, Reached)] = &[
    ("a context diagram", |source, _| source.trim_start().starts_with("C4Context")),
    ("a container diagram", |source, _| source.trim_start().starts_with("C4Container")),
    ("a component diagram", |source, _| source.trim_start().starts_with("C4Component")),
    ("a dynamic diagram with numbered relations", |_, svg| layer(svg, "class=\"pele-edge-labels\"").contains(">1: ")),
    ("a deployment diagram", |source, _| source.trim_start().starts_with("C4Deployment")),
    ("a person", |_, svg| has_kind(svg, "person")),
    ("an external person", |_, svg| has_kind(svg, "external_person")),
    ("a system", |_, svg| has_kind(svg, "system")),
    ("a system database", |_, svg| has_kind(svg, "system_db")),
    ("a system queue", |_, svg| has_kind(svg, "system_queue")),
    ("an external system", |_, svg| has_kind(svg, "external_system")),
    ("an external system database", |_, svg| has_kind(svg, "external_system_db")),
    ("an external system queue", |_, svg| has_kind(svg, "external_system_queue")),
    ("a container", |_, svg| has_kind(svg, "container")),
    ("a container database", |_, svg| has_kind(svg, "container_db")),
    ("a container queue", |_, svg| has_kind(svg, "container_queue")),
    ("an external container", |_, svg| has_kind(svg, "external_container")),
    ("an external container database", |_, svg| has_kind(svg, "external_container_db")),
    ("an external container queue", |_, svg| has_kind(svg, "external_container_queue")),
    ("a component", |_, svg| has_kind(svg, "component")),
    ("a component database", |_, svg| has_kind(svg, "component_db")),
    ("a component queue", |_, svg| has_kind(svg, "component_queue")),
    ("an external component", |_, svg| has_kind(svg, "external_component")),
    ("an external component database", |_, svg| has_kind(svg, "external_component_db")),
    ("an external component queue", |_, svg| has_kind(svg, "external_component_queue")),
    ("a shape drawn in another form by its $shape, $sprite or $tags", |_, svg| {
        node_forms(svg).iter().any(|&(form, kind)| form != default_form(kind))
    }),
    ("a $sprite that names no form", |source, svg| {
        source.contains("$sprite=\"") && svg.contains("<g class=\"pele-node ")
    }),
    ("a shape with technology and a description", |_, svg| {
        layer(svg, "<g class=\"pele-nodes\">").contains("class=\"pele-c4-techn\"")
            && layer(svg, "<g class=\"pele-nodes\">").contains("class=\"pele-c4-descr\"")
    }),
    ("a shape with a fill of its own and ink to match", |_, svg| {
        tag_has(layer(svg, "<g class=\"pele-nodes\">"), "<text class=\"pele-label\"", "fill=\"var(--pele-fg-on-")
    }),
    ("a shape with a border colour", |_, svg| {
        let nodes = layer(svg, "<g class=\"pele-nodes\">");
        nodes.contains(" style=\"stroke:") || nodes.contains(";stroke:")
    }),
    ("a boundary", |_, svg| svg.contains("<g class=\"pele-cluster pele-c4-boundary\"")),
    ("a deployment node", |_, svg| svg.contains("<g class=\"pele-cluster pele-c4-node\"")),
    ("a boundary inside a boundary", |_, svg| {
        let frames = boundaries(svg);
        frames.iter().any(|(outer, _)| {
            frames.iter().any(|(inner, _)| {
                inner != outer
                    && outer[0] <= inner[0]
                    && outer[1] <= inner[1]
                    && outer[0] + outer[2] >= inner[0] + inner[2]
                    && outer[1] + outer[3] >= inner[1] + inner[3]
            })
        })
    }),
    ("a boundary heading moved clear of a line", |_, svg| {
        boundaries(svg).iter().any(|(frame, heading)| heading.is_some_and(|x| (x - frame[0] - 16.0).abs() > 0.01))
    }),
    ("a boundary with a description", |_, svg| layer(svg, "<g class=\"pele-clusters\">").contains("pele-c4-descr")),
    ("a boundary with a fill", |_, svg| {
        tag_has(layer(svg, "<g class=\"pele-clusters\">"), "<rect", "style=\"rx:var(--_r);fill:")
    }),
    ("a relation", |_, svg| svg.contains("<g class=\"pele-edge\"")),
    ("Rel_U or Rel_Up", |source, svg| uses(source, svg, "Rel_U")),
    ("Rel_D or Rel_Down", |source, svg| uses(source, svg, "Rel_D")),
    ("Rel_L or Rel_Left", |source, svg| uses(source, svg, "Rel_L")),
    ("Rel_R or Rel_Right", |source, svg| uses(source, svg, "Rel_R")),
    ("Rel_Back", |source, svg| uses(source, svg, "Rel_Back")),
    ("RelIndex", |source, svg| uses(source, svg, "RelIndex")),
    ("a relation with arrowheads at both ends", |_, svg| {
        svg.split("<g class=\"pele-edge\"")
            .skip(1)
            .any(|edge| edge[..edge.find("</g>").unwrap_or(0)].matches("pele-marker").count() == 2)
    }),
    ("a relation to itself", |_, svg| edge_paths(svg).iter().any(|d| d.contains('C'))),
    ("a relation with a corner", |_, svg| edge_paths(svg).iter().any(|d| d.contains('Q'))),
    ("a relation from a boundary", |_, svg| {
        let clusters: Vec<&str> = svg
            .split("<g class=\"pele-cluster pele-c4-")
            .skip(1)
            .filter_map(|rest| {
                let (_, id) = rest.split_once("data-id=\"")?;
                Some(&id[..id.find('"')?])
            })
            .collect();
        svg.split("<g class=\"pele-edge\" data-id=\"")
            .skip(1)
            .any(|rest| clusters.iter().any(|id| rest.starts_with(&format!("{id}-"))))
    }),
    ("a relation cased over a fill's border", |_, svg| svg.contains("class=\"pele-casings\"")),
    ("a relation in a fill's ink", |_, svg| tag_has(svg, "<g class=\"pele-edge\"", "stroke=\"var(--pele-fg-on-")),
    ("a relation with a line colour of its own", |_, svg| tag_has(svg, "<g class=\"pele-edge\"", " style=\"stroke:")),
    ("a relation label with technology", |_, svg| layer(svg, "class=\"pele-edge-labels\"").contains("pele-c4-techn")),
    ("a relation label with a text colour", |_, svg| {
        tag_has(layer(svg, "class=\"pele-edge-labels\""), "<text", "style=\"fill:")
    }),
    ("a relation label with a tooltip", |_, svg| {
        svg.contains("<g class=\"pele-edge-label\"") && svg.contains("<title>")
    }),
    ("a relation label moved by its offsets", |source, svg| {
        source.contains("$offset") && svg.contains("<g class=\"pele-edge-label\"")
    }),
    ("a linked shape", |_, svg| layer(svg, "<g class=\"pele-nodes\">").contains("<a href=")),
    ("a linked boundary heading", |_, svg| {
        layer(svg, "<g class=\"pele-clusters\">")
            .split("<g class=\"pele-edges\"")
            .next()
            .unwrap_or("")
            .contains("<a href=")
    }),
    ("a linked relation label", |_, svg| layer(svg, "class=\"pele-edge-labels\"").contains("<a href=")),
    ("UpdateLayoutConfig", |source, svg| {
        source.contains("UpdateLayoutConfig(") && svg.contains("<g class=\"pele-node ")
    }),
    ("UpdateElementStyle", |source, svg| source.contains("UpdateElementStyle(") && svg.contains(" style=\"")),
    ("UpdateRelStyle", |source, svg| source.contains("UpdateRelStyle(") && svg.contains("<g class=\"pele-edge\"")),
    ("tags that name no form", |source, svg| source.contains("$tags=\"") && svg.contains("<g class=\"pele-node ")),
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("an accessible description", |_, svg| svg.contains("aria-describedby=")),
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
                    *if r#type == "c4" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "c4" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "c4" => {
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
                Some(format!("pele rejects a C4 diagram, ours reports it as not ported: {error}"))
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
            self.failures[..self.failures.len().min(8)].join("\n")
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

/// The widths a host may have.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

/// How many of the cases Pele draws another way for the width they give than with room to
/// spare, which for C4 is the tight drawing.
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested C4 sources");
    assert!(tally.identical > 1000, "the harvest holds C4 diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds C4 diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested C4 sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated C4 sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let linked = "C4Context\nSystem(a, \"A\", $link=\"https://example.com/a#x;y\")\nBoundary(b, \"B\", $link=\"/docs\") {\nSystem(c, \"C\", $link=\"javascript:alert(1)\")\n}\nRel(a, c, \"uses\", \"HTTPS\", \"why\", $link=\"mailto:x@example.com\")";
    let wide = "C4Container\nUpdateLayoutConfig(\"6\", \"3\")\nPerson(p, \"Person\", \"A user of the system with a long description\")\nSystem(s1, \"One\")\nSystem(s2, \"Two\")\nSystemDb(s3, \"Three\")\nSystemQueue(s4, \"Four\")\nSystem_Ext(s5, \"Five\")\nRel(p, s5, \"crosses every shape between\", \"HTTPS\")\nRel(s5, p, \"and back\")\nRel(s1, s4, \"skips\")\nBiRel(s2, s3, \"both\")";
    let filled = "C4Context\nBoundary(b, \"Filled\", \"type\", \"descr\") {\nSystem(a, \"A\")\nSystem(c, \"C\")\n}\nSystem(d, \"D\")\nRel(a, c, \"inside\")\nRel(a, d, \"crosses\")\nUpdateElementStyle(b, $bgColor=\"#123\", $fontColor=\"white\", $borderColor=\"red\")\nUpdateElementStyle(a, $bgColor=\"yellow\")\nUpdateRelStyle(a, d, $lineColor=\"green\", $textColor=\"blue\", $offsetX=\"30\", $offsetY=\"-20\")";
    let titled = "---\ntitle: A title much wider than the diagram under it, as wide as can be\n---\nC4Context\naccDescr: Described\nPerson(a, \"A\")";
    let dynamic = "C4Dynamic\nContainer(a, \"A\")\nContainer(b, \"B\")\nContainer(c, \"C\")\nRelIndex(1, a, b, \"one\")\nRelIndex(2, b, c, \"two\", \"tech\")\nRel(c, a, \"three\")";
    let mut out = vec![
        (linked.to_owned(), json!({})),
        (linked.to_owned(), json!({ "links": false })),
        (linked.to_owned(), json!({ "linkSchemes": ["javascript"], "linkRel": "" })),
        (linked.to_owned(), json!({ "linkRel": "noopener noreferrer" })),
        (wide.to_owned(), json!({})),
        (wide.to_owned(), json!({ "maxWidth": 300 })),
        (wide.to_owned(), json!({ "maxWidth": 600, "padding": 0 })),
        (wide.to_owned(), json!({ "maxWidth": 0 })),
        (wide.to_owned(), json!({ "fontSize": 12 })),
        (wide.to_owned(), json!({ "fontSize": 20, "maxWidth": 500 })),
        (filled.to_owned(), json!({})),
        (filled.to_owned(), json!({ "maxWidth": 320 })),
        (titled.to_owned(), json!({ "padding": 0 })),
        (titled.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (titled.to_owned(), json!({ "maxWidth": 200 })),
        (dynamic.to_owned(), json!({})),
        ("C4Context\nPerson(a, \"A\")".to_owned(), json!({ "outputLimit": 200 })),
        ("C4Context\nPerson(a, \"A\")".to_owned(), json!({ "limit": 10 })),
    ];
    for config in [json!({"c4": {"useMaxWidth": false}}), json!({"c4": "x"}), json!({"c4": {"c4ShapeInRow": 1}})] {
        out.push((wide.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("C4 cases");
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

const HEADERS: &[&str] = &["C4Context", "C4Container", "C4Component", "C4Dynamic", "C4Deployment"];
const SHAPES: &[&str] = &[
    "Person",
    "Person_Ext",
    "System",
    "SystemDb",
    "SystemQueue",
    "System_Ext",
    "SystemDb_Ext",
    "SystemQueue_Ext",
    "Container",
    "ContainerDb",
    "ContainerQueue",
    "Container_Ext",
    "ContainerDb_Ext",
    "ContainerQueue_Ext",
    "Component",
    "ComponentDb",
    "ComponentQueue",
    "Component_Ext",
    "ComponentDb_Ext",
    "ComponentQueue_Ext",
];
const BOUNDARIES: &[&str] = &[
    "Boundary",
    "Enterprise_Boundary",
    "System_Boundary",
    "Container_Boundary",
    "Deployment_Node",
    "Node",
    "Node_L",
    "Node_R",
];
const RELS: &[&str] = &[
    "Rel",
    "Rel",
    "BiRel",
    "Rel_U",
    "Rel_Up",
    "Rel_D",
    "Rel_Down",
    "Rel_L",
    "Rel_Left",
    "Rel_R",
    "Rel_Right",
    "Rel_Back",
];
const LABELS: &[&str] = &[
    "Customer",
    "Internet Banking System",
    "A",
    "Sends e-mails to the customer with a summary of the day",
    "Two<br>lines",
    "&amp; <b>bold</b>",
    "unbreakableunbreakableunbreakableunbreakable",
    "",
];
const COLOURS: &[&str] = &["red", "#036", "navy", "#ffeb3b", "rgba(0,0,0,0.2)", "white", "grey", "url(x)"];
const FORMS: &[&str] = &["db", "queue", "person", "box", "Cylinder", "pipe", "rounded", "nothing"];

/// A random C4 diagram: shapes of every kind in boundaries nested up to three deep, relations
/// of every keyword between shapes and boundaries, to themselves and back again, and the
/// statements that restyle them.
fn random_diagram(rng: &mut Rng) -> String {
    let header = rng.pick(HEADERS);
    let mut src = format!("{header}\n");
    if rng.chance(4) {
        src.push_str(&format!(
            "title {}\n",
            rng.pick(&["System context", "A title wider than the diagram, surely, as long as it is"])
        ));
    }
    if rng.chance(8) {
        src.push_str("accDescr: what the diagram shows\n");
    }
    let mut shapes: Vec<String> = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    let mut depth = 0;
    for _ in 0..1 + rng.below(12) {
        match rng.below(10) {
            0..=1 if depth < 3 => {
                let alias = format!("b{}", groups.len());
                let mut args = format!("{alias}, \"{}\"", rng.pick(LABELS));
                if rng.chance(2) {
                    args.push_str(&format!(", \"{}\"", rng.pick(&["software", "", "region"])));
                }
                if rng.chance(3) {
                    args.push_str(&format!(", \"{}\"", rng.pick(LABELS)));
                }
                if rng.chance(8) {
                    args.push_str(", $link=\"https://example.com\"");
                }
                src.push_str(&format!("{}({args}) {{\n", rng.pick(BOUNDARIES)));
                groups.push(alias);
                depth += 1;
                // The first statement in a boundary has to be a diagram statement.
                let alias = format!("s{}", shapes.len());
                src.push_str(&format!("{}({alias}, \"{}\")\n", rng.pick(SHAPES), rng.pick(LABELS)));
                shapes.push(alias);
            }
            2 if depth > 0 => {
                src.push_str("}\n");
                depth -= 1;
            }
            _ => {
                let alias = format!("s{}", shapes.len());
                let kind = rng.pick(SHAPES);
                let mut args = format!("{alias}, \"{}\"", rng.pick(LABELS));
                for _ in 0..rng.below(3) {
                    args.push_str(&format!(", \"{}\"", rng.pick(LABELS)));
                }
                match rng.below(10) {
                    0 => args.push_str(&format!(", $shape=\"{}\"", rng.pick(FORMS))),
                    1 => args.push_str(&format!(", $sprite=\"{}\"", rng.pick(FORMS))),
                    2 => args.push_str(&format!(", $tags=\"v1, {}\"", rng.pick(FORMS))),
                    3 => args.push_str(", $link=\"https://example.com/x\""),
                    4 => args.push_str(&format!(", $descr=\"{}\", $techn=\"Go\"", rng.pick(LABELS))),
                    _ => {}
                }
                src.push_str(&format!("{kind}({args})\n"));
                shapes.push(alias);
            }
        }
    }
    for _ in 0..depth {
        src.push_str("}\n");
    }
    let ends: Vec<&String> = shapes.iter().chain(groups.iter().filter(|_| rng.chance(3))).collect();
    let mut rels: Vec<(String, String)> = Vec::new();
    for _ in 0..rng.below(2 + 2 * ends.len()) {
        let from = ends[rng.below(ends.len())].clone();
        let to = if rng.chance(10) {
            from.clone()
        } else if let (true, Some((a, b))) = (rng.chance(5), rels.last()) {
            let _ = a;
            b.clone()
        } else {
            ends[rng.below(ends.len())].clone()
        };
        let mut args = format!("{from}, {to}, \"{}\"", rng.pick(LABELS));
        if rng.chance(2) {
            args.push_str(&format!(", \"{}\"", rng.pick(&["HTTPS", "JSON/HTTPS, async", ""])));
        }
        if rng.chance(4) {
            args.push_str(&format!(", \"{}\"", rng.pick(&["Why it matters", ""])));
        }
        if rng.chance(10) {
            args.push_str(", $link=\"https://example.com/r\"");
        }
        let keyword = if header == "C4Dynamic" && rng.chance(2) { "RelIndex" } else { rng.pick(RELS) };
        if keyword == "RelIndex" {
            args = format!("{}, {args}", rels.len() + 1);
        }
        src.push_str(&format!("{keyword}({args})\n"));
        rels.push((from, to));
    }
    if rng.chance(3) && !shapes.is_empty() {
        let alias = &shapes[rng.below(shapes.len())];
        src.push_str(&format!(
            "UpdateElementStyle({alias}, $bgColor=\"{}\", $fontColor=\"{}\", $borderColor=\"{}\")\n",
            rng.pick(COLOURS),
            rng.pick(COLOURS),
            rng.pick(COLOURS)
        ));
    }
    if rng.chance(3) && !groups.is_empty() {
        let alias = &groups[rng.below(groups.len())];
        src.push_str(&format!("UpdateElementStyle({alias}, \"{}\", \"{}\")\n", rng.pick(COLOURS), rng.pick(COLOURS)));
    }
    if rng.chance(3) && !rels.is_empty() {
        let (from, to) = &rels[rng.below(rels.len())];
        let offsets = if rng.chance(2) { ", $offsetX=\"40\", $offsetY=\"-30\"" } else { "" };
        src.push_str(&format!(
            "UpdateRelStyle({from}, {to}, $textColor=\"{}\", $lineColor=\"{}\"{offsets})\n",
            rng.pick(COLOURS),
            rng.pick(COLOURS)
        ));
    }
    if rng.chance(4) {
        src.push_str(&format!(
            "UpdateLayoutConfig($c4ShapeInRow=\"{}\", $c4BoundaryInRow=\"{}\")\n",
            1 + rng.below(5),
            1 + rng.below(3)
        ));
    }
    src
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x0c4_d1a9_2a35);
    let mut cases = Vec::new();
    for _ in 0..4000 {
        let source = random_diagram(&mut rng);
        let options = match rng.below(6) {
            0 => json!({ "maxWidth": 320 }),
            1 => json!({ "maxWidth": *rng.pick(&["480", "640", "900"]).parse::<f64>().as_ref().unwrap() }),
            _ => json!({}),
        };
        cases.push((source, options));
    }
    let tally = run("random", &cases).finish("random C4 diagrams");
    let redrawn = redrawn_to_fit(
        &cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect::<Vec<_>>(),
    );
    println!("their widths make Pele draw {redrawn} of the random C4 diagrams another way");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random C4 diagram reaches {missed:?}");
}

/// Pele's own worst cases for C4 (`tests/c4/stress.test.ts`) that render, at a tenth of their
/// size where they are larger than this test needs, with the source limit lifted as that test
/// lifts it. Many relations among few shapes is where labels stay at the middle and lines are
/// not steered round shapes.
fn stress() -> Vec<String> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let h = "C4Context\n";
    let mut out = vec![
        format!("{h}{}", repeat(250, &|i| format!("System(s{i}, \"S{i}\")\n"))),
        format!(
            "{h}UpdateLayoutConfig(\"99999999\", \"1\")\n{}",
            repeat(250, &|i| format!("System(s{i}, \"S{i}\")\n"))
        ),
        format!(
            "{h}{}System(s, \"S\")\n{}",
            repeat(200, &|i| format!("Boundary(b{i}, \"B{i}\") {{\n")),
            "}\n".repeat(200)
        ),
        format!("{h}{}", repeat(120, &|i| format!("Boundary(b{i}, \"B\") {{\nSystem(s{i}, \"S\")\n}}\n"))),
        format!("{h}{}", "System(a, \"A\", \"d\")\n".repeat(300)),
        format!(
            "{h}System(a,\"A\")\nSystem(b,\"B\")\n{}",
            "Rel(a,a,\"x\")\nRel(a,b,\"y\")\nRel(b,a,\"z\")\n".repeat(70)
        ),
        format!(
            "{h}System(top, \"T\")\n{}System(deep, \"D\")\n{}Rel(top, deep, \"x\")\nRel(n0, deep, \"y\")\nRel(deep, n99, \"z\")",
            repeat(100, &|i| format!("Node(n{i}, \"N\") {{\n")),
            "}\n".repeat(100)
        ),
        format!("{h}System(a, \"{}\")", "word ".repeat(2000)),
        format!("{h}System(a, \"{}\")", "w".repeat(5000)),
        format!(
            "{h}System(a,\"A\")\nSystem(b,\"B\")\nRel(a, b, \"{}\", \"{}\")",
            "word ".repeat(2000),
            "tech ".repeat(100)
        ),
        format!("{h}Boundary(b, \"{}\") {{\nSystem(a,\"A\")\n}}", "word ".repeat(2000)),
        format!("{h}System(a, \"A\", $tags=\"{}\")", "a,".repeat(2500)),
    ];
    let mut careless = format!("{h}{}", repeat(50, &|i| format!("System(s{i},\"S\")\n")));
    for i in 0..50 {
        for j in 0..50 {
            if i != j {
                careless.push_str(&format!("Rel(s{i},s{j},\"x\")\n"));
            }
        }
    }
    out.push(careless);
    let mut row =
        format!("{h}UpdateLayoutConfig(\"100\", \"1\")\n{}", repeat(100, &|i| format!("System(s{i},\"S\")\n")));
    for k in 0..48 {
        row.push_str(&format!("Rel(s0,s{},\"x\")\n", k + 2));
    }
    out.push(row);
    let mut boundaries =
        format!("{h}{}", repeat(60, &|i| format!("Boundary(b{i}, \"A heading {i}\") {{\nSystem(s{i},\"S\")\n}}\n")));
    for k in 0..60 {
        boundaries.push_str(&format!("Rel(b{k},s{},\"x\")\n", (k * 11 + 5) % 60));
    }
    out.push(boundaries);
    out
}

#[test]
fn stress_sources_match_pele() {
    let options = json!({ "limit": 1_000_000_000u64, "outputLimit": 1_000_000_000u64 });
    let cases: Vec<(String, Value)> = stress().into_iter().map(|source| (source, options.clone())).collect();
    let tally = run("stress", &cases).finish("Pele's worst-case C4 diagrams");
    assert!(tally.identical == cases.len(), "every worst case renders");
}
