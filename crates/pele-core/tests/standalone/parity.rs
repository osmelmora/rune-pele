//! What the render tests of eventmodeling, treeview and radar share: Pele's render options mapped
//! onto this port's, both engines run over a list of cases, and a tally of what came out alike,
//! with the drawing paths the identical outputs went through.

use pele_core::{HostConfig, LayoutOptions, MonospacePart, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// One list of a type's harvest.
pub fn harvest(kind: &str, list: &str) -> Vec<String> {
    let path = pele_oracle::root().join(format!("tests/fixtures/{kind}/harvest.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
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
            "monospace" => {
                layout.monospace = value
                    .as_array()
                    .expect("a list")
                    .iter()
                    .map(|part| match part.as_str().expect("a string") {
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

/// An output both engines drew alike, for a drawing path to recognise itself in.
pub struct Case<'a> {
    pub source: &'a str,
    pub svg: &'a str,
}

pub type Reached = fn(&Case) -> bool;

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
pub struct Tally {
    pub identical: usize,
    pub rejected: usize,
    pub other_types: usize,
    pub failures: Vec<String>,
    /// The drawing paths that the identical outputs went through, by name.
    pub reached: BTreeSet<&'static str>,
}

/// One diagram type's render test: its name as Pele reports it and its drawing paths.
pub struct Parity {
    pub kind: &'static str,
    pub paths: &'static [(&'static str, Reached)],
}

impl Parity {
    fn compare(
        &self,
        tally: &mut Tally,
        name: &str,
        source: &str,
        ours: Result<Rendered, RenderError>,
        theirs: &PeleRender,
    ) {
        let kind = self.kind;
        let problem = match (theirs, ours) {
            (PeleRender::Svg { r#type, svg, width, height }, Ok(ours)) => {
                if ours.svg != *svg {
                    Some(first_difference(&ours.svg, svg))
                } else if ours.width != *width || ours.height != *height {
                    Some(format!("size {}x{}, pele {width}x{height}", ours.width, ours.height))
                } else if r#type == kind {
                    tally.identical += 1;
                    let case = Case { source, svg };
                    tally
                        .reached
                        .extend(self.paths.iter().filter(|(_, reached)| reached(&case)).map(|(name, _)| *name));
                    None
                } else {
                    tally.other_types += 1;
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != kind => {
                tally.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(other), .. }, Err(RenderError::NotPorted)) if other != kind => {
                tally.other_types += 1;
                None
            }
            (PeleRender::Error { error, .. }, Err(RenderError::Diagnostic(ours))) => {
                let ours = format!("PeleError: {}", ours.message);
                if ours == *error {
                    tally.rejected += 1;
                    None
                } else {
                    Some(format!("rejected with another message\n    ours: {ours:?}\n    pele: {error:?}"))
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(error)) => Some(format!("pele renders a {type}, ours fails: {error}")),
            (PeleRender::Error { error, .. }, Ok(_)) => Some(format!("pele rejects it, ours renders: {error}")),
            (PeleRender::Error { error, .. }, Err(_)) => {
                Some(format!("pele rejects it, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            tally
                .failures
                .push(format!("{name}: {problem}\n    source: {:?}", source.chars().take(600).collect::<String>()));
        }
    }

    /// Renders each case through both engines. A panic in this port is a failure of that case.
    pub fn run(&self, name: &str, cases: &[(String, Value)]) -> Tally {
        let requests: Vec<(String, String)> =
            cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
        let mut tally = Tally::default();
        for (index, ((source, options), theirs)) in
            cases.iter().zip(pele_oracle::render_all_with(&requests)).enumerate()
        {
            let name = format!("{name} {index} with {options}");
            match std::panic::catch_unwind(|| render_with(source, options)) {
                Ok(ours) => self.compare(&mut tally, &name, source, ours, &theirs),
                Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
            }
        }
        tally
    }

    /// The paths no identical output went through.
    pub fn missed(&self, tally: &Tally) -> Vec<&'static str> {
        self.paths.iter().map(|(name, _)| *name).filter(|name| !tally.reached.contains(name)).collect()
    }
}

impl Tally {
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
}

pub fn plain(sources: Vec<String>) -> Vec<(String, Value)> {
    sources.into_iter().map(|source| (source, json!({}))).collect()
}

/// The widths a host may have.
pub const WIDTHS: [f64; 4] = [320.0, 480.0, 640.0, 900.0];

/// How many of the cases Pele draws another way for the width they give than with room to spare.
pub fn redrawn_to_fit(cases: &[(String, Value)]) -> usize {
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

/// The text of every element that starts with `open`, up to the end of that element's tag.
pub fn tags<'a>(svg: &'a str, open: &str) -> Vec<&'a str> {
    svg.match_indices(open).map(|(at, _)| &svg[at..at + svg[at..].find('>').unwrap_or(0)]).collect()
}

/// The value of an attribute in a tag.
pub fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    Some(&tag[at..at + tag[at..].find('"')?])
}
