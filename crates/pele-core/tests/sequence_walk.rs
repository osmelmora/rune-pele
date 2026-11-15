//! A sequence diagram's Scene read the way an interpreter with no SVG reads it: every drawing
//! has a key of its own, a key names what its drawing is and not where it stands, and the host's
//! monospace setting reaches the messages' text and nothing else.

use pele_core::scene::{BaselineRun, Geometry, PaintVisit, Scene};
use pele_core::{FontFamily, FontWeight, LayoutOptions, MonospacePart, ParseOptions};
use std::collections::HashSet;

fn scene(source: &str, layout: &LayoutOptions) -> Scene {
    pele_core::parity_scene_with(source, ParseOptions::default(), layout)
        .expect("a sequence diagram draws")
        .scene()
        .clone()
}

/// Each drawing's key with its text, where it is text.
fn drawings(scene: &Scene) -> Vec<(String, Option<BaselineRun>)> {
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => {
                let text = match draw.geometry {
                    Geometry::Text(run) => Some(run),
                    _ => None,
                };
                Some((draw.key.to_string(), text))
            }
            _ => None,
        })
        .collect()
}

fn key_of<'a>(drawn: &'a [(String, Option<BaselineRun>)], text: &str) -> &'a str {
    drawn
        .iter()
        .find(|(_, run)| run.as_ref().is_some_and(|run| run.text == text))
        .map(|(key, _)| key.as_str())
        .unwrap_or_else(|| panic!("no run {text:?}"))
}

fn run<'a>(drawn: &'a [(String, Option<BaselineRun>)], text: &str) -> &'a BaselineRun {
    drawn
        .iter()
        .filter_map(|(_, run)| run.as_ref())
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("no run {text:?}"))
}

const CHAT: &str = "sequenceDiagram\ntitle Chat\nautonumber\nbox Aqua Team\nparticipant A as Alice\nactor B\nend\nA->>+B: hello\nNote over A,B: a note\nloop each day\nB-->>-A: reply\nB-->>A: reply again\nend\nalt first\nA->>A: think\nelse second\nA-xB: stop\nend\nrect rgb(0, 0, 0)\nA()->>()B: both\nend\ncreate participant C\nA->>C: make\ndestroy C\nC-xA: leave";

#[test]
fn every_drawing_has_a_key_of_its_own() {
    for layout in [LayoutOptions::default(), LayoutOptions { max_width: Some(200.0), ..LayoutOptions::default() }] {
        let drawn = drawings(&scene(CHAT, &layout));
        let mut seen = HashSet::new();
        let repeated: Vec<&String> = drawn.iter().map(|(key, _)| key).filter(|key| !seen.insert(*key)).collect();
        assert!(repeated.is_empty(), "keys drawn twice: {repeated:?}");
        let keys: HashSet<&str> = drawn.iter().map(|(key, _)| key.as_str()).collect();
        for want in [
            "diagram:/title/0.0",
            "group:Team/shape",
            "group:Team/title/0.0",
            "group:loop/shape",
            "group:loop/stereotype/0.0",
            "group:alt/label/0.0",
            "group:rect/shape",
            "node:A/shape",
            "node:A/label/0.0",
            "node:C/shape",
            "edge:A~B~0/shape",
            "edge:A~B~0/end",
            "edge:A~B~0/start-label/0.0",
            "edge:A~B~0/label/0.0",
            "edge:B~A~1/label/0.0",
            "edge:B~A~1#1/label/0.0",
            "note:A~B~note 2/shape",
            "note:A~B~note 2/label/0.0",
        ] {
            assert!(keys.contains(want), "no drawing has the key {want}; the keys are {keys:?}");
        }
    }
}

#[test]
fn a_key_survives_what_is_written_before_and_inside_it() {
    let before = drawings(&scene(CHAT, &LayoutOptions::default()));
    let edited = CHAT
        .replace("participant A as Alice", "participant Z\nparticipant A as Alice")
        .replace("A->>+B: hello", "Z->>A: first\nNote over Z: another\nA->>+B: hello there")
        .replace("B-->>-A: reply\n", "B-->>-A: a longer reply\n");
    let after = drawings(&scene(&edited, &LayoutOptions::default()));
    for (was, now) in [("hello", "hello there"), ("reply", "a longer reply"), ("a note", "a note"), ("stop", "stop")] {
        assert_eq!(key_of(&before, was), key_of(&after, now), "{was:?} keeps its key as {now:?}");
    }
    assert_eq!(key_of(&before, "Alice"), key_of(&after, "Alice"));
    assert_eq!(key_of(&before, "[each day]"), key_of(&after, "[each day]"));
}

#[test]
fn the_host_sets_only_the_messages_in_mono() {
    let mono = LayoutOptions { monospace: vec![MonospacePart::Messages], ..LayoutOptions::default() };
    let drawn = drawings(&scene(CHAT, &mono));
    for text in ["hello", "reply", "think", "leave"] {
        assert_eq!(run(&drawn, text).font.family, FontFamily::Mono, "the host sets {text:?} in mono");
        assert_eq!(run(&drawn, text).font.size, 14.0, "messages are set at seven eighths of the size");
    }
    for text in ["a note", "Alice", "[each day]", "loop", "Chat", "1"] {
        assert_eq!(run(&drawn, text).font.family, FontFamily::Sans, "{text:?} stays in its font");
    }
    assert_eq!(run(&drawn, "loop").font.weight, FontWeight::Bold, "a block's kind is set in the heading weight");
    assert_eq!(run(&drawn, "loop").font.size, 12.0, "a block's kind is set at three quarters of the size");
    assert_eq!(run(&drawn, "Chat").font.weight, FontWeight::Bold, "the title is bold");

    let plain = drawings(&scene(CHAT, &LayoutOptions::default()));
    assert_eq!(run(&plain, "hello").font.family, FontFamily::Sans, "messages are in the label font otherwise");
}
