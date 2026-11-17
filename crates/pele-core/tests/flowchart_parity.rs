//! Differential test for drawing flowcharts: every source must come out of this port as it comes
//! out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/flowchart/harvest.json` holds the flowchart sources of Pele's own test suite
//! and the inputs of its seeded mutation runs, written by `scripts/harvest-flowchart.mjs`. The
//! widths put every source through the attempts of `turn_to_fit` and `tighten`.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/flowchart/harvest.json");
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
            "padding" => layout.padding = Some(number(value)),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(text(value)),
            "responsive" => layout.responsive = Some(flag(value)),
            "autoDirection" => layout.auto_direction = flag(value),
            "directionBreakpoint" => layout.direction_breakpoint = Some(number(value)),
            "links" => layout.resources.links = flag(value),
            "images" => layout.resources.images = flag(value),
            "linkSchemes" => layout.resources.link_schemes = list(value),
            "imageSchemes" => layout.resources.image_schemes = list(value),
            "linkRel" => layout.resources.link_rel = text(value),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            "maxEdges" => parse.limits.max_edges = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
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

/// What in Pele's SVG shows that a source went through one path of the draw step.
const PATHS: &[(&str, &str)] = &[
    ("a title", "class=\"pele-title\""),
    ("a group", "class=\"pele-clusters\""),
    ("a group that sets its fill", "fill=\"var(--_a)\" stroke="),
    ("a title struck by an edge", "class=\"pele-cluster-titles\""),
    ("a halo mixed from the theme", "color-mix(in srgb,var(--_a) "),
    ("a casing", "class=\"pele-casings\""),
    ("a casing wider than the default", "<g stroke-width=\""),
    ("a casing with a computed width", "stroke-width:calc("),
    ("a casing as faint as its edge", "<g style=\"opacity:"),
    ("an edge in the ink of the fill under it", "\" stroke=\"var(--pele-fg-on-"),
    ("a label in the ink of the fill under it", "\" fill=\"var(--pele-fg-on-"),
    ("an edge with a style", "class=\"pele-edge\" data-id=\"L_N1_N2_0\" style="),
    ("a marker in its edge's colour", "class=\"pele-marker\" d=\"M"),
    ("a circle marker", "<circle class=\"pele-marker\""),
    ("a dotted edge", "stroke-dasharray=\"3 4\""),
    ("a thick edge", "stroke-width=\"2.5\""),
    ("an edge label", "class=\"pele-edge-label\""),
    ("a picture", "<image href="),
    ("an icon", "class=\"pele-icon\""),
    ("a link", "<a href="),
    ("a link with a target", "target=\"_blank\""),
    ("a link inside the host", "class=\"internal-link\""),
    ("a tooltip", "<title>"),
    ("an author's class", "class=\"pele-node pele-shape-rect c1\""),
    ("a label of several runs", "xml:space=\"preserve\""),
    ("an italic run", "font-style=\"italic\""),
    ("a bold label", "font-weight=\"var(--_w)\""),
    ("a style on a label", "class=\"pele-label\" style="),
    ("a rounded shape with a style", "style=\"rx:var(--_r);"),
    ("an accessible title", "aria-labelledby="),
];

/// Where two texts first differ, with what each holds there.
fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(80).min(bytes.len())..(at + 80).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
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
                    *if r#type == "flowchart" { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached.extend(PATHS.iter().filter(|(_, mark)| svg.contains(mark)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "flowchart" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "flowchart" => {
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
                Some(format!("pele rejects a flowchart, ours reports it as not ported: {error}"))
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

/// The widths a host may have. 320 and 480 are under the breakpoint at which a flowchart that
/// runs across is turned to run down, 640 is on it and 900 over it.
const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

#[test]
fn harvested_sources_match_pele() {
    let tally = run("harvested", &plain(harvest("sources"))).finish("harvested flowchart sources");
    assert!(tally.identical > 1400, "the harvest holds flowcharts Pele renders");
    assert!(tally.rejected > 100, "the harvest holds flowcharts Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        run("harvested", &cases).finish(&format!("harvested flowchart sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
        assert!(redrawn > 0, "maxWidth {width} changes some drawing");
    }
}

/// How many of the cases Pele draws another way for the width they give than with room to spare:
/// tightly, or running down instead of across.
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
    run("mutated", &plain(harvest("mutated"))).finish("mutated flowchart sources");
}

/// Hand-written sources for what the harvest reaches least, each with the host options that
/// reach it.
fn cases() -> Vec<(String, Value)> {
    let across = "flowchart LR\nA[Start here] --> B{Is it?} -->|yes| C[(Store)] --> D([Done and dusted])\nB -->|no| E[[Sub]] --> D";
    let linked = "flowchart TD\nA[Home] --> B[Notes]\nclick A \"https://example.com/a?b=1&c=2\" \"A tip & more\" _blank\nclick B \"file:///etc/passwd\"\nC[My Note]:::internal-link --> D[Other / Note]\nclass D internal-link\nE[\"javascript:alert(1)\"]:::internal-link\nF[\" \"]:::internal-link";
    let pictures = "flowchart TD\nA@{ img: \"https://example.com/a.png\", label: \"Pic\", w: 60, h: 40, pos: t, constraint: on }\nB@{ img: \"ftp://example.com/b.png\" }\nC@{ icon: \"fa:user\", form: circle, label: \"User\", pos: t, h: 60 }\nD@{ icon: \"fa:#35;x\", form: rounded }\nE@{ icon: \"fa:y\", form: square, label: \"Below\" }\nF@{ icon: \"fa:z\" }\nA --> B --> C --> D --> E --> F";
    let filled = "flowchart TD\nsubgraph Dark [Dark group]\n  A --> B\n  B --> B\nend\nsubgraph Pale [\"A much longer pale title\"]\n  C -.-> D\nend\nsubgraph Faint\n  E ==> F\nend\nA --> C\nB --x E\nD o--o F\nstyle Dark fill:#036\nstyle Pale fill:#ffd,opacity:0.9\nstyle Faint fill:#000,fill-opacity:0.2\nlinkStyle 0 stroke:red,stroke-width:3px,opacity:0.5\nlinkStyle 3 stroke-width:2em,stroke-opacity:0.4\nlinkStyle 4 stroke:none\nlinkStyle 5 stroke:transparent";
    let struck = "flowchart TD\nsubgraph Outer [An outer title that is quite long]\n  subgraph Inner [Inner title here]\n    A\n  end\nend\nX --> A\nY --> Inner\nZ --> Outer\nstyle Inner color:red";
    let mut out = vec![
        (across.to_owned(), json!({ "maxWidth": 200 })),
        (across.to_owned(), json!({ "maxWidth": 200, "autoDirection": false })),
        (across.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (across.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (across.to_owned(), json!({ "maxWidth": 0 })),
        (across.to_owned(), json!({ "maxWidth": 320.5, "padding": 0, "fontSize": 13 })),
        (across.to_owned(), json!({ "responsive": false, "idPrefix": "a\"b", "padding": 20.5 })),
        (linked.to_owned(), json!({})),
        (linked.to_owned(), json!({ "links": false })),
        (linked.to_owned(), json!({ "linkRel": "" })),
        (linked.to_owned(), json!({ "linkRel": "nofollow \"x\"", "linkSchemes": ["file"] })),
        (pictures.to_owned(), json!({})),
        (pictures.to_owned(), json!({ "images": false })),
        (pictures.to_owned(), json!({ "imageSchemes": ["ftp"] })),
        (filled.to_owned(), json!({})),
        (filled.to_owned(), json!({ "maxWidth": 200 })),
        (struck.to_owned(), json!({})),
        (struck.to_owned(), json!({ "config": { "flowchart": { "curve": "step" } } })),
    ];
    let more = [
        "flowchart TD\nA[\"`**Bold** and _italic_\n  and a second line that is long enough to wrap around`\"] --> B[\"`plain`\"]\nB -- \"`**edge** label`\" --> C",
        "flowchart LR\nA[<b>all bold</b>] --> B[a <i>mixed</i> <code>label</code> x<sub>2</sub>]\nB --> C[fa:fa-car Car fa:fa-none]\nC --> D[line one<br>line two<br/><br>four]",
        "flowchart TD\nA --> B\nstyle A fill:#f9f,stroke:#333,stroke-width:4px,color:#fff,font-weight:bold,font-size:20px,font-style:italic\nstyle B stroke:a&fill:navy,opacity:50%,rx:20\nclassDef default fill:#eee,stroke-dasharray: 5 5\nclassDef big font-size:30px\nC:::big --> D:::missing",
        "flowchart TD\nA e1@--> B\nB e2@--> C\nC e3@--> D\nD e4@--> A\ne1@{ curve: linear }\ne2@{ curve: step }\ne3@{ curve: stepBefore }\ne4@{ curve: stepAfter }\nA --> C\nB --> D",
        "---\ntitle: A title that is long enough to be wider than the chart under it\nconfig:\n  flowchart:\n    curve: linear\n    nodeSpacing: 10\n    rankSpacing: 5000\n    wrappingWidth: 60\n    useMaxWidth: false\n---\nflowchart TD\nA[Some words that wrap in a narrow box] --> B & C\nB --> D\nC --> D",
        "flowchart TD\n  accTitle: The <title> & more\n  accDescr: A \"description\"\nA --> A\nA --> A\nA -- looped --> A\nB ~~~ C\nC --o C",
        "flowchart RL\nsubgraph S\n direction TB\n A --> B\n B -- self --> B\nend\nsubgraph T\n direction LR\n C --> D\nend\nS --> T\nA --> D",
        "flowchart TD\nsubgraph O\n subgraph I\n  a --> b\n end\n c\nend\nx --> a\nb --> x\nI@{ view: collapsed }",
        "flowchart TD\nsubgraph \" \"\nA\nend\nsubgraph E [x]\nB\nend\nsubgraph Tall [One<br>Two<br>Three]\nC\nend\nA --> B --> C",
        "flowchart TD\nA[x]:::a.b --> B\nclass A,B q\nclassDef q stroke:blue\nA:::default --> C:::defaultx",
        "flowchart TD\nA@{ shape: text, label: \"Only text\" } --> B@{ shape: braces, label: \"Braced\" }\nB --> C@{ shape: fork }\nC --> D@{ shape: f-circ }\nstyle A fill:#000\nstyle B fill:#000,stroke:red\nstyle D stroke:red,fill:blue",
        "flowchart LR\nA -->|\"a label\"| B\nB -->|\"another label that is long enough to wrap onto more lines than one\"| C\nlinkStyle default color:red,font-size:20px,stroke:#00f\nlinkStyle 1 stroke-width:1px",
        "flowchart TD\nA --> B\nlinkStyle 0 stroke-width:.,stroke:green",
        "flowchart TD\nA((x)) --> B((\" \"))\nC[\"#quot;quoted#quot; #amp; #lt;tag#gt; #35;\"] --> D[\"&amp; &lt;b&gt; &#9829;\"]",
        "flowchart TD\nA[\"\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{9577}\u{3044}\u{30e9}\u{30d9}\u{30eb}\u{3067}\u{3059}\u{3002}\u{6298}\u{308a}\u{8fd4}\u{3057}\u{307e}\u{3059}\u{3002}\u{3082}\u{3063}\u{3068}\u{9577}\u{304f}\u{3057}\u{307e}\u{3059}\"] --> B[\"emoji \u{1f600} \u{1f469}\u{200d}\u{1f4bb}\"]",
        "flowchart TD\nA --> B\nclick A \"https://example.com\" \"tip\"\nclick B call f() \"only a tip\"\nstyle A fill:#222",
        "flowchart TD\nsubgraph G\nA\nend\nG --> G\nA --> G\nG --> A",
        "flowchart TD\nA --> B\nB --> A\nA --> B\nA <--> B\nA x--x B\nA o--o B",
    ];
    out.extend(more.iter().flat_map(|source| {
        [json!({}), json!({ "maxWidth": 320 }), json!({ "maxWidth": 120 })]
            .map(|options| ((*source).to_owned(), options))
    }));
    let shapes = [
        "rect",
        "rounded",
        "stadium",
        "fr-rect",
        "cyl",
        "datastore",
        "folder",
        "bucket",
        "console",
        "browser",
        "person",
        "circle",
        "bang",
        "cloud",
        "diam",
        "hex",
        "lean-r",
        "lean-l",
        "trap-b",
        "trap-t",
        "dbl-circ",
        "text",
        "notch-rect",
        "lin-rect",
        "sm-circ",
        "fr-circ",
        "fork",
        "hourglass",
        "brace",
        "brace-r",
        "braces",
        "bolt",
        "doc",
        "delay",
        "h-cyl",
        "lin-cyl",
        "curv-trap",
        "div-rect",
        "tri",
        "win-pane",
        "f-circ",
        "notch-pent",
        "flip-tri",
        "sl-rect",
        "docs",
        "st-rect",
        "bow-rect",
        "cross-circ",
        "tag-doc",
        "tag-rect",
        "flag",
        "odd",
        "lin-doc",
        "state",
        "choice",
        "note",
        "anchor",
        "icon",
    ];
    for direction in ["TD", "LR", "BT", "RL"] {
        let mut source = format!("flowchart {direction}\n");
        for (index, shape) in shapes.iter().enumerate() {
            source += &format!("N{index}@{{ shape: {shape}, label: \"The {shape} shape\" }}\n");
            source += &format!("N{index} --> N{}\nN{index} --> M{}\n", (index + 1) % shapes.len(), index % 7);
            if index % 3 == 0 {
                source += &format!("style N{index} fill:#135,stroke:#f00,stroke-width:2px\n");
            }
        }
        out.push((source.clone(), json!({})));
        out.push((source, json!({ "maxWidth": 480 })));
    }
    out
}

#[test]
fn cases_match_pele() {
    let tally = run("case", &cases()).finish("flowchart cases");
    assert_eq!(tally.rejected, 0, "every case renders");
    let shapes: Vec<String> =
        cases().iter().filter(|(source, _)| source.contains("N0@{")).map(|(source, _)| source.clone()).collect();
    let PeleRender::Svg { svg, .. } = &pele_oracle::render_all(&shapes[..1])[0] else { panic!("the shapes render") };
    for shape in NAMED_SHAPES {
        assert!(svg.contains(&format!("pele-shape-{shape}\"")), "the cases draw a {shape}");
    }
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

const LABELS: &[&str] = &[
    "Start",
    "A longer label with several words in it",
    "\"`**Bold** and _italic_ words`\"",
    "\"`A markdown label\n  on two lines`\"",
    "<b>bold</b> text",
    "one<br>two",
    "#35; and #amp;",
    "fa:fa-car Car",
    "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30e9}\u{30d9}\u{30eb}",
    "x",
    "\"Quoted (text) [here]\"",
    "Supercalifragilisticexpialidocious",
];
const BRACKETS: &[(&str, &str)] = &[
    ("[", "]"),
    ("(", ")"),
    ("((", "))"),
    ("(((", ")))"),
    ("([", "])"),
    ("[[", "]]"),
    ("[(", ")]"),
    ("{", "}"),
    ("{{", "}}"),
    ("[/", "/]"),
    ("[\\", "\\]"),
    ("[/", "\\]"),
    ("[\\", "/]"),
    (">", "]"),
];
const NAMED_SHAPES: &[&str] = &[
    "rect",
    "rounded",
    "stadium",
    "fr-rect",
    "cyl",
    "datastore",
    "folder",
    "bucket",
    "console",
    "browser",
    "person",
    "circle",
    "bang",
    "cloud",
    "diam",
    "hex",
    "lean-r",
    "lean-l",
    "trap-b",
    "trap-t",
    "dbl-circ",
    "text",
    "notch-rect",
    "lin-rect",
    "sm-circ",
    "fr-circ",
    "fork",
    "hourglass",
    "brace",
    "brace-r",
    "braces",
    "bolt",
    "doc",
    "delay",
    "h-cyl",
    "lin-cyl",
    "curv-trap",
    "div-rect",
    "tri",
    "win-pane",
    "f-circ",
    "notch-pent",
    "flip-tri",
    "sl-rect",
    "docs",
    "st-rect",
    "bow-rect",
    "cross-circ",
    "tag-doc",
    "tag-rect",
    "flag",
    "odd",
    "lin-doc",
];
const LINKS: &[&str] = &[
    "-->",
    "-->",
    "-->",
    "---",
    "-.->",
    "==>",
    "~~~",
    "--x",
    "--o",
    "<-->",
    "x--x",
    "o--o",
    "---->",
    "-..->",
    "====>",
    "-- yes -->",
    "-. maybe .->",
    "== no ==>",
    "-->|a label|",
    "-->|\"`**md** label`\"|",
    "<-- both ways -->",
    "-->|a much longer label that wraps onto more than one line of text|",
];
const DECLARATIONS: &[&str] = &[
    "fill:#f9f",
    "fill:#036",
    "fill:#000,fill-opacity:0.3",
    "fill:navy,opacity:0.8",
    "fill:#0368",
    "stroke:a&fill:navy",
    "fill:none",
    "stroke:#333,stroke-width:4px",
    "stroke:red,stroke-dasharray:5 5",
    "color:red",
    "color:#fff,fill:#222",
    "font-weight:bold",
    "font-size:20px,font-style:italic",
    "stroke-width:2em",
    "opacity:0.5",
    "stroke:none",
    "rx:10,ry:10",
];
const METADATA: &[&str] = &[
    "{ img: \"https://example.com/a.png\", w: 60, h: 40, label: \"Pic\" }",
    "{ img: \"https://example.com/a.png\", pos: t, constraint: on, label: \"Top\" }",
    "{ img: \"javascript:alert(1)\" }",
    "{ icon: \"fa:user\", form: circle, label: \"User\" }",
    "{ icon: \"fa:user\", form: square, pos: t, label: \"Top\", h: 60 }",
    "{ icon: \"fa:user\", form: rounded }",
    "{ icon: \"fa:user\" }",
];
const CURVES: &[&str] = &["basis", "linear", "step", "stepBefore", "stepAfter", "natural", "monotoneX"];

fn id(rng: &mut Rng, most: usize) -> String {
    format!("N{}", rng.below(most))
}

fn node(rng: &mut Rng, most: usize) -> String {
    let id = id(rng, most);
    match rng.below(8) {
        0..=2 => {
            let (open, close) = BRACKETS[rng.below(BRACKETS.len())];
            format!("{id}{open}{}{close}", rng.pick(LABELS))
        }
        3 => {
            let shape = rng.pick(NAMED_SHAPES);
            let label = rng.pick(LABELS);
            let label = if label.starts_with('"') { label.to_owned() } else { format!("\"{label}\"") };
            format!("{id}@{{ shape: {shape}, label: {label} }}")
        }
        4 if rng.chance(3) => format!("{id}@{}", rng.pick(METADATA)),
        _ => id,
    }
}

/// One statement. `edges` counts the links written so far, so that every `linkStyle` index exists.
fn statement(rng: &mut Rng, depth: usize, most: usize, edges: &mut usize, groups: &mut Vec<String>) -> String {
    match rng.below(20) {
        0..=8 => {
            let mut out = node(rng, most);
            for _ in 0..1 + rng.below(3) {
                let edge_id = if rng.chance(8) { format!("e{}@", *edges) } else { String::new() };
                let gap = if !edge_id.is_empty() && out.ends_with('}') { "" } else { " " };
                out += &format!("{gap}{edge_id}{} {}", rng.pick(LINKS), node(rng, most));
                *edges += 1;
            }
            out
        }
        9 | 10 if depth < 3 => {
            let name = format!("G{}", groups.len());
            groups.push(name.clone());
            let mut out = match rng.below(3) {
                0 => format!("subgraph {name}\n"),
                1 => format!("subgraph {name} [{}]\n", rng.pick(LABELS)),
                _ => format!("subgraph {name} [\"A group title that is rather long, to be crossed\"]\n"),
            };
            if rng.chance(3) {
                out += &format!("direction {}\n", rng.pick(&["TB", "LR", "RL", "BT"]));
            }
            for _ in 0..1 + rng.below(4) {
                out += &statement(rng, depth + 1, most, edges, groups);
                out += "\n";
            }
            out + "end"
        }
        11 => format!("style {} {}", id(rng, most), rng.pick(DECLARATIONS)),
        12 if !groups.is_empty() => {
            let group = groups[rng.below(groups.len())].clone();
            format!("style {group} {}", rng.pick(DECLARATIONS))
        }
        13 => format!("classDef {} {}", rng.pick(&["c1", "c2", "default", "internal-link"]), rng.pick(DECLARATIONS)),
        14 => format!("class {} {}", id(rng, most), rng.pick(&["c1", "c2", "internal-link"])),
        15 if *edges > 0 => {
            let at = if rng.chance(4) { "default".to_owned() } else { rng.below(*edges).to_string() };
            match rng.below(3) {
                0 => format!("linkStyle {at} {}", rng.pick(DECLARATIONS)),
                1 => format!("linkStyle {at} interpolate {}", rng.pick(CURVES)),
                _ => format!("linkStyle {at} interpolate {} {}", rng.pick(CURVES), rng.pick(DECLARATIONS)),
            }
        }
        16 => {
            let id = id(rng, most);
            match rng.below(3) {
                0 => {
                    let target = rng.pick(&[" _blank", " _self", ""]);
                    format!("click {id} \"https://example.com/{id}?a=1&b=2\" \"A tip\"{target}")
                }
                1 => {
                    let url = rng.pick(&["/local/path", "mailto:a@b.c", "file:///x", "//host/share"]);
                    format!("click {id} \"{url}\"")
                }
                _ => format!("click {id} call f() \"Only a tip\""),
            }
        }
        17 => {
            let id = id(rng, most);
            format!("{id} --> {id}")
        }
        18 if !groups.is_empty() => {
            let group = groups[rng.below(groups.len())].clone();
            match rng.below(3) {
                0 => format!("{group} --> {}", id(rng, most)),
                1 => format!("{} --> {group}", id(rng, most)),
                _ => format!("{group}@{{ view: collapsed }}"),
            }
        }
        _ => rng.pick(&["accTitle: The title", "accDescr: A description", "%% a comment", ""]).to_owned(),
    }
}

/// A flowchart that Pele nearly always renders, with host options, to put many graphs through
/// the shapes, the routes, the fills under edges and the width attempts.
fn random_flowchart(rng: &mut Rng) -> (String, Value) {
    let mut source = String::new();
    let mut config = String::new();
    if rng.chance(4) {
        config += &format!("    curve: {}\n", rng.pick(CURVES));
    }
    if rng.chance(6) {
        config += &format!("    nodeSpacing: {}\n    rankSpacing: {}\n", 5 + rng.below(90), 5 + rng.below(90));
    }
    if rng.chance(6) {
        config += &format!("    wrappingWidth: {}\n", 40 + rng.below(300));
    }
    let title = rng.chance(5);
    if !config.is_empty() || title {
        source += "---\n";
        if title {
            source += &format!(
                "title: {}\n",
                rng.pick(&["A title", "A much longer title than the chart is wide, most likely"])
            );
        }
        if !config.is_empty() {
            source += &format!("config:\n  flowchart:\n{config}");
        }
        source += "---\n";
    }
    source += &format!("flowchart {}\n", rng.pick(&["TD", "LR", "LR", "RL", "BT", "TB"]));
    let (many_nodes, many_statements) = (rng.chance(6), rng.chance(6));
    let most = 2 + rng.below(if many_nodes { 30 } else { 9 });
    let count = 1 + rng.below(if many_statements { 40 } else { 12 });
    let (mut edges, mut groups) = (0, Vec::new());
    for _ in 0..count {
        source += &statement(rng, 0, most, &mut edges, &mut groups);
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
        options.insert("directionBreakpoint".to_owned(), (200.0 + rng.below(800) as f64).into());
    }
    if rng.chance(8) {
        options.insert("padding".to_owned(), (rng.below(41) as f64 / 2.0).into());
    }
    if rng.chance(8) {
        options.insert("fontSize".to_owned(), [12.0, 13.0, 14.0, 18.5, 20.0][rng.below(5)].into());
    }
    if rng.chance(12) {
        options.insert(rng.pick(&["links", "images"]).to_owned(), false.into());
    }
    (source, Value::Object(options))
}

#[test]
fn random_flowcharts_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..4000).map(|_| random_flowchart(&mut rng)).collect();
    let tally = run("random flowchart", &cases).finish("random flowcharts");
    assert!(tally.identical > 3600, "nearly every random flowchart renders");
    tally.reaches_every_path("random flowcharts");
    let redrawn = redrawn_to_fit(&cases);
    println!("the widths make Pele draw {redrawn} of the random flowcharts another way");
    assert!(redrawn > 1000, "the widths put many random flowcharts through the attempts to fit");
}
