//! Rune adapter for rune-pele. This is the only workspace crate that depends on Rune.
//!
//! A [`Diagram`] draws Mermaid source as native Rune elements: the Scene pele-core lays out,
//! measured with Rune's own shaper, lowered to keyed shapes and text in Pele's paint order.
//!
//! ```ignore
//! let chart = Diagram::new("deploy", FontCatalog::rune());
//! fn view(app: &App, cx: &mut Cx<App>) -> El {
//!     app.chart
//!         .view_with(cx, &app.source, ViewOptions::new(480.0), |app: &mut App, event, tx| match event {
//!             Event::Activate(link) => app.follow(link, tx),
//!         })
//!         .unwrap_or_else(|error| text(error.to_string()))
//! }
//! ```
//!
//! [`lower::NATIVE`] lists the diagram types drawn this way. Any other type, and any source whose
//! Scene asks for something the lowering does not draw, is drawn by Rune's own rune-mermaid.
//!
//! Layout is cached by source, layout options, device scale and Rune's font epoch, and painting
//! is a `memo` keyed by the layout and the resolved colours. So a palette or dark-mode change
//! repaints without parsing, laying out or measuring anything.

pub mod bands;
pub mod capture;
pub mod fill;
pub mod geometry;
pub mod lower;
pub mod stroke;
pub mod text;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pele_core::scene::Rgba;
use pele_core::{LayoutOptions, NativeInputs, ParseOptions, RenderError, Scene};
use rune::{Color, Cx, El, Tx, memo, tok};

/// Where a link in a diagram leads.
pub use pele_core::scene::LinkRef as Link;
pub use pele_core::scene::Role;
pub use text::FontCatalog;

use lower::{Drawing, Handler, Unsupported};
use text::RuneMeasure;

/// What a diagram tells the app that shows it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// A link was pressed. The view opens nothing itself: the app decides where a link may lead.
    Activate(Link),
}

/// The colours a diagram is drawn in.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Palette {
    core: pele_core::scene::Palette,
}

fn rgba(colour: Color) -> Rgba {
    Rgba { linear_srgb: [colour.r, colour.g, colour.b].map(f64::from), alpha: f64::from(colour.a) }
}

impl Palette {
    /// Pele's own colours, as its SVG falls back to them.
    pub fn pele() -> Self {
        Palette { core: pele_core::scene::Palette::pele() }
    }

    /// The roles in a Rune theme's colours, and Pele's series. Nodes are filled as cards are,
    /// and groups a step from that towards the text. Text on a light fill is Rune's light-mode
    /// text colour, and on a dark one its dark-mode text colour. The accent is Rune's first
    /// accent for the theme's lightness. Rune has no danger colour, so critical keeps Pele's red.
    pub fn from_tokens(theme: &tok::Theme) -> Self {
        let [card, text, bg] = [theme.card, theme.text, theme.bg].map(Color::to_oklab);
        let step: [f32; 4] = std::array::from_fn(|i| card[i] + (text[i] - card[i]) * 0.08);
        let dark = bg[0] < 0.5;
        Palette::pele()
            .with(Role::Accent, tok::accent(0, dark))
            .with(Role::Background, theme.bg)
            .with(Role::Foreground, theme.text)
            .with(Role::Line, theme.text)
            .with(Role::Border, theme.text_secondary)
            .with(Role::Muted, theme.text_secondary)
            .with(Role::Surface, theme.card)
            .with(Role::SurfaceAlt, Color::from_oklab(step))
            .with(Role::OnLight, tok::theme(false).text)
            .with(Role::OnDark, tok::theme(true).text)
    }

    pub fn with(self, role: Role, colour: Color) -> Self {
        Palette { core: self.core.with(role, rgba(colour)) }
    }

    /// Sets the colour of the series at `position`, counted from 0. Positions wrap after eight.
    pub fn with_series(self, position: usize, colour: Color) -> Self {
        Palette { core: self.core.with_series(position, rgba(colour)) }
    }

    pub fn core(&self) -> &pele_core::scene::Palette {
        &self.core
    }
}

/// How one view of a diagram is drawn.
#[derive(Clone, Debug)]
pub struct ViewOptions {
    layout: LayoutOptions,
    palette: Option<Palette>,
    metrics: Metrics,
}

/// What a layout measures its text with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Metrics {
    /// The fonts the text is drawn in.
    #[default]
    Drawn,
    /// Pele's own tables of Arial's widths.
    Pele,
}

impl ViewOptions {
    /// A view at most `width` points wide. A diagram that can draw itself narrower does.
    pub fn new(width: f32) -> Self {
        let layout = LayoutOptions { max_width: Some(f64::from(width)), ..LayoutOptions::default() };
        ViewOptions { layout, palette: None, metrics: Metrics::Drawn }
    }

    /// A view as wide as the diagram lays itself out.
    pub fn natural() -> Self {
        ViewOptions { layout: LayoutOptions::default(), palette: None, metrics: Metrics::Drawn }
    }

    /// Lays the diagram out with Pele's own text metrics instead of the fonts it is drawn in,
    /// so that every shape is where Pele's SVG has it. Text in another font then no longer fits
    /// its boxes exactly, so this is for comparing a drawing with Pele's, not for showing one.
    pub fn pele_metrics(mut self) -> Self {
        self.metrics = Metrics::Pele;
        self
    }

    /// Without one, the view follows the window's dark mode in Rune's theme colours.
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = Some(palette);
        self
    }

    /// The size of plain label text in points, Pele's `fontSize`. 16 by default.
    pub fn font_size(mut self, size: f32) -> Self {
        self.layout.typography.normal.size = f64::from(size);
        self
    }

    /// Pele's `idPrefix`, which names the ids an SVG of this drawing declares. Elements are keyed
    /// by what they draw, so it changes no key.
    pub fn id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.layout.id_prefix = Some(prefix.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewError {
    /// pele-core rejected the source, with Pele's message. No other grammar is tried.
    Core(RenderError),
    /// The diagram type is not ported, and rune-mermaid cannot draw the source either.
    NoFallback,
    /// A view option is not a positive number.
    Option(&'static str),
    /// The Scene asks for something that is not drawn natively, and rune-mermaid cannot draw
    /// the source either.
    Geometry(Unsupported),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::Core(error) => write!(f, "{error}"),
            ViewError::NoFallback => f.write_str("this diagram type is not ported, and rune-mermaid cannot draw it"),
            ViewError::Option(name) => write!(f, "the {name} must be a positive number"),
            ViewError::Geometry(unsupported) => write!(f, "{unsupported}, and rune-mermaid cannot draw it"),
        }
    }
}

impl std::error::Error for ViewError {}

/// Who drew the last view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderStatus {
    Unrendered,
    /// rune-pele, from pele-core's Scene.
    Pele,
    /// Rune's own rune-mermaid, for a source that is not drawn natively. [`Diagram::fallback`]
    /// says why.
    RuneMermaid,
}

/// Why the last view was left to rune-mermaid.
#[derive(Clone, Debug, PartialEq)]
pub enum Fallback {
    /// pele-core has no port of the diagram type.
    NotPorted,
    /// pele-core draws the type, and the Scene is not one the lowering draws.
    NotLowered(Unsupported),
}

/// How much work a diagram has done, for tests and profiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewStats {
    /// Parse and layout runs.
    pub layouts: u64,
    /// Runs of text shaped to measure or place them.
    pub measurements: u64,
    /// Times the elements were built from a layout and colours.
    pub paints: u64,
}

/// How long the last layout and the last paint took, for profiling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewTimes {
    /// Parsing the source and laying it out, with the text measured.
    pub layout: Duration,
    /// Lowering the Scene to shapes and placed text.
    pub lower: Duration,
    /// Building the elements from the lowered drawing and the colours.
    pub paint: Duration,
}

/// Everything that decides a layout. Colours are not part of it.
#[derive(Clone, Debug, PartialEq)]
struct LayoutKey {
    source: String,
    options: LayoutOptions,
    metrics: Metrics,
    scale: f32,
    font_epoch: u64,
}

struct Layout {
    key: LayoutKey,
    /// Names this layout in the paint memo's version.
    id: u64,
    scene: Rc<Scene>,
    drawing: Drawing,
}

/// The measurer for one device scale, kept while the scale holds.
struct Measure {
    scale: f32,
    measure: RuneMeasure,
}

/// Layouts kept for sources and options a view may go back to.
const KEPT: usize = 4;

#[derive(Default)]
struct Session {
    layouts: Vec<Rc<Layout>>,
    /// The inputs last found not to draw natively, so that a view of them does not parse and
    /// lay out again only to find the same.
    refused: Option<(LayoutKey, Fallback)>,
    fallback: Option<Fallback>,
    measure: Option<Measure>,
    /// Runs shaped by measurers since dropped.
    shaped_before: u64,
    next_id: u64,
    status: Option<RenderStatus>,
    last_scene: Option<Rc<Scene>>,
    stats: ViewStats,
    times: ViewTimes,
}

/// One diagram on screen. Make one per diagram instance and keep it across views: it holds the
/// layouts and measurer its views reuse.
pub struct Diagram {
    id: String,
    fonts: FontCatalog,
    session: RefCell<Session>,
}

impl Diagram {
    /// `id` keys the diagram's element and must be unique in its window. `fonts` are the fonts
    /// the app gives Rune.
    pub fn new(id: impl Into<String>, fonts: FontCatalog) -> Self {
        Diagram { id: id.into(), fonts, session: RefCell::default() }
    }

    /// `source` drawn as native elements for this window, with its links inert. A source that
    /// is not drawn natively is drawn by rune-mermaid; any other failure is an error.
    pub fn view<S: 'static>(&self, cx: &mut Cx<S>, source: &str, options: ViewOptions) -> Result<El, ViewError> {
        self.draw(cx, source, options, None)
    }

    /// [`Diagram::view`] with the diagram's links live: pressing one tells `on_event`. The
    /// handler is kept with the painted elements, so it is a plain function of the app's state.
    pub fn view_with<S: 'static>(
        &self,
        cx: &mut Cx<S>,
        source: &str,
        options: ViewOptions,
        on_event: fn(&mut S, Event, &mut Tx),
    ) -> Result<El, ViewError> {
        self.draw(cx, source, options, Some(on_event))
    }

    fn draw<S: 'static>(
        &self,
        cx: &mut Cx<S>,
        source: &str,
        options: ViewOptions,
        on_event: Option<Handler<S>>,
    ) -> Result<El, ViewError> {
        let env = cx.env();
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if !options.layout.max_width.is_none_or(positive) {
            return Err(ViewError::Option("width"));
        }
        if !positive(options.layout.typography.normal.size) {
            return Err(ViewError::Option("font size"));
        }
        let layout = match self.layout(source, &options, env.scale)? {
            Ok(layout) => layout,
            Err(why) => return self.rune_mermaid(source, env.system_dark, &options, why),
        };
        let palette = options.palette.unwrap_or_else(|| Palette::from_tokens(&tok::theme(env.system_dark)));
        let paints = layout.scene.resolve_paints(palette.core(), env.system_dark);
        {
            let mut session = self.session.borrow_mut();
            session.status = Some(RenderStatus::Pele);
            session.fallback = None;
            session.last_scene = Some(layout.scene.clone());
        }
        // The handler is part of what is painted, so another one paints again.
        let handler = on_event.map(|handler| handler as usize);
        Ok(memo(self.id.as_str(), (layout.id, &paints, handler), || {
            let started = Instant::now();
            let painted = layout.drawing.el(&paints, on_event).key(self.id.as_str());
            let mut session = self.session.borrow_mut();
            session.stats.paints += 1;
            session.times.paint = started.elapsed();
            painted
        }))
    }

    /// Why the last view was drawn by rune-mermaid, where it was.
    pub fn fallback(&self) -> Option<Fallback> {
        self.session.borrow().fallback.clone()
    }

    pub fn times(&self) -> ViewTimes {
        self.session.borrow().times
    }

    /// What the last native view asks of Rune.
    pub fn census(&self) -> Option<lower::Census> {
        let session = self.session.borrow();
        let scene = session.last_scene.as_ref()?;
        session.layouts.iter().find(|layout| Rc::ptr_eq(&layout.scene, scene)).map(|layout| layout.drawing.census())
    }

    pub fn status(&self) -> RenderStatus {
        self.session.borrow().status.unwrap_or(RenderStatus::Unrendered)
    }

    /// The Scene behind the last view rune-pele drew itself.
    pub fn scene(&self) -> Option<Rc<Scene>> {
        self.session.borrow().last_scene.clone()
    }

    pub fn stats(&self) -> ViewStats {
        let session = self.session.borrow();
        let shaped = session.measure.as_ref().map_or(0, |m| m.measure.baselines().shaped());
        ViewStats { measurements: session.shaped_before + shaped, ..session.stats }
    }

    /// The layout for these inputs, from the cache or laid out now, or why they are not drawn
    /// natively.
    fn layout(
        &self,
        source: &str,
        options: &ViewOptions,
        scale: f32,
    ) -> Result<Result<Rc<Layout>, Fallback>, ViewError> {
        let font_epoch = rune::text::font_epoch();
        let metrics = options.metrics;
        let options = &options.layout;
        let key = LayoutKey { source: source.to_owned(), options: options.clone(), metrics, scale, font_epoch };
        let mut session = self.session.borrow_mut();
        if let Some(at) = session.layouts.iter().position(|layout| layout.key == key) {
            let layout = session.layouts.remove(at);
            session.layouts.push(layout.clone());
            return Ok(Ok(layout));
        }
        if let Some((_, why)) = session.refused.as_ref().filter(|(refused, _)| *refused == key) {
            return Ok(Err(why.clone()));
        }
        if session.measure.as_ref().is_none_or(|m| m.scale != scale) {
            let before = session.measure.take().map_or(0, |m| m.measure.baselines().shaped());
            session.shaped_before += before;
            session.measure = Some(Measure { scale, measure: RuneMeasure::new(self.fonts.baselines(scale)) });
        }
        let measure = &session.measure.as_ref().expect("made above").measure;
        let started = Instant::now();
        let mut laid_out = started;
        let lowered = match pele_core::prepare(source, ParseOptions::default()) {
            Err(RenderError::NotPorted) => Err(Fallback::NotPorted),
            Err(error) => return Err(ViewError::Core(error)),
            // No Scene is laid out for a kind that is not drawn from one.
            Ok(prepared) if !lower::NATIVE.contains(&prepared.kind()) => {
                Err(Fallback::NotLowered(Unsupported::Kind(prepared.kind())))
            }
            Ok(prepared) => {
                let scene = match metrics {
                    Metrics::Drawn => prepared.scene(NativeInputs { measure, options }),
                    Metrics::Pele => pele_core::parity_scene_with(source, ParseOptions::default(), options)
                        .map(|parity| parity.scene().clone()),
                };
                let scene = scene.map_err(ViewError::Core)?;
                laid_out = Instant::now();
                Drawing::new(prepared.kind(), &scene, &mut measure.baselines(), scale)
                    .map(|drawing| (scene, drawing))
                    .map_err(Fallback::NotLowered)
            }
        };
        let (scene, drawing) = match lowered {
            Ok(lowered) => lowered,
            Err(why) => {
                session.refused = Some((key, why.clone()));
                return Ok(Err(why));
            }
        };
        session.times.layout = laid_out - started;
        session.times.lower = laid_out.elapsed();
        session.stats.layouts += 1;
        session.next_id += 1;
        let layout = Rc::new(Layout { key, id: session.next_id, scene: Rc::new(scene), drawing });
        if session.layouts.len() == KEPT {
            session.layouts.remove(0);
        }
        session.layouts.push(layout.clone());
        Ok(Ok(layout))
    }

    fn rune_mermaid(&self, source: &str, dark: bool, options: &ViewOptions, why: Fallback) -> Result<El, ViewError> {
        let mut style = rune_mermaid::DiagramStyle::neutral(dark);
        if let Some(width) = options.layout.max_width {
            style.max_width = width as f32;
        }
        let drawn = rune_mermaid::render(source, &style).ok_or(match &why {
            Fallback::NotPorted => ViewError::NoFallback,
            Fallback::NotLowered(unsupported) => ViewError::Geometry(unsupported.clone()),
        })?;
        let mut session = self.session.borrow_mut();
        session.status = Some(RenderStatus::RuneMermaid);
        session.fallback = Some(why);
        session.last_scene = None;
        Ok(drawn.key(self.id.as_str()))
    }
}
