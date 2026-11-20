//! Differential test for drawing quadrant charts: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/quadrant/harvest.json` holds the quadrant sources of Pele's own test suite and
//! the inputs of its mutated runs, written by `scripts/harvest-quadrant.mjs`. A chart is drawn to
//! the host's width where that is less than its own, so the harvest is drawn at four widths too.
//! The random charts carry the proof for what the three corpus sources never draw, and must reach
//! every drawing path in [`PATHS`].

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/quadrant/harvest.json");
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
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

type Reached = fn(&str, &str) -> bool;

/// The part of the drawing inside the layer that opens with `start`, up to the next layer.
fn layer<'a>(svg: &'a str, start: &str) -> &'a str {
    let rest = svg.split_once(start).map_or("", |(_, rest)| rest);
    let next = [
        "<g class=\"pele-quadrants\"",
        "<g class=\"pele-axis-labels\"",
        "<g class=\"pele-points\"",
        "<g class=\"pele-point-labels\"",
    ]
    .iter()
    .filter_map(|layer| rest.find(layer))
    .min()
    .unwrap_or(rest.len());
    &rest[..next]
}

fn attribute(tag: &str, name: &str) -> Option<f64> {
    let (_, rest) = tag.split_once(&format!(" {name}=\""))?;
    rest[..rest.find('"')?].parse().ok()
}

/// The points' circles as cx, cy and r, in the order the labels are written.
fn points(svg: &str) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = layer(svg, "<g class=\"pele-points\"")
        .split("<circle ")
        .skip(1)
        .filter_map(|rest| {
            let tag = &rest[..rest.find('>')?];
            Some([attribute(tag, "cx")?, attribute(tag, "cy")?, attribute(tag, "r")?])
        })
        .collect();
    out.reverse();
    out
}

/// The point labels as the x and the baseline y of their first line.
fn point_labels(svg: &str) -> Vec<[f64; 2]> {
    layer(svg, "<g class=\"pele-point-labels\"")
        .split("<text ")
        .skip(1)
        .filter_map(|rest| {
            let text = &rest[..rest.find("</text>")?];
            Some([attribute(text, "x")?, attribute(text, "y")?])
        })
        .collect()
}

/// Where each point's label is against its point: below, above, right or left of it, or moved
/// along its side.
fn placements(svg: &str) -> Vec<&'static str> {
    let (dots, labels) = (points(svg), point_labels(svg));
    if dots.len() != labels.len() {
        return Vec::new();
    }
    dots.iter()
        .zip(&labels)
        .map(|([cx, cy, r], [x, y])| {
            if (y - cy).abs() < 6.0 && x > cx {
                "right"
            } else if (y - cy).abs() < 6.0 && x < cx {
                "left"
            } else if *y > cy + r {
                if (x - cx).abs() > 0.02 { "below, moved along" } else { "below" }
            } else if (x - cx).abs() > 0.02 {
                "above, moved along"
            } else {
                "above"
            }
        })
        .collect()
}

fn quadrant_labels(svg: &str) -> Vec<&str> {
    layer(svg, "<g class=\"pele-quadrants\"").split("<g class=\"pele-quadrant\"").skip(1).collect()
}

/// The drawing paths a source can go through, each with what in the source and Pele's SVG shows
/// it did.
const PATHS: &[(&str, Reached)] = &[
    ("four quadrant labels", |_, svg| quadrant_labels(svg).iter().filter(|q| q.contains("<text")).count() == 4),
    ("an empty quadrant", |_, svg| quadrant_labels(svg).iter().any(|q| !q.contains("<text"))),
    ("a quadrant label in two lines", |_, svg| quadrant_labels(svg).iter().any(|q| q.matches("<tspan").count() > 1)),
    ("a markdown quadrant label", |source, svg| {
        source.contains("quadrant-")
            && source.contains("\"`")
            && quadrant_labels(svg).iter().any(|q| q.contains("font-weight") || q.contains("font-style"))
    }),
    ("a quadrant label in its own size", |_, svg| {
        quadrant_labels(svg).iter().any(|q| q.contains("class=\"pele-label\" font-size="))
    }),
    ("x axis labels at both ends", |_, svg| {
        let axes = layer(svg, "<g class=\"pele-axis-labels\"");
        axes.split("<g transform").next().unwrap_or("").matches("<text").count() == 2
    }),
    ("an x axis label at one end", |_, svg| {
        let axes = layer(svg, "<g class=\"pele-axis-labels\"");
        !axes.is_empty() && axes.split("<g transform").next().unwrap_or("").matches("<text").count() == 1
    }),
    ("an x axis above the plot", |_, svg| {
        let axes = layer(svg, "<g class=\"pele-axis-labels\"");
        let first =
            layer(svg, "<g class=\"pele-quadrants\"").split("<rect").nth(1).and_then(|rect| attribute(rect, "y"));
        let label =
            axes.split("<g transform").next().and_then(|x| x.split_once("<text").map(|(_, text)| text.to_owned()));
        matches!((first, label), (Some(top), Some(text)) if attribute(&text, "y").is_some_and(|y| y < top))
    }),
    ("an x axis below the plot", |_, svg| {
        let axes = layer(svg, "<g class=\"pele-axis-labels\"");
        let first =
            layer(svg, "<g class=\"pele-quadrants\"").split("<rect").nth(1).and_then(|rect| attribute(rect, "y"));
        let label =
            axes.split("<g transform").next().and_then(|x| x.split_once("<text").map(|(_, text)| text.to_owned()));
        matches!((first, label), (Some(top), Some(text)) if attribute(&text, "y").is_some_and(|y| y > top + 50.0))
    }),
    ("y axis labels at both ends", |_, svg| {
        layer(svg, "<g class=\"pele-axis-labels\"").matches("rotate(-90)").count() == 2
    }),
    ("a y axis label at one end", |_, svg| {
        layer(svg, "<g class=\"pele-axis-labels\"").matches("rotate(-90)").count() == 1
    }),
    ("a y axis on the right", |source, svg| source.contains("yAxisPosition: right") && svg.contains("rotate(-90)")),
    ("a y axis label in its own size", |_, svg| svg.contains("rotate(-90)\" font-size=")),
    ("an axis arrow with nothing after it", |_, svg| layer(svg, "<g class=\"pele-axis-labels\"").contains('\u{27f6}')),
    ("points", |_, svg| svg.contains("<g class=\"pele-points\"")),
    ("a point with a class", |_, svg| svg.contains("<circle class=\"pele-point ")),
    ("a point with a radius of its own", |_, svg| points(svg).iter().any(|[_, _, r]| *r != 5.0)),
    ("a point with a fill", |_, svg| layer(svg, "<g class=\"pele-points\"").contains("style=\"fill:")),
    ("a point with a stroke colour", |_, svg| layer(svg, "<g class=\"pele-points\"").contains("stroke:#")),
    ("a point stroked in the series colour", |_, svg| {
        layer(svg, "<g class=\"pele-points\"")
            .split("<circle")
            .skip(1)
            .any(|c| c.contains(" stroke=\"var(--pele-series-1"))
    }),
    ("a point left out", |source, svg| {
        source.matches(": [").count() > svg.matches("<circle").count() && svg.contains("<g class=\"pele-points\"")
    }),
    ("a label below its point", |_, svg| placements(svg).contains(&"below")),
    ("a label above its point", |_, svg| placements(svg).contains(&"above")),
    ("a label right of its point", |_, svg| placements(svg).contains(&"right")),
    ("a label left of its point", |_, svg| placements(svg).contains(&"left")),
    ("a label kept inside the plot", |_, svg| placements(svg).iter().any(|p| p.ends_with("moved along"))),
    ("a markdown point label", |_, svg| layer(svg, "<g class=\"pele-point-labels\"").contains("font-weight")),
    ("a point label wrapped to the plot", |_, svg| layer(svg, "<g class=\"pele-point-labels\"").contains("<tspan")),
    ("a point with nothing to show", |_, svg| points(svg).len() > point_labels(svg).len()),
    ("more points than are placed with care", |_, svg| svg.matches("<circle").count() > 300),
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("a title in its own size", |_, svg| svg.contains("class=\"pele-title\" font-size=")),
    ("an accessible title and description", |_, svg| {
        svg.contains("aria-labelledby=") && svg.contains("aria-describedby=")
    }),
    ("a chart narrowed to the host", |source, svg| {
        !source.contains("chartWidth") && svg.contains("width=\"") && {
            let width = svg.split_once(" width=\"").and_then(|(_, rest)| rest[..rest.find('"')?].parse::<f64>().ok());
            width.is_some_and(|width| width < 500.0)
        }
    }),
    ("a chart of its own size", |source, svg| source.contains("chartWidth") && !svg.contains("width=\"500\"")),
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
                    *if r#type == "quadrantChart" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "quadrantChart" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "quadrantChart" => {
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
                Some(format!("pele rejects a quadrant chart, ours reports it as not ported: {error}"))
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
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested quadrant sources");
    assert!(tally.identical > 500, "the harvest holds quadrant charts Pele renders");
    assert!(tally.rejected > 100, "the harvest holds quadrant charts Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested quadrant sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated quadrant sources");
}

/// A source with the `quadrantChart` config given in its front matter.
fn configured(body: &str, source: &str) -> String {
    format!("---\nconfig:\n  quadrantChart:\n{body}\n---\n{source}")
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it: every setting of the `quadrantChart` config, in range and out, and host widths,
/// font sizes and padding.
fn cases() -> Vec<(String, Value)> {
    let chart = "quadrantChart\n  title Reach and engagement\n  accTitle: Reach\n  accDescr: Campaigns by reach\n  x-axis Low Reach --> High Reach\n  y-axis Low Engagement --> High Engagement\n  quadrant-1 Expand\n  quadrant-2 Promote\n  quadrant-3 Re-evaluate\n  quadrant-4 \"`**Improve** _now_`\"\n  Campaign A:::hot: [0.25, 0.75]\n  Campaign B: [0.75, 0.25] radius: 12, color: #ff3300\n  Campaign C: [0, 0] stroke-width: 3px\n  Campaign D: [1, 1] stroke-color: 0f0, stroke-width: 9px\n  Campaign E: [0.5, 0.5]\n  classDef hot color: #c00, radius: 8";
    let settings = [
        "    chartWidth: 400\n    chartHeight: 320",
        "    chartWidth: 90\n    chartHeight: 5000",
        "    chartWidth: \"wide\"\n    pointRadius: -3",
        "    chartWidth: 4001",
        "    titleFontSize: 30\n    quadrantLabelFontSize: 9.5\n    pointLabelFontSize: 2",
        "    xAxisLabelFontSize: 20\n    yAxisLabelFontSize: 80",
        "    xAxisLabelFontSize: 11\n    yAxisLabelFontSize: 11",
        "    quadrantPadding: 30\n    xAxisLabelPadding: 0\n    yAxisLabelPadding: 40",
        "    quadrantPadding: 250\n    titlePadding: 0",
        "    pointRadius: 9\n    pointTextPadding: 30",
        "    quadrantTextTopPadding: 40",
        "    xAxisPosition: bottom\n    yAxisPosition: right",
        "    xAxisPosition: top\n    yAxisPosition: left",
        "    titleFontSize: .inf\n    pointRadius: .nan",
    ];
    let mut out = vec![(chart.to_owned(), json!({}))];
    for body in settings {
        out.push((configured(body, chart), json!({})));
        out.push((
            configured(body, "quadrantChart\n  x-axis Left\n  y-axis Down --> Up\n  quadrant-2 Only"),
            json!({}),
        ));
    }
    for options in [
        json!({ "maxWidth": 300 }),
        json!({ "maxWidth": 200 }),
        json!({ "maxWidth": 0 }),
        json!({ "maxWidth": 1000 }),
        json!({ "fontSize": 12 }),
        json!({ "fontSize": 22, "padding": 0 }),
        json!({ "padding": 30, "maxWidth": 420 }),
        json!({ "idPrefix": "x\"<y", "responsive": false }),
        json!({ "config": { "quadrantChart": { "chartWidth": 600, "useMaxWidth": false } } }),
        json!({ "config": { "quadrantChart": { "pointRadius": 2, "pointTextPadding": 1 } } }),
    ] {
        out.push((chart.to_owned(), options));
    }
    let crowd: String =
        (0..40).map(|i| format!("  P{i}: [0.{:02}, 0.{:02}]\n", (i * 37) % 100, (i * 53) % 100)).collect();
    out.push((format!("quadrantChart\n  quadrant-1 A long quadrant title that wraps\n{crowd}"), json!({})));
    out.push((format!("quadrantChart\n{crowd}"), json!({ "maxWidth": 300 })));
    let corner = "quadrantChart\n  x-axis L --> R\n  A rather long label for a corner point: [0, 0]\n  Another long label at the far corner: [1, 1]\n  Top edge label: [0.5, 1]\n  Bottom edge label: [0.5, 0]\n  Left: [0, 0.5]\n  Right: [1, 0.5]";
    out.push((corner.to_owned(), json!({})));
    out.push((corner.to_owned(), json!({ "maxWidth": 280 })));
    out
}

#[test]
fn hand_written_cases_match_pele() {
    run("case", &cases()).finish("quadrant cases");
}

/// The shapes of Pele's worst quadrant cases (`tests/quadrant/stress.test.ts`) at sizes the
/// default limits allow.
fn worst() -> Vec<String> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let q = "quadrantChart\n";
    vec![
        format!("{q}{}!", " ".repeat(40000)),
        format!("{q}A{}:x", "\t".repeat(40000)),
        format!("{q}{}", format!("{}:", "\t".repeat(500)).repeat(80)),
        format!("{q}{}", ": ".repeat(20000)),
        format!("{q}x-axis a {}", "-".repeat(40000)),
        format!("{q}x-axis a{}", format!("  {}", "-".repeat(50)).repeat(800)),
        format!("quadrantChart{}", "\n%% c".repeat(8000)),
        format!("{q}{}", "a%%".repeat(13000)),
        format!("{q}{}", format!("accTitle{}", " ".repeat(40)).repeat(1000)),
        format!("{q}accDescr {{{}", " a\n".repeat(13000)),
        format!("{q}quadrant-1 \"{}", "a".repeat(40000)),
        format!("{q}quadrant-1 \"`{}", "a ".repeat(20000)),
        format!("{q}A: [0.{}, 1]", "5".repeat(40000)),
        format!("{q}A: [0{}1]", " ".repeat(40000)),
        format!("{q}A:::c: [0.5, 0.5]\nclassDef c {}radius: 6", "radius: 5,".repeat(4000)),
        format!("{q}A: [0.5, 0.5] {}radius: 6", "color: #abc ,".repeat(3000)),
        format!("{q}{}", repeat(3000, &|i| format!("P{i}: [0.{}, 0.{}]\n", i % 100, (i * 7) % 100))),
        format!("{q}{}", repeat(300, &|i| format!("Point {i}: [0.5, 0.5]\n"))),
        format!("{q}{}A:::c7: [0.5, 0.5]", repeat(2000, &|i| format!("classDef c{i} radius: {i}\n"))),
        format!("{q}{}: [0.5, 0.5]", "word ".repeat(9000)),
        format!("{q}quadrant-1 \"`{}`\"", "*a _b ".repeat(6000)),
        format!("{q}x-axis \"{}\"", "#35;&amp;<br>".repeat(3500)),
        format!("{q}title {}", "long title ".repeat(4000)),
        format!("{q}A:::{}: [0.5, 0.5]", "c".repeat(40000)),
        format!("quadrantChart;{}", "quadrant-1 a;".repeat(3500)),
        format!("{q}{}", format!("{}quadrant-2 b\n", " ".repeat(100)).repeat(400)),
        format!("{q}A: [0.5, 0.5] radius: {}", "9".repeat(400)),
        format!("{q}  A: [0.5, 0.5] radius: {}, stroke-width: {}px", "9".repeat(400), "9".repeat(400)),
    ]
}

#[test]
fn worst_cases_match_pele() {
    run("worst", &plain(worst())).finish("Pele's worst-case quadrant charts");
}

/// A small seeded generator, so a failure names an input that can be made again.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, list: &[&'a str]) -> &'a str {
        list[self.below(list.len())]
    }
}

const TEXTS: &[&str] = &[
    "Low",
    "High reach",
    "\"Quoted\"",
    "\"`**Bold** and _it_`\"",
    "Re-evaluate",
    "x_y.z",
    "Ünïcode",
    "日本",
    "A label long enough that the quadrant has to wrap it",
    "50% & more",
    "\" \"",
];

/// A coordinate as a source writes it: often near an edge, sometimes out of range.
fn coordinate(rng: &mut Rng) -> String {
    match rng.below(10) {
        0 => "0".to_owned(),
        1 => "1".to_owned(),
        2 => format!("0.0{}", rng.below(10)),
        3 => format!("0.9{}", rng.below(10)),
        4 => ["0x5", "0e3", "0b1", "0.5", "0X1"][rng.below(5)].to_owned(),
        _ => format!("0.{}", 1 + rng.below(999)),
    }
}

/// A random quadrant chart with its labels, points, classes, config and host width.
fn random_chart(rng: &mut Rng) -> (String, Value) {
    let mut src = String::new();
    if rng.below(3) == 0 {
        let mut body = Vec::new();
        for (key, values) in [
            ("chartWidth", &["300", "400", "700", "120"][..]),
            ("chartHeight", &["300", "420", "800"][..]),
            ("pointRadius", &["2", "9", "15"][..]),
            ("pointTextPadding", &["0", "12"][..]),
            ("quadrantPadding", &["0", "20"][..]),
            ("titleFontSize", &["24", "10"][..]),
            ("quadrantLabelFontSize", &["12", "20"][..]),
            ("xAxisLabelFontSize", &["10", "18"][..]),
            ("yAxisLabelFontSize", &["10", "18"][..]),
            ("pointLabelFontSize", &["9", "16"][..]),
            ("xAxisPosition", &["bottom", "top"][..]),
            ("yAxisPosition", &["right", "left"][..]),
            ("quadrantTextTopPadding", &["0", "20"][..]),
        ] {
            if rng.below(4) == 0 {
                body.push(format!("    {key}: {}", rng.pick(values)));
            }
        }
        if !body.is_empty() {
            src.push_str(&format!("---\nconfig:\n  quadrantChart:\n{}\n---\n", body.join("\n")));
        }
    }
    src.push_str("quadrantChart\n");
    if rng.below(3) == 0 {
        src.push_str(&format!("  title {}\n", rng.pick(TEXTS)));
    }
    if rng.below(6) == 0 {
        src.push_str("  accTitle: Chart\n  accDescr: A chart of things\n");
    }
    let axis = |rng: &mut Rng| match rng.below(4) {
        0 => rng.pick(TEXTS).to_owned(),
        1 => format!("{} -->", rng.pick(TEXTS)),
        _ => format!("{} --> {}", rng.pick(TEXTS), rng.pick(TEXTS)),
    };
    if rng.below(3) > 0 {
        src.push_str(&format!("  x-axis {}\n", axis(rng)));
    }
    if rng.below(3) > 0 {
        src.push_str(&format!("  y-axis {}\n", axis(rng)));
    }
    for k in 1..=4 {
        if rng.below(4) > 0 {
            src.push_str(&format!("  quadrant-{k} {}\n", rng.pick(TEXTS)));
        }
    }
    let count = match rng.below(20) {
        0..=3 => 0,
        4..=17 => 1 + rng.below(12),
        18 => 20 + rng.below(30),
        _ => 301 + rng.below(20),
    };
    let classes = ["hot", "cold", "big"];
    for i in 0..count {
        let name = if rng.below(4) == 0 { rng.pick(TEXTS).to_owned() } else { format!("Point {i}") };
        let class = if rng.below(4) == 0 { format!(":::{}", classes[rng.below(3)]) } else { String::new() };
        let mut styles = Vec::new();
        for style in
            ["radius: 3", "radius: 14", "color: #ff3300", "color: 0a0", "stroke-color: #222", "stroke-width: 4px"]
        {
            if rng.below(8) == 0 {
                styles.push(style);
            }
        }
        let styles = if styles.is_empty() { String::new() } else { format!(" {}", styles.join(", ")) };
        src.push_str(&format!("  {name}{class}: [{}, {}]{styles}\n", coordinate(rng), coordinate(rng)));
    }
    for class in classes {
        if rng.below(3) == 0 {
            src.push_str(&format!(
                "  classDef {class} {}\n",
                rng.pick(&["radius: 10", "color: #0000ff", "stroke-width: 2px, stroke-color: #f00"])
            ));
        }
    }
    let options = match rng.below(4) {
        0 => {
            let width = [280.0, 320.0, 450.0, 640.0][rng.below(4)];
            json!({ "maxWidth": width })
        }
        _ => json!({}),
    };
    (src, options)
}

#[test]
fn random_charts_match_pele() {
    let mut rng = Rng(0x0a0d_c4a7_5eed_0001);
    let cases: Vec<(String, Value)> = (0..3000).map(|_| random_chart(&mut rng)).collect();
    let tally = run("random", &cases).finish("random quadrant charts");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random quadrant chart reaches: {}", missed.join(", "));
}
