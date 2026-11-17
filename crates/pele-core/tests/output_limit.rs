//! The error for an SVG past the host's output limit, held to Pele's for every Mermaid alias.
//!
//! Pele throws it after drawing, with the type and Mermaid's alias it detected
//! (`src/core.ts`, `render`). The oracle's `render_with` answers Pele's type and message, and
//! `preprocess` then `detect` answer the alias Pele's `parseSource` found for the same source.

use pele_core::{LayoutOptions, Limits, ParseOptions, RenderError};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Mermaid's name for each detector in Pele's `DETECTORS` (`src/detect.ts`). The test fails
/// unless some source reaches each one.
const ALIASES: [&str; 37] = [
    "flowchart-elk",
    "mindmap",
    "architecture",
    "agentflow",
    "c4",
    "kanban",
    "classDiagram",
    "er",
    "gantt",
    "info",
    "pie",
    "requirement",
    "sequence",
    "swimlane",
    "flowchart-v2",
    "timeline",
    "gitGraph",
    "stateDiagram",
    "journey",
    "quadrantChart",
    "sankey",
    "packet",
    "xychart",
    "block",
    "eventmodeling",
    "treeView",
    "radar",
    "ishikawa",
    "treemap",
    "railroad",
    "railroadEbnf",
    "railroadAbnf",
    "railroadPeg",
    "venn",
    "wardley",
    "cynefin",
    "usecase",
];

fn line(source: &str) -> String {
    serde_json::to_string(source)
        .expect("a string encodes")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn ours(source: &str) -> Value {
    let parse = ParseOptions { limits: Limits { svg_utf16: 1, ..Limits::default() }, ..ParseOptions::default() };
    let drawn = pele_core::parity_scene_with(source, parse, &LayoutOptions::default())
        .and_then(|scene| pele_core::svg::write(&scene));
    match drawn {
        Err(RenderError::Diagnostic(diagnostic)) => json!({
            "type": diagnostic.kind.map(|kind| kind.name()),
            "mermaidType": diagnostic.mermaid_type.map(|mermaid| mermaid.name()),
            "message": diagnostic.message,
        }),
        other => json!({ "unexpected": format!("{other:?}") }),
    }
}

fn pele(sources: &[String]) -> Vec<Value> {
    let renders: Vec<String> =
        sources.iter().map(|s| format!("render_with\t{}\t{{\"outputLimit\":1}}", line(s))).collect();
    let preprocessed: Vec<String> = sources.iter().map(|s| format!("preprocess\t{}", line(s))).collect();
    let renders = pele_oracle::ask(&renders);
    let detects: Vec<String> = pele_oracle::ask(&preprocessed)
        .iter()
        .map(|answer| {
            let pre: Value = serde_json::from_str(answer).expect("the oracle answers JSON");
            format!("detect\t{}", line(pre["text"].as_str().expect("a source that renders preprocesses")))
        })
        .collect();
    let detected = pele_oracle::ask(&detects);
    renders
        .iter()
        .zip(&detected)
        .map(|(render, detected)| {
            let render: Value = serde_json::from_str(render).expect("the oracle answers JSON");
            let detected: Value = serde_json::from_str(detected).expect("the oracle answers JSON");
            let error = render["error"].as_str().unwrap_or("no error");
            json!({
                "type": render["type"],
                "mermaidType": detected["mermaidType"],
                "message": error.strip_prefix("PeleError: ").unwrap_or(error),
            })
        })
        .collect()
}

#[test]
fn the_output_limit_error_names_the_type_and_alias_as_pele_does() {
    let mut sources: Vec<String> = pele_oracle::corpus().into_iter().flat_map(|(_, sources)| sources).collect();
    sources.push("flowchart-elk TD\n  A --> B".to_owned());
    let theirs = pele(&sources);
    let mut reached = BTreeSet::new();
    let mut failures = Vec::new();
    for (source, theirs) in sources.iter().zip(&theirs) {
        let ours = ours(source);
        if ours != *theirs {
            failures.push(format!("ours: {ours}\n    pele: {theirs}\n    source: {source:?}"));
        }
        if let Some(alias) = theirs["mermaidType"].as_str() {
            reached.insert(alias.to_owned());
        }
    }
    let missed: Vec<&str> = ALIASES.iter().copied().filter(|alias| !reached.contains(*alias)).collect();
    println!("output limit errors: {} inputs, {} aliases, {} different", sources.len(), reached.len(), failures.len());
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        sources.len(),
        failures[..failures.len().min(5)].join("\n")
    );
    assert!(missed.is_empty(), "no source reaches the aliases {missed:?}");
}
