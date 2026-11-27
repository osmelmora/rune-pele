//! What an app reads when it logs what it holds: `{:?}` of a prepared source.

use pele_core::{Limits, ParseOptions};

#[test]
fn a_prepared_source_shows_its_type_and_limits_and_not_its_model() {
    let options = ParseOptions::default().limits(Limits::default().max_edges(12));
    let prepared = pele_core::prepare("flowchart LR\n  Ask --> Answer\n", options).expect("a flowchart prepares");
    let shown = format!("{prepared:?}");
    assert!(shown.starts_with("Prepared { kind: Flowchart, limits: Limits { "), "{shown}");
    assert!(shown.contains("max_edges: 12"), "the app's limit: {shown}");
    assert!(shown.ends_with("}, .. }"), "the rest left out: {shown}");
    assert!(!shown.contains("Answer"), "the parsed model stays out: {shown}");
}
