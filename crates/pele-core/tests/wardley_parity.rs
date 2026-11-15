//! Differential test for drawing wardley maps: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/wardley/harvest.json` holds the wardley sources of Pele's own test suite, its
//! worst cases and the inputs of its seeded mutation runs, written by
//! `scripts/harvest-wardley.mjs`. The widths put every map wider than the host through the
//! narrower draws of `fitWidth`.

mod wardley_random;

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use wardley_random::Rng;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/wardley/harvest.json");
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
    ("an accessible title and description", |_, svg| svg.contains("<title id=") && svg.contains("<desc id=")),
    ("a component", |_, svg| svg.contains("pele-node pele-wardley-component")),
    ("an anchor", |_, svg| svg.contains("pele-node pele-wardley-anchor")),
    ("a market", |_, svg| svg.contains("class=\"pele-wardley-market\"")),
    ("a build", |_, svg| svg.contains("class=\"pele-wardley-build\"")),
    ("a buy", |_, svg| svg.contains("class=\"pele-wardley-buy\"")),
    ("an outsource", |_, svg| svg.contains("class=\"pele-wardley-outsource\"")),
    ("inertia", |_, svg| svg.contains("class=\"pele-wardley-inertia\"")),
    ("inertia beside a strategy", |_, svg| {
        groups(svg, "<g class=\"pele-node")
            .any(|node| node.contains("pele-wardley-inertia") && node.contains("r=\"12\""))
    }),
    ("evolution stages from the source", |source, _| source.contains("\nevolution ")),
    ("stages with their own boundaries", |source, _| {
        source.lines().any(|line| line.starts_with("evolution ") && line.matches('@').count() >= 2)
    }),
    ("a stage label on several lines", |_, svg| {
        groups(svg, "<text class=\"pele-wardley-stage-label\"").any(|text| text.matches("<tspan").count() > 1)
    }),
    ("a grid", |_, svg| svg.contains("class=\"pele-wardley-grid\"")),
    ("a pipeline", |_, svg| svg.contains("class=\"pele-wardley-pipeline\"")),
    ("a pipeline of several components", |_, svg| {
        groups(svg, "<g class=\"pele-wardley-pipeline\"").any(|pipeline| pipeline.contains("<path"))
    }),
    ("a pipeline parent", |_, svg| svg.contains("pele-wardley-pipeline-parent")),
    ("a pipeline component", |_, svg| svg.contains("pele-wardley-pipeline-component")),
    ("a link", |_, svg| svg.contains("<g class=\"pele-edge")),
    ("a dashed link", |_, svg| svg.contains("pele-wardley-dashed")),
    ("a link with one arrowhead", |_, svg| edges(svg).any(|edge| edge.matches("pele-marker").count() == 1)),
    ("a link with two arrowheads", |_, svg| edges(svg).any(|edge| edge.matches("pele-marker").count() == 2)),
    ("a link label", |_, svg| svg.contains("class=\"pele-edge-label\"")),
    ("a link to an anchor", |_, svg| link_to_anchor(svg)),
    ("an evolve arrow", |_, svg| groups(svg, "<g class=\"pele-wardley-trend\"").any(|trend| trend.contains("<path"))),
    ("an evolve target too near for an arrow", |_, svg| {
        groups(svg, "<g class=\"pele-wardley-trend\"")
            .any(|trend| trend.starts_with(" data-id=\"") && !trend.contains("<path"))
    }),
    ("a label moved off its links", |_, svg| label_moved(svg)),
    ("a label with an offset", |source, _| source.contains(" label [")),
    ("a label on several lines", |_, svg| {
        groups(svg, "<text class=\"pele-label\"").any(|text| text.matches(" x=\"").count() > 1)
    }),
    ("a label with emphasis of its own", |_, svg| {
        groups(svg, "<text class=\"pele-label\"").any(|text| text.contains("<tspan font-weight=\"var(--_w)\""))
    }),
    ("a note", |_, svg| svg.contains("class=\"pele-wardley-note\"")),
    ("an annotation", |_, svg| svg.contains("class=\"pele-wardley-annotation\"")),
    ("a list of annotations", |_, svg| svg.contains("class=\"pele-wardley-annotations\"")),
    ("a list of annotations placed by the source", |source, svg| {
        source.contains("\nannotations [") && svg.contains("class=\"pele-wardley-annotations\"")
    }),
    ("an accelerator", |_, svg| svg.contains("class=\"pele-wardley-accelerator\"")),
    ("a deaccelerator", |_, svg| svg.contains("class=\"pele-wardley-deaccelerator\"")),
    ("a size from the source", |source, _| source.contains("\nsize [")),
    ("a size from the config", |source, _| source.contains("    width: ")),
];

/// The text after each `start` up to the start of the next one.
fn groups<'a>(svg: &'a str, start: &'a str) -> impl Iterator<Item = &'a str> {
    svg.split(start).skip(1)
}

/// Each link group's markup, up to the next group of any kind.
fn edges(svg: &str) -> impl Iterator<Item = &str> {
    groups(svg, "<g class=\"pele-edge").map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())])
}

fn data_id(rest: &str) -> Option<&str> {
    let (_, id) = rest.split_once("data-id=\"")?;
    Some(&id[..id.find('"')?])
}

fn link_to_anchor(svg: &str) -> bool {
    let anchors: Vec<&str> = groups(svg, "<g class=\"pele-node pele-wardley-anchor\"").filter_map(data_id).collect();
    edges(svg).filter_map(data_id).any(|id| {
        anchors.iter().any(|anchor| id.starts_with(&format!("{anchor}-")) || id.ends_with(&format!("-{anchor}")))
    })
}

/// Whether a component's label starts left of its circle or below it.
fn label_moved(svg: &str) -> bool {
    let number = |text: &str, name: &str| -> Option<f64> {
        let (_, rest) = text.split_once(&format!(" {name}=\""))?;
        rest[..rest.find('"')?].parse().ok()
    };
    groups(svg, "<g class=\"pele-node pele-wardley-component\"").any(|node| {
        let Some((circle, text)) = node.split_once("<text") else { return false };
        match (number(circle, "cx"), number(circle, "cy"), number(text, "x"), number(text, "y")) {
            (Some(cx), Some(cy), Some(x), Some(y)) => x < cx || y > cy,
            _ => false,
        }
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
                    *if r#type == "wardley" { &mut self.identical } else { &mut self.other_types } += 1;
                    if r#type == "wardley" {
                        self.reached
                            .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    }
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "wardley" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "wardley" => {
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
                Some(format!("pele rejects a wardley map, ours reports it as not ported: {error}"))
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
/// spare, which for a wardley map is a narrower plot.
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested wardley sources");
    assert!(tally.identical > 500, "the harvest holds wardley maps Pele renders");
    assert!(tally.rejected > 50, "the harvest holds wardley maps Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested wardley sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated wardley sources");
}

/// Pele's worst cases, with the limit on a source's length lifted as its stress test lifts it.
#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> =
        harvest("worst").into_iter().map(|source| (source, json!({ "limit": 1_000_000_000u64 }))).collect();
    run("worst", &cases).finish("Pele's worst-case wardley maps");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let tea = "wardley-beta\ntitle Tea Shop\nanchor Business [0.95, 0.63]\nanchor Public [0.95, 0.78]\ncomponent Cup of Tea [0.79, 0.61] label [19, -4]\ncomponent Tea [0.63, 0.81]\ncomponent Hot Water [0.52, 0.80]\ncomponent Kettle [0.43, 0.35] (build) inertia\ncomponent Power [0.1, 0.7] (outsource)\nBusiness -> Cup of Tea\nPublic -> Cup of Tea\nCup of Tea -> Tea\nCup of Tea -> Hot Water\nHot Water -> Kettle\nKettle -> Power\nevolve Kettle 0.62\nnote \"Standardising power allows Kettles to evolve faster\" [0.30, 0.49]\naccelerator Pressure [0.6, 0.2]\nannotation 1,[0.48, 0.85] \"Standardising\"\nannotation 2,[0.9, 0.3] \"two\"\nannotations [0.05, 0.95]";
    let crowded = "wardley-beta\ncomponent A [0.5, 0.5]\ncomponent N [0.9, 0.5]\ncomponent S [0.1, 0.5]\ncomponent E [0.5, 0.9]\ncomponent W [0.5, 0.1]\ncomponent NE [0.8, 0.8]\ncomponent NW [0.8, 0.2]\ncomponent SE [0.2, 0.8]\ncomponent SW [0.2, 0.2]\nA -> NE\nA -> SE\nA -> NW\nA -> SW\nA +<> N; north\nA +> E; east\n";
    let pipeline = "wardley-beta\ncomponent Kettle [0.45, 0.57] inertia\ncomponent Water [0.8, 0.8]\npipeline Kettle {\n  component Campfire Kettle [0.35]\n  component Electric Kettle [0.53] label [5, 5]\n  component Smart Kettle [0.8]\n}\nWater -> Electric Kettle\nCampfire Kettle -> Kettle\nKettle -> Water\nevolve Kettle 0.9";
    let stages = "wardley-beta\nevolution Genesis@0.3 / Concept -> Custom@0.5 -> Product a long stage name that has to wrap@0.9 -> Utility@1.0\ncomponent A [0.5, 0.5]";
    let titled = "---\ntitle: A title much wider than the map under it, surely, it goes on and on and on\n---\nwardley-beta\ncomponent A [0.5, 0.5]";
    let mut out = vec![
        (tea.to_owned(), json!({})),
        (tea.to_owned(), json!({ "maxWidth": 200 })),
        (tea.to_owned(), json!({ "maxWidth": 500, "fontSize": 12 })),
        (tea.to_owned(), json!({ "fontSize": 20, "padding": 0 })),
        (tea.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (crowded.to_owned(), json!({})),
        (crowded.to_owned(), json!({ "maxWidth": 300 })),
        (pipeline.to_owned(), json!({})),
        (pipeline.to_owned(), json!({ "maxWidth": 250 })),
        (stages.to_owned(), json!({})),
        (stages.to_owned(), json!({ "maxWidth": 320 })),
        (titled.to_owned(), json!({})),
        (titled.to_owned(), json!({ "maxWidth": 200 })),
        ("wardley-beta\ncomponent A [0.5, 0.5]".to_owned(), json!({ "outputLimit": 200 })),
        ("wardley-beta\ncomponent A [0.5, 0.5]".to_owned(), json!({ "limit": 5 })),
    ];
    for config in [
        json!({"wardley-beta": {"showGrid": true}}),
        json!({"wardley-beta": {"showGrid": "true"}}),
        json!({"wardley-beta": {"width": 300, "height": 1e6}}),
        json!({"wardley-beta": {"width": 0.5}}),
        json!({"wardley-beta": {"width": 2e6}}),
        json!({"wardley-beta": {"width": "800"}}),
        json!({"wardley-beta": "wide"}),
        json!({"wardley-beta": {"useMaxWidth": false}}),
    ] {
        out.push((tea.to_owned(), json!({ "config": config.clone() })));
        out.push((pipeline.to_owned(), json!({ "config": config, "maxWidth": 400 })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("wardley cases");
}

/// Random maps, each with host options drawn at random: a width, a font size or a padding.
#[test]
fn random_wardley_maps_match_pele() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let cases: Vec<(String, Value)> = (0..4000)
        .map(|_| {
            let source = wardley_random::source(&mut rng);
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
    let tally = run("random", &cases).finish("random wardley maps");
    let redrawn = redrawn_to_fit(
        &cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect::<Vec<_>>(),
    );
    println!("their widths make Pele draw {redrawn} of the random maps another way");
    tally.reaches_every_path("random wardley maps");
}
