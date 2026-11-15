//! Differential test for drawing tree views: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/treeview/harvest.json` holds the tree view sources of Pele's test suite and the
//! inputs of its mutation runs, written by `scripts/harvest-treeview.mjs`. A tree wider than the
//! host is drawn tightly, with its names and descriptions wrapped, which the widths reach. The
//! random trees must reach every drawing path in [`PATHS`].

#[path = "treeview/random.rs"]
mod random;
#[path = "standalone/parity.rs"]
mod standalone;

use serde_json::{Value, json};
use standalone::{Parity, Reached, WIDTHS, attribute, harvest, plain, redrawn_to_fit, tags};

/// The inside of every entry's group.
fn rows(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-node pele-tree-")
        .skip(1)
        .map(|rest| {
            let end = rest.find("</g>").unwrap_or(rest.len());
            &rest[..end]
        })
        .collect()
}

/// The path of every guide.
fn guides(svg: &str) -> Vec<&str> {
    let Some(at) = svg.find("<g class=\"pele-tree-guides\"") else {
        return Vec::new();
    };
    let layer = &svg[at..at + svg[at..].find("</g>").unwrap_or(0)];
    tags(layer, "<path d=\"").iter().filter_map(|tag| attribute(tag, "d")).collect()
}

fn icons(svg: &str) -> Vec<&str> {
    tags(svg, "<svg class=\"pele-icon\"").iter().filter_map(|tag| attribute(tag, "data-icon")).collect()
}

fn tight(svg: &str) -> bool {
    svg.contains("class=\"pele-label\"")
        && tags(svg, "<text class=\"pele-label\"").iter().any(|tag| tag.contains("text-anchor=\"start\""))
}

/// The drawing paths a source can go through, each with what Pele's SVG shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a file", |c| c.svg.contains("<g class=\"pele-node pele-tree-file")),
    ("a folder", |c| c.svg.contains("<g class=\"pele-node pele-tree-dir")),
    ("an entry three folders deep", |c| guides(c.svg).len() >= 3),
    ("a guide with an elbow", |c| guides(c.svg).iter().any(|d| d.contains('Q'))),
    ("a guide with no elbow", |c| guides(c.svg).iter().any(|d| !d.contains('Q'))),
    ("a guide with ticks for earlier entries", |c| guides(c.svg).iter().any(|d| d.matches('M').count() > 1)),
    ("a guide of another thickness", |c| {
        tags(c.svg, "<g class=\"pele-tree-guides\"").iter().any(|tag| attribute(tag, "stroke-width") != Some("1"))
    }),
    ("a highlighted row", |c| c.svg.contains("class=\"pele-tree-highlight\"")),
    ("an author's class", |c| {
        tags(c.svg, "<g class=\"pele-node pele-tree-")
            .iter()
            .any(|tag| attribute(tag, "class").is_some_and(|class| class.split(' ').count() > 2))
    }),
    ("the built-in folder icon", |c| icons(c.svg).contains(&"mermaid-treeview:folder")),
    ("the built-in file icon", |c| icons(c.svg).contains(&"mermaid-treeview:file")),
    ("a built-in icon drawn for another name", |c| {
        icons(c.svg)
            .iter()
            .any(|icon| icon.starts_with("mermaid-treeview:") && !icon.ends_with(":folder") && !icon.ends_with(":file"))
    }),
    ("an icon of another pack", |c| icons(c.svg).iter().any(|icon| !icon.starts_with("mermaid-treeview:"))),
    ("an icon from the default pack", |c| {
        icons(c.svg).iter().any(|icon| icon.starts_with("p:") || icon.starts_with("mdi:"))
    }),
    ("an icon by file name", |c| icons(c.svg).iter().any(|icon| icon.ends_with(":book"))),
    ("an icon by extension", |c| icons(c.svg).iter().any(|icon| icon.ends_with(":ts") || icon.ends_with(":md"))),
    ("an entry beside one with an icon, with none of its own", |c| {
        !icons(c.svg).is_empty() && rows(c.svg).iter().any(|row| !row.contains("pele-icon"))
    }),
    ("a description", |c| c.svg.contains("class=\"pele-tree-description\"")),
    ("drawn tightly", |c| tight(c.svg)),
    ("a name wrapped", |c| rows(c.svg).iter().any(|row| row.matches("<text class=\"pele-label\"").count() > 1)),
    ("a description wrapped", |c| {
        rows(c.svg).iter().any(|row| row.matches("<text class=\"pele-tree-description\"").count() > 1)
    }),
    ("a name and a description on one row, tightly", |c| {
        tight(c.svg)
            && rows(c.svg).iter().any(|row| row.contains("pele-label") && row.contains("pele-tree-description"))
    }),
    ("a name in the monospace font", |c| {
        tags(c.svg, "<text class=\"pele-label\"").iter().any(|tag| tag.contains("font-family"))
    }),
    ("an entry with no name to show", |c| rows(c.svg).iter().any(|row| !row.contains("pele-label"))),
    ("box drawing", |c| c.source.contains(['├', '└', '┣', '┗'])),
    ("a quoted name", |c| c.source.contains("\n\"") || c.source.contains(" \"") || c.source.contains("\n'")),
    ("a title", |c| c.svg.contains("class=\"pele-title\"")),
    ("an accessible title and description", |c| {
        c.svg.contains("aria-labelledby=") && c.svg.contains("aria-describedby=")
    }),
    ("a setting out of its range", |c| {
        ["101", "200", "500", "0.1", "13", "-1", "'9'"].iter().any(|bad| c.source.contains(&format!(": {bad}\n")))
    }),
    ("nothing to draw", |c| !c.svg.contains("pele-node")),
];

const TREEVIEW: Parity = Parity { kind: "treeView", paths: PATHS };

#[test]
fn harvested_sources_match_pele() {
    let tally = TREEVIEW.run("harvested", &plain(harvest("treeview", "sources"))).finish("harvested tree view sources");
    assert!(tally.identical > 500, "the harvest holds trees Pele draws");
    assert!(tally.rejected > 50, "the harvest holds trees Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("treeview", "sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        TREEVIEW.run("harvested", &cases).finish(&format!("harvested tree view sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    TREEVIEW.run("mutated", &plain(harvest("treeview", "mutated"))).finish("mutated tree view sources");
}

/// Host options the harvest does not set, and Pele's worst cases for tree views
/// (`tests/treeview/stress.test.ts`) with the source limit lifted as that test lifts it.
fn cases() -> Vec<(String, Value)> {
    const N: usize = 50_000;
    let head = "treeView-beta\n";
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let project = "---\nconfig:\n  treeView:\n    showIcons: true\n---\ntreeView-beta\ntitle Project\nsrc/ :::highlight\n  index.ts ## entry point\n  lib/\n    a.ts\n    b.ts icon(p:b)\nREADME.md\n";
    let mut out = vec![
        (project.to_owned(), json!({})),
        (project.to_owned(), json!({ "monospace": ["files"] })),
        (project.to_owned(), json!({ "monospace": ["files", "members"], "maxWidth": 160 })),
        (project.to_owned(), json!({ "monospace": ["members"] })),
        (project.to_owned(), json!({ "fontSize": 12, "padding": 0 })),
        (project.to_owned(), json!({ "fontSize": 21.5, "maxWidth": 200 })),
        (project.to_owned(), json!({ "maxWidth": 60 })),
        (project.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (project.to_owned(), json!({ "config": { "treeView": { "useMaxWidth": false, "rowIndent": 40 } } })),
        (project.to_owned(), json!({ "config": { "treeView": { "showIcons": false, "lineThickness": 2.5 } } })),
        (project.to_owned(), json!({ "outputLimit": 100 })),
        (project.to_owned(), json!({ "limit": 10 })),
    ];
    for source in [
        "a\n".repeat(N / 2),
        repeat(300, &|i| " ".repeat(i) + "d/\n"),
        repeat(310, &|i| "\t".repeat(i % 2) + &" ".repeat(i) + "x\n"),
        "a".repeat(N),
        format!("a{}b", " ".repeat(N)),
        format!("{}b", format!("a{}", " ".repeat(99)).repeat(N / 100)),
        format!("{}1", format!("a :::{}", " ".repeat(95)).repeat(N / 100)),
        format!("a{}", " ::: 1 icon[ #".repeat(N / 14)),
        "accDescr {\n".repeat(N / 11),
        format!("a{}", " :::b icon(c) ## d\n x"[..13].repeat(N / 13)),
        format!("a{}", " :::highlight".repeat(N / 13)),
        repeat(2500, &|i| format!(" f{i} :::highlight ## d{i}\n")),
        "%% c\n".repeat(N / 5),
        format!("a ## {}", "word ".repeat(N / 5)),
        format!(" {}", "#35;&amp;".repeat(N / 9)),
        "├── a\n".repeat(N / 6),
        repeat(300, &|i| "│".repeat(i) + "├─ x\n"),
        "├ a\n".to_owned() + &format!("{}└ b\n", " ".repeat(490)).repeat(100),
        "│   │   │\n".repeat(N / 10),
        format!("{}├── a\n", "\t".repeat(40)).repeat(N / 46),
        "a ─ b\n".repeat(N / 6),
    ] {
        for options in [json!({ "limit": 1_000_000_000u64 }), json!({ "limit": 1_000_000_000u64, "maxWidth": 320 })] {
            out.push((format!("{head}{source}"), options));
        }
    }
    let icons = format!(
        "---\nconfig:\n  treeView:\n    showIcons: true\n---\n{head}{}",
        repeat(5000, &|i| format!(" f{i}.ts icon(p:i{i})\n"))
    );
    out.push((icons, json!({ "limit": 1_000_000_000u64 })));
    out
}

#[test]
fn cases_match_pele() {
    TREEVIEW.run("case", &cases()).finish("tree view cases");
}

#[test]
fn random_trees_match_pele() {
    let mut rng = random::Rng(0x0b0c_5ee0_7e11);
    let mut cases = Vec::new();
    for _ in 0..4000 {
        let source = random::source(&mut rng);
        let options = match rng.below(8) {
            0 | 1 => json!({ "maxWidth": *rng.pick(&[120.0, 200.0, 320.0, 480.0]) }),
            2 => json!({ "maxWidth": 240, "monospace": ["files"] }),
            3 => json!({ "monospace": ["files"], "fontSize": *rng.pick(&[12.0, 14.5, 20.0]) }),
            4 => json!({ "padding": *rng.pick(&[0.0, 3.0, 16.0]) }),
            _ => json!({}),
        };
        cases.push((source, options));
    }
    let tally = TREEVIEW.run("random", &cases).finish("random tree views");
    let widths: Vec<(String, Value)> =
        cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect();
    println!("their widths make Pele draw {} of the random tree views another way", redrawn_to_fit(&widths));
    let missed = TREEVIEW.missed(&tally);
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random tree view reaches {missed:?}");
}
