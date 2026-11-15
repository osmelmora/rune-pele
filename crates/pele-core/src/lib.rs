//! Rune-free core of rune-pele: Mermaid parsing, layout and scene building, ported from Pele.

mod catalog;
#[doc(hidden)]
pub mod common;
mod diagrams;
mod fit;
mod graph;
mod input;
pub mod js;
// Renderers inside this crate are the callers. It is public so that `tests/layout_replay.rs` can
// hold it to Pele, and hidden because it is no part of the API.
#[doc(hidden)]
pub mod layout;
mod measure;
pub mod scene;
pub mod svg;

pub use catalog::{DiagramKind, MermaidType};
pub use measure::{Font, FontFamily, FontWeight, Script, TextMeasure, TextMeasurer, TextRef, Typography};
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
    /// The most edges a flowchart may have. `A & B --> C & D` multiplies, so a short source can
    /// ask for very many.
    pub max_edges: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { source_utf16: 50_000, svg_utf16: 4_000_000, max_edges: 5_000 }
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
    /// Whether a diagram that runs across may be drawn running down when it does not fit
    /// `max_width`.
    pub auto_direction: bool,
    /// The width under which that is done. Unset, it is 640 pixels.
    pub direction_breakpoint: Option<f64>,
    pub resources: ResourcePolicy,
    /// Pele's `monospace`: the parts of diagrams the host sets in the monospace font.
    pub monospace: Vec<MonospacePart>,
}

/// A part of a diagram the host may set in the monospace font: Pele's `MonospacePart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonospacePart {
    /// A class's attributes and methods.
    Members,
    /// An entity's attributes.
    Attributes,
    Files,
    Messages,
}

/// What a diagram may link to and load. A source cannot change it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourcePolicy {
    /// Whether a node may be a link.
    pub links: bool,
    /// Whether a node may show a picture.
    pub images: bool,
    /// The schemes a link may use besides a relative address.
    pub link_schemes: Vec<String>,
    /// The schemes a picture's address may use besides a relative address.
    pub image_schemes: Vec<String>,
    /// The `rel` of a link. Empty, a link has none.
    pub link_rel: String,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        let schemes = |names: &[&str]| names.iter().map(|&name| name.to_owned()).collect();
        ResourcePolicy {
            links: true,
            images: true,
            link_schemes: schemes(&["http", "https", "mailto", "tel"]),
            image_schemes: schemes(&["http", "https"]),
            link_rel: "noopener".to_owned(),
        }
    }
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            typography: Typography::default(),
            padding: 8.0,
            max_width: None,
            responsive: None,
            id_prefix: None,
            auto_direction: true,
            direction_breakpoint: None,
            resources: ResourcePolicy::default(),
            monospace: Vec::new(),
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
        if catalog::detect(&pre.text).is_some_and(|found| found.kind.keeps_comments()) {
            steps.clear();
            let pre = input::preprocess_keeping_comments(self.source, Some(&mut steps))
                .expect("a source that parsed preprocesses");
            input::encode_entities(&pre.with_comments, Some(&mut steps));
        } else {
            input::encode_entities(&pre.text, Some(&mut steps));
        }
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

/// Pele's `inSource`: the source as the author wrote it, with its line breaks normalized and the
/// line break that parsing appends, and where each offset of the parsed text falls in it.
pub(crate) struct Written {
    source: Vec<u16>,
    steps: input::Steps,
}

impl Written {
    pub(crate) fn source(&self) -> &[u16] {
        &self.source
    }

    /// Pele's `written(src, offset)[1]`, for an offset into the parsed text.
    pub(crate) fn offset(&self, parsed: usize) -> usize {
        input::input_offset(&self.steps, parsed)
    }
}

impl Origin<'_> {
    /// The map from the parsed text back to the written source, for a parser that words its
    /// own positions instead of taking [`Origin::locate`]'s.
    pub(crate) fn written(&self) -> Written {
        let mut steps = input::Steps::new();
        let pre = input::preprocess(self.source, Some(&mut steps)).expect("a source that parsed preprocesses");
        input::encode_entities(&pre.text, Some(&mut steps));
        let mut source = input::normalize_line_breaks(&self.source.encode_utf16().collect::<Vec<_>>());
        source.push(10);
        Written { source, steps }
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

/// What every parser starts from: the detected kind, the text it reads, the title front matter
/// sets and the effective config.
struct Front {
    found: catalog::Detected,
    parsed: Vec<u16>,
    title: Option<input::JsText>,
    config: input::JsObject,
    line_offset: usize,
}

/// Pele's `parseSource` up to the detected kind's parser: limits, preprocessing, detection and
/// the config merge.
fn front(source: &str, options: &ParseOptions) -> Result<Front, RenderError> {
    let limit = options.limits.source_utf16;
    if source.encode_utf16().count() > limit {
        let message = format!("Diagram source is longer than the limit of {limit} characters.");
        return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, message)));
    }
    let pre = input::preprocess(source, None)?;
    let Some(found) = catalog::detect(&pre.text) else {
        let message = "No diagram type detected for the given text.".to_owned();
        return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Unsupported, message)));
    };
    let text = if found.kind.keeps_comments() { &pre.with_comments } else { &pre.text };
    let mut parsed = input::encode_entities(text, None);
    parsed.push(10);
    let config = options.config.value.spread(&pre.config);
    Ok(Front { found, parsed, title: pre.title, config, line_offset: pre.line_offset })
}

impl Front {
    fn context<'a>(&'a self, origin: &'a Origin<'a>, limits: Limits) -> catalog::ParseContext<'a> {
        catalog::ParseContext {
            text: &self.parsed,
            title: self.title.as_ref(),
            origin,
            config: &self.config,
            limits,
            line_offset: self.line_offset,
        }
    }
}

/// Pele's `parseSource`: limits, preprocessing, detection, the config merge and the detected
/// kind's parser. A kind with no port is [`RenderError::NotPorted`].
pub fn prepare(source: &str, options: ParseOptions) -> Result<Prepared, RenderError> {
    let limits = options.limits;
    let front = front(source, &options)?;
    let origin = Origin { source };
    let model = catalog::parse(front.found.kind, &front.context(&origin, limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(Prepared { found: front.found, config: front.config, model, limits })
}

/// The flowchart front end's model and flow graph as JSON, in the words of the oracle's
/// `flow_front` op, for the differential test. A source of another type answers only that type.
#[doc(hidden)]
pub fn flowchart_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Flowchart {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::flowchart::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    let graph = diagrams::flowchart::build_flow_graph(&model, diagrams::flowchart::default_curve(&front.config));
    Ok(diagrams::flowchart::json::front_json(&model, &graph, front.found.mermaid.name()))
}

/// The class diagram front end's model and class graph as JSON, in the words of the oracle's
/// `class_front` op, for the differential test. A source of another type answers only that type.
#[doc(hidden)]
pub fn class_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Class {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::class::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    let graph = diagrams::class::build_class_graph(&model, diagrams::class::hierarchical(&front.config));
    Ok(diagrams::class::json::front_json(&model, &graph, front.found.mermaid.name()))
}

/// The state diagram front end's statement tree, db and graph as JSON, in the words of the
/// oracle's `state_front` op, for the differential test. A source of another type answers only
/// that type.
#[doc(hidden)]
pub fn state_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::State {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::state::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::state::json::front_json(&model))
}

/// The swimlane front end, which is the flowchart's, as JSON in the words of the oracle's
/// `flow_front` op: the model and flow graph, or the error with the flowchart named as its
/// type. A flowchart answers as `flowchart_front_json` does, and a source of another type
/// answers only that type.
#[doc(hidden)]
pub fn swimlane_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    match front.found.kind {
        DiagramKind::Swimlane => {}
        DiagramKind::Flowchart => return flowchart_front_json(source, options),
        other => return Ok(format!("{{\"type\":\"{}\"}}", other.name())),
    }
    let origin = Origin { source };
    let cx = front.context(&origin, options.limits);
    let model =
        <diagrams::swimlane::Swimlane as catalog::Diagram>::parse(&cx).map_err(|error| tagged(error, front.found))?;
    let graph = diagrams::flowchart::build_flow_graph(&model, diagrams::flowchart::default_curve(&front.config));
    Ok(diagrams::flowchart::json::front_json(&model, &graph, front.found.mermaid.name()))
}

/// The requirement front end's model and the graph it lays out as JSON, in the words of the
/// oracle's `requirement_front` op, for the differential test. A source of another type
/// answers only that type.
#[doc(hidden)]
pub fn requirement_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Requirement {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::requirement::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::requirement::json::front_json(&model, front.found.mermaid.name()))
}

/// The use case front end's model and graph as JSON, in the words of the oracle's
/// `usecase_front` op, for the differential test. A source of another type answers only that type.
#[doc(hidden)]
pub fn usecase_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Usecase {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::usecase::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    let graph = diagrams::usecase::build_usecase_graph(&model);
    Ok(diagrams::usecase::view::front_json(&model, &graph, front.found.mermaid.name()))
}

/// The ER front end's model and ER graph as JSON, in the words of the oracle's `er_front` op,
/// for the differential test. A source of another type answers only that type.
#[doc(hidden)]
pub fn er_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Er {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::er::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    let graph = diagrams::er::build_er_graph(&model);
    Ok(diagrams::er::json::front_json(&model, &graph, front.found.mermaid.name()))
}

/// The agentflow front end's model, graph and semantic model as JSON, in the words of the
/// oracle's `agentflow_front` op, for the differential test. A source of another type answers
/// only that type.
#[doc(hidden)]
pub fn agentflow_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Agentflow {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::agentflow::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::agentflow::json::front_json(&model, front.found.mermaid.name()))
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
    prepare(source, ParseOptions::default()).map(|prepared| match prepared.model {
        catalog::PreparedModel::Pie(model) => model,
        _ => panic!("{source:?} is not a pie"),
    })
}

/// A string as a JSON literal with every code unit escaped, for a request to the oracle.
#[cfg(test)]
fn json_string(text: &str) -> String {
    let escaped: String = text.encode_utf16().map(|unit| format!("\\u{unit:04x}")).collect();
    format!("\"{escaped}\"")
}
