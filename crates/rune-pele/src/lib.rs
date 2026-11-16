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
//!
//! While a source fails after one that drew, as a source still streaming in does, the view
//! shows the last drawing that drew, in the [`KeptLook`] its options name, and
//! [`Diagram::error`] says why.

pub mod bands;
pub mod capture;
pub mod fill;
pub mod geometry;
mod kept;
pub mod lower;
pub mod stroke;
pub mod text;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pele_core::scene::{Rgba, ThemeSeries};
use pele_core::{LayoutOptions, NativeInputs, ParseOptions, RenderError, Scene};
use rune::{Color, Cx, El, Tx, memo, tok};

pub use kept::KeptLook;
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
    /// Whose colour a series the source's theme config sets is drawn in.
    theme: ThemeSeries,
}

fn rgba(colour: Color) -> Rgba {
    Rgba { linear_srgb: [colour.r, colour.g, colour.b].map(f64::from), alpha: f64::from(colour.a) }
}

impl Palette {
    /// Pele's own colours, as its SVG falls back to them. A series colour the source sets in its
    /// theme config wins over them, as in Pele. Any other palette, even one made from this one
    /// with [`Palette::with`], wins over the source's.
    pub fn pele() -> Self {
        Palette { core: pele_core::scene::Palette::pele(), theme: ThemeSeries::Source }
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
        Palette { core: self.core.with(role, rgba(colour)), theme: ThemeSeries::Palette }
    }

    /// Sets the colour of the series at `position`, counted from 0. Positions wrap after eight.
    pub fn with_series(self, position: usize, colour: Color) -> Self {
        Palette { core: self.core.with_series(position, rgba(colour)), theme: ThemeSeries::Palette }
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
    kept: KeptLook,
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

/// The id prefix a view lays out under when the app gives none: Pele's own default.
const DEFAULT_ID_PREFIX: &str = "pele";

impl ViewOptions {
    /// A view at most `width` points wide. A diagram that can draw itself narrower does.
    pub fn new(width: f32) -> Self {
        let layout = LayoutOptions { max_width: Some(f64::from(width)), ..Self::natural().layout };
        ViewOptions { layout, ..Self::natural() }
    }

    /// A view as wide as the diagram lays itself out.
    pub fn natural() -> Self {
        // Pele draws a sankey's links along gradients only under an id prefix, so that two SVGs
        // in one page never share a gradient's id; its `mount` always gives one. A view has no
        // ids to share, so it lays out under Pele's own default prefix, which also seeds a
        // cynefin's boundaries as Pele does without one.
        let layout = LayoutOptions { id_prefix: Some(DEFAULT_ID_PREFIX.to_owned()), ..LayoutOptions::default() };
        ViewOptions { layout, palette: None, metrics: Metrics::Drawn, kept: KeptLook::default() }
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
    /// by what they draw, so it changes no key. It is `pele` unless set. As in Pele, it also
    /// seeds a cynefin's boundaries, and a prefix of other characters than letters, digits, `_`
    /// and `-` draws a sankey's links in one colour each.
    pub fn id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.layout.id_prefix = Some(prefix.into());
        self
    }

    /// The time a gantt chart marks as today, in milliseconds since 1970, Pele's `now`. Without
    /// it, the chart takes the system clock when the view lays it out, and keeps that time until
    /// the view lays it out again for another reason: the marker does not move with the clock. A
    /// different `now` lays the diagram out again, and the same one does not. A time that is not
    /// finite is the system clock.
    pub fn now(mut self, milliseconds: f64) -> Self {
        // A NaN in the layout's key would equal no layout, so every view would lay out again.
        self.layout.now = milliseconds.is_finite().then_some(milliseconds);
        self
    }

    /// How the view shows the last drawing that drew while the source fails:
    /// [`KeptLook::DimmedWithError`], as Pele's `mount` does, unless set.
    pub fn kept_look(mut self, look: KeptLook) -> Self {
        self.kept = look;
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
    /// Nothing: the view was an error, or there has been none.
    Unrendered,
    /// rune-pele, from pele-core's Scene.
    Pele,
    /// Rune's own rune-mermaid, for a source that is not drawn natively. [`Diagram::fallback`]
    /// says why.
    RuneMermaid,
    /// The source failed, and the view showed the last drawing that drew, in the options'
    /// [`KeptLook`]. [`Diagram::error`] says why the source failed. [`Diagram::fallback`] and
    /// [`Diagram::scene`] are those of the drawing shown.
    Kept,
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

/// A drawing a view drew, kept to show again while a later source fails.
#[derive(Clone)]
enum Drawn {
    /// rune-pele's own, by its layout, which the cache may drop while the drawing is kept.
    Pele(Rc<Layout>),
    RuneMermaid(Rc<MermaidDrawing>),
}

/// What rune-mermaid drew, and what it drew it from.
struct MermaidDrawing {
    source: String,
    max_width: Option<f32>,
    why: Fallback,
    /// Whether `el` is in dark mode's colours.
    dark: bool,
    el: El,
}

/// What the last view did.
enum Outcome {
    /// Its source drew.
    Drew,
    /// Its source failed, and the last drawing that drew was shown.
    Kept(ViewError),
    /// It drew nothing.
    Failed(ViewError),
}

#[derive(Default)]
struct Session {
    layouts: Vec<Rc<Layout>>,
    /// The inputs last found not to draw natively, so that a view of them does not parse and
    /// lay out again only to find the same.
    refused: Option<(LayoutKey, Fallback)>,
    measure: Option<Measure>,
    /// Runs shaped by measurers since dropped.
    shaped_before: u64,
    next_id: u64,
    /// The last drawing a view drew.
    drawn: Option<Drawn>,
    outcome: Option<Outcome>,
    stats: ViewStats,
    times: ViewTimes,
}

impl Session {
    /// The drawing the last view showed, whether its source drew it or it was kept.
    fn shown(&self) -> Option<&Drawn> {
        match self.outcome {
            Some(Outcome::Drew | Outcome::Kept(_)) => self.drawn.as_ref(),
            Some(Outcome::Failed(_)) | None => None,
        }
    }
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
    /// is not drawn natively is drawn by rune-mermaid. A source that fails after one that drew
    /// shows that drawing, as [`ViewOptions::kept_look`] says. Any other failure is an error.
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
        let error = match self.draw_source(cx, source, &options, on_event) {
            Ok(drawn) => {
                self.session.borrow_mut().outcome = Some(Outcome::Drew);
                return Ok(drawn);
            }
            Err(error) => error,
        };
        let kept = match error {
            ViewError::Option(_) => None,
            _ => self.session.borrow().drawn.clone(),
        };
        let Some(kept) = kept else {
            self.session.borrow_mut().outcome = Some(Outcome::Failed(error.clone()));
            return Err(error);
        };
        let env = cx.env();
        let palette = options.palette.clone().unwrap_or_else(|| Palette::from_tokens(&tok::theme(env.system_dark)));
        let drawing = match &kept {
            Drawn::Pele(layout) => self.paint(layout, &palette, env.system_dark, on_event),
            Drawn::RuneMermaid(drawing) => self.redraw(drawing, env.system_dark),
        };
        let critical = lower::colour(palette.core().role(Role::Critical));
        let font_size = options.layout.typography.normal.size as f32;
        let shown = options.kept.el(drawing, &self.id, &error.to_string(), font_size, critical);
        self.session.borrow_mut().outcome = Some(Outcome::Kept(error));
        Ok(shown)
    }

    /// `source` drawn, natively or by rune-mermaid.
    fn draw_source<S: 'static>(
        &self,
        cx: &mut Cx<S>,
        source: &str,
        options: &ViewOptions,
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
        let layout = match self.layout(source, options, env.scale)? {
            Ok(layout) => layout,
            Err(why) => return self.rune_mermaid(source, env.system_dark, options, why),
        };
        let palette = options.palette.clone().unwrap_or_else(|| Palette::from_tokens(&tok::theme(env.system_dark)));
        self.session.borrow_mut().drawn = Some(Drawn::Pele(layout.clone()));
        Ok(self.paint(&layout, &palette, env.system_dark, on_event))
    }

    /// `layout`'s drawing in `palette`, painted again only when the colours or the handler change.
    fn paint<S: 'static>(&self, layout: &Layout, palette: &Palette, dark: bool, on_event: Option<Handler<S>>) -> El {
        let paints = layout.scene.resolve_paints_with(palette.core(), dark, palette.theme);
        // The handler is part of what is painted, so another one paints again.
        let handler = on_event.map(|handler| handler as usize);
        memo(self.id.as_str(), (layout.id, &paints, handler), || {
            let started = Instant::now();
            let painted = layout.drawing.el(&paints, on_event).key(self.id.as_str());
            let mut session = self.session.borrow_mut();
            session.stats.paints += 1;
            session.times.paint = started.elapsed();
            painted
        })
    }

    /// Why the drawing the last view showed was left to rune-mermaid, where it was.
    pub fn fallback(&self) -> Option<Fallback> {
        match self.session.borrow().shown() {
            Some(Drawn::RuneMermaid(drawing)) => Some(drawing.why.clone()),
            Some(Drawn::Pele(_)) | None => None,
        }
    }

    /// Why the last view's source drew nothing, or why its options were refused. After a view
    /// that drew, `None`. While the view keeps an earlier drawing ([`RenderStatus::Kept`]), why
    /// the source fails.
    pub fn error(&self) -> Option<ViewError> {
        match &self.session.borrow().outcome {
            Some(Outcome::Kept(error) | Outcome::Failed(error)) => Some(error.clone()),
            Some(Outcome::Drew) | None => None,
        }
    }

    pub fn times(&self) -> ViewTimes {
        self.session.borrow().times
    }

    /// What the drawing the last view showed asks of Rune, where rune-pele drew it.
    pub fn census(&self) -> Option<lower::Census> {
        match self.session.borrow().shown() {
            Some(Drawn::Pele(layout)) => Some(layout.drawing.census()),
            Some(Drawn::RuneMermaid(_)) | None => None,
        }
    }

    pub fn status(&self) -> RenderStatus {
        let session = self.session.borrow();
        match (&session.outcome, &session.drawn) {
            (Some(Outcome::Drew), Some(Drawn::Pele(_))) => RenderStatus::Pele,
            (Some(Outcome::Drew), Some(Drawn::RuneMermaid(_))) => RenderStatus::RuneMermaid,
            (Some(Outcome::Kept(_)), _) => RenderStatus::Kept,
            (Some(Outcome::Failed(_)) | None, _) | (Some(Outcome::Drew), None) => RenderStatus::Unrendered,
        }
    }

    /// The Scene behind the drawing the last view showed, where rune-pele drew it.
    pub fn scene(&self) -> Option<Rc<Scene>> {
        match self.session.borrow().shown() {
            Some(Drawn::Pele(layout)) => Some(layout.scene.clone()),
            Some(Drawn::RuneMermaid(_)) | None => None,
        }
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
        let max_width = options.layout.max_width.map(|width| width as f32);
        let drawn = self.by_rune_mermaid(source, dark, max_width).ok_or(match &why {
            Fallback::NotPorted => ViewError::NoFallback,
            Fallback::NotLowered(unsupported) => ViewError::Geometry(unsupported.clone()),
        })?;
        let drawing = MermaidDrawing { source: source.to_owned(), max_width, why, dark, el: drawn.clone() };
        self.session.borrow_mut().drawn = Some(Drawn::RuneMermaid(Rc::new(drawing)));
        Ok(drawn)
    }

    fn by_rune_mermaid(&self, source: &str, dark: bool, max_width: Option<f32>) -> Option<El> {
        let mut style = rune_mermaid::DiagramStyle::neutral(dark);
        if let Some(width) = max_width {
            style.max_width = width;
        }
        Some(rune_mermaid::render(source, &style)?.key(self.id.as_str()))
    }

    /// A kept rune-mermaid drawing, drawn again only for a change of dark mode: rune-mermaid
    /// has no colours to change without drawing again.
    fn redraw(&self, drawing: &MermaidDrawing, dark: bool) -> El {
        if drawing.dark == dark {
            return drawing.el.clone();
        }
        let Some(el) = self.by_rune_mermaid(&drawing.source, dark, drawing.max_width) else {
            return drawing.el.clone();
        };
        let again = MermaidDrawing {
            source: drawing.source.clone(),
            why: drawing.why.clone(),
            dark,
            el: el.clone(),
            ..*drawing
        };
        self.session.borrow_mut().drawn = Some(Drawn::RuneMermaid(Rc::new(again)));
        el
    }
}
