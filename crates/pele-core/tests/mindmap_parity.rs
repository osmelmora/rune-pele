//! Differential test for drawing mindmaps: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele rejects
//! must be rejected here with Pele's message.
//!
//! `tests/fixtures/mindmap/harvest.json` holds the mindmap sources of Pele's own test suite, the
//! inputs of its mutated runs and its worst cases, written by `scripts/harvest-mindmap.mjs`. The
//! random maps must reach every drawing path in [`PATHS`]. A map wider than a narrow host is drawn
//! again running down as an outline, so the harvest is also drawn at four widths.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<Value> {
    let path = pele_oracle::root().join("tests/fixtures/mindmap/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").clone()
}

fn sources(list: &str) -> Vec<String> {
    harvest(list).iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// This port's render with Pele's render options, which are what `render_with` gives the oracle.
fn render_with(source: &str, options: &Value) -> Result<Rendered, RenderError> {
    let mut parse = ParseOptions::default();
    let mut layout = LayoutOptions::default();
    let number = |value: &Value| value.as_f64().expect("a number");
    let flag = |value: &Value| value.as_bool().expect("a boolean");
    let count = |value: &Value| usize::try_from(value.as_u64().expect("a count")).expect("a count");
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = Some(number(value)),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(text(value)),
            "responsive" => layout.responsive = Some(flag(value)),
            "autoDirection" => layout.auto_direction = flag(value),
            "directionBreakpoint" => layout.direction_breakpoint = Some(number(value)),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

/// A drawing path, and how an identical output shows that it went through it: by a mark in
/// Pele's SVG, or, where the SVG has none of its own, by the `%% tag` the random generator writes
/// beside what takes it.
type Reached = fn(&str, &str) -> bool;

/// The class of each node, as Pele writes it.
fn node_classes(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-node ").skip(1).filter_map(|rest| rest.split('"').next()).collect()
}

/// Each node, up to the end of its group.
fn node_groups(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-node ").skip(1).map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())]).collect()
}

/// The `d` of each link.
fn links(svg: &str) -> Vec<&str> {
    svg.split("class=\"pele-edge\" d=\"").skip(1).filter_map(|rest| rest.split('"').next()).collect()
}

/// The x of each node's translation.
fn node_xs(svg: &str) -> Vec<f64> {
    node_groups(svg)
        .iter()
        .filter_map(|group| group.split_once("translate(")?.1.split_once(',')?.0.parse().ok())
        .collect()
}

fn shaped(svg: &str, shape: &str, root: bool) -> bool {
    let class = format!("pele-shape-{shape}");
    node_classes(svg).iter().any(|classes| classes.starts_with(&class) && classes.contains("pele-root") == root)
}

/// Running down, where each link leaves the trunk it drops from.
fn trunk_starts(svg: &str) -> Vec<&str> {
    links(svg).iter().filter(|d| d.contains('Q')).filter_map(|d| d.strip_prefix('M')?.split(',').next()).collect()
}

const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("a root alone", |_, svg| node_classes(svg).len() == 1),
    ("a stadium root", |_, svg| shaped(svg, "stadium", true)),
    ("a rounded root", |_, svg| shaped(svg, "rounded", true)),
    ("a rect root", |_, svg| shaped(svg, "rect", true)),
    ("a circle root", |_, svg| shaped(svg, "circle", true)),
    ("a cloud root", |_, svg| shaped(svg, "cloud", true)),
    ("a bang root", |_, svg| shaped(svg, "bang", true)),
    ("a hexagon root", |_, svg| shaped(svg, "hex", true)),
    ("a rounded node", |_, svg| shaped(svg, "rounded", false)),
    ("a rect node", |_, svg| shaped(svg, "rect", false)),
    ("a circle node", |_, svg| shaped(svg, "circle", false)),
    ("a cloud node", |_, svg| shaped(svg, "cloud", false)),
    ("a bang node", |_, svg| shaped(svg, "bang", false)),
    ("a hexagon node", |_, svg| shaped(svg, "hex", false)),
    ("a text node", |_, svg| shaped(svg, "text", false)),
    ("a pill of a node that branches", |_, svg| shaped(svg, "branch", false)),
    ("branches to the right", |_, svg| node_xs(svg).iter().any(|&x| x > 0.0) && svg.contains("C")),
    ("branches to the left", |_, svg| {
        links(svg).iter().any(|d| d.contains('C')) && {
            let xs = node_xs(svg);
            xs.last().is_some_and(|&root| xs.iter().any(|&x| x < root))
        }
    }),
    ("a tree four levels deep", |source, _| source.contains("%% deep")),
    ("more branches than series colours", |_, svg| svg.contains("pele-section-8")),
    ("more branches than sections", |source, svg| {
        source.contains("%% wide") && svg.contains("pele-section-0") && svg.matches("pele-section-0\"").count() > 1
    }),
    ("an outline", |_, svg| links(svg).iter().any(|d| d.contains('Q'))),
    ("an outline with a multi-line text node", |_, svg| {
        links(svg).iter().any(|d| d.contains('Q'))
            && node_groups(svg)
                .iter()
                .any(|group| group.starts_with("pele-shape-text") && group.matches("<text").count() > 1)
    }),
    ("an outline root that keeps its box", |_, svg| {
        links(svg).iter().any(|d| d.contains('Q')) && (shaped(svg, "rect", true) || shaped(svg, "rounded", true))
    }),
    ("a trunk shared by siblings", |_, svg| {
        let starts = trunk_starts(svg);
        starts.iter().enumerate().any(|(i, start)| starts[i + 1..].contains(start))
    }),
    ("a label over two lines", |_, svg| {
        svg.contains("<tspan x=") && node_groups(svg).iter().any(|group| group.matches("<tspan x=").count() > 1)
    }),
    ("a label with emphasis", |_, svg| {
        node_groups(svg).iter().any(|group| group.contains("font-weight=\"var(--_w)\""))
    }),
    ("a node's own classes", |_, svg| {
        node_classes(svg).iter().any(|classes| classes.split(' ').count() > 2 && !classes.contains("pele-root"))
    }),
    ("a set node width", |source, _| source.contains("maxNodeWidth: 1")),
    ("nodes named alike", |source, _| source.contains("%% twins")),
    ("an icon the host does not draw", |source, svg| source.contains("::icon(") && !svg.contains("pele-icon")),
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
                    *if r#type == "mindmap" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "mindmap" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: kind, .. }, Err(RenderError::NotPorted))
                if kind.as_deref().is_some_and(|kind| kind != "mindmap") =>
            {
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
                Some(format!("pele rejects a mindmap, ours reports it as not ported: {error}"))
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

/// How many of `cases` Pele draws otherwise than with no width, which is how many run down.
fn folded(cases: &[(String, Value)]) -> usize {
    let wide: Vec<(String, String)> = cases.iter().map(|(source, _)| (source.clone(), "{}".to_owned())).collect();
    let narrow: Vec<(String, String)> =
        cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
    let (wide, narrow) = (pele_oracle::render_all_with(&wide), pele_oracle::render_all_with(&narrow));
    wide.iter()
        .zip(&narrow)
        .filter(|(a, b)| match (a, b) {
            (PeleRender::Svg { svg: a, .. }, PeleRender::Svg { svg: b, .. }) => a != b,
            _ => false,
        })
        .count()
}

/// Hand-written maps for what the harvest reaches least, each with the host options that reach it.
fn cases() -> Vec<(String, Value)> {
    let map = "mindmap\n  root((mindmap))\n    Origins\n      Long history\n      ::icon(fa fa-book)\n      Popularisation\n        British popular psychology author Tony Buzan\n    Research\n      On effectiveness<br/>and features\n      On Automatic creation\n        Uses\n            Creative techniques\n            Strategic planning\n            Argument mapping\n    Tools\n      Pen and paper\n      Mermaid\n";
    let bare = "mindmap\n  root";
    let mut out = vec![
        (map.to_owned(), json!({})),
        (map.to_owned(), json!({ "maxWidth": 320 })),
        (map.to_owned(), json!({ "maxWidth": 100 })),
        (map.to_owned(), json!({ "maxWidth": 10 })),
        (map.to_owned(), json!({ "maxWidth": 0 })),
        (map.to_owned(), json!({ "maxWidth": 639 })),
        (map.to_owned(), json!({ "maxWidth": 640 })),
        (map.to_owned(), json!({ "maxWidth": 320, "padding": 0 })),
        (map.to_owned(), json!({ "maxWidth": 300, "autoDirection": false })),
        (map.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (map.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (map.to_owned(), json!({ "fontSize": 12 })),
        (map.to_owned(), json!({ "fontSize": 21, "padding": 3, "maxWidth": 400 })),
        (map.to_owned(), json!({ "idPrefix": "other", "responsive": false })),
        (bare.to_owned(), json!({})),
        ("mindmap\n".to_owned(), json!({})),
        ("mindmap\n".to_owned(), json!({ "maxWidth": 320 })),
        (
            "---\ntitle: A title much wider than the whole map can ever be made to be\n---\nmindmap\n  root\n    a"
                .to_owned(),
            json!({}),
        ),
        (
            "---\ntitle: A title much wider than the whole map can ever be made to be\n---\nmindmap\n  root\n    a"
                .to_owned(),
            json!({ "maxWidth": 200 }),
        ),
        (bare.to_owned(), json!({ "outputLimit": 200 })),
        (bare.to_owned(), json!({ "limit": 5 })),
    ];
    for config in [
        json!({"mindmap": {"maxNodeWidth": 80}}),
        json!({"mindmap": {"maxNodeWidth": 3000}}),
        json!({"mindmap": {"maxNodeWidth": 10.5}}),
        json!({"mindmap": {"maxNodeWidth": -1}}),
        json!({"mindmap": {"maxNodeWidth": "300"}}),
        json!({"mindmap": "x"}),
        json!({"mindmap": {"useMaxWidth": false}}),
        json!({"themeVariables": {"cScale1": "#ff0000", "cScale2": "red"}}),
    ] {
        out.push((map.to_owned(), json!({ "config": config })));
        out.push((map.to_owned(), json!({ "config": config, "maxWidth": 320 })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("mindmap cases");
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(sources("sources"))).finish("harvested mindmap sources");
    assert!(tally.identical > 700, "the harvest holds mindmaps Pele renders");
    assert!(tally.rejected > 100, "the harvest holds mindmaps Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    for width in [320, 480, 640, 900] {
        let cases: Vec<(String, Value)> =
            sources("sources").into_iter().map(|source| (source, json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested mindmap sources at maxWidth {width}"));
        println!("  Pele draws {} of them otherwise at maxWidth {width}", folded(&cases));
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(sources("mutated"))).finish("mutated mindmap sources");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> = harvest("worst")
        .iter()
        .map(|item| {
            let source = item["source"].as_str().expect("a source").to_owned();
            (source, json!({ "limit": 100_000_000, "outputLimit": 1_000_000_000 }))
        })
        .collect();
    let tally = run("worst", &cases).finish("Pele's worst-case mindmaps");
    assert!(tally.identical > 15, "the worst cases are drawn");
}

/// xorshift64, so the maps are the same on every run.
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

const SHAPES: &[(&str, &str)] =
    &[("", ""), ("", ""), ("", ""), ("[", "]"), ("(", ")"), ("((", "))"), ("))", "(("), (")", "("), ("{{", "}}")];
const TEXTS: &[&str] = &[
    "Topic",
    "Long history",
    "A node whose text is long enough to wrap over more than one line of the map",
    "**bold** idea",
    "*italic*",
    "one<br>two",
    "#35; &amp; \u{e9}\u{91c7}\u{1f913}",
    "Supercalifragilisticexpialidociousness",
    "x",
];

/// A map Pele draws: a root of any shape, branches of any count and depth, nodes of every shape,
/// classes, icons, and the config and host options that change it.
fn random_map(rng: &mut Rng) -> (String, Value) {
    let mut src = String::new();
    let wide = rng.chance(5);
    let deep = rng.chance(15);
    let narrow = rng.chance(10);
    let titled = rng.chance(8);
    if narrow || titled {
        src += "---\n";
        if titled {
            src += "title: Mind the map\n";
        }
        if narrow {
            src += &format!("config:\n  mindmap:\n    maxNodeWidth: {}\n", ["100", "150", "1"][rng.below(3)]);
        }
        src += "---\n";
    }
    src += "mindmap\n";
    if wide {
        src += "%% wide\n";
    }
    if deep {
        src += "%% deep\n";
    }
    let mut count = 0;
    let mut line = |rng: &mut Rng, depth: usize, src: &mut String| {
        let (open, close) = SHAPES[rng.below(SHAPES.len())];
        let text = rng.pick(TEXTS);
        let twin = count > 0 && rng.chance(5);
        let id = if twin { 0 } else { count };
        count += 1;
        let node = if open.is_empty() { format!("{text} {id}") } else { format!("n{id}{open}{text}{close}") };
        *src += &format!("{}{node}{}\n", "  ".repeat(depth + 1), if twin { " %% twins" } else { "" });
        if rng.chance(8) {
            *src += &format!("{}:::{}\n", "  ".repeat(depth + 1), ["big", "a b", "urgent"][rng.below(3)]);
        }
        if rng.chance(4) {
            *src += &format!("{}::icon(fa fa-book)\n", "  ".repeat(depth + 1));
        }
    };
    line(rng, 0, &mut src);
    let branches = if wide { 9 + rng.below(6) } else { rng.below(6) };
    for _ in 0..branches {
        line(rng, 1, &mut src);
        let mut depth = 1;
        for _ in 0..rng.below(if deep { 8 } else { 4 }) {
            depth = if deep { depth + 1 } else { 1 + rng.below(depth + 1) }.min(6);
            line(rng, depth, &mut src);
        }
    }
    let options = match rng.below(10) {
        0..=2 => {
            let width = [320, 480, 600][rng.below(3)];
            json!({ "maxWidth": width })
        }
        3 => {
            let width = [200, 900][rng.below(2)];
            json!({ "maxWidth": width })
        }
        4 => {
            let size = [12, 14, 20][rng.below(3)];
            json!({ "fontSize": size })
        }
        5 => json!({ "padding": 0 }),
        _ => json!({}),
    };
    (src, options)
}

#[test]
fn random_maps_match_pele() {
    let mut rng = Rng(0x3a1d_3a90_0042_0042);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_map(&mut rng)).collect();
    let tally = run("random", &cases).finish("random mindmaps");
    assert!(tally.identical > 3900, "random maps are ones Pele draws");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random mindmap reaches {missed:?}");
}
