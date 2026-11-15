//! Differential test for drawing gantt charts: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/gantt/harvest.json` holds the gantt sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-gantt.mjs`. A gantt chart is
//! drawn to the host's width when that is narrower than its own, so every width redraws it.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "gantt/random.rs"]
mod random;

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use random::{Rng, random_gantt, random_options};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/gantt/harvest.json");
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
            "now" => layout.now = Some(number(value)),
            "links" => layout.resources.links = flag(value),
            "linkRel" => layout.resources.link_rel = text(value),
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

/// The group with `class` and everything inside it, empty when there is none.
fn group<'a>(svg: &'a str, class: &str) -> &'a str {
    let open = format!("<g class=\"{class}\"");
    let Some(start) = svg.find(&open) else { return "" };
    let rest = &svg[start..];
    let mut depth = 0;
    let mut at = 0;
    while at < rest.len() {
        if rest[at..].starts_with("<g ") || rest[at..].starts_with("<g>") {
            depth += 1;
        } else if rest[at..].starts_with("</g>") {
            depth -= 1;
            if depth == 0 {
                return &rest[..at + 4];
            }
        }
        at += rest[at..].chars().next().map_or(1, char::len_utf8);
    }
    rest
}

/// The `d` of the first path in the chart's grid group, which holds a line for each tick.
fn grid_lines(svg: &str) -> &str {
    group(svg, "pele-grid").split("d=\"").nth(1).and_then(|rest| rest.split('"').next()).unwrap_or("")
}

/// The `d` of the axis line, which runs twice with a top axis.
fn axis_line(svg: &str) -> &str {
    group(svg, "pele-grid").split("d=\"").nth(2).and_then(|rest| rest.split('"').next()).unwrap_or("")
}

/// The first number after `key="` in `text`.
fn first(text: &str, key: &str) -> Option<f64> {
    text.split(&format!("{key}=\"")).nth(1)?.split('"').next()?.parse().ok()
}

/// The drawing paths of `renderGantt`, each with what in Pele's SVG shows it was taken.
const PATHS: &[(&str, Shows)] = &[
    ("a title", |svg| contains(svg, "class=\"pele-title\"")),
    ("a chart with nothing to plot", |svg| !contains(svg, "pele-grid")),
    ("an accessible title", |svg| contains(svg, "aria-labelledby=")),
    ("an accessible description", |svg| contains(svg, "aria-describedby=")),
    ("a shaded section", |svg| contains(svg, "class=\"pele-section pele-section-0\"")),
    ("an unshaded section", |svg| contains(svg, "class=\"pele-section pele-section-1\"")),
    ("a third section style", |svg| contains(svg, "class=\"pele-section pele-section-2\"")),
    ("a section label", |svg| contains(svg, "class=\"pele-section-label\"")),
    ("a task bar", |svg| contains(svg, "style=\"rx:var(--_r)\"")),
    ("an active task", |svg| contains(svg, "pele-task pele-active")),
    ("a done task", |svg| contains(svg, "pele-task pele-done")),
    ("a critical task", |svg| contains(svg, "fill=\"var(--pele-critical,#e45756)\"")),
    ("a done critical task", |svg| contains(svg, "pele-done pele-crit")),
    ("a milestone", |svg| contains(svg, "stroke-linejoin=\"round\"")),
    ("a clickable task", |svg| contains(svg, "pele-clickable")),
    ("a link", |svg| contains(svg, "<a href=")),
    ("an unscheduled task", |svg| contains(svg, "pele-unscheduled")),
    ("a translucent active bar", |svg| contains(svg, "fill-opacity=\"0.25\"")),
    ("a label inside its bar", |svg| contains(svg, "class=\"pele-label\" fill=\"var(--_bg)\"")),
    ("a muted label beside its bar", |svg| contains(svg, "class=\"pele-label\" fill=\"var(--_m)\"")),
    ("a plain label beside its bar", |svg| contains(svg, "class=\"pele-label\" x=")),
    ("a label of styled runs", |svg| group(svg, "pele-tasks").contains("<tspan")),
    ("a label left of its bar", |svg| {
        // A label that ends left of where its bar starts: Pele puts it there only when it does not fit on the right.
        let tasks = group(svg, "pele-tasks");
        tasks.split("<g class=\"pele-task").skip(1).any(|task| {
            let bar = task
                .split("<rect x=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .and_then(|x| x.parse::<f64>().ok());
            let label = task
                .split("text-anchor")
                .next()
                .and_then(|head| head.rsplit(" x=\"").next())
                .and_then(|rest| rest.split('"').next())
                .and_then(|x| x.parse::<f64>().ok());
            matches!((bar, label), (Some(bar), Some(label)) if label < bar)
        })
    }),
    ("excluded days", |svg| contains(svg, "class=\"pele-excluded\"")),
    ("a vertical marker", |svg| contains(svg, "class=\"pele-vert\"")),
    ("a vertical marker label", |svg| contains(svg, "class=\"pele-vert-label\"")),
    ("vertical marker labels on two lines", |svg| {
        let labels = group(svg, "pele-vert-labels");
        let ys: BTreeSet<&str> = labels.split(" y=\"").skip(1).filter_map(|rest| rest.split('"').next()).collect();
        ys.len() > 1
    }),
    ("a today marker", |svg| contains(svg, "class=\"pele-today\"")),
    ("a today marker's head", |svg| group(svg, "pele-today").contains("Z\" fill=\"var(--_c)\"")),
    ("a styled today marker", |svg| contains(svg, "stroke-width=\"2\" style=")),
    ("a top axis", |svg| axis_line(svg).matches('M').count() > 1),
    ("an axis label left out to keep labels apart", |svg| {
        let ticks = grid_lines(svg).matches('V').count();
        let labels = group(svg, "pele-axis").matches("<text").count();
        let rows = if axis_line(svg).matches('M').count() > 1 { 2 } else { 1 };
        labels > 0 && labels / rows < ticks
    }),
    ("grid lines that start above the first row", |svg| {
        let top = grid_lines(svg).split(',').nth(1).and_then(|y| y.split('V').next()?.parse::<f64>().ok());
        let row = first(group(svg, "pele-sections"), "y");
        matches!((top, row), (Some(top), Some(row)) if top < row)
    }),
    ("two tasks on one row", |svg| {
        let tasks = group(svg, "pele-tasks");
        let ys: Vec<&str> = tasks
            .split("<rect x=\"")
            .skip(1)
            .filter_map(|rest| rest.split(" y=\"").nth(1)?.split('"').next())
            .collect();
        ys.iter().collect::<BTreeSet<_>>().len() < ys.len()
    }),
    ("a chart narrowed to the host's width", |svg| first(svg, "width").is_some_and(|width| width < 700.0)),
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
                    *if r#type == "gantt" { &mut self.identical } else { &mut self.other_types } += 1;
                    if r#type == "gantt" {
                        self.reached.extend(PATHS.iter().filter(|(_, shows)| shows(svg)).map(|(name, _)| *name));
                    }
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "gantt" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "gantt" => {
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
                Some(format!("pele rejects a gantt chart, ours reports it as not ported: {error}"))
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

/// The widths a host may have, each under the 800 pixels a gantt chart is drawn to by default.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested gantt sources");
    assert!(tally.identical > 1000, "the harvest holds gantt charts Pele renders");
    assert!(tally.rejected > 100, "the harvest holds gantt charts Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested gantt sources at maxWidth {width}"));
        println!("maxWidth {width} makes Pele draw {} of the harvested sources another way", redrawn(&cases));
    }
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

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated gantt sources");
}

const HEAD: &str = "gantt\n  dateFormat YYYY-MM-DD\n";
const TASKS: &str = "  section S\n  A task :a1, 2014-01-01, 30d\n  Another :after a1, 20d\n";

/// Hand-written charts for what the harvest reaches least, each with the host options that reach it.
fn cases() -> Vec<(String, Value)> {
    let chart = format!(
        "{HEAD}  title A chart\n  todayMarker stroke-width:5px,stroke:#0f0\n  excludes weekends\n{TASKS}  section T\n  Crit :crit, c1, after a1, 5d\n  Done :done, crit, d1, 2014-01-10, 3d\n  Mile :milestone, m1, 2014-02-01, 0d\n  Vert :vert, v1, 2014-01-15, 0d\n  Vert two :vert, v2, 2014-01-16, 0d\n  click a1 href \"https://example.com\"\n"
    );
    let options = [
        json!({}),
        json!({ "now": 1_389_873_600_000.0_f64 }),
        json!({ "maxWidth": 320 }),
        json!({ "maxWidth": 100 }),
        json!({ "padding": 0, "fontSize": 13 }),
        json!({ "padding": 20.5, "fontSize": 20 }),
        json!({ "responsive": false, "idPrefix": "a\"b" }),
        json!({ "links": false }),
        json!({ "linkRel": "" }),
        json!({ "linkRel": "noopener noreferrer" }),
        json!({ "config": { "gantt": { "topPadding": 120, "gridLineStartPadding": 35, "topAxis": true } } }),
        json!({ "config": { "gantt": { "barHeight": 40, "barGap": 0, "fontSize": 9, "sectionFontSize": 18, "numberSectionStyles": 3 } } }),
        json!({ "config": { "gantt": { "leftPadding": 0, "rightPadding": 0, "useWidth": 2000 } } }),
        json!({ "config": { "gantt": { "axisFormat": "%a %d", "tickInterval": "1day", "weekday": "monday", "displayMode": "compact" } } }),
        json!({ "config": { "gantt": { "useMaxWidth": false } } }),
    ];
    let mut out: Vec<(String, Value)> = options.iter().map(|options| (chart.clone(), options.clone())).collect();
    let more = [
        format!("{HEAD}{TASKS}"),
        "gantt\n  title Only a title\n".to_owned(),
        "gantt\n".to_owned(),
        "gantt\n  A :a, after b, 1d\n  B :b, after a, 1d\n".to_owned(),
        "gantt\n  A :vert, 2014-01-01, 1d\n".to_owned(),
        format!("{HEAD}  A :a, 2014-01-01, 1d\n  B :b, 2014-01-01, 1d\n  C :c, 2014-01-01, 1d\n"),
        format!(
            "---\ndisplayMode: compact\n---\n{HEAD}  section S\n  A :a, 2014-01-01, 3d\n  B :b, 2014-01-02, 3d\n  C :c, 2014-01-05, 3d\n  D :d, after zz, 1d\n"
        ),
        format!("{HEAD}  axisFormat %Y\n  A :a, 2014-01-01, 3d\n"),
        format!("{HEAD}  tickInterval 1millisecond\n  A :a, 2014-01-01, 1d\n"),
        format!("{HEAD}  tickInterval 2week\n  weekday friday\n  A :a, 2014-01-01, 90d\n"),
        format!("{HEAD}  excludes 2014-01-03, saturday\n  includes 2014-01-04\n  A :a, 2014-01-01, 10d\n"),
        format!("{HEAD}  excludes weekends\n  weekend friday\n  A :a, 2014-01-01, 10d\n"),
        format!("{HEAD}  A :a, 2014-01-01, 20000d\n  excludes weekends\n"),
        format!("{HEAD}  todayMarker off\n  A :a, 2026-01-01, 30d\n"),
        format!("{HEAD}  todayMarker  off\n  A :a, 2026-01-01, 30d\n"),
        format!("{HEAD}  A long name for a short task in the middle :a, 2014-01-10, 1d\n  B :b, 2014-01-01, 30d\n"),
        format!("{HEAD}  section {}\n  A :a, 2014-01-01, 1d\n", "very long section name ".repeat(12)),
        format!("{HEAD}  accTitle: The <title> & more\n  accDescr: A \"description\"\n  A :a, 2014-01-01, 1d\n"),
        format!("{HEAD}  A :a, 2014-01-01, 1d\n  click a href \"javascript:alert(1)\"\n"),
        format!("{HEAD}  A <b>bold</b> and <code>code</code> :a, 2014-01-01, 30d\n"),
        format!("{HEAD}  section\n  A :a, 2014-01-01, 3d\n"),
        "gantt\n  dateFormat x\n  inclusiveEndDates\n  A :a, 0, 8640000000000000\n".to_owned(),
    ];
    out.extend(more.iter().flat_map(|source| {
        [json!({}), json!({ "maxWidth": 320 }), json!({ "fontSize": 13.5 })].map(|options| (source.clone(), options))
    }));
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("gantt cases");
    assert!(tally.identical > 60, "the cases render");
}

/// Pele's own worst cases (`tests/gantt/stress.test.ts`), each with the source limit raised.
fn worst_cases() -> Vec<(String, Value)> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let big = json!({ "limit": 10_000_000, "now": 1_705_276_800_000.0_f64 });
    let head = "gantt\ndateFormat YYYY-MM-DD\n";
    [
        format!("{head}{}", repeat(8000, &|i| format!("T{i} :2024-01-01, {}d\n", 1 + i % 30))),
        format!("{head}A :t0, 2024-01-01, 1d\n{}", repeat(5000, &|i| format!("T :t{}, after t{i}, 1d\n", i + 1))),
        format!("{head}{}Z :t3000, 2024-01-01, 1d\n", repeat(3000, &|i| format!("T :t{i}, after t{}, 1d\n", i + 1))),
        format!("{head}A :a, 2024-01-01, 1d\nB :b, after{}, 1d\n", " a zz".repeat(6000)),
        format!("{head}{}", repeat(5000, &|i| format!("section S{i}\nT :2024-01-01, 2d\n"))),
        format!("---\ndisplayMode: compact\n---\n{head}{}", repeat(6000, &|i| format!("T :2024-01-01, {}d\n", i + 1))),
        format!("{head}{}:2024-01-01, 2d\n", "word ".repeat(10000)),
        format!("{head}excludes weekends\n{}", repeat(400, &|_| "T :2024-01-01, 10950d\n".to_owned())),
        format!(
            "{head}excludes weekends monday tuesday wednesday thursday friday\n{}",
            repeat(2000, &|_| "T :2024-01-01, 7d\n".to_owned())
        ),
        format!("{head}excludes weekends\nT :2024-01-01, 99999999d\n"),
        format!("{head}A :0001-01-01, 1d\nB :2024-01-01, 99990000d\n"),
        format!("{head}tickInterval 1millisecond\nA :1950-01-01, 36500d\n"),
        format!("{head}tickInterval 99999999second\nA :1950-01-01, 36500d\n"),
        format!("{head}tickInterval 100000week\nA :0100-01-01, 99000000d\n"),
        format!("{head}axisFormat {}\nA :2024-01-01, 30d\n", "%c%j%U%V".repeat(3000)),
        format!("{head}todayMarker {}\nA :2024-01-01, 30d\n", "stroke:red,".repeat(4500)),
        format!(
            "{head}{}{}",
            repeat(2000, &|i| format!("T :t{i}, 2024-01-01, 2d\n")),
            repeat(2000, &|i| format!("click t{i} href \"https://example.com/{i}\"\n"))
        ),
        format!("{head}T :t, 2024-01-01, 2d\nclick {}t call f()\n", "t,".repeat(12000)),
    ]
    .into_iter()
    .map(|source| (source, big.clone()))
    .collect()
}

#[test]
fn worst_cases_match_pele() {
    run("worst", &worst_cases()).finish("Pele's worst-case gantt charts");
}

#[test]
fn random_charts_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_gantt(&mut rng), random_options(&mut rng))).collect();
    let tally = run("random", &cases).finish("random gantt charts");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random chart's output has {missed:?}");
}
