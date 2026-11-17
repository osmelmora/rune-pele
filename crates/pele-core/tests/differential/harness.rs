//! What the differential tests of the standalone types share: their harvest, a seeded random
//! source of inputs, the comparison of a front end's model with an oracle op, and the comparison
//! of a whole render with Pele's, each tallied with what made it differ.
//!
//! A test includes it with `#[path = "differential/harness.rs"] mod harness;`.

#![allow(dead_code, reason = "each test uses the part it needs")]

use pele_core::{ErrorCode, HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// One list of `tests/fixtures/<kind>/harvest.json`.
pub fn harvest(kind: &str, list: &str) -> Vec<String> {
    let path = pele_oracle::root().join(format!("tests/fixtures/{kind}/harvest.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// xorshift64, so the inputs are the same on every run.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    pub fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    pub fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
    /// A number between 0 and 1.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A value as one JSON string on one line, as the oracle reads it.
pub fn json_line(text: &str) -> String {
    let text = serde_json::to_string(text).expect("a string encodes");
    text.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029")
}

/// The bits of a double, as the oracle's ops write them.
pub fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

/// Where two texts first differ, with what each holds there.
pub fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(80).min(bytes.len())..(at + 80).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
}

fn code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Unsupported => "unsupported-diagram",
        ErrorCode::Syntax => "syntax",
        ErrorCode::Semantic => "semantic",
        ErrorCode::Limit => "limit",
        _ => "unknown",
    }
}

/// How a front end's inputs compared with Pele's.
#[derive(Default)]
pub struct FrontTally {
    pub identical: usize,
    pub other_types: usize,
    /// Rejections with Pele's error, by its code.
    pub rejected: BTreeMap<String, usize>,
    pub failures: Vec<String>,
    /// The models that held each feature a test names.
    pub held: BTreeMap<&'static str, usize>,
}

/// A feature a model can hold, as what in the model's JSON shows it.
pub type Feature = (&'static str, fn(&Value) -> bool);

impl FrontTally {
    fn compare(
        &mut self,
        name: &str,
        source: &str,
        ours: Result<String, RenderError>,
        theirs: &str,
        features: &[Feature],
    ) {
        let problem = match ours {
            Ok(ours) if ours == theirs => {
                if theirs.starts_with("{\"type\":") {
                    self.other_types += 1;
                } else {
                    self.identical += 1;
                    // A model nested past serde's depth holds no feature a test names.
                    if let Ok(model) = serde_json::from_str::<Value>(theirs) {
                        for (feature, holds) in features {
                            if holds(&model["model"]) {
                                *self.held.entry(feature).or_default() += 1;
                            }
                        }
                    }
                }
                None
            }
            Ok(ours) => Some(first_difference(&ours, theirs)),
            Err(RenderError::Diagnostic(diagnostic)) => {
                let location = diagnostic.location.as_ref();
                let ours = json!({
                    "error": format!("PeleError: {}", diagnostic.message),
                    "code": code_name(diagnostic.code),
                    "type": diagnostic.kind.map(|kind| kind.name()),
                    "mermaidType": diagnostic.mermaid_type.map(|mermaid| mermaid.name()),
                    "line": location.map_or(0, |location| location.line),
                    "column": location.map_or(0, |location| location.column_utf16),
                    "snippet": location.map_or("", |location| location.snippet.as_str()),
                });
                let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
                if ours == theirs {
                    *self.rejected.entry(code_name(diagnostic.code).to_owned()).or_default() += 1;
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Err(error) => Some(format!("ours fails with {error}, pele answers {}", &theirs[..theirs.len().min(200)])),
        };
        if let Some(problem) = problem {
            let shown: String = source.chars().take(600).collect();
            self.failures.push(format!("{name}: {problem}\n    source: {shown:?}"));
        }
    }

    pub fn finish(self, what: &str) -> Self {
        let rejected: usize = self.rejected.values().sum();
        let total = self.identical + rejected + self.other_types + self.failures.len();
        println!(
            "{what}: {total} inputs, {} identical to Pele, {rejected} rejected with Pele's error {:?}, {} of another type, {} different",
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

    pub fn rejected(&self, code: &str) -> usize {
        self.rejected.get(code).copied().unwrap_or(0)
    }

    /// Fails unless some model held each feature, and prints how many did.
    pub fn require(&self, features: &[Feature]) {
        let missed: Vec<&str> =
            features.iter().map(|(name, _)| *name).filter(|name| !self.held.contains_key(name)).collect();
        println!("{} of {} model features held: {:?}", features.len() - missed.len(), features.len(), self.held);
        assert!(missed.is_empty(), "no model holds {missed:?}");
    }
}

/// Compares this port's front end with the oracle's `op` for each source, both with the longest
/// source `limit` allows, or Pele's default limit. A panic in this port is a failure of that
/// source.
pub fn run_front(
    op: &str,
    name: &str,
    sources: &[String],
    limit: Option<usize>,
    port: fn(&str, ParseOptions) -> Result<String, RenderError>,
    features: &[Feature],
) -> FrontTally {
    let given = limit.map_or(String::new(), |limit| format!("\t{limit}"));
    let requests: Vec<String> = sources.iter().map(|source| format!("{op}\t{}{given}", json_line(source))).collect();
    let answers = pele_oracle::ask(&requests);
    let mut options = ParseOptions::default();
    if let Some(limit) = limit {
        options.limits.source_utf16 = limit;
    }
    let mut tally = FrontTally::default();
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let name = format!("{name} {index}");
        match std::panic::catch_unwind(|| port(source, options.clone())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs, features),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

/// This port's render with Pele's render options, which are what `render_with` gives the oracle.
pub fn render_with(source: &str, options: &Value) -> Result<Rendered, RenderError> {
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
            "autoDirection" => layout.auto_direction = flag(value),
            "directionBreakpoint" => layout.direction_breakpoint = Some(number(value)),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
            other => panic!("{other} is not a render option this test maps"),
        }
    }
    pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)
}

/// A drawing path a render can go through, as what in the source and Pele's SVG shows it did.
pub type Path = (&'static str, fn(&str, &str) -> bool);

/// How renders compared with Pele's.
#[derive(Default)]
pub struct RenderTally {
    pub identical: usize,
    pub rejected: usize,
    pub other_types: usize,
    pub failures: Vec<String>,
    /// The drawing paths the identical outputs went through, by name.
    pub reached: BTreeSet<&'static str>,
    /// The identical outputs that differ from the same source drawn with no options.
    pub reacted: usize,
}

impl RenderTally {
    fn compare(
        &mut self,
        kind: &str,
        name: &str,
        source: &str,
        ours: Result<Rendered, RenderError>,
        theirs: &PeleRender,
        paths: &[Path],
    ) {
        let problem = match (theirs, ours) {
            (PeleRender::Svg { r#type, svg, width, height }, Ok(ours)) => {
                if ours.svg != *svg {
                    Some(first_difference(&ours.svg, svg))
                } else if ours.width != *width || ours.height != *height {
                    Some(format!("size {}x{}, pele {width}x{height}", ours.width, ours.height))
                } else {
                    *if r#type == kind { &mut self.identical } else { &mut self.other_types } += 1;
                    self.reached
                        .extend(paths.iter().filter(|(_, reached)| reached(source, svg)).map(|(name, _)| *name));
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != kind => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(other), .. }, Err(RenderError::NotPorted)) if other != kind => {
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
                Some(format!("pele rejects a {kind} diagram, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            let shown: String = source.chars().take(600).collect();
            self.failures.push(format!("{name}: {problem}\n    source: {shown:?}"));
        }
    }

    pub fn finish(self, what: &str) -> Self {
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

    /// Fails unless some identical output reached each path, and prints how many did.
    pub fn require(&self, paths: &[Path]) {
        let missed: Vec<&str> =
            paths.iter().map(|(name, _)| *name).filter(|name| !self.reached.contains(name)).collect();
        println!("reach {} of {} named drawing paths", paths.len() - missed.len(), paths.len());
        assert!(missed.is_empty(), "no output reaches {missed:?}");
    }
}

fn pele_renders(cases: &[(String, Value)]) -> Vec<PeleRender> {
    let requests: Vec<(String, String)> =
        cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
    pele_oracle::render_all_with(&requests)
}

/// Compares this port's render of each case with Pele's. A panic in this port is a failure of
/// that case.
fn compare_all(
    kind: &str,
    name: &str,
    cases: &[(String, Value)],
    theirs: &[PeleRender],
    paths: &[Path],
) -> RenderTally {
    let mut tally = RenderTally::default();
    for (index, ((source, options), theirs)) in cases.iter().zip(theirs).enumerate() {
        let name = format!("{name} {index} with {options}");
        match std::panic::catch_unwind(|| render_with(source, options)) {
            Ok(ours) => tally.compare(kind, &name, source, ours, theirs, paths),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    tally
}

/// Renders each case through both engines.
pub fn run_render(kind: &str, name: &str, cases: &[(String, Value)], paths: &[Path]) -> RenderTally {
    compare_all(kind, name, cases, &pele_renders(cases), paths)
}

/// Renders each source at a width through both engines, and counts the sources that Pele draws
/// another way at that width than with no options.
pub fn run_at_width(kind: &str, name: &str, sources: &[String], width: f64, paths: &[Path]) -> RenderTally {
    let cases: Vec<(String, Value)> =
        sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
    let theirs = pele_renders(&cases);
    let mut tally = compare_all(kind, name, &cases, &theirs, paths);
    for (free, narrow) in pele_oracle::render_all(sources).iter().zip(&theirs) {
        if let (PeleRender::Svg { svg: a, .. }, PeleRender::Svg { svg: b, .. }) = (free, narrow)
            && a != b
        {
            tally.reacted += 1;
        }
    }
    tally
}

/// Each source with no render options.
pub fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

/// The Scene of a source as Pele draws it with the host options given, where it draws.
pub fn scene(source: &str, options: &LayoutOptions) -> Option<pele_core::Scene> {
    let parity = pele_core::parity_scene_with(source, ParseOptions::default(), options).ok()?;
    Some(parity.scene().clone())
}

/// Every drawing's key, failing on one that is drawn twice.
pub fn drawn_keys(scene: &pele_core::Scene, source: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for visit in scene.paint() {
        if let pele_core::scene::PaintVisit::Draw(draw) = visit {
            let key = draw.key.to_string();
            assert!(seen.insert(key.clone()), "{key} is drawn twice in {source:?}");
        }
    }
    seen
}
