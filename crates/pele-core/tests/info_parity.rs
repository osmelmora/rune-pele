//! Differential test for drawing info diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/info/harvest.json` holds the corpus of Pele's info parser test and its
//! mutations, written by `scripts/harvest-info.mjs`. An info diagram does not react to the width
//! the host has, which the widths below show.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/info/harvest.json");
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
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = Some(number(value)),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(value.as_str().expect("a string").to_owned()),
            "responsive" => layout.responsive = Some(value.as_bool().expect("a boolean")),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

/// Renders each case through both engines, and answers how many came out identical and how many
/// were rejected alike.
fn run(what: &str, cases: &[(String, Value)]) -> (usize, usize) {
    let requests: Vec<(String, String)> =
        cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
    let (mut identical, mut rejected, mut other, mut failures) = (0, 0, 0, Vec::new());
    for (index, ((source, options), theirs)) in cases.iter().zip(pele_oracle::render_all_with(&requests)).enumerate() {
        let ours = std::panic::catch_unwind(|| render_with(source, options));
        let problem = match (&theirs, ours) {
            (_, Err(_)) => Some("this port panics".to_owned()),
            (PeleRender::Svg { r#type, svg, width, height }, Ok(Ok(ours))) => {
                if ours.svg != *svg || ours.width != *width || ours.height != *height {
                    Some(format!("drawn differently\n    ours: {}\n    pele: {svg}", ours.svg))
                } else {
                    *if r#type == "info" { &mut identical } else { &mut other } += 1;
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Ok(Err(RenderError::NotPorted))) if r#type != "info" => {
                other += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Ok(Err(RenderError::NotPorted))) if kind != "info" => {
                other += 1;
                None
            }
            (PeleRender::Error { error, .. }, Ok(Err(RenderError::Diagnostic(ours)))) => {
                if format!("PeleError: {}", ours.message) == *error {
                    rejected += 1;
                    None
                } else {
                    Some(format!("rejected with another message\n    ours: {:?}\n    pele: {error:?}", ours.message))
                }
            }
            (theirs, Ok(ours)) => Some(format!("pele answers {theirs:?}, ours {:?}", ours.map(|ours| ours.svg))),
        };
        if let Some(problem) = problem {
            failures.push(format!(
                "{what} {index} with {options}: {problem}\n    source: {:?}",
                &source[..source.len().min(300)]
            ));
        }
    }
    println!(
        "{what}: {} inputs, {identical} identical to Pele, {rejected} rejected with Pele's message, {other} of another type, {} different",
        cases.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} {what} differ from Pele:\n{}",
        failures.len(),
        failures[..failures.len().min(8)].join("\n")
    );
    (identical, rejected)
}

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

#[test]
fn harvested_sources_match_pele() {
    let (identical, _) = run("harvested info sources", &plain(harvest("sources")));
    assert!(identical > 15, "the harvest holds info diagrams Pele draws");
    for width in [320.0, 480.0, 640.0, 900.0] {
        let cases: Vec<(String, Value)> =
            harvest("sources").into_iter().map(|source| (source, json!({ "maxWidth": width }))).collect();
        run(&format!("harvested info sources at maxWidth {width}"), &cases);
    }
}

#[test]
fn mutated_sources_match_pele() {
    let (identical, rejected) = run("mutated info sources", &plain(harvest("mutated")));
    assert!(identical > 100 && rejected > 1000, "the mutations are drawn and rejected alike");
}

/// Titles and host options the harvest does not hold, and Pele's worst cases for rendering info
/// (`tests/info/stress.test.ts`) with the source limit lifted as that test lifts it.
fn cases() -> Vec<(String, Value)> {
    const N: usize = 50_000;
    let titled = "info\ntitle A title<br>on two lines, and **more**\naccTitle: at\naccDescr {\n  two\n  lines\n}";
    let wide = format!("info\ntitle {}", "word ".repeat(200));
    let big = json!({ "limit": 1_000_000_000u64 });
    let mut out = vec![
        ("info".to_owned(), json!({})),
        ("info showInfo".to_owned(), json!({ "fontSize": 24 })),
        ("info showInfo".to_owned(), json!({ "fontSize": 9.5, "padding": 0 })),
        (titled.to_owned(), json!({})),
        (titled.to_owned(), json!({ "padding": 30, "idPrefix": "x\"y", "responsive": false })),
        (titled.to_owned(), json!({ "config": { "info": { "useMaxWidth": false } } })),
        (wide.clone(), json!({})),
        (wide, json!({ "maxWidth": 320 })),
        ("info\ntitle t".to_owned(), json!({ "outputLimit": 100 })),
        ("info\ntitle t".to_owned(), json!({ "limit": 5 })),
        ("---\ntitle: From front matter\n---\ninfo".to_owned(), json!({})),
    ];
    for source in [
        format!("info\ntitle {}", "word ".repeat(N / 5)),
        format!("info\n{}", "title t\n".repeat(N / 8)),
        format!("info\ntitle {}", "#35;&amp;<br>".repeat(N / 13)),
        format!("info{}showInfo", "\n ".repeat(N / 2)),
        format!("info\naccDescr {{{}}}", " a\n".repeat(N / 3)),
        format!("info\naccTitle: {}", "x".repeat(N)),
    ] {
        out.push((source, big.clone()));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("info cases", &cases());
}
