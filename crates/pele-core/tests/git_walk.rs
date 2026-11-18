//! A git graph's Scene read the way an interpreter with no SVG reads it: each commit is an entity
//! named by its id, each branch by its name and each edge by the commits it joins, every drawing
//! has a key of its own across and down, and a key stays put when the source changes elsewhere.

#[path = "differential/harness.rs"]
mod harness;

use harness::{
    Ident, drawn_entity_texts, drawn_keys, edit, keys_survive, named, names_exactly, ranked, rune_fixture, scene,
    walk_sources,
};
use pele_core::scene::{EntityKind, EntityName};
use pele_core::{LayoutOptions, ParseOptions};
use serde_json::Value;

const MERGE: u64 = 3;

/// The entities a git model names: its commits by id, its branches by name where they are shown,
/// and an edge from each parent to its child. An edge between two branches is a merge from a
/// merge's second parent and a cherry-pick from another commit's.
fn named_in(model: &Value, branches_shown: bool) -> Vec<Ident> {
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    let commits = model["commits"].as_array().expect("a list");
    let lanes = model["lanes"].as_array().expect("a list");
    let mut out = ranked(EntityKind::Node, commits.iter().map(|commit| named(&text(&commit["id"]))));
    if branches_shown {
        out.extend(ranked(EntityKind::Group, lanes.iter().map(|lane| named(&text(lane)))));
    }
    let mut edges = Vec::new();
    for commit in commits {
        for (place, parent) in commit["parents"].as_array().expect("a list").iter().enumerate() {
            let Some(parent) = commits.iter().find(|other| other["id"] == *parent && other["id"] != commit["id"])
            else {
                continue;
            };
            let kind = if parent["branch"] == commit["branch"] || place == 0 {
                "parent"
            } else if commit["type"] == MERGE {
                "merge"
            } else {
                "cherry-pick"
            };
            let (from, to) = (text(&parent["id"]), text(&commit["id"]));
            let (from, to) = (from.encode_utf16().collect(), to.encode_utf16().collect());
            edges.push(EntityName::Between { from, to, kind: kind.to_owned() });
        }
    }
    out.extend(ranked(EntityKind::Edge, edges));
    out
}

#[test]
fn every_element_is_drawn_under_a_key_of_its_own() {
    let down = LayoutOptions { max_width: Some(320.0), ..LayoutOptions::default() };
    let (mut drawn, mut hidden, mut unread) = (0, 0, 0);
    for source in walk_sources("git") {
        let Ok(model) = pele_core::git_front_json(&source, ParseOptions::default()) else { continue };
        // A name with a lone surrogate is JSON that serde does not read, so only its keys are walked.
        let model = match serde_json::from_str::<Value>(&model) {
            Ok(model) if model.get("model").is_none() => continue,
            Ok(model) => Some(model),
            Err(_) => None,
        };
        for options in [&LayoutOptions::default(), &down] {
            let Some(scene) = scene(&source, options) else { continue };
            drawn_keys(&scene, &source);
            // `showBranches: false` draws no branch, so the Scene names none.
            let branches_shown = scene.entities().any(|entity| entity.kind() == EntityKind::Group);
            hidden += usize::from(!branches_shown);
            match &model {
                Some(model) => names_exactly(&scene, &named_in(&model["model"], branches_shown), &source),
                None => unread += 1,
            }
            drawn += 1;
        }
    }
    println!("git drawings walked: {drawn}. Their names unread: {unread}. Drawn with branches hidden: {hidden}.");
    assert!(drawn > 1500, "the corpus, the fixture and the harvest draw at both widths");
}

#[test]
fn keys_come_from_commit_ids_and_branch_names() {
    let fixture = scene(&rune_fixture("git"), &LayoutOptions::default()).expect("the fixture draws");
    let want = [
        "group:main",
        "group:develop",
        "group:hotfix",
        "edge:init~api~parent",
        "edge:tests~release~merge",
        "edge:release~patch~parent",
        "edge:patch~6-538453d~cherry-pick",
        "edge:api~tests~parent",
        "edge:init~docs~parent",
        "edge:docs~release~parent",
        "edge:release~6-538453d~parent",
        "node:init",
        "node:api",
        "node:tests",
        "node:docs",
        "node:release",
        "node:patch",
        "node:6-538453d",
    ];
    assert_eq!(drawn_entity_texts(&fixture), want, "a cherry-pick takes the id Pele makes up for it");
}

#[test]
fn a_key_survives_an_edit_elsewhere() {
    let before = rune_fixture("git");
    let after = edit(&before, "\"Add the API\"", "\"Add the public API\"");
    let after = after
        + "    branch docs-site\n    checkout docs-site\n    commit id: \"site\"\n    checkout main\n    commit\n";
    keys_survive(&before, &after, &["node:api"]);
    let branch = "    branch spike\n    checkout spike\n    commit id: \"try\"\n    checkout main\n    branch develop";
    let named_before = edit(&before, "    branch develop", branch);
    let cherry_picked = ["node:6-538453d", "edge:patch~6-538453d~cherry-pick", "edge:release~6-538453d~parent"];
    keys_survive(&before, &named_before, &cherry_picked);
}
