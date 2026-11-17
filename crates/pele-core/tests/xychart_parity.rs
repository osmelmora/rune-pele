//! Differential test for drawing XY charts: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele rejects
//! must be rejected here with Pele's message.
//!
//! `tests/fixtures/xychart/harvest.json` holds the XY chart sources of Pele's own test suite, its
//! worst cases and the inputs of its seeded mutation runs, written by
//! `scripts/harvest-xychart.mjs`. A chart is drawn to its own width or the host's, whichever is
//! smaller, so the widths draw every chart wider than the host again.

mod xychart_random;

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use xychart_random::Rng;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/xychart/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list]
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().or_else(|| item["source"].as_str()).expect("a source").to_owned())
        .collect()
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

/// The drawing paths a source can go through, each with what in the source and in Pele's SVG
/// shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("an accessible title and description", |_, svg| svg.contains("<title id=") && svg.contains("<desc id=")),
    ("a bar series", |_, svg| svg.contains("class=\"pele-series pele-bars\"")),
    ("several bar series", |_, svg| svg.matches("class=\"pele-series pele-bars\"").count() > 1),
    ("a line series", |_, svg| svg.contains("class=\"pele-series pele-line\"")),
    ("several line series", |_, svg| svg.matches("class=\"pele-series pele-line\"").count() > 1),
    ("bars and lines together", |_, svg| svg.contains("pele-bars") && svg.contains("pele-line")),
    ("a series with a title", |_, svg| {
        svg.contains("pele-series pele-bars\" data-id") || svg.contains("pele-series pele-line\" data-id")
    }),
    ("a horizontal chart", |source, _| horizontal(source)),
    ("a category axis", |source, _| source.lines().any(|line| line.starts_with("x-axis") && line.contains('['))),
    ("a numeric x axis from the source", |source, _| {
        source.lines().any(|line| line.starts_with("x-axis") && line.contains("-->"))
    }),
    ("a numeric x axis from the data", |source, _| !source.contains("x-axis")),
    ("a value range from the source", |source, _| {
        source.lines().any(|line| line.starts_with("y-axis") && line.contains("-->"))
    }),
    ("a value range from the data", |source, _| {
        !source.lines().any(|line| line.starts_with("y-axis") && line.contains("-->"))
    }),
    ("a negative value", |source, _| source.contains("[-") || source.contains(", -")),
    ("bars that stand on a line inside the plot", |_, svg| {
        groups(svg, "<path class=\"pele-axis\" d=\"").any(|rest| {
            let d = &rest[..rest.find('"').unwrap_or(0)];
            d.matches('M').count() > 2 + d.matches('h').count() + d.matches('v').count()
        })
    }),
    ("a line cut where it leaves the range", |_, svg| {
        groups(svg, "<g class=\"pele-series pele-line\"").any(|line| {
            line.split_once("<path d=\"").is_some_and(|(_, d)| d[..d.find('"').unwrap_or(0)].matches('M').count() > 1)
        })
    }),
    ("dots on a line", |_, svg| svg.contains(" r=\"3\"/>")),
    ("a line too dense for dots", |_, svg| {
        groups(svg, "<g class=\"pele-series pele-line\"").any(|line| {
            let line = &line[..line.find("</g>").unwrap_or(line.len())];
            line.contains("<path") && !line.contains("<circle")
        })
    }),
    ("point labels", |_, svg| svg.contains("class=\"pele-point-labels\"")),
    ("point labels beside a horizontal line", |_, svg| {
        groups(svg, "<g class=\"pele-point-labels\"").any(|labels| labels.contains("text-anchor=\"start\""))
    }),
    ("data labels", |_, svg| svg.contains("class=\"pele-data-labels\"")),
    ("data labels past the end of a horizontal bar", |_, svg| {
        groups(svg, "<g class=\"pele-data-labels\"")
            .any(|labels| labels.contains("text-anchor=\"start\"") || labels.contains("text-anchor=\"end\""))
    }),
    ("a legend", |_, svg| svg.contains("class=\"pele-legend\"")),
    ("a legend with a line swatch", |_, svg| svg.contains("h12\" stroke=")),
    ("a legend with a bar swatch", |_, svg| svg.contains("rx=\"2\"")),
    ("a turned left axis title", |_, svg| svg.contains(") rotate(-90)\">")),
    ("a bottom axis title", |_, svg| {
        groups(svg, "<g class=\"pele-axis-titles\"").any(|titles| !titles.starts_with("><g transform"))
    }),
    ("axis labels", |_, svg| svg.contains("class=\"pele-axis-labels\"")),
    ("an axis label on several lines", |_, svg| {
        groups(svg, "<g class=\"pele-axis-labels\"")
            .any(|labels| labels[..labels.find("</g>").unwrap_or(0)].contains("<tspan"))
    }),
    ("tick marks", |_, svg| svg.contains("h-4") || svg.contains("v4")),
    ("a grid", |_, svg| svg.contains("class=\"pele-grid\"")),
    ("an axis without its line", |source, _| source.contains("showAxisLine: false")),
    ("a palette from the source", |_, svg| svg.contains(";--pele-series-1:")),
];

/// Whether the source turns the chart on its side, in its header or its config.
fn horizontal(source: &str) -> bool {
    source.lines().any(|line| line.starts_with("xychart") && line.ends_with(" horizontal"))
        || (source.contains("chartOrientation: horizontal") && !source.lines().any(|line| line.ends_with(" vertical")))
}

/// The text after each `start` up to the start of the next one.
fn groups<'a>(svg: &'a str, start: &'a str) -> impl Iterator<Item = &'a str> {
    svg.split(start).skip(1)
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
                    *if r#type == "xychart" { &mut self.identical } else { &mut self.other_types } += 1;
                    if r#type == "xychart" {
                        self.reached
                            .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    }
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "xychart" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "xychart" => {
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
                Some(format!("pele rejects an XY chart, ours reports it as not ported: {error}"))
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

/// The widths a host may have.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

/// How many of the cases Pele draws another way for the width they give than with room to
/// spare, which for an XY chart is a narrower one.
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested XY chart sources");
    assert!(tally.identical > 800, "the harvest holds XY charts Pele renders");
    assert!(tally.rejected > 300, "the harvest holds XY charts Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested XY chart sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated XY chart sources");
}

/// Pele's worst cases, with the limit on a source's length lifted as its stress test lifts it.
#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> =
        harvest("worst").into_iter().map(|source| (source, json!({ "limit": 1_000_000_000u64 }))).collect();
    run("worst", &cases).finish("Pele's worst-case XY charts");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let sales = "xychart\n  title \"Sales Revenue\"\n  x-axis Months [jan, feb, mar, apr, may, jun, jul, aug, sep, oct, nov, dec]\n  y-axis \"Revenue (in $)\" 4000 --> 11000\n  bar Sales [5000, 6000, 7500, 8200, 9500, 10500, 11000, 10200, 9200, 8500, 7000, 6000]\n  line Trend [5000, 6000, 7500, 8200, 9500, 10500, 11000, 10200, 9200, 8500, 7000, 6000]";
    let both = "xychart horizontal\n  x-axis [a, \"a longer category\", c]\n  bar up [3, -2, 5]\n  bar down [-1, 4, -6]\n  line [2 \"two\", -3 \"minus three\", 1]";
    let cut = "xychart\n  x-axis 0 --> 10\n  y-axis 0 --> 10\n  line [5, 15, 5, -5, 5, 12, 12, 3]\n  line [20, 30]";
    let mut out = vec![
        (sales.to_owned(), json!({})),
        (sales.to_owned(), json!({ "maxWidth": 300 })),
        (sales.to_owned(), json!({ "maxWidth": 500, "fontSize": 12 })),
        (sales.to_owned(), json!({ "fontSize": 22, "padding": 0 })),
        (sales.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (both.to_owned(), json!({})),
        (both.to_owned(), json!({ "maxWidth": 260 })),
        (cut.to_owned(), json!({})),
        ("xychart\n title \"Empty\"".to_owned(), json!({})),
        ("xychart\n line [1]".to_owned(), json!({ "outputLimit": 200 })),
        ("xychart\n line [1]".to_owned(), json!({ "limit": 5 })),
    ];
    for config in [
        json!({"xyChart": {"showDataLabel": true}}),
        json!({"xyChart": {"showDataLabel": true, "chartOrientation": "horizontal"}}),
        json!({"xyChart": {"width": 300, "height": 900}}),
        json!({"xyChart": {"width": 50, "height": 9000}}),
        json!({"xyChart": {"width": "800"}}),
        json!({"xyChart": {"showTitle": false, "showLegend": false}}),
        json!({"xyChart": {"xAxis": {"showLabel": false, "showTick": false}, "yAxis": {"showTitle": false, "showAxisLine": false}}}),
        json!({"xyChart": {"xAxis": {"showAxisLine": false}, "yAxis": {"showLabel": false, "showTick": false}}}),
        json!({"xyChart": "wide"}),
        json!({"xyChart": {"useMaxWidth": false}}),
        json!({"themeVariables": {"xyChart": {"plotColorPalette": "#ff0000, rgb(0, 0, 255)"}}}),
        json!({"themeVariables": {"xyChart": {"plotColorPalette": "url(a), blue"}}}),
    ] {
        out.push((sales.to_owned(), json!({ "config": config.clone() })));
        out.push((both.to_owned(), json!({ "config": config, "maxWidth": 400 })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("XY chart cases");
}

/// Random charts, each with host options drawn at random: a width, a font size or a padding.
#[test]
fn random_xy_charts_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..4000)
        .map(|_| {
            let source = xychart_random::source(&mut rng);
            let options = match rng.below(8) {
                0 => json!({ "maxWidth": 320 }),
                1 => json!({ "maxWidth": 480 }),
                2 => json!({ "maxWidth": 640 }),
                3 => json!({ "fontSize": 12 }),
                4 => json!({ "padding": 0, "maxWidth": 900 }),
                _ => json!({}),
            };
            (source, options)
        })
        .collect();
    let tally = run("random", &cases).finish("random XY charts");
    let bounded: Vec<(String, Value)> =
        cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect();
    println!("their widths make Pele draw {} of the random charts another way", redrawn_to_fit(&bounded));
    tally.reaches_every_path("random XY charts");
}
