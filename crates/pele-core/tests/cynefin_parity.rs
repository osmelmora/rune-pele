//! Differential test for drawing cynefin diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/cynefin/harvest.json` holds the cynefin sources of Pele's own test suite, the
//! inputs of its mutated runs and its worst cases, written by `scripts/harvest-cynefin.mjs`. With
//! three corpus sources, the harvest, the hand-written cases and the random diagrams carry the
//! proof, and the random diagrams must reach every drawing path in [`PATHS`]. Cynefin is drawn at
//! the size its config sets, whatever width the host has, so one width is enough to show that.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/cynefin/harvest.json");
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

/// Each element that starts with `start`, up to the end of its group.
fn groups<'a>(svg: &'a str, start: &str) -> Vec<&'a str> {
    svg.split(start).skip(1).map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())]).collect()
}

/// The value of attribute `name` in the first tag of `text`.
fn attribute(text: &str, name: &str) -> Option<f64> {
    let (_, rest) = text.split_once(&format!(" {name}=\""))?;
    rest[..rest.find('"')?].parse().ok()
}

/// The root's width and height.
fn size(svg: &str) -> (f64, f64) {
    (attribute(svg, "width").unwrap_or(0.0), attribute(svg, "height").unwrap_or(0.0))
}

/// The radii of the ellipse of Confusion.
fn radii(svg: &str) -> Option<(f64, f64)> {
    let (_, rest) = svg.split_once("data-id=\"confusion\" d=\"")?;
    let (_, arc) = rest.split_once('A')?;
    let (rx, rest) = arc.split_once(',')?;
    let (ry, _) = rest.split_once(' ')?;
    Some((rx.parse().ok()?, ry.parse().ok()?))
}

/// The transitions drawn, as the two domains their id names.
fn edges(svg: &str) -> Vec<(&str, &str)> {
    svg.split("<g class=\"pele-edge\" data-id=\"")
        .skip(1)
        .filter_map(|rest| rest[..rest.find('"')?].split_once('-'))
        .collect()
}

fn opposite((a, b): (&str, &str)) -> bool {
    matches!(
        (a, b),
        ("complex", "clear") | ("clear", "complex") | ("complicated", "chaotic") | ("chaotic", "complicated")
    )
}

/// The x of each transition label.
fn label_xs(svg: &str) -> Vec<f64> {
    svg.split("class=\"pele-edge-label\"").skip(1).filter_map(|rest| attribute(rest, "x")).collect()
}

/// The drawing paths a source can go through, each with what in the source and Pele's SVG shows
/// it did.
const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("an accessible title and description", |_, svg| {
        svg.contains("aria-labelledby=") && svg.contains("aria-describedby=")
    }),
    ("every domain with items", |_, svg| {
        groups(svg, "<g class=\"pele-cynefin-domain\"").iter().all(|d| d.contains("pele-node"))
    }),
    ("no domain with items", |_, svg| !svg.contains("pele-node")),
    ("Confusion with more items than it shows", |_, svg| svg.contains("pele-cynefin-overflow")),
    ("domains without their descriptions", |_, svg| !svg.contains("pele-cynefin-subtitle")),
    ("an item over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-node\"").iter().any(|item| item.matches("<tspan x=").count() > 1)
    }),
    ("an item with emphasis", |_, svg| {
        groups(svg, "<g class=\"pele-node\"").iter().any(|item| item.contains("<tspan") && item.contains("font-weight"))
    }),
    ("an item named like another", |source, _| {
        let items: Vec<&str> = source.lines().map(str::trim).filter(|line| line.starts_with('"')).collect();
        items.iter().enumerate().any(|(i, item)| items[i + 1..].contains(item))
    }),
    ("quadrants grown wider than the config", |source, svg| !source.contains("width:") && size(svg).0 > 656.0),
    ("quadrants grown taller than the config", |source, svg| {
        !source.contains("height:") && !svg.contains("pele-title") && size(svg).1 > 456.0
    }),
    ("the ellipse grown for Confusion's items", |_, svg| radii(svg).is_some_and(|(rx, ry)| rx > 88.0 || ry > 64.0)),
    ("a set width and height", |source, _| source.contains("width:") && source.contains("height:")),
    ("a configured seed", |source, _| source.contains("seed:")),
    ("straight boundaries", |source, _| source.contains("boundaryAmplitude: 0.0\n")),
    ("a transition between neighbours", |_, svg| {
        edges(svg).iter().any(|&e| e.0 != "confusion" && e.1 != "confusion" && !opposite(e))
    }),
    ("a transition across the middle", |_, svg| edges(svg).iter().any(|&e| opposite(e))),
    ("a transition to or from Confusion", |_, svg| {
        edges(svg).iter().any(|&(a, b)| a == "confusion" || b == "confusion")
    }),
    ("a transition repeated", |_, svg| {
        let edges = edges(svg);
        edges
            .iter()
            .enumerate()
            .any(|(i, &(a, b))| edges[i + 1..].iter().any(|&(c, d)| (a, b) == (c, d) || (a, b) == (d, c)))
    }),
    ("a transition with a label", |_, svg| svg.contains("class=\"pele-edge-label\"")),
    ("a transition label kept inside the diagram", |_, svg| {
        let (width, _) = size(svg);
        label_xs(svg).iter().any(|&x| x < 120.0 || x > width - 120.0)
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
                    *if r#type == "cynefin" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "cynefin" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "cynefin" => {
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
                Some(format!("pele rejects a cynefin diagram, ours reports it as not ported: {error}"))
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

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let full = "cynefin-beta\n  title Incident Response\n  accTitle: Incidents\n  accDescr: Where each kind of incident goes\n  complex\n    \"Investigate root cause\"\n    \"Run chaos experiment\"\n  complicated\n    \"Expert review needed\"\n  clear\n    \"Restart service\"\n  chaotic\n    \"Page on-call immediately\"\n  confusion\n    \"Unknown failure mode\"\n    \"b\"\n    \"c\"\n    \"d\"\n    \"e\"\n  complex --> complicated : \"Pattern identified\"\n  clear --> chaotic : \"Complacency\"\n  complex --> clear\n  clear --> complex\n  confusion --> chaotic : \"A label long enough to need the room it is given\"\n";
    let bare = "cynefin-beta";
    let mut out = vec![
        (full.to_owned(), json!({})),
        (full.to_owned(), json!({ "idPrefix": "other" })),
        (full.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (full.to_owned(), json!({ "fontSize": 12 })),
        (full.to_owned(), json!({ "fontSize": 22, "padding": 0 })),
        (full.to_owned(), json!({ "maxWidth": 320 })),
        (bare.to_owned(), json!({})),
        (bare.to_owned(), json!({ "idPrefix": "" })),
        (bare.to_owned(), json!({ "outputLimit": 200 })),
        (bare.to_owned(), json!({ "limit": 5 })),
    ];
    for config in [
        json!({"cynefin": {"useMaxWidth": false}}),
        json!({"cynefin": "x"}),
        json!({"cynefin": {"showDomainDescriptions": false}}),
        json!({"cynefin": {"showDomainDescriptions": "false"}}),
        json!({"cynefin": {"width": 100, "height": 100}}),
        json!({"cynefin": {"width": 99, "height": 10001}}),
        json!({"cynefin": {"width": 2000, "height": 1500, "boundaryAmplitude": 50}}),
        json!({"cynefin": {"boundaryAmplitude": 0, "seed": 1}}),
        json!({"cynefin": {"boundaryAmplitude": 51, "seed": -1e15}}),
        json!({"cynefin": {"seed": 1.5}}),
        json!({"cynefin": {"seed": 1e15}}),
        json!({"cynefin": {"seed": 2e15}}),
        json!({"cynefin": {"seed": -7}}),
    ] {
        out.push((full.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("cynefin cases");
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested cynefin sources");
    assert!(tally.identical > 300, "the harvest holds cynefin diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds cynefin diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_a_width() {
    let cases: Vec<(String, Value)> =
        harvest("sources").into_iter().map(|source| (source, json!({ "maxWidth": 320 }))).collect();
    run("harvested", &cases).finish("harvested cynefin sources at maxWidth 320");
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated cynefin sources");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> =
        harvest("worst").into_iter().map(|source| (source, json!({ "limit": 100_000_000 }))).collect();
    let tally = run("worst", &cases).finish("Pele's worst-case cynefin diagrams");
    assert!(tally.identical > 10, "the worst cases are drawn, not rejected");
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

const DOMAINS: &[&str] = &["complex", "complicated", "clear", "chaotic", "confusion"];
const ITEMS: &[&str] = &[
    "\"Restart service\"",
    "'a'",
    "\"Run a chaos experiment to see what the system does under load\"",
    "\"An item that is a good deal wider than two hundred pixels, so it wraps\"",
    "\"<b>bold</b> and <i>italic</i>\"",
    "\"two<br>lines\"",
    "\"#35; &amp; \\\" q\"",
    "\"\u{e9}\u{91c7}\u{1f600}\"",
    "\"unbreakableunbreakableunbreakableunbreakableunbreakable\"",
];
const LABELS: &[&str] =
    &["\"x\"", "\"Pattern identified\"", "\"A transition label long enough to need two lines of room\"", "''"];

/// A diagram Pele draws: items in each domain, many in Confusion now and then, transitions between
/// any two domains with and without labels, and the config and host options that change the
/// drawing.
fn random_diagram(rng: &mut Rng) -> (String, Value) {
    let mut src = String::new();
    let mut config = serde_json::Map::new();
    if rng.chance(25) {
        let pick = |rng: &mut Rng, key: &str, values: &[f64]| (key.to_owned(), json!(values[rng.below(values.len())]));
        for (key, value) in [
            pick(rng, "width", &[100.0, 300.0, 640.0, 1200.0]),
            pick(rng, "height", &[100.0, 300.0, 440.0, 900.0]),
            pick(rng, "seed", &[0.0, 1.0, 42.5, -3.0, 123_456_789.0]),
            pick(rng, "boundaryAmplitude", &[0.0, 3.0, 8.0, 20.0, 50.0]),
        ] {
            if rng.chance(50) {
                config.insert(key, value);
            }
        }
        if rng.chance(30) {
            config.insert("showDomainDescriptions".to_owned(), json!(false));
        }
    }
    if !config.is_empty() {
        src += "---\nconfig:\n  cynefin:\n";
        for (key, value) in &config {
            src += &format!("    {key}: {value}\n");
        }
        src += "---\n";
    }
    src += "cynefin-beta\n";
    if rng.chance(30) {
        src += &format!(
            "  title {}\n",
            rng.pick(&["Strategy", "A title much wider than the whole diagram, as wide as it can be made"])
        );
    }
    if rng.chance(8) {
        src += "  accTitle: Acc\n  accDescr: What it shows\n";
    }
    let full = rng.chance(20);
    for &domain in DOMAINS {
        if !full && rng.chance(30) {
            continue;
        }
        src += &format!("  {domain}\n");
        let count = if domain == "confusion" && rng.chance(30) {
            3 + rng.below(6)
        } else {
            rng.below(if full { 4 } else { 3 }) + usize::from(full)
        };
        for _ in 0..count {
            src += &format!("    {}\n", rng.pick(ITEMS));
        }
    }
    for _ in 0..rng.below(6) {
        let (a, b) = (rng.pick(DOMAINS), rng.pick(DOMAINS));
        let label = if rng.chance(50) { format!(" : {}", rng.pick(LABELS)) } else { String::new() };
        src += &format!("  {a} --> {b}{label}\n");
    }
    let options = match rng.below(8) {
        0 => {
            let prefix = ["d1", "pele-2", "x"][rng.below(3)];
            json!({ "idPrefix": prefix })
        }
        1 => {
            let size = [12, 14, 20][rng.below(3)];
            json!({ "fontSize": size })
        }
        2 => json!({ "padding": 0 }),
        _ => json!({}),
    };
    (src, options)
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x0c7e_f1a5_eed0_0042);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_diagram(&mut rng)).collect();
    let tally = run("random", &cases).finish("random cynefin diagrams");
    assert!(tally.identical > 3900, "random diagrams are ones Pele draws");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random cynefin diagram reaches {missed:?}");
}
