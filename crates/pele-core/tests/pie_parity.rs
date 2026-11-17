//! Differential test for pie: every source must come out of this port as it comes out of Pele
//! 1.13.0, byte for byte, and a source Pele rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/pie/harvest.json` holds the pie sources of Pele's own test suite, written by
//! `scripts/harvest-pie.mjs`. `tests/fixtures/pie/cases.json` holds this port's own cases, some
//! with Pele render options.

use pele_core::{HostConfig, LayoutOptions, ParseOptions, RenderError, Rendered};
use pele_oracle::PeleRender;
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = pele_oracle::root().join("tests/fixtures/pie").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn strings(list: &Value) -> Vec<String> {
    list.as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// This port's render with Pele's render options, which are what `render_with` gives the oracle.
fn render_with(source: &str, options: &Value) -> Result<Rendered, RenderError> {
    let mut parse = ParseOptions::default();
    let mut layout = LayoutOptions::default();
    let number = |value: &Value| value.as_f64().expect("a number");
    let count = |value: &Value| usize::try_from(value.as_u64().expect("a count")).expect("a count");
    for (name, value) in options.as_object().expect("options are an object") {
        match name.as_str() {
            "maxWidth" => layout.max_width = Some(number(value)),
            "padding" => layout.padding = Some(number(value)),
            "fontSize" => layout.typography.normal.size = number(value),
            "idPrefix" => layout.id_prefix = Some(value.as_str().expect("a string").to_owned()),
            "responsive" => layout.responsive = Some(value.as_bool().expect("a boolean")),
            "config" => parse.config = HostConfig::from_init_json(&value.to_string())?,
            "limit" => parse.limits.source_utf16 = count(value),
            "outputLimit" => parse.limits.svg_utf16 = count(value),
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
}

/// Where two texts first differ, with what each holds there.
fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(60).min(bytes.len())..(at + 60).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
}

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<Rendered, RenderError>, theirs: &PeleRender) {
        let problem = match (theirs, ours) {
            (PeleRender::Svg { r#type, svg, width, height }, Ok(ours)) if r#type == "pie" => {
                if ours.svg != *svg {
                    Some(first_difference(&ours.svg, svg))
                } else if ours.width != *width || ours.height != *height {
                    Some(format!("size {}x{}, pele {width}x{height}", ours.width, ours.height))
                } else {
                    self.identical += 1;
                    None
                }
            }
            (PeleRender::Svg { r#type, .. }, Err(RenderError::NotPorted)) if r#type != "pie" => {
                self.other_types += 1;
                None
            }
            (PeleRender::Error { r#type: Some(kind), .. }, Err(RenderError::NotPorted)) if kind != "pie" => {
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
            (PeleRender::Svg { r#type, .. }, Ok(_)) => Some(format!("pele renders a {type}, ours a pie")),
            (PeleRender::Error { error, .. }, Ok(_)) => Some(format!("pele rejects it, ours renders: {error}")),
            (PeleRender::Error { error, .. }, Err(RenderError::NotPorted)) => {
                Some(format!("pele rejects a pie, ours reports it as not ported: {error}"))
            }
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {source:?}"));
        }
    }

    fn finish(self, what: &str) {
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
            self.failures[..self.failures.len().min(25)].join("\n")
        );
    }
}

fn harvested(list: &str) -> Tally {
    let sources = strings(&fixture("harvest.json")[list]);
    let mut tally = Tally::default();
    for (index, (source, theirs)) in sources.iter().zip(pele_oracle::render_all(&sources)).enumerate() {
        tally.compare(&format!("{list} {index}"), source, pele_core::render(source), &theirs);
    }
    tally
}

#[test]
fn harvested_sources_match_pele() {
    let tally = harvested("sources");
    assert!(tally.identical > 100, "the harvest holds sources Pele renders");
    assert!(tally.rejected > 20, "the harvest holds sources Pele rejects");
    tally.finish("harvested pie sources");
}

#[test]
fn mutated_sources_match_pele() {
    harvested("mutated").finish("mutated pie sources");
}

#[test]
fn cases_match_pele() {
    let cases = fixture("cases.json");
    let cases: Vec<(String, String, Value)> = cases
        .as_array()
        .expect("a list of cases")
        .iter()
        .map(|case| {
            let text = |field: &str| case[field].as_str().unwrap_or_else(|| panic!("a case has a {field}")).to_owned();
            (text("name"), text("source"), case.get("options").cloned().unwrap_or_else(|| serde_json::json!({})))
        })
        .collect();
    let requests: Vec<(String, String)> =
        cases.iter().map(|(_, source, options)| (source.clone(), options.to_string())).collect();
    let mut tally = Tally::default();
    for ((name, source, options), theirs) in cases.iter().zip(pele_oracle::render_all_with(&requests)) {
        tally.compare(name, source, render_with(source, options), &theirs);
    }
    tally.finish("pie cases");
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len() as u64) as usize]
    }
}

/// A pie with random slice values, config and host options, to put many angles and widths
/// through the trigonometry, the rounding and the width attempts.
fn random_pie(rng: &mut Rng) -> (String, Value) {
    let mut source = String::new();
    let mut config = String::new();
    if rng.below(3) > 0 {
        config += &format!("    textPosition: 0.{:04}\n", rng.below(10_000));
    }
    if rng.below(3) == 0 {
        config += &format!("    donutHole: 0.{:03}\n", rng.below(900));
    }
    if rng.below(2) == 0 {
        config +=
            &format!("    legendPosition: {}\n", rng.pick(&["left", "right", "center", "top", "bottom", "nowhere"]));
    }
    if !config.is_empty() {
        source += &format!("---\nconfig:\n  pie:\n{config}---\n");
    }
    source += "pie";
    if rng.below(4) == 0 {
        source += " showData";
    }
    if rng.below(2) == 0 {
        let words = 1 + rng.below(9);
        let title: Vec<&str> =
            (0..words).map(|_| rng.pick(&["Share", "of", "the", "market", "by", "region", "2026"])).collect();
        source += &format!(" title {}", title.join(" "));
    }
    source += "\n";
    let most = if rng.below(5) == 0 { 60 } else { 9 };
    let sections = 1 + rng.below(most);
    for index in 0..sections {
        let value = match rng.below(4) {
            0 => format!("{}", rng.below(1000)),
            1 => format!("{}.{:02}", rng.below(100), rng.below(100)),
            2 => format!("{}.{}", rng.below(10), rng.below(1000)),
            _ => format!("{}", 1 + rng.below(12)),
        };
        let label = rng.pick(&["S", "Section ", "A longer label ", "<b>B</b>", "x"]);
        source += &format!("  \"{label}{index}\" : {value}\n");
    }

    let mut options = serde_json::Map::new();
    if rng.below(3) > 0 {
        let width = 60.0 + rng.below(480) as f64 + if rng.below(4) == 0 { 0.5 } else { 0.0 };
        options.insert("maxWidth".to_owned(), width.into());
    }
    if rng.below(4) == 0 {
        options.insert("padding".to_owned(), (rng.below(41) as f64 / 2.0).into());
    }
    if rng.below(4) == 0 {
        options.insert("fontSize".to_owned(), [12.0, 13.0, 14.0, 18.5, 20.0][rng.below(5) as usize].into());
    }
    (source, Value::Object(options))
}

#[test]
fn random_pies_match_pele() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let cases: Vec<(String, Value)> = (0..3000).map(|_| random_pie(&mut rng)).collect();
    let requests: Vec<(String, String)> =
        cases.iter().map(|(source, options)| (source.clone(), options.to_string())).collect();
    let mut tally = Tally::default();
    for ((source, options), theirs) in cases.iter().zip(pele_oracle::render_all_with(&requests)) {
        tally.compare(&format!("random pie with {options}"), source, render_with(source, options), &theirs);
    }
    assert!(tally.identical > 2500, "nearly every random pie renders");
    tally.finish("random pies");
}
