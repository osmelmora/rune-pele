//! Asks Pele 1.13.0, built from source by `mise run setup`, what it would output.

use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The workspace root, where `harness/` and `vendor/pele/` live.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Sends one oracle request per element (`<op>\t<arg>...`) and returns one answer per element.
pub fn ask(requests: &[String]) -> Vec<String> {
    let script = root().join("harness/dist/oracle.mjs");
    assert!(script.exists(), "missing {}; run `mise run setup`", script.display());
    let mut child = Command::new("node")
        .arg(&script)
        .env("TZ", "UTC")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("node is on PATH (run under `mise exec` or a mise task)");
    let mut stdin = child.stdin.take().expect("piped stdin");
    let body = requests.join("\n") + "\n";
    let writer = std::thread::spawn(move || stdin.write_all(body.as_bytes()).expect("oracle reads stdin"));
    let output = child.wait_with_output().expect("oracle runs");
    writer.join().expect("writer thread");
    assert!(output.status.success(), "oracle failed: {}", output.status);
    let answers: Vec<String> = String::from_utf8(output.stdout).expect("utf8").lines().map(str::to_owned).collect();
    assert_eq!(answers.len(), requests.len(), "one answer per request");
    answers
}

/// What Pele did with one Mermaid source.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PeleRender {
    Svg { r#type: String, svg: String, width: f64, height: f64 },
    Error { r#type: Option<String>, error: String },
}

impl PeleRender {
    pub fn diagram_type(&self) -> Option<&str> {
        match self {
            PeleRender::Svg { r#type, .. } => Some(r#type),
            PeleRender::Error { r#type, .. } => r#type.as_deref(),
        }
    }
}

/// A source as one JSON string on one line. The oracle reads requests line by line, and Node
/// ends a line at U+2028 and U+2029 as well, which JSON leaves unescaped.
fn json(source: &str) -> String {
    serde_json::to_string(source).expect("string encodes").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029")
}

/// Renders every source through Pele with the corpus test's pinned measurer and clock.
pub fn render_all(sources: &[String]) -> Vec<PeleRender> {
    let requests: Vec<String> = sources.iter().map(|s| format!("render\t{}", json(s))).collect();
    ask(&requests).iter().map(|line| serde_json::from_str(line).expect("oracle answers JSON")).collect()
}

/// Renders each source through Pele with the JSON object of render options beside it laid over
/// the pinned measurer and clock.
pub fn render_all_with(cases: &[(String, String)]) -> Vec<PeleRender> {
    let requests: Vec<String> =
        cases.iter().map(|(source, options)| format!("render_with\t{}\t{options}", json(source))).collect();
    ask(&requests).iter().map(|line| serde_json::from_str(line).expect("oracle answers JSON")).collect()
}

/// Pele's documentation corpus: `(type, sources)` per `tests/corpus/<type>-docs.json`, sorted by type.
pub fn corpus() -> Vec<(String, Vec<String>)> {
    let dir = root().join("vendor/pele/tests/corpus");
    let mut out: Vec<(String, Vec<String>)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}; run `mise run setup`", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?.strip_suffix("-docs.json")?.to_owned();
            let sources = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
            Some((name, sources))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
