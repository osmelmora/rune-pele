//! A timeline's Scene read the way an interpreter with no SVG reads it: each drawing has a key of
//! its own, across and turned to run down, and a period's keys do not move when an event is added
//! to the period before it.

#[path = "differential/harness.rs"]
mod harness;

use harness::{drawn_keys, harvest, scene};
use pele_core::LayoutOptions;

#[test]
fn every_drawing_has_a_key_of_its_own() {
    let narrow = LayoutOptions::default().max_width(320.0);
    let mut drawn = 0;
    for source in harvest("timeline", "sources") {
        for options in [&LayoutOptions::default(), &narrow] {
            if let Some(scene) = scene(&source, options) {
                drawn_keys(&scene, &source);
                drawn += 1;
            }
        }
    }
    assert!(drawn > 1500, "the harvest draws");
}

#[test]
fn a_period_keeps_its_keys_when_the_one_before_it_grows() {
    let of = |source: &str| {
        let scene = scene(source, &LayoutOptions::default()).expect("it draws");
        let keys = drawn_keys(&scene, source);
        keys.into_iter().filter(|key| key.starts_with("node:2002")).collect::<Vec<_>>()
    };
    let before = of("timeline\n  section S\n  2001 : a\n  2002 : b : c");
    let after = of("timeline\n  section S\n  2001 : a : x : y\n  2002 : b : c");
    assert!(before.len() >= 5, "a period draws a chip, its label, its line, its dot and its events: {before:?}");
    assert_eq!(before, after);
}
