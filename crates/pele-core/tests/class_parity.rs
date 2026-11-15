//! Differential test for drawing class diagrams: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/class/harvest.json` holds the class sources of Pele's own test suite and the
//! inputs of its seeded mutation runs, written by `scripts/harvest-class.mjs`. The widths put
//! every source through the attempts of `turn_to_fit` and `tighten`, and through the wrapping of
//! a diagram's parts into rows.

use pele_core::{HostConfig, LayoutOptions, MonospacePart, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/class/harvest.json");
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
    let list = |value: &Value| value.as_array().expect("a list").iter().map(text).collect::<Vec<String>>();
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = number(value),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(text(value)),
            "responsive" => layout.responsive = Some(flag(value)),
            "autoDirection" => layout.auto_direction = flag(value),
            "directionBreakpoint" => layout.direction_breakpoint = Some(number(value)),
            "links" => layout.resources.links = flag(value),
            "linkSchemes" => layout.resources.link_schemes = list(value),
            "linkRel" => layout.resources.link_rel = text(value),
            "monospace" => {
                layout.monospace = list(value)
                    .iter()
                    .map(|part| match part.as_str() {
                        "members" => MonospacePart::Members,
                        "attributes" => MonospacePart::Attributes,
                        "files" => MonospacePart::Files,
                        "messages" => MonospacePart::Messages,
                        other => panic!("{other} is not a monospace part"),
                    })
                    .collect();
            }
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

type Reached = fn(&str) -> bool;

/// The drawing paths a source can go through, each with what in Pele's SVG shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a title", |svg| svg.contains("class=\"pele-title\"")),
    ("a namespace", |svg| svg.contains("class=\"pele-cluster pele-namespace\"")),
    ("a namespace title struck by an edge", |svg| svg.contains("class=\"pele-cluster-titles\"")),
    ("a halo mixed from the theme", |svg| svg.contains("color-mix(in srgb,var(--_a) ")),
    ("a note", |svg| svg.contains("class=\"pele-node pele-note\"")),
    ("a note's edge", |svg| svg.contains("class=\"pele-edge pele-note-edge\"")),
    ("a lollipop interface", |svg| svg.contains("class=\"pele-node pele-interface\"")),
    ("a lollipop", |svg| svg.contains("<circle class=\"pele-marker\"")),
    ("an extension", |svg| marker_points(svg, false).contains(&3)),
    ("an aggregation", |svg| marker_points(svg, false).contains(&4)),
    ("a composition", |svg| marker_points(svg, true).contains(&4)),
    ("a dependency", |svg| svg.contains("fill=\"var(--_l)\" stroke=\"none\"")),
    ("a dashed relation", |svg| svg.contains("stroke-dasharray=\"6 4\"")),
    ("an edge label", |svg| svg.contains("class=\"pele-edge-label\"")),
    ("a cardinality", |svg| svg.contains("class=\"pele-cardinality\"")),
    ("a relation of a class to itself", |svg| {
        NAMES.iter().any(|name| svg.contains(&format!("data-id=\"id_{name}_{name}_")))
    }),
    ("an annotation", |svg| svg.contains("class=\"pele-annotation\"")),
    ("an annotation in a fill's ink", |svg| {
        tag_has(svg, "<text class=\"pele-annotation\"", "fill=\"var(--pele-fg-on-")
    }),
    ("a static member", |svg| svg.contains(" pele-static\" style=\"text-decoration:underline\"")),
    ("a static member with a text style", |svg| svg.contains(";text-decoration:underline\"")),
    ("an abstract member", |svg| svg.contains(" pele-abstract\" font-style=\"italic\"")),
    ("members in the monospace font", |svg| tag_has(svg, "<g class=\"pele-members\"", "font-family=\"var(--_fm)\"")),
    ("a divider", |svg| svg.contains("class=\"pele-divider\"")),
    ("a divider in the class's stroke", |svg| svg.contains("stroke=\"var(--_b)\" style=\"stroke:")),
    ("a class without dividers", |svg| has_class_without_divider(svg)),
    ("a class name in a fill's ink", |svg| svg.contains("font-weight=\"var(--_hw)\" fill=\"var(--pele-fg-on-")),
    ("a class name in a size of its own", |svg| {
        tag_has(svg, "<text class=\"pele-label\" font-size=", "font-weight=\"var(--_hw)\"")
    }),
    ("a class name with a text style", |svg| tag_has(svg, "font-weight=\"var(--_hw)\"", " style=")),
    ("a generic class name", |svg| svg.contains("&lt;T&gt;")),
    ("a rounded box with a style", |svg| svg.contains("style=\"rx:var(--_r);")),
    ("an author's class", |svg| svg.contains("class=\"pele-node pele-class k")),
    ("a link", |svg| svg.contains("<a href=")),
    ("a link with a target", |svg| svg.contains("target=\"_self\"")),
    ("a tooltip", |svg| svg.contains("<title>")),
    ("an accessible title", |svg| svg.contains("aria-labelledby=")),
];

/// Whether a tag that starts with `start` has `needle` among its attributes.
fn tag_has(svg: &str, start: &str, needle: &str) -> bool {
    svg.split(start).skip(1).any(|rest| rest[..rest.find('>').unwrap_or(rest.len())].contains(needle))
}

/// The number of points of each class marker drawn as a path, with or without a fill.
fn marker_points(svg: &str, filled: bool) -> Vec<usize> {
    let mut out = Vec::new();
    for piece in svg.split("<path class=\"pele-marker\" d=\"").skip(1) {
        let Some(end) = piece.find("Z\" stroke-linejoin=\"round\"") else {
            continue;
        };
        let rest = &piece[end + 26..];
        if rest.starts_with(" fill=") == filled {
            out.push(piece[..end].matches(['M', 'L']).count());
        }
    }
    out
}

fn has_class_without_divider(svg: &str) -> bool {
    svg.split("class=\"pele-node pele-class").skip(1).any(|class| {
        let class = &class[..class.find("</g>").unwrap_or(class.len())];
        !class.contains("pele-divider")
    })
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
                } else {
                    *if r#type == "class" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached.extend(PATHS.iter().filter(|(_, reached)| reached(svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "class" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "class" => {
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
            (PeleRender::Error { error, .. }, Err(RenderError::NotPorted)) => {
                Some(format!("pele rejects a class diagram, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {source:?}"));
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
            self.failures[..self.failures.len().min(12)].join("\n")
        );
        self
    }

    /// Fails unless the identical outputs went through every path named in [`PATHS`].
    fn reaches_every_path(&self, what: &str) {
        let missed: Vec<&str> =
            PATHS.iter().map(|(name, _)| *name).filter(|name| !self.reached.contains(name)).collect();
        assert!(missed.is_empty(), "no output of the {what} has {missed:?}");
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

/// The widths a host may have. 320 and 480 are under the breakpoint at which a diagram that
/// runs across is turned to run down, 640 is on it and 900 over it.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested class sources");
    assert!(tally.identical > 3000, "the harvest holds class diagrams Pele renders");
    assert!(tally.rejected > 1000, "the harvest holds class diagrams Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested class sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
        assert!(redrawn > 0, "maxWidth {width} changes some drawing");
    }
}

/// How many of the cases Pele draws another way for the width they give than with room to
/// spare: tightly, running down instead of across, or wrapped into rows.
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
fn mutated_sources_match_pele() {
    run("mutated", &plain(harvest("mutated"))).finish("mutated class sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "classDiagram\ndirection LR\nAnimal <|-- Duck\nAnimal <|-- Fish\nAnimal <|-- Zebra\nAnimal \"1\" *-- \"many\" Leg : has\nDuck o-- Egg\nFish ..> Water\nZebra --() Stripes";
    let crossing = "classDiagram\nnamespace Outer[\"A rather long namespace title\"] {\n  class Inside\n}\nnamespace Other {\n  class Far\n}\nLeft --> Inside\nRight --> Inside\nTop --> Inside\nFar --> Left\nInside \"0..*\" --> \"1\" Far : uses";
    let styled = "classDiagram\nclass Dark {\n  +x$\n  -y()*\n  #z : int\n}\nclass Pale:::k {\n  +a\n}\nclass Sized\n<<interface>> Dark\nstyle Dark fill:#036,stroke:#f00,font-size:20px\nclassDef k fill:#ffd,color:red,font-family:serif\nstyle Sized color:#333,font-size:24px,font-style:italic";
    let linked = "classDiagram\nclass A\nclass B\nclass C\nlink A \"https://example.com/a?b=1&c=2\" \"A tip & more\" _self\nclick B href \"file:///etc/passwd\" \"tip\"\nclick C call f() \"called\"\nA --> B\nB --> C";
    let loops = "classDiagram\nA \"1\" --> \"2\" A : self\nA --> A\nA \"x\" ..> A\nB --> B : again\nA --> B";
    let notes = "classDiagram\nnote \"A free note\nwith two lines\"\nclass A\nnote for A \"About A\"\nnamespace N {\n  class B\n  note \"In N\"\n}\nnote for B \"About B\"";
    let empty = "---\nconfig:\n  class:\n    hideEmptyMembersBox: true\n---\nclassDiagram\nclass A\nclass B {\n  +x\n}\nA --> B";
    let flat = "---\nconfig:\n  class:\n    hierarchicalNamespaces: false\n---\nclassDiagram\nnamespace A.B.C {\n  class X\n}\nnamespace A {\n  class Y\n}\nX --> Y";
    let spaced = "---\nconfig:\n  class:\n    nodeSpacing: 10\n    rankSpacing: 120\n---\nclassDiagram\nA <|-- B\nA <|-- C\nA <|-- D";
    let titled = "---\ntitle: A title much wider than the diagram under it, surely\n---\nclassDiagram\naccTitle: Accessible\naccDescr: Described\nclass A";
    let crowded = "classDiagram\nHub \"1\" --> \"one\" A\nHub \"1\" --> \"two\" B\nHub \"1\" --> \"three\" C\nHub \"1\" --> \"four\" D\nE --> Hub\nF --> Hub\nG --> Hub";
    let parts = "classDiagram\nA --> B\nC --> D\nE --> F\nG\nH \"1\" --> \"2\" I\nnote \"apart\"";
    let mut out = vec![
        (across.to_owned(), json!({ "maxWidth": 200 })),
        (across.to_owned(), json!({ "maxWidth": 200, "autoDirection": false })),
        (across.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (across.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (across.to_owned(), json!({ "maxWidth": 0 })),
        (crossing.to_owned(), json!({})),
        (crossing.to_owned(), json!({ "maxWidth": 300 })),
        (styled.to_owned(), json!({})),
        (styled.to_owned(), json!({ "monospace": ["members"] })),
        (styled.to_owned(), json!({ "monospace": ["attributes"] })),
        (linked.to_owned(), json!({})),
        (linked.to_owned(), json!({ "links": false })),
        (linked.to_owned(), json!({ "linkSchemes": ["file"], "linkRel": "" })),
        (loops.to_owned(), json!({})),
        (loops.to_owned(), json!({ "maxWidth": 200 })),
        (notes.to_owned(), json!({})),
        (empty.to_owned(), json!({})),
        (flat.to_owned(), json!({})),
        (spaced.to_owned(), json!({})),
        (titled.to_owned(), json!({ "padding": 0 })),
        (titled.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (crowded.to_owned(), json!({})),
        (crowded.to_owned(), json!({ "fontSize": 12 })),
        (parts.to_owned(), json!({ "maxWidth": 250 })),
        (parts.to_owned(), json!({ "maxWidth": 120, "padding": 20 })),
        ("classDiagram\nclass A".to_owned(), json!({ "outputLimit": 200 })),
        ("classDiagram\nclass A".to_owned(), json!({ "limit": 10 })),
    ];
    for config in [
        json!({"class": {"nodeSpacing": 1e308, "rankSpacing": -1}}),
        json!({"class": {"hideEmptyMembersBox": "true", "useMaxWidth": false}}),
        json!({"class": {"hierarchicalNamespaces": false}}),
    ] {
        out.push((crossing.to_owned(), json!({ "config": config })));
    }
    out
}

#[test]
fn cases_match_pele() {
    run("case", &cases()).finish("class cases");
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

const NAMES: &[&str] = &["A", "B", "C", "D", "E", "Animal", "Duck", "Order", "Line", "x1", "Shape", "Circle"];
const ENDS: &[&str] = &["", "", "<|", "|>", "<", ">", "*", "o"];
const MEMBERS: &[&str] = &[
    "+String name",
    "-int age",
    "+run(a, b) bool",
    "#List~int~ items$",
    "+area()* double",
    "~helper()",
    "+getSize() int$",
    "<<interface>>",
    "id",
    "+Map~K, V~ map",
    "-#35; hash",
    "+a much longer attribute name that takes room",
];
/// Style lists that Pele parses: the first declaration of a list cannot name a property with a
/// hyphen, which the grammar reads as a `MINUS`.
const STYLES: &[&str] = &[
    "fill:#f9f",
    "fill:#036",
    "stroke:#333,stroke-width:4px",
    "color:red",
    "fill:#bbf,color:#fff",
    "color:#333,font-size:20px",
    "fill:#ffe,font-family:serif",
    "color:navy,font-style:italic",
    "fill:#000,opacity:0.3",
];
const CARDS: &[&str] = &["1", "0..1", "*", "many", "1..n", "0..*"];

fn name(rng: &mut Rng, most: usize) -> String {
    NAMES[rng.below(most)].to_owned()
}

fn class_body(rng: &mut Rng) -> String {
    let mut out = String::new();
    for _ in 0..rng.below(5) {
        out += "  ";
        out += rng.pick(MEMBERS);
        out += "\n";
    }
    out
}

fn statement(rng: &mut Rng, depth: usize, most: usize) -> String {
    match rng.below(16) {
        0..=5 => {
            let (a, b) = (name(rng, most), name(rng, most));
            let (left, right) = (rng.pick(ENDS), rng.pick(ENDS));
            let line = if rng.chance(4) { ".." } else { "--" };
            let card = |rng: &mut Rng| if rng.chance(3) { format!("\"{}\" ", rng.pick(CARDS)) } else { String::new() };
            let (from, to) = (card(rng), card(rng));
            let label = if rng.chance(3) {
                format!(" : {}", rng.pick(&["uses", "has many", "x", "a longer label"]))
            } else {
                String::new()
            };
            format!("{a} {from}{left}{line}{right} {to}{b}{label}")
        }
        6 | 7 => {
            let mut out = format!("class {}", name(rng, most));
            if rng.chance(8) {
                out += "~T~";
            }
            if rng.chance(8) {
                out += "[\"A label\"]";
            }
            match rng.below(5) {
                0 => out += ":::k",
                1 => out += " <<service>>",
                _ => {}
            }
            if rng.chance(2) && !out.ends_with(":::k") {
                out += &format!(" {{\n{}}}", class_body(rng));
            }
            out
        }
        8 => format!("{} : {}", name(rng, most), rng.pick(MEMBERS)),
        9 if depth < 2 => {
            let label = if rng.chance(3) { "[\"Group title\"]" } else { "" };
            let mut out = format!("namespace {}{label} {{\n", rng.pick(&["N", "M", "N.M", "Outer.Inner", "Pkg"]));
            for _ in 0..1 + rng.below(3) {
                if rng.chance(5) && depth < 1 {
                    out += &format!("namespace {} {{\nclass {}\n}}", rng.pick(&["Sub", "N.Sub"]), name(rng, most));
                } else if rng.chance(5) {
                    out += "note \"in a namespace\"";
                } else {
                    out += &format!("class {}", name(rng, most));
                }
                out += "\n";
            }
            out + "}"
        }
        10 => match rng.below(3) {
            0 => format!("note \"{}\"", rng.pick(&["free note", "two\\nlines", "**bold** note"])),
            _ => format!("note for {} \"{}\"", name(rng, most), rng.pick(&["n", "about it"])),
        },
        11 => match rng.below(3) {
            0 => format!("{} ()-- {}", rng.pick(&["IFace", "Port", "x1"]), name(rng, most)),
            1 => format!("{} --() {}", name(rng, most), rng.pick(&["Plug", "Socket"])),
            _ => format!("<<{}>> {}", rng.pick(&["interface", "enum", "abstract"]), name(rng, most)),
        },
        12 => match rng.below(3) {
            0 => format!("style {} {}", name(rng, most), rng.pick(STYLES)),
            1 => format!("classDef k {}", rng.pick(STYLES)),
            _ => format!("cssClass \"{}\" k", name(rng, most)),
        },
        13 => match rng.below(4) {
            0 => format!("link {} \"https://example.com\" \"tip\" _self", name(rng, most)),
            1 => format!("click {} href \"/local\"", name(rng, most)),
            2 => format!("callback {} \"cb\" \"a tooltip\"", name(rng, most)),
            _ => format!("click {} call f()", name(rng, most)),
        },
        14 => {
            let a = name(rng, most);
            format!(
                "{a} {}--> {a}{}",
                if rng.chance(2) { "\"1\" " } else { "" },
                if rng.chance(2) { " : self" } else { "" }
            )
        }
        _ => rng.pick(&["%% a comment", "accTitle: The title", "accDescr: A description", ""]).to_owned(),
    }
}

/// A class diagram that Pele nearly always renders, with host options, to put many graphs
/// through the markers, the spreading of edges along a side, the cardinalities, the namespaces
/// and the width attempts.
fn random_class(rng: &mut Rng) -> (String, Value) {
    let mut source = String::new();
    let mut config = String::new();
    if rng.chance(8) {
        config += "    hideEmptyMembersBox: true\n";
    }
    if rng.chance(8) {
        config += "    hierarchicalNamespaces: false\n";
    }
    if rng.chance(8) {
        config += &format!("    nodeSpacing: {}\n    rankSpacing: {}\n", 5 + rng.below(90), 5 + rng.below(90));
    }
    let title = rng.chance(6);
    if !config.is_empty() || title {
        source += "---\n";
        if title {
            source += &format!(
                "title: {}\n",
                rng.pick(&["A title", "A much longer title than the diagram is wide, most likely"])
            );
        }
        if !config.is_empty() {
            source += &format!("config:\n  class:\n{config}");
        }
        source += "---\n";
    }
    source += "classDiagram\n";
    if rng.chance(2) {
        source += &format!("direction {}\n", rng.pick(&["LR", "RL", "BT", "TB"]));
    }
    let most = 2 + rng.below(NAMES.len() - 1);
    let many = rng.chance(5);
    for _ in 0..1 + rng.below(if many { 30 } else { 12 }) {
        source += &statement(rng, 0, most);
        source += "\n";
    }

    let mut options = serde_json::Map::new();
    if rng.below(3) > 0 {
        let width = match rng.below(5) {
            0 => 100.0 + rng.below(900) as f64 + 0.5,
            width => WIDTHS[width - 1],
        };
        options.insert("maxWidth".to_owned(), width.into());
    }
    if rng.chance(8) {
        options.insert("autoDirection".to_owned(), false.into());
    }
    if rng.chance(8) {
        options.insert("padding".to_owned(), (rng.below(41) as f64 / 2.0).into());
    }
    if rng.chance(8) {
        options.insert("fontSize".to_owned(), [12.0, 13.0, 14.0, 18.5, 20.0][rng.below(5)].into());
    }
    if rng.chance(8) {
        options.insert("monospace".to_owned(), json!(["members"]));
    }
    if rng.chance(16) {
        options.insert("links".to_owned(), false.into());
    }
    (source, Value::Object(options))
}

#[test]
fn random_class_diagrams_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_class(&mut rng)).collect();
    let tally = run("random class diagram", &cases).finish("random class diagrams");
    assert!(tally.identical > 3000, "nearly every random class diagram renders");
    tally.reaches_every_path("random class diagrams");
    println!("the random class diagrams reach the {} drawing paths: {:?}", PATHS.len(), tally.reached);
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random class diagrams another way");
    assert!(redrawn > 500, "the widths put many random class diagrams through the attempts to fit");
}
