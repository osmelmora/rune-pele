//! A sankey diagram's Scene read the way an interpreter with no SVG reads it: each node is an
//! entity named by its name and each link by the nodes it joins, links between the same nodes
//! ranked, every drawing has a key of its own at every width, and a key stays put when the source
//! changes elsewhere.

#[path = "differential/harness.rs"]
mod harness;

use harness::{
    Ident, drawn_entity_texts, drawn_keys, edit, keys_survive, named, names_exactly, ranked, rune_fixture, scene,
    walk_sources,
};
use pele_core::scene::{EntityKind, EntityName};
use pele_core::{LayoutOptions, ParseOptions};
use serde_json::Value;

/// The entities a sankey model names: its nodes by name, and its links by the nodes they join.
fn named_in(model: &Value) -> Vec<Ident> {
    let nodes: Vec<String> = model["nodes"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|node| node.as_str().expect("a name").to_owned())
        .collect();
    let mut out = ranked(EntityKind::Node, nodes.iter().map(|node| named(node)));
    let end = |link: &Value, at: usize| nodes[link[at].as_u64().expect("a node") as usize].encode_utf16().collect();
    let links = model["links"].as_array().expect("a list").iter().map(|link| EntityName::Between {
        from: end(link, 0),
        to: end(link, 1),
        kind: "link".to_owned(),
    });
    out.extend(ranked(EntityKind::Edge, links));
    out
}

#[test]
fn every_element_is_drawn_under_a_key_of_its_own() {
    let narrow = LayoutOptions { max_width: Some(320.0), ..LayoutOptions::default() };
    let (mut drawn, mut ranked_apart, mut unread) = (0, 0, 0);
    for source in walk_sources("sankey") {
        let Ok(model) = pele_core::sankey_front_json(&source, ParseOptions::default()) else { continue };
        // A name with a lone surrogate is JSON that serde does not read, so only its keys are walked.
        let expected = match serde_json::from_str::<Value>(&model) {
            Ok(model) if model.get("model").is_none() => continue,
            Ok(model) => Some(named_in(&model["model"])),
            Err(_) => None,
        };
        for options in [&LayoutOptions::default(), &narrow] {
            let Some(scene) = scene(&source, options) else { continue };
            drawn_keys(&scene, &source);
            match &expected {
                Some(expected) => names_exactly(&scene, expected, &source),
                None => unread += 1,
            }
            drawn += 1;
        }
        ranked_apart += usize::from(expected.iter().flatten().any(|(_, _, rank)| *rank > 0));
    }
    println!(
        "sankey drawings walked: {drawn}. Their names unread: {unread}. Sources that link two nodes twice: {ranked_apart}."
    );
    assert!(drawn > 1000, "the corpus, the fixture and the harvest draw at both widths");
    assert!(ranked_apart > 0, "some source links the same nodes twice");
}

#[test]
fn keys_come_from_node_names() {
    let fixture = scene(&rune_fixture("sankey"), &LayoutOptions::default()).expect("the fixture draws");
    let want = [
        "edge:Coal~Power~link",
        "edge:Gas~Power~link",
        "edge:Solar~Power~link",
        "edge:Power~Homes~link",
        "edge:Power~Industry~link",
        "edge:Power~Losses~link",
        "node:Coal",
        "node:Power",
        "node:Gas",
        "node:Solar",
        "node:Homes",
        "node:Industry",
        "node:Losses",
    ];
    assert_eq!(drawn_entity_texts(&fixture), want);
    let twice = scene("sankey\n\nA,B,1\nB,C,3\nA,B,2", &LayoutOptions::default()).expect("it draws");
    let want = ["edge:A~B~link", "edge:B~C~link", "edge:A~B~link#1", "node:A", "node:B", "node:C"];
    assert_eq!(drawn_entity_texts(&twice), want, "a second link between the same nodes ranks after the first");
}

#[test]
fn a_key_survives_an_edit_elsewhere() {
    let before = rune_fixture("sankey");
    let after = edit(&before, "Coal,Power,25", "Wind,Grid,12\nCoal,Power,25");
    let after = edit(&after, "Power,Losses,8", "Power,Losses,7");
    let after = after + "Industry,Exports,6\n";
    keys_survive(&before, &after, &["edge:Power~Losses~link", "node:Losses"]);
}
