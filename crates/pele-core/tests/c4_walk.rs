//! A C4 diagram's Scene read the way an interpreter with no SVG reads it: each element and
//! boundary is an entity named by its alias and each relationship by the aliases it joins, every
//! drawing has a key of its own at every width, and a key stays put when the source changes
//! elsewhere.

#[path = "differential/harness.rs"]
mod harness;

use harness::{
    Ident, drawn_entity_texts, drawn_keys, edit, keys_survive, named, names_exactly, ranked, rune_fixture, scene,
    walk_sources,
};
use pele_core::scene::{EntityKind, EntityName};
use pele_core::{LayoutOptions, ParseOptions};
use serde_json::Value;

/// The entities a C4 model names: its shapes and boundaries by alias, the outer boundary that no
/// source writes among them, and its relationships by the aliases they join and their kind.
fn named_in(model: &Value) -> Vec<Ident> {
    let list = |key: &str| model[key].as_array().expect("a list");
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    let units = |value: &Value| text(value).encode_utf16().collect();
    let mut out = ranked(EntityKind::Node, list("shapes").iter().map(|shape| named(&text(&shape["alias"]))));
    out.extend(ranked(EntityKind::Group, list("boundaries").iter().map(|group| named(&text(&group["alias"])))));
    let rels = list("rels").iter().map(|rel| EntityName::Between {
        from: units(&rel["from"]),
        to: units(&rel["to"]),
        kind: text(&rel["type"]),
    });
    out.extend(ranked(EntityKind::Edge, rels));
    out
}

#[test]
fn every_element_is_drawn_under_a_key_of_its_own() {
    let narrow = LayoutOptions::default().max_width(320.0);
    let (mut drawn, mut ranked_apart, mut unread) = (0, 0, 0);
    for source in walk_sources("c4") {
        let Ok(model) = pele_core::c4_front_json(&source, ParseOptions::default()) else { continue };
        // A name with a lone surrogate is JSON that serde does not read, so only its keys are walked.
        let expected = match serde_json::from_str::<Value>(&model) {
            Ok(model) if model["type"] != "c4" => continue,
            Ok(model) => Some(named_in(&model)),
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
    println!("C4 drawings walked: {drawn}. Their names unread: {unread}. Sources that rank an alias: {ranked_apart}.");
    assert!(drawn > 4000, "the corpus, the fixture and the harvest draw at both widths");
    assert!(ranked_apart > 0, "some source has elements that share an alias");
}

#[test]
fn keys_come_from_the_aliases_the_source_gives() {
    let fixture = scene(&rune_fixture("c4"), &LayoutOptions::default()).expect("the fixture draws");
    let want = [
        "diagram:",
        "group:shop",
        "edge:customer~checkout~rel",
        "edge:checkout~bank~rel",
        "node:customer",
        "node:bank",
        "node:checkout",
    ];
    assert_eq!(drawn_entity_texts(&fixture), want);
    let unnamed = "C4Context\nPerson($sprite=\"a\", \"Ann\")\nPerson($sprite=\"b\", \"Bo\")\nSystem(shop, \"Shop\")";
    let unnamed = scene(unnamed, &LayoutOptions::default()).expect("it draws");
    assert_eq!(drawn_entity_texts(&unnamed), ["node:", "node:#1", "node:shop"], "elements with no alias rank apart");
}

#[test]
fn a_key_survives_an_edit_elsewhere() {
    let before = rune_fixture("c4");
    let after = edit(&before, "  Person(customer", "  Person(admin, \"Admin\", \"Runs the shop\")\n  Person(customer");
    let after = edit(&after, "\"Bank\"", "\"Card network\"");
    let after = after + "  Rel(admin, checkout, \"Configures\")\n  System(ledger, \"Ledger\")\n";
    keys_survive(&before, &after, &["node:bank"]);
}
