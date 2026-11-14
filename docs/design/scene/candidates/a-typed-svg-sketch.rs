// rune-pele candidate 1: "Pele's SVG, typed".
// One file, sectioned per crate and module. It does not compile as one unit; each section is
// precise enough to become its file. Bodies are `todo!("not implemented")`; `// TODO` lines
// carry the pseudocode for tricky logic. Doc comments state intent and invariants.

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: lib
// ═══════════════════════════════════════════════════════════════════════════════════════════
//! Rune-free core: Mermaid source to a `Drawing`, and a `Drawing` to Pele-exact SVG or to a
//! renderer-neutral `Picture`. Everything a porter touches (`scene`, `kit`, `layout`,
//! `diagrams`) is `pub(crate)`, so no other crate can import it.

pub mod js; // existing; gains sin/cos/atan2/hypot/max/min/PI below
pub mod paint; // Ink, Theme, Srgba: public because Picture carries them
pub mod picture; // the renderer-neutral flattened view (public data)
pub mod geom; // pure lowering maths for non-SVG targets (public)
pub mod text; // Measure, Metrics, Style are public; label layout is crate-private inside

mod config;
mod error;
mod fit;
mod kit;
mod layout;
mod options;
mod scene;
mod source;
mod svg;
mod types;
mod diagrams;

pub use config::Config;
pub use error::{Code, PeleError};
pub use options::{IconResolver, Limits, MathMarkup, MathResolver, Monospace, Options, ParseOptions};
pub use scene::{Anchor, Cap, Drawing, Join, LinkId, LinkInfo, PartId, PartKind, Role, Shift, Token};
pub use text::{Measure, Metrics, Style};
pub use types::{Parsed, Type};

/// A diagram rendered to SVG. Unchanged: `pele-cli` compares these three fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub svg: String,
    pub width: f64,
    pub height: f64,
}

/// Why a source did not become a drawing.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    /// Pele draws this type; rune-pele has not ported it yet (the `types` row says `_`).
    NotPorted(Type),
    /// What Pele itself would throw: unsupported diagram, syntax, semantic or limit.
    Pele(PeleError),
}

impl From<PeleError> for RenderError {
    fn from(e: PeleError) -> Self {
        RenderError::Pele(e)
    }
}

/// Parity entry, the CLI's only call: Pele's corpus inputs (Arial metric tables, fixed clock,
/// default options) and Pele's output limit. Byte-identical to Pele 1.13.0 for ported types.
pub fn render(source: &str) -> Result<Rendered, RenderError> {
    let options = Options::parity();
    let drawing = parse(source, &ParseOptions::default())?.draw(&options, &Metrics)?;
    let svg = drawing.to_svg();
    options.limits.check_output(&svg)?;
    Ok(Rendered { width: drawing.width(), height: drawing.height(), svg })
}

/// Pele's `parse`: limits, preprocessing, detection, entity encoding and the type's parser.
/// The result can be drawn many times (several widths, several measurers) without reparsing.
pub fn parse(source: &str, options: &ParseOptions) -> Result<Parsed, RenderError> {
    // TODO: options.limits.check_source(source)?            (UTF-16 length, as text.length)
    //       let pre = source::prepare(source);               (front matter, directives, comments)
    //       let found = types::detect(&pre.text).ok_or(PeleError::unsupported())?;
    //       let config = options.config.merged_shallow(&pre.config); (`{...extra, ...pre.config}`)
    //       types::parse(found, &pre, config, &options.limits)  (dispatch; NotPorted for `_` rows)
    todo!("not implemented")
}

/// Pele's `detectType`: the diagram type, or `None`. Cheap: preprocessing plus regexes.
pub fn detect(source: &str) -> Option<Type> {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: js  (additions only; existing round/to_string/to_fixed/num/write_num stay)
// ═══════════════════════════════════════════════════════════════════════════════════════════

pub const PI: f64 = std::f64::consts::PI;

/// `Math.sin` as V8 computes it (fdlibm), via the `libm` crate. `f64::sin` may differ in the
/// last bit on some platforms, and a last bit can move a `num()` hundredth.
pub fn sin(x: f64) -> f64 {
    todo!("not implemented") // libm::sin(x)
}
pub fn cos(x: f64) -> f64 {
    todo!("not implemented")
}
pub fn atan2(y: f64, x: f64) -> f64 {
    todo!("not implemented")
}
/// `Math.hypot`: V8's algorithm (scaled sum of squares), not `f64::hypot`.
pub fn hypot(a: f64, b: f64) -> f64 {
    todo!("not implemented")
}
/// `Math.max(a, b)`: NaN if either is NaN, and `+0` over `-0`. Rust's `f64::max` drops NaN.
pub fn max(a: f64, b: f64) -> f64 {
    todo!("not implemented")
}
pub fn min(a: f64, b: f64) -> f64 {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: error
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Pele's `PeleErrorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    UnsupportedDiagram,
    Syntax,
    Semantic,
    Limit,
}

/// Pele's `PeleError`, with the fields `tagErrors` fills in. `message` and `snippet` have
/// entity placeholders turned back into what the author wrote (`writtenCodes`).
#[derive(Debug, Clone, PartialEq)]
pub struct PeleError {
    pub code: Code,
    pub message: String,
    pub ty: Option<Type>,
    pub mermaid_type: Option<&'static str>,
    /// 1-based, 0 when unknown, in the author's source (front matter counted).
    pub line: u32,
    pub column: u32,
    pub snippet: String,
}

impl PeleError {
    pub(crate) fn new(code: Code, message: impl Into<String>) -> Self {
        todo!("not implemented")
    }
    /// "Diagram is nested too deeply." Rust cannot catch a stack overflow as Pele catches
    /// `RangeError`, so every recursive layout step counts depth against `Limits::depth`.
    pub(crate) fn too_deep() -> Self {
        todo!("not implemented")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: options
// ═══════════════════════════════════════════════════════════════════════════════════════════

use std::sync::Arc;

/// Read when the source is parsed (Pele's `ParseOptions`).
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub limits: Limits,
    /// The host's config. A source's own section replaces the host's whole section (shallow).
    pub config: Option<Config>,
}

/// Limits the host sets. A diagram's own config cannot change them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Source length in UTF-16 code units (Pele's `text.length`). Default 50,000.
    pub source: usize,
    /// SVG length in UTF-16 code units. Default 4,000,000. Checked by `render`, not `to_svg`.
    pub output: usize,
    /// Pele's `maxEdges`. Default 5,000.
    pub max_edges: usize,
    /// Nesting depth for recursive parse and layout steps. Stands in for Pele's `RangeError`.
    pub depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { source: 50_000, output: 4_000_000, max_edges: 5_000, depth: 2_000 }
    }
}

impl Limits {
    pub(crate) fn check_source(&self, source: &str) -> Result<(), PeleError> {
        todo!("not implemented")
    }
    pub fn check_output(&self, svg: &str) -> Result<(), PeleError> {
        todo!("not implemented")
    }
}

/// Read when a parsed diagram is drawn (Pele's `RenderOptions` minus the measurer, which is a
/// separate argument because it is a strategy, and minus parse-time fields).
#[derive(Clone)]
pub struct Options {
    pub font_size: f64,       // 16
    pub padding: f64,         // 8
    pub max_width: Option<f64>,
    pub auto_direction: bool, // true
    pub direction_breakpoint: Option<f64>,
    pub id_prefix: String,       // "pele"; also gates sankey gradients (`^[\w-]+$`)
    pub responsive: Option<bool>, // None: `!section.useMaxWidth === false`
    pub links: bool,
    pub link_schemes: Vec<String>,
    pub images: bool,
    pub image_schemes: Vec<String>,
    pub link_rel: Option<String>,
    pub font_family: Option<String>,
    pub font_family_mono: Option<String>,
    pub monospace: Monospace,
    /// What gantt treats as now, in ms since the epoch. Core never reads a clock.
    pub now: f64,
    pub icons: Option<Arc<dyn IconResolver>>,
    pub math: Option<Arc<dyn MathResolver>>,
    pub limits: Limits,
}

impl Default for Options {
    /// Pele's defaults: fontSize 16, padding 8, idPrefix "pele", links and images on,
    /// autoDirection on, `now` 0 (a host that draws gantt passes its clock).
    fn default() -> Self {
        todo!("not implemented")
    }
}

impl Options {
    /// The oracle's `PARITY` inputs: `now = Date.UTC(2026, 0, 15, 12)`, everything else default.
    pub fn parity() -> Self {
        todo!("not implemented")
    }
}

bitflags::bitflags! {
    /// Pele's `MonospacePart` list.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Monospace: u8 { const MEMBERS = 1; const ATTRIBUTES = 2; const FILES = 4; const MESSAGES = 8; }
}

/// Host icon markup by name, as Pele's `IconResolver`. Names arrive entity-decoded.
pub trait IconResolver {
    fn markup(&self, name: &str) -> Option<String>;
}

/// Pele's `MathMarkup`: dimensions in px.
#[derive(Debug, Clone, PartialEq)]
pub enum MathMarkup {
    Svg { svg: String, width: f64, height: f64 },
    Html { html: String, width: f64, height: f64 },
}

pub trait MathResolver {
    fn typeset(&self, tex: &str, size: f64) -> Option<MathMarkup>;
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: config
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// A JS value tree from YAML front matter, `%%{init}%%` JSON and the host. Objects keep JS
/// key order: integer-like keys first ascending, then insertion order. Merges skip
/// `__proto__`, `constructor` and `prototype`, as Pele's `merge` does.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Config {
    #[default]
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Config>),
    Object(Vec<(String, Config)>),
}

impl Config {
    /// `{...self, ...over}` (shallow, as `core.ts:101`).
    pub(crate) fn merged_shallow(&self, over: &Config) -> Config {
        todo!("not implemented")
    }
    pub(crate) fn get(&self, key: &str) -> &Config {
        todo!("not implemented")
    }
}

/// A config section read with JavaScript's coercions, at the point a type reads it. This is
/// the validation boundary for config: callers get typed values or `None`, never a `Config`.
#[derive(Clone, Copy)]
pub(crate) struct ConfigView<'a>(pub(crate) &'a Config);

impl<'a> ConfigView<'a> {
    /// `typeof v === 'number' ? v : undefined`.
    pub(crate) fn number(&self, key: &str) -> Option<f64> {
        todo!("not implemented")
    }
    /// `v === undefined || v === null ? None : Some(String(v))`; arrays join with ",".
    pub(crate) fn js_string(&self, key: &str) -> Option<String> {
        todo!("not implemented")
    }
    /// `typeof v === 'string' ? v : undefined`.
    pub(crate) fn string(&self, key: &str) -> Option<&'a str> {
        todo!("not implemented")
    }
    /// `v === true`.
    pub(crate) fn is_true(&self, key: &str) -> bool {
        todo!("not implemented")
    }
    pub(crate) fn section(&self, key: &str) -> ConfigView<'a> {
        todo!("not implemented")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: source  (port of preprocess.ts, entities.ts, util/yaml.ts)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Preprocessed source, as `preprocess()` returns it.
pub(crate) struct Prepared {
    /// Comments removed and trimmed at the start.
    pub(crate) text: String,
    /// Comments kept, for `KEEP_COMMENTS` types.
    pub(crate) with_comments: String,
    pub(crate) config: Config,
    pub(crate) title: Option<String>,
    /// Newlines removed with the front matter.
    pub(crate) line_offset: usize,
}

pub(crate) fn prepare(source: &str) -> Prepared {
    // TODO: \r\n? → \n; HTML attribute quotes; front matter + parse_yaml; directives
    //       (trim, ' → ", JSON.parse, deep merge, invalid ignored); cleanup_comments.
    todo!("not implemented")
}

/// `encodeEntities`: hides `#name;` codes behind placeholders before parsing.
pub(crate) fn encode_entities(text: &str) -> String {
    todo!("not implemented")
}

/// `decodeEntities`: the one place placeholders turn back into characters. `Txt::decoded`
/// and label layout call it, so every string in a Scene is already final text.
pub(crate) fn decode_entities(text: &str) -> String {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: types  (the registry: one row per type, nothing else lists types)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// One row per Pele `DiagramType`: `Variant = "pele name" => port` where port is the type's
/// `Port` impl, or `_` while unported. This macro is the only list of types. It defines
/// `Type`, `Type::ALL`, `Type::name`, `Type::is_ported` and dispatch. A new port changes one row.
macro_rules! diagram_types {
    ($($v:ident = $name:literal => $port:tt),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Type { $($v),* }
        impl Type {
            pub const ALL: &'static [Type] = &[$(Type::$v),*];
            /// The `pele-{name}` class and `aria-roledescription` (every `svgDocument` literal
            /// equals the DiagramType name; checked across all 33 types).
            pub fn name(self) -> &'static str { match self { $(Type::$v => $name),* } }
            pub fn is_ported(self) -> bool { match self { $(Type::$v => diagram_types!(@ported $port)),* } }
        }
        fn dispatch(ty: Type, job: Job<'_>) -> Result<Parsed, RenderError> {
            match ty { $(Type::$v => diagram_types!(@run $port, job, ty)),* }
        }
    };
    (@ported _) => { false };
    (@ported $p:path) => { true };
    (@run _, $job:ident, $ty:ident) => { Err(RenderError::NotPorted($ty)) };
    (@run $p:path, $job:ident, $ty:ident) => { $job.run::<$p>() };
}

diagram_types! {
    Flowchart = "flowchart" => crate::diagrams::flowchart::Flowchart,
    Pie = "pie" => crate::diagrams::pie::Pie,
    Sequence = "sequence" => _,
    Class = "class" => _,
    State = "state" => _,
    Er = "er" => _,
    Gantt = "gantt" => _,
    GitGraph = "gitGraph" => _,
    Mindmap = "mindmap" => _,
    Timeline = "timeline" => _,
    Journey = "journey" => _,
    Xychart = "xychart" => _,
    QuadrantChart = "quadrantChart" => _,
    Requirement = "requirement" => _,
    C4 = "c4" => _,
    Sankey = "sankey" => _,
    Block = "block" => _,
    Packet = "packet" => _,
    Kanban = "kanban" => _,
    Architecture = "architecture" => _,
    Radar = "radar" => _,
    Treemap = "treemap" => _,
    Info = "info" => _,
    Agentflow = "agentflow" => _,
    Usecase = "usecase" => _,
    Railroad = "railroad" => _,
    TreeView = "treeView" => _,
    Venn = "venn" => _,
    Cynefin = "cynefin" => _,
    Wardley = "wardley" => _,
    Swimlane = "swimlane" => _,
    Ishikawa = "ishikawa" => _,
    Eventmodeling = "eventmodeling" => _,
}

/// Pele's `DETECTORS`, in Mermaid's registration order (first match wins). Order is
/// meaning, so this is data, not a derived list. A test asserts every `Type::ALL` entry
/// appears at least once, so a type cannot be registered without a detector.
const DETECTORS: &[(Type, &str, fn(&str) -> bool)] = &[
    // TODO: 37 rows copied from detect.ts, e.g.
    // (Type::Flowchart, "flowchart-elk", |s| starts_ws(s, "flowchart-elk")),
    // (Type::Mindmap, "mindmap", ...), … (Type::Pie, "pie", |s| starts_ws(s, "pie")), …
];

pub(crate) fn detect(cleaned: &str) -> Option<(Type, &'static str)> {
    todo!("not implemented")
}

/// What a type gets to parse: preprocessed, entity-encoded text ending in '\n', as Pele's
/// `Diagram.parse(source, config, title, lineOffset, limits)`.
pub(crate) struct Source<'a> {
    pub(crate) text: &'a str,
    pub(crate) config: &'a Config,
    pub(crate) title: Option<&'a str>,
    pub(crate) line_offset: usize,
    pub(crate) limits: &'a Limits,
}

/// The contract a porter implements: Pele's `Diagram<Model>`, globals made explicit.
/// `draw` measures, lays out and builds the Scene, redrawing through `fit` as Pele does.
pub(crate) trait Port: 'static {
    type Model: 'static;
    /// Pele's `section`: the config key when it is not the type's own name.
    const SECTION: Option<&'static str> = None;
    const KEEP_COMMENTS: bool = false;
    /// Enables `$$…$$` in labels: `DrawCx::math` is `None` unless this is true.
    const MATH: bool = false;
    fn parse(src: &Source<'_>) -> Result<Self::Model, PeleError>;
    fn draw(model: &Self::Model, cx: &DrawCx<'_>) -> Result<Drawing, PeleError>;
}

struct Job<'a> {
    found: (Type, &'static str),
    pre: &'a Prepared,
    config: Config,
    limits: &'a Limits,
}

impl Job<'_> {
    fn run<P: Port>(self) -> Result<Parsed, RenderError> {
        // TODO: text = encode_entities(if P::KEEP_COMMENTS { with_comments } else { text }) + "\n";
        //       model = tag_errors(found, locating_in(.., || P::parse(&Source{..})))?;
        //       Ok(Parsed { ty, mermaid_type, config, model: Box::new(Ported::<P>(model)) })
        todo!("not implemented")
    }
}

/// A parsed source, its model type-erased so the caller need not know the type.
pub struct Parsed {
    ty: Type,
    mermaid_type: &'static str,
    config: Config,
    model: Box<dyn Drawable>,
}

trait Drawable {
    fn draw(&self, cx: &DrawCx<'_>) -> Result<Drawing, PeleError>;
    fn math(&self) -> bool;
    fn section(&self) -> Option<&'static str>;
}

struct Ported<P: Port>(P::Model);

impl<P: Port> Drawable for Ported<P> {
    fn draw(&self, cx: &DrawCx<'_>) -> Result<Drawing, PeleError> {
        P::draw(&self.0, cx)
    }
    fn math(&self) -> bool {
        P::MATH
    }
    fn section(&self) -> Option<&'static str> {
        P::SECTION
    }
}

impl Parsed {
    pub fn ty(&self) -> Type {
        self.ty
    }
    /// Mermaid's name for it, such as `flowchart-v2` for `graph TD`.
    pub fn mermaid_type(&self) -> &'static str {
        self.mermaid_type
    }
    /// Pele's `render(model, options)` with `measure` as the measurer.
    pub fn draw(&self, options: &Options, measure: &dyn Measure) -> Result<Drawing, RenderError> {
        // TODO: section = config[section ?? ty.name()]; responsive = options.responsive
        //         .unwrap_or(!(section is object && section.useMaxWidth === false));
        //       palette = kit::palette::palette(ty, &config);   (was withPalette)
        //       math = if model.math() { options.math } else { None }; (was withMath)
        //       icons resolve decoded names (was withIcons + decodeEntities wrapper)
        //       cx = DrawCx { .. }; drawing = tag_errors(self.model.draw(&cx))?;
        //       links ids are already decoded (Sketch::link decodes on push).
        todo!("not implemented")
    }
}

/// What a draw step reads. Replaces Pele's module globals (`withPalette`, `withMath`,
/// `withIcons`, label `marks`). Shared read-only by every fit attempt; nothing in it mutates.
pub(crate) struct DrawCx<'a> {
    pub(crate) ty: Type,
    pub(crate) options: &'a Options,
    pub(crate) measure: &'a dyn Measure,
    pub(crate) config: &'a Config,
    pub(crate) palette: Palette,
    pub(crate) math: Option<&'a dyn MathResolver>,
    pub(crate) responsive: bool,
}

impl<'a> DrawCx<'a> {
    pub(crate) fn font_size(&self) -> f64 {
        self.options.font_size
    }
    pub(crate) fn padding(&self) -> f64 {
        self.options.padding
    }
    pub(crate) fn measure(&self) -> &dyn Measure {
        self.measure
    }
    pub(crate) fn section(&self, key: &str) -> ConfigView<'a> {
        todo!("not implemented")
    }
    /// Pele's `layoutLabel(raw, markdown, measurer, size, maxWidth, base)` with this
    /// context's measurer and math resolver. An empty or absent raw gives `Label::EMPTY`.
    pub(crate) fn label(&self, raw: Option<&str>, markdown: bool, size: f64, max_width: f64, base: Style) -> Arc<Label> {
        todo!("not implemented")
    }
    /// The host's markup for an icon, by entity-decoded name.
    pub(crate) fn icon(&self, name: &str) -> Option<Arc<str>> {
        todo!("not implemented")
    }
    /// A fresh sketch for one draw attempt. Every `fit` attempt makes its own, so a losing
    /// attempt's parts and links are dropped with it and never reach the winner.
    pub(crate) fn sketch(&self, ty: Type) -> Sketch<'_> {
        todo!("not implemented")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: text::measure
// ═══════════════════════════════════════════════════════════════════════════════════════════

bitflags::bitflags! {
    /// Pele's `Style` bits, with the same values.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Style: u8 { const BOLD = 1; const ITALIC = 2; const MONO = 4; const SUB = 8; const SUP = 16; const UNDERLINE = 32; }
}

/// The width oracle: the one strategy the host injects. Pele's `TextMeasurer`.
/// Invariant: a pure function of its arguments for a fixed `epoch()`.
pub trait Measure {
    /// The advance of `text` at `size` px in `style`, in px. Sub/sup are measured smaller
    /// (`size / 1.2`), as `metricsMeasurer` does; an implementation must do the same.
    fn width(&self, text: &str, size: f64, style: Style) -> f64;
    /// Changes when the same call could answer differently (Rune: `font_epoch()`). Part of
    /// every layout cache key. `Metrics` is constant.
    fn epoch(&self) -> u64 {
        0
    }
}

/// Pele's `metricsMeasurer`: Arial advance tables, iterated per UTF-16 code unit (an astral
/// character is a wide high surrogate plus a zero-width low one). The parity measurer.
pub struct Metrics;

impl Measure for Metrics {
    fn width(&self, text: &str, size: f64, style: Style) -> f64 {
        todo!("not implemented")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: text::label  (port of text/label.ts; the wrapping stays Pele's)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Pele's `Span`. `text` is decoded, final text.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub icon: Option<String>,
    pub math: Option<MathMarkup>,
    pub width: f64,
}

/// Pele's `Label`: lines of styled spans with measured widths. Measured once, shared by
/// the writer and the flattener through `Arc`.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub lines: Vec<Vec<Span>>,
    pub widths: Vec<f64>,
    pub width: f64,
    pub height: f64,
    pub size: f64,
    pub line_height: f64,
    /// Present only when math needs extra line height.
    pub heights: Option<Vec<f64>>,
}

/// Pele's baseline convention: a line's text sits at `mid + size * SHIFT`. The writer adds
/// it; the Rune adapter subtracts it from raw `<text>` y. One constant for both directions.
pub const SHIFT: f64 = 0.35;

/// One line of a label placed at a centre, as `labelSvg` walks it. The writer and
/// `picture` both iterate this, so the placement arithmetic exists once.
pub(crate) struct PlacedLine<'l> {
    pub(crate) spans: &'l [Span],
    pub(crate) mid: f64,
    /// `mid + size * SHIFT`.
    pub(crate) baseline: f64,
    /// `cx - widths[i] / 2`, the left edge for lines that place spans one by one.
    pub(crate) left: f64,
    /// Some span is an icon or math, so each span is positioned on its own.
    pub(crate) placed: bool,
}

pub(crate) fn place(label: &Label, cx: f64, cy: f64) -> impl Iterator<Item = PlacedLine<'_>> {
    // TODO: top = cy - height/2; per line h = heights[i] ?? line_height; mid = top + h/2;
    //       baseline = mid + size*SHIFT; placed = any span with icon or math; top += h.
    std::iter::empty()
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: scene  (the Scene: Pele's SVG, typed; crate-private structure)
// ═══════════════════════════════════════════════════════════════════════════════════════════

use std::borrow::Cow;

/// A node of the Scene tree. The tree mirrors what Pele writes, element for element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Elem(Box<Elem>),
    /// Character data. Written with `esc`, shown as is by Rune.
    Text(Txt),
    /// `labelSvg(label, cx, cy, attrs, icons)`: the writer picks the single-`<text>` fast
    /// path or the `<tspan>` form from the label's content. Icons and math are siblings after
    /// `</text>`.
    Label(Box<LabelNode>),
    /// The fixed nested `<svg class="pele-icon" …>` Pele writes for icons outside labels.
    Icon(Box<IconNode>),
}

/// One element. `attrs` are written in exactly this order. They are never sorted or
/// deduplicated: the order of the porter's calls is Pele's call-site order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Elem {
    pub(crate) tag: Tag,
    pub(crate) attrs: Attrs,
    pub(crate) children: Vec<Node>,
    pub(crate) meta: Meta,
}

/// Never written. What only the product side needs.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Meta {
    /// Membership for an element with no `data-id` (pie's inside labels).
    pub(crate) part: Option<PartId>,
    /// Names the layer this node starts. A name, not an index, because Pele omits empty
    /// layers and an index would shift keys between edits.
    pub(crate) layer: Option<&'static str>,
    /// Set on `Tag::A` and the internal-link `<a>`: an index into the drawing's link table.
    pub(crate) link: Option<LinkId>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LabelNode {
    pub(crate) label: Arc<Label>,
    pub(crate) cx: f64,
    pub(crate) cy: f64,
    /// Pele's `attrs` string, typed. `textStyle` hoisting checks `attrs.has(FontWeight)`.
    pub(crate) attrs: Attrs,
    pub(crate) meta: Meta,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IconNode {
    pub(crate) name: Txt,
    pub(crate) x: N,
    pub(crate) y: N,
    pub(crate) size: N,
    /// Host markup, trusted and written verbatim inside the nested svg. `None` writes an empty icon.
    pub(crate) markup: Option<Arc<str>>,
    pub(crate) meta: Meta,
}

/// The closed set of elements Pele writes. The survey found no `<line>`, `<polyline>`,
/// `<marker>`, `<use>`, `<clipPath>`, `<mask>`, `<pattern>`, `<textPath>` or `<style>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tag {
    G,
    A,
    Title,
    Path,
    Rect,
    Circle,
    Ellipse,
    Polygon,
    Text,
    Tspan,
    Image,
    Defs,
    LinearGradient,
    Stop,
}

impl Tag {
    /// Void tags are written `<tag …/>`; containers always `<tag …></tag>`, even when empty
    /// (Pele writes `<g class="pele-nodes"></g>`).
    pub(crate) fn is_void(self) -> bool {
        matches!(self, Tag::Path | Tag::Rect | Tag::Circle | Tag::Ellipse | Tag::Polygon | Tag::Image | Tag::Stop)
    }
}

/// A number and how Pele prints it at that call site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum N {
    /// `num(x)`: hundredths, `-0` as `0`, via `js::write_num`.
    Num(f64),
    /// A template literal `${x}`: `Number#toString`, via `js::to_string` (`r="${radius}"`,
    /// `stroke-width="2.5"`, `width="${SWATCH}"`).
    Raw(f64),
}

impl N {
    /// The value a reader of the written text would parse back (Pele's `written()`), for
    /// tests that must sample what was written, like `fillUnder`.
    pub(crate) fn written(self) -> f64 {
        todo!("not implemented")
    }
    pub(crate) fn value(self) -> f64 {
        match self {
            N::Num(v) | N::Raw(v) => v,
        }
    }
}

pub(crate) fn num(x: f64) -> N {
    N::Num(x)
}
pub(crate) fn raw(x: f64) -> N {
    N::Raw(x)
}

/// Text in the Scene: always final, decoded text. How it was made states the call site's
/// escaping intent, so the writer can always use `esc`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Txt(Arc<str>);

impl Txt {
    /// Where Pele calls `escText`: entity codes are decoded first (tooltips, `data-id`,
    /// accessible names, label text).
    pub(crate) fn decoded(raw: &str) -> Txt {
        todo!("not implemented") // Txt(decode_entities(raw).into())
    }
    /// Where Pele calls `esc` on a value that was checked before it was written (URLs,
    /// targets, rel): written as checked, not decoded.
    pub(crate) fn verbatim(s: impl Into<Arc<str>>) -> Txt {
        Txt(s.into())
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

use crate::kit::style::CssText;

/// Every attribute Pele writes, each carrying its value type. Grep of the Pele source:
/// about 50 names. Root-only attributes (`xmlns`, `role`, `aria-*`) belong to the writer's
/// root function, not to this enum.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Attr {
    // identity and structure
    Class(Cow<'static, str>),
    /// Writes `esc(part.id)`. The part is the identity; `data-id` cannot drift from it.
    DataId(PartId),
    DataIcon(Txt),
    DataHref(Txt),
    Id(Txt),
    Href(Txt),
    Target(Txt),
    Rel(Txt),
    XmlSpacePreserve,
    PointerEventsNone,
    Overflow(&'static str),
    PreserveAspectRatio(&'static str),
    ViewBox([N; 4]),
    GradientUnitsUserSpace,
    // geometry
    X(N),
    Y(N),
    X1(N),
    X2(N),
    Width(N),
    Height(N),
    Rx(N),
    Ry(N),
    R(N),
    Cx(N),
    Cy(N),
    D(PathData),
    Points(Vec<(N, N)>),
    Transform(Transform),
    Offset(N),
    // paint
    Fill(Paint),
    Stroke(Paint),
    StrokeWidth(N),
    StrokeDasharray(Vec<N>),
    StrokeLinejoin(Join),
    StrokeLinecap(Cap),
    FillOpacity(N),
    Opacity(N),
    StopColor(Paint),
    Color(Paint),
    Style(Decls),
    // text
    FontSize(FontSize),
    FontWeight(Weight),
    FontStyleItalic,
    FontFamily(Family),
    TextAnchor(Anchor),
    TextDecorationUnderline,
    BaselineShift(Shift),
}

/// Attribute names, for `has`/`get`/`drop` without matching on values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrName {
    Class,
    DataId,
    Fill,
    Stroke,
    StrokeWidth,
    Style,
    FontWeight,
    FontStyle,
    FontFamily,
    // TODO: one per Attr variant, derived by a small macro from the Attr list so the two
    //       never drift (or `Attr::name(&self) -> AttrName` with an exhaustive match).
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Attrs(pub(crate) Vec<Attr>);

impl Attrs {
    pub(crate) fn new() -> Attrs {
        Attrs(Vec::new())
    }
    pub(crate) fn has(&self, name: AttrName) -> bool {
        todo!("not implemented")
    }
    /// `a + b` of two Pele attribute strings.
    pub(crate) fn then(mut self, more: Attrs) -> Attrs {
        self.0.extend(more.0);
        self
    }
    /// Pele's `rounded(attrs)`: splice `rx:var(--_r)` first into an existing style, else
    /// append `style="rx:var(--_r)"` (unterminated, as Pele's `ROUNDED`).
    pub(crate) fn rounded(self) -> Attrs {
        // TODO: if let Some(Style(decls)) = first Style attr { decls.items.insert(0, Decl{Rx, CssText::radius()}) }
        //       else push Style(Decls{ items: [Decl{Rx, CssText::radius()}], terminated: false })
        todo!("not implemented")
    }
    /// Pele's `a.replace('stroke="var(--_b)"', 'stroke="none"')`: the first attribute equal
    /// to `from` becomes `to`, in place, keeping its position.
    pub(crate) fn replace(self, from: Attr, to: Attr) -> Attrs {
        todo!("not implemented")
    }
    /// Removes every attribute with this name (casing strips `class="pele-marker"`).
    pub(crate) fn without(self, name: AttrName) -> Attrs {
        todo!("not implemented")
    }
    pub(crate) fn fill(&self) -> Option<&Paint> {
        todo!("not implemented")
    }
    pub(crate) fn style(&self) -> Option<&Decls> {
        todo!("not implemented")
    }
    // Builder methods mirror `Elem`'s (below) for attribute lists passed around as values.
    pub(crate) fn class(mut self, c: impl Into<Cow<'static, str>>) -> Attrs {
        self.0.push(Attr::Class(c.into()));
        self
    }
    pub(crate) fn font_weight(mut self, w: Weight) -> Attrs {
        self.0.push(Attr::FontWeight(w));
        self
    }
    pub(crate) fn fill_paint(mut self, p: impl Into<Paint>) -> Attrs {
        self.0.push(Attr::Fill(p.into()));
        self
    }
    pub(crate) fn stroke_paint(mut self, p: impl Into<Paint>) -> Attrs {
        self.0.push(Attr::Stroke(p.into()));
        self
    }
    pub(crate) fn extend(mut self, more: Option<Attr>) -> Attrs {
        self.0.extend(more);
        self
    }
}

/// A `style` attribute's declarations in order. `terminated` reproduces the trailing `;`
/// that `resolveStyle` output has and Pele's bare `ROUNDED` lacks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decls {
    pub(crate) items: Vec<Decl>,
    pub(crate) terminated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decl {
    pub(crate) prop: Prop,
    pub(crate) value: CssText,
}

impl Decls {
    /// The value of a property, as casing reads `stroke-width` out of a line style.
    pub(crate) fn get(&self, prop: Prop) -> Option<&CssText> {
        todo!("not implemented")
    }
}

/// CSS properties Pele lets through (`SHAPE_PROPS` ∪ `TEXT_PROPS`, with `color` already
/// renamed to `fill` in text styles) plus the ones Pele adds itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Prop {
    Fill,
    FillOpacity,
    Stroke,
    StrokeWidth,
    StrokeDasharray,
    StrokeDashoffset,
    StrokeLinecap,
    StrokeLinejoin,
    StrokeOpacity,
    Opacity,
    Rx,
    Ry,
    FontSize,
    FontFamily,
    FontWeight,
    FontStyle,
    TextDecoration,
    LetterSpacing,
    WordSpacing,
}

/// A paint, as a closed token vocabulary. SVG writes the `var()` form; `picture` turns it
/// into `paint::Ink`; Rune resolves `Ink` against a `Theme` at paint time.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Paint {
    /// `none`
    None,
    /// `currentColor`
    Current,
    /// `var(--_bg)` … `var(--_c)`
    Role(Role),
    /// `var(--pele-series-{k+1},{SERIES[k]})`, `k = i % 8`.
    Series(u8),
    /// `var(--pele-critical,#e45756)`, `var(--pele-fg-on-light,#1f1f1f)`, …
    Token(Token),
    /// `color-mix(in srgb,{a} {num(pct)}%,{b})`, used by struck cluster titles.
    Mix { a: Box<Paint>, pct: N, b: Box<Paint> },
    /// `url(#{id}) {fallback}`, used by sankey gradients.
    Url { id: Txt, fallback: Box<Paint> },
    /// Author colour text that a validator accepted, written `esc`'d.
    Css(CssText),
}

impl Paint {
    pub(crate) fn series(index: usize) -> Paint {
        Paint::Series((index % 8) as u8)
    }
}

impl From<Role> for Paint {
    fn from(r: Role) -> Paint {
        Paint::Role(r)
    }
}

/// Pele's private aliases set once on the root (`ROOT_STYLE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Bg,         // --_bg  var(--pele-bg,#fff)
    Fg,         // --_fg  var(--pele-fg,#1f1f1f)
    Muted,      // --_m   var(--pele-muted,#6e6e6e)
    Line,       // --_l   var(--pele-line,#1f1f1f)
    Surface,    // --_s   var(--pele-surface,#f3f3f3)
    SurfaceAlt, // --_a   var(--pele-surface-alt,#e6e6e6)
    Border,     // --_b   var(--pele-border,#8a8a8a)
    Accent,     // --_c   var(--pele-accent,#5b7bd5)
}

/// Public tokens Pele writes directly, with their fallbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    Critical,
    OnLight,
    OnDark,
    Error,
}

/// `font-weight` values. The writer's root scan sees `var(--_tw|hw|w)` in the output and
/// declares exactly the weights used.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Weight {
    Title,   // var(--_tw)
    Heading, // var(--_hw)
    Bold,    // var(--_w)
    Css(CssText),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    Mono, // var(--_fm)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FontSize {
    Px(N),
    /// `font-size="smaller"` with a baseline shift.
    Smaller,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    Start,
    Middle,
    End,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Join {
    Miter,
    Round,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cap {
    Butt,
    Round,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shift {
    None,
    Sub,
    Super,
}

/// `transform="…"` as written: `translate(x,y)`, `rotate(a)`, `rotate(a cx cy)`, space-joined.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Transform(pub(crate) Vec<TransformOp>);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TransformOp {
    Translate(N, N),
    Rotate(N, Option<(N, N)>),
    Scale(N),
}

pub(crate) fn translate(x: N, y: N) -> Transform {
    Transform(vec![TransformOp::Translate(x, y)])
}

/// Path data as typed segments. It replaces both Pele's `d` string and the `tokens` sidecar
/// that `fillUnder` replays: sampling uses `N::written`.
/// Written form (the only one in Pele): command letter, `x,y` pairs separated by spaces,
/// `A rx,ry rot large sweep x,y`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PathData(pub(crate) Vec<Seg>);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Seg {
    pub(crate) cmd: Cmd,
    /// Lowercase (relative) command, as railroad writes `a`, `q`, `t`, `h`, `v`, `l`.
    pub(crate) rel: bool,
}

pub(crate) type P = (N, N);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cmd {
    M(P),
    L(P),
    H(N),
    V(N),
    C(P, P, P),
    Q(P, P),
    T(P),
    A { rx: N, ry: N, rot: N, large: bool, sweep: bool, to: P },
    Z,
}

impl PathData {
    pub(crate) fn new() -> PathData {
        PathData(Vec::new())
    }
    pub(crate) fn m(mut self, x: N, y: N) -> Self {
        self.0.push(Seg { cmd: Cmd::M((x, y)), rel: false });
        self
    }
    pub(crate) fn l(mut self, x: N, y: N) -> Self {
        self.0.push(Seg { cmd: Cmd::L((x, y)), rel: false });
        self
    }
    pub(crate) fn a(mut self, rx: N, ry: N, large: bool, sweep: bool, x: N, y: N) -> Self {
        self.0.push(Seg { cmd: Cmd::A { rx, ry, rot: raw(0.0), large, sweep, to: (x, y) }, rel: false });
        self
    }
    pub(crate) fn c(mut self, a: P, b: P, to: P) -> Self {
        self.0.push(Seg { cmd: Cmd::C(a, b, to), rel: false });
        self
    }
    pub(crate) fn z(mut self) -> Self {
        self.0.push(Seg { cmd: Cmd::Z, rel: false });
        self
    }
    /// Appends another path's segments (`ring(outer) + ring(inner)`).
    pub(crate) fn then(mut self, more: PathData) -> Self {
        self.0.extend(more.0);
        self
    }
}

/// Element builder. Each method appends one attribute; call order is write order.
pub(crate) fn el(tag: Tag) -> Elem {
    Elem { tag, attrs: Attrs::new(), children: Vec::new(), meta: Meta::default() }
}

/// `labelSvg(label, cx, cy, attrs)` as a node.
pub(crate) fn label(label: &Arc<Label>, cx: f64, cy: f64, attrs: Attrs) -> Node {
    Node::Label(Box::new(LabelNode { label: label.clone(), cx, cy, attrs, meta: Meta::default() }))
}

impl Elem {
    pub(crate) fn class(mut self, c: impl Into<Cow<'static, str>>) -> Self {
        self.attrs.0.push(Attr::Class(c.into()));
        self
    }
    pub(crate) fn data_id(mut self, part: PartId) -> Self {
        self.attrs.0.push(Attr::DataId(part));
        self
    }
    pub(crate) fn x(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::X(v));
        self
    }
    pub(crate) fn y(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::Y(v));
        self
    }
    pub(crate) fn width(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::Width(v));
        self
    }
    pub(crate) fn height(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::Height(v));
        self
    }
    pub(crate) fn rx(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::Rx(v));
        self
    }
    pub(crate) fn r(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::R(v));
        self
    }
    pub(crate) fn d(mut self, d: PathData) -> Self {
        self.attrs.0.push(Attr::D(d));
        self
    }
    pub(crate) fn transform(mut self, t: Transform) -> Self {
        self.attrs.0.push(Attr::Transform(t));
        self
    }
    pub(crate) fn fill(mut self, p: impl Into<Paint>) -> Self {
        self.attrs.0.push(Attr::Fill(p.into()));
        self
    }
    /// `${seriesInkFill(i)}` and friends: nothing when `None`.
    pub(crate) fn fill_opt(mut self, p: Option<Paint>) -> Self {
        self.attrs.0.extend(p.map(Attr::Fill));
        self
    }
    pub(crate) fn stroke(mut self, p: impl Into<Paint>) -> Self {
        self.attrs.0.push(Attr::Stroke(p.into()));
        self
    }
    pub(crate) fn stroke_width(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::StrokeWidth(v));
        self
    }
    pub(crate) fn stroke_linejoin(mut self, j: Join) -> Self {
        self.attrs.0.push(Attr::StrokeLinejoin(j));
        self
    }
    pub(crate) fn font_size(mut self, v: N) -> Self {
        self.attrs.0.push(Attr::FontSize(FontSize::Px(v)));
        self
    }
    pub(crate) fn text_anchor(mut self, a: Anchor) -> Self {
        self.attrs.0.push(Attr::TextAnchor(a));
        self
    }
    /// Appends a whole attribute list (`${SHAPE_ATTRS}${style.shape}`).
    pub(crate) fn attrs(mut self, more: Attrs) -> Self {
        self.attrs.0.extend(more.0);
        self
    }
    pub(crate) fn text(mut self, t: Txt) -> Self {
        self.children.push(Node::Text(t));
        self
    }
    pub(crate) fn child(mut self, n: impl Into<Node>) -> Self {
        self.children.push(n.into());
        self
    }
    pub(crate) fn children(mut self, ns: impl IntoIterator<Item = impl Into<Node>>) -> Self {
        self.children.extend(ns.into_iter().map(Into::into));
        self
    }
    /// Silent part membership (not written).
    pub(crate) fn belongs(mut self, part: PartId) -> Self {
        self.meta.part = Some(part);
        self
    }
    /// Names the layer this element starts (not written).
    pub(crate) fn layer(mut self, name: &'static str) -> Self {
        self.meta.layer = Some(name);
        self
    }
    /// Pele's `x ? '<g …>x</g>' : ''`: the element only when it has children.
    pub(crate) fn if_any(self) -> Option<Node> {
        (!self.children.is_empty()).then(|| self.into())
    }
}

impl From<Elem> for Node {
    fn from(e: Elem) -> Node {
        Node::Elem(Box::new(e))
    }
}

impl Node {
    /// Names the layer this node starts, for nodes that are not `Elem` (a top-level title label).
    pub(crate) fn layer(self, name: &'static str) -> Node {
        todo!("not implemented")
    }
    /// `hollowOn`: every `fill` equal to `from` in this subtree becomes `to`.
    pub(crate) fn recolor(self, from: &Paint, to: &Paint) -> Node {
        todo!("not implemented")
    }
    /// Casing: removes `class` attributes equal to `class` in this subtree.
    pub(crate) fn strip_class(self, class: &str) -> Node {
        todo!("not implemented")
    }
}

/// What a part is, for keys and hover policy. Not written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartKind {
    Node,
    Edge,
    Group,
    /// A data item: a slice, a bar, a point, a legend entry for it.
    Item,
}

/// A part's index in its drawing's part table. Only `Sketch::part` issues one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartId(u32);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Part {
    pub(crate) kind: PartKind,
    /// Decoded id: what `data-id` writes `esc`'d, and what links report.
    pub(crate) id: Arc<str>,
    /// For edges: the parts at either end. Hover lights a node's edges through this.
    pub(crate) ends: Option<(PartId, PartId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(u32);

/// Pele's `LinkInfo`. `id` is decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkInfo {
    pub id: String,
    pub href: String,
    pub internal: bool,
}

/// Accessible title and description (`<title id>`/`<desc id>` + `aria-*`). Decoded.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Accessible {
    pub(crate) title: Option<String>,
    pub(crate) descr: Option<String>,
}

/// `themeVariables` series overrides for this type (`palette()`), in slot order.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Palette {
    pub(crate) series: [Option<CssText>; 8],
    /// `ink(';fill:'+color)` per accepted slot: the label fill on that series.
    pub(crate) inks: [Option<Paint>; 8],
}

/// The mutable state of one draw attempt: the part table and the link table. Pele's
/// `links.push(..)` call sites become `Sketch::link`, in the same order.
pub(crate) struct Sketch<'cx> {
    cx: &'cx DrawCx<'cx>,
    ty: Type,
    parts: Vec<Part>,
    index: std::collections::HashMap<(PartKind, Arc<str>), PartId>,
    links: Vec<LinkInfo>,
}

impl<'cx> Sketch<'cx> {
    /// Interns `(kind, decode(raw_id))`. Idempotent: the same key always returns the same
    /// id, so a slice and its legend item share one part without coordination.
    pub(crate) fn part(&mut self, kind: PartKind, raw_id: &str) -> PartId {
        todo!("not implemented")
    }
    /// An edge part with its ends. Calling it again with other ends is a porting bug
    /// (`debug_assert`).
    pub(crate) fn edge_part(&mut self, raw_id: &str, from: PartId, to: PartId) -> PartId {
        todo!("not implemented")
    }
    /// `seriesInkFill(index)`: the palette's label ink for this series, if it set one.
    pub(crate) fn series_ink(&self, index: usize) -> Option<Paint> {
        self.cx.palette.inks[index % 8].clone()
    }
    /// Pele's `links.push(info)`. The returned id goes on the `<a>` element's meta.
    pub(crate) fn link(&mut self, info: LinkInfo) -> LinkId {
        todo!("not implemented")
    }
    /// `svgDocument(type, width, height, fontSize, options, acc, content)` without writing.
    /// The root's attributes come from here (type, size, font, prefix, responsive, palette).
    pub(crate) fn finish(self, width: f64, height: f64, acc: Accessible, body: Vec<Node>) -> Drawing {
        // TODO: debug_assert every DataId/belongs PartId in body < parts.len() (a PartId
        //       from a losing fit attempt would fail here).
        todo!("not implemented")
    }
}

/// A finished drawing: the Scene plus its tables. Immutable; only `Sketch::finish` makes one.
/// Public as an opaque handle; its structure is crate-private.
#[derive(Debug, Clone)]
pub struct Drawing {
    pub(crate) ty: Type,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) font_size: f64,
    pub(crate) root: Root,
    pub(crate) acc: Accessible,
    pub(crate) palette: Palette,
    pub(crate) parts: Vec<Part>,
    pub(crate) links: Vec<LinkInfo>,
    pub(crate) body: Vec<Node>,
}

/// The root's option-dependent inputs, captured at draw time so `to_svg` takes no options.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Root {
    pub(crate) id_prefix: String,
    pub(crate) responsive: bool,
    pub(crate) font_family: Option<String>,
    pub(crate) font_family_mono: Option<String>,
}

impl Drawing {
    /// Total width in px (Pele's `Math.ceil`ed value). `fit` compares these.
    pub fn width(&self) -> f64 {
        self.width
    }
    pub fn height(&self) -> f64 {
        self.height
    }
    /// Pele's `Rendered.links`, in Pele's push order.
    pub fn links(&self) -> &[LinkInfo] {
        &self.links
    }
    /// Pele's SVG, byte for byte. Pure: the same drawing always writes the same bytes.
    pub fn to_svg(&self) -> String {
        crate::svg::write(self)
    }
    /// The renderer-neutral flattened view (`picture` module).
    pub fn picture(&self) -> crate::picture::Picture {
        crate::picture::flatten(self)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: kit  (Scene-producing ports of Pele's src/svg/*; shared call sites)
// ═══════════════════════════════════════════════════════════════════════════════════════════
// A type never re-encodes these. Each function keeps Pele's attribute order inside.

pub(crate) mod style {
    use super::*;

    /// Author CSS text that a validator accepted. Only `resolve_style` here and
    /// `palette::safe_color` construct it (`pub(in crate::kit)`), so unchecked author text
    /// cannot reach the tree; an import from outside `kit` fails the build.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct CssText(Arc<str>);

    impl CssText {
        pub(in crate::kit) fn checked(s: &str) -> CssText {
            CssText(s.into())
        }
        /// The one literal Pele adds itself: `rx:var(--_r)` from `rounded()`.
        pub(crate) fn radius() -> CssText {
            CssText("var(--_r)".into())
        }
        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    /// `resolveStyle`: Mermaid `style`/`classDef` split into shape, line and text styles.
    /// Map semantics match JS `Map.set` (a repeated property keeps its first position and
    /// takes the last value), so the port uses an insertion-ordered map.
    #[derive(Debug, Clone, Default, PartialEq)]
    pub(crate) struct ResolvedStyle {
        pub(crate) shape: Option<Attr>, // Attr::Style, terminated
        pub(crate) line: Option<Attr>,
        pub(crate) text: Option<Attr>,
        pub(crate) stroke: Option<CssText>,
        pub(crate) bold: bool,
        pub(crate) italic: bool,
        pub(crate) font_size: Option<f64>,
    }
    pub(crate) fn resolve_style(declarations: &[String]) -> ResolvedStyle {
        todo!("not implemented")
    }
    /// `classNames(raw)`: `[^\w-]+` collapsed, a leading space when non-empty.
    pub(crate) fn class_names(raw: &str) -> String {
        todo!("not implemented")
    }
    /// `setsFill(shape)`.
    pub(crate) fn sets_fill(shape: &Option<Attr>) -> bool {
        todo!("not implemented")
    }
}

pub(crate) mod palette {
    use super::*;
    /// `palette(type, config)`: `themeVariables` series overrides, each through `safe_color`.
    pub(crate) fn palette(ty: Type, config: &Config) -> Palette {
        todo!("not implemented")
    }
    /// `safeColor`: allowed characters, balanced parentheses, allow-listed functions,
    /// `var(--pele-*)` only. The only other constructor of `CssText`.
    pub(crate) fn safe_color(value: &Config) -> Option<CssText> {
        todo!("not implemented")
    }
}

pub(crate) mod ink {
    use super::*;
    /// `ink(shape, text)`: ON_LIGHT/ON_DARK for a fill's contrast, or `None`. Pure; Pele's
    /// cache is an optimisation the port may drop.
    pub(crate) fn ink(shape: Option<&Decls>, text: Option<&Decls>) -> Option<Paint> {
        todo!("not implemented")
    }
    pub(crate) fn paints(shape: Option<&Decls>) -> bool {
        todo!("not implemented")
    }
    /// `inkFill(style, fallback)` as an optional fill attribute.
    pub(crate) fn ink_fill(style: &style::ResolvedStyle, fallback: Option<Paint>) -> Option<Attr> {
        todo!("not implemented")
    }
}

pub(crate) mod title {
    use super::*;
    /// `titleLabel`: bold, math off.
    pub(crate) fn title_label(cx: &DrawCx<'_>, title: Option<&str>, size: f64, max_width: f64) -> Arc<Label> {
        todo!("not implemented")
    }
    /// `withTitle`: the title above content wrapped in one `<g transform>`. Returns new size
    /// and body. The content's layers stay nested inside the translate group, so layers are
    /// meta on nodes, not top-level slots.
    pub(crate) fn with_title(
        cx: &DrawCx<'_>,
        title: Option<&str>,
        pad: f64,
        width: f64,
        height: f64,
        content: Vec<Node>,
    ) -> (f64, f64, Vec<Node>) {
        todo!("not implemented")
    }
}

pub(crate) mod shapes {
    use super::*;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub(crate) struct ShapeSize {
        pub(crate) w: f64,
        pub(crate) h: f64,
        pub(crate) dy: f64,
    }
    /// The ~55 shapes, keyed by canonical name. One table drives size, inset, span and draw,
    /// as `SHAPES` does.
    pub(crate) fn size(shape: &str, tw: f64, th: f64) -> ShapeSize {
        todo!("not implemented")
    }
    /// `drawShape(shape, w, h, a, line)`: nodes centred on 0,0. `a` is
    /// `SHAPE_ATTRS + style.shape`; `rect` calls `a.rounded()`, datastore calls
    /// `a.replace(Attr::Stroke(Paint::Role(Role::Border)), Attr::Stroke(Paint::None))`.
    pub(crate) fn draw(shape: &str, w: f64, h: f64, a: Attrs, line: Attrs) -> Vec<Node> {
        todo!("not implemented")
    }
    pub(crate) fn inset(shape: &str, w: f64, h: f64, side: u8, t: f64) -> f64 {
        todo!("not implemented")
    }
    pub(crate) fn span(shape: &str, w: f64, h: f64, sideways: bool) -> f64 {
        todo!("not implemented")
    }
    pub(crate) fn filled(shape: &str) -> bool {
        todo!("not implemented")
    }
    pub(crate) fn has_label(shape: &str) -> bool {
        todo!("not implemented")
    }
}

pub(crate) mod edges {
    use super::*;
    /// `EdgePath` without the `tokens` sidecar: `d` itself is the token list.
    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct EdgePath {
        pub(crate) d: PathData,
        pub(crate) start: [f64; 2],
        pub(crate) start_dir: [f64; 2],
        pub(crate) end: [f64; 2],
        pub(crate) end_dir: [f64; 2],
    }
    /// A filled box edges may lie over: `[l, t, r, b]` and its shape style.
    pub(crate) struct Fill {
        pub(crate) bbox: [f64; 4],
        pub(crate) shape: Option<Decls>,
    }
    /// `routePath`: route triples to M/L/C/Q, trimmed for markers. All numbers `Num`.
    pub(crate) fn route_path(route: &[f64], curve: Option<&str>, start_trim: f64, end_trim: f64, start_reach: f64, end_reach: f64) -> EdgePath {
        todo!("not implemented")
    }
    /// `marker(type, x, y, dx, dy, color, stroke)`: attribute order class, d, fill, stroke;
    /// the cross marker writes neither fill nor stroke.
    pub(crate) fn marker(kind: &str, at: [f64; 2], dir: [f64; 2], color: Paint, stroke: Paint) -> Option<Node> {
        todo!("not implemented")
    }
    /// `overFills`: samples `d` by `N::written` values in Pele's De Casteljau order.
    pub(crate) fn over_fills(path: &EdgePath, fills: &[Fill], line: Option<&Decls>) -> Option<Option<Paint>> {
        // TODO: Some(None) is Pele's '' (crosses a border: casing). None is undefined (off fills).
        todo!("not implemented")
    }
    /// `casing(d, markers, line, thick)`: the markers are `strip_class("pele-marker")`'d and
    /// the stroke width is read with `Decls::get(Prop::StrokeWidth)`.
    pub(crate) fn casing(path: &EdgePath, markers: Vec<Node>, line: Option<&Decls>, thick: bool) -> Option<Node> {
        todo!("not implemented")
    }
    /// `casingsSvg`: the casings layer, `pointer-events="none"`.
    pub(crate) fn casings(casings: Vec<Node>) -> Option<Node> {
        todo!("not implemented")
    }
    /// `edgeLabelSvg`: `<g class="pele-edge-label" data-id>` + background rect + label.
    /// Takes the edge's `PartId`, so the label joins the edge's part in its own layer.
    pub(crate) fn edge_label(part: PartId, label: &Arc<Label>, x: f64, y: f64, attrs: Attrs) -> Node {
        todo!("not implemented")
    }
    pub(crate) fn loop_path(/* as loopPath */) -> EdgePath {
        todo!("not implemented")
    }
}

pub(crate) mod cluster {
    use super::*;
    /// `struckTitle`: the halo pass uses `Paint::Mix { a: Role::SurfaceAlt, pct: num(100 *
    /// (1 - 0.5^depth)), b: Role::Bg }`.
    pub(crate) fn struck_title(label: &Arc<Label>, x: f64, y: f64, attrs: Attrs, part: PartId, depth: u32) -> Node {
        todo!("not implemented")
    }
    pub(crate) fn title_x(/* as clusterTitleX */) -> f64 {
        todo!("not implemented")
    }
}

pub(crate) mod url {
    use super::*;
    /// `linked(body, url, id, links, options, attrs)`: wraps `body` in `<a href target rel>`
    /// and pushes the link in Pele's order, or returns `body` unchanged when the URL is refused.
    pub(crate) fn linked(d: &mut Sketch<'_>, body: Vec<Node>, url: &str, id: &str, target: Option<&str>) -> Vec<Node> {
        todo!("not implemented")
    }
    /// The flowchart `internal-link` wrapper around the label only.
    pub(crate) fn internal(d: &mut Sketch<'_>, text: Node, name: &str, id: &str) -> Node {
        todo!("not implemented")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: fit  (fit-width.ts; identical in both modes)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// `fitWidth`: draw at the natural plot size, then up to 3 shrinking redraws. Each attempt
/// returns a whole `Drawing` built with its own `Sketch`; losers are dropped.
/// Invariant (tested over the corpus): if `max_width >= draw(None).width()`, the result
/// equals the unconstrained result. rune-pele relies on it to skip relayouts.
pub(crate) fn fit_width(cx: &DrawCx<'_>, natural: f64, least: f64, mut draw: impl FnMut(f64, bool) -> Drawing) -> Drawing {
    // TODO: let mut result = draw(natural, false);
    //       let Some(avail) = cx.options.max_width.filter(|a| *a > 0.0) else { return result };
    //       if result.width() <= avail { return result }
    //       let floor = js::min(least, natural); let mut plot = natural; result = draw(plot, true);
    //       for _ in 0..3 { if !(result.width() > avail && plot > floor) { break }
    //           plot = js::max(floor, plot - (result.width() - avail)); result = draw(plot, true); }
    //       result
    todo!("not implemented")
}

/// `turnToFit`: across, then down when allowed, narrower and under the breakpoint.
pub(crate) fn turn_to_fit(cx: &DrawCx<'_>, draw: impl FnMut(bool) -> Result<Drawing, PeleError>) -> Result<Drawing, PeleError> {
    todo!("not implemented")
}

/// `tighten`: loose, then tight when loose overflows and tight is narrower.
pub(crate) fn tighten(cx: &DrawCx<'_>, draw: impl FnMut(bool) -> Result<Drawing, PeleError>) -> Result<Drawing, PeleError> {
    todo!("not implemented")
}

/// `titleRoom`.
pub(crate) fn title_room(cx: &DrawCx<'_>) -> f64 {
    todo!("not implemented")
}

/// `capWidth`.
pub(crate) fn cap_width(cx: &DrawCx<'_>, natural: f64, least: f64, around: f64) -> f64 {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: svg  (the one writer; generic, no per-type code)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// The whole document. The body is written first and then scanned. The root style is
/// derived from those bytes exactly as `svgDocument` derives it from `content`.
pub(crate) fn write(d: &Drawing) -> String {
    // TODO: let mut body = String::new();
    //       write_acc(&mut body_head, d)   -> <title id="{p}-title">… <desc id="{p}-desc">…
    //       for n in &d.body { write_node(&mut body, d, n) }
    //       let used = scan_used(&body);    -> {r, tw, hw, w} by RE `var\(--_(r|tw|hw|w)\)`
    //       root = `<svg xmlns=… class="pele pele-{ty}" viewBox="0 0 {W} {H}" width="{W}"
    //               height="{H}" style="{fit}{ROOT_STYLE}{weights(used)}{esc(font_style)}{palette}"
    //               fill="var(--_fg)" role="graphics-document document"
    //               aria-roledescription="{ty}"{aria}>` + head + body + `</svg>`
    //       W, H via js::to_string; palette = esc(";--pele-series-N:{css}" for accepted slots).
    todo!("not implemented")
}

fn write_node(out: &mut String, d: &Drawing, node: &Node) {
    // TODO: Elem: `<{tag}` + attrs in order + (void ? "/>" : ">" + children + "</tag>")
    //       Text: esc. Label: write_label. Icon: the fixed pele-icon attribute string.
    todo!("not implemented")
}

fn write_attr(out: &mut String, d: &Drawing, attr: &Attr) {
    // TODO: ` name="value"`; N::Num via js::write_num, N::Raw via js::to_string;
    //       DataId(p) → esc(d.parts[p].id); Paint per its variant (see scene::Paint docs);
    //       Style(decls) → `style="` + esc(join(prop:value, ";") + if terminated ";") + `"`;
    //       PathData per Cmd; Points "x,y x,y"; Transform space-joined ops;
    //       StrokeDasharray space-joined; FontSize::Smaller → "smaller".
    todo!("not implemented")
}

/// `labelSvg`: fast path when one line has one span with style 0 and no icon or math.
/// Otherwise `textStyle` hoisting (skipped per attribute already present), tspans (first span
/// of a line gets x/y; blank line `<tspan x y> </tspan>`; placed spans `text-anchor="start"`),
/// then icons and math as siblings after `</text>`. `mathLook` reads `attrs.fill()` and
/// `attrs.style()` instead of regexes.
fn write_label(out: &mut String, d: &Drawing, node: &LabelNode) {
    todo!("not implemented")
}

/// Pele's `esc`: drops C0 controls (but tab, LF, CR), DEL, U+FFFE/U+FFFF and lone
/// surrogates; escapes `& < > "`; leaves `'`.
pub(crate) fn esc(out: &mut String, text: &str) {
    todo!("not implemented")
}

fn scan_used(body: &str) -> [bool; 4] {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: paint  (renderer-neutral colour: CSS parsing, theme resolution)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Gamma-encoded sRGB, straight alpha, 0..1. CSS's space; Rune converts at its edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Srgba(pub [f32; 4]);

/// A paint expression, parsed once by `picture` and theme-independent. Rune resolves it per
/// frame without string work.
#[derive(Debug, Clone, PartialEq)]
pub enum Ink {
    Role(crate::scene::Role),
    Series(u8),
    Token(crate::scene::Token),
    /// Author colours parsed to sRGB. `currentColor` never appears: the cascade resolves it
    /// to the element's `color` first.
    Rgba(Srgba),
    /// `color-mix(in srgb, a pct%, b)`.
    Mix { a: Box<Ink>, pct: f64, b: Box<Ink> },
    /// `light-dark(a, b)`.
    LightDark(Box<Ink>, Box<Ink>),
}

/// What every role and token resolves to. Colours only (plus the corner radius): fonts
/// and weights change measurement and belong to the layout key, not here.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Indexed by `Role as usize`.
    pub roles: [Srgba; 8],
    pub series: [Srgba; 8],
    pub critical: Srgba,
    pub on_light: Srgba,
    pub on_dark: Srgba,
    pub error: Srgba,
    /// `--pele-radius`, in px.
    pub radius: f64,
    pub dark: bool,
}

impl Theme {
    /// Pele's fallbacks: `#fff`, `#1f1f1f`, `#6e6e6e`, …, series `#4c78a8` …, radius 4.
    pub const PELE: Theme = Theme {
        roles: [Srgba([1.0; 4]); 8], // TODO: the eight ROOT_STYLE fallbacks
        series: [Srgba([0.0; 4]); 8], // TODO: SERIES
        critical: Srgba([0.0; 4]),
        on_light: Srgba([0.0; 4]),
        on_dark: Srgba([1.0; 4]),
        error: Srgba([0.0; 4]),
        radius: 4.0,
        dark: false,
    };
}

impl Ink {
    /// The colour under `theme`. `Mix` is CSS `color-mix(in srgb, …)`: interpolation in
    /// gamma-encoded sRGB with premultiplied alpha, not Rune's linear-light `Color::mix`.
    pub fn resolve(&self, theme: &Theme) -> Srgba {
        todo!("not implemented")
    }
}

/// CSS colour text to `Ink`: hex, named, rgb/rgba, hsl/hsla, hwb, lab/lch, oklab/oklch,
/// `color-mix`, `light-dark`, `transparent`, `var(--pele-*)` and `var(--_*)` (mapped to
/// roles and tokens). `None` for anything a browser would reject, so the cascade falls
/// back to the attribute, as a browser drops an invalid declaration.
pub(crate) fn parse_color(text: &str) -> Option<Ink> {
    todo!("not implemented")
}

/// CSS `<length>` (`4`, `4px`, `calc(2.5px + 4px)` for casing widths) in px.
pub(crate) fn parse_length(text: &str) -> Option<f64> {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: picture  (SVG semantics resolved once: the renderer-neutral view)
// ═══════════════════════════════════════════════════════════════════════════════════════════

use crate::paint::Ink;
use crate::scene::{Anchor, Cap, Join, LinkId, LinkInfo, PartId, PartKind, Shift};

/// A drawing flattened: absolute geometry, explicit presentation, items in paint order, each
/// tagged with its part, layer and link. Theme-independent. Carries no strings for paints.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub ty: crate::Type,
    pub size: [f64; 2],
    /// Layer names in first-seen paint order; `Item::layer` indexes this.
    pub layers: Vec<&'static str>,
    pub items: Vec<Item>,
    pub parts: Vec<PartInfo>,
    pub links: Vec<LinkInfo>,
    pub title: Option<String>,
    pub descr: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub part: Option<PartId>,
    pub layer: usize,
    pub link: Option<LinkId>,
    /// Product of ancestor and own `opacity`.
    pub opacity: f64,
    /// False under `pointer-events="none"` (casings): never a hover or press target.
    pub hit: bool,
    pub what: What,
}

#[derive(Debug, Clone, PartialEq)]
pub enum What {
    Shape(ShapeItem),
    Text(TextItem),
    Icon(IconItem),
    Image(ImageItem),
    Math(MathItem),
}

/// Any rect, circle, ellipse, polygon or path, as contours.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeItem {
    pub outline: Outline,
    pub fill: Option<FillPaint>,
    pub stroke: Option<StrokePaint>,
}

/// Absolute coordinates; lines and cubics only (arcs and quads converted, transforms
/// applied). Non-zero fill rule (Pele never sets `fill-rule`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outline {
    pub contours: Vec<Contour>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub start: [f64; 2],
    pub segs: Vec<Segment>,
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line([f64; 2]),
    Cubic([f64; 2], [f64; 2], [f64; 2]),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FillPaint {
    Solid { ink: Ink, opacity: f64 },
    /// A resolved `url(#id)`: user-space x1→x2 stops.
    Linear { x1: f64, x2: f64, stops: Vec<(f64, Ink)>, opacity: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrokePaint {
    pub ink: Ink,
    pub opacity: f64,
    pub width: f64,
    pub dash: Option<Dash>,
    pub cap: Cap,
    pub join: Join,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dash {
    pub array: Vec<f64>,
    pub offset: f64,
}

/// Both of Pele's text forms, and hand-built `<text>`/`<tspan>`, become this.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    pub lines: Vec<TextLine>,
    /// Absolute rotation in radians about `pivot` (git, quadrant, xychart, swimlane).
    pub rotate: f64,
    pub pivot: [f64; 2],
    /// A struck cluster title's halo pass: stroke colour and width under fill none.
    pub halo: Option<(Ink, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    /// The anchor point: x per `anchor`, and the line's visual middle in y. For a Label
    /// that is Pele's `mid`. For raw text it is `y - size * text::SHIFT`.
    pub at: [f64; 2],
    pub anchor: Anchor,
    /// Pele's line box height (`round(size*1.5)`, or 1.3 when wrapped, or the math height).
    pub line_height: f64,
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub size: f64,
    pub weight: RunWeight,
    pub italic: bool,
    pub mono: bool,
    pub underline: bool,
    pub shift: Shift,
    pub fill: Option<Ink>,
    /// Known when the run came from a measured Label; `None` for raw text (measure it).
    pub width: Option<f64>,
    /// Some when the run is positioned on its own (placed spans, explicit tspan x).
    pub left: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunWeight {
    Normal,
    Bold,
    Title,
    Heading,
    Numeric(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct IconItem {
    pub name: String,
    pub markup: Option<std::sync::Arc<str>>,
    pub rect: [f64; 4],
    /// `currentColor` as the cascade resolved it.
    pub color: Ink,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageItem {
    pub href: String,
    pub rect: [f64; 4],
    pub meet: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MathItem {
    pub markup: crate::MathMarkup,
    pub rect: [f64; 4],
    pub color: Option<Ink>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartInfo {
    pub kind: PartKind,
    pub id: String,
    pub ends: Option<(PartId, PartId)>,
    /// The `<title>` tooltip found in this part's subtree, wherever the type placed it.
    pub tip: Option<String>,
}

impl Picture {
    /// The accessible name of a part, derived from its text runs (an edge: "A to B: label").
    pub fn name(&self, part: PartId) -> String {
        todo!("not implemented")
    }
}

/// The SVG user-agent half of the design, for the closed vocabulary.
pub(crate) fn flatten(d: &crate::Drawing) -> Picture {
    // TODO: walk the tree depth-first with a Computed cascade:
    //   root: fill=Role::Fg, color=Role::Fg, stroke=None, stroke-width=1, cap=Butt, join=Miter,
    //         font-size=d.font_size, anchor=Start, weight=Normal, opacity=1, affine=identity.
    //   per element: apply presentation attrs in order, then `style` decls in order (CSS
    //         beats attributes); a decl whose value fails parse_color/parse_length is skipped.
    //   transform: affine *= ops. part = DataId part ?? meta.part ?? inherited. layer from
    //         meta.layer (intern name). link from <a> meta. pointer-events none → hit=false.
    //   Title child → parts[part].tip. Defs/LinearGradient/Stop → gradient table by id.
    //   Shapes → Outline (rect+rx/ry and circle/ellipse as cubic arcs; polygon closed;
    //         path via geom::arc_to_cubics for A, quad→cubic; H/V/relative resolved) × affine.
    //   Paint → Ink: Role/Series(palette override first)/Token/Css(parse_color)/Mix/Url(defs or fallback).
    //   Label → TextItem via text::place (same arithmetic as the writer); raw text/tspan → TextItem
    //         with mid = y - size*SHIFT, width None.
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: geom  (pure lowering maths; policy lives in the adapter)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// SVG endpoint arc to at most 4 cubics (one per ≤90°), after SVG's radius correction.
pub fn arc_to_cubics(from: [f64; 2], rx: f64, ry: f64, large: bool, sweep: bool, to: [f64; 2]) -> Vec<[[f64; 2]; 3]> {
    todo!("not implemented")
}

/// Contours to polylines within `tolerance` (Rune's is 0.1 pt; 1–256 segments per curve).
pub fn flatten(outline: &crate::picture::Outline, tolerance: f64) -> Vec<(Vec<[f64; 2]>, bool)> {
    todo!("not implemented")
}

/// A polyline cut into dash pieces by `array` (repeated if odd) starting at `offset`.
pub fn dash(points: &[[f64; 2]], closed: bool, array: &[f64], offset: f64) -> Vec<Vec<[f64; 2]>> {
    todo!("not implemented")
}

/// Non-zero fill of several contours as simple polygons: each hole is split from its outer
/// contour by a vertical slab cut, and pieces overlap by `overlap` across cuts so
/// antialiased edges do not seam.
pub fn simple_polygons(contours: &[Vec<[f64; 2]>], overlap: f64) -> Vec<Vec<[f64; 2]>> {
    todo!("not implemented")
}

/// The ring a mitered stroke of `width` covers on a closed polygon, as simple polygons.
pub fn stroke_ring(points: &[[f64; 2]], width: f64, miter_limit: f64) -> Vec<Vec<[f64; 2]>> {
    todo!("not implemented")
}

/// A polygon clipped into vertical bands between `x0` and `x1` of at most `step` width
/// (Sutherland–Hodgman per band), with each band's centre `t` in 0..1, for path gradients.
pub fn bands(polygon: &[[f64; 2]], x0: f64, x1: f64, step: f64, max: usize) -> Vec<(f64, Vec<[f64; 2]>)> {
    todo!("not implemented")
}

pub fn bounds(points: &[[f64; 2]]) -> [f64; 4] {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: layout  (compound.ts, layered.ts, lanes.ts, components.ts)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// `CNode`: centre, size, parent, group padding, direction, span. Mutated by layout.
pub(crate) struct CNode {/* fields as compound.ts:10 */}
/// `CEdge`: ends, min length, label box, route triples and label position.
pub(crate) struct CEdge {/* fields as compound.ts:28 */}
pub(crate) struct LayeredOptions {/* nodeSep, edgeSep, rankSep, portSep, tracks, straight, headRoom */}
pub(crate) struct CompoundResult {
    pub(crate) width: f64,
    pub(crate) height: f64,
}
/// Deterministic, no prior layout (parity). Depth-counted recursion (`Limits::depth`).
pub(crate) fn compound_layout(nodes: &mut [CNode], edges: &mut [CEdge], dir: Dir, opt: &LayeredOptions, depth: usize) -> Result<CompoundResult, PeleError> {
    todo!("not implemented")
}
#[derive(Clone, Copy)]
pub(crate) enum Dir {
    TB,
    BT,
    LR,
    RL,
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: diagrams::pie  (the first port; see design.md Usage 3 for draw)
// ═══════════════════════════════════════════════════════════════════════════════════════════

use crate::scene::{Anchor as A_, Attrs as Attrs_, Join as J_, Node as N_, Paint as Pt, PartKind as PK, Role as R_, Tag as T_, Txt as Tx, Weight as W_};

const RADIUS: f64 = 120.0;
const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const GAP: f64 = 32.0;
const OUTSIDE: f64 = 28.0;

pub(crate) struct Pie;

/// `PieModel`: sections in first-occurrence order (an insertion-ordered map; the first value
/// and position of a duplicate label win), title, accessible title/description, showData.
pub(crate) struct PieModel {
    pub(crate) sections: Vec<(String, f64)>,
    pub(crate) show_data: bool,
    pub(crate) title: Option<String>,
    pub(crate) acc: Accessible,
}

impl Port for Pie {
    type Model = PieModel;
    fn parse(src: &Source<'_>) -> Result<PieModel, PeleError> {
        // TODO: hand-written Langium-equivalent parser (parser.ts), then buildPie: Number(token),
        //       negative → Semantic error, title = ast.title || src.title, runs of spaces/tabs
        //       collapsed in titles.
        todo!("not implemented")
    }
    fn draw(m: &PieModel, cx: &DrawCx<'_>) -> Result<Drawing, PeleError> {
        // Unqualified `draw` is the free function below: methods are not in scope unqualified.
        Ok(fit::fit_width(cx, RADIUS * 2.0, 140.0, |diameter, tight| draw(m, cx, diameter / 2.0, tight)))
    }
}

/// render.ts `draw`, line for line. See design.md Usage 3 for the body.
fn draw(m: &PieModel, cx: &DrawCx<'_>, radius: f64, tight: bool) -> Drawing {
    todo!("not implemented")
}

/// The outside-label nudge: step `2 / rim` radians, up to 360 steps, until the label clears
/// every label already placed (`|dx| >= (w + w') / 2 + 2 || |dy| >= small + 2`). Pushes the
/// placed box and returns its centre.
fn nudge_clear(mid: f64, rim: f64, placed: &mut Vec<(f64, f64, f64)>, width: f64) -> (f64, f64) {
    todo!("not implemented")
}

/// render.ts `slice`: every number through `num`, arc flags as bits, rotation `0`.
fn slice(start: f64, end: f64, outer: f64, inner: f64) -> PathData {
    // TODO: full circle (end-start >= 2π-1e-9): ring(outer, sweep 1) + (inner>0 ? ring(inner, 0)).
    //       ring(r, s) = M0,-r A r,r 0 1 s 0,r A r,r 0 1 s 0,-r Z.
    //       else large = end-start > π; inner<=0: M0,0 L point(start,outer) A … point(end,outer) Z
    //       else outer arc, L point(end,inner), inner arc back (sweep 0), Z.
    //       point(a, r) = (num(js::sin(a)*r), num(-js::cos(a)*r)).
    todo!("not implemented")
}

/// The legend: `<g class="pele-legend-item" data-id>` + swatch rect (x y width height rx
/// fill) + label. `d.part(Item, name)` returns the slice's part: same id, other layer.
fn legend_items(d: &mut Sketch<'_>, m: &PieModel, labels: &[Arc<Label>], lx: f64, ly: f64) -> Vec<N_> {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-core, module: diagrams::flowchart  (next: the shape of one node and one edge)
// ═══════════════════════════════════════════════════════════════════════════════════════════

pub(crate) struct Flowchart;

pub(crate) struct FlowDb {/* as db.ts: vertices, edges, subgraphs, classes, direction, title, acc */}

impl Port for Flowchart {
    type Model = FlowDb;
    const MATH: bool = true;
    fn parse(src: &Source<'_>) -> Result<FlowDb, PeleError> {
        todo!("not implemented")
    }
    fn draw(db: &FlowDb, cx: &DrawCx<'_>) -> Result<Drawing, PeleError> {
        // runsAcross(db.direction) ? turnToFit(fitted) : fitted(false), fitted = tighten(draw).
        if runs_across(db) {
            turn_to_fit(cx, |down| tighten(cx, |tight| draw_flow(db, cx, down, tight)))
        } else {
            tighten(cx, |tight| draw_flow(db, cx, false, tight))
        }
    }
}

fn runs_across(db: &FlowDb) -> bool {
    todo!("not implemented")
}

/// render.ts `draw`. Layer order: title, clusters, casings, edges, cluster-titles,
/// edge-labels, nodes (nodes always present, the rest only when non-empty).
fn draw_flow(db: &FlowDb, cx: &DrawCx<'_>, turned: bool, tight: bool) -> Result<Drawing, PeleError> {
    // A node, as render.ts writes it (illustrative; names follow the TS):
    //   let part = d.part(PK::Node, &node.id);
    //   let a = Attrs_::new().fill_paint(R_::Surface).stroke_paint(R_::Border).extend(view.style.shape.clone());
    //   let mut body = kit::shapes::draw(&view.shape, w, h, a, line_attrs);
    //   body.push(label(&view.label, 0.0, view.dy, Attrs_::new().class("pele-label").extend(ink).extend(view.style.text.clone())));
    //   if let Some(tip) = &node.tooltip { body.insert(0, el(T_::Title).text(Tx::decoded(tip)).into()); } // tooltip first: flowchart's position
    //   if let Some(url) = &node.link { body = kit::url::linked(&mut d, body, url, &node.id, node.link_target.as_deref()); }
    //   nodes.push(el(T_::G).class(format!("pele-node pele-shape-{}{}", view.shape, classes)).data_id(part)
    //       .transform(translate(num(c.x), num(c.y))).children(body));
    // An edge and its label, two layers, one part:
    //   let e = d.edge_part(&edge.id, parts[src], parts[dst]);
    //   edges_out.push(el(T_::G).class(..).data_id(e) [.stroke(over)] [.attrs(style.line)]
    //       .child(el(T_::Path).d(path.d.clone()) [.stroke_width(raw(2.5)) | dasharray [3,4]])
    //       .children(marker(start..)).children(marker(end..)));
    //   labels_out.push(kit::edges::edge_label(e, &label, ce.label_x, ce.label_y, text_attrs));
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: pele-cli, module: main  (only the parity match changes)
// ═══════════════════════════════════════════════════════════════════════════════════════════

fn parity_one(source: &str, want: &str, width: f64, height: f64, tally: &mut Tally) {
    match pele_core::render(source) {
        Err(pele_core::RenderError::NotPorted(_)) => tally.not_ported += 1,
        // Pele drew this source and the port refused it: a parity failure.
        Err(pele_core::RenderError::Pele(_)) => tally.different += 1,
        Ok(ours) => {
            if ours.svg == want && ours.width == width && ours.height == height {
                tally.identical += 1
            } else {
                tally.different += 1
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: rune-pele, module: lib  (the only crate that depends on `rune` and `rune-mermaid`)
// ═══════════════════════════════════════════════════════════════════════════════════════════
//! Mermaid diagrams as native Rune elements: Pele's layout, Rune's text, the app's theme.
//! One builder; everything else (detection, Pele's width adaptation, caching, lowering,
//! theming, hover, links, accessibility, streaming, the rune-mermaid fallback) is inside.

use rune::{Color, El, Family, Tx};
use std::rc::Rc;

mod cache;
mod fallback;
mod lower;
mod measure;
mod paint;

/// A Mermaid diagram for a view function. Generic over the app state `S` its link handler
/// takes; `()` until `on_link` names it.
pub struct Diagram<'a, S = ()> {
    id: String,
    source: &'a str,
    theme: Option<&'a Theme>,
    fonts: Fonts,
    streaming: bool,
    hover: Hover,
    on_link: Option<Rc<dyn Fn(&mut S, &Link, &mut Tx)>>,
    on_error: OnError,
}

impl<'a> Diagram<'a, ()> {
    /// `id` scopes the memo and every element key. It must be unique among its siblings.
    pub fn new(id: impl Into<String>, source: &'a str) -> Self {
        todo!("not implemented")
    }
}

impl<'a, S: 'static> Diagram<'a, S> {
    /// Colours. A change repaints from the cached layout (no relayout). Default:
    /// `Theme::neutral(false)`.
    pub fn theme(self, theme: &'a Theme) -> Self {
        todo!("not implemented")
    }
    /// Families, size and bold weight. A change relays out: they decide every width.
    pub fn fonts(self, fonts: Fonts) -> Self {
        todo!("not implemented")
    }
    /// The source is still arriving. Draw the longest prefix of complete lines that parses
    /// (searching back at most 8 lines), and let new parts come in while the rest glide.
    pub fn streaming(self, on: bool) -> Self {
        todo!("not implemented")
    }
    pub fn hover(self, hover: Hover) -> Self {
        todo!("not implemented")
    }
    /// Makes links pressable. Without it, links stay accessible and show their href as a
    /// tooltip, but do nothing on press.
    pub fn on_link<T: 'static>(self, f: impl Fn(&mut T, &Link, &mut Tx) + 'static) -> Diagram<'a, T> {
        todo!("not implemented")
    }
    pub fn on_error(self, e: OnError) -> Self {
        todo!("not implemented")
    }
    /// Always an element. Pele-ported types draw natively; others use rune-mermaid when it
    /// supports them; anything else shows `on_error`.
    pub fn el(self) -> El {
        // TODO: frame = fill-width container keyed "{id}/frame"; width = rune::layout_size(frame key)
        //       src = if streaming { cache::drawable_prefix(source) } else { source }
        //       match pele_core::detect(src) {
        //         Some(t) if t.is_ported() => native(..),
        //         _ if rune_mermaid::supported(src) => fallback::rune_mermaid(src, theme, fonts, hover, streaming, width),
        //         _ => fallback::error_card(..) }
        //       native: let p = cache::prepared(src, &fonts, width)?;   (layout + lower, cached)
        //               rune::memo(&self.id, (p.key, theme.hash(), self.hover), || paint::el(&p, theme, look, on_link))
        todo!("not implemented")
    }
}

/// Pele's roles, series and tokens as Rune colours. `Hash` covers every field, so it is the
/// memo version for paint.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub line: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub accent: Color,
    pub series: [Color; 8],
    pub critical: Color,
    pub on_light: Color,
    pub on_dark: Color,
    pub error: Color,
    pub radius: f32,
    pub dark: bool,
}

impl Theme {
    /// From `rune::tok::theme(dark)`: bg, text, text_secondary, separator, card, indicator,
    /// with Pele's series palette.
    pub fn neutral(dark: bool) -> Self {
        todo!("not implemented")
    }
    pub fn accent(self, c: Color) -> Self {
        todo!("not implemented")
    }
    pub fn series(self, palette: [Color; 8]) -> Self {
        todo!("not implemented")
    }
    /// The core theme in sRGB, where `color-mix` is defined.
    pub(crate) fn core(&self) -> pele_core::paint::Theme {
        todo!("not implemented")
    }
    pub(crate) fn hash64(&self) -> u64 {
        todo!("not implemented")
    }
}

/// What measurement depends on. Part of the layout cache key; never of the paint key.
/// Hashed by `f32::to_bits` (a manual `Hash` impl), as rune-mermaid hashes `DiagramStyle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fonts {
    pub sans: Family,
    pub mono: Family,
    /// Pele's `fontSize`, in points (1 px = 1 pt).
    pub size: f32,
    pub bold: f32,
}

impl Fonts {
    /// `Family::Sans` / `Family::SystemMono`, 14 pt, bold 700.
    pub fn rune() -> Self {
        todo!("not implemented")
    }
}

/// What hovering lights up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Hover {
    Off,
    /// The part under the pointer, across all its layers (an edge and its label).
    #[default]
    Parts,
    /// A node's edges and their labels as well (through `PartInfo::ends`).
    Edges,
    /// Edges and the nodes they reach.
    Neighbours,
}

/// A link a press reported.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub id: String,
    pub href: String,
    pub internal: bool,
}

impl Link {
    /// Pele's default for external links: open in the browser.
    pub fn open(&self, tx: &mut Tx) {
        todo!("not implemented") // let _ = tx.open_url(&self.href);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OnError {
    /// A small card with Pele's message and line:column.
    #[default]
    Message,
    /// The source as a code block.
    Source,
    Nothing,
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: rune-pele, module: measure
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Pele's measurer seam answered by Rune's text system. Lines are still broken by Pele's
/// code in core; Rune only answers widths, so labels never re-wrap at paint time.
pub(crate) struct RuneMeasure {
    pub(crate) fonts: Fonts,
}

impl pele_core::Measure for RuneMeasure {
    fn width(&self, text: &str, size: f64, style: pele_core::Style) -> f64 {
        // TODO: size' = if SUB|SUP { size / 1.2 } else { size };
        //       ts = TextStyle::new(size', if BOLD { fonts.bold } else { 400 }).family(if MONO { mono } else { sans }).italic(ITALIC);
        //       rune::text::advance(text, ts) as f64
        todo!("not implemented")
    }
    fn epoch(&self) -> u64 {
        rune::text::font_epoch()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: rune-pele, module: cache  (content-addressed, so sharing it is harmless)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Everything a layout depends on. Two diagrams with the same key share one entry, and both
/// would compute the same value, so concurrent use needs no coordination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LayoutKey {
    pub(crate) source: u64,
    pub(crate) fonts: u64,
    pub(crate) epoch: u64,
    /// `None`: the free layout. `Some(w)`: the frame width quantised to 8 pt steps, used
    /// only when the frame is narrower than the free layout (core's `fit` invariant).
    pub(crate) width: Option<u32>,
}

/// The lowered drawing for `source` at the frame width. Thread-local LRU of 32 entries.
/// Inserted only after a successful build inside `catch_unwind`, so a crash or error leaves
/// nothing half-built behind.
pub(crate) fn prepared(source: &str, fonts: &Fonts, frame_width: Option<f32>) -> Result<std::rc::Rc<lower::Prepared>, pele_core::RenderError> {
    // TODO: parsed = parse cached by source hash;
    //       free = lay out with max_width None (cached);
    //       if frame_width.map_or(true, |w| w >= free.width) { return free }
    //       else lay out with Some(quantise(w)) (cached) and lower.
    //       options: Options { font_size: fonts.size, max_width, ..Options::default() }, measure: RuneMeasure.
    todo!("not implemented")
}

/// The longest prefix of complete lines that parses, searching back at most 8 lines. A
/// pure function of the input, so a repeat finds the same answer (and a cache hit).
pub(crate) fn drawable_prefix(source: &str) -> &str {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: rune-pele, module: lower  (Rune policy over pele_core::geom; theme-independent)
// ═══════════════════════════════════════════════════════════════════════════════════════════

use pele_core::paint::Ink as Ink_;
use pele_core::picture::{Picture, What};

/// A Picture made drawable by Rune: points ready for `Shape::fill`/`stroke_with`, paints
/// still `Ink` (resolved per theme in `paint`), indexes for hover. Cached with the layout.
pub(crate) struct Prepared {
    pub(crate) key: LayoutKey,
    pub(crate) size: [f32; 2],
    /// Paint order. Text sits between shapes where the tree put it.
    pub(crate) slots: Vec<Slot>,
    /// Per `PartId`: its slots across layers, its label, its tip, its link.
    pub(crate) parts: Vec<PartSlots>,
    /// Per node part: the edge parts that end at it (from `PartInfo::ends`).
    pub(crate) node_edges: Vec<Vec<pele_core::PartId>>,
    pub(crate) links: Vec<pele_core::LinkInfo>,
    pub(crate) title: Option<String>,
    pub(crate) descr: Option<String>,
}

pub(crate) enum Slot {
    /// One `canvas` El: consecutive shape items of one part and layer, local to `origin`.
    Piece(Piece),
    Text(TextPiece),
    Image { href: String, rect: [f32; 4], key: String },
}

pub(crate) struct Piece {
    /// `"{kind}:{id}@{layer}#{n}"`, or `"@{layer}#{n}"` for items with no part.
    pub(crate) key: String,
    pub(crate) origin: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) ops: Vec<PreOp>,
    /// Set for a translucent stroke drawn opaque in its own piece, so joints never double-blend.
    pub(crate) opacity: f32,
    pub(crate) part: Option<pele_core::PartId>,
    pub(crate) hit: bool,
}

pub(crate) enum PreOp {
    Fill { polygon: Vec<[f32; 2]>, ink: Ink_, alpha: f32 },
    Stroke { points: Vec<[f32; 2]>, closed: bool, width: f32, cap: rune::LineCap, ink: Ink_, alpha: f32 },
    /// A gradient band: a solid fill whose ink is the gradient sampled at `t`.
    Band { polygon: Vec<[f32; 2]>, stops: std::rc::Rc<Vec<(f64, Ink_)>>, t: f64, alpha: f32 },
}

pub(crate) struct TextPiece {
    pub(crate) key: String,
    /// Left edge (anchor applied; raw runs measured with `advance`) and line-box top.
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) line_height: f32,
    /// One Rune text per run group. Runs with no baseline shift share an El via `Span`s; a
    /// sub/sup run is its own El at size/1.2, shifted −0.33 em or +0.2 em.
    pub(crate) runs: Vec<pele_core::picture::Run>,
    pub(crate) rotate: f32,
    pub(crate) pivot: [f32; 2],
    pub(crate) halo: Option<(Ink_, f32)>,
    pub(crate) part: Option<pele_core::PartId>,
}

pub(crate) struct PartSlots {
    pub(crate) slots: Vec<usize>,
    pub(crate) name: String,
    pub(crate) tip: Option<String>,
    pub(crate) link: Option<pele_core::LinkId>,
}

/// Rune policy: arcs are already cubics; flatten at 0.1 pt; dashes via `geom::dash`; closed
/// strokes ≥ 2 pt with miter joins via `geom::stroke_ring`, thinner ones round; multi-contour
/// fills via `geom::simple_polygons(.., 0.25)`; path gradients via `geom::bands(.., 4.0, 64)`;
/// translucent strokes into their own piece with opacity; batch by (part, layer) runs.
pub(crate) fn prepare(picture: &Picture, key: LayoutKey) -> Prepared {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: rune-pele, module: paint  (Prepared + Theme → El; the only per-theme work)
// ═══════════════════════════════════════════════════════════════════════════════════════════

pub(crate) struct Look {
    pub(crate) hover: Hover,
    pub(crate) hover_color: Color,
}

/// Builds the diagram El inside `memo(id, (key, theme, hover))`. Absolute, keyed siblings
/// in paint order, so streaming edits animate by key and labels interleave with shapes.
pub(crate) fn el<S: 'static>(p: &Prepared, theme: &Theme, look: &Look, id: &str, on_link: Option<Rc<dyn Fn(&mut S, &Link, &mut Tx)>>) -> El {
    // TODO: core = theme.core();
    //       root = stack sized p.size, .role("img"), .label(p.title ?? type name), help(descr)
    //       for slot in p.slots:
    //         Piece → canvas(Shape from ops with to_rune(ink.resolve(&core)) * alpha)
    //                  .absolute(origin).size(size).opacity(piece.opacity).key("{id}/{key}")
    //         Text  → text(run text).font(TextStyle{size, weight, family, italic, line_height})
    //                  .spans(colour/weight/underline ranges).single_line().w(width + 2.0)
    //                  .absolute(left, top).rotate(..).origin(..).key(..)  (halo: 8 offset copies)
    //       per part (look.hover != Off): hit group over its hit slots' bounds that shows the
    //         part (and node_edges under Edges/Neighbours) lit in hover_color; .help(tip);
    //         link: .on_press_tx(move |s, tx| f(s, &link, tx)).cursor(Cursor::Pointer)
    //         .role("link").label(name).
    todo!("not implemented")
}

/// Gamma sRGB to Rune's linear `Color` (`Color` is linear with straight alpha).
pub(crate) fn to_rune(c: pele_core::paint::Srgba) -> Color {
    todo!("not implemented")
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// crate: rune-pele, module: fallback
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Types rune-pele has not ported, drawn by Rune's own `rune-mermaid` (17 types). No list of
/// those types is kept here: `rune_mermaid::supported(src)` is asked at runtime.
pub(crate) fn rune_mermaid(src: &str, theme: &Theme, fonts: &Fonts, hover: Hover, streaming: bool, width: Option<f32>) -> Option<El> {
    // TODO: let mut style = rune_mermaid::DiagramStyle::neutral(theme.dark);
    //       style.text = theme.fg; style.muted = theme.muted; style.surface = theme.bg;
    //       style.surface_border = theme.border; style.node_fill = theme.surface;
    //       style.node_border = theme.border; style.line = theme.line;
    //       style.cluster_fill = theme.surface_alt; style.font = TextStyle::new(fonts.size, 400.0);
    //       style.max_width = width.unwrap_or(680.0);
    //       let h = match hover { Off => rune_mermaid::Hover::Off, Neighbours => Neighbours, _ => Edges };
    //       if streaming { render_streaming(src, &style, h, theme.accent) } else { render_with_hover(..) }
    todo!("not implemented")
}

pub(crate) fn error_card(src: &str, error: &pele_core::RenderError, on_error: OnError, theme: &Theme) -> El {
    todo!("not implemented")
}
