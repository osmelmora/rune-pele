//! Differential test for the sankey layout on its own: `diagrams/sankey/layout.rs` must place every
//! node and band where Pele 1.13.0's `layoutSankey` does, double for double, or name the same node
//! of a cycle, through the oracle's `sankey_layout` op.
//!
//! The flows are those of every harvested source Pele parses, each laid out with the options a
//! render gives and with other alignments and sizes, and seeded random flows, values that are
//! zero, negative, NaN, infinite or huge among them.

#[allow(dead_code, reason = "each test uses the parts of the generator it needs")]
#[path = "sankey/random.rs"]
mod random;

use pele_core::ParseOptions;
use pele_core::sankey_layout::{Alignment, Flows, LayoutOptions, layout};
use random::{Rng, random_flows};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/sankey/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn alignment_name(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Justify => "justify",
        Alignment::Left => "left",
        Alignment::Right => "right",
        Alignment::Center => "center",
    }
}

/// One call as the oracle's `sankey_layout` op reads it.
fn request(nodes: usize, links: &[(usize, usize, f64)], options: &LayoutOptions) -> String {
    let call = json!({
        "nodes": nodes,
        "links": links.iter().map(|&(s, t, v)| json!([s, t, bits(v)])).collect::<Vec<_>>(),
        "options": {
            "width": bits(options.width),
            "height": bits(options.height),
            "nodeWidth": bits(options.node_width),
            "nodePadding": bits(options.node_padding),
            "labelHeight": bits(options.label_height),
            "alignment": alignment_name(options.alignment),
        },
    });
    format!("sankey_layout\t{call}")
}

/// This port's answer in the words of the op.
fn ours(nodes: usize, links: &[(usize, usize, f64)], options: &LayoutOptions) -> String {
    match layout(&Flows::new(nodes, links), options) {
        Ok(placed) => json!({
            "nodes": placed.nodes.iter().map(|n| json!([n.layer, bits(n.value), bits(n.x), bits(n.y), bits(n.height)])).collect::<Vec<_>>(),
            "links": placed.links.iter().map(|l| json!([bits(l.value), bits(l.thickness), bits(l.source_y), bits(l.target_y)])).collect::<Vec<_>>(),
        })
        .to_string(),
        Err(stuck) => json!({
            "error": format!("PeleError: Sankey diagrams cannot have circular links, and \"n{stuck}\" is part of one.")
        })
        .to_string(),
    }
}

/// A number of nodes and the links between them, each with its value.
type Flow = (usize, Vec<(usize, usize, f64)>);
type Call = (usize, Vec<(usize, usize, f64)>, LayoutOptions);

/// Compares each call with the oracle's and fails with the first few that differ.
fn compare(what: &str, calls: &[Call]) {
    let requests: Vec<String> = calls.iter().map(|(n, links, options)| request(*n, links, options)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut failures = Vec::new();
    let mut cycles = 0;
    for ((n, links, options), theirs) in calls.iter().zip(&answers) {
        let ours = ours(*n, links, options);
        let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
        let ours: Value = serde_json::from_str(&ours).expect("our answer is JSON");
        if ours != theirs {
            failures.push(format!("{n} nodes, links {links:?}, {options:?}\n    ours: {ours}\n    pele: {theirs}"));
        } else if theirs.get("error").is_some() {
            cycles += 1;
        }
    }
    println!(
        "{what}: {} calls, {} placed as Pele places them, {cycles} cycles named as Pele names them, {} different",
        calls.len(),
        calls.len() - failures.len() - cycles,
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} {what} differ:\n{}",
        failures.len(),
        calls.len(),
        failures[..failures.len().min(5)].join("\n")
    );
}

/// The options a render gives a chart of default size, then other alignments and sizes.
fn variants() -> Vec<LayoutOptions> {
    let base = LayoutOptions {
        width: 600.0,
        height: 400.0,
        node_width: 10.0,
        node_padding: 27.0,
        label_height: 14.0 * 1.15,
        alignment: Alignment::Justify,
    };
    let mut out = vec![base];
    for alignment in [Alignment::Left, Alignment::Right, Alignment::Center] {
        out.push(LayoutOptions { alignment, ..base });
    }
    out.push(LayoutOptions { label_height: 0.0, node_padding: 12.0, ..base });
    out.push(LayoutOptions {
        width: 160.0,
        height: 40.0,
        label_height: 10.0 * 1.15,
        alignment: Alignment::Center,
        ..base
    });
    out.push(LayoutOptions { width: 1.0, height: 1.0, node_width: 1.0, node_padding: 200.0, ..base });
    out
}

/// The flows of every source Pele parses, from the model the front end gives.
fn flows_of(sources: &[String]) -> Vec<Flow> {
    let mut out = Vec::new();
    for source in sources {
        let Ok(json) = pele_core::sankey_front_json(source, ParseOptions::default()) else { continue };
        let Ok(model) = serde_json::from_str::<Value>(&json) else { continue };
        let Some(nodes) = model["model"]["nodes"].as_array() else { continue };
        let links = model["model"]["links"]
            .as_array()
            .expect("links")
            .iter()
            .map(|link| {
                let index = |value: &Value| usize::try_from(value.as_u64().expect("an index")).expect("an index");
                let value = f64::from_bits(u64::from_str_radix(link[2].as_str().expect("bits"), 16).expect("hex"));
                (index(&link[0]), index(&link[1]), value)
            })
            .collect();
        out.push((nodes.len(), links));
    }
    out
}

#[test]
fn harvested_flows_are_laid_out_as_pele_lays_them_out() {
    let mut calls = Vec::new();
    for (nodes, links) in flows_of(&harvest("sources")).into_iter().chain(flows_of(&harvest("mutated"))) {
        for options in variants() {
            calls.push((nodes, links.clone(), options));
        }
    }
    compare("harvested sankey flows", &calls);
}

#[test]
fn random_flows_are_laid_out_as_pele_lays_them_out() {
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let alignments = [Alignment::Justify, Alignment::Left, Alignment::Right, Alignment::Center];
    let calls: Vec<Call> = (0..6000)
        .map(|_| {
            let (nodes, links) = random_flows(&mut rng);
            let options = LayoutOptions {
                width: *rng.pick(&[600.0, 160.0, 333.3, 1.0]),
                height: *rng.pick(&[400.0, 40.0, 1000.0, 1.0]),
                node_width: *rng.pick(&[10.0, 1.0, 24.5]),
                node_padding: *rng.pick(&[27.0, 12.0, 0.0, 215.0]),
                label_height: *rng.pick(&[0.0, 16.099_999_999_999_998, 11.5, 60.0]),
                alignment: *rng.pick(&alignments),
            };
            (nodes, links, options)
        })
        .collect();
    compare("random sankey flows", &calls);
}
