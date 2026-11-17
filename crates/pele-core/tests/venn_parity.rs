//! Differential test for drawing venn diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/venn/harvest.json` holds the venn sources of Pele's own test suite and the
//! inputs of its mutated and generated runs, written by `scripts/harvest-venn.mjs`. A venn
//! diagram is drawn the same at any host width, which the width test shows. The random diagrams
//! carry the proof for what the eight corpus sources never draw, and must reach every drawing
//! path in [`PATHS`].

use pele_core::venn_probe::{self, TrigCall};
use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/venn/harvest.json");
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

/// The part of the drawing inside the layer that opens with `start`.
fn layer<'a>(svg: &'a str, start: &str) -> &'a str {
    svg.split_once(start).map_or("", |(_, rest)| rest)
}

/// The circles of the sets as x, y and r.
fn circles(svg: &str) -> Vec<[f64; 3]> {
    let attribute = |text: &str, name: &str| -> f64 {
        let (_, rest) = text.split_once(&format!(" {name}=\"")).expect("a circle has its geometry");
        rest[..rest.find('"').expect("a closing quote")].parse().expect("a number")
    };
    svg.split("<circle class=\"pele-venn-set\"")
        .skip(1)
        .map(|rest| {
            let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
            [attribute(tag, "cx"), attribute(tag, "cy"), attribute(tag, "r")]
        })
        .collect()
}

fn pairs(svg: &str, test: impl Fn([f64; 3], [f64; 3], f64) -> bool) -> bool {
    let circles = circles(svg);
    circles.iter().enumerate().any(|(i, &a)| {
        circles[i + 1..].iter().any(|&b| test(a, b, ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()))
    })
}

/// The `d` of each overlap's outline.
fn outlines(svg: &str) -> Vec<&str> {
    svg.split("<path class=\"pele-venn-overlap\"")
        .skip(1)
        .filter_map(|rest| rest.split_once(" d=\"").map(|(_, d)| &d[..d.find('"').unwrap_or(0)]))
        .collect()
}

/// The groups of the regions' text.
fn regions(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-venn-region\"")
        .skip(1)
        .map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())])
        .collect()
}

/// The drawing paths a source can go through, each with what in the source and Pele's SVG shows
/// it did.
const PATHS: &[(&str, Reached)] = &[
    ("one set", |_, svg| circles(svg).len() == 1),
    ("two sets", |_, svg| circles(svg).len() == 2),
    ("three sets", |_, svg| circles(svg).len() == 3),
    ("four to 64 sets on a ring", |_, svg| (4..=64).contains(&circles(svg).len())),
    ("more than 64 sets on a grid", |_, svg| circles(svg).len() > 64),
    ("series colours past the eighth", |_, svg| svg.matches("fill=\"var(--pele-series-1,").count() > 1),
    ("sets that do not overlap", |_, svg| pairs(svg, |a, b, d| d > a[2] + b[2] + 0.5)),
    ("sets that overlap", |_, svg| pairs(svg, |a, b, d| d < a[2] + b[2] - 0.5 && d > (a[2] - b[2]).abs() + 0.5)),
    ("a set inside another", |_, svg| pairs(svg, |a, b, d| d + a[2].min(b[2]) < a[2].max(b[2]) - 0.5)),
    ("circles of different sizes", |_, svg| pairs(svg, |a, b, _| (a[2] - b[2]).abs() > 1.0)),
    ("a sized overlap", |source, svg| source.contains("union ") && source.contains("]:") && svg.contains("|")),
    ("a set's own label", |_, svg| regions(svg).iter().any(|r| !r.contains('|') && r.contains("class=\"pele-label\""))),
    ("an overlap label", |_, svg| {
        svg.split("<g class=\"pele-venn-region\" data-id=\"")
            .skip(1)
            .any(|r| r[..r.find('"').unwrap_or(0)].contains('|'))
    }),
    ("an overlap of three or more sets with a label", |_, svg| {
        svg.split("<g class=\"pele-venn-region\" data-id=\"")
            .skip(1)
            .any(|r| r[..r.find('"').unwrap_or(0)].matches('|').count() > 1)
    }),
    ("an overlap label in the smaller font", |_, svg| {
        layer(svg, "class=\"pele-venn-labels\"").contains("class=\"pele-label\" font-size=")
    }),
    ("a text in a set", |_, svg| {
        regions(svg).iter().any(|r| !r[..r.find('>').unwrap_or(0)].contains('|') && r.contains("pele-venn-text"))
    }),
    ("a text in an overlap", |_, svg| {
        regions(svg).iter().any(|r| r[..r.find('>').unwrap_or(0)].contains('|') && r.contains("pele-venn-text"))
    }),
    ("a muted text", |_, svg| svg.contains("class=\"pele-venn-text\"") && svg.contains("fill=\"var(--_m)\"")),
    ("a text with a style of its own", |_, svg| {
        svg.split("class=\"pele-venn-text\"").skip(1).any(|t| t[..t.find('>').unwrap_or(0)].contains(" style=\""))
    }),
    ("several lines in one region", |_, svg| regions(svg).iter().any(|r| r.matches("<text").count() > 1)),
    ("a label wrapped to its region", |_, svg| layer(svg, "class=\"pele-venn-labels\"").contains("<tspan")),
    ("a bold label", |_, svg| layer(svg, "class=\"pele-venn-labels\"").contains("font-weight=\"var(--_w)\"")),
    ("an italic label", |_, svg| layer(svg, "class=\"pele-venn-labels\"").contains("font-style=\"italic\"")),
    ("a styled set", |_, svg| {
        svg.split("<circle class=\"pele-venn-set\"")
            .skip(1)
            .any(|c| c[..c.find('>').unwrap_or(0)].contains(" style=\""))
    }),
    ("a styled overlap drawn as an outline", |_, svg| !outlines(svg).is_empty()),
    ("an outline with corners", |_, svg| outlines(svg).iter().any(|d| d.contains(" 0 0 1 "))),
    ("an outline that is a whole circle", |_, svg| outlines(svg).iter().any(|d| d.matches(" 0 1 1 ").count() == 2)),
    ("an outline of three or more circles", |_, svg| outlines(svg).iter().any(|d| d.matches('A').count() > 2)),
    ("a styled overlap the circles do not share", |source, svg| {
        source.contains("style ") && source.contains(',') && svg.contains("|") && outlines(svg).is_empty() && {
            let data = |svg: &str| {
                svg.split("data-id=\"").skip(1).filter(|r| r[..r.find('"').unwrap_or(0)].contains('|')).count()
            };
            data(svg) > 0 && !svg.contains("pele-venn-overlap")
        }
    }),
    ("label ink on a dark fill", |_, svg| layer(svg, "class=\"pele-venn-labels\"").contains("var(--pele-fg-on-dark")),
    ("label ink on a light fill", |_, svg| layer(svg, "class=\"pele-venn-labels\"").contains("var(--pele-fg-on-light")),
    ("an overlap with nothing written in it", |source, svg| {
        source.lines().filter_map(|line| line.strip_prefix("union ")).any(|ids| {
            let mut ids: Vec<&str> = ids.trim().split(',').collect();
            ids.sort_unstable();
            ids.dedup();
            !ids.iter().any(|id| id.contains(['[', ':', '"'])) && ids.len() > 1 && {
                let id = ids.join("|");
                svg.contains(&format!("<circle class=\"pele-venn-set\" data-id=\"{}\"", ids[0]))
                    && !svg.contains(&format!("<g class=\"pele-venn-region\" data-id=\"{id}\""))
            }
        })
    }),
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("a palette the source sets", |_, svg| svg.contains(";--pele-series-")),
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
    /// Sources whose drawing differs from Pele's, but is Pele's once the layout is given Node's
    /// `Math` answers.
    trig_only: Vec<String>,
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
                    *if r#type == "venn" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "venn" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "venn" => {
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
                Some(format!("pele rejects a venn diagram, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {source:?}"));
        }
    }

    fn finish(self, what: &str) -> Self {
        let total = self.identical + self.rejected + self.other_types + self.trig_only.len() + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {} rejected with Pele's message, {} of another type, {} identical only with Node's Math answers, {} different",
            self.identical,
            self.rejected,
            self.other_types,
            self.trig_only.len(),
            self.failures.len()
        );
        for source in &self.trig_only {
            println!("    libm's last bit reaches the drawing of {source:?}");
        }
        assert!(
            self.failures.is_empty(),
            "{} of {total} {what} differ from Pele:\n{}",
            self.failures.len(),
            self.failures[..self.failures.len().min(8)].join("\n")
        );
        self
    }
}

/// Node's answer to each call, asked of the oracle's `sin`, `cos`, `acos` and `atan2` ops.
fn ask_node(calls: &[TrigCall], answers: &mut HashMap<TrigCall, f64>) {
    let requests: Vec<String> = calls
        .iter()
        .map(|call| {
            format!("{}\t{}", call.op, call.args.iter().map(|&x| format!("{x:016x}")).collect::<Vec<_>>().join("\t"))
        })
        .collect();
    for (call, answer) in calls.iter().zip(pele_oracle::ask(&requests)) {
        answers.insert(call.clone(), f64::from_bits(u64::from_str_radix(&answer, 16).expect("bits")));
    }
}

/// Each source's SVG with every `Math` call of the layout answered by Node. A call that is only
/// made once an earlier one has Node's answer is asked in the next round.
fn svgs_with_node_trig(sources: &[&String]) -> Vec<Option<String>> {
    let mut answers = HashMap::new();
    loop {
        let mut missed: Vec<TrigCall> = Vec::new();
        let svgs: Vec<Option<String>> = sources
            .iter()
            .map(|source| {
                let (rendered, calls) = venn_probe::render_answered(source, &answers);
                missed.extend(calls);
                rendered.ok().map(|rendered| rendered.svg)
            })
            .collect();
        if missed.is_empty() {
            return svgs;
        }
        missed.sort_by(|a, b| (a.op, &a.args).cmp(&(b.op, &b.args)));
        missed.dedup();
        ask_node(&missed, &mut answers);
    }
}

/// Renders each case through both engines. A panic in this port is a failure of that case. A
/// source drawn with the default options that differs is drawn again with Node's `Math` answers,
/// and passes only if that drawing is Pele's byte for byte.
fn run(name: &str, cases: &[(String, Value)]) -> Tally {
    let requests: Vec<(String, String)> =
        cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
    let theirs = pele_oracle::render_all_with(&requests);
    let mut tally = Tally::default();
    // The failures that a drawing with Node's answers may explain: the source and Pele's SVG.
    let mut differing: Vec<(String, &String, &String)> = Vec::new();
    for (index, ((source, options), theirs)) in cases.iter().zip(&theirs).enumerate() {
        let name = format!("{name} {index} with {options}");
        let before = tally.failures.len();
        match std::panic::catch_unwind(|| render_with(source, options)) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
        let plain = options.as_object().is_some_and(serde_json::Map::is_empty);
        if let (true, Some(failure), PeleRender::Svg { svg, .. }) = (plain, tally.failures.get(before), theirs)
            && failure.contains("first differing byte")
        {
            differing.push((tally.failures.pop().expect("the failure above"), source, svg));
        }
    }
    let replayed = svgs_with_node_trig(&differing.iter().map(|(_, source, _)| *source).collect::<Vec<_>>());
    for ((failure, source, svg), ours) in differing.into_iter().zip(replayed) {
        if ours.as_deref() == Some(svg.as_str()) {
            tally.trig_only.push(source.clone());
        } else {
            tally.failures.push(failure);
        }
    }
    tally
}

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

/// The widths a host may have.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested venn sources");
    assert!(tally.identical > 500, "the harvest holds venn diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds venn diagrams Pele rejects");
}

/// Pele draws a venn diagram the same at any width: each width gives the bytes the harvest
/// gives without one.
#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    let unbounded = pele_oracle::render_all(&sources);
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested venn sources at maxWidth {width}"));
        let requests: Vec<(String, String)> =
            cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
        let redrawn = pele_oracle::render_all_with(&requests)
            .iter()
            .zip(&unbounded)
            .filter(|(bounded, unbounded)| match (bounded, unbounded) {
                (PeleRender::Svg { svg: a, .. }, PeleRender::Svg { svg: b, .. }) => a != b,
                _ => false,
            })
            .count();
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
        assert_eq!(redrawn, 0, "a venn diagram does not depend on the host's width");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated venn sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it: palettes, font sizes, padding, ids, and the cases Pele's render test builds from
/// numbers.
fn cases() -> Vec<(String, Value)> {
    let three = "venn-beta\ntitle Three\nset A[\"Alpha\"]:20\n  text a1[\"React\"]\nset B:12\nset C\nunion A,B[\"AB\"]:3\nunion A,B,C[\"all\"]\n  text abc\nstyle A fill:#1f3a5f, color:#eee\nstyle A,B fill:gold\nstyle a1 color:red";
    let mut out = vec![
        (three.to_owned(), json!({})),
        (three.to_owned(), json!({ "fontSize": 12 })),
        (three.to_owned(), json!({ "fontSize": 22, "padding": 0 })),
        (three.to_owned(), json!({ "idPrefix": "x\"<y", "responsive": false })),
        (three.to_owned(), json!({ "config": { "themeVariables": { "venn1": "red", "venn3": "#000" } } })),
        (three.to_owned(), json!({ "config": { "themeVariables": { "venn2": "red", "venn4": "blue" } } })),
        (
            three.to_owned(),
            json!({ "config": { "themeVariables": { "venn1": "", "venn2": "rgb(1, 2, 3)", "venn5": "nope" } } }),
        ),
        (three.to_owned(), json!({ "config": { "venn": { "useMaxWidth": false } } })),
        ("%%{init: {\"themeVariables\": {\"venn1\": \"#123456\"}}}%%\nvenn-beta\nset A\nset B".to_owned(), json!({})),
        ("---\nconfig:\n  themeVariables:\n    venn8: navy\n---\nvenn-beta\nset A\nset B".to_owned(), json!({})),
        ("---\ntitle: \"Front\"\n---\nvenn-beta\nset A".to_owned(), json!({})),
        ("venn-beta".to_owned(), json!({})),
        ("venn-beta\ntitle Only a title".to_owned(), json!({})),
        ("venn-beta\nset A\nset B\nunion A,B:0".to_owned(), json!({ "limit": 30 })),
        ("venn-beta\nset A\nset B\nunion A,B[\"x\"]".to_owned(), json!({ "outputLimit": 500 })),
    ];
    for [a, b, shared] in [[20, 12, 3], [10, 10, 2], [30, 5, 1], [8, 40, 6]] {
        out.push((format!("venn-beta\nset A:{a}\nset B:{b}\nunion A,B:{shared}"), json!({})));
    }
    for fill in ["black", "white", "#777", "rgba(0,0,0,0.3)", "hsl(200, 50%, 20%)", "navy", "var(--x)"] {
        for opacity in ["", ", fill-opacity:0.4", ", fill-opacity:1", ", opacity:calc(1)"] {
            out.push((
                format!("venn-beta\nset A\nset B\nunion A,B[\"both\"]\n  text t\nstyle A fill:{fill}{opacity}\nstyle A,B fill:{fill}{opacity}"),
                json!({}),
            ));
        }
    }
    out
}

#[test]
fn hand_written_cases_match_pele() {
    run("case", &cases()).finish("venn cases");
}

/// The shapes of Pele's worst venn cases (`tests/venn/stress.test.ts`) at sizes the default
/// limits allow. Its 40 unions of all 60 sets are unions of 24 of them here: a union of 40 or more
/// of 60 sets never lets the layout settle, and libm's last bit reaches the drawing, which
/// `venn_layout.rs` holds as the one known difference.
fn worst() -> Vec<String> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let v = "venn-beta\n";
    let mut every_pair = format!("{v}{}", repeat(40, &|i| format!("set s{i}\n")));
    for i in 0..40 {
        for j in i + 1..40 {
            every_pair.push_str(&format!("union s{i},s{j}\n"));
        }
    }
    let nonsense = "set A:0\nset B:-1000000000\nset C:9999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999\nset D:.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001\nunion A,B:99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999\nunion C,D:-5\nunion A,B,C,D[\"x\"]:0\n";
    vec![
        format!("{v}{}", repeat(2000, &|i| format!("set s{i}\n"))),
        format!(
            "{v}{}{}",
            repeat(300, &|i| format!("set s{i}\n")),
            repeat(299, &|i| format!("union s{i},s{}\n", i + 1))
        ),
        every_pair,
        format!(
            "{v}{}union {}s299[\"all\"]\n",
            repeat(300, &|i| format!("set s{i}\n")),
            repeat(299, &|i| format!("s{i},"))
        ),
        format!(
            "{v}{}{}",
            repeat(60, &|i| format!("set s{i}\n")),
            repeat(40, &|_| format!("union {}s23\n", repeat(23, &|i| format!("s{i},"))))
        ),
        format!("{v}{}", "set A[\"x\"]:5\n".repeat(3000)),
        format!("{v}set A\nset B\n{}", "union A,B[\"x\"]:1\n".repeat(2500)),
        format!("{v}set A\n{}", "  text t\n".repeat(4000)),
        format!("{v}set A\nset B\nset C\nunion A,B,C\n{}", "  text \"t t\"[\"l\"]\n".repeat(2000)),
        format!("{v}set A\n{}", "style A fill:red, color:blue\n".repeat(1500)),
        format!("{v}set A\nstyle A {}fill:red\n", repeat(4000, &|i| format!("p{i}:1,"))),
        format!("{v}set A\nstyle A fill:{}\n", "a ".repeat(20000)),
        format!("{v}set A\nset B\nset C\nunion A,B,C[\"x\"]\n{}", "style A,B,C fill:gold\n".repeat(2000)),
        format!("{v}set A[\"{}\"]", "word ".repeat(9000)),
        format!("{v}set A[{}]", "a".repeat(40000)),
        format!("{v}set A[\"{}\"]", "#35;&amp;<br>".repeat(3500)),
        format!("{v}{nonsense}"),
        format!("{v}set A:{}\nset B\nunion A,B:{}", "9".repeat(40000), "9".repeat(400)),
        format!("{v}set A\n{}", "union A,A,A,A[\"x\"]\n".repeat(2000)),
    ]
}

#[test]
fn worst_cases_match_pele() {
    run("worst", &plain(worst())).finish("Pele's worst-case venn diagrams");
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

const WORDS: &[&str] = &[
    "Alpha",
    "Beta",
    "Gamma",
    "React",
    "Rust",
    "a",
    "Shared ground",
    "Things both teams own and care for",
    "日本",
    "A much longer label that has to wrap inside its region",
    "#35; &amp;",
    "<b>bold</b>",
    "x_y",
];

/// A random venn diagram over sets that exist, with labels, sizes, texts, styles and a title.
fn random_diagram(rng: &mut Rng) -> String {
    let n = match rng.below(30) {
        0..=2 => 1,
        3..=9 => 2,
        10..=17 => 3,
        18..=23 => 4 + rng.below(4),
        24..=28 => 8 + rng.below(8),
        _ => 60 + rng.below(10),
    };
    let mut src = String::from("venn-beta\n");
    if rng.below(4) == 0 {
        src.push_str(&format!("title {}\n", rng.pick(WORDS)));
    }
    let size = |rng: &mut Rng| match rng.below(8) {
        0..=3 => String::new(),
        4 => format!(":{}", 1 + rng.below(40)),
        5 => format!(":{}.{}", rng.below(20), rng.below(100)),
        6 => [":0", ":-1", ":0.1", ":100", ":500"][rng.below(5)].to_owned(),
        _ => format!(":{}", 5 + rng.below(6)),
    };
    let label = |rng: &mut Rng| match rng.below(4) {
        0 | 1 => String::new(),
        2 => format!("[\"{}\"]", rng.pick(WORDS)),
        _ => format!("[{}]", rng.pick(&["L", "two words", "Ünïcode"])),
    };
    let mut texts = 0;
    let mut text = |rng: &mut Rng, src: &mut String| {
        for _ in 0..rng.below(3) {
            texts += 1;
            let shown = if rng.below(2) == 0 { format!("[\"{}\"]", rng.pick(WORDS)) } else { String::new() };
            src.push_str(&format!("  text t{texts}{shown}\n"));
        }
    };
    for i in 0..n {
        src.push_str(&format!("set S{i}{}{}\n", label(rng), size(rng)));
        if n < 20 {
            text(rng, &mut src);
        }
    }
    let mut overlaps: Vec<Vec<usize>> = Vec::new();
    let unions = if n > 20 {
        rng.below(n / 2)
    } else if n > 1 {
        rng.below(2 * n + 1)
    } else {
        0
    };
    for _ in 0..unions {
        let count = if n >= 3 && rng.below(4) == 0 { 3 + rng.below(n.min(5) - 2) } else { 2 };
        let mut members: Vec<usize> = Vec::new();
        while members.len() < count {
            let pick = rng.below(n);
            if !members.contains(&pick) {
                members.push(pick);
            }
        }
        let ids: Vec<String> = members.iter().map(|i| format!("S{i}")).collect();
        src.push_str(&format!("union {}{}{}\n", ids.join(","), label(rng), size(rng)));
        text(rng, &mut src);
        overlaps.push(members);
    }
    let fills = ["navy", "#1f3a5f", "gold", "white", "#eee", "black", "rgba(0,0,0,0.2)", "#ff6b6b", "red"];
    for _ in 0..rng.below(4) {
        let target = match rng.below(3) {
            0 if !overlaps.is_empty() => {
                let members = &overlaps[rng.below(overlaps.len())];
                members.iter().map(|i| format!("S{i}")).collect::<Vec<_>>().join(",")
            }
            1 if texts > 0 => format!("t{}", 1 + rng.below(texts)),
            _ => format!("S{}", rng.below(n)),
        };
        let mut declarations = vec![format!("fill:{}", fills[rng.below(fills.len())])];
        for extra in [
            "fill-opacity:0.6",
            "fill-opacity:1",
            "color:#333",
            "color:white",
            "font-weight:bold",
            "font-style:italic",
            "stroke-width:3",
            "stroke:black",
        ] {
            if rng.below(5) == 0 {
                declarations.push(extra.to_owned());
            }
        }
        src.push_str(&format!("style {target} {}\n", declarations.join(", ")));
    }
    src
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x0e77_d1a9_5eed_0001);
    let sources: Vec<String> = (0..3000).map(|_| random_diagram(&mut rng)).collect();
    let mut cases = plain(sources);
    for (index, case) in cases.iter_mut().enumerate() {
        if index % 10 == 0 {
            case.1 = json!({ "config": { "themeVariables": { "venn1": "#222", "venn2": "khaki" } } });
        }
    }
    let tally = run("random", &cases).finish("random venn diagrams");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random venn diagram reaches: {}", missed.join(", "));
}
