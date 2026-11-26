//! Rune-free core of rune-pele: Mermaid parsing, layout and scene building, ported from Pele.
//!
//! [`render`] draws Mermaid source as the SVG Pele 1.13.0 writes, byte for byte, with Pele's
//! default options and its own Arial metrics.
//!
//! ```
//! let rendered = pele_core::render("flowchart LR\n  A[Write] --> B[Draw]\n")?;
//! assert!(rendered.svg.starts_with("<svg"));
//! assert!(rendered.width > rendered.height);
//! # Ok::<(), pele_core::RenderError>(())
//! ```
//!
//! The host's options are Pele's render options: [`ParseOptions`] for what parsing reads and
//! [`LayoutOptions`] for the layout. [`parity_scene_with`] lays a source out with them, and
//! [`svg::write`] writes it.
//!
//! ```
//! use pele_core::{LayoutOptions, Limits, ParseOptions};
//!
//! let source = "flowchart LR\n  A[Model writes] --> B[Chat streams] --> C[Reader follows]\n";
//! let parse = ParseOptions::default().limits(Limits::default().max_edges(100));
//! let layout = LayoutOptions::default().max_width(240.0).padding(4.0);
//! let rendered = pele_core::svg::write(&pele_core::parity_scene_with(source, parse, &layout)?)?;
//! assert!(rendered.width <= 240.0, "a flowchart that runs across turns to fit");
//! # Ok::<(), pele_core::RenderError>(())
//! ```
//!
//! A source Pele rejects is a [`RenderError::Diagnostic`], with Pele's message and, for a syntax
//! error, where it is.
//!
//! ```
//! use pele_core::{ErrorCode, RenderError};
//!
//! let Err(RenderError::Diagnostic(diagnostic)) = pele_core::render("flowchart LR\n  A -->\n") else {
//!     panic!("a syntax error");
//! };
//! assert_eq!(diagnostic.code, ErrorCode::Syntax);
//! assert_eq!(diagnostic.location.map(|at| at.line), Some(2));
//! ```

#![warn(missing_docs)]

mod catalog;
#[doc(hidden)]
pub mod common;
mod diagrams;
mod fit;
mod graph;
#[cfg(doctest)]
mod grows;
mod input;
/// Not part of the API: the gantt chart's date helpers, public for `tests/gantt_dates.rs`.
#[doc(hidden)]
pub use diagrams::gantt::probe as gantt_probe;
/// Not part of the API: JavaScript's number rules, public for the workspace's tests.
#[doc(hidden)]
pub mod js;
#[doc(hidden)]
pub mod layout;
/// Not part of the API: public so that `tests/block_layout.rs` can hold it to Pele.
#[doc(hidden)]
pub use diagrams::block::layout as block_layout;
/// Not part of the API: public so that `tests/sankey_layout.rs` can hold it to Pele.
#[doc(hidden)]
pub use diagrams::sankey::layout as sankey_layout;
/// Not part of the API: public so that `tests/xychart_ticks.rs` can hold it to Pele bit for bit.
#[doc(hidden)]
pub use diagrams::xychart::ticks as xychart_ticks;
mod measure;
pub mod scene;
pub mod svg;

pub use catalog::{DiagramKind, MermaidType};
/// Not part of the API: architecture's grid, public for the workspace's tests.
#[doc(hidden)]
pub use diagrams::architecture::probe as architecture_probe;
/// Not part of the API: C4's routes and layout, public for the workspace's tests.
#[doc(hidden)]
pub use diagrams::c4::probe as c4_probe;
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub use diagrams::eventmodeling::json::front_json as eventmodeling_front_json;
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub use diagrams::info::front_json as info_front_json;
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub use diagrams::radar::front_json as radar_front_json;
/// Not part of the API: railroad's front end, public for the workspace's tests.
#[doc(hidden)]
pub use diagrams::railroad::probe as railroad_probe;
/// Not part of the API: treemap's layout and numbers, public for the workspace's tests.
#[doc(hidden)]
pub use diagrams::treemap::probe as treemap_probe;
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub use diagrams::treeview::front_json as treeview_front_json;
/// Not part of the API: venn's geometry and trig, public for the workspace's tests.
#[doc(hidden)]
pub use diagrams::venn::probe as venn_probe;
pub use measure::{Font, FontFamily, FontWeight, Script, TextMeasure, TextMeasurer, TextRef, Typography};
pub use scene::{LinkRef, Role, Scene, Size};

/// A diagram rendered to SVG.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Rendered {
    /// The SVG document, as Pele writes it.
    pub svg: String,
    /// The drawing's width in pixels, the SVG's `width`.
    pub width: f64,
    /// The drawing's height in pixels, the SVG's `height`.
    pub height: f64,
}

/// Why a source drew nothing.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RenderError {
    /// The source is a diagram type this port does not render yet.
    NotPorted,
    /// The source or a limit is at fault, as Pele's `PeleError` reports it.
    Diagnostic(Diagnostic),
}

/// Why a source was rejected. `message` is Pele's wording.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// What kind of fault it is.
    pub code: ErrorCode,
    /// The diagram type the source is, where it was detected.
    pub kind: Option<DiagramKind>,
    /// The type as Mermaid names it, where it was detected.
    pub mermaid_type: Option<MermaidType>,
    /// Pele's message.
    pub message: String,
    /// Where in the source a syntax error is.
    pub location: Option<Location>,
}

/// A position in the source as the author wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// No diagram type was detected.
    Unsupported,
    /// The source does not parse.
    Syntax,
    /// The source parses, and asks for something the type does not allow.
    Semantic,
    /// The source or the SVG is longer than its limit, or a flowchart has more edges.
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
#[non_exhaustive]
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
#[non_exhaustive]
pub struct ParseOptions {
    /// Pele's `config`: the Mermaid config the source's own is laid over.
    pub config: HostConfig,
    /// The limits on the source, the SVG and a flowchart's edges.
    pub limits: Limits,
}

/// How the host wants a diagram laid out. Colours are absent: changing them cannot change a layout.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LayoutOptions {
    /// The fonts labels are measured in.
    pub typography: Typography,
    /// The space around the drawing, in pixels. Unset, as Pele's `padding` may be left out, each
    /// type takes its own default: 8 pixels, or a treemap's `diagramPadding`.
    pub padding: Option<f64>,
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
    /// A width over which a diagram that runs across, and does not fit `max_width`, is drawn
    /// running down where that is narrower, whatever the breakpoint. Unset, only the breakpoint
    /// decides, as in Pele, which has no such option.
    pub turn_wider_than: Option<f64>,
    /// What the diagram may link to and load.
    pub resources: ResourcePolicy,
    /// Pele's `monospace`: the parts of diagrams the host sets in the monospace font.
    pub monospace: Vec<MonospacePart>,
    /// Pele's `now`: the time a gantt chart treats as now, in milliseconds since 1970. Unset or
    /// not finite, it is the current time; a parity draw pins it to Pele's corpus clock.
    pub now: Option<f64>,
}

/// A part of a diagram the host may set in the monospace font: Pele's `MonospacePart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MonospacePart {
    /// A class's attributes and methods.
    Members,
    /// An entity's attributes.
    Attributes,
    /// A treeview's files and folders.
    Files,
    /// A sequence diagram's messages.
    Messages,
}

/// What a diagram may link to and load. A source cannot change it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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
            padding: None,
            max_width: None,
            responsive: None,
            id_prefix: None,
            auto_direction: true,
            direction_breakpoint: None,
            turn_wider_than: None,
            resources: ResourcePolicy::default(),
            monospace: Vec::new(),
            now: None,
        }
    }
}

impl LayoutOptions {
    /// Pele's `options.padding ?? 8`: the host's padding, else the 8 pixels every type but
    /// treemap takes.
    pub(crate) fn padding_or_default(&self) -> f64 {
        self.padding.unwrap_or(8.0)
    }

    /// The size of plain label text, in pixels: Pele's `fontSize`. 16 unless set.
    #[must_use]
    pub fn font_size(mut self, pixels: f64) -> Self {
        self.typography.normal.size = pixels;
        self
    }

    /// The space around the drawing, in pixels: Pele's `padding`. Unset, each type takes its own
    /// default.
    #[must_use]
    pub fn padding(mut self, pixels: f64) -> Self {
        self.padding = Some(pixels);
        self
    }

    /// The width the host has for the diagram, in pixels: Pele's `maxWidth`.
    #[must_use]
    pub fn max_width(mut self, pixels: f64) -> Self {
        self.max_width = Some(pixels);
        self
    }

    /// Whether the drawing scales down with its container: Pele's `responsive`. Unset, the
    /// source's `useMaxWidth` decides.
    #[must_use]
    pub fn responsive(mut self, responsive: bool) -> Self {
        self.responsive = Some(responsive);
        self
    }

    /// The prefix of the ids a drawing declares: Pele's `idPrefix`. `pele` unless set.
    #[must_use]
    pub fn id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.id_prefix = Some(prefix.into());
        self
    }

    /// Whether a diagram that runs across may be drawn running down when it does not fit
    /// [`LayoutOptions::max_width`]: Pele's `autoDirection`. On unless set.
    #[must_use]
    pub fn auto_direction(mut self, on: bool) -> Self {
        self.auto_direction = on;
        self
    }

    /// The width under which a diagram that runs across may be drawn running down, in pixels:
    /// Pele's `directionBreakpoint`. 640 unless set.
    #[must_use]
    pub fn direction_breakpoint(mut self, pixels: f64) -> Self {
        self.direction_breakpoint = Some(pixels);
        self
    }

    /// A width over which a diagram that runs across, and does not fit
    /// [`LayoutOptions::max_width`], is drawn running down where that is narrower, whatever the
    /// breakpoint. Pele has no such option.
    #[must_use]
    pub fn turn_wider_than(mut self, pixels: f64) -> Self {
        self.turn_wider_than = Some(pixels);
        self
    }

    /// What the diagram may link to and load. Pele's defaults unless set.
    #[must_use]
    pub fn resources(mut self, policy: ResourcePolicy) -> Self {
        self.resources = policy;
        self
    }

    /// The parts of diagrams set in the monospace font: Pele's `monospace`. None unless set.
    #[must_use]
    pub fn monospace(mut self, parts: impl IntoIterator<Item = MonospacePart>) -> Self {
        self.monospace = parts.into_iter().collect();
        self
    }

    /// The time a gantt chart treats as now, in milliseconds since 1970: Pele's `now`. A time
    /// that is not finite, and no time, is the current time.
    #[must_use]
    pub fn now(mut self, milliseconds: f64) -> Self {
        self.now = Some(milliseconds);
        self
    }
}

impl Limits {
    /// The longest source, in UTF-16 code units: Pele's `limit`. 50,000 unless set.
    #[must_use]
    pub fn source_utf16(mut self, units: usize) -> Self {
        self.source_utf16 = units;
        self
    }

    /// The longest SVG, in UTF-16 code units: Pele's `outputLimit`. 4,000,000 unless set.
    #[must_use]
    pub fn svg_utf16(mut self, units: usize) -> Self {
        self.svg_utf16 = units;
        self
    }

    /// The most edges a flowchart may have: Pele's `maxEdges`. 5,000 unless set.
    #[must_use]
    pub fn max_edges(mut self, edges: usize) -> Self {
        self.max_edges = edges;
        self
    }
}

impl ParseOptions {
    /// The Mermaid config the source's own is laid over: Pele's `config`. Empty unless set.
    #[must_use]
    pub fn config(mut self, config: HostConfig) -> Self {
        self.config = config;
        self
    }

    /// The limits on the source, the SVG and a flowchart's edges. Pele's unless set.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
}

impl ResourcePolicy {
    /// Whether a node may be a link: Pele's `links`. On unless set.
    #[must_use]
    pub fn links(mut self, allowed: bool) -> Self {
        self.links = allowed;
        self
    }

    /// Whether a node may show a picture: Pele's `images`. On unless set.
    #[must_use]
    pub fn images(mut self, allowed: bool) -> Self {
        self.images = allowed;
        self
    }

    /// The schemes a link may use besides a relative address: Pele's `linkSchemes`. `http`,
    /// `https`, `mailto` and `tel` unless set.
    #[must_use]
    pub fn link_schemes(mut self, schemes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.link_schemes = schemes.into_iter().map(Into::into).collect();
        self
    }

    /// The schemes a picture's address may use besides a relative address: Pele's
    /// `imageSchemes`. `http` and `https` unless set.
    #[must_use]
    pub fn image_schemes(mut self, schemes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.image_schemes = schemes.into_iter().map(Into::into).collect();
        self
    }

    /// The `rel` of a link: Pele's `linkRel`. Empty, a link has none. `noopener` unless set.
    #[must_use]
    pub fn link_rel(mut self, rel: impl Into<String>) -> Self {
        self.link_rel = rel.into();
        self
    }
}

/// What drawing reads from the host. Not part of the API: public for rune-pele, which lays a
/// diagram out with the fonts it draws in.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct NativeInputs<'a> {
    pub measure: &'a dyn TextMeasurer,
    pub options: &'a LayoutOptions,
}

impl std::fmt::Debug for NativeInputs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeInputs").field("options", self.options).finish_non_exhaustive()
    }
}

/// A parsed source with its effective config. Drawing it again needs no parsing.
pub struct Prepared {
    found: catalog::Detected,
    config: input::JsObject,
    model: catalog::PreparedModel,
    limits: Limits,
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prepared").field("kind", &self.found.kind).field("limits", &self.limits).finish_non_exhaustive()
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
/// Not part of the API: public for the workspace's differential tests.
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
/// Not part of the API: public for the workspace's differential tests.
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
/// Not part of the API: public for the workspace's differential tests.
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
/// Not part of the API: public for the workspace's differential tests.
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
/// Not part of the API: public for the workspace's differential tests.
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
/// Not part of the API: public for the workspace's differential tests.
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
/// Not part of the API: public for the workspace's differential tests.
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

/// The sequence front end's model as JSON, in the words of the oracle's `sequence_front` op, for
/// the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn sequence_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Sequence {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::sequence::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::sequence::json::front_json(&model, front.found.mermaid.name()))
}

/// The agentflow front end's model, graph and semantic model as JSON, in the words of the
/// oracle's `agentflow_front` op, for the differential test. A source of another type answers
/// only that type.
/// Not part of the API: public for the workspace's differential tests.
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

/// The block diagram front end's model as JSON, in the words of the oracle's `block_front` op,
/// for the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn block_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Block {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::block::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::block::json::front_json(&model))
}

/// The C4 front end's model as JSON, in the words of the oracle's `c4_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn c4_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::C4 {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::c4::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::c4::json::front_json(&model, front.found.mermaid.name()))
}

/// The gantt front end's model and each task's schedule against `now`, in milliseconds since
/// 1970, as JSON in the words of the oracle's `gantt_front` op, for the differential test. A
/// source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn gantt_front_json(source: &str, options: ParseOptions, now: f64) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Gantt {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::gantt::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::gantt::json::front_json(&model, now))
}

/// The architecture front end's model as JSON, in the words of the oracle's `architecture_front`
/// op, for the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn architecture_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Architecture {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::architecture::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::architecture::json::front_json(&model))
}

/// The cynefin front end's model as JSON, in the words of the oracle's `cynefin_front` op, for
/// the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn cynefin_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Cynefin {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::cynefin::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::cynefin::json::front_json(&model))
}

/// The wardley front end's model as JSON, in the words of the oracle's `wardley_front` op, for
/// the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn wardley_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Wardley {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::wardley::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::wardley::json::front_json(&model))
}

/// The xychart front end's db as JSON, in the words of the oracle's `xychart_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn xychart_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Xychart {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::xychart::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::xychart::json::front_json(&model))
}

/// The venn front end's model as JSON, in the words of the oracle's `venn_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn venn_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Venn {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::venn::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::venn::json::front_json(&model))
}

/// The quadrant chart front end's model as JSON, in the words of the oracle's `quadrant_front`
/// op, for the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn quadrant_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::QuadrantChart {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::quadrant::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::quadrant::json::front_json(&model))
}

/// The git graph front end's model as JSON, in the words of the oracle's `git_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn git_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::GitGraph {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::git::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::git::json::front_json(&model))
}

/// The sankey front end's model as JSON, in the words of the oracle's `sankey_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn sankey_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Sankey {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::sankey::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::sankey::json::front_json(&model))
}

/// The packet front end's model as JSON, in the words of the oracle's `packet_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn packet_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Packet {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::packet::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::packet::json::front_json(&model))
}

/// The kanban front end's board as JSON, in the words of the oracle's `kanban_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn kanban_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Kanban {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model =
        diagrams::kanban::parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::kanban::json::front_json(&model))
}

/// The mindmap front end's map as JSON, in the words of the oracle's `mindmap_front` op, for the
/// differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn mindmap_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Mindmap {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::mindmap::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::mindmap::json::front_json(&model))
}

/// The Ishikawa front end's outline as JSON, in the words of the oracle's `ishikawa_front` op, for
/// the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn ishikawa_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Ishikawa {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::ishikawa::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::ishikawa::json::front_json(&model))
}

/// The treemap front end's model as JSON, in the words of the oracle's `treemap_front` op, for
/// the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn treemap_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Treemap {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::treemap::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::treemap::json::front_json(&model))
}

/// The timeline front end's model as JSON, in the words of the oracle's `timeline_front` op, for
/// the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn timeline_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Timeline {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::timeline::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::timeline::json::front_json(&model))
}

/// The journey front end's model as JSON, in the words of the oracle's `journey_front` op, for
/// the differential test. A source of another type answers only that type.
/// Not part of the API: public for the workspace's differential tests.
#[doc(hidden)]
pub fn journey_front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = front(source, &options)?;
    if front.found.kind != DiagramKind::Journey {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = diagrams::journey::parse(&front.context(&origin, options.limits))
        .map_err(|error| tagged(error, front.found))?;
    Ok(diagrams::journey::json::front_json(&model))
}

impl Prepared {
    /// The diagram type the source is.
    pub fn kind(&self) -> DiagramKind {
        self.found.kind
    }

    /// Draws the parsed source. The model is not changed, so one `Prepared` can be drawn for
    /// several widths. Not part of the API: public for rune-pele, which draws the Scene natively.
    #[doc(hidden)]
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
    /// The type and Mermaid's alias detection found, which Pele's late limit error names.
    found: catalog::Detected,
}

impl std::fmt::Debug for ParityScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = self.scene.size();
        f.debug_struct("ParityScene").field("kind", &self.found.kind).field("size", &size).finish_non_exhaustive()
    }
}

impl ParityScene {
    /// The Scene the SVG writer reads. Not part of the API: public for rune-pele, which draws it
    /// natively to compare with Pele's SVG.
    #[doc(hidden)]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
}

/// The clock Pele's corpus test and the oracle give a render: `Date.UTC(2026, 0, 15, 12)`.
const PARITY_NOW: f64 = 1_768_478_400_000.0;

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
    let layout = LayoutOptions { now: layout.now.or(Some(PARITY_NOW)), ..layout.clone() };
    let scene = prepared.scene(NativeInputs { measure: &measure::ArialMetrics, options: &layout })?;
    Ok(ParityScene { scene, found: prepared.found })
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
