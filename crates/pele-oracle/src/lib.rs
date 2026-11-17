//! Asks Pele 1.13.0, built from source by `mise run setup`, what it would output.

use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// The workspace root, where `harness/` and `vendor/pele/` live.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Why the oracle gave no answer: it is not built, its Node is not the pinned one, or it failed.
#[derive(Debug, Clone)]
pub struct OracleError(String);

impl std::fmt::Display for OracleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OracleError {}

/// The Node executable the oracle runs.
#[derive(Debug)]
pub struct Node {
    pub path: PathBuf,
    /// As `node --version` prints it, such as `v26.13.1`.
    pub version: String,
}

/// The Node that `mise which node` names in the workspace, which is the one the doctor checks,
/// once its version is the one `mise.toml` pins. The oracle never looks `node` up on `PATH`,
/// where another Node can stand first even under `mise exec`.
pub fn node() -> Result<&'static Node, OracleError> {
    static NODE: OnceLock<Result<Node, OracleError>> = OnceLock::new();
    NODE.get_or_init(resolve_node).as_ref().map_err(Clone::clone)
}

fn resolve_node() -> Result<Node, OracleError> {
    let which = Command::new("mise").args(["which", "node"]).current_dir(root()).output().map_err(|e| {
        OracleError(format!("cannot run `mise which node`: {e}; the oracle runs the Node mise.toml pins"))
    })?;
    if !which.status.success() {
        let why = String::from_utf8_lossy(&which.stderr);
        return Err(OracleError(format!("`mise which node` failed: {}; run `mise install`", why.trim())));
    }
    let path = PathBuf::from(String::from_utf8_lossy(&which.stdout).trim());
    let version = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|e| OracleError(format!("cannot run {} --version: {e}", path.display())))?;
    let version = String::from_utf8_lossy(&version.stdout).trim().to_owned();
    let mise = root().join("mise.toml");
    let pinned = std::fs::read_to_string(&mise)
        .ok()
        .and_then(|toml| {
            toml.lines().find_map(|line| Some(line.strip_prefix("node = \"")?.strip_suffix('"')?.to_owned()))
        })
        .ok_or_else(|| OracleError(format!("{} pins no `node = \"<version>\"`", mise.display())))?;
    if version != format!("v{pinned}") {
        return Err(OracleError(format!("node at {} is {version}, mise.toml pins v{pinned}", path.display())));
    }
    Ok(Node { path, version })
}

/// Sends one oracle request per element (`<op>\t<arg>...`) and returns one answer per element.
/// Panics with the reason when the oracle cannot answer; [`try_ask`] returns it instead.
pub fn ask(requests: &[String]) -> Vec<String> {
    try_ask(requests).unwrap_or_else(|error| panic!("{error}"))
}

/// Sends one oracle request per element and returns one answer per element, or why the oracle
/// could not answer. Node's own report of a crash goes to this process's stderr.
pub fn try_ask(requests: &[String]) -> Result<Vec<String>, OracleError> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let script = root().join("harness/dist/oracle.mjs");
    if !script.exists() {
        return Err(OracleError(format!("missing {}; run `mise run setup`", script.display())));
    }
    let node = node()?;
    let mut child = Command::new(&node.path)
        .arg(&script)
        .env("TZ", "UTC")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| OracleError(format!("cannot start {}: {e}", node.path.display())))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let body = requests.join("\n") + "\n";
    let writer = std::thread::spawn(move || stdin.write_all(body.as_bytes()));
    let output = child.wait_with_output().map_err(|e| OracleError(format!("the oracle did not finish: {e}")))?;
    let written = writer.join().expect("the writer thread does not panic");
    if !output.status.success() {
        return Err(OracleError(format!("the oracle failed ({}); Node's report is above", output.status)));
    }
    written.map_err(|e| OracleError(format!("the oracle stopped reading its requests: {e}")))?;
    let answers: Vec<String> = String::from_utf8(output.stdout)
        .map_err(|e| OracleError(format!("the oracle answered text that is not UTF-8: {e}")))?
        .lines()
        .map(str::to_owned)
        .collect();
    if answers.len() != requests.len() {
        return Err(OracleError(format!("the oracle answered {} lines to {} requests", answers.len(), requests.len())));
    }
    Ok(answers)
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
    try_render_all(sources).unwrap_or_else(|error| panic!("{error}"))
}

/// Renders every source through Pele with the corpus test's pinned measurer and clock, or says
/// why the oracle could not.
pub fn try_render_all(sources: &[String]) -> Result<Vec<PeleRender>, OracleError> {
    let requests: Vec<String> = sources.iter().map(|s| format!("render\t{}", json(s))).collect();
    try_ask(&requests)?
        .iter()
        .map(|line| {
            serde_json::from_str(line).map_err(|e| {
                let start: String = line.chars().take(200).collect();
                OracleError(format!("the oracle answered something other than a render ({e}): {start}"))
            })
        })
        .collect()
}

/// Renders each source through Pele with the JSON object of render options beside it laid over
/// the pinned measurer and clock.
pub fn render_all_with(cases: &[(String, String)]) -> Vec<PeleRender> {
    let requests: Vec<String> =
        cases.iter().map(|(source, options)| format!("render_with\t{}\t{options}", json(source))).collect();
    ask(&requests).iter().map(|line| serde_json::from_str(line).expect("oracle answers JSON")).collect()
}

/// Why the corpus could not be read whole.
#[derive(Debug, Clone)]
pub struct CorpusError(String);

impl std::fmt::Display for CorpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CorpusError {}

/// Where the pinned checkout keeps Pele's documentation corpus.
pub fn corpus_dir() -> PathBuf {
    root().join("vendor/pele/tests/corpus")
}

/// The types of Pele's documentation corpus, sorted: one per `tests/corpus/<type>-docs.json` in
/// the tree of the commit `scripts/pele-pin.txt` names. They come from git, not from the files on
/// disk, so a file that is gone from the checkout is still expected.
pub fn corpus_types() -> Result<Vec<String>, CorpusError> {
    let pin_file = root().join("scripts/pele-pin.txt");
    let pin = std::fs::read_to_string(&pin_file).map_err(|e| CorpusError(format!("{}: {e}", pin_file.display())))?;
    let pin = pin.trim();
    let listed = Command::new("git")
        .arg("-C")
        .arg(root().join("vendor/pele"))
        .args(["ls-tree", "--name-only", pin, "tests/corpus/"])
        .output()
        .map_err(|e| CorpusError(format!("cannot run git to list Pele's corpus: {e}")))?;
    if !listed.status.success() {
        let why = String::from_utf8_lossy(&listed.stderr);
        return Err(CorpusError(format!("cannot list Pele's corpus at {pin}: {}; run `mise run setup`", why.trim())));
    }
    let mut types: Vec<String> = String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|path| Some(path.strip_prefix("tests/corpus/")?.strip_suffix("-docs.json")?.to_owned()))
        .collect();
    types.sort();
    if types.is_empty() {
        return Err(CorpusError(format!("Pele at {pin} holds no tests/corpus/<type>-docs.json")));
    }
    Ok(types)
}

/// Reads every file of the pinned corpus ([`corpus_types`]) from `dir`: `(type, sources)`,
/// sorted by type. Each file must be there and be a JSON array of at least one string.
pub fn load_corpus(dir: &Path) -> Result<Vec<(String, Vec<String>)>, CorpusError> {
    corpus_types()?
        .into_iter()
        .map(|name| {
            let path = dir.join(format!("{name}-docs.json"));
            let bytes = std::fs::read(&path).map_err(|e| CorpusError(format!("{}: {e}", path.display())))?;
            let sources: Vec<String> = serde_json::from_slice(&bytes)
                .map_err(|e| CorpusError(format!("{}: not a JSON array of sources: {e}", path.display())))?;
            if sources.is_empty() {
                return Err(CorpusError(format!("{}: holds no sources", path.display())));
            }
            Ok((name, sources))
        })
        .collect()
}

/// Pele's documentation corpus: `(type, sources)` per `tests/corpus/<type>-docs.json`, sorted by
/// type. Panics naming the file when one is missing or broken; [`load_corpus`] returns that instead.
pub fn corpus() -> Vec<(String, Vec<String>)> {
    load_corpus(&corpus_dir()).unwrap_or_else(|error| panic!("{error}"))
}
