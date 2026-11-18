//! Differential test for drawing Ishikawa diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/ishikawa/harvest.json` holds the Ishikawa sources of Pele's own test suite, the
//! inputs of its mutated runs, its random outlines and its worst cases, written by
//! `scripts/harvest-ishikawa.mjs`. With one corpus source, the harvest, the hand-written cases and
//! the random diagrams carry the proof, and the random diagrams must reach every drawing path in
//! [`PATHS`]. A diagram wider than a narrow host is drawn again as an indented list, so the harvest
//! is also drawn at four widths.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<Value> {
    let path = pele_oracle::root().join("tests/fixtures/ishikawa/harvest.json");
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

/// Each category across, up to the end of its group.
fn categories(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-ishikawa-category\"").skip(1).collect()
}

/// The `d` of each category's bones.
fn bones(svg: &str) -> Vec<&str> {
    svg.split("class=\"pele-ishikawa-bones\" d=\"").skip(1).filter_map(|rest| rest.split('"').next()).collect()
}

/// The texts of the causes across.
fn causes(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-ishikawa-causes\"")
        .skip(1)
        .map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())])
        .collect()
}

const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("no effect", |_, svg| !svg.contains("pele-ishikawa-head")),
    ("an effect alone", |_, svg| svg.contains("pele-ishikawa-spine") && !svg.contains("pele-ishikawa-category")),
    ("an effect over two lines", |_, svg| {
        svg.split("pele-ishikawa-head\"")
            .nth(1)
            .is_some_and(|head| head.split("</g>").next().unwrap_or("").matches("<tspan x=").count() > 1)
    }),
    ("a category above the spine", |_, svg| !categories(svg).is_empty()),
    ("a category below the spine", |_, svg| categories(svg).len() > 1),
    ("three pairs of categories", |_, svg| categories(svg).len() > 4),
    ("an odd category without a partner", |_, svg| categories(svg).len() % 2 == 1 && categories(svg).len() > 2),
    ("a category without causes", |_, svg| categories(svg).iter().any(|c| !c.contains("pele-ishikawa-causes"))),
    ("a category over two lines", |_, svg| {
        categories(svg)
            .iter()
            .any(|c| c.split("pele-ishikawa-causes").next().unwrap_or("").matches("<tspan x=").count() > 1)
    }),
    ("a level bone", |_, svg| bones(svg).iter().any(|d| d.contains('H'))),
    ("a slanted twig", |_, svg| bones(svg).iter().any(|d| d.matches('L').count() > 1)),
    ("a level bone off a twig", |_, svg| {
        causes(svg).iter().any(|c| c.contains("fill=\"var(--_m)\""))
            && bones(svg).iter().any(|d| d.matches('H').count() > 1)
    }),
    ("a cause six levels deep", |source, _| source.contains("%% deep")),
    ("a cause in muted text", |_, svg| causes(svg).iter().any(|c| c.contains("fill=\"var(--_m)\""))),
    ("a cause over two lines", |_, svg| causes(svg).iter().any(|c| c.contains("<tspan x="))),
    ("a cause with emphasis", |_, svg| causes(svg).iter().any(|c| c.contains("font-weight=\"var(--_w)\""))),
    ("a category whose causes reach past its pair", |source, _| source.contains("%% reach")),
    ("an indented list", |_, svg| svg.contains("pele-node pele-ishikawa-head")),
    ("a list with categories", |_, svg| svg.contains("pele-node pele-ishikawa-category")),
    ("a list with causes", |_, svg| svg.contains("pele-node pele-ishikawa-cause\"")),
    ("a list with a word broken across lines", |source, svg| {
        source.contains("Pneumonoultramicroscopicsilicovolcanoconiosis")
            && svg.contains("pele-node pele-ishikawa-head")
            && !svg.contains(">Pneumonoultramicroscopicsilicovolcanoconiosis<")
    }),
    ("a list indented by the least step", |_, svg| {
        svg.contains("pele-node pele-ishikawa-head")
            && svg.split("<path class=\"pele-edge\" d=\"M").skip(1).count() > 12
    }),
    ("causes named alike", |source, _| source.contains("%% twins")),
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
                    *if r#type == "ishikawa" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "ishikawa" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: kind, .. }, Err(RenderError::NotPorted))
                if kind.as_deref().is_some_and(|kind| kind != "ishikawa") =>
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
                Some(format!("pele rejects an Ishikawa diagram, ours reports it as not ported: {error}"))
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

/// Hand-written diagrams for what the harvest reaches least, each with the host options that reach
/// it.
fn cases() -> Vec<(String, Value)> {
    let doc = "ishikawa-beta\n    Blurry Photo\n    Process\n        Out of focus\n        Shutter speed too slow\n        Protective film not removed\n        Beautification filter applied\n    User\n        Shaky hands\n    Equipment\n        LENS\n            Inappropriate lens\n            Damaged lens\n            Dirty lens\n        SENSOR\n            Damaged sensor\n            Dirty sensor\n    Environment\n        Subject moved too quickly\n        Too dark\n";
    let bare = "ishikawa-beta\n  E\n";
    let mut out = vec![
        (doc.to_owned(), json!({})),
        (doc.to_owned(), json!({ "maxWidth": 320 })),
        (doc.to_owned(), json!({ "maxWidth": 100 })),
        (doc.to_owned(), json!({ "maxWidth": 10 })),
        (doc.to_owned(), json!({ "maxWidth": 0 })),
        (doc.to_owned(), json!({ "maxWidth": 639 })),
        (doc.to_owned(), json!({ "maxWidth": 640 })),
        (doc.to_owned(), json!({ "maxWidth": 320, "padding": 0 })),
        (doc.to_owned(), json!({ "maxWidth": 300, "autoDirection": false })),
        (doc.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (doc.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (doc.to_owned(), json!({ "fontSize": 12 })),
        (doc.to_owned(), json!({ "fontSize": 21, "padding": 3, "maxWidth": 400 })),
        (doc.to_owned(), json!({ "idPrefix": "other", "responsive": false })),
        (doc.to_owned(), json!({ "config": {"ishikawa": {"useMaxWidth": false}} })),
        (bare.to_owned(), json!({})),
        (bare.to_owned(), json!({ "maxWidth": 200 })),
        ("ishikawa-beta%% only a comment".to_owned(), json!({})),
        ("ishikawa-beta%% only a comment".to_owned(), json!({ "maxWidth": 200 })),
        ("ishikawa-beta\n".to_owned(), json!({ "maxWidth": 200 })),
        ("---\ntitle: A title much wider than the whole diagram can ever be made to be\n---\nishikawa-beta\n  E\n    C\n".to_owned(), json!({})),
        ("---\ntitle: A title much wider than the whole diagram can ever be made to be\n---\nishikawa-beta\n  E\n    C\n".to_owned(), json!({ "maxWidth": 200 })),
        (bare.to_owned(), json!({ "outputLimit": 200 })),
        (bare.to_owned(), json!({ "limit": 5 })),
    ];
    let deep: String = (0..40).map(|i| format!("{}n{i}\n", " ".repeat(i + 1))).collect();
    out.push((format!("ishikawa-beta\n{deep}"), json!({})));
    out.push((format!("ishikawa-beta\n{deep}"), json!({ "maxWidth": 300 })));
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("Ishikawa cases");
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(sources("sources"))).finish("harvested Ishikawa sources");
    assert!(tally.identical > 500, "the harvest holds Ishikawa diagrams Pele renders");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    for width in [320, 480, 640, 900] {
        let cases: Vec<(String, Value)> =
            sources("sources").into_iter().map(|source| (source, json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested Ishikawa sources at maxWidth {width}"));
        println!("  Pele draws {} of them otherwise at maxWidth {width}", folded(&cases));
    }
}

#[test]
fn mutated_and_generated_sources_match_pele() {
    run("mutated", &plain(sources("mutated"))).finish("mutated Ishikawa sources");
    run("generated", &plain(sources("generated"))).finish("Pele's random Ishikawa token sequences and outlines");
    let narrow: Vec<(String, Value)> =
        sources("generated").into_iter().map(|source| (source, json!({ "maxWidth": 320 }))).collect();
    run("generated", &narrow).finish("Pele's random Ishikawa outlines at maxWidth 320");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> = harvest("worst")
        .iter()
        .map(|item| {
            let source = item["source"].as_str().expect("a source").to_owned();
            (source, json!({ "limit": 100_000_000 }))
        })
        .collect();
    let tally = run("worst", &cases).finish("Pele's worst-case Ishikawa diagrams");
    assert!(tally.identical > 10, "the worst cases that fit Pele's output limit are drawn");
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

const TEXTS: &[&str] = &[
    "Cause",
    "Shaky hands",
    "A cause that takes a good many words to state in full, and then some more",
    "<b>bold</b> cause",
    "<i>italic</i>",
    "one<br>two",
    "#35; &amp; \u{e9}\u{65e5}\u{672c}",
    "Pneumonoultramicroscopicsilicovolcanoconiosis",
    "x",
];

/// A diagram Pele draws: an effect, categories in any number, causes nested to any depth, long
/// and emphasised text, and the host options that change it.
fn random_diagram(rng: &mut Rng) -> (String, Value) {
    let mut src = String::new();
    let titled = rng.chance(10);
    if titled {
        src += "---\ntitle: Why it broke\n---\n";
    }
    src += "ishikawa-beta\n";
    let deep = rng.chance(12);
    if deep {
        src += "%% deep\n";
    }
    let reach = rng.chance(10);
    if reach {
        src += "%% reach\n";
    }
    let mut count = 0;
    let mut line = |rng: &mut Rng, depth: usize, src: &mut String| {
        let twin = count > 0 && rng.chance(5);
        let id = if twin { 0 } else { count };
        count += 1;
        let text = rng.pick(TEXTS);
        *src += &format!("{}{text} {id}{}\n", "  ".repeat(depth + 1), if twin { " %% twins" } else { "" });
    };
    if rng.chance(3) {
        src = src.trim_end().to_owned() + "%% no effect\n";
        return (src, json!({}));
    }
    line(rng, 0, &mut src);
    let most = if rng.chance(20) { 9 } else { 5 };
    let categories = if rng.chance(10) { 0 } else { 1 + rng.below(most) };
    for category in 0..categories {
        line(rng, 1, &mut src);
        let mut depth = 1;
        let many = reach && category % 2 == 0;
        for _ in 0..rng.below(if deep {
            10
        } else if many {
            7
        } else {
            4
        }) {
            depth = if deep { depth + 1 } else { 2 + rng.below(depth.min(3)) }.min(7);
            line(rng, depth, &mut src);
        }
    }
    let options = match rng.below(10) {
        0..=2 => {
            let width = [320, 480, 600][rng.below(3)];
            json!({ "maxWidth": width })
        }
        3 => {
            let width = [160, 900][rng.below(2)];
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
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x15b1_a3a0_0042_0042);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_diagram(&mut rng)).collect();
    let tally = run("random", &cases).finish("random Ishikawa diagrams");
    assert!(tally.identical > 3900, "random diagrams are ones Pele draws");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random Ishikawa diagram reaches {missed:?}");
}
