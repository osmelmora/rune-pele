//! What the Scenes of the types opened with the corpus side by side ask of the lowering beyond
//! what that corpus shows: each feature counted over Pele's documentation corpus and over every
//! source harvested from Pele's tests, and every source the lowering refuses, with its reason.
//!
//! A feature the harvest has and the corpus lacks was never compared with Pele's drawing. Each is
//! either refused, so the view falls back to rune-mermaid, or listed in `DIVERGENCES.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use pele_core::scene::{Geometry, Paint, PaintVisit};
use pele_core::{DiagramKind, ParseOptions, Script};
use rune_pele::lower::Drawing;
use rune_pele::text::Baselines;

const TYPES: [&str; 16] = [
    "sequence",
    "gantt",
    "block",
    "c4",
    "railroad",
    "info",
    "cynefin",
    "wardley",
    "venn",
    "packet",
    "eventmodeling",
    "treemap",
    "mindmap",
    "journey",
    "kanban",
    "ishikawa",
];

fn harvested(kind: &str) -> Vec<String> {
    let path = pele_oracle::root().join(format!("tests/fixtures/{kind}/harvest.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: serde_json::Value = serde_json::from_str(&text).expect("the harvest is JSON");
    let lists = harvest.as_object().expect("an object").values().filter_map(|list| list.as_array());
    lists.flatten().filter_map(|source| source.as_str().map(str::to_owned)).collect()
}

/// The features of `scene` the lowering draws only approximately or not at all, or that the
/// corpus may not show, each named once.
fn features(scene: &pele_core::Scene) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => {
                if let Some(turn) = group.rotate {
                    found.insert(format!("group turned {} degrees", turn.degrees));
                }
                if group.opacity != 1.0 {
                    found.insert("group faded".to_owned());
                }
                if let Some(link) = &group.link {
                    found.insert(format!("link{}", if link.internal { " to a host note" } else { "" }));
                }
                if group.tooltip.is_some() {
                    found.insert("tooltip".to_owned());
                }
                if group.untouchable {
                    found.insert("group the pointer passes through".to_owned());
                }
            }
            PaintVisit::End => {}
            PaintVisit::Draw(draw) => {
                match &draw.geometry {
                    Geometry::Icon { glyph: Some(glyph), .. } => {
                        found.insert(format!("icon with Pele's own glyph {glyph:?}"));
                    }
                    Geometry::Icon { name, .. } => {
                        let set = name.split(':').next().unwrap_or_default();
                        found.insert(format!("icon box for the host ({set}:…)"));
                    }
                    Geometry::Image { .. } => {
                        found.insert("picture".to_owned());
                    }
                    Geometry::Text(run) => {
                        if run.script != Script::Normal {
                            found.insert(format!("text {:?}", run.script));
                        }
                        if run.font.family == pele_core::FontFamily::Mono {
                            found.insert("text monospace".to_owned());
                        }
                        if run.font.italic {
                            found.insert("text italic".to_owned());
                        }
                        if run.underline {
                            found.insert("text underlined".to_owned());
                        }
                    }
                    _ => {}
                }
                let style = &draw.style;
                if style.opacity != 1.0 {
                    found.insert("drawing faded".to_owned());
                }
                if style.fill_opacity != 1.0 && style.fill != Paint::None {
                    found.insert("fill translucent".to_owned());
                }
                if let Some(stroke) = &style.stroke {
                    if !stroke.dash.is_empty() {
                        found.insert("stroke dashed".to_owned());
                    }
                    if stroke.opacity != 1.0 {
                        found.insert("stroke translucent".to_owned());
                    }
                }
                for paint in [Some(style.fill), style.stroke.as_ref().map(|stroke| stroke.paint)].into_iter().flatten()
                {
                    match paint {
                        Paint::Source(_) => found.insert("colour the source writes".to_owned()),
                        Paint::Mix { .. } => found.insert("colour mixed of two roles".to_owned()),
                        _ => false,
                    };
                }
            }
        }
    }
    found
}

#[derive(Default)]
struct Tally {
    sources: usize,
    /// Sources Pele rejects, which no view draws.
    rejected: usize,
    /// Sources by why the lowering refuses them.
    refused: BTreeMap<String, usize>,
    /// Sources by feature.
    features: BTreeMap<String, usize>,
}

fn tally(kind: DiagramKind, sources: &[String], baselines: &mut Baselines) -> (Tally, BTreeMap<String, String>) {
    let mut tally = Tally::default();
    let mut example = BTreeMap::new();
    for source in sources {
        tally.sources += 1;
        let Ok(parity) = pele_core::parity_scene(source) else {
            tally.rejected += 1;
            continue;
        };
        let scene = parity.scene();
        // A type's harvest holds a few sources of other types, which their own tallies count.
        if pele_core::prepare(source, ParseOptions::default()).map(|prepared| prepared.kind()) != Ok(kind) {
            tally.sources -= 1;
            continue;
        }
        if let Err(why) = Drawing::new(kind, scene, baselines, 1.0) {
            let why = match why {
                rune_pele::lower::Unsupported::Style(style) => {
                    format!("Style({})", style.split(':').next().unwrap_or_default())
                }
                other => format!("{other:?}"),
            };
            example.entry(format!("refused {why}")).or_insert_with(|| source.clone());
            *tally.refused.entry(why).or_default() += 1;
            continue;
        }
        for feature in features(scene) {
            example.entry(feature.clone()).or_insert_with(|| source.clone());
            *tally.features.entry(feature).or_default() += 1;
        }
    }
    (tally, example)
}

#[test]
#[ignore = "a measurement, not a check: run it in release with --ignored --nocapture"]
fn what_the_opened_types_ask_of_the_lowering_beyond_their_corpus() {
    let mut baselines = Baselines::new(&[], "Inter", "Inter", 1.0);
    let mut report = String::from(
        "Per type: corpus sources, then harvested ones. `refused` is why the lowering leaves a source to rune-mermaid;\n\
         a feature counts the sources that draw it. `harvest only` marks what the corpus never shows.\n",
    );
    for name in TYPES {
        let kind = *DiagramKind::ALL.iter().find(|kind| kind.name() == name).expect("a kind of that name");
        let corpus: Vec<String> = pele_oracle::corpus()
            .into_iter()
            .filter(|(corpus_name, _)| corpus_name == name)
            .flat_map(|(_, sources)| sources)
            .collect();
        let (shown, _) = tally(kind, &corpus, &mut baselines);
        let (wider, example) = tally(kind, &harvested(name), &mut baselines);
        writeln!(
            report,
            "\n{name}: corpus {} sources, harvest {} ({} rejected by Pele)",
            shown.sources, wider.sources, wider.rejected
        )
        .unwrap();
        for (why, count) in &wider.refused {
            let in_corpus = shown.refused.get(why).copied().unwrap_or(0);
            writeln!(report, "  refused {why:<40} corpus {in_corpus:>4}  harvest {count:>6}").unwrap();
        }
        for (feature, count) in &wider.features {
            let in_corpus = shown.features.get(feature).copied().unwrap_or(0);
            let mark = if in_corpus == 0 { "  harvest only" } else { "" };
            writeln!(report, "  {feature:<48} corpus {in_corpus:>4}  harvest {count:>6}{mark}").unwrap();
        }
        for (feature, source) in example
            .iter()
            .filter(|(feature, _)| feature.starts_with("refused") || !shown.features.contains_key(*feature))
        {
            let one_line: String = source.chars().take(160).collect::<String>().replace('\n', "⏎");
            writeln!(report, "    e.g. {feature}: {one_line}").unwrap();
        }
    }
    print!("{report}");
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/verify/rune-graph");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("scene-features.txt"), &report).unwrap();
}
