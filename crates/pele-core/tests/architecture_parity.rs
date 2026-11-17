//! Differential test for drawing architecture diagrams: every source must come out of this port
//! as it comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source
//! Pele rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/architecture/harvest.json` holds the architecture sources of Pele's own test
//! suite, the inputs of its mutated and generated runs and its worst cases, written by
//! `scripts/harvest-architecture.mjs`. The widths put every diagram wider than the host through
//! Pele's tight drawing. The random diagrams carry the proof for what the six corpus sources
//! never draw, and must reach every drawing path in [`PATHS`].

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/architecture/harvest.json");
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

/// Each element that starts with `start`, up to the end of its opening tag's group.
fn groups<'a>(svg: &'a str, start: &str) -> Vec<&'a str> {
    svg.split(start).skip(1).map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())]).collect()
}

/// The value of attribute `name` in the first tag of `text`.
fn attribute(text: &str, name: &str) -> Option<f64> {
    let (_, rest) = text.split_once(&format!(" {name}=\""))?;
    rest[..rest.find('"')?].parse().ok()
}

/// The frames of the groups as x, y, w, h.
fn clusters(svg: &str) -> Vec<[f64; 4]> {
    groups(svg, "<g class=\"pele-cluster\"")
        .into_iter()
        .filter_map(|group| {
            let rect = &group[group.find("<rect")?..];
            Some([attribute(rect, "x")?, attribute(rect, "y")?, attribute(rect, "width")?, attribute(rect, "height")?])
        })
        .collect()
}

/// The `d` of each edge's line.
fn edge_paths(svg: &str) -> Vec<&str> {
    groups(svg, "<g class=\"pele-edge\"")
        .into_iter()
        .filter_map(|edge| edge.split_once("<path d=\"").map(|(_, d)| &d[..d.find('"').unwrap_or(0)]))
        .collect()
}

fn icon_with(svg: &str, glyph: &str) -> bool {
    svg.split("<svg class=\"pele-icon\"").skip(1).any(|icon| icon[..icon.find("</svg>").unwrap_or(0)].contains(glyph))
}

/// The drawing paths a source can go through, each with what in the source and Pele's SVG shows
/// it did.
const PATHS: &[(&str, Reached)] = &[
    ("a group", |_, svg| svg.contains("<g class=\"pele-cluster\"")),
    ("a group inside a group", |_, svg| {
        let frames = clusters(svg);
        frames.iter().any(|outer| {
            frames.iter().any(|inner| {
                inner != outer
                    && outer[0] <= inner[0]
                    && outer[1] <= inner[1]
                    && outer[0] + outer[2] >= inner[0] + inner[2]
                    && outer[1] + outer[3] >= inner[1] + inner[3]
            })
        })
    }),
    ("a group with a title", |_, svg| svg.contains("class=\"pele-cluster-label\"")),
    ("a group with an icon", |_, svg| {
        groups(svg, "<g class=\"pele-cluster\"").iter().any(|group| group.contains("pele-icon"))
    }),
    ("a group with only an icon", |_, svg| {
        groups(svg, "<g class=\"pele-cluster\"")
            .iter()
            .any(|group| group.contains("pele-icon") && !group.contains("pele-cluster-label"))
    }),
    ("a group title moved clear of an edge", |_, svg| {
        groups(svg, "<g class=\"pele-cluster\"").iter().any(|group| {
            let x = attribute(group, "x");
            let label = group.split_once("class=\"pele-cluster-label\"").map(|(_, label)| label);
            let icon = group.split_once("<svg class=\"pele-icon\"").map(|(_, icon)| icon);
            let start = icon.and_then(|icon| attribute(icon, "x"));
            matches!((x, start), (Some(x), Some(start)) if (start - x - 10.0).abs() > 0.01) || label.is_none()
        }) && svg.contains("<g class=\"pele-edge\"")
    }),
    ("an empty group", |source, svg| {
        source.lines().any(|line| line.starts_with("group ")) && svg.contains("<g class=\"pele-cluster\"") && {
            let ids: Vec<&str> = source
                .lines()
                .filter_map(|line| line.strip_prefix("group "))
                .filter_map(|rest| rest.split([' ', '(', '[']).next())
                .collect();
            ids.iter().any(|id| !source.contains(&format!(" in {id}\n")) && !source.ends_with(&format!(" in {id}")))
        }
    }),
    ("a service", |_, svg| svg.contains("<g class=\"pele-node pele-service\"")),
    ("a junction", |_, svg| svg.contains("<g class=\"pele-node pele-junction\"")),
    ("a database glyph", |_, svg| icon_with(svg, "M4 6a8 3")),
    ("a server glyph", |_, svg| icon_with(svg, "M3 4h18v7H3z")),
    ("a disk glyph", |_, svg| icon_with(svg, "M5 3h14v18H5z")),
    ("an internet glyph", |_, svg| icon_with(svg, "M3 12a9 9")),
    ("a cloud glyph", |_, svg| icon_with(svg, "M7 18a4 4")),
    ("an icon with no glyph", |_, svg| svg.contains("stroke-linejoin=\"round\"></svg>")),
    ("text in place of an icon", |_, svg| svg.contains("class=\"pele-icon-text\"")),
    ("icon text cut short", |_, svg| {
        layer(svg, "class=\"pele-icon-text\"").split("</text>").next().is_some_and(|text| text.contains('\u{2026}'))
    }),
    ("a service title over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-service\"").iter().any(|node| {
            node.split_once("class=\"pele-label\"").is_some_and(|(_, label)| label.matches("<tspan x=").count() > 1)
        })
    }),
    ("a title with markdown", |_, svg| layer(svg, "<g class=\"pele-nodes\">").contains("font-weight=\"var(--_w)\"")),
    ("an edge", |_, svg| svg.contains("<g class=\"pele-edge\"")),
    ("an arrowhead at the start", |source, svg| source.contains(" <-") && svg.contains("class=\"pele-marker\"")),
    ("an arrowhead at the end", |source, svg| source.contains("-> ") && svg.contains("class=\"pele-marker\"")),
    ("arrowheads at both ends", |_, svg| {
        groups(svg, "<g class=\"pele-edge\"").iter().any(|edge| edge.matches("pele-marker").count() == 2)
    }),
    ("a straight edge", |_, svg| edge_paths(svg).iter().any(|d| !d.contains('Q') && d.matches('L').count() == 1)),
    ("an edge with a corner", |_, svg| edge_paths(svg).iter().any(|d| d.contains('Q'))),
    ("an edge round both nodes", |_, svg| edge_paths(svg).iter().any(|d| d.matches('Q').count() >= 4)),
    ("an edge back to the same side", |source, svg| {
        source.lines().any(|line| {
            let parts: Vec<&str> = line.split(':').collect();
            parts.len() == 3
                && parts[0].trim_end_matches("{group}") == parts[2].trim_end_matches("{group}").trim()
                && parts[1].trim_start().chars().next() == parts[1].trim_end().chars().last()
        }) && svg.contains("<g class=\"pele-edge\"")
    }),
    ("an edge through a group's border", |source, svg| {
        source.contains("{group}") && svg.contains("<g class=\"pele-edge\"")
    }),
    ("a junction reached through its group's border", |source, svg| {
        source.contains("{group}") && source.contains("junction ") && svg.contains("<g class=\"pele-edge\"")
    }),
    ("an edge label", |_, svg| svg.contains("<g class=\"pele-edge-label\"")),
    ("an edge label over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-edge-label\"").iter().any(|label| label.matches("<tspan x=").count() > 1)
    }),
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("an accessible title and description", |_, svg| {
        svg.contains("aria-labelledby=") && svg.contains("aria-describedby=")
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
                    *if r#type == "architecture" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "architecture" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "architecture" => {
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
                Some(format!("pele rejects an architecture diagram, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            let shown: String = source.chars().take(600).collect();
            self.failures.push(format!("{name}: {problem}\n    source: {shown:?}"));
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
/// spare, which for architecture is the tight drawing.
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested architecture sources");
    assert!(tally.identical > 1000, "the harvest holds architecture diagrams Pele renders");
    assert!(tally.rejected > 500, "the harvest holds architecture diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested architecture sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated architecture sources");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> =
        harvest("worst").into_iter().map(|source| (source, json!({ "limit": 100_000_000 }))).collect();
    let tally = run("worst", &cases).finish("Pele's worst-case architecture diagrams");
    assert!(tally.identical > 30, "the worst cases are drawn, not rejected");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let groups = "architecture-beta\ntitle Platform\naccTitle: Platform\naccDescr: The services and their stores\ngroup api(cloud)[API]\ngroup inner(server)[Inner services] in api\nservice db(database)[Database] in inner\nservice disk(disk)[Storage] in api\nservice web(internet)[Web] \nservice gw(logos:aws)[Gateway] in api\njunction j in inner\nweb:R --> L:gw\ngw:B -[reads and writes]- T:db\ndb:R <--> L:disk\ndisk{group}:R -- L:web\nj:T -- B:db\n";
    let text = "architecture-beta\nservice a \"A long text in place of an icon that does not fit\"[A]\nservice b 'x'[B]\nservice c \"\u{1f600}\u{1f600}\u{1f600}\u{1f600}\u{1f600}\u{1f600}\"\na:R -- L:b\nb:R -- L:c";
    let wide = "architecture-beta\ngroup g[A group title much wider than everything the group holds, as wide as can be]\nservice a[A] in g\nservice b[A title that wraps over a few lines because it is long] in g\na:R -[an edge label that is long enough to wrap onto lines]-> L:b";
    let mut out = vec![
        (groups.to_owned(), json!({})),
        (groups.to_owned(), json!({ "maxWidth": 300 })),
        (groups.to_owned(), json!({ "maxWidth": 600, "padding": 0 })),
        (groups.to_owned(), json!({ "fontSize": 12 })),
        (groups.to_owned(), json!({ "fontSize": 22, "maxWidth": 500 })),
        (groups.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (text.to_owned(), json!({})),
        (wide.to_owned(), json!({})),
        (wide.to_owned(), json!({ "maxWidth": 320 })),
        ("architecture-beta".to_owned(), json!({})),
        ("architecture-beta\ngroup g".to_owned(), json!({})),
        ("architecture-beta\ntitle Only a title".to_owned(), json!({})),
        ("architecture-beta\nservice a".to_owned(), json!({ "outputLimit": 200 })),
        ("architecture-beta\nservice a".to_owned(), json!({ "limit": 10 })),
    ];
    for config in [
        json!({"architecture": {"useMaxWidth": false}}),
        json!({"architecture": "x"}),
        json!({"architecture": {"iconSize": 20, "fontSize": 10, "padding": 4}}),
        json!({"architecture": {"iconSize": 2000, "fontSize": 1, "padding": -5}}),
        json!({"architecture": {"iconSize": "48", "fontSize": null}}),
    ] {
        out.push((groups.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("architecture cases");
}

/// xorshift64, so the diagrams are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const SIDES: [&str; 4] = ["L", "R", "T", "B"];
const ICONS: &[&str] = &["(database)", "(server)", "(disk)", "(internet)", "(cloud)", "(logos:aws-s3)", "(x)"];
const TITLES: &[&str] = &[
    "[A]",
    "[Database]",
    "[Two words]",
    "[A service title that wraps over lines]",
    "[**bold** and _italic_]",
    "[\"quoted \\\" title\"]",
    "[#35; &amp; <b>x</b>]",
    "[\u{e9}\u{91c7}\u{1f600}]",
];
const TEXTS: &[&str] =
    &["\"S3\"", "'db'", "\"A long text in place of an icon\"", "\"\u{1f600}\u{1f600}\u{1f600}\u{1f600}\u{1f600}\""];
const LABELS: &[&str] = &["[reads]", "[a label long enough to wrap onto a second line]", "[x]", "[**b**]"];
const GROUP_TITLES: &[&str] = &["[API]", "[Backend services]", "[A group title much wider than its group]", "[_i_]"];

/// A diagram the db accepts, with titles, icons, text in place of icons, edge labels and
/// arrowheads, edges through group borders, and alignments.
fn random_diagram(rng: &mut Rng) -> String {
    let mut src = String::from("architecture-beta\n");
    if rng.chance(15) {
        src += &format!("title {}\n", rng.pick(&["Platform", "A title wider than most of these diagrams are, surely"]));
    }
    if rng.chance(8) {
        src += "accTitle: Acc\naccDescr: What it shows\n";
    }
    let groups = rng.below(6);
    let nodes = 1 + rng.below(12);
    for g in 0..groups {
        let icon = if rng.chance(40) { rng.pick(ICONS) } else { "" };
        let title = if rng.chance(60) { rng.pick(GROUP_TITLES) } else { "" };
        let parent = if g > 0 && rng.chance(50) { format!(" in g{}", rng.below(g)) } else { String::new() };
        src += &format!("group g{g}{icon}{title}{parent}\n");
    }
    let mut home = Vec::new();
    let mut junctions = Vec::new();
    for i in 0..nodes {
        let g = (groups > 0 && rng.chance(65)).then(|| rng.below(groups));
        let place = g.map_or(String::new(), |g| format!(" in g{g}"));
        if rng.chance(15) {
            src += &format!("junction n{i}{place}\n");
            junctions.push(i);
        } else {
            let look = match rng.below(10) {
                0..=4 => rng.pick(ICONS).to_owned(),
                5..=6 => format!(" {}", rng.pick(TEXTS)),
                _ => String::new(),
            };
            let title = if rng.chance(70) { rng.pick(TITLES) } else { "" };
            src += &format!("service n{i}{look}{title}{place}\n");
        }
        home.push(g);
    }
    for _ in 0..rng.below(nodes * 2 + 1) {
        let (a, b) = (rng.below(nodes), rng.below(nodes));
        let sa = SIDES[rng.below(4)];
        let sb = if rng.chance(15) { sa } else { SIDES[rng.below(4)] };
        let crosses = home[a].is_none() || home[a] != home[b];
        let into_a = ["", "", "<"][rng.below(3)];
        let into_b = ["", ">", ">"][rng.below(3)];
        let line = if rng.chance(25) { format!("-{}-", rng.pick(LABELS)) } else { "--".to_owned() };
        let group_a = if crosses && home[a].is_some() && rng.chance(25) { "{group}" } else { "" };
        let group_b = if crosses && home[b].is_some() && rng.chance(25) { "{group}" } else { "" };
        src += &format!("n{a}{group_a}:{sa} {into_a}{line}{into_b} {sb}:n{b}{group_b}\n");
    }
    if nodes > 2 && rng.chance(25) {
        let (a, b) = (rng.below(nodes), rng.below(nodes));
        if a != b {
            src += &format!("align {} n{a} n{b}\n", if rng.chance(50) { "row" } else { "column" });
        }
    }
    src
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x00a2_c41e_c7de_5eed);
    let mut cases = Vec::new();
    for _ in 0..4000 {
        let source = random_diagram(&mut rng);
        let options = match rng.below(6) {
            0 => json!({ "maxWidth": 320 }),
            1 => {
                let width = [480, 640, 900][rng.below(3)];
                json!({ "maxWidth": width })
            }
            _ => json!({}),
        };
        cases.push((source, options));
    }
    let tally = run("random", &cases).finish("random architecture diagrams");
    assert!(tally.identical > 3500, "random diagrams are ones Pele draws");
    let redrawn = redrawn_to_fit(
        &cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect::<Vec<_>>(),
    );
    println!("their widths make Pele draw {redrawn} of the random architecture diagrams another way");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random architecture diagram reaches {missed:?}");
}
