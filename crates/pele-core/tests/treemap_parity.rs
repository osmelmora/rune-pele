//! Differential test for drawing treemaps: every source must come out of this port as it comes out
//! of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele rejects
//! must be rejected here with Pele's message.
//!
//! `tests/fixtures/treemap/harvest.json` holds the treemap sources of Pele's own test suite, the
//! inputs of its mutated runs and its worst cases, written by `scripts/harvest-treemap.mjs`. With
//! eight corpus sources, the harvest, the hand-written cases and the random treemaps carry the
//! proof, and the random treemaps must reach every drawing path in [`PATHS`]. A treemap is drawn
//! to the host's width where that is smaller than its own, so the harvest is drawn at four.

#[path = "differential/harness.rs"]
mod harness;

use harness::{Path, RenderTally, Rng, harvest, plain, run_at_width, run_render};
use serde_json::{Value, json};

/// Each group that starts with `start`, up to the end of its group.
fn groups<'a>(svg: &'a str, start: &str) -> Vec<&'a str> {
    svg.split(start).skip(1).map(|rest| &rest[..rest.find("</g>").unwrap_or(rest.len())]).collect()
}

fn sections(svg: &str) -> Vec<&str> {
    groups(svg, "<g class=\"pele-node pele-section")
}

fn leaves(svg: &str) -> Vec<&str> {
    groups(svg, "<g class=\"pele-node pele-leaf")
}

/// The numbers a text's `font-size` attributes give.
fn font_sizes(svg: &str) -> Vec<f64> {
    svg.split("<text")
        .skip(1)
        .filter_map(|text| text.split_once(" font-size=\"")?.1.split('"').next()?.parse().ok())
        .collect()
}

const PATHS: &[Path] = &[
    ("a title", |_, svg| svg.contains("class=\"pele-title\"")),
    ("an accessible title and description", |_, svg| {
        svg.contains("aria-labelledby=") && svg.contains("aria-describedby=")
    }),
    ("a section with a header", |_, svg| sections(svg).iter().any(|s| s.contains("fill-opacity=\"0.12\""))),
    ("a section with its value in the header", |_, svg| {
        sections(svg).iter().any(|s| s.contains("class=\"pele-value\""))
    }),
    ("a value muted in a header", |_, svg| {
        sections(svg).iter().any(|s| s.contains("text-anchor=\"end\" fill=\"var(--_m)\""))
    }),
    ("a short header", |_, svg| sections(svg).iter().any(|s| s.contains("font-weight=\"var(--_hw)\" font-size="))),
    ("a section that gives its header to its rows", |_, svg| {
        sections(svg).iter().any(|s| s.contains("fill=\"none\"/>"))
    }),
    ("a section with no name drawn", |_, svg| {
        sections(svg).iter().any(|s| s.contains("fill-opacity") && !s.contains("<text"))
    }),
    ("a leaf with its name and value", |_, svg| leaves(svg).iter().any(|l| l.contains("class=\"pele-value\""))),
    ("a leaf with its name alone", |_, svg| {
        leaves(svg).iter().any(|l| l.contains("<text") && !l.contains("pele-value"))
    }),
    ("a leaf too small for text", |_, svg| leaves(svg).iter().any(|l| !l.contains("<text"))),
    ("a name cut short", |_, svg| svg.contains('\u{2026}')),
    ("a name shrunk to fit", |source, svg| {
        !source.contains("font-size") && font_sizes(svg).iter().any(|&s| s > 9.0 && s < 16.0 && s != 12.0 && s != 14.0)
    }),
    ("a tooltip on a leaf", |_, svg| leaves(svg).iter().any(|l| l.contains("<title>"))),
    ("a tooltip on a section", |_, svg| sections(svg).iter().any(|s| s.contains("<title>"))),
    ("a tooltip that names the sections a leaf covers", |_, svg| svg.contains(" \u{203a} ")),
    ("a corner capped for a small box", |_, svg| svg.contains("rx:min(var(--_r),")),
    ("a box of no size", |_, svg| svg.contains("width=\"0\"") || svg.contains("height=\"0\"")),
    ("a class that fills a section", |_, svg| {
        sections(svg).iter().any(|s| s.contains(";fill:") && !s.contains("fill-opacity"))
    }),
    ("text in a colour that contrasts with a class's fill", |_, svg| svg.contains("fill=\"var(--pele-fg-on-")),
    ("a class that styles text", |_, svg| {
        svg.split("<text").skip(1).any(|t| t.split('>').next().is_some_and(|t| t.contains(" style=\"")))
    }),
    ("author classes", |_, svg| svg.contains("pele-leaf ") || svg.contains("pele-section ")),
    ("values in another format", |source, svg| source.contains("valueFormat") && svg.contains("pele-value")),
    ("values hidden", |source, svg| {
        source.contains("showValues: false") && svg.contains("pele-leaf") && !svg.contains("pele-value")
    }),
    ("series colours from the theme", |_, svg| svg.contains(";--pele-series-1:")),
    ("a crowded treemap", |_, svg| leaves(svg).len() >= 150),
    ("a diagram drawn to the host's width", |_, svg| {
        svg.contains("viewBox=\"0 0 33") || svg.contains("viewBox=\"0 0 2")
    }),
];

fn cases() -> Vec<(String, Value)> {
    let basic = "treemap-beta\n\"Category A\"\n    \"Item A1\": 10\n    \"Item A2\": 20\n\"Category B\"\n    \"Item B1\": 15\n    \"Item B2\": 25\n";
    let config = |treemap: &str, body: &str| format!("---\nconfig:\n  treemap:\n{treemap}\n---\n{body}");
    let mut deep = String::from("treemap\n");
    for i in 0..6 {
        deep += &format!("\"top {i}\"\n");
        for j in 0..8 {
            deep += &format!("  \"mid {i}.{j}\"\n    \"inner {i}.{j}\"\n");
            for k in 0..8 {
                deep += &format!("      \"leaf {i}.{j}.{k}\": {}\n", 1 + ((i * 64 + j * 8 + k) % 37_i32).pow(3));
            }
        }
    }
    let long = "treemap\n\"A section with a very long name indeed\"\n  \"big\": 1000\n  \"A leaf with quite a long name\": 60\n  \"tiny\": 12\n\"other\": 3000";
    let fixed = "treemap\n\"A section with a long name\":::head\n  \"A leaf with a fairly long name\": 30:::big\n  \"Another leaf with a long name\": 20:::big\n  \"rest\": 400\nclassDef big font-size:20px\nclassDef head font-size:18px";
    let styled = "treemap\n\"Styled\":::paint\n  \"a\": 3:::ink\n  \"b\": 2:::dark\n  \"c\": 1:::thin\nclassDef paint fill:#1f3a5f,stroke:#ffcc00,stroke-width:3px\nclassDef ink fill:#ffeb3b\nclassDef dark fill:navy,color:white\nclassDef thin font-weight:bold,font-style:italic,fill:rgba(0\\,0\\,0\\,0.2)";
    let mut out: Vec<(String, Value)> = vec![
        (basic.to_owned(), json!({})),
        (config("    showValues: false", basic), json!({})),
        (config("    showValues: true", basic), json!({})),
        (config("    nodeWidth: 30\n    nodeHeight: 20", basic), json!({})),
        (config("    diagramPadding: 50", basic), json!({})),
        (config("    diagramPadding: 50", basic), json!({ "padding": 0 })),
        (config("    diagramPadding: 50", basic), json!({ "padding": 8 })),
        (config("    padding: 20", basic), json!({})),
        (config("    padding: 2.5", basic), json!({})),
        (config("    valueFormat: '$0,0'", "treemap\n\"Budget\"\n  \"Salaries\": 700000\n  \"Equipment\": 200000"), json!({})),
        (config("    valueFormat: '.1%'", basic), json!({})),
        (config("    valueFormat: ',.2f'", basic), json!({})),
        (config("    valueFormat: '.2e'", basic), json!({})),
        (config("    valueFormat: '<b>'", "treemap\n\"a\": 1234"), json!({})),
        (config("    nodeWidth: -3\n    nodeHeight: \"x\"\n    padding: 100000\n    valueFormat: 7", basic), json!({})),
        (config("    nodeWidth: 400\n    nodeHeight: 400", basic), json!({})),
        (config("    nodeWidth: 401\n    nodeHeight: 0.5", basic), json!({})),
        (config("    nodeHeight: 3", "treemap\n\"A & B\"\n  \"Inner\"\n    \"Leaf\": 1"), json!({})),
        (config("    nodeHeight: 3", "treemap\n\"#1;\"\n  \"Leaf\": 1"), json!({})),
        (config("    nodeHeight: 3", "treemap\n\"Section\"\n  \"Leaf\": 1"), json!({})),
        (config("    nodeHeight: 4", "treemap\n\"Section\"\n  \"Leaf\": 1"), json!({})),
        (config("    nodeHeight: 4", "treemap\n\"Section\":::big\n  \"Leaf\": 1\nclassDef big font-size:14.4px"), json!({})),
        (config("    nodeHeight: 2", "treemap\n\"Label\": 1:::frac\nclassDef frac font-size:12.5px"), json!({})),
        ("treemap\n\"Section\"\n  \"Leaf\": 1".to_owned(), json!({ "fontSize": 24 })),
        ("treemap\n\"Section\":::big\n  \"Leaf\": 1\nclassDef big font-size:20px".to_owned(), json!({})),
        ("treemap\n\"Section\":::big\n  \"Leaf\": 1\nclassDef big font-size:32px".to_owned(), json!({})),
        (deep.clone(), json!({})),
        (deep, json!({ "maxWidth": 300 })),
        (long.to_owned(), json!({})),
        (fixed.to_owned(), json!({})),
        (styled.to_owned(), json!({})),
        ("treemap\n\"big\": 1000\n\"small\": 4".to_owned(), json!({})),
        ("treemap\n\"big\": 1000\n\"&amp;lt;b&amp;gt; #35;\": 1".to_owned(), json!({})),
        ("---\ntitle: From front matter\n---\n".to_owned() + basic, json!({})),
        ("---\ntitle: A title much wider than the whole diagram so that it is wider than six hundred and forty pixels\n---\n".to_owned() + basic, json!({})),
        ("treemap\naccTitle: Name\naccDescr: Text\n\"a\": 1".to_owned(), json!({ "idPrefix": "x\"y" })),
        ("treemap".to_owned(), json!({})),
        ("treemap\n\"Only a section\"".to_owned(), json!({})),
        ("treemap\n\"a\": 0\n\"b\": .".to_owned(), json!({})),
        (format!("treemap\n\"a\": {}", "9".repeat(400)), json!({})),
        ("treemap\n\"a\": 1e308\n\"b\": 99999999999999999999999999999999999999999999999999".to_owned(), json!({})),
    ];
    for options in
        [json!({ "fontSize": 12 }), json!({ "padding": 0 }), json!({ "responsive": false }), json!({ "maxWidth": 200 })]
    {
        out.push((basic.to_owned(), options));
    }
    for colours in [
        json!({ "cScale0": "#000", "cScale1": "#fff" }),
        json!({ "cScale0": "navy", "cScale1": "url(x)", "cScale2": "rgba(255,255,255,0.4)" }),
        json!({ "cScale3": "red" }),
    ] {
        out.push((basic.to_owned(), json!({ "config": { "themeVariables": colours } })));
        out.push((styled.to_owned(), json!({ "config": { "themeVariables": colours } })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run_render("treemap", "case", &cases(), PATHS).finish("treemap cases");
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run_render("treemap", "harvested", &plain(harvest("treemap", "sources")), PATHS)
        .finish("harvested treemap sources");
    assert!(tally.identical > 700, "the harvest holds treemaps Pele draws");
    assert!(tally.rejected > 50, "the harvest holds treemaps Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("treemap", "sources");
    for width in [320.0, 480.0, 640.0, 900.0] {
        let tally = run_at_width("treemap", "harvested", &sources, width, PATHS)
            .finish(&format!("harvested treemap sources at maxWidth {width}"));
        println!("  maxWidth {width} makes Pele draw {} of them another way", tally.reacted);
    }
}

#[test]
fn mutated_sources_match_pele() {
    run_render("treemap", "mutated", &plain(harvest("treemap", "mutated")), PATHS).finish("mutated treemap sources");
}

#[test]
fn worst_cases_match_pele() {
    let cases: Vec<(String, Value)> =
        harvest("treemap", "worst").into_iter().map(|source| (source, json!({ "limit": 100_000_000 }))).collect();
    let tally = run_render("treemap", "worst", &cases, PATHS).finish("Pele's worst-case treemaps");
    assert!(tally.identical > 15, "the worst cases are drawn, not rejected");
}

const NAMES: &[&str] = &[
    "Item",
    "Two words",
    "A name long enough that a box has to cut it short or shrink it",
    "Unbreakableunbreakableunbreakable",
    "#35; &amp; <b>x</b>",
    "\u{e9}\u{91c7}\u{1f600}",
    "  padded  ",
    "",
    "A & B",
    "x",
];
const VALUES: &[&str] =
    &["1", "2", "5", "10", "25", "100", "1000", "1,500.5", "0.5", "0", "1_000", "99999999", "0.001", "3"];
const FORMATS: &[&str] = &["','", "'$0,0'", "'.1%'", "',.2f'", "'.3e'", "'d'", "'$,.2~f'", "'+,'", "''"];
const CLASS_STYLES: &[&str] = &[
    "fill:#1f3a5f,color:white",
    "fill:#ffeb3b",
    "stroke:red,stroke-width:3px",
    "font-size:20px",
    "font-size:9px",
    "font-weight:bold",
    "font-style:italic,color:#333",
    "fill:none",
    "fill:rgba(0\\,0\\,0\\,0.3)",
];

fn random_tree(rng: &mut Rng, src: &mut String, depth: usize, indent: usize, classes: usize) {
    let most = if depth > 0 && rng.chance(8) { 40 } else { 5 };
    let count = 1 + rng.below(most);
    for _ in 0..count {
        let name = rng.pick(NAMES);
        let class = if classes > 0 && rng.chance(20) { format!(":::k{}", rng.below(classes)) } else { String::new() };
        let pad = " ".repeat(indent);
        if depth > 0 && rng.chance(45) {
            *src += &format!("{pad}\"{name}\"{class}\n");
            random_tree(rng, src, depth - 1, indent + 2, classes);
        } else {
            *src += &format!("{pad}\"{name}\": {}{class}\n", rng.pick(VALUES));
        }
    }
}

/// A treemap Pele draws: sections nested a few deep, leaves of many sizes, classes, and the
/// config and host options that change the drawing.
fn random_treemap(rng: &mut Rng) -> (String, Value) {
    let mut src = String::new();
    let mut settings = Vec::new();
    if rng.chance(30) {
        let pick = |rng: &mut Rng, list: &[&str]| list[rng.below(list.len())].to_owned();
        for (key, values) in [
            ("valueFormat", FORMATS),
            ("showValues", &["false", "true"][..]),
            ("nodeWidth", &["20", "30", "64", "120"][..]),
            ("nodeHeight", &["3", "6", "10", "25", "40", "60"][..]),
            ("padding", &["0", "1", "6", "12"][..]),
            ("diagramPadding", &["0", "20"][..]),
        ] {
            if rng.chance(35) {
                settings.push(format!("    {key}: {}", pick(rng, values)));
            }
        }
    }
    let palette = rng.chance(8);
    if !settings.is_empty() || palette {
        src += "---\nconfig:\n";
        if !settings.is_empty() {
            src += &format!("  treemap:\n{}\n", settings.join("\n"));
        }
        if palette {
            src += "  themeVariables:\n    cScale0: \"#111\"\n    cScale1: \"#fafafa\"\n    cScale2: navy\n";
        }
        src += "---\n";
    }
    src += "treemap\n";
    if rng.chance(25) {
        src += rng.pick(&["title Budget\n", "title A title much wider than the whole treemap is going to be at all\n"]);
    }
    if rng.chance(8) {
        src += "accTitle: Acc\naccDescr: What it shows\n";
    }
    let classes = if rng.chance(30) { 1 + rng.below(4) } else { 0 };
    let depth = rng.below(4);
    random_tree(rng, &mut src, depth, 0, classes);
    for k in 0..classes {
        src += &format!("classDef k{k} {}\n", rng.pick(CLASS_STYLES));
    }
    let options = match rng.below(10) {
        0 => {
            let width = [200, 320, 480][rng.below(3)];
            json!({ "maxWidth": width })
        }
        1 => {
            let size = [12, 14, 20][rng.below(3)];
            json!({ "fontSize": size })
        }
        2 => json!({ "padding": 0 }),
        _ => json!({}),
    };
    (src, options)
}

#[test]
fn random_treemaps_match_pele() {
    let mut rng = Rng(0x7ee3_a9f1_d4a3_0007);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_treemap(&mut rng)).collect();
    let tally: RenderTally = run_render("treemap", "random", &cases, PATHS).finish("random treemaps");
    assert!(tally.identical > 3900, "random treemaps are ones Pele draws");
    tally.require(PATHS);
}
