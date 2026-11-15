//! Differential test for drawing block diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/block/harvest.json` holds the block sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-block.mjs`. The widths put every
//! source that does not fit through the tight attempt of `tighten`.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/block/harvest.json");
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
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

type Reached = fn(&str, &str) -> bool;

/// The drawing paths a source can go through, each with what in the source and in Pele's SVG
/// shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("a composite", |_, svg| svg.contains("<g class=\"pele-cluster")),
    ("a composite with a title", |_, svg| svg.contains("class=\"pele-cluster-label\"")),
    ("a composite inside a composite", |_, svg| nested_clusters(svg)),
    ("a composite with a fill of its own", |_, svg| svg.contains("fill=\"var(--_a)\" stroke=\"var(--_b)\" style=")),
    ("a composite title in a fill's ink", |_, svg| {
        svg.contains("class=\"pele-cluster-label\" fill=\"var(--pele-fg-on-")
    }),
    ("a block over several columns", |source, _| has_span(source)),
    ("a column count", |source, _| source.contains("columns ")),
    ("a space", |source, _| source.contains("space")),
    ("a rect", |_, svg| svg.contains("pele-shape-rect")),
    ("a rounded block", |_, svg| svg.contains("pele-shape-rounded")),
    ("a circle", |_, svg| svg.contains("pele-shape-circle")),
    ("a double circle", |_, svg| svg.contains("pele-shape-dbl-circ")),
    ("a diamond", |_, svg| svg.contains("pele-shape-diam")),
    ("a hexagon", |_, svg| svg.contains("pele-shape-hex")),
    ("a stadium", |_, svg| svg.contains("pele-shape-stadium")),
    ("a subroutine", |_, svg| svg.contains("pele-shape-fr-rect")),
    ("a cylinder", |_, svg| svg.contains("pele-shape-cyl")),
    ("an odd block", |_, svg| svg.contains("pele-shape-odd")),
    ("a lean right", |_, svg| svg.contains("pele-shape-lean-r")),
    ("a lean left", |_, svg| svg.contains("pele-shape-lean-l")),
    ("a trapezoid", |_, svg| svg.contains("pele-shape-trap-b")),
    ("an inverted trapezoid", |_, svg| svg.contains("pele-shape-trap-t")),
    ("a block arrow with one head", |_, svg| arrow_corners(svg).contains(&7)),
    ("a block arrow with two heads", |_, svg| arrow_corners(svg).contains(&10)),
    ("a block arrow pointing nowhere", |_, svg| arrow_corners(svg).contains(&4)),
    ("a block arrow along both axes", |_, svg| arrow_corners(svg).iter().any(|&n| n == 13 || n == 16)),
    ("an arrow to the right", |source, svg| source.contains("(right)") && svg.contains("block-arrow")),
    ("an arrow to the left", |source, svg| source.contains("(left)") && svg.contains("block-arrow")),
    ("an arrow up", |source, svg| source.contains("(up)") && svg.contains("block-arrow")),
    ("an arrow down", |source, svg| source.contains("(down)") && svg.contains("block-arrow")),
    ("a straight edge", |_, svg| edge_paths(svg).iter().any(|d| straight(d) == Some(true))),
    ("a diagonal edge", |_, svg| edge_paths(svg).iter().any(|d| straight(d) == Some(false))),
    ("an edge with a bend", |_, svg| edge_paths(svg).iter().any(|d| d.contains('Q'))),
    ("a self-loop", |_, svg| edge_paths(svg).iter().any(|d| d.contains('A'))),
    ("a self-loop of more than half a turn", |_, svg| edge_paths(svg).iter().any(|d| d.contains(" 0 1 1 "))),
    ("an edge label", |_, svg| svg.contains("class=\"pele-edge-label\"")),
    ("a thick edge", |_, svg| svg.contains("stroke-width=\"2.5\"")),
    ("a dotted edge", |_, svg| svg.contains("stroke-dasharray=\"3 4\"")),
    ("an arrowhead", |_, svg| svg.contains("<path class=\"pele-marker\" d=\"M") && svg.contains("Z\" fill=")),
    ("a circle marker", |_, svg| svg.contains("<circle class=\"pele-marker\"")),
    ("a cross marker", |_, svg| {
        svg.split("<path class=\"pele-marker\" d=\"")
            .skip(1)
            .any(|d| d[..d.find('"').unwrap_or(0)].matches('M').count() == 2)
    }),
    ("an edge cased over a fill's border", |_, svg| svg.contains("class=\"pele-casings\"")),
    ("an edge in a fill's ink", |_, svg| tag_has(svg, "<g class=\"pele-edge\"", "stroke=\"var(--pele-fg-on-")),
    ("a label in a fill's ink", |_, svg| svg.contains("class=\"pele-label\" fill=\"var(--pele-fg-on-")),
    ("a styled block", |_, svg| {
        tag_has(svg, "<g class=\"pele-node", "") && svg.contains("stroke=\"var(--_b)\" style=")
    }),
    ("a label with a text style", |_, svg| tag_has(svg, "<text class=\"pele-label\"", " style=")),
    ("a bold label", |_, svg| tag_has(svg, "<text class=\"pele-label\"", "font-weight=")),
    ("an author's class", |_, svg| has_author_class(svg)),
    ("a wrapped label", |_, svg| {
        svg.split("<text class=\"pele-label\"")
            .skip(1)
            .any(|rest| rest[..rest.find("</text>").unwrap_or(0)].matches("<tspan").count() > 1)
    }),
];

/// Whether a tag that starts with `start` has `needle` among its attributes.
fn tag_has(svg: &str, start: &str, needle: &str) -> bool {
    svg.split(start).skip(1).any(|rest| rest[..rest.find('>').unwrap_or(rest.len())].contains(needle))
}

fn has_author_class(svg: &str) -> bool {
    svg.split("<g class=\"pele-node pele-shape-").skip(1).any(|rest| rest[..rest.find('"').unwrap_or(0)].contains(' '))
}

/// Whether a block is given more than one column: a size after a node or a composite's header.
fn has_span(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes.windows(3).any(|w| {
        (w[0].is_ascii_alphanumeric() || w[0] == b')' || w[0] == b']' || w[0] == b'}')
            && w[1] == b':'
            && (b'2'..=b'9').contains(&w[2])
    })
}

/// The number of corners of each block arrow.
fn arrow_corners(svg: &str) -> Vec<usize> {
    svg.split("pele-shape-block-arrow")
        .skip(1)
        .filter_map(|rest| {
            rest.split_once("<polygon points=\"").map(|(_, p)| p[..p.find('"').unwrap_or(0)].split(' ').count())
        })
        .collect()
}

/// The `d` of each edge's line.
fn edge_paths(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-edge\"")
        .skip(1)
        .filter_map(|rest| rest.split_once("<path d=\"").map(|(_, d)| &d[..d.find('"').unwrap_or(0)]))
        .collect()
}

/// For a path of one straight piece, whether it runs level or upright.
fn straight(d: &str) -> Option<bool> {
    let rest = d.strip_prefix('M')?;
    let (from, to) = rest.split_once('L')?;
    if to.contains(['L', 'Q', 'A', 'M']) {
        return None;
    }
    let pair = |text: &str| text.split_once(',').map(|(x, y)| (x.to_owned(), y.to_owned()));
    let ((x1, y1), (x2, y2)) = (pair(from)?, pair(to)?);
    Some(x1 == x2 || y1 == y2)
}

/// Whether one composite's box lies inside another's.
fn nested_clusters(svg: &str) -> bool {
    let attribute = |tag: &str, name: &str| -> Option<f64> {
        let (_, rest) = tag.split_once(&format!(" {name}=\""))?;
        rest[..rest.find('"')?].parse().ok()
    };
    let boxes: Vec<[f64; 4]> = svg
        .split("<g class=\"pele-cluster")
        .skip(1)
        .filter_map(|rest| {
            let (_, rect) = rest.split_once("<rect")?;
            let tag = &rect[..rect.find('>')?];
            Some([attribute(tag, "x")?, attribute(tag, "y")?, attribute(tag, "width")?, attribute(tag, "height")?])
        })
        .collect();
    boxes.iter().any(|a| {
        boxes
            .iter()
            .any(|b| a != b && b[0] > a[0] && b[1] > a[1] && b[0] + b[2] < a[0] + a[2] && b[1] + b[3] < a[1] + a[3])
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
                    *if r#type == "block" { &mut self.identical } else { &mut self.other_types } += 1;
                    if r#type == "block" {
                        self.reached
                            .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    }
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "block" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "block" => {
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
                Some(format!("pele rejects a block diagram, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {:?}", &source[..source.len().min(600)]));
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

/// The widths a host may have.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

/// How many of the cases Pele draws another way for the width they give than with room to
/// spare, which for a block diagram is the tight attempt.
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested block sources");
    assert!(tally.identical > 1000, "the harvest holds block diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds block diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested block sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated block sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let crowded = "block\n A space:3 B\n A -- \"label one\" --> B\n A -- \"label two\" --> B\n A -- \"three\" --> B\n B -- \"back again\" --> A";
    let column = "block\n columns 1\n A\n space\n B\n A -- \"x\" --> B\n A -- \"y\" --> B\n B -- \"z\" --> A";
    let nested = "block\n columns 2\n block:outer[\"Outer\"]:2\n  columns 2\n  block:inner[\"Inner\"]\n   a b\n  end\n  c\n end\n d e\n a --> d\n inner --> e\n c --> outer";
    let filled = "block\n block:g\n  a b\n end\n c\n a --> b\n b --> c\n style g fill:#036\n style a fill:#ffd\n classDef k fill:navy,color:white\n class b k";
    let loops = "block\n a((\"round\")) b{\"diamond\"} c\n a --> a\n a --> a\n b --> b\n c --> c\n c -- \"self\" --> c\n c --> c\n c --> c";
    let spans = "block\n columns 4\n a[\"A rather long label that wraps over lines\"]:3 b\n c:2 d<[\"An arrow with a long label too\"]>(right):2\n e((\"wide circle that wraps\")):4";
    let arrows = "block\n columns 4\n a<[\"r\"]>(right) b<[\"l\"]>(left) c<[\"u\"]>(up) d<[\"d\"]>(down)\n e<[\"x\"]>(x) f<[\"y\"]>(y) g<[\"xd\"]>(x, down) h<[\"all\"]>(x, y)\n i<[\" \"]>(up, right) j<[\"none\"]>(sideways) k<[\"&nbsp;\"]>(left, down)";
    let titled = "---\ntitle: A title much wider than the diagram under it, surely\n---\nblock\n a b";
    let mut out = vec![
        (crowded.to_owned(), json!({})),
        (crowded.to_owned(), json!({ "maxWidth": 200 })),
        (crowded.to_owned(), json!({ "maxWidth": 400 })),
        (column.to_owned(), json!({})),
        (column.to_owned(), json!({ "maxWidth": 100 })),
        (nested.to_owned(), json!({})),
        (nested.to_owned(), json!({ "maxWidth": 200 })),
        (filled.to_owned(), json!({})),
        (loops.to_owned(), json!({})),
        (spans.to_owned(), json!({})),
        (spans.to_owned(), json!({ "maxWidth": 300 })),
        (arrows.to_owned(), json!({})),
        (arrows.to_owned(), json!({ "fontSize": 12 })),
        (titled.to_owned(), json!({ "padding": 0 })),
        (titled.to_owned(), json!({ "maxWidth": 200 })),
        (titled.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        ("block\n a".to_owned(), json!({ "outputLimit": 200 })),
        ("block\n a".to_owned(), json!({ "limit": 5 })),
    ];
    for config in [
        json!({"block": {"padding": 0}}),
        json!({"block": {"padding": 40}}),
        json!({"block": {"padding": 1e308}}),
        json!({"block": {"padding": -1}}),
        json!({"block": {"padding": "8"}}),
        json!({"block": "spaced"}),
        json!({"block": {"useMaxWidth": false}}),
    ] {
        out.push((nested.to_owned(), json!({ "config": config.clone() })));
        out.push((crowded.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("block cases");
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
    "a",
    "Label",
    "A longer label",
    "A label long enough to wrap onto a second line, surely",
    "**b**",
    "x<br>y",
    "&nbsp;",
];
const SHAPES: &[(&str, &str)] = &[
    ("[\"", "\"]"),
    ("(\"", "\")"),
    ("((\"", "\"))"),
    ("(((\"", "\")))"),
    (">\"", "\"]"),
    ("{\"", "\"}"),
    ("{{\"", "\"}}"),
    ("([\"", "\"])"),
    ("[[\"", "\"]]"),
    ("[(\"", "\")]"),
    ("[/\"", "\"/]"),
    ("[\\\"", "\"\\]"),
    ("[/\"", "\"\\]"),
    ("[\\\"", "\"/]"),
];
const DIRECTIONS: &[&str] =
    &["right", "left", "up", "down", "x", "y", "x, down", "up, right", "left, y", "x, y", "sideways", " up "];
const LINKS: &[&str] = &[
    " --> ",
    " --- ",
    " --x ",
    " --o ",
    " <--> ",
    " x--x ",
    " o--o ",
    " ==> ",
    " === ",
    " -.-> ",
    " -.- ",
    " -- \"label\" --> ",
    " == \"thick\" ==> ",
    " -. \"dotted\" .-> ",
    " -- \"a longer edge label\" --> ",
];
const STYLES: &[&str] = &[
    "fill:#f96",
    "fill:#036",
    "fill:navy,color:white",
    "stroke:#333,stroke-width:4px",
    "color:red",
    "fill:#bbf,stroke:#f66,stroke-width:2px,color:#fff,stroke-dasharray: 5 5",
    "font-weight:bold",
    "font-style:italic,font-size:20px",
    "fill:#000,fill-opacity:0",
    "fill:none",
];

struct Diagram {
    rng: Rng,
    /// The ids declared so far, which edges and styles name.
    ids: Vec<String>,
    composites: Vec<String>,
}

impl Diagram {
    fn id(&mut self) -> String {
        let id = format!("n{}", self.ids.len());
        self.ids.push(id.clone());
        id
    }

    fn known(&mut self) -> String {
        if self.ids.is_empty() {
            return "n0".to_owned();
        }
        let all: Vec<&String> = self.ids.iter().chain(&self.composites).collect();
        all[self.rng.below(all.len())].clone()
    }

    fn node(&mut self) -> String {
        let id = self.id();
        let mut out = id;
        match self.rng.below(6) {
            0 => {}
            1 => {
                out += &format!("<[\"{}\"]>({})", self.rng.pick(LABELS), self.rng.pick(DIRECTIONS));
            }
            _ => {
                let (open, close) = SHAPES[self.rng.below(SHAPES.len())];
                out += &format!("{open}{}{close}", self.rng.pick(LABELS));
            }
        }
        if self.rng.chance(5) {
            out += self.rng.pick(&[":2", ":3", ":4"]);
        }
        out
    }

    fn statement(&mut self, depth: usize) -> String {
        match self.rng.below(12) {
            0..=4 => (0..1 + self.rng.below(4)).map(|_| self.node()).collect::<Vec<_>>().join(" "),
            5 if depth < 3 => {
                let id = format!("g{}", self.composites.len());
                self.composites.push(id.clone());
                let header = match self.rng.below(3) {
                    0 => "block".to_owned(),
                    1 => format!("block:{id}"),
                    _ => format!("block:{id}[\"{}\"]{}", self.rng.pick(LABELS), self.rng.pick(&["", ":2", ":3"])),
                };
                let mut out = header + "\n";
                if self.rng.chance(2) {
                    out += &format!("columns {}\n", self.rng.pick(&["1", "2", "3", "auto"]));
                }
                for _ in 0..1 + self.rng.below(4) {
                    out += &self.statement(depth + 1);
                    out += "\n";
                }
                out + "end"
            }
            6 => self.rng.pick(&["space", "space:2", "space:3"]).to_owned(),
            7 | 8 => {
                let (a, b) = (self.known(), if self.rng.chance(6) { "self".to_owned() } else { self.known() });
                let b = if b == "self" { a.clone() } else { b };
                format!("{a}{}{b}", self.rng.pick(LINKS))
            }
            9 => match self.rng.below(3) {
                0 => format!("classDef {} {}", self.rng.pick(&["k", "m", "default"]), self.rng.pick(STYLES)),
                1 => format!("class {} {}", self.known(), self.rng.pick(&["k", "m"])),
                _ => format!("style {} {}", self.known(), self.rng.pick(STYLES)),
            },
            _ => self.node(),
        }
    }
}

/// A block diagram that Pele nearly always renders, with host options, to put many through the
/// shapes, the arrows, the nesting, the spans, the routes and the labels kept apart.
fn random_block(rng: Rng) -> (String, Value, Rng) {
    let mut diagram = Diagram { rng, ids: Vec::new(), composites: Vec::new() };
    let mut source = String::new();
    let title = diagram.rng.chance(6);
    let padding = diagram.rng.chance(8);
    if title || padding {
        source += "---\n";
        if title {
            source +=
                &format!("title: {}\n", diagram.rng.pick(&["A title", "A much longer title than the diagram is wide"]));
        }
        if padding {
            source += &format!("config:\n  block:\n    padding: {}\n", diagram.rng.below(30));
        }
        source += "---\n";
    }
    source += "block\n";
    if diagram.rng.chance(2) {
        source += &format!("columns {}\n", 1 + diagram.rng.below(5));
    }
    let many = diagram.rng.chance(5);
    for _ in 0..1 + diagram.rng.below(if many { 20 } else { 8 }) {
        source += &diagram.statement(0);
        source += "\n";
    }
    let mut rng = diagram.rng;
    let mut options = serde_json::Map::new();
    if rng.below(3) > 0 {
        let width = match rng.below(5) {
            0 => 100.0 + rng.below(900) as f64 + 0.5,
            width => WIDTHS[width - 1],
        };
        options.insert("maxWidth".to_owned(), width.into());
    }
    if rng.chance(8) {
        options.insert("padding".to_owned(), (rng.below(41) as f64 / 2.0).into());
    }
    if rng.chance(8) {
        options.insert("fontSize".to_owned(), [12.0, 13.0, 14.0, 18.5, 20.0][rng.below(5)].into());
    }
    (source, Value::Object(options), rng)
}

#[test]
fn random_block_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut cases = Vec::new();
    for _ in 0..4000 {
        let (source, options, next) = random_block(rng);
        rng = next;
        cases.push((source, options));
    }
    let tally = run("random block diagram", &cases).finish("random block diagrams");
    assert!(tally.identical > 3000, "nearly every random block diagram renders");
    println!("the random block diagrams reach {} of {} named drawing paths", tally.reached.len(), PATHS.len());
    tally.reaches_every_path("random block diagrams");
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random block diagrams another way");
    assert!(redrawn > 100, "the widths put many random block diagrams through the tight attempt");
}

/// The sources of Pele's `tests/block/stress.test.ts`, built as that test builds them, with the
/// source limit lifted as that test lifts it.
fn worst_cases() -> Vec<String> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let n = 50_000;
    let mut complete = String::from("block\ncolumns 9\n");
    for i in 0..70 {
        for j in i + 1..70 {
            complete += &format!("n{i}-->n{j}\n");
        }
    }
    let nines = "9".repeat(400);
    vec![
        format!("block\n{}", repeat(16000, &|i| format!("b{i} "))),
        format!("block\ncolumns 1\n{}", repeat(10000, &|i| format!("b{i}\n"))),
        format!("block\ncolumns 100\n{}", repeat(8000, &|i| format!("b{i}[\"x\"] "))),
        format!("block\n{}a\n{}", "block\n".repeat(8000), "end\n".repeat(8000)),
        format!("block\n{}a\n{}", repeat(5000, &|i| format!("block:g{i}\n")), "end\n".repeat(5000)),
        format!("block\n{}a", "block\n".repeat(12000)),
        format!("block\na\n{}", "end\n".repeat(12000)),
        format!("block\ncolumns 50\n{}", repeat(5000, &|i| format!("block\nn{i}\nend\n"))),
        format!("block\na0{}", repeat(8000, &|i| format!("-->a{}", i + 1))),
        format!(
            "block\ncolumns 20\n{}\n{}",
            repeat(400, &|i| format!("n{i} ")),
            repeat(4000, &|i| format!("n{} --> n{}\n", i % 400, (i * 37 + 11) % 400))
        ),
        complete,
        format!("block\nA space B\n{}{}", "A-->B\n".repeat(4000), "A-->A\n".repeat(4000)),
        format!("block\nA space:3 B\n{}", "A -- \"label text\" --> B\n".repeat(2500)),
        format!("block\n{}", "space:99999 ".repeat(4000)),
        format!("block\na space:{nines} b"),
        "block\na space:99999 b".to_owned(),
        format!("block\ncolumns 3\na:{nines} b c"),
        format!("block\ncolumns {nines}\na b"),
        format!("block\n{}", repeat(5000, &|i| format!("b{i}:999 "))),
        format!("block\nA[\"{}\"]", "word ".repeat(n / 5)),
        format!("block\nA[\"{}\"]", "#35;&amp;<br>".repeat(n / 13)),
        format!("block\nA[\"{}", "x".repeat(n)),
        format!("block\nA[\"`{}", "x`".repeat(n / 2)),
        format!("block\nA<[\"{}\"]>", "x".repeat(n)),
        format!("block\nA<[\"x\"]>({})", "up, down , x,y ".repeat(2500)),
        format!("block\nA<[\"x\"]>({}sideways)", " ".repeat(n)),
        format!("block\nA<[\"x\"]>{}x", " ".repeat(n)),
        format!("block\nA\nclass {}A hot\nclassDef hot fill:#f96", "A,".repeat(25000)),
        format!("block\nA\nclass A{} x", format!(",{}", " ".repeat(40)).repeat(1200)),
        format!("block\nA\nstyle A {}", ",".repeat(n)),
        format!("block\nA\nstyle A {}", "fill:red;".repeat(n / 9)),
        format!("block\nA\nclassDef c {}\nclass A c", "x".repeat(n)),
        format!("block\nA\nclassDef c {}x\nclass A c", "fill:red,".repeat(10000)),
        format!("block\nA\n{}", repeat(2500, &|i| format!("classDef c{i} fill:red\nclass A c{i}\n"))),
        format!("block\nclassDef {}{}", "a".repeat(n / 2), " ".repeat(n / 2)),
        format!("block\n{}", "--> ".repeat(n / 4)),
        format!("block\n{}", "a".repeat(n)),
        format!("block {}", "a;".repeat(n / 2)),
        "block\n__proto__ constructor toString\nclassDef __proto__ fill:red\nclass constructor,toString __proto__\nstyle __proto__ fill:blue\n__proto__ --> constructor".to_owned(),
        "block\na space:100001 b".to_owned(),
        format!("block\n{}", "space:60000 ".repeat(2)),
        "block\na space:5 b".to_owned(),
        "block\na:0 b:00 c".to_owned(),
        format!("block\na:{nines} b c"),
        "block\ncolumns 99999999999999999999\na b".to_owned(),
        "block\nblock:g:99999999\na\nend\nb".to_owned(),
    ]
}

#[test]
fn worst_cases_match_pele() {
    let options = json!({ "limit": 1_000_000_000u64 });
    let cases: Vec<(String, Value)> = worst_cases().into_iter().map(|source| (source, options.clone())).collect();
    run("worst case", &cases).finish("Pele's worst-case block diagrams");
}

/// Self-loops on a block in a filled composite, with the composite's padding swept in quarter
/// pixels, so that its border passes through each loop's arc. Whether an edge is cased or inked
/// then turns on the points Pele samples along the arc.
#[test]
fn self_loops_against_a_fill_border_match_pele() {
    let mut cases = Vec::new();
    for loops in 1..=4 {
        let source = format!(
            "block\n block:g\n  a[\"A block\"] b((\"round\")) c{{\"diamond\"}}\n end\n{}style g fill:#036",
            ["a", "b", "c"].iter().map(|id| format!(" {id} --> {id}\n").repeat(loops)).collect::<String>()
        );
        for quarter in 0..120 {
            let padding = f64::from(quarter) / 4.0;
            cases.push((source.clone(), json!({ "config": { "block": { "padding": padding } } })));
        }
    }
    let tally = run("self-loop", &cases).finish("self-loops against a fill's border");
    assert!(tally.reached.contains("an edge cased over a fill's border"), "some loops cross the fill's border");
    assert!(tally.reached.contains("an edge in a fill's ink"), "some loops stay inside the fill");
}
