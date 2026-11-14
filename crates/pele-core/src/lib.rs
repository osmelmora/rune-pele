//! Rune-free core of rune-pele: Mermaid parsing, layout and scene building, ported from Pele.

mod catalog;
mod diagrams;
mod input;
pub mod js;
mod measure;

pub use catalog::{DiagramKind, MermaidType};
pub use measure::{Font, FontFamily, FontWeight, TextMeasure, TextMeasurer, TextRef, Typography};

/// A diagram rendered to SVG.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub svg: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    /// The source is a diagram type this port does not render yet.
    NotPorted,
    /// The source or a limit is at fault, as Pele's `PeleError` reports it.
    Diagnostic(Diagnostic),
}

/// Why a source was rejected. `message` is Pele's wording.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub code: ErrorCode,
    pub kind: Option<DiagramKind>,
    pub mermaid_type: Option<MermaidType>,
    pub message: String,
    /// Where in the source a syntax error is.
    pub location: Option<Location>,
}

/// A position in the source as the author wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Counted from 1.
    pub line: usize,
    /// Counted from 1, in UTF-16 code units.
    pub column_utf16: usize,
    /// The line, then a line of dashes ending in a caret under the column.
    pub snippet: String,
}

/// Pele's `PeleErrorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCode {
    Unsupported,
    Syntax,
    Semantic,
    Limit,
}

impl Diagnostic {
    pub(crate) fn new(code: ErrorCode, message: String) -> Self {
        Diagnostic { code, kind: None, mermaid_type: None, message, location: None }
    }
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::NotPorted => f.write_str("this diagram type is not ported yet"),
            RenderError::Diagnostic(diagnostic) => f.write_str(&diagnostic.message),
        }
    }
}

impl std::error::Error for RenderError {}

/// Limits the host sets. A source cannot change them. The defaults are Pele's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The longest source, in UTF-16 code units.
    pub source_utf16: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { source_utf16: 50_000 }
    }
}

/// Maps a position in the text a parser reads back to the source the author wrote.
pub(crate) struct Origin<'a> {
    source: &'a str,
}

impl Origin<'_> {
    /// Pele's `locate`, for an offset into `parsed`.
    pub(crate) fn locate(&self, parsed: &[u16], offset: usize) -> Location {
        use input::Units;
        // Keep EOF errors on the last content line.
        let end = parsed.trim_end_js().len();
        let (parsed_offset, past) = if end > 0 && offset >= end { (end - 1, 1) } else { (offset, 0) };
        let mut steps = input::Steps::new();
        let pre = input::preprocess(self.source, Some(&mut steps)).expect("a source that parsed preprocesses");
        input::encode_entities(&pre.text, Some(&mut steps));
        let mut source = input::normalize_line_breaks(&self.source.encode_utf16().collect::<Vec<_>>());
        source.push(10);
        let offset = input::input_offset(&steps, parsed_offset) + past;

        let line_start = source[..offset.min(source.len())].iter().rposition(|&c| c == 10).map_or(0, |at| at + 1);
        let line_end = source.find_unit(10, offset).unwrap_or(source.len());
        let column = offset - line_start + 1;
        let text = String::from_utf16_lossy(&source[line_start..line_end.max(line_start)]);
        Location {
            line: 1 + source[..line_start].iter().filter(|&&c| c == 10).count(),
            column_utf16: column,
            snippet: format!("{text}\n{}^", "-".repeat(column - 1)),
        }
    }
}

/// Pele's `tagErrors`: an error names the diagram it is in, and shows entity codes as written.
fn tagged(error: RenderError, found: catalog::Detected) -> RenderError {
    let written =
        |text: &str| String::from_utf16_lossy(&input::written_codes(&text.encode_utf16().collect::<Vec<_>>()));
    match error {
        RenderError::Diagnostic(diagnostic) => RenderError::Diagnostic(Diagnostic {
            kind: diagnostic.kind.or(Some(found.kind)),
            mermaid_type: diagnostic.mermaid_type.or(Some(found.mermaid)),
            message: written(&diagnostic.message),
            location: diagnostic.location.map(|location| Location { snippet: written(&location.snippet), ..location }),
            ..diagnostic
        }),
        other => other,
    }
}

/// Pele's `parseSource`: limits, preprocessing, detection and the detected kind's parser.
fn parse_source(source: &str, limits: Limits) -> Result<catalog::PreparedModel, RenderError> {
    let limit = limits.source_utf16;
    if source.encode_utf16().count() > limit {
        let message = format!("Diagram source is longer than the limit of {limit} characters.");
        return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, message)));
    }
    let pre = input::preprocess(source, None)?;
    let Some(found) = catalog::detect(&pre.text) else {
        let message = "No diagram type detected for the given text.".to_owned();
        return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Unsupported, message)));
    };
    let mut parsed = input::encode_entities(&pre.text, None);
    parsed.push(10);
    let cx = catalog::ParseContext { text: &parsed, title: pre.title.as_ref(), origin: &Origin { source } };
    catalog::parse(found.kind, &cx).map_err(|error| tagged(error, found))
}

/// Renders Mermaid source with Pele's parity inputs: Arial metric tables and no prior layout.
pub fn render(source: &str) -> Result<Rendered, RenderError> {
    parse_source(source, Limits::default())?;
    Err(RenderError::NotPorted)
}

#[cfg(test)]
fn prepare_pie_for_test(source: &str) -> Result<diagrams::pie::PieModel, RenderError> {
    parse_source(source, Limits::default()).map(|catalog::PreparedModel::Pie(model)| model)
}

/// A string as a JSON literal with every code unit escaped, for a request to the oracle.
#[cfg(test)]
fn json_string(text: &str) -> String {
    let escaped: String = text.encode_utf16().map(|unit| format!("\\u{unit:04x}")).collect();
    format!("\"{escaped}\"")
}
