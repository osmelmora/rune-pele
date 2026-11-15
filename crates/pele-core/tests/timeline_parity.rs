//! Differential test for drawing timelines: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele rejects
//! must be rejected here with Pele's message.
//!
//! `tests/fixtures/timeline/harvest.json` holds the timeline sources of Pele's own test suite, the
//! inputs of its mutated runs and its worst cases, written by `scripts/harvest-timeline.mjs`. A
//! timeline that runs across is drawn running down where the host is narrow, so the harvest is
//! drawn at four widths, and the random timelines must reach every drawing path in [`PATHS`].

#[path = "differential/harness.rs"]
mod harness;

use harness::{Path, Rng, harvest, plain, run_at_width, run_render};
use serde_json::{Value, json};

/// Each group that starts with `start`, up to its first `</g>`.
fn groups<'a>(svg: &'a str, start: &str) -> Vec<&'a str> {
    svg.split(start).skip(1).map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())]).collect()
}

/// Each period's group, with the event cards in it.
fn periods(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-node pele-period\"")
        .skip(1)
        .map(|rest| &rest[..rest.find("</g></g>").or_else(|| rest.find("</g>")).unwrap_or(rest.len())])
        .collect()
}

/// The width of each period's chip.
fn chip_widths(svg: &str) -> Vec<f64> {
    periods(svg).iter().filter_map(|period| period.split_once(" width=\"")?.1.split('"').next()?.parse().ok()).collect()
}

fn down(svg: &str) -> bool {
    svg.split("class=\"pele-axis\" d=\"").nth(1).is_some_and(|d| d.split('"').next().is_some_and(|d| d.contains('V')))
}

const PATHS: &[Path] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("a title wider than the periods", |source, svg| source.contains("title A title") && svg.contains("pele-period")),
    ("an accessible title and description", |_, svg| {
        svg.contains("aria-labelledby=") && svg.contains("aria-describedby=")
    }),
    ("running across", |_, svg| svg.contains("pele-axis") && !down(svg)),
    ("running down as written", |source, svg| source.contains(" TD") && down(svg)),
    ("turned to run down for the host", |source, svg| !source.to_lowercase().contains(" td") && down(svg)),
    ("sections", |_, svg| svg.matches("pele-cluster pele-section").count() > 1),
    ("a section with no periods", |_, svg| {
        groups(svg, "<g class=\"pele-cluster pele-section\"").iter().any(|s| !s.contains("pele-period"))
    }),
    ("periods before the first section", |_, svg| svg.contains("pele-section") && svg.contains("fill=\"var(--_m)\"")),
    ("a colour for each period", |_, svg| !svg.contains("pele-section") && svg.contains("--pele-series-2,")),
    ("one colour for every period", |source, svg| {
        source.contains("disableMulticolor: true") && svg.contains("pele-period")
    }),
    ("colours that come round again", |_, svg| {
        svg.contains("--pele-series-8,") && svg.matches("pele-dot\" cx=").count() > 8
    }),
    ("several events of one period", |_, svg| periods(svg).iter().any(|p| p.matches("pele-event").count() > 1)),
    ("a period with no events", |_, svg| periods(svg).iter().any(|p| !p.contains("pele-event"))),
    ("an event over two lines", |_, svg| {
        groups(svg, "<g class=\"pele-node pele-event\"").iter().any(|e| e.matches("<tspan").count() > 1)
    }),
    ("a period over two lines", |_, svg| {
        periods(svg).iter().any(|p| p.split("pele-edge").next().is_some_and(|chip| chip.matches("<tspan").count() > 1))
    }),
    ("columns widened for a long word", |_, svg| chip_widths(svg).iter().any(|&w| w > 176.0 && w < 240.0)),
    ("columns at their widest", |_, svg| chip_widths(svg).contains(&240.0)),
    ("text on a colour from the theme", |_, svg| svg.contains("fill=\"var(--pele-fg-on-")),
    ("an empty timeline", |_, svg| !svg.contains("pele-axis")),
    ("an empty section name", |source, svg| source.contains("section \n") && svg.contains("pele-section")),
];

fn cases() -> Vec<(String, Value)> {
    let history = "timeline\n    title History of Social Media Platform\n    2002 : LinkedIn\n    2004 : Facebook : Google\n    2005 : YouTube\n    2006 : Twitter\n";
    let sections = "timeline\n    title Timeline of Industrial Revolution\n    section 17th-20th century\n        Industry 1.0 : Machinery, Water power, Steam <br>power\n        Industry 2.0 : Electricity, Internal combustion engine, Mass production\n        Industry 3.0 : Electronics, Computers, Automation\n    section 21st century\n        Industry 4.0 : Internet, Robotics, Internet of Things\n        Industry 5.0 : Artificial intelligence, Big data, 3D printing\n";
    let long = "timeline\n    2001 : Unbreakableunbreakableunbreakableunbreakableunbreakable\n    Averyveryverylongperiodnamethatdoesnotfit : event";
    let leading =
        "timeline\n    1999 : before\n    section A\n    2001 : a : b\n    section B\n    section C\n    2002";
    let many = format!("timeline\n{}", (0..11).map(|i| format!("    {} : e{i}\n", 2000 + i)).collect::<String>());
    let mut out = vec![
        (history.to_owned(), json!({})),
        (sections.to_owned(), json!({})),
        (long.to_owned(), json!({})),
        (leading.to_owned(), json!({})),
        (many.clone(), json!({})),
        (format!("timeline TD\n{}", &sections[9..]), json!({})),
        (format!("timeline TD\n{}", &long[9..]), json!({ "maxWidth": 300 })),
        ("timeline".to_owned(), json!({})),
        ("timeline TD".to_owned(), json!({})),
        ("timeline\n    section Only".to_owned(), json!({})),
        ("timeline TD\n    section Only\n    section Two".to_owned(), json!({})),
        ("timeline\n    title A title much wider than the timeline it sits over, wider than three hundred and twenty pixels\n    2001".to_owned(), json!({})),
        ("---\nconfig:\n  timeline:\n    disableMulticolor: true\n---\n".to_owned() + history, json!({})),
        ("---\nconfig:\n  timeline:\n    disableMulticolor: \"true\"\n---\n".to_owned() + history, json!({})),
        ("---\nconfig:\n  timeline: \"x\"\n---\n".to_owned() + history, json!({})),
        ("timeline\n    accTitle: Acc\n    accDescr: Descr\n    2001 : a".to_owned(), json!({ "idPrefix": "t" })),
    ];
    for options in [
        json!({ "maxWidth": 320 }),
        json!({ "maxWidth": 480 }),
        json!({ "maxWidth": 639 }),
        json!({ "maxWidth": 640 }),
        json!({ "maxWidth": 320, "autoDirection": false }),
        json!({ "maxWidth": 700, "directionBreakpoint": 800 }),
        json!({ "maxWidth": 100 }),
        json!({ "maxWidth": 320, "padding": 0 }),
        json!({ "fontSize": 12 }),
        json!({ "fontSize": 22, "maxWidth": 400 }),
        json!({ "responsive": false }),
    ] {
        out.push((history.to_owned(), options.clone()));
        out.push((sections.to_owned(), options.clone()));
        out.push((many.clone(), options));
    }
    for colours in [json!({ "cScale0": "#000", "cScale1": "#fff", "cScale2": "navy" }), json!({ "cScale0": "url(x)" })]
    {
        out.push((history.to_owned(), json!({ "config": { "themeVariables": colours } })));
        out.push((sections.to_owned(), json!({ "config": { "themeVariables": colours } })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run_render("timeline", "case", &cases(), PATHS).finish("timeline cases");
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run_render("timeline", "harvested", &plain(harvest("timeline", "sources")), PATHS)
        .finish("harvested timeline sources");
    assert!(tally.identical > 700, "the harvest holds timelines Pele draws");
    assert!(tally.rejected > 20, "the harvest holds timelines Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("timeline", "sources");
    for width in [320.0, 480.0, 640.0, 900.0] {
        let tally = run_at_width("timeline", "harvested", &sources, width, PATHS)
            .finish(&format!("harvested timeline sources at maxWidth {width}"));
        println!("  maxWidth {width} makes Pele draw {} of them another way", tally.reacted);
    }
}

#[test]
fn mutated_sources_match_pele() {
    run_render("timeline", "mutated", &plain(harvest("timeline", "mutated")), PATHS).finish("mutated timeline sources");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> =
        harvest("timeline", "worst").into_iter().map(|source| (source, json!({ "limit": 100_000_000 }))).collect();
    let tally = run_render("timeline", "worst", &cases, PATHS).finish("Pele's worst-case timelines");
    assert!(tally.identical > 25, "the worst cases are drawn, not rejected");
}

const PERIOD_NAMES: &[&str] = &[
    "2001",
    "Q1 2020",
    "1990s",
    "A period with a name long enough to wrap over lines",
    "Averyveryverylongperiodnamethatdoesnotfitatall",
    "Internationalisation",
    "\u{e9}\u{91c7}\u{1f600}",
    "#35; &amp; <b>b</b>",
    "x",
];
const EVENT_TEXTS: &[&str] = &[
    "LinkedIn",
    "Facebook",
    "Electricity, Internal combustion engine, Mass production",
    "two<br>lines",
    "Unbreakableunbreakableunbreakableunbreakableunbreakableunbreakable",
    "Counterrevolutionaries",
    "a",
    "<i>it</i> and *em*",
];
const SECTION_NAMES: &[&str] =
    &["17th-20th century", "21st century", "A", "A section with a long name that needs to wrap", ""];

/// A timeline Pele draws: periods with events, sections, and the config and host options that
/// change the drawing.
fn random_timeline(rng: &mut Rng) -> (String, Value) {
    let mut src = String::new();
    let multicolor = rng.chance(10);
    let palette = rng.chance(8);
    if multicolor || palette {
        src += "---\nconfig:\n";
        if multicolor {
            src += "  timeline:\n    disableMulticolor: true\n";
        }
        if palette {
            src += "  themeVariables:\n    cScale0: \"#111\"\n    cScale1: \"#fafafa\"\n    cScale2: navy\n";
        }
        src += "---\n";
    }
    src += rng.pick(&["timeline\n", "timeline\n", "timeline LR\n", "timeline TD\n"]);
    if rng.chance(30) {
        src += rng
            .pick(&["  title History\n", "  title A title much wider than the timeline below it, as wide as can be\n"]);
    }
    if rng.chance(8) {
        src += "  accTitle: Acc\n  accDescr: What it shows\n";
    }
    let sectioned = rng.chance(45);
    let leading = sectioned && rng.chance(20);
    let count = if rng.chance(10) { 9 + rng.below(6) } else { rng.below(6) };
    for i in 0..count {
        if sectioned && (i > 0 || !leading) && (i == 0 || rng.chance(35)) {
            let name = rng.pick(SECTION_NAMES);
            src += &format!("  section {name}\n");
            if rng.chance(10) {
                src += &format!("  section {}\n", rng.pick(SECTION_NAMES));
            }
        }
        src += &format!("    {}", rng.pick(PERIOD_NAMES));
        for _ in 0..rng.below(4) {
            src += &format!(" : {}", rng.pick(EVENT_TEXTS));
        }
        src += "\n";
        if rng.chance(15) {
            src += &format!("         : {}\n", rng.pick(EVENT_TEXTS));
        }
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
fn random_timelines_match_pele() {
    let mut rng = Rng(0x71e1_17e5_d4a3_0009);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_timeline(&mut rng)).collect();
    let tally = run_render("timeline", "random", &cases, PATHS).finish("random timelines");
    assert!(tally.identical > 3900, "random timelines are ones Pele draws");
    tally.require(PATHS);
}
