//! The error for an SVG past the host's output limit, held to Pele's for every Mermaid alias.
//!
//! Pele throws it after drawing, with the type and Mermaid's alias it detected
//! (`src/core.ts`, `render`). The oracle's `render_error` answers that whole error: its message,
//! code, type, alias, line, column and snippet.

use pele_core::{ErrorCode, LayoutOptions, Limits, ParseOptions, RenderError};
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

fn code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Unsupported => "unsupported-diagram",
        ErrorCode::Syntax => "syntax",
        ErrorCode::Semantic => "semantic",
        ErrorCode::Limit => "limit",
        _ => "unknown",
    }
}

fn ours(source: &str) -> Value {
    let parse = ParseOptions::default().limits(Limits::default().svg_utf16(1));
    let drawn = pele_core::parity_scene_with(source, parse, &LayoutOptions::default())
        .and_then(|scene| pele_core::svg::write(&scene));
    match drawn {
        Err(RenderError::Diagnostic(diagnostic)) => {
            let location = diagnostic.location.as_ref();
            json!({
                "error": format!("PeleError: {}", diagnostic.message),
                "code": code_name(diagnostic.code),
                "type": diagnostic.kind.map(|kind| kind.name()),
                "mermaidType": diagnostic.mermaid_type.map(|mermaid| mermaid.name()),
                "line": location.map_or(0, |location| location.line),
                "column": location.map_or(0, |location| location.column_utf16),
                "snippet": location.map_or("", |location| location.snippet.as_str()),
            })
        }
        other => json!({ "unexpected": format!("{other:?}") }),
    }
}

fn pele(sources: &[String]) -> Vec<Value> {
    let requests: Vec<String> =
        sources.iter().map(|s| format!("render_error\t{}\t{{\"outputLimit\":1}}", line(s))).collect();
    pele_oracle::ask(&requests)
        .iter()
        .map(|answer| serde_json::from_str(answer).expect("the oracle answers JSON"))
        .collect()
}

#[test]
fn the_output_limit_error_is_pele_s_in_every_field_for_every_alias() {
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
