//! Differential test for drawing kanban boards: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/kanban/harvest.json` holds the kanban sources of Pele's own test suite, the
//! ticket links of its security test with their configs, the inputs of its mutated runs and its
//! worst cases, written by `scripts/harvest-kanban.mjs`. With three corpus sources, the harvest,
//! the hand-written cases and the random boards carry the proof, and the random boards must reach
//! every drawing path in [`PATHS`]. A board wider than the host folds its columns into rows, so the
//! harvest is also drawn at four widths.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<Value> {
    let path = pele_oracle::root().join("tests/fixtures/kanban/harvest.json");
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
    let list = |value: &Value| value.as_array().expect("a list").iter().map(text).collect::<Vec<String>>();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = number(value),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(text(value)),
            "responsive" => layout.responsive = Some(flag(value)),
            "links" => layout.resources.links = flag(value),
            "linkSchemes" => layout.resources.link_schemes = list(value),
            "linkRel" => layout.resources.link_rel = text(value),
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

/// Each element that starts with `start`, up to the end of its group.
fn groups<'a>(svg: &'a str, start: &str) -> Vec<&'a str> {
    svg.split(start).skip(1).map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())]).collect()
}

/// Each column, up to its first card.
fn heads(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-cluster pele-column")
        .skip(1)
        .map(|rest| &rest[..rest.find("<g").unwrap_or(rest.len())])
        .collect()
}

/// The texts of the elements whose class is `class`.
fn texts<'a>(svg: &'a str, class: &str) -> Vec<&'a str> {
    let marker = format!("class=\"{class}\"");
    svg.split(marker.as_str())
        .skip(1)
        .filter_map(|rest| {
            let start = rest.find('>')? + 1;
            Some(&rest[start..start + rest[start..].find('<')?])
        })
        .collect()
}

/// The distinct tops of the columns.
fn rows(svg: &str) -> usize {
    let tops: BTreeSet<&str> = svg
        .split("pele-column")
        .skip(1)
        .filter_map(|rest| rest.split_once("translate(")?.1.split_once(')')?.0.split_once(','))
        .map(|(_, y)| y)
        .collect();
    tops.len()
}

const LONG: &str = "Supercalifragilisticexpialidociousness";

const PATHS: &[(&str, Reached)] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("a title wider than the board", |source, _| source.contains("%% wide title")),
    ("a column with no cards", |_, svg| svg.contains("text-anchor=\"end\" font-size=\"14\" fill=\"var(--_m)\">0<")),
    ("columns folded into rows", |_, svg| rows(svg) > 1),
    ("a column title over two lines", |_, svg| {
        heads(svg).iter().any(|head| head.matches("pele-cluster-label").count() > 1)
    }),
    ("a column title broken inside a word", |_, svg| {
        texts(svg, "pele-cluster-label").iter().any(|text| !text.is_empty() && *text != LONG && LONG.starts_with(text))
    }),
    ("a column title with emphasis", |_, svg| {
        heads(svg).iter().any(|head| head.contains("pele-cluster-label") && head.contains("<tspan"))
    }),
    ("a card label over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-card")
            .iter()
            .any(|card| card.matches("class=\"pele-label\"").count() > 1)
    }),
    ("a word broken across a card's lines", |_, svg| {
        texts(svg, "pele-label").iter().any(|text| !text.is_empty() && *text != LONG && LONG.starts_with(text))
    }),
    ("a card line of one bold run", |_, svg| svg.contains("class=\"pele-label\" font-weight=\"var(--_w)\" x=")),
    ("a card line of one italic run", |_, svg| svg.contains("class=\"pele-label\" font-style=\"italic\" x=")),
    ("a card line of one monospace run", |_, svg| svg.contains("class=\"pele-label\" font-family=\"var(--_fm)\" x=")),
    ("a card line of several runs", |_, svg| svg.contains("class=\"pele-label\" text-anchor=\"middle\" xml:space")),
    ("an underlined run", |_, svg| svg.contains("text-decoration=\"underline\"")),
    ("a shifted run", |_, svg| svg.contains("baseline-shift=")),
    ("an empty line", |_, svg| svg.contains("> </tspan>")),
    ("a ticket", |_, svg| svg.contains("class=\"pele-ticket\" fill=\"var(--_m)\"")),
    ("a linked ticket", |_, svg| svg.contains("class=\"pele-ticket\" fill=\"var(--_c)\"")),
    ("a ticket link that is refused", |source, svg| {
        source.contains("ticketBaseUrl") && source.contains("ticket:") && !svg.contains("<a ")
    }),
    ("a ticket over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-card").iter().any(|card| card.matches("pele-ticket").count() > 1)
    }),
    ("an assignee", |_, svg| svg.contains("class=\"pele-assigned\"")),
    ("an assignee over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-card").iter().any(|card| card.matches("pele-assigned").count() > 1)
    }),
    ("a ticket and an assignee on one row", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-card").iter().any(|card| {
            let ticket =
                card.split("pele-ticket").nth(1).and_then(|rest| rest.split(" y=\"").nth(1)?.split('"').next());
            let assigned =
                card.split("pele-assigned").nth(1).and_then(|rest| rest.split(" y=\"").nth(1)?.split('"').next());
            ticket.is_some() && ticket == assigned
        })
    }),
    ("a ticket above an assignee", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-card").iter().any(|card| {
            let ticket =
                card.split("pele-ticket").nth(1).and_then(|rest| rest.split(" y=\"").nth(1)?.split('"').next());
            let assigned =
                card.split("pele-assigned").nth(1).and_then(|rest| rest.split(" y=\"").nth(1)?.split('"').next());
            matches!((ticket, assigned), (Some(t), Some(a)) if t != a)
        })
    }),
    ("a very high priority", |_, svg| svg.contains("pele-priority-very-high")),
    ("a high priority", |_, svg| svg.contains("pele-priority-high")),
    ("a low priority", |_, svg| svg.contains("pele-priority-low")),
    ("a very low priority", |_, svg| svg.contains("pele-priority-very-low")),
    ("a priority Pele does not mark", |source, svg| source.contains("priority: Medium") && svg.contains("pele-card")),
    ("a column's own classes", |_, svg| svg.contains("pele-column ")),
    ("a card's own classes", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-card")
            .iter()
            .any(|card| card.starts_with(' ') || card.contains("pele-card "))
    }),
    ("a set section width", |source, _| source.contains("sectionWidth: 3") || source.contains("sectionWidth: 1")),
    ("a section width past its bounds", |source, _| {
        source.contains("sectionWidth: 40") || source.contains("sectionWidth: 9000")
    }),
    ("cards named alike", |source, _| source.contains("%% twins")),
    ("a label from metadata", |source, _| source.contains("label:")),
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
                    *if r#type == "kanban" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(PATHS.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "kanban" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: kind, .. }, Err(RenderError::NotPorted))
                if kind.as_deref().is_some_and(|kind| kind != "kanban") =>
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
                Some(format!("pele rejects a kanban board, ours reports it as not ported: {error}"))
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

/// How many of `cases` Pele draws otherwise than with no width, which is how many fold.
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

/// Hand-written boards for what the harvest reaches least, each with the host options that reach it.
fn cases() -> Vec<(String, Value)> {
    let board = "---\nconfig:\n  kanban:\n    ticketBaseUrl: 'https://example.com/browse/#TICKET#'\n---\nkanban\n  Todo\n    [Create Documentation]\n    docs[Create Blog about the new diagram]\n  [In progress]\n    id6[Create renderer so that it works in all cases. We also add some extra text here for testing purposes. And some more just for the extra flare.]\n  id9[Ready for deploy]\n    id8[Design grammar]@{ assigned: 'knsv' }\n  id10[Ready for test]\n    id4[Create parsing tests]@{ ticket: MC-2038, assigned: 'K.Sveidqvist', priority: 'High' }\n    id66[last item]@{ priority: 'Very Low', assigned: 'knsv' }\n  id11[Done]\n    id5[define getData]\n    id2[Title of diagram is more than 100 chars when user duplicates diagram with 100 char]@{ ticket: MC-2036, priority: 'Very High'}\n    id3[Update DB function]@{ ticket: MC-2037, assigned: knsv, priority: 'High' }\n  id12[Can't reproduce]\n    id3[Weird flickering in Firefox]\n";
    let bare = "kanban\n  todo\n    a";
    let mut out = vec![
        (board.to_owned(), json!({})),
        (board.to_owned(), json!({ "maxWidth": 100 })),
        (board.to_owned(), json!({ "maxWidth": 10 })),
        (board.to_owned(), json!({ "maxWidth": 0 })),
        (board.to_owned(), json!({ "maxWidth": -5 })),
        (board.to_owned(), json!({ "maxWidth": 700, "padding": 0 })),
        (board.to_owned(), json!({ "maxWidth": 1300 })),
        (board.to_owned(), json!({ "fontSize": 12 })),
        (board.to_owned(), json!({ "fontSize": 21, "padding": 3 })),
        (board.to_owned(), json!({ "idPrefix": "other", "responsive": false })),
        (board.to_owned(), json!({ "links": false })),
        (board.to_owned(), json!({ "linkRel": "" })),
        (board.to_owned(), json!({ "linkRel": "nofollow \"x\"", "linkSchemes": ["file"] })),
        (bare.to_owned(), json!({})),
        ("kanban\n\n".to_owned(), json!({})),
        ("kanban\n\n".to_owned(), json!({ "maxWidth": 320 })),
        (
            "---\ntitle: A title much wider than one column can ever be made to hold\n---\nkanban\n  todo\n    a"
                .to_owned(),
            json!({}),
        ),
        (
            "---\ntitle: A title much wider than one column can ever be made to hold\n---\nkanban\n  todo\n    a"
                .to_owned(),
            json!({ "maxWidth": 200 }),
        ),
        (bare.to_owned(), json!({ "outputLimit": 200 })),
        (bare.to_owned(), json!({ "limit": 5 })),
    ];
    for config in [
        json!({"kanban": {"sectionWidth": 320}}),
        json!({"kanban": {"sectionWidth": 10}}),
        json!({"kanban": {"sectionWidth": 5000}}),
        json!({"kanban": {"sectionWidth": 250.75}}),
        json!({"kanban": {"sectionWidth": -1}}),
        json!({"kanban": {"sectionWidth": "300"}}),
        json!({"kanban": "x"}),
        json!({"kanban": {"ticketBaseUrl": "javascript:alert(#TICKET#)"}}),
        json!({"kanban": {"ticketBaseUrl": "#TICKET#"}}),
        json!({"kanban": {"ticketBaseUrl": "https://x/#TICKET#/#TICKET#"}}),
        json!({"kanban": {"ticketBaseUrl": "rel/path?t=#TICKET#&amp;"}}),
        json!({"kanban": {"ticketBaseUrl": 7}}),
        json!({"kanban": {"useMaxWidth": false}}),
    ] {
        out.push((board.to_owned(), json!({ "config": config })));
        out.push((board.to_owned(), json!({ "config": config, "maxWidth": 500 })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("kanban cases");
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(sources("sources"))).finish("harvested kanban sources");
    assert!(tally.identical > 800, "the harvest holds kanban boards Pele renders");
    assert!(tally.rejected > 100, "the harvest holds kanban boards Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    for width in [320, 480, 640, 900] {
        let cases: Vec<(String, Value)> =
            sources("sources").into_iter().map(|source| (source, json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested kanban sources at maxWidth {width}"));
        println!("  Pele folds {} of them at maxWidth {width}", folded(&cases));
    }
}

#[test]
fn configured_and_mutated_sources_match_pele() {
    let configured: Vec<(String, Value)> = harvest("configured")
        .iter()
        .map(|item| (item["source"].as_str().expect("a source").to_owned(), json!({ "config": item["config"] })))
        .collect();
    run("configured", &configured).finish("kanban sources with a ticket link config");
    run("mutated", &plain(sources("mutated"))).finish("mutated kanban sources");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> = harvest("worst")
        .iter()
        .map(|item| {
            let mut options = json!({ "limit": 100_000_000 });
            if !item["config"].is_null() {
                options["config"] = item["config"].clone();
            }
            (item["source"].as_str().expect("a source").to_owned(), options)
        })
        .collect();
    let tally = run("worst", &cases).finish("Pele's worst-case kanban boards");
    assert!(tally.identical > 15, "the worst cases that fit Pele's output limit are drawn");
}

/// A word whose letters add up to exactly the card's text width: Pele keeps the last letter that
/// meets the edge on the line, since `width + w > max` is false at the tie. Each column is set as
/// wide as that from Pele's own width of the letter, for words that break after 10 to 15 letters.
#[test]
fn a_word_that_only_just_fits_keeps_its_last_letter() {
    let request = format!("label\t\"m\"\t{:016x}\t{:016x}\t0\tfalse", 14f64.to_bits(), 1e9f64.to_bits());
    let answer: Value = serde_json::from_str(&pele_oracle::ask(&[request])[0]).expect("the oracle answers JSON");
    let bits = u64::from_str_radix(answer["width"].as_str().expect("a width"), 16).expect("hex bits");
    let letter = f64::from_bits(bits);
    let mut cases = Vec::new();
    for fits in 10..16 {
        let mut sum = 0.0;
        for _ in 0..fits {
            sum += letter;
        }
        let section = sum + 36.0;
        if section - 16.0 - 20.0 == sum {
            let source = format!("kanban\n  todo\n    a[{}]", "m".repeat(fits * 2 + 1));
            cases.push((source, json!({ "config": { "kanban": { "sectionWidth": section } } })));
        }
    }
    assert!(cases.len() > 3, "most widths give an exact tie");
    let tally = run("tie", &cases).finish("kanban cards whose letters meet the edge exactly");
    assert_eq!(tally.identical, cases.len());
}

/// xorshift64, so the boards are the same on every run.
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

const COLUMNS: &[&str] = &[
    "To do",
    "In progress",
    "A column title long enough to need two lines",
    "<b>Bold</b> column",
    "*em*",
    "*em* and more",
    "Supercalifragilisticexpialidociousness",
];
const LABELS: &[&str] = &[
    "Task",
    "Write the documentation for the new board",
    "A card whose text is long enough to wrap over three lines of a narrow column",
    "**bold**",
    "*italic*",
    "`code`",
    "<code>mono</code>",
    "**bold** and plain",
    "<u>underlined</u>",
    "H<sub>2</sub>O",
    "x<sup>2</sup>",
    "one<br>two",
    "one<br><br>three",
    "Supercalifragilisticexpialidociousness",
    "#35; &amp; \u{e9}\u{91c7}\u{1f913}",
];
const TICKETS: &[&str] =
    &["MC-1", "T-2037", "A-ticket-long-enough-to-need-a-row-of-its-own", "'two words'", "\"<b>x</b>\""];
const PEOPLE: &[&str] = &["knsv", "'K.Sveidqvist'", "'Someone with a name too long for one row of a card'", "a"];
const PRIORITIES: &[&str] = &["'Very High'", "High", "Medium", "'low'", "'Very Low'", "' high '", "whenever"];
const BASES: &[&str] = &[
    "'https://example.com/browse/#TICKET#'",
    "'javascript:alert(#TICKET#)'",
    "'#TICKET#'",
    "'//host/#TICKET#'",
    "'mailto:#TICKET#@x'",
];

/// A board Pele draws: columns with cards that carry metadata in every combination, emphasis and
/// long words in their text, classes, a title, and the config and host options that change it.
fn random_board(rng: &mut Rng) -> (String, Value) {
    let mut config = Vec::new();
    if rng.chance(20) {
        let width = ["sectionWidth: 300", "sectionWidth: 140", "sectionWidth: 40", "sectionWidth: 9000"][rng.below(4)];
        config.push(width.to_owned());
    }
    if rng.chance(35) {
        config.push(format!("ticketBaseUrl: {}", rng.pick(BASES)));
    }
    let wide_title = rng.chance(8);
    let mut src = String::new();
    let titled = rng.chance(10);
    if !config.is_empty() || wide_title || titled {
        src += "---\n";
        if wide_title {
            src += "title: A board title wider than every column of the board put side by side, by far\n";
        } else if titled {
            src += "title: Sprint 12\n";
        }
        if !config.is_empty() {
            src += "config:\n  kanban:\n";
            for line in &config {
                src += &format!("    {line}\n");
            }
        }
        src += "---\n";
    }
    src += "kanban\n";
    if wide_title {
        src += "%% wide title\n";
    }
    let mut ids = 0;
    for column in 0..=rng.below(5) {
        if rng.chance(10) {
            src += &format!("  c{column}\n");
        } else {
            src += &format!("  c{column}[{}]\n", rng.pick(COLUMNS));
        }
        if rng.chance(15) {
            src += &format!("  :::{}\n", rng.pick(&["wide", "a b", "urgent"]));
        }
        for _ in 0..rng.below(5) {
            let twin = ids > 0 && rng.chance(8);
            let id = if twin { 0 } else { ids };
            ids += 1;
            let mut pairs = Vec::new();
            if rng.chance(40) {
                pairs.push(format!("ticket: {}", rng.pick(TICKETS)));
            }
            if rng.chance(40) {
                pairs.push(format!("assigned: {}", rng.pick(PEOPLE)));
            }
            if rng.chance(40) {
                pairs.push(format!("priority: {}", rng.pick(PRIORITIES)));
            }
            let label = rng.pick(LABELS);
            let card = if rng.chance(10) {
                pairs.push(format!("label: \"{}\"", label.replace('"', "'")));
                format!("    t{id}[Old]")
            } else if label.contains('`') || rng.chance(30) {
                format!("    t{id}[{label}]")
            } else {
                format!("    t{id}[\"`{label}`\"]")
            };
            let metadata = if pairs.is_empty() { String::new() } else { format!("@{{ {} }}", pairs.join(", ")) };
            src += &format!("{card}{metadata}{}\n", if twin { " %% twins" } else { "" });
            if rng.chance(10) {
                src += &format!("    :::{}\n", rng.pick(&["done", "x y"]));
            }
            if rng.chance(5) {
                src += "    ::icon(star)\n";
            }
        }
    }
    let options = match rng.below(10) {
        0 => {
            let width = [320, 480, 640][rng.below(3)];
            json!({ "maxWidth": width })
        }
        1 => {
            let width = [150, 900][rng.below(2)];
            json!({ "maxWidth": width })
        }
        2 => {
            let size = [12, 14, 20][rng.below(3)];
            json!({ "fontSize": size })
        }
        3 => json!({ "padding": 0 }),
        _ => json!({}),
    };
    (src, options)
}

#[test]
fn random_boards_match_pele() {
    let mut rng = Rng(0x6a2b_a2d0_0042_0042);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_board(&mut rng)).collect();
    let tally = run("random", &cases).finish("random kanban boards");
    assert!(tally.identical > 3900, "random boards are ones Pele draws");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random kanban board reaches {missed:?}");
}
