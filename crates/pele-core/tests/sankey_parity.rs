//! Differential test for drawing sankey diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message, a cycle among them.
//!
//! `tests/fixtures/sankey/harvest.json` holds the sankey sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-sankey.mjs`. A chart wider than
//! the host is drawn again narrower, so the widths matter, and an id prefix decides whether its
//! links are drawn with gradients.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "sankey/random.rs"]
mod random;

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use random::{Rng, random_options, random_sankey};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/sankey/harvest.json");
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

/// Whether Pele's SVG shows that a source went through one path of the draw step.
type Shows = fn(&str) -> bool;

fn contains(svg: &str, mark: &str) -> bool {
    svg.contains(mark)
}

/// The value of `name` in the attributes that follow `mark` in `text`.
fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split(&format!(" {name}=\"")).nth(1)?.split('"').next()
}

/// Each node's bar: its id, then its fill, `y` and `height`.
fn bars(svg: &str) -> Vec<(&str, &str, f64, f64)> {
    svg.split("<rect class=\"pele-sankey-node\"")
        .skip(1)
        .filter_map(|rest| {
            let tag = rest.split("/>").next()?;
            let number = |name| attribute(tag, name)?.parse::<f64>().ok();
            Some((attribute(tag, "data-id")?, attribute(tag, "fill")?, number("y")?, number("height")?))
        })
        .collect()
}

/// Each link's ends and its fill.
fn bands(svg: &str) -> Vec<(&str, &str, &str)> {
    svg.split("<path class=\"pele-sankey-link\"")
        .skip(1)
        .filter_map(|rest| {
            let tag = rest.split('>').next()?;
            let (from, to) = attribute(tag, "data-id")?.split_once("-&gt;")?;
            Some((from, to, attribute(tag, "fill")?))
        })
        .collect()
}

/// Whether some label is not on the line through the middle of its bar.
fn moved_label(svg: &str) -> bool {
    let Some(size) = attribute(svg.split("<g class=\"pele-labels\"").nth(1).unwrap_or(""), "font-size")
        .and_then(|size| size.parse::<f64>().ok())
    else {
        return false;
    };
    let labels: Vec<(&str, f64)> = svg
        .split("<g class=\"pele-label\" data-id=\"")
        .skip(1)
        .filter_map(|rest| Some((rest.split('"').next()?, attribute(rest, "y")?.parse().ok()?)))
        .collect();
    let at: HashMap<&str, f64> = labels.into_iter().collect();
    bars(svg)
        .iter()
        .any(|(id, _, y, height)| at.get(id).is_some_and(|at| (at - (y + height / 2.0 + size * 0.35)).abs() > 1.0))
}

/// Whether some link is in its target's colour and not its source's.
fn target_coloured(svg: &str) -> bool {
    let fills: HashMap<&str, &str> = bars(svg).into_iter().map(|(id, fill, ..)| (id, fill)).collect();
    bands(svg).iter().any(|(from, to, fill)| fills.get(to) == Some(fill) && fills.get(from) != Some(fill))
}

/// The drawing paths of `renderSankey`, each with what in Pele's SVG shows it was taken.
const PATHS: &[(&str, Shows)] = &[
    ("a title", |svg| contains(svg, "class=\"pele-title\"")),
    ("links with gradients", |svg| contains(svg, "<linearGradient")),
    ("a gradient two links share", |svg| {
        svg.matches("url(#").count() > svg.matches("<linearGradient").count() && contains(svg, "<defs>")
    }),
    ("a gradient to a node of its own colour", |svg| {
        svg.split("stop-color=\"").skip(1).any(|rest| !rest.starts_with("var("))
    }),
    ("links without gradients", |svg| bands(svg).iter().any(|(_, _, fill)| fill.starts_with("var("))),
    ("links in their target's colour", target_coloured),
    ("links in one colour", |svg| {
        svg.split("<path class=\"pele-sankey-link\"")
            .skip(1)
            .any(|link| link.split('>').next().is_some_and(|tag| tag.contains("style=\"fill:")))
    }),
    ("a node in its own colour", |svg| {
        svg.split("<rect class=\"pele-sankey-node\"")
            .skip(1)
            .any(|bar| bar.split("/>").next().is_some_and(|tag| tag.contains("style=")))
    }),
    ("values shown", |svg| contains(svg, "class=\"pele-sankey-value\"")),
    ("values hidden", |svg| contains(svg, "<g class=\"pele-label\"") && !contains(svg, "pele-sankey-value")),
    ("outlined labels", |svg| contains(svg, "stroke-width=\"3\" stroke-linejoin=\"round\"")),
    ("a label after its bar", |svg| contains(svg, "text-anchor=\"start\"")),
    ("a label before its bar", |svg| contains(svg, "text-anchor=\"end\"")),
    ("a label moved off the middle of its bar", moved_label),
    ("a bar thinner than a pixel", |svg| bars(svg).iter().any(|(.., height)| *height == 1.0)),
    ("labels shrunk to fit", |svg| {
        attribute(svg.split("<g class=\"pele-labels\"").nth(1).unwrap_or(""), "font-size")
            .is_some_and(|size| size != "14")
    }),
    ("three columns or more", |svg| {
        let columns: BTreeSet<&str> = svg
            .split("<rect class=\"pele-sankey-node\"")
            .skip(1)
            .filter_map(|bar| attribute(bar.split("/>").next()?, "x"))
            .collect();
        columns.len() > 2
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
                    if r#type == "sankey" {
                        self.identical += 1;
                        self.reached.extend(PATHS.iter().filter(|(_, shows)| shows(svg)).map(|(name, _)| *name));
                    } else {
                        self.other_types += 1;
                    }
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "sankey" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "sankey" => {
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
                Some(format!("pele rejects a sankey diagram, ours reports it as not ported: {error}"))
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
            self.failures[..self.failures.len().min(10)].join("\n")
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

/// How many of the cases Pele draws another way for the width they give than with no width.
fn redrawn(cases: &[(String, Value)]) -> usize {
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

const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested sankey sources");
    assert!(tally.identical > 300, "the harvest holds sankey diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds sankey diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested sankey sources at maxWidth {width}"));
        println!("maxWidth {width} makes Pele draw {} of the harvested sources another way", redrawn(&cases));
    }
}

#[test]
fn harvested_sources_match_pele_with_gradients() {
    let cases: Vec<(String, Value)> =
        harvest("sources").into_iter().map(|source| (source, json!({ "idPrefix": "p1" }))).collect();
    run("gradient", &cases).finish("harvested sankey sources with an id prefix");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated sankey sources");
}

const BASIC: &str = "sankey\na,b,5\na,c,3\nb,d,4\nc,d,1.239\n";

fn configured(yaml: &str, body: &str) -> String {
    let lines: Vec<String> = yaml.split('\n').map(|line| format!("    {line}")).collect();
    format!("---\nconfig:\n  sankey:\n{}\n---\n{body}", lines.join("\n"))
}

/// Hand-written charts for what the harvest reaches least, each with the host options that reach it.
fn cases() -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let configs = [
        "",
        "linkColor: source",
        "linkColor: target",
        "linkColor: \"#a1a1a1\"",
        "linkColor: \"url(#x)\"",
        "nodeColors:\n  a: \"#123456\"\n  b: \"url(#x)\"\n  c: 'red&blue'\n  d: 'x\"y'",
        "linkColor: target\nnodeColors:\n  d: \"rgb(1, 2, 3)\"",
        "labelStyle: outlined\nprefix: \"&euro;\"\nsuffix: \" &amp;lt;\"",
        "showValues: false\nnodePadding: 40",
        "nodeAlignment: left",
        "nodeAlignment: right",
        "nodeAlignment: center",
        "width: 300\nheight: 150\nnodeWidth: 24",
        "width: 10001\nheight: 0\nnodeWidth: 500\nnodePadding: -1",
        "prefix: 7\nsuffix: .5",
    ];
    for config in configs {
        let source = if config.is_empty() { BASIC.to_owned() } else { configured(config, BASIC) };
        for options in [
            json!({}),
            json!({ "idPrefix": "p1" }),
            json!({ "idPrefix": "p_1-2" }),
            json!({ "idPrefix": "p 1" }),
            json!({ "maxWidth": 320, "idPrefix": "p1" }),
            json!({ "maxWidth": 100 }),
            json!({ "fontSize": 13.5, "padding": 20.5 }),
            json!({ "responsive": false }),
        ] {
            out.push((source.clone(), options));
        }
    }
    let chart = "sankey-beta\n\nAgricultural 'waste',Bio-conversion,124.729\nBio-conversion,Liquid,0.597\nBio-conversion,Losses,26.862\nBio-conversion,Solid,280.322\nBio-conversion,Gas,81.144\nBiofuel imports,Liquid,35\nBiomass imports,Solid,35\nCoal imports,Coal,11.606\nCoal reserves,Coal,63.965\nCoal,Solid,75.571\n";
    let more = [
        chart.to_owned(),
        format!("---\ntitle: A title much wider than the whole of the chart below it\n---\n{BASIC}"),
        "sankey\na,b,1\nb,c,1\nc,a,1\n".to_owned(),
        "sankey\na,a,1\n".to_owned(),
        "sankey\na,b,0\na,c,-1\n".to_owned(),
        "sankey\nbig,t,1000\nthin0,t,1\nthin1,t,1\nthin2,t,1\nthin3,t,1\nthin4,t,1\nthin5,t,1\n".to_owned(),
        format!("sankey\n{}", (0..40).map(|i| format!("s,t{i},{}\n", 1 + i % 5)).collect::<String>()),
        format!("sankey\n{}", (0..12).map(|i| format!("a{},b{},{}\n", i % 3, i % 4, 1 + i)).collect::<String>()),
        "sankey\n\"a\nb\",c,1\n\"<script>\",\"x\"\"y\",2\n".to_owned(),
    ];
    for source in more {
        for options in [json!({}), json!({ "maxWidth": 320 }), json!({ "idPrefix": "p1" })] {
            out.push((source.clone(), options));
        }
    }
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("sankey cases");
    assert!(tally.identical > 100, "the cases render");
}

/// Pele's own worst cases (`tests/sankey/stress.test.ts`) that it renders, each with the source
/// limit raised.
fn worst_cases() -> Vec<(String, Value)> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let head = "sankey\n";
    let mut mesh = head.to_owned();
    for i in 0..70 {
        for j in 0..70 {
            mesh.push_str(&format!("a{i},b{j},{}\n", (i * 7 + j) % 13));
        }
    }
    let mut layered = head.to_owned();
    for l in 0..40 {
        for i in 0..12 {
            for j in (0..12).step_by(3) {
                layered.push_str(&format!("{l}_{i},{}_{},{}\n", l + 1, (i + j) % 12, 1 + (i + j) % 5));
            }
        }
    }
    let big = json!({ "limit": 10_000_000 });
    [
        format!("{head}{}", repeat(8000, &|i| format!("{i},{},1\n", i + 1))),
        format!("{head}{}", repeat(6000, &|i| format!("c,t{i},{}\n", i + 1))),
        format!("{head}{}", repeat(6000, &|i| format!("s{i},c,{}\n", i + 1))),
        format!("{head}{}", "a,b,1\n".repeat(8000)),
        format!("{head}{}", repeat(2500, &|i| format!("n{i},n{},1\nhub,n{},1\n", i + 1, i + 1))),
        mesh,
        layered,
        format!("{head}{}", repeat(8000, &|i| format!("{i},{},1\n", (i + 1) % 8000))),
        format!("{head}a,b,1\nb,a,1\n{}7000,a,1\n", repeat(7000, &|i| format!("{i},{},1\n", i + 1))),
        format!(
            "{head}{}",
            repeat(3000, &|i| format!("a{},b{},{}\n", i % 50, i % 60, if i % 2 == 1 { "1e308" } else { "5e-324" }))
        ),
        format!(
            "{head}{}",
            repeat(5, &|i| format!("{},{},1\n", i.to_string().repeat(5000), (i + 1).to_string().repeat(5000)))
        ),
    ]
    .into_iter()
    .map(|source| (source, big.clone()))
    .collect()
}

#[test]
fn worst_cases_match_pele() {
    run("worst", &worst_cases()).finish("Pele's worst-case sankey diagrams");
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_sankey(&mut rng), random_options(&mut rng))).collect();
    let tally = run("random", &cases).finish("random sankey diagrams");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random diagram's output has {missed:?}");
}
