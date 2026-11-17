//! Differential test for drawing packet diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/packet/harvest.json` holds the packet sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-packet.mjs`. A packet wider than
//! a narrow host is drawn again in shorter rows, with each label wrapped in its field, so the
//! widths matter.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "packet/random.rs"]
mod random;

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use random::{Rng, random_options, random_packet};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/packet/harvest.json");
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

/// The fields, each from its group's opening up to the next.
fn fields(svg: &str) -> impl Iterator<Item = &str> {
    group(svg, "pele-packet-fields").split("<g class=\"pele-node\"").skip(1)
}

/// The numbers of each field's first and last bit, where it has two.
fn bit_pairs(svg: &str) -> Vec<(f64, f64)> {
    let bits = group(svg, "pele-packet-bits");
    let texts: Vec<&str> = bits.split("<text").skip(1).collect();
    texts
        .windows(2)
        .filter(|pair| !pair[0].contains("text-anchor") && pair[1].contains("text-anchor=\"end\""))
        .filter_map(|pair| {
            let number = |text: &str| text.split('>').nth(1)?.split('<').next()?.parse::<f64>().ok();
            Some((number(pair[0])?, number(pair[1])?))
        })
        .collect()
}

/// The `y` of each field's box.
fn rows(svg: &str) -> BTreeSet<&str> {
    fields(svg).filter_map(|field| field.split("<rect x=\"").nth(1)?.split(" y=\"").nth(1)?.split('"').next()).collect()
}

/// The drawing paths of `renderPacket`, each with what in Pele's SVG shows it was taken.
const PATHS: &[(&str, Shows)] = &[
    ("a title", |svg| contains(svg, "class=\"pele-title\"")),
    ("a title and no fields", |svg| contains(svg, "class=\"pele-title\"") && !contains(svg, "pele-packet-fields")),
    ("an accessible title", |svg| contains(svg, "aria-labelledby=")),
    ("bit numbers", |svg| contains(svg, "class=\"pele-packet-bits\"")),
    ("bit numbers hidden", |svg| contains(svg, "pele-packet-fields") && !contains(svg, "pele-packet-bits")),
    ("a one-bit field", |svg| group(svg, "pele-packet-bits").contains("text-anchor=\"middle\"")),
    ("bits in ascending order", |svg| bit_pairs(svg).iter().any(|(first, last)| first < last)),
    ("bits in descending order", |svg| bit_pairs(svg).iter().any(|(first, last)| first > last)),
    ("a label at its own size", |svg| {
        fields(svg).any(|field| field.contains("<text x=") && !field.contains("font-size"))
    }),
    ("a label shrunk to fit", |svg| fields(svg).any(|field| field.contains("font-size="))),
    ("a label cut short", |svg| fields(svg).any(|field| field.contains('\u{2026}'))),
    ("a field too narrow for its label", |svg| fields(svg).any(|field| !field.contains("<text"))),
    ("a label wrapped in its field", |svg| fields(svg).any(|field| field.contains("<tspan"))),
    ("two rows or more", |svg| rows(svg).len() > 1),
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
                    if r#type == "packet" {
                        self.identical += 1;
                        self.reached.extend(PATHS.iter().filter(|(_, shows)| shows(svg)).map(|(name, _)| *name));
                    } else {
                        self.other_types += 1;
                    }
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "packet" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "packet" => {
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
                Some(format!("pele rejects a packet diagram, ours reports it as not ported: {error}"))
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
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested packet sources");
    assert!(tally.identical > 300, "the harvest holds packet diagrams Pele renders");
    assert!(tally.rejected > 100, "the harvest holds packet diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested packet sources at maxWidth {width}"));
        println!("maxWidth {width} makes Pele draw {} of the harvested sources another way", redrawn(&cases));
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated packet sources");
}

const TCP: &str = "packet\n0-15: \"Source Port\"\n16-31: \"Destination Port\"\n32-63: \"Sequence Number\"\n64: \"URG\"";

/// Hand-written packets for what the harvest reaches least, each with the host options that reach it.
fn cases() -> Vec<(String, Value)> {
    let config = |yaml: &str, body: &str| format!("---\nconfig:\n  packet:\n{yaml}\n---\n{body}");
    let bodies = [
        TCP.to_owned(),
        "packet-beta\n0-15: \"Source Port\"\n16-31: \"Destination Port\"\n32-4095: \"Payload (variable length)\"\n"
            .to_owned(),
        "packet\n0-31: \"Header\"\n32-39: \"Flags\"\n40-127: \"Payload\"".to_owned(),
        "packet\n0: \"Wide\"\n1: \"Extraordinarily long\"\n2-31: \"x\"".to_owned(),
        "packet\ntitle Nothing yet".to_owned(),
        "packet".to_owned(),
        format!("packet\n0-31: \"{}\"", "word ".repeat(40)),
        "packet\n0-31: \"#35;&amp; <b>x</b> \\n line\"".to_owned(),
    ];
    let configs = [
        "",
        "    bitsPerRow: 8\n    bitWidth: 10",
        "    bitsPerRow: 16\n    bitOrder: descending",
        "    rowHeight: 80\n    paddingY: 30",
        "    paddingX: 20\n    showBits: false",
        "    bitsPerRow: -4\n    bitWidth: \"wide\"\n    rowHeight: 0\n    paddingX: 9999",
        "    bitsPerRow: 20",
        "    bitsPerRow: 1",
    ];
    let mut out = Vec::new();
    for body in &bodies {
        for yaml in configs {
            let source = if yaml.is_empty() { body.clone() } else { config(yaml, body) };
            for options in [
                json!({}),
                json!({ "maxWidth": 320 }),
                json!({ "maxWidth": 120, "padding": 0 }),
                json!({ "fontSize": 13.5 }),
            ] {
                out.push((source.clone(), options));
            }
        }
    }
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("packet cases");
    assert!(tally.identical > 150, "the cases render");
}

/// Pele's own worst cases (`tests/packet/stress.test.ts`) that it renders, each with the source
/// limit raised.
fn worst_cases() -> Vec<(String, Value)> {
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let config = |yaml: &str, body: &str| format!("---\nconfig:\n  packet:\n{yaml}\n---\n{body}");
    let big = json!({ "limit": 10_000_000 });
    let sources = [
        format!("packet\n{}", repeat(4000, &|i| format!("{i}: \"f{i}\"\n"))),
        format!("packet\n{}", repeat(5000, &|i| format!("+{}: \"f\"\n", i % 40 + 1))),
        "packet\n+1000000000: \"wide\"\n+1: \"after\"".to_owned(),
        format!("packet\n+{}: \"x\"\n+1: \"y\"", "9".repeat(400)),
        config("    bitsPerRow: 1", "packet\n+20000: \"a\""),
        format!("packet\n0-31: \"{}\"", "word ".repeat(10000)),
        config("    bitsPerRow: 1", &format!("packet\n+20000: \"{}\"", "x".repeat(40000))),
        format!("packet\n0-31: \"{}\"", "#35;&amp;".repeat(5555)),
        format!("packet title {}", "x".repeat(50000)),
    ];
    let mut out = Vec::new();
    for source in sources {
        out.push((source.clone(), big.clone()));
        out.push((source, json!({ "limit": 10_000_000, "maxWidth": 320 })));
    }
    out
}

#[test]
fn worst_cases_match_pele() {
    run("worst", &worst_cases()).finish("Pele's worst-case packet diagrams");
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| (random_packet(&mut rng), random_options(&mut rng))).collect();
    let tally = run("random", &cases).finish("random packet diagrams");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random diagram's output has {missed:?}");
}
