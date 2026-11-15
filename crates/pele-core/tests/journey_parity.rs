//! Differential test for drawing user journeys: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/journey/harvest.json` holds the journey sources of Pele's own test suite, the
//! inputs of its mutated runs and its worst cases, written by `scripts/harvest-journey.mjs`. With
//! one corpus source, the harvest, the hand-written cases and the random journeys carry the proof,
//! and the random journeys must reach every drawing path in [`PATHS`]. A journey is drawn as steps
//! running down where the host is narrow, so the harvest is drawn at four widths.

#[path = "differential/harness.rs"]
mod harness;

use harness::{Path, Rng, harvest, plain, run_at_width, run_render};
use serde_json::{Value, json};

/// Each group that starts with `start`, up to its first `</g>`.
fn groups<'a>(svg: &'a str, start: &str) -> Vec<&'a str> {
    svg.split(start).skip(1).map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())]).collect()
}

/// The width of each task's card.
fn card_widths(svg: &str) -> Vec<f64> {
    groups(svg, "<g class=\"pele-node pele-task\"")
        .iter()
        .filter_map(|card| card.split_once(" width=\"")?.1.split('"').next()?.parse().ok())
        .collect()
}

/// The data-id of each band, in order.
fn bands(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-cluster pele-section\" data-id=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect()
}

/// The rows of the legend, by the y of their dots.
fn legend_rows(svg: &str) -> usize {
    let mut rows: Vec<&str> = groups(svg, "<g class=\"pele-legend-item\"")
        .iter()
        .filter_map(|item| item.split_once(" cy=\"")?.1.split('"').next())
        .collect();
    rows.dedup();
    rows.len()
}

const PATHS: &[Path] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("an accessible title and description", |_, svg| {
        svg.contains("aria-labelledby=") && svg.contains("aria-describedby=")
    }),
    ("a happy face", |_, svg| svg.contains("pele-face-happy")),
    ("a neutral face", |_, svg| svg.contains("pele-face-neutral")),
    ("a sad face", |_, svg| svg.contains("pele-face-sad")),
    ("a score above five drawn at five", |source, svg| source.contains(": 6") && svg.contains("pele-scale")),
    ("a score that is no number drawn at three", |source, svg| source.contains(": x") && svg.contains("pele-scale")),
    ("a line between the scores", |_, svg| {
        svg.contains("class=\"pele-edge\" d=\"M") && svg.contains("stroke-linejoin")
    }),
    ("one task, with no line", |_, svg| svg.matches("pele-node pele-task").count() == 1),
    ("bands over sections", |_, svg| svg.contains("pele-cluster pele-section")),
    ("a section that comes back", |_, svg| {
        let bands = bands(svg);
        bands.iter().enumerate().any(|(i, band)| bands[i + 1..].contains(band))
    }),
    ("tasks before the first section", |_, svg| {
        svg.contains("pele-cluster pele-section")
            && svg.split("pele-cluster pele-section").next().is_some_and(|head| head.contains("pele-task"))
    }),
    ("actors with their colours", |_, svg| {
        svg.contains("<circle class=\"pele-actor\"") && svg.contains("--pele-series-2,")
    }),
    ("an actor's name as its dot's tooltip", |_, svg| svg.contains("\"><title>")),
    ("dots drawn closer for many actors", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-task\"").iter().any(|card| card.matches("pele-actor").count() > 7)
    }),
    ("tasks with no actors", |_, svg| svg.contains("pele-task") && !svg.contains("pele-actor")),
    ("a legend over two rows", |_, svg| legend_rows(svg) > 1),
    ("cards widened for a long word", |_, svg| card_widths(svg).iter().any(|&w| w > 144.0 && w < 220.0)),
    ("cards at their widest", |_, svg| card_widths(svg).contains(&220.0)),
    ("a task over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-task\"").iter().any(|card| card.matches("<tspan").count() > 1)
    }),
    ("steps running down", |_, svg| svg.contains("pele-journey-step")),
    ("a section's name over steps", |_, svg| svg.contains("pele-cluster-label")),
    ("a step's score", |_, svg| svg.contains("pele-journey-score")),
    ("a step with no score", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-journey-step\"").iter().any(|step| !step.contains("pele-journey-score"))
    }),
    ("a step's actors", |_, svg| svg.contains("pele-journey-actor")),
    ("an empty journey", |_, svg| !svg.contains("pele-task") && !svg.contains("pele-journey-step")),
];

fn cases() -> Vec<(String, Value)> {
    let day = "journey\n    title My working day\n    section Go to work\n      Make tea: 5: Me\n      Go upstairs: 3: Me\n      Do work: 1: Me, Cat\n    section Go home\n      Go downstairs: 5: Me\n      Sit down: 5: Me\n";
    let crowded = "journey\n  Meeting: 3: Ann, Bob, Cy, Dee, Eve, Fay, Gus, Hal, Ivy, Jo\n  Lunch: 4: Ann\n";
    let odd = "journey\n  A: 6\n  B: -2\n  C: x: Me\n  D: 3.5:\n  E: 5: , Me,\n";
    let mut out = vec![
        (day.to_owned(), json!({})),
        (crowded.to_owned(), json!({})),
        (odd.to_owned(), json!({})),
        ("journey".to_owned(), json!({})),
        ("journey\n  section Empty".to_owned(), json!({})),
        ("journey\n  Only: 4: Me".to_owned(), json!({})),
        (
            "journey\n  First: 3\n  section S\n  One<br>two: 2: A, B, C\n  section T\n  section S\n  Back: 4: A"
                .to_owned(),
            json!({}),
        ),
        (
            "journey\n  title A title wider than three hundred and twenty pixels, which is quite a lot of text\n  T: 3"
                .to_owned(),
            json!({}),
        ),
        ("journey\n  Averyveryverylongtasknamethatdoesnotfitatall: 5: Me".to_owned(), json!({})),
        ("journey\n  accTitle: a\n  accDescr { b }\n  First: 3\n".to_owned(), json!({ "idPrefix": "j" })),
    ];
    for options in [
        json!({ "maxWidth": 200 }),
        json!({ "maxWidth": 320 }),
        json!({ "maxWidth": 480 }),
        json!({ "maxWidth": 639 }),
        json!({ "maxWidth": 640 }),
        json!({ "maxWidth": 320, "autoDirection": false }),
        json!({ "maxWidth": 700, "directionBreakpoint": 800 }),
        json!({ "maxWidth": 320, "padding": 0 }),
        json!({ "fontSize": 12 }),
        json!({ "fontSize": 22, "maxWidth": 400 }),
        json!({ "responsive": false }),
    ] {
        out.push((day.to_owned(), options.clone()));
        out.push((crowded.to_owned(), options.clone()));
        out.push((odd.to_owned(), options));
    }
    out
}

#[test]
fn cases_match_pele() {
    run_render("journey", "case", &cases(), PATHS).finish("journey cases");
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run_render("journey", "harvested", &plain(harvest("journey", "sources")), PATHS)
        .finish("harvested journey sources");
    assert!(tally.identical > 700, "the harvest holds journeys Pele draws");
    assert!(tally.rejected > 100, "the harvest holds journeys Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("journey", "sources");
    for width in [320.0, 480.0, 640.0, 900.0] {
        let tally = run_at_width("journey", "harvested", &sources, width, PATHS)
            .finish(&format!("harvested journey sources at maxWidth {width}"));
        println!("  maxWidth {width} makes Pele draw {} of them another way", tally.reacted);
    }
}

#[test]
fn mutated_sources_match_pele() {
    run_render("journey", "mutated", &plain(harvest("journey", "mutated")), PATHS).finish("mutated journey sources");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> =
        harvest("journey", "worst").into_iter().map(|source| (source, json!({ "limit": 100_000_000 }))).collect();
    let tally = run_render("journey", "worst", &cases, PATHS).finish("Pele's worst-case journeys");
    assert!(tally.identical > 20, "the worst cases are drawn, not rejected");
}

const TASKS: &[&str] = &[
    "Make tea",
    "Go upstairs",
    "Do work",
    "A task with a name long enough to wrap",
    "Internationalisation",
    "Averyveryverylongtasknamethatdoesnotfitatall",
    "\u{e9}\u{91c7}\u{1f600}",
    "<b>b</b> & <i>c</i>",
    "one<br>two",
    "x",
];
const SCORES: &[&str] = &["5", "5", "4", "3", "2", "1", "0", "6", "3.5", "x", "-1"];
const ACTORS: &[&str] = &["Me", "Cat", "Dog", "Alice", "Bob", "Charlie", "", "A very long actor name indeed"];
const SECTIONS: &[&str] = &["Go to work", "Go home", "A section with a long name", "Work"];

/// A journey Pele draws: tasks with scores of every mood, actors, sections that come back, and
/// the host options that change the drawing.
fn random_journey(rng: &mut Rng) -> (String, Value) {
    let mut src = String::from("journey\n");
    if rng.chance(30) {
        src += rng.pick(&[
            "  title My working day\n",
            "  title A title much wider than the journey below it, as wide as can be\n",
        ]);
    }
    if rng.chance(8) {
        src += "  accTitle: Acc\n  accDescr: What it shows\n";
    }
    let sectioned = rng.chance(50);
    let count = if rng.chance(8) { 8 + rng.below(8) } else { rng.below(6) };
    for i in 0..count {
        if sectioned && (i > 0 || rng.chance(80)) && (i == 0 || rng.chance(30)) {
            src += &format!("  section {}\n", rng.pick(SECTIONS));
        }
        src += &format!("    {}: {}", rng.pick(TASKS), rng.pick(SCORES));
        if rng.chance(75) {
            let people = if rng.chance(8) { 6 + rng.below(8) } else { 1 + rng.below(3) };
            let actors: Vec<&str> = (0..people).map(|_| rng.pick(ACTORS)).collect();
            src += &format!(": {}", actors.join(", "));
        }
        src += "\n";
    }
    let options = match rng.below(8) {
        0 | 1 => {
            let width = [200, 320, 480, 600, 640, 900][rng.below(6)];
            json!({ "maxWidth": width })
        }
        2 => {
            let size = [12, 14, 20][rng.below(3)];
            json!({ "fontSize": size })
        }
        3 => json!({ "padding": 0 }),
        _ => json!({}),
    };
    (src, options)
}

#[test]
fn random_journeys_match_pele() {
    let mut rng = Rng(0x1a7e_d4a3_7ee3_000d);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_journey(&mut rng)).collect();
    let tally = run_render("journey", "random", &cases, PATHS).finish("random journeys");
    assert!(tally.identical > 3900, "random journeys are ones Pele draws");
    tally.require(PATHS);
}
