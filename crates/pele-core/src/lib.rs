//! Rune-free core of rune-pele: Mermaid parsing, layout and scene building, ported from Pele.

mod catalog;
mod input;
pub mod js;

pub use catalog::{DiagramKind, MermaidType};

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
        Diagnostic { code, kind: None, mermaid_type: None, message }
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

/// Renders Mermaid source with Pele's parity inputs: Arial metric tables and no prior layout.
pub fn render(source: &str) -> Result<Rendered, RenderError> {
    let limit = Limits::default().source_utf16;
    if source.encode_utf16().count() > limit {
        let message = format!("Diagram source is longer than the limit of {limit} characters.");
        return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, message)));
    }
    let pre = input::preprocess(source)?;
    let Some(_found) = catalog::detect(&pre.text) else {
        let message = "No diagram type detected for the given text.".to_owned();
        return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Unsupported, message)));
    };
    Err(RenderError::NotPorted)
}
