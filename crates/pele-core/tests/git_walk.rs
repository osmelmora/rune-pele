//! A git graph's Scene read the way an interpreter with no SVG reads it: each commit is an entity
//! named by its id, or with none by its branch and its place among that branch's commits with no
//! id, each branch by its name and each edge by the commits it joins, every drawing has a key of
//! its own across and down, and a key stays put when the source changes elsewhere.

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
const CHERRY_PICK: u64 = 4;

/// Whether Pele made the commit's id up: a cherry-pick's always, a merge's where its `customId`
/// says so, and another's where it is the commit's `seq`, a dash and seven hex digits, as Pele
/// makes them.
fn made_up(commit: &Value) -> bool {
    let id = commit["id"].as_str().expect("an id");
    let made = id.strip_prefix(&format!("{}-", commit["seq"])).is_some_and(|hash| {
        hash.len() == 7 && hash.bytes().all(|digit| digit.is_ascii_digit() || (b'a'..=b'f').contains(&digit))
    });
    match commit["type"].as_u64() {
        Some(CHERRY_PICK) => true,
        Some(MERGE) => commit["customId"] == false,
        _ => made,
    }
}

/// The entities a git model names: its commits by id, or with none by their branch in quotes and
/// their place among the commits so named, its branches by name where they are shown, and an
/// edge from each parent to its child, whose ends name a commit with no id `"branch"#place`. An
/// edge between two branches is a merge from a merge's second parent and a cherry-pick from
/// another commit's.
fn named_in(model: &Value, branches_shown: bool) -> Vec<Ident> {
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    let mut commits: Vec<&Value> = model["commits"].as_array().expect("a list").iter().collect();
    commits.sort_by_key(|commit| commit["seq"].as_u64());
    let lanes = model["lanes"].as_array().expect("a list");
    let name = |commit: &Value| match made_up(commit) {
        true => format!("\"{}\"", text(&commit["branch"])),
        false => text(&commit["id"]),
    };
    let mut out = ranked(EntityKind::Node, commits.iter().map(|commit| named(&name(commit))));
    let ends: Vec<Vec<u16>> = out
        .iter()
        .map(|(_, name, place)| {
            let EntityName::Named(name) = name else { unreachable!("a commit is named") };
            let place = if *place > 0 { format!("#{place}") } else { String::new() };
            name.iter().copied().chain(place.encode_utf16()).collect()
        })
        .collect();
    if branches_shown {
        out.extend(ranked(EntityKind::Group, lanes.iter().map(|lane| named(&text(lane)))));
    }
    let mut edges = Vec::new();
    for (child, commit) in commits.iter().enumerate() {
        for (place, parent) in commit["parents"].as_array().expect("a list").iter().enumerate() {
            let Some(from) = commits.iter().position(|other| other["id"] == *parent && other["id"] != commit["id"])
            else {
                continue;
            };
            let parent = commits[from];
            let kind = if parent["branch"] == commit["branch"] || place == 0 {
                "parent"
            } else if commit["type"] == MERGE {
                "merge"
            } else {
                "cherry-pick"
            };
            edges.push(EntityName::Between {
                from: ends[from].clone(),
                to: ends[child].clone(),
                kind: kind.to_owned(),
            });
        }
    }
    out.extend(ranked(EntityKind::Edge, edges));
    out
}

#[test]
fn every_element_is_drawn_under_a_key_of_its_own() {
    let down = LayoutOptions::default().max_width(320.0);
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
        "edge:patch~\"main\"~cherry-pick",
        "edge:api~tests~parent",
        "edge:init~docs~parent",
        "edge:docs~release~parent",
        "edge:release~\"main\"~parent",
        "node:init",
        "node:api",
        "node:tests",
        "node:docs",
        "node:release",
        "node:patch",
        "node:\"main\"",
    ];
    assert_eq!(drawn_entity_texts(&fixture), want, "a cherry-pick is named by its branch, as it has no id");
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
    keys_survive(&before, &named_before, &[]);
}

/// Commits with no id on three branches, each with a tag to tell it by. Pele labels such a commit
/// with the id it makes up, which counts every commit before it, so the labels are hidden.
const UNNAMED: &str = "---\nconfig:\n  gitGraph:\n    showCommitLabel: false\n---\ngitGraph\n    commit tag: \"m0\"\n    \
                       branch develop\n    checkout develop\n    commit tag: \"d0\"\n    commit id: \"api\"\n    \
                       commit tag: \"d1\"\n    checkout main\n    commit tag: \"m1\"\n    branch hotfix\n    \
                       checkout hotfix\n    commit tag: \"h0\"\n    checkout main\n    merge develop\n    \
                       cherry-pick id: \"api\"\n    merge hotfix\n    commit tag: \"m2\"\n";

#[test]
fn a_commit_with_no_id_added_to_one_branch_moves_no_key_of_another() {
    let fixture = scene(UNNAMED, &LayoutOptions::default()).expect("it draws");
    let drawn = drawn_entity_texts(&fixture);
    let on = |branch: &str, after: &str| {
        let edited = edit(UNNAMED, after, &format!("{after}    commit tag: \"new\"\n"));
        let moved: Vec<&str> = drawn.iter().map(String::as_str).filter(|key| key.contains(branch)).collect();
        keys_survive(UNNAMED, &edited, &moved);
    };
    on("\"develop\"", "    checkout develop\n");
    on("\"main\"", "    commit tag: \"m0\"\n");
    on("\"hotfix\"", "    checkout hotfix\n");
    for key in ["node:\"main\"#5", "node:\"develop\"#1", "node:\"hotfix\"", "edge:\"develop\"\\#1~\"main\"\\#2~merge"] {
        assert!(drawn.contains(&key.to_owned()), "{key} is drawn: {drawn:?}");
    }
}
