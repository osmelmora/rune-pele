//! Differential test for drawing ER diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/er/harvest.json` holds the ER sources of Pele's own test suite and the inputs
//! of its seeded mutation runs, written by `scripts/harvest-er.mjs`. The widths put every source
//! that runs across through the attempt of `turn_to_fit` to draw it running down.

use pele_core::{HostConfig, LayoutOptions, MonospacePart, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/er/harvest.json");
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
            "padding" => layout.padding = Some(number(value)),
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
    ("a subgraph", |svg| svg.contains("<g class=\"pele-cluster")),
    ("a subgraph with a fill of its own", |svg| svg.contains("fill=\"var(--_a)\" stroke=\"var(--_b)\" style=")),
    ("a subgraph title in a fill's ink", |svg| svg.contains("class=\"pele-cluster-label\" fill=\"var(--pele-fg-on-")),
    ("an edge cased over a fill's border", |svg| svg.contains("class=\"pele-casings\"")),
    ("an edge in a fill's ink", |svg| tag_has(svg, "<g class=\"pele-edge pele-er-", "stroke=\"var(--pele-fg-on-")),
    ("a marker hollow in a fill's colour", |svg| {
        svg.split("<circle class=\"pele-marker\"").skip(1).any(|rest| !rest.starts_with_fill_bg())
    }),
    ("an identifying relationship", |svg| svg.contains("class=\"pele-edge pele-er-identifying\"")),
    ("a non-identifying relationship", |svg| svg.contains("stroke-dasharray=\"4 3\"")),
    ("an only-one marker", |svg| has_marker(svg, "MLML")),
    ("a zero-or-one marker", |svg| has_marker(svg, "ML")),
    ("a one-or-more marker", |svg| has_marker(svg, "MLLML")),
    ("a zero-or-more marker", |svg| has_marker(svg, "MLL")),
    ("a parent marker", |svg| has_marker(svg, "MLLLZ")),
    ("a curved relationship", |svg| edge_paths(svg).iter().any(|d| d.contains('C') && !d.contains(['H', 'V']))),
    ("a relationship to itself out of the right side", |svg| edge_paths(svg).iter().any(|d| d.contains('H'))),
    ("a relationship to itself out of the bottom", |svg| edge_paths(svg).iter().any(|d| d.contains('V'))),
    ("an edge label", |svg| svg.contains("class=\"pele-edge-label\"")),
    ("an attribute table", |svg| svg.contains("class=\"pele-er-attributes\"")),
    ("a key", |svg| svg.contains("class=\"pele-er-keys\"")),
    ("a comment", |svg| svg.contains("class=\"pele-er-comment\"")),
    ("a wrapped comment", |svg| {
        svg.split("<text class=\"pele-er-comment\"").skip(1).any(|rest| {
            let text = &rest[..rest.find("</text>").unwrap_or(rest.len())];
            text.matches("<tspan x=").count() > 1
        })
    }),
    ("a generic type", |svg| tag_text_has(svg, "<text class=\"pele-er-type\"", "&lt;")),
    ("attributes in the monospace font", |svg| {
        tag_has(svg, "<text class=\"pele-er-type\"", "font-family=\"var(--_fm)\"")
    }),
    ("monospace text that keeps its repeated spaces", |svg| {
        svg.contains("font-family=\"var(--_fm)\" xml:space=\"preserve\"><tspan")
    }),
    ("bold attributes", |svg| tag_has(svg, "<text class=\"pele-er-name\"", "font-weight=\"var(--_w)\"")),
    ("an alias", has_alias),
    ("stripes tinted over a fill", |svg| svg.contains("opacity=\"0.6\">")),
    ("stripes dark on a light fill", |svg| svg.contains("class=\"pele-er-stripes\" fill=\"var(--pele-fg-on-dark")),
    ("stripes light on a dark fill", |svg| svg.contains("class=\"pele-er-stripes\" fill=\"var(--pele-fg-on-light")),
    ("a divider in the entity's stroke", |svg| {
        svg.split("class=\"pele-er-divider\"")
            .skip(1)
            .any(|rest| !rest[..rest.find('>').unwrap_or(0)].contains("stroke=\"var(--_b)\""))
    }),
    ("a name in a fill's ink", |svg| svg.contains("class=\"pele-label\" fill=\"var(--pele-fg-on-")),
    ("a name with a text style", |svg| tag_has(svg, "<text class=\"pele-label\"", " style=")),
    ("a rounded box with a style", |svg| svg.contains("style=\"rx:var(--_r);")),
    ("an author's class", |svg| svg.contains("class=\"pele-node pele-entity ")),
    ("an internal link", |svg| svg.contains("<a class=\"internal-link\"")),
    ("an internal link that is followed", |svg| svg.contains("<a class=\"internal-link\" href=")),
    ("an accessible title", |svg| svg.contains("aria-labelledby=")),
];

trait FillBg {
    fn starts_with_fill_bg(&self) -> bool;
}

impl FillBg for str {
    /// Whether a marker circle's attributes, which this text starts with, fill it with the page.
    fn starts_with_fill_bg(&self) -> bool {
        self[..self.find('>').unwrap_or(self.len())].contains("fill=\"var(--_bg)\"")
    }
}

/// Whether a tag that starts with `start` has `needle` among its attributes.
fn tag_has(svg: &str, start: &str, needle: &str) -> bool {
    svg.split(start).skip(1).any(|rest| rest[..rest.find('>').unwrap_or(rest.len())].contains(needle))
}

/// Whether a text element that starts with `start` holds `needle` in its text.
fn tag_text_has(svg: &str, start: &str, needle: &str) -> bool {
    svg.split(start).skip(1).any(|rest| rest[..rest.find("</text>").unwrap_or(rest.len())].contains(needle))
}

/// Whether a marker is drawn as a path whose commands are `shape`, such as `MLML` for two bars.
fn has_marker(svg: &str, shape: &str) -> bool {
    svg.split("<path class=\"pele-marker\" d=\"").skip(1).any(|piece| {
        let d = &piece[..piece.find('"').unwrap_or(0)];
        d.chars().filter(char::is_ascii_uppercase).eq(shape.chars())
    })
}

/// The `d` of each edge's line.
fn edge_paths(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-edge pele-er-")
        .skip(1)
        .filter_map(|rest| rest.split_once("<path d=\"").map(|(_, d)| &d[..d.find('"').unwrap_or(0)]))
        .collect()
}

/// Whether an entity is shown by a name other than its own.
fn has_alias(svg: &str) -> bool {
    svg.split("<g class=\"pele-node pele-entity").skip(1).any(|rest| {
        let Some((_, id)) = rest.split_once("data-id=\"") else {
            return false;
        };
        let id = &id[..id.find('"').unwrap_or(0)];
        let Some((_, label)) = rest.split_once("class=\"pele-label\"") else {
            return false;
        };
        let text = &label[label.find('>').map_or(0, |at| at + 1)..label.find("</text>").unwrap_or(0)];
        !text.contains('<') && text != id
    })
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
                    *if r#type == "er" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached.extend(PATHS.iter().filter(|(_, reached)| reached(svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "er" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "er" => {
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
            (PeleRender::Error { error, .. }, Err(_)) => {
                Some(format!("pele rejects an ER diagram, ours reports it as not ported: {error}"))
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

/// The widths a host may have. 320 and 480 are under the breakpoint at which a diagram that
/// runs across is turned to run down, 640 is on it and 900 over it.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested ER sources");
    assert!(tally.identical > 1000, "the harvest holds ER diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds ER diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested ER sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

/// How many of the cases Pele draws another way for the width they give than with room to
/// spare, which for ER is running down instead of across.
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
    run("mutated", &plain(harvest("mutated"))).finish("mutated ER sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "erDiagram\ndirection LR\nCUSTOMER ||--o{ ORDER : places\nORDER ||--|{ LINE-ITEM : contains\nCUSTOMER }|..|{ DELIVERY-ADDRESS : uses\nCUSTOMER {\n  string name\n  string custNumber PK\n}";
    let filled = "erDiagram\nsubgraph s [Filled group]\nA ||--o{ B : inside\nB {\n  int id PK\n}\nend\nC |o--o| A : crosses\nstyle s fill:#036\nstyle A fill:#ffd,stroke:#f00\nstyle B fill:navy,color:white";
    let loops = "erDiagram\nA ||--o{ A : self\nA }o--|| A : again\nA u..o| A : \"a much longer label\"\nB ||--|| B : first\nsubgraph g\nC\nend\ng ||--o{ g : group";
    let loops_down =
        "erDiagram\ndirection LR\nA ||--o{ A : self\nA }o--|| A : again\nB { int a\nint b\nint c }\nB ||--|| B : r";
    let tables = "erDiagram\nBOOK {\n  string title PK \"The title of the book, which may be rather long and so wraps onto more lines\"\n  string? subtitle\n  int *pages UK\n  List~string~ tags FK, UK \"many\"\n  `my  type` `my  name` PK,FK\n}\nstyle BOOK font-weight:bold";
    let linked = "erDiagram\nA:::internal-link\nB[\"Shown as\"]:::internal-link,k\n\"javascript:alert(1)\":::internal-link\nA ||--o{ B : r";
    let spaced = "---\nconfig:\n  er:\n    nodeSpacing: 10\n    rankSpacing: 120\n---\nerDiagram\nA ||--o{ B : r\nA ||--o{ C : r\nA ||--o{ D : r";
    let titled = "---\ntitle: A title much wider than the diagram under it, surely\n---\nerDiagram\naccTitle: Accessible\naccDescr: Described\nA";
    let fanned = "erDiagram\nHub ||--o{ A : first\nHub ||--o{ B : second\nHub ||--o{ C : third\nHub ||--o{ D : fourth\nE }|--|| Hub : in\nF }|--|| Hub : in";
    let mut out = vec![
        (across.to_owned(), json!({ "maxWidth": 200 })),
        (across.to_owned(), json!({ "maxWidth": 200, "autoDirection": false })),
        (across.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (across.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (across.to_owned(), json!({ "maxWidth": 0 })),
        (filled.to_owned(), json!({})),
        (loops.to_owned(), json!({})),
        (loops_down.to_owned(), json!({})),
        (loops_down.to_owned(), json!({ "maxWidth": 200 })),
        (tables.to_owned(), json!({})),
        (tables.to_owned(), json!({ "monospace": ["attributes"] })),
        (tables.to_owned(), json!({ "monospace": ["members"] })),
        (linked.to_owned(), json!({})),
        (linked.to_owned(), json!({ "links": false })),
        (linked.to_owned(), json!({ "linkSchemes": ["javascript"], "linkRel": "" })),
        (spaced.to_owned(), json!({})),
        (titled.to_owned(), json!({ "padding": 0 })),
        (titled.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (fanned.to_owned(), json!({})),
        (fanned.to_owned(), json!({ "fontSize": 12 })),
        ("erDiagram\nA".to_owned(), json!({ "outputLimit": 200 })),
        ("erDiagram\nA".to_owned(), json!({ "limit": 10 })),
    ];
    for config in [
        json!({"er": {"nodeSpacing": 1e308, "rankSpacing": -1}}),
        json!({"er": {"useMaxWidth": false, "nodeSpacing": "20"}}),
        json!({"er": "spaced"}),
    ] {
        out.push((fanned.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("ER cases");
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

const NAMES: &[&str] =
    &["A", "B", "C", "D", "E", "CUSTOMER", "ORDER", "LINE-ITEM", "\"Two words\"", "x1", "Product", "s"];
const LEFT: &[&str] = &["||", "|o", "}o", "}|", "u", "one", "zero or one", "1+", "many(0)"];
const RIGHT: &[&str] = &["||", "o|", "o{", "|{", "only one", "one or more", "0+", "1"];
const TYPES: &[&str] = &["string", "int", "List~int~", "Map~K, V~", "`a  type`", "string?", "*decimal", "date"];
const ATTRIBUTE_NAMES: &[&str] = &["name", "id", "`a  name`", "created_at", "a_longer_attribute_name"];
const KEYS: &[&str] = &["PK", "FK", "UK", "PK, FK", "PK,UK"];
const COMMENTS: &[&str] = &[
    "a comment",
    "",
    "**bold** and _soft_",
    "a comment long enough to wrap onto a second line in the table, at the least",
];
/// A relationship's cardinalities and line. The parent cardinality `u` is one only where a line
/// follows it at once.
fn relation(rng: &mut Rng) -> String {
    let left = rng.pick(LEFT);
    let line =
        if left == "u" { rng.pick(&["--", ".."]) } else { rng.pick(&["--", "--", "..", " to ", " optionally to "]) };
    format!("{left}{line}{}", rng.pick(RIGHT))
}

/// Style lists that Pele's style lexer takes: it has no `.` or `%`, so no fractions.
const STYLES: &[&str] = &[
    "fill:#f9f",
    "fill:#036",
    "fill:navy,color:white",
    "stroke:#333,stroke-width:4px",
    "color:red",
    "fill:#bbf,color:#fff",
    "color:#333,font-size:20px",
    "fill:#ffe,font-family:serif",
    "font-weight:bold,font-style:italic",
    "fill:#000,fill-opacity:0",
    "fill:none",
];

fn name(rng: &mut Rng, most: usize) -> String {
    NAMES[rng.below(most)].to_owned()
}

fn attribute(rng: &mut Rng) -> String {
    let mut out = format!("  {} {}", rng.pick(TYPES), rng.pick(ATTRIBUTE_NAMES));
    if rng.chance(3) {
        out += &format!(" {}", rng.pick(KEYS));
    }
    if rng.chance(3) {
        out += &format!(" \"{}\"", rng.pick(COMMENTS));
    }
    out
}

fn statement(rng: &mut Rng, depth: usize, most: usize) -> String {
    match rng.below(14) {
        0..=4 => {
            let (a, b) = (name(rng, most), if rng.chance(8) { "self".to_owned() } else { name(rng, most) });
            let b = if b == "self" { a.clone() } else { b };
            let label = rng.pick(&["has", "places", "\"a longer label\"", "\"\"", "r", "\"two\\nwords\""]);
            format!("{a} {} {b} : {label}", relation(rng))
        }
        5..=7 => {
            let mut out = name(rng, most);
            if rng.chance(4) {
                out += rng.pick(&["[Alias]", "[\"A long alias\"]"]);
            }
            if rng.chance(5) {
                out += rng.pick(&[":::k", ":::internal-link", ":::k,m"]);
            }
            if rng.chance(3) {
                out += " {\n";
                for _ in 0..1 + rng.below(4) {
                    out += &attribute(rng);
                    out += "\n";
                }
                out += "}";
            }
            out
        }
        8 if depth < 2 => {
            let mut out = format!(
                "subgraph {}{}\n",
                rng.pick(&["s", "g", "\"Group\"", "Outer"]),
                rng.pick(&["", "", " [A title]", " [\"A rather long group title\"]"])
            );
            if rng.chance(3) {
                out += &format!("direction {}\n", rng.pick(&["LR", "TB", "RL", "BT"]));
            }
            for _ in 0..1 + rng.below(3) {
                out += &statement(rng, depth + 1, most);
                out += "\n";
            }
            out + "end"
        }
        9 | 10 => match rng.below(3) {
            0 => format!("style {} {}", rng.pick(&["A", "B", "s", "g", "A,C"]), rng.pick(STYLES)),
            1 => format!("classDef {} {}", rng.pick(&["k", "m", "default"]), rng.pick(STYLES)),
            _ => format!("class {} {}", rng.pick(&["A", "B,C", "s"]), rng.pick(&["k", "m"])),
        },
        11 => {
            let a = name(rng, most);
            format!("{a} {} {a} : {}", relation(rng), rng.pick(&["self", "\"to itself\"", "x"]))
        }
        _ => rng.pick(&["%% a comment", "accTitle: The title", "accDescr: A description", ""]).to_owned(),
    }
}

/// An ER diagram that Pele nearly always renders, with host options, to put many graphs
/// through the markers, the spreading of ends along a side, the loops, the tables, the fills
/// and the turn to run down.
fn random_er(rng: &mut Rng) -> (String, Value) {
    let mut source = String::new();
    let mut config = String::new();
    if rng.chance(8) {
        config += &format!("    nodeSpacing: {}\n    rankSpacing: {}\n", 5 + rng.below(90), 5 + rng.below(90));
    }
    let title = rng.chance(6);
    if !config.is_empty() || title {
        source += "---\n";
        if title {
            source += &format!(
                "title: {}\n",
                rng.pick(&["A title", "A much longer title than the diagram is wide, most likely"])
            );
        }
        if !config.is_empty() {
            source += &format!("config:\n  er:\n{config}");
        }
        source += "---\n";
    }
    source += "erDiagram\n";
    // Most diagrams run across, so that the widths turn many to run down.
    if rng.below(3) > 0 {
        source += &format!("direction {}\n", rng.pick(&["LR", "RL", "LR", "RL", "BT", "TB"]));
    }
    let most = 2 + rng.below(NAMES.len() - 1);
    let many = rng.chance(5);
    for _ in 0..1 + rng.below(if many { 24 } else { 10 }) {
        source += &statement(rng, 0, most);
        source += "\n";
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
        options.insert("padding".to_owned(), (rng.below(41) as f64 / 2.0).into());
    }
    if rng.chance(8) {
        options.insert("fontSize".to_owned(), [12.0, 13.0, 14.0, 18.5, 20.0][rng.below(5)].into());
    }
    if rng.chance(5) {
        options.insert("monospace".to_owned(), json!(["attributes"]));
    }
    if rng.chance(16) {
        options.insert("links".to_owned(), false.into());
    }
    (source, Value::Object(options))
}

#[test]
fn random_er_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_er(&mut rng)).collect();
    let tally = run("random ER diagram", &cases).finish("random ER diagrams");
    assert!(tally.identical > 3000, "nearly every random ER diagram renders");
    tally.reaches_every_path("random ER diagrams");
    println!("the random ER diagrams reach the {} drawing paths: {:?}", PATHS.len(), tally.reached);
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random ER diagrams another way");
    assert!(redrawn > 150, "the widths put many random ER diagrams through the turn to run down");
}
