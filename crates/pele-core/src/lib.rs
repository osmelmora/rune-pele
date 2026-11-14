//! Rune-free core of rune-pele: Mermaid parsing, layout and scene building, ported from Pele.

pub mod js;

/// A diagram rendered to SVG.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub svg: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    /// No diagram type has been ported yet that accepts this source.
    NotPorted,
}

/// Renders Mermaid source with Pele's parity inputs: Arial metric tables and no prior layout.
pub fn render(_source: &str) -> Result<Rendered, RenderError> {
    Err(RenderError::NotPorted)
}
