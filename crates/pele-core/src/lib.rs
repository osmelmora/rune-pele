//! Rune-free core of rune-pele: Mermaid parsing, layout and scene building, ported from Pele.

mod catalog;
mod diagrams;
mod fit;
mod input;
pub mod js;
mod measure;
pub mod scene;
pub mod svg;

pub use catalog::{DiagramKind, MermaidType};
pub use measure::{Font, FontFamily, FontWeight, TextMeasure, TextMeasurer, TextRef, Typography};
pub use scene::Scene;

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
    /// The longest SVG, in UTF-16 code units.
    pub svg_utf16: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { source_utf16: 50_000, svg_utf16: 4_000_000 }
    }
}

/// Mermaid config the host supplies, as Pele's `config` option. A source's own config is laid
/// over it one top-level section at a time.
#[derive(Debug, Clone, Default)]
pub struct HostConfig {
    value: input::JsObject,
}

impl HostConfig {
    /// Reads a config from the JSON an `init` directive would hold.
    pub fn from_init_json(text: &str) -> Result<Self, RenderError> {
        match input::parse_json(&text.encode_utf16().collect::<Vec<_>>()) {
            Ok(input::JsValue::Object(value)) => Ok(HostConfig { value }),
            Err(input::JsonError::TooDeep) => Err(input::nested_too_deeply()),
            Ok(_) | Err(input::JsonError::Invalid) => {
                let message = "The host config is not a JSON object.".to_owned();
                Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Syntax, message)))
            }
        }
    }
}

/// What parsing reads from the host. A render that is given a parsed source has nothing left to
/// do with these.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub config: HostConfig,
    pub limits: Limits,
}

/// How the host wants a diagram laid out. Colours are absent: changing them cannot change a layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
    pub typography: Typography,
    /// The space around the drawing, in pixels.
    pub padding: f64,
    /// The width the host has for the diagram, in pixels. A type that can draw itself narrower
    /// does, so that its text is not shrunk with it.
    pub max_width: Option<f64>,
    /// Whether the drawing scales down with its container. Unset, the source's `useMaxWidth` decides.
    pub responsive: Option<bool>,
    /// The prefix of the ids a drawing declares. Unset, it is `pele`.
    pub id_prefix: Option<String>,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            typography: Typography::default(),
            padding: 8.0,
            max_width: None,
            responsive: None,
            id_prefix: None,
        }
    }
}

/// What drawing reads from the host.
#[derive(Clone, Copy)]
pub struct NativeInputs<'a> {
    pub measure: &'a dyn TextMeasurer,
    pub options: &'a LayoutOptions,
}

/// A parsed source with its effective config. Drawing it again needs no parsing.
pub struct Prepared {
    found: catalog::Detected,
    config: input::JsObject,
    model: catalog::PreparedModel,
    limits: Limits,
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

/// Pele's `parseSource`: limits, preprocessing, detection, the config merge and the detected
/// kind's parser. A kind with no port is [`RenderError::NotPorted`].
pub fn prepare(source: &str, options: ParseOptions) -> Result<Prepared, RenderError> {
    let limits = options.limits;
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
    let model = catalog::parse(found.kind, &cx).map_err(|error| tagged(error, found))?;
    Ok(Prepared { found, config: options.config.value.spread(&pre.config), model, limits })
}

impl Prepared {
    pub fn kind(&self) -> DiagramKind {
        self.found.kind
    }

    /// Draws the parsed source. The model is not changed, so one `Prepared` can be drawn for
    /// several widths.
    pub fn scene(&self, inputs: NativeInputs<'_>) -> Result<Scene, RenderError> {
        let policy = self.found.kind.port().expect("a source is prepared only for a ported kind");
        let section = self.config.get(policy.section).and_then(input::JsValue::as_record);
        let fixed = section.and_then(|section| section.get("useMaxWidth")) == Some(&input::JsValue::Bool(false));
        let cx = catalog::DrawContext {
            kind: self.found.kind,
            measure: inputs.measure,
            options: inputs.options,
            config: &self.config,
            palette: &scene::SourcePalette::from_config(policy.palette, &self.config),
            responsive: inputs.options.responsive.unwrap_or(!fixed),
            limits: self.limits,
        };
        catalog::scene(&self.model, &cx).map_err(|error| tagged(error, self.found))
    }
}

/// A Scene drawn with Pele's own inputs: Arial metric tables and no prior layout. Only this
/// crate can make one, so the SVG writer never sees a drawing that Pele could not have made.
pub struct ParityScene {
    scene: Scene,
}

impl ParityScene {
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
}

/// Draws a source as Pele does with its default options.
pub fn parity_scene(source: &str) -> Result<ParityScene, RenderError> {
    parity_scene_with(source, ParseOptions::default(), &LayoutOptions::default())
}

/// Draws a source as Pele does with the host options given.
pub fn parity_scene_with(
    source: &str,
    parse: ParseOptions,
    layout: &LayoutOptions,
) -> Result<ParityScene, RenderError> {
    let prepared = prepare(source, parse)?;
    let scene = prepared.scene(NativeInputs { measure: &measure::ArialMetrics, options: layout })?;
    Ok(ParityScene { scene })
}

/// Renders Mermaid source with Pele's parity inputs: Arial metric tables and no prior layout.
pub fn render(source: &str) -> Result<Rendered, RenderError> {
    svg::write(&parity_scene(source)?)
}

#[cfg(test)]
fn prepare_pie_for_test(source: &str) -> Result<diagrams::pie::PieModel, RenderError> {
    prepare(source, ParseOptions::default()).map(|prepared| {
        let catalog::PreparedModel::Pie(model) = prepared.model;
        model
    })
}

/// A string as a JSON literal with every code unit escaped, for a request to the oracle.
#[cfg(test)]
fn json_string(text: &str) -> String {
    let escaped: String = text.encode_utf16().map(|unit| format!("\\u{unit:04x}")).collect();
    format!("\"{escaped}\"")
}
