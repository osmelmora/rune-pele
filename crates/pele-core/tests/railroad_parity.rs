//! Differential test for drawing railroad diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/railroad/harvest.json` holds the railroad sources of Pele's own test suite and
//! the inputs of its generated and mutated runs, written by `scripts/harvest-railroad.mjs`. The
//! widths put every diagram wider than the host through Pele's tight drawing, which folds long
//! sequences into rows. The random diagrams must reach every drawing path in [`PATHS`].

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/railroad/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// This port's render with Pele's render options, which are what `render_with` gives the oracle.
fn render_with(source: &str, options: &Value) -> Result<Rendered, RenderError> {
    let mut parse = ParseOptions::default();
    let mut layout = LayoutOptions::default();
    let number = |value: &Value| value.as_f64().expect("a number");
    let count = |value: &Value| usize::try_from(value.as_u64().expect("a count")).expect("a count");
    let flag = |value: &Value| value.as_bool().expect("a boolean");
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = Some(number(value)),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(text(value)),
            "responsive" => layout.responsive = Some(flag(value)),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

/// How a subpath of a rule's track starts: its point, and the turn it takes first, as the end of
/// a relative arc, when it starts with one.
struct Start {
    x: f64,
    y: f64,
    turn: Option<(bool, f64, f64)>,
    /// The letters of the subpath's commands, the move left out.
    letters: String,
}

/// The subpaths of every track in the drawing.
fn track_starts(svg: &str) -> Vec<Start> {
    let mut out = Vec::new();
    for rest in svg.split("class=\"pele-edge pele-track\" d=\"").skip(1) {
        let d = &rest[..rest.find('"').unwrap_or(0)];
        for piece in d.split('M').skip(1) {
            let end = piece.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(piece.len());
            let Some((x, y)) = piece[..end].split_once(',') else { continue };
            let (Ok(x), Ok(y)) = (x.parse(), y.parse()) else { continue };
            let after = &piece[end..];
            let turn = after.strip_prefix('a').and_then(|arc| {
                let fields: Vec<&str> = arc.split(' ').collect();
                let (dx, dy) = fields.get(4)?.split_once(',')?;
                let dy = &dy[..dy.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(dy.len())];
                Some((fields.get(3)? == &"1", dx.parse().ok()?, dy.parse().ok()?))
            });
            let letters = after.chars().filter(|c| c.is_ascii_alphabetic()).collect();
            out.push(Start { x, y, turn, letters });
        }
    }
    out
}

/// Whether some subpath starts with a turn that sweeps as given towards the signs given.
fn starts_with_turn(svg: &str, sweep: bool, east: bool, south: bool) -> bool {
    track_starts(svg).iter().any(|start| {
        start.turn.is_some_and(|(turned, dx, dy)| turned == sweep && (dx > 0.0) == east && (dy > 0.0) == south)
    })
}

/// The `d` of each rule's arrowheads on its rails.
fn arrowheads(svg: &str) -> Vec<&str> {
    svg.split("<path class=\"pele-marker\" d=\"").skip(1).map(|rest| &rest[..rest.find('"').unwrap_or(0)]).collect()
}

/// The arrowheads at the start of each row a folded sequence goes on to, by the x they start at.
fn fold_arrows(svg: &str) -> Vec<String> {
    arrowheads(svg)
        .iter()
        .flat_map(|d| d.split('M').skip(1))
        .filter(|head| head.contains("l7,3.5l-7,3.5z"))
        .map(|head| head.split(',').next().unwrap_or("").to_owned())
        .collect()
}

/// The rule model of a source, as this port's front end gives it (held to Pele by
/// `railroad_front.rs`).
fn model(source: &str) -> Value {
    pele_core::railroad_probe::front_json(source, ParseOptions::default())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or(Value::Null)
}

/// Every node of every rule's definition, each with the type of the node it is part of.
fn nodes(model: &Value) -> Vec<(&Value, Option<&str>)> {
    let mut out = Vec::new();
    let mut work: Vec<(&Value, Option<&str>)> = model["rules"]
        .as_array()
        .map(|rules| rules.iter().map(|rule| (&rule["definition"], None)).collect())
        .unwrap_or_default();
    while let Some((node, parent)) = work.pop() {
        let kind = node["type"].as_str();
        for key in ["elements", "alternatives"] {
            for part in node[key].as_array().into_iter().flatten() {
                work.push((part, kind));
            }
        }
        if !node["element"].is_null() {
            work.push((&node["element"], kind));
        }
        out.push((node, parent));
    }
    out
}

fn has_node(model: &Value, test: impl Fn(&Value, Option<&str>) -> bool) -> bool {
    nodes(model).into_iter().any(|(node, parent)| test(node, parent))
}

fn of_type(node: &Value, kind: &str) -> bool {
    node["type"] == kind
}

struct Case<'a> {
    source: &'a str,
    svg: &'a str,
    model: Value,
}

type Reached = fn(&Case) -> bool;

/// The drawing paths a source can go through, each with what Pele's SVG or the rule model
/// shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a terminal", |c| c.svg.contains("<g class=\"pele-node pele-terminal\">")),
    ("a non-terminal", |c| c.svg.contains("<g class=\"pele-node pele-nonterminal\"")),
    ("a special sequence", |c| c.svg.contains("<g class=\"pele-node pele-special\">")),
    ("a sequence", |c| has_node(&c.model, |node, _| of_type(node, "sequence"))),
    ("a choice", |c| has_node(&c.model, |node, _| of_type(node, "choice"))),
    ("a choice of three or more, with a middle branch", |c| starts_with_turn(c.svg, false, true, true)),
    ("a branch narrower than its rails", |c| {
        track_starts(c.svg).iter().any(|start| match start.turn {
            Some((false, dx, dy)) if dx > 0.0 && dy > 0.0 => start.letters == "aH",
            Some((true, dx, dy)) if dx > 0.0 && dy > 0.0 => start.letters == "aVaH",
            _ => false,
        })
    }),
    ("an optional", |c| starts_with_turn(c.svg, false, true, false)),
    ("a loop", |c| starts_with_turn(c.svg, false, false, true)),
    ("a loop with no count", |c| {
        starts_with_turn(c.svg, false, false, true)
            && arrowheads(c.svg).iter().any(|d| d.contains("l7,-3.5v7z"))
            && !c.svg.contains("pele-count")
    }),
    ("a bypass round a loop", |c| {
        let starts = track_starts(c.svg);
        starts.iter().any(|bypass| {
            bypass.turn.is_some_and(|(sweep, dx, dy)| !sweep && dx > 0.0 && dy < 0.0)
                && starts.iter().any(|looped| {
                    looped.turn.is_some_and(|(sweep, dx, dy)| !sweep && dx < 0.0 && dy > 0.0)
                        && (looped.y - bypass.y).abs() < 0.02
                        && bypass.turn.is_some_and(|(_, arc, _)| (looped.x - bypass.x - 3.0 * arc).abs() < 0.03)
                })
        })
    }),
    ("a count of exactly n", |c| counts(c.svg).iter().any(|count| !count.contains(['≥', '≤', '–', '∞']))),
    ("a count of at least n", |c| counts(c.svg).iter().any(|count| count.starts_with('≥'))),
    ("a count of at most n", |c| counts(c.svg).iter().any(|count| count.starts_with('≤'))),
    ("a count from n to m", |c| counts(c.svg).iter().any(|count| count.contains('–'))),
    ("a count of infinitely many", |c| counts(c.svg).contains(&"∞×")),
    ("a sequence inside a choice", |c| {
        has_node(&c.model, |node, parent| of_type(node, "sequence") && parent == Some("choice"))
    }),
    ("a choice inside a sequence", |c| {
        has_node(&c.model, |node, parent| of_type(node, "choice") && parent == Some("sequence"))
    }),
    ("a choice as a branch of a choice", |c| {
        has_node(&c.model, |node, parent| of_type(node, "choice") && parent == Some("choice"))
    }),
    ("a repetition of a choice", |c| {
        has_node(&c.model, |node, parent| of_type(node, "choice") && parent == Some("repetition"))
    }),
    ("an optional sequence", |c| {
        has_node(&c.model, |node, parent| of_type(node, "sequence") && parent == Some("optional"))
    }),
    ("a folded sequence", |c| !fold_arrows(c.svg).is_empty()),
    ("a sequence folded into three rows or more", |c| {
        let arrows = fold_arrows(c.svg);
        arrows.iter().enumerate().any(|(i, x)| arrows[i + 1..].contains(x))
    }),
    ("a tight drawing", |c| !c.source.contains("arcRadius") && c.svg.contains("a6,6 0 0 ")),
    ("an EBNF exception", |c| {
        has_node(&c.model, |node, _| {
            of_type(node, "sequence")
                && node["elements"]
                    .as_array()
                    .is_some_and(|parts| parts.len() == 3 && parts[1] == json!({"type": "terminal", "value": "-"}))
        }) && c.model["notation"] == "ebnf"
    }),
    ("a PEG predicate", |c| {
        c.model["notation"] == "peg"
            && has_node(&c.model, |node, _| node["text"].as_str().is_some_and(|text| text.starts_with(['&', '!'])))
    }),
    ("a PEG any", |c| c.model["notation"] == "peg" && c.svg.contains(">.<")),
    ("an ABNF numeric value", |c| c.model["notation"] == "abnf" && c.svg.contains(">%")),
    ("an IR constructor diagram", |c| c.model["notation"] == "railroad"),
    ("a terminal of spaces", |c| c.svg.contains('␣')),
    ("text whose spaces are kept", |c| c.svg.contains(" xml:space=\"preserve\"")),
    ("an escape written out", |c| c.svg.contains("\\n") || c.svg.contains("\\t")),
    ("an entity code shown as its character", |c| c.source.contains("#35;") && c.svg.contains(">#<")),
    ("a rule defined twice", |c| {
        let names: Vec<&str> = c
            .svg
            .split("<g class=\"pele-rule\" data-id=\"")
            .skip(1)
            .map(|rest| &rest[..rest.find('"').unwrap_or(0)])
            .collect();
        names.iter().enumerate().any(|(i, name)| names[i + 1..].contains(name))
    }),
    ("a title", |c| c.svg.contains("class=\"pele-title\"")),
    ("a title wrapped to fit", |c| {
        c.svg.contains("class=\"pele-title\"")
            && c.svg
                .split("class=\"pele-title\"")
                .nth(1)
                .is_some_and(|t| t[..t.find("</text>").unwrap_or(0)].matches("<tspan x=").count() > 1)
    }),
    ("an accessible title and description", |c| {
        c.svg.contains("aria-labelledby=") && c.svg.contains("aria-describedby=")
    }),
    ("no markers", |c| c.svg.contains("pele-rule\"") && !c.svg.contains("<circle")),
    ("a stroke width", |c| c.svg.contains("stroke=\"var(--_l)\" stroke-width=")),
    ("arcs of no radius", |c| c.svg.contains("a0,0 0 0 ")),
    ("a font size from the config", |c| c.source.contains("fontSize") && c.svg.contains("<g class=\"pele-rules\"")),
    ("no rules", |c| !c.svg.contains("pele-rules")),
];

/// The count written on each loop.
fn counts(svg: &str) -> Vec<&str> {
    svg.split("class=\"pele-edge-label pele-count\"")
        .skip(1)
        .filter_map(|rest| {
            let text = &rest[rest.find('>')? + 1..];
            Some(&text[..text.find('<')?])
        })
        .collect()
}

/// Where two texts first differ, with what each holds there.
fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(80).min(bytes.len())..(at + 80).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
}

#[derive(Default)]
struct Tally {
    identical: usize,
    rejected: usize,
    other_types: usize,
    failures: Vec<String>,
    /// The drawing paths that the identical outputs went through, by name.
    reached: BTreeSet<&'static str>,
}

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<Rendered, RenderError>, theirs: &PeleRender) {
        let problem = match (theirs, ours) {
            (PeleRender::Svg { r#type, svg, width, height }, Ok(ours)) => {
                if ours.svg != *svg {
                    Some(first_difference(&ours.svg, svg))
                } else if ours.width != *width || ours.height != *height {
                    Some(format!("size {}x{}, pele {width}x{height}", ours.width, ours.height))
                } else if r#type == "railroad" {
                    self.identical += 1;
                    let case = Case { source, svg, model: model(source) };
                    self.reached.extend(PATHS.iter().filter(|(_, reached)| reached(&case)).map(|(name, _)| *name));
                    None
                } else {
                    self.other_types += 1;
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "railroad" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "railroad" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { error, .. }, Err(RenderError::Diagnostic(ours))) => {
                let ours = format!("PeleError: {}", ours.message);
                if ours == *error {
                    self.rejected += 1;
                    None
                } else {
                    Some(format!("rejected with another message\n    ours: {ours:?}\n    pele: {error:?}"))
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(error)) => Some(format!("pele renders a {type}, ours fails: {error}")),
            (PeleRender::Error { error, .. }, Ok(_)) => Some(format!("pele rejects it, ours renders: {error}")),
            (PeleRender::Error { error, .. }, Err(_)) => {
                Some(format!("pele rejects a railroad diagram, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {:?}", &source[..source.len().min(600)]));
        }
    }

    fn finish(self, what: &str) -> Self {
        let total = self.identical + self.rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {} rejected with Pele's message, {} of another type, {} different",
            self.identical,
            self.rejected,
            self.other_types,
            self.failures.len()
        );
        assert!(
            self.failures.is_empty(),
            "{} of {total} {what} differ from Pele:\n{}",
            self.failures.len(),
            self.failures[..self.failures.len().min(8)].join("\n")
        );
        self
    }
}

/// Renders each case through both engines. A panic in this port is a failure of that case.
fn run(name: &str, cases: &[(String, Value)]) -> Tally {
    let requests: Vec<(String, String)> =
        cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
    let mut tally = Tally::default();
    for (index, ((source, options), theirs)) in cases.iter().zip(pele_oracle::render_all_with(&requests)).enumerate() {
        let name = format!("{name} {index} with {options}");
        match std::panic::catch_unwind(|| render_with(source, options)) {
            Ok(ours) => tally.compare(&name, source, ours, &theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

/// The widths a host may have.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

/// How many of the cases Pele draws another way for the width they give than with room to
/// spare, which for railroad is the tight drawing.
fn redrawn_to_fit(cases: &[(String, Value)]) -> usize {
    let svg = |render: &PeleRender| match render {
        PeleRender::Svg { svg, .. } => Some(svg.clone()),
        PeleRender::Error { .. } => None,
    };
    let with = |drop_width: bool| -> Vec<(String, String)> {
        cases
            .iter()
            .map(|(source, options)| {
                let mut options = options.clone();
                if drop_width {
                    options.as_object_mut().expect("options are an object").remove("maxWidth");
                }
                (source.clone(), options.to_string())
            })
            .collect()
    };
    let (bounded, unbounded) = (pele_oracle::render_all_with(&with(false)), pele_oracle::render_all_with(&with(true)));
    bounded.iter().zip(&unbounded).filter(|(bounded, unbounded)| svg(bounded) != svg(unbounded)).count()
}

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested railroad sources");
    assert!(tally.identical > 1500, "the harvest holds railroad diagrams Pele renders");
    assert!(tally.rejected > 500, "the harvest holds railroad diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested railroad sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated railroad sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let grammar =
        "railroad-ebnf-beta\ntitle T\nr = a ( b | [ c d ] | { e | f } )+ g? ;\ns = ( ( a+ )+ | b* )* - \"x\" ;";
    let long = "railroad-ebnf-beta\nexpression = term ( ( \"+\" | \"-\" | \"or else\" ) term )* identifier \"a fairly long terminal\" another_name ? special text ? yet_another more and more names to fold ;";
    let titled = "---\ntitle: A title much wider than the diagram under it, as wide as can be, wider still\n---\nrailroad-peg-beta\nR <- a / b ;";
    let mut out = vec![
        (grammar.to_owned(), json!({})),
        (grammar.to_owned(), json!({ "fontSize": 24 })),
        (grammar.to_owned(), json!({ "fontSize": 9 })),
        (grammar.to_owned(), json!({ "padding": 0 })),
        (grammar.to_owned(), json!({ "padding": 30, "maxWidth": 320 })),
        (grammar.to_owned(), json!({ "responsive": false, "idPrefix": "x\"y" })),
        (long.to_owned(), json!({ "maxWidth": 200 })),
        (long.to_owned(), json!({ "maxWidth": 1 })),
        (long.to_owned(), json!({ "maxWidth": 0 })),
        (long.to_owned(), json!({ "maxWidth": -5 })),
        (long.to_owned(), json!({ "maxWidth": 320, "padding": 100 })),
        (long.to_owned(), json!({ "maxWidth": 320, "fontSize": 30 })),
        (titled.to_owned(), json!({ "maxWidth": 200 })),
        (titled.to_owned(), json!({})),
        ("railroad-beta".to_owned(), json!({ "outputLimit": 100 })),
        ("railroad-beta\nr = terminal(\"a\") ;".to_owned(), json!({ "limit": 10 })),
    ];
    for railroad in [
        json!({ "arcRadius": 0 }),
        json!({ "arcRadius": 40, "verticalSeparation": 0 }),
        json!({ "padding": 0, "horizontalSeparation": 0, "verticalSeparation": 0 }),
        json!({ "arcRadius": 7.5, "padding": 3.3, "verticalSeparation": 2.5, "horizontalSeparation": 1.5, "fontSize": 9.5 }),
        json!({ "arcRadius": 1e9, "padding": 1e9, "fontSize": 1e9, "markerRadius": 1e9, "strokeWidth": 1e9 }),
        json!({ "arcRadius": -1, "padding": null, "fontSize": -1, "showMarkers": "no", "markerRadius": null }),
        json!({ "showMarkers": false, "markerRadius": 9 }),
        json!({ "markerRadius": 0 }),
        json!({ "strokeWidth": 0.005 }),
        json!({ "strokeWidth": 2, "fontSize": 0 }),
        json!({ "arcRadius": "wide", "padding": -4, "fontSize": "x" }),
        json!({ "useMaxWidth": false }),
        json!({ "arcRadius": -0.0 }),
    ] {
        out.push((grammar.to_owned(), json!({ "config": { "railroad": railroad } })));
        out.push((long.to_owned(), json!({ "config": { "railroad": railroad }, "maxWidth": 320 })));
    }
    out.push((grammar.to_owned(), json!({ "config": { "railroad": "x" } })));
    out.push((grammar.to_owned(), json!({ "config": { "railroad": [1, 2] } })));
    for front in ["arcRadius: .inf", "padding: .nan", "fontSize: 1e400", "arcRadius: 0x10", "verticalSeparation: 1_000"]
    {
        out.push((format!("---\nconfig:\n  railroad:\n    {front}\n---\n{}", grammar), json!({})));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("railroad cases");
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

/// A grammar node, before it is written in a notation.
enum Tree {
    Terminal(&'static str),
    NonTerminal(&'static str),
    Special(&'static str),
    Sequence(Vec<Tree>),
    Choice(Vec<Tree>),
    Optional(Box<Tree>),
    /// `None` for one or more, `Some((min, max))` for a count ABNF can write.
    Repetition(Box<Tree>, bool, Option<(String, String)>),
    /// EBNF's `a - b`, and PEG's `&a` and `!a`, which other notations write as a sequence.
    Except(Box<Tree>, Box<Tree>),
    Predicate(&'static str, Box<Tree>),
}

const TERMINALS: &[&str] = &[
    "a",
    "+",
    "if",
    "while",
    "  ",
    " ",
    "a  b",
    " x",
    "#35;",
    "&amp;",
    "<br>",
    "\u{e9}",
    "\u{65e5}\u{672c}",
    "identifier",
    "a fairly long terminal",
    "0",
    "9",
    "(",
    "\\n",
    "\\t",
    "*a*",
];
const NAMES: &[&str] =
    &["expr", "term", "factor", "digit", "letter", "Digit2", "rule-name", "x", "a", "statement-list", "number"];
const SPECIALS: &[&str] = &["any character", "x", "letters and digits", "#35; code"];

fn tree(rng: &mut Rng, depth: usize) -> Tree {
    if depth == 0 || rng.below(10) < 4 {
        return match rng.below(6) {
            0..=2 => Tree::Terminal(rng.pick(TERMINALS)),
            3 | 4 => Tree::NonTerminal(rng.pick(NAMES)),
            _ => Tree::Special(rng.pick(SPECIALS)),
        };
    }
    let several = |rng: &mut Rng, most: usize| (0..2 + rng.below(most)).map(|_| tree(rng, depth - 1)).collect();
    match rng.below(9) {
        0 | 1 => {
            let most = if rng.chance(3) { 10 } else { 3 };
            Tree::Sequence(several(rng, most))
        }
        2 | 3 => Tree::Choice(several(rng, 3)),
        4 => Tree::Optional(Box::new(tree(rng, depth - 1))),
        5 | 6 => {
            let count = rng.chance(2).then(|| {
                let nines = "9".repeat(400);
                let counts = [
                    ("2", "4"),
                    ("3", "3"),
                    ("", "5"),
                    ("2", ""),
                    ("0", "0"),
                    ("1", "1"),
                    ("0", "1"),
                    ("12", ""),
                    ("", ""),
                ];
                match counts[rng.below(counts.len())] {
                    ("", "") => (nines.clone(), nines),
                    (min, max) => (min.to_owned(), max.to_owned()),
                }
            });
            Tree::Repetition(Box::new(tree(rng, depth - 1)), rng.chance(2), count)
        }
        7 => Tree::Except(Box::new(tree(rng, depth - 1)), Box::new(tree(rng, depth - 1))),
        _ => Tree::Predicate(rng.pick(&["&", "!"]), Box::new(tree(rng, depth - 1))),
    }
}

/// A terminal's text as a quoted string the notation reads back as that text.
fn quoted(text: &str, notation: usize) -> String {
    if notation == 2 { format!("\"{}\"", text.replace('"', "'")) } else { format!("\"{}\"", text.replace('"', "\\\"")) }
}

/// The tree written in one notation: 0 constructor calls, 1 EBNF, 2 ABNF, 3 PEG.
fn write(tree: &Tree, notation: usize, out: &mut String) {
    let join = |parts: &[Tree], joint: &str, out: &mut String| {
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                out.push_str(joint);
            }
            write(part, notation, out);
        }
    };
    match (tree, notation) {
        (Tree::Terminal(text), 0) => out.push_str(&format!("terminal({})", quoted(text, 0))),
        (Tree::Terminal(text), _) => out.push_str(&quoted(text, notation)),
        (Tree::NonTerminal(name), 0) => out.push_str(&format!("nonterminal(\"{name}\")")),
        (Tree::NonTerminal(name), _) => out.push_str(name),
        (Tree::Special(text), 0) => out.push_str(&format!("special(\"{text}\")")),
        (Tree::Special(text), 1) => out.push_str(&format!("? {text} ?")),
        (Tree::Special(_), 2) => out.push_str("%x30-39"),
        (Tree::Special(_), _) => out.push('.'),
        (Tree::Sequence(parts), 0) => {
            out.push_str("sequence(");
            join(parts, ", ", out);
            out.push(')');
        }
        (Tree::Sequence(parts), _) => {
            out.push_str("( ");
            join(parts, " ", out);
            out.push_str(" )");
        }
        (Tree::Choice(parts), 0) => {
            out.push_str("choice(");
            join(parts, ", ", out);
            out.push(')');
        }
        (Tree::Choice(parts), _) => {
            out.push_str("( ");
            join(parts, if notation == 1 { " | " } else { " / " }, out);
            out.push_str(" )");
        }
        (Tree::Optional(part), 0) => {
            out.push_str("optional(");
            write(part, notation, out);
            out.push(')');
        }
        (Tree::Optional(part), 3) => {
            out.push_str("( ");
            write(part, notation, out);
            out.push_str(" )?");
        }
        (Tree::Optional(part), _) => {
            out.push_str("[ ");
            write(part, notation, out);
            out.push_str(" ]");
        }
        (Tree::Repetition(part, zero, count), 2) => {
            let repeat = match count {
                Some((min, max)) if min == max => min.clone(),
                Some((min, max)) => format!("{min}*{max}"),
                None if *zero => "*".to_owned(),
                None => "1*".to_owned(),
            };
            out.push_str(&format!("{repeat}( "));
            write(part, notation, out);
            out.push_str(" )");
        }
        (Tree::Repetition(part, zero, _), 0) => {
            out.push_str(if *zero { "zeroOrMore(" } else { "oneOrMore(" });
            write(part, notation, out);
            out.push(')');
        }
        (Tree::Repetition(part, zero, _), _) => {
            out.push_str("( ");
            write(part, notation, out);
            out.push_str(if *zero { " )*" } else { " )+" });
        }
        (Tree::Except(left, right), 1) => {
            out.push_str("( ");
            write(left, notation, out);
            out.push_str(" - ");
            write(right, notation, out);
            out.push_str(" )");
        }
        (Tree::Except(left, right), _) => write(&Tree::Sequence(vec![take(left), take(right)]), notation, out),
        (Tree::Predicate(operator, part), 3) => {
            out.push_str(operator);
            out.push_str("( ");
            write(part, notation, out);
            out.push_str(" )");
        }
        (Tree::Predicate(_, part), _) => write(part, notation, out),
    }
}

/// A copy of a leaf, or a stand-in for anything bigger, for a notation that has no exception.
fn take(tree: &Tree) -> Tree {
    match tree {
        Tree::Terminal(text) => Tree::Terminal(text),
        Tree::NonTerminal(name) => Tree::NonTerminal(name),
        Tree::Special(text) => Tree::Special(text),
        _ => Tree::NonTerminal("part"),
    }
}

/// A random railroad diagram: a notation, the config that changes its look, a title, and rules
/// of every kind of node, nested and long enough to fold.
fn random_diagram(rng: &mut Rng) -> String {
    let notation = rng.below(4);
    let mut src = String::new();
    if rng.chance(4) {
        src.push_str("---\nconfig:\n  railroad:\n");
        for _ in 0..1 + rng.below(3) {
            let line = rng.pick(&[
                "arcRadius: 0",
                "arcRadius: 4",
                "arcRadius: 7.5",
                "arcRadius: 20",
                "padding: 0",
                "padding: 3.3",
                "padding: 20",
                "fontSize: 9.5",
                "fontSize: 20",
                "horizontalSeparation: 0",
                "horizontalSeparation: 40",
                "verticalSeparation: 0",
                "verticalSeparation: 30",
                "strokeWidth: 2",
                "markerRadius: 8",
                "showMarkers: false",
            ]);
            src.push_str(&format!("    {line}\n"));
        }
        src.push_str("---\n");
    }
    src.push_str(["railroad-beta", "railroad-ebnf-beta", "railroad-abnf-beta", "railroad-peg-beta"][notation]);
    src.push('\n');
    if rng.chance(3) {
        let title = rng.pick(&[
            "Grammar",
            "\"Quoted title\"",
            "A title wide enough to need wrapping when the host is narrow, surely",
        ]);
        src.push_str(&format!("title {title}\n"));
    }
    if rng.chance(8) {
        src.push_str("accTitle: Grammar\naccDescr {\n  Two\n  lines\n}\n");
    }
    let mut names: Vec<&str> = Vec::new();
    for _ in 0..rng.below(4) {
        let name = if !names.is_empty() && rng.chance(6) { names[rng.below(names.len())] } else { rng.pick(NAMES) };
        names.push(name);
        let mut body = String::new();
        write(&tree(rng, 4), notation, &mut body);
        let arrow = ["=", "=", "=", "<-"][notation];
        src.push_str(&format!("{name} {arrow} {body} ;\n"));
    }
    src
}

#[test]
fn random_diagrams_match_pele() {
    let mut rng = Rng(0x7a11_d1a9_2a35);
    let mut cases = Vec::new();
    for _ in 0..4000 {
        let source = random_diagram(&mut rng);
        let options = match rng.below(6) {
            0 => json!({ "maxWidth": 320 }),
            1 => json!({ "maxWidth": *rng.pick(&["200", "480", "640", "900"]).parse::<f64>().as_ref().unwrap() }),
            _ => json!({}),
        };
        cases.push((source, options));
    }
    let tally = run("random", &cases).finish("random railroad diagrams");
    let redrawn = redrawn_to_fit(
        &cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect::<Vec<_>>(),
    );
    println!("their widths make Pele draw {redrawn} of the random railroad diagrams another way");
    let missed: Vec<&str> = PATHS.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect();
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random railroad diagram reaches {missed:?}");
}

/// Pele's own worst cases for railroad (`tests/railroad/stress.test.ts`) that render or parse,
/// with the source limit lifted as that test lifts it.
fn stress() -> Vec<String> {
    const N: usize = 50_000;
    let repeat = |count: usize, part: &dyn Fn(usize) -> String| (0..count).map(part).collect::<String>();
    let (ir, ebnf, abnf, peg) =
        ("railroad-beta\n", "railroad-ebnf-beta\n", "railroad-abnf-beta\n", "railroad-peg-beta\n");
    vec![
        format!("{ebnf}{} = {} ;", "a".repeat(N), "b".repeat(N)),
        format!("{ebnf}r = \"{}\" ;", "word ".repeat(N / 5)),
        format!("{ebnf}r = \"{}\" ;", "#35;&amp;<br>*_".repeat(N / 15)),
        format!("{peg}R <- \"{}\" ;", "\\n\\t\\\\".repeat(N / 6)),
        format!("{ebnf}r = {}a{} ;", "(".repeat(N / 2), ")".repeat(N / 2)),
        format!("{ebnf}r = {}a{} ;", "[".repeat(12000), "]".repeat(12000)),
        format!("{ebnf}r = {}a{} ;", "{".repeat(12000), "}".repeat(12000)),
        format!("{ebnf}r = a{} ;", "*+?".repeat(N / 3)),
        format!("{ebnf}r = a{} ;", " - b".repeat(12000)),
        format!("{ebnf}r = a{}{} ;", " - ( b".repeat(8000), " )".repeat(8000)),
        format!("{ebnf}r = {}", "([{".repeat(N / 3)),
        format!("{ebnf}r = {}z ;", repeat(12000, &|i| format!("a{i} | "))),
        format!("{ebnf}r = {};", "a ".repeat(12000)),
        format!("{ebnf}r = {}b{} ;", "( a | ".repeat(5000), " )".repeat(5000)),
        format!("{ebnf}{}", repeat(5000, &|i| format!("r{i} = a ;\n"))),
        format!("{ebnf}{}", "r = a | b ;\n".repeat(5000)),
        format!("{ir}r = {}terminal(\"a\"){} ;", "optional(oneOrMore(".repeat(2500), "))".repeat(2500)),
        format!("{ir}r = {}terminal(\"a\"){} ;", "sequence(".repeat(6000), ")".repeat(6000)),
        format!("{ir}r = choice({}special(\"b\")) ;", "terminal(\"a\"),".repeat(6000)),
        format!("{ir}r = {}", "choice(".repeat(N / 7)),
        format!("{abnf}r = {}a{} ;", "2*3( ".repeat(10000), " )".repeat(10000)),
        format!("{abnf}r = {}a{} ;", "[".repeat(12000), "]".repeat(12000)),
        format!("{abnf}r = {}*{}a {}b ;", "9".repeat(400), "9".repeat(400), "9".repeat(400)),
        format!("{abnf}r = {}b ;", "a / ".repeat(12000)),
        format!("{peg}R <- {}a{} ;", "!( &( ".repeat(5000), " ) )".repeat(5000)),
        format!("{peg}R <- {}a{} ;", "( ".repeat(10000), " )*".repeat(10000)),
        format!("{peg}R <- {}a ;", "!&".repeat(N / 2)),
        format!("{peg}R <- {};", "!a &\"b\" ".repeat(6000)),
    ]
}

#[test]
fn stress_sources_match_pele() {
    let options = json!({ "limit": 1_000_000_000u64 });
    let cases: Vec<(String, Value)> = stress().into_iter().map(|source| (source, options.clone())).collect();
    let tally = run("stress", &cases).finish("Pele's worst-case railroad diagrams");
    assert!(tally.identical > 20, "most worst cases render");
}
