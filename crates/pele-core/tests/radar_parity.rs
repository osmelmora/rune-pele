//! Differential test for drawing radar charts: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/radar/harvest.json` holds the radar sources of Pele's test suite and the inputs
//! of its mutation runs, written by `scripts/harvest-radar.mjs`. A chart wider than the host is
//! drawn smaller, up to three times, with its legend under it, which the widths reach. The corpus
//! has three sources, so the harvest, the mutations and the random charts carry the proof. The
//! random charts must reach every drawing path in [`PATHS`].

#![expect(clippy::disallowed_methods, reason = "the test's own arithmetic, which no SVG reads")]

#[path = "radar/random.rs"]
mod random;
#[path = "standalone/parity.rs"]
mod standalone;

use pele_core::ParseOptions;
use serde_json::{Value, json};
use standalone::{Case, Parity, Reached, WIDTHS, attribute, harvest, plain, redrawn_to_fit, tags};

/// The model this port reads, which `radar_front.rs` holds to Pele's.
fn model(source: &str) -> Value {
    pele_core::radar_front_json(source, ParseOptions::default())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or(Value::Null)
}

fn number(value: &Value) -> f64 {
    value.as_str().and_then(|text| text.parse().ok()).unwrap_or(f64::NAN)
}

/// Each drawn curve's values, with the scale they are drawn on.
fn scales(c: &Case) -> Vec<(Vec<f64>, f64, f64)> {
    let model = model(c.source);
    let axes = model["axes"].as_array().map_or(0, Vec::len);
    let curves = model["curves"].as_array().cloned().unwrap_or_default();
    let values = |curve: &Value| curve["entries"].as_array().map(|list| list.iter().map(number).collect::<Vec<_>>());
    let max = match &model["options"]["max"] {
        Value::Null => curves.iter().filter_map(values).flatten().fold(f64::NEG_INFINITY, f64::max),
        max => number(max),
    };
    let min = number(&model["options"]["min"]);
    curves.iter().filter_map(values).filter(|values| values.len() == axes).map(|values| (values, min, max)).collect()
}

/// The inside of every axis label's group.
fn axis_labels(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-axis-label\"")
        .skip(1)
        .filter_map(|rest| {
            let open = rest.find('>')? + 1;
            Some(&rest[open..open + rest[open..].find("</g>")?])
        })
        .collect()
}

/// The centre of every axis label, relative to the chart's centre.
fn label_sides(svg: &str) -> Vec<f64> {
    axis_labels(svg)
        .iter()
        .filter_map(|inside| {
            tags(inside, "<text").into_iter().next().and_then(|text| attribute(text, "x")?.parse().ok())
        })
        .collect()
}

/// The drawing paths a source can go through, each with what Pele's SVG or the model shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a circle graticule", |c| {
        c.svg.contains("<g class=\"pele-graticule\" fill=\"none\" stroke=\"var(--_a)\"><circle")
    }),
    ("a polygon graticule", |c| {
        c.svg.contains("<g class=\"pele-graticule\" fill=\"none\" stroke=\"var(--_a)\"><polygon")
    }),
    ("no graticule", |c| !c.svg.contains("pele-graticule")),
    ("a graticule ring capped at the radius", |c| {
        let ticks = number(&model(c.source)["options"]["ticks"]);
        ticks.fract() != 0.0 && c.svg.contains("pele-graticule")
    }),
    ("spokes", |c| c.svg.contains("class=\"pele-axes\"")),
    ("no axes", |c| c.svg.contains("<g transform=") && !c.svg.contains("pele-axes")),
    ("a curve as a closed spline", |c| c.svg.contains("<path class=\"pele-curve\"")),
    ("a curve as a polygon", |c| c.svg.contains("<polygon class=\"pele-curve\"")),
    ("a curve left out for lacking values", |c| {
        let model = model(c.source);
        let axes = model["axes"].as_array().map_or(0, Vec::len);
        model["curves"].as_array().is_some_and(|curves| {
            curves.iter().any(|curve| curve["entries"].as_array().is_some_and(|e| e.len() != axes))
        })
    }),
    ("more than eight curves", |c| c.svg.matches("class=\"pele-curve\"").count() > 8),
    ("a value past the maximum", |c| scales(c).iter().any(|(values, _, max)| values.iter().any(|value| value > max))),
    ("a value under the minimum", |c| scales(c).iter().any(|(values, min, _)| values.iter().any(|value| value < min))),
    ("a scale with no range", |c| scales(c).iter().any(|(_, min, max)| min >= max)),
    ("a value too large for a double", |c| {
        scales(c).iter().any(|(values, _, _)| values.iter().any(|value| value.is_infinite()))
    }),
    ("an axis label to the right", |c| label_sides(c.svg).iter().any(|&x| x > 0.5)),
    ("an axis label to the left", |c| label_sides(c.svg).iter().any(|&x| x < -0.5)),
    ("an axis label above or below", |c| label_sides(c.svg).iter().any(|&x| x.abs() <= 0.5)),
    ("an axis with an empty label", |c| axis_labels(c.svg).iter().any(|inside| inside.is_empty())),
    ("an axis label on two lines", |c| axis_labels(c.svg).iter().any(|inside| inside.matches("<tspan x=").count() > 1)),
    ("a legend beside the chart", |c| c.svg.contains("pele-legend-item") && !c.svg.contains("<rect x=\"-")),
    ("a legend under the chart", |c| c.svg.contains("pele-legend-item") && c.svg.contains("<rect x=\"-")),
    ("no legend", |c| c.svg.contains("pele-curve") && !c.svg.contains("pele-legend")),
    ("series colours from the theme", |c| c.svg.contains("--pele-series-1:")),
    ("the chart's own size and margins", |c| c.source.contains("  radar:\n")),
    ("a title", |c| c.svg.contains("class=\"pele-title\"")),
    ("a title wrapped to fit", |c| {
        c.svg
            .split("class=\"pele-title\"")
            .nth(1)
            .is_some_and(|t| t[..t.find("</text>").unwrap_or(0)].matches("<tspan x=").count() > 1)
    }),
    ("an accessible title and description", |c| {
        c.svg.contains("aria-labelledby=") && c.svg.contains("aria-describedby=")
    }),
];

const RADAR: Parity = Parity { kind: "radar", paths: PATHS };

#[test]
fn harvested_sources_match_pele() {
    let tally = RADAR.run("harvested", &plain(harvest("radar", "sources"))).finish("harvested radar sources");
    assert!(tally.identical > 500, "the harvest holds charts Pele draws");
    assert!(tally.rejected > 100, "the harvest holds charts Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("radar", "sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        RADAR.run("harvested", &cases).finish(&format!("harvested radar sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    RADAR.run("mutated", &plain(harvest("radar", "mutated"))).finish("mutated radar sources");
}

/// Host options the harvest does not set, and Pele's worst cases for radar
/// (`tests/radar/stress.test.ts`) with the source limit lifted as that test lifts it.
fn cases() -> Vec<(String, Value)> {
    const N: usize = 50_000;
    let list = |count: usize, item: &dyn Fn(usize) -> String| (0..count).map(item).collect::<Vec<_>>().join(", ");
    let chart = "radar-beta\ntitle Skills\naxis speed[\"Speed\"], power, range\ncurve a[\"Alpha\"]{3, 4, 5}, b{range: 1, speed 2, power 3}\nmax 6\n";
    let mut out = vec![
        (chart.to_owned(), json!({})),
        (chart.to_owned(), json!({ "fontSize": 12, "padding": 0 })),
        (chart.to_owned(), json!({ "fontSize": 22, "maxWidth": 300 })),
        (chart.to_owned(), json!({ "maxWidth": 100 })),
        (chart.to_owned(), json!({ "maxWidth": 1 })),
        (chart.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (
            chart.to_owned(),
            json!({ "config": { "radar": { "useMaxWidth": false, "width": 500, "curveTension": 0.4 } } }),
        ),
        (
            chart.to_owned(),
            json!({ "config": { "radar": { "height": 100, "axisLabelFactor": 2, "axisScaleFactor": 0.5 } } }),
        ),
        (chart.to_owned(), json!({ "config": { "themeVariables": { "cScale0": "#123456", "cScale1": "red" } } })),
        (chart.to_owned(), json!({ "outputLimit": 100 })),
        (chart.to_owned(), json!({ "limit": 10 })),
    ];
    let big = json!({ "limit": 1_000_000_000u64 });
    for source in [
        format!("radar-beta\naxis {}\ncurve c{{{}}}", list(5000, &|i| format!("a{i}")), list(5000, &|i| i.to_string())),
        format!(
            "radar-beta\naxis {}\ncurve c{{{}}}\ngraticule polygon\nticks 32",
            list(5000, &|i| format!("a{i}")),
            list(5000, &|_| "1".to_owned())
        ),
        format!(
            "radar-beta\naxis {}\n{}",
            list(2000, &|i| format!("a{i}")),
            (0..20)
                .map(|k| format!("curve c{k}{{{}}}\n", list(2000, &|i| format!("a{i}: {}", (i + k) % 7))))
                .collect::<String>()
        ),
        format!("radar-beta\naxis a,b,c\n{}", (0..4000).map(|i| format!("curve c{i}{{1,2,3}}\n")).collect::<String>()),
        format!("radar-beta\n{}", "ticks 3\nmax 9\n".repeat(2500)),
        format!("radar-beta\naxis a\ncurve c{{1}}\nmax {}", "9".repeat(N)),
        format!("radar-beta\naxis {}", "a".repeat(N)),
        format!("radar-beta\naxis a[\"{}\"], b\ncurve c{{1,2}}", "word ".repeat(N / 5)),
        format!("radar-beta\naxis a[\"{}\"], b\ncurve c{{1,2}}", "#35;&amp;<br>".repeat(N / 13)),
        format!("radar-beta\naxis a\ncurve c{{{}1{}}}", "\n".repeat(20000), "\n".repeat(20000)),
        format!("radar-beta\ntitle {}", "x".repeat(N)),
    ] {
        out.push((source, big.clone()));
    }
    out
}

#[test]
fn cases_match_pele() {
    RADAR.run("case", &cases()).finish("radar cases");
}

#[test]
fn random_charts_match_pele() {
    let mut rng = random::Rng(0x0dab_ad0e_5eed);
    let mut cases = Vec::new();
    for _ in 0..4000 {
        let source = random::source(&mut rng);
        let options = match rng.below(8) {
            0 | 1 => json!({ "maxWidth": *rng.pick(&[200.0, 320.0, 360.0, 480.0]) }),
            2 => json!({ "maxWidth": 300, "fontSize": *rng.pick(&[12.0, 20.0]) }),
            3 => json!({ "fontSize": *rng.pick(&[11.0, 14.5, 18.0]), "padding": *rng.pick(&[0.0, 4.0, 16.0]) }),
            _ => json!({}),
        };
        cases.push((source, options));
    }
    let tally = RADAR.run("random", &cases).finish("random radar charts");
    let widths: Vec<(String, Value)> =
        cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect();
    println!("their widths make Pele draw {} of the random radar charts another way", redrawn_to_fit(&widths));
    let missed = RADAR.missed(&tally);
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random radar chart reaches {missed:?}");
}
