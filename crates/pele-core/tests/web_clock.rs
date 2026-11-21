//! A gantt chart laid out with no `now` marks today by the platform's clock, as Pele's does by
//! `Date.now()`: the standard library's natively, and the page's on wasm32-unknown-unknown, where
//! the standard library's panics. `mise run web-check` runs this test there, in Node.

use pele_core::scene::PaintVisit;
use pele_core::{Font, LayoutOptions, NativeInputs, ParseOptions, TextMeasure, TextMeasurer, TextRef, prepare};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

/// One task from 2000 to 2100, so that today lies on the chart.
const CENTURY: &str =
    "gantt\n    dateFormat YYYY-MM-DD\n    section Century\n    The whole of it :2000-01-01, 2100-01-01\n";

/// Every character half an em wide: the test needs a layout, not Pele's widths.
struct HalfEm;

impl TextMeasurer for HalfEm {
    fn measure(&self, text: TextRef<'_>, font: &Font) -> TextMeasure {
        TextMeasure { width: text.utf16_units().len() as f64 * font.size / 2.0 }
    }
}

fn drawn(now: Option<f64>) -> Vec<PaintVisit> {
    let options = match now {
        Some(now) => LayoutOptions::default().now(now),
        None => LayoutOptions::default(),
    };
    let prepared = prepare(CENTURY, ParseOptions::default()).expect("the chart parses");
    let scene = prepared.scene(NativeInputs { measure: &HalfEm, options: &options }).expect("the chart draws");
    scene.paint().collect()
}

/// The platform's clock in whole milliseconds since 1970, read as the chart reads it.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn clock() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("after 1970");
    since.as_millis() as u64
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn clock() -> u64 {
    js_sys::Date::now() as u64
}

#[test]
fn an_unset_now_is_the_platforms_clock() {
    let before = clock();
    let unset = drawn(None);
    let after = clock();
    assert!(
        (before..=after).any(|ms| drawn(Some(ms as f64)) == unset),
        "the chart with no `now` is the chart at a millisecond read between {before} and {after}"
    );
    assert_ne!(drawn(Some(0.0)), unset, "and 1970, off the chart, draws no today");
}
