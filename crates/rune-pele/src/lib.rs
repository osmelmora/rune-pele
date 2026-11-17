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
//! That drawing has no links or tooltips and ignores the zoom: [`Diagram::engine`] says who drew
//! the drawing a view shows, and [`Engine`] what it can do.
//!
//! Layout is cached by source, layout options, device scale and Rune's font epoch, lowering by
//! layout and zoom, and painting is a `memo` keyed by the lowered drawing and the resolved
//! colours. So a palette or dark-mode change repaints without parsing, laying out or measuring
//! anything, and a zoom lowers again without laying out.
//!
//! [`ViewOptions::fit`] fits a diagram to the width its view has, which it reads from Rune's
//! layout.
//!
//! While a source fails after one that drew, as a source still streaming in does, the view
//! shows the last source that drew, in the [`KeptLook`] its options name, and
//! [`Diagram::error`] says why. That source is drawn as the view's options and width draw it, as
//! Pele's `mount` draws its last parsed text again for a new width: from the caches when nothing
//! changed, at a new zoom lowered again, and at a new step of a fit laid out at that step.

pub mod bands;
pub mod capture;
pub mod fill;
mod fit;
pub mod geometry;
mod kept;
pub mod lower;
pub mod stroke;
pub mod text;

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pele_core::scene::{Rgba, ThemeSeries};
use pele_core::{DiagramKind, LayoutOptions, NativeInputs, ParseOptions, RenderError, Scene};
use rune::{Color, Cx, El, Env, Tx, el, memo, tok};

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
///
/// A drawing rune-mermaid draws ([`Engine::RuneMermaid`]) follows only the width and the kept
/// look of these options. It ignores the palette, the font size, the zoom and a fit's smallest
/// text, and is drawn in Rune's neutral light or dark colours.
#[derive(Clone, Debug)]
pub struct ViewOptions {
    layout: LayoutOptions,
    palette: Option<Palette>,
    metrics: Metrics,
    kept: KeptLook,
    /// How many times its laid-out size the diagram is drawn, after any fit.
    zoom: f32,
    fit: Option<Fit>,
}

/// How a view fits the width it has: see [`ViewOptions::fit`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    /// The least size plain text is drawn at, in points.
    smallest_text: f32,
}

/// The least size a fitted view draws plain text at, in points, unless the app sets another: a
/// little over two thirds of Pele's 16.
const SMALLEST_TEXT: f32 = 11.0;

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
        ViewOptions { layout, palette: None, metrics: Metrics::Drawn, kept: KeptLook::default(), zoom: 1.0, fit: None }
    }

    /// A view as wide as its parent, which fits the diagram to that width. A diagram that fits
    /// is drawn at its own size. One that does not, and draws itself narrower when it has less
    /// room, is laid out again for the width, as Pele's `mount` does. What is still too wide is
    /// drawn smaller, but never with plain text under [`ViewOptions::smallest_text`]: at that
    /// size the view scrolls sideways instead. A fit never draws a diagram larger than its size.
    ///
    /// Plain text is text the diagram draws at the view's font size or larger at its own size.
    /// The least size holds for it as drawn: a narrower layout that sets it smaller itself, as a
    /// treemap does in small cells, is drawn only where it can still reach that size, and the
    /// diagram is otherwise drawn as it lays itself out. A diagram that runs across and can run
    /// down, as a flowchart can, runs down wherever across would scroll at that size, at any
    /// width, where Pele turns it only under 640 points.
    ///
    /// The width is the view's own from Rune's last layout, and a change of it runs the app's
    /// view again. The diagram is laid out again only when the width crosses one of the steps
    /// a twelfth of an octave apart, at which it is then laid out, so that dragging a window's
    /// edge zooms between steps. Before Rune has laid the view out, it draws the diagram at its
    /// own size; the same commit runs the view again with the width before anything is shown.
    pub fn fit() -> Self {
        ViewOptions { fit: Some(Fit { smallest_text: SMALLEST_TEXT }), ..Self::natural() }
    }

    /// The least size, in points, a fitted view draws plain text at: 11 unless set. It makes
    /// the view's least zoom this over the plain text size, or 1 where that is less, and more
    /// for a narrower layout that sets plain text smaller.
    pub fn smallest_text(mut self, points: f32) -> Self {
        if let Some(fit) = &mut self.fit {
            fit.smallest_text = points;
        }
        self
    }

    /// Draws the diagram `zoom` times the size it is laid out at, after any fit: its shapes, its
    /// text and the boxes its links and tooltips are found in. 1 unless set. A zoom lowers the
    /// laid-out diagram again, and lays nothing out.
    pub fn zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    /// Lays the diagram out with Pele's own text metrics instead of the fonts it is drawn in,
    /// so that every shape is where Pele's SVG has it. Text in another font then no longer fits
    /// its boxes exactly, so this is for comparing a drawing with Pele's, not for showing one.
    /// It changes only how text is measured: a gantt chart's today is still [`ViewOptions::now`]
    /// or the system clock, not the clock Pele's corpus is drawn at.
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

    /// The width rune-mermaid draws in: the view's, or a fitted view's `room`.
    fn mermaid_width(&self, room: Option<f32>) -> Option<f32> {
        self.layout.max_width.map(|width| width as f32).or(room)
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

/// Who drew the drawing a view shows, and so what that drawing can do. [`Diagram::engine`]
/// reads it after a view, whether the view's source drew or the view kept an earlier drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// rune-pele, from pele-core's Scene: its links are live, it shows tooltips, and it follows
    /// every view option.
    Pele,
    /// Rune's own rune-mermaid, for a source that is not drawn natively: [`Diagram::fallback`]
    /// says why. It draws no links or tooltips, and of the view's options follows only the
    /// width and the kept look, as [`ViewOptions`] says.
    RuneMermaid,
}

impl Engine {
    /// Whether the drawing's links are live: pressing one tells the handler that
    /// [`Diagram::view_with`] gives. rune-mermaid draws no links, so a press tells the app
    /// nothing.
    pub fn has_links(self) -> bool {
        match self {
            Engine::Pele => true,
            Engine::RuneMermaid => false,
        }
    }

    /// Whether the drawing shows the source's tooltips where the pointer rests. rune-mermaid
    /// draws none.
    pub fn has_tooltips(self) -> bool {
        match self {
            Engine::Pele => true,
            Engine::RuneMermaid => false,
        }
    }

    /// Whether the drawing follows [`ViewOptions::zoom`], and a fitted view shrinks it no
    /// further than [`ViewOptions::smallest_text`]. rune-mermaid does neither: it draws its own
    /// 13 point text, at most the view's width wide or 680 points without one, shrinks a wider
    /// diagram to 0.6 of its size whatever its text then measures, and past that shows the
    /// middle with its sides faded out.
    pub fn zooms(self) -> bool {
        match self {
            Engine::Pele => true,
            Engine::RuneMermaid => false,
        }
    }
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
    /// Times a layout was lowered to shapes and placed text, once for each zoom it is drawn at.
    pub lowerings: u64,
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
    /// Names this layout among its lowerings.
    id: u64,
    kind: DiagramKind,
    scene: Rc<Scene>,
    /// Whether the diagram draws itself narrower for less room, once a fit has asked.
    narrows: OnceCell<bool>,
    /// The least share of its size at the source's own size that this layout draws plain text
    /// at, once a fit has asked: 1 where it sets none smaller.
    shrink: OnceCell<f64>,
}

/// A layout lowered at one zoom, or why it is not drawn natively.
struct Lowered {
    layout: Rc<Layout>,
    zoom: f32,
    /// Names this drawing in the paint memo's version.
    id: u64,
    drawing: Result<Drawing, Unsupported>,
}

/// The layout a view draws a source from and the zoom it draws it at, with the layout at the
/// source's own size that a fit measures the width against.
struct Sized {
    natural: Rc<Layout>,
    laid: Rc<Layout>,
    zoom: f32,
}

/// The measurer for one device scale, kept while the scale holds.
struct Measure {
    scale: f32,
    measure: RuneMeasure,
}

/// Layouts kept for sources and options a view may go back to.
const KEPT: usize = 4;

/// The system clock in milliseconds since 1970, as pele-core reads it for a `now` left unset.
fn system_now() -> f64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0.0, |since| since.as_millis() as f64)
}

/// A number no other layout or lowering in the process has, of any `Diagram`. The paint memo is
/// named by the diagram's id, so a `Diagram` that replaces another under that id must not number
/// its first drawing as the one it replaces did.
fn unique_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A drawing a view drew, kept to show again while a later source fails.
#[derive(Clone)]
enum Drawn {
    /// rune-pele's own: its lowering, which holds the layout it lowers, and the layout at the
    /// source's own size that a fit sized it from. The layout cache may drop both while it is
    /// kept, for a failing source that lays out three times a frame: its own size, the probe and
    /// a step.
    Pele {
        natural: Rc<Layout>,
        lowered: Rc<Lowered>,
    },
    RuneMermaid(Rc<MermaidDrawing>),
}

impl Drawn {
    /// Its layout for `key`, where it was laid out for that key.
    fn layout(&self, key: &LayoutKey) -> Option<Rc<Layout>> {
        let Drawn::Pele { natural, lowered } = self else { return None };
        [natural, &lowered.layout].into_iter().find(|layout| layout.key == *key).cloned()
    }
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
    lowered: Vec<Rc<Lowered>>,
    /// The inputs last found not to draw natively, so that a view of them does not parse and
    /// lay out again only to find the same.
    refused: Option<(LayoutKey, Fallback)>,
    measure: Option<Measure>,
    /// Runs shaped by measurers since dropped.
    shaped_before: u64,
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

    /// The measurer for `scale`, made anew when the scale changes.
    fn measure(&mut self, fonts: &FontCatalog, scale: f32) -> &RuneMeasure {
        if self.measure.as_ref().is_none_or(|m| m.scale != scale) {
            let before = self.measure.take().map_or(0, |m| m.measure.baselines().shaped());
            self.shaped_before += before;
            self.measure = Some(Measure { scale, measure: RuneMeasure::new(fonts.baselines(scale)) });
        }
        &self.measure.as_ref().expect("made above").measure
    }
}

/// One diagram on screen. Make one per diagram instance and keep it across views: it holds the
/// layouts and measurer its views reuse.
///
/// A source the lowering cannot draw is drawn by rune-mermaid instead, which draws no links or
/// tooltips and ignores the zoom. After each view, [`Diagram::engine`] says who drew the
/// drawing shown, and so what it can do.
pub struct Diagram {
    id: String,
    fonts: FontCatalog,
    session: RefCell<Session>,
}

impl Diagram {
    /// `id` keys the diagram's element and must be unique in its window. In a fitted view it
    /// keys the frame the fit reads its width from, and the drawing inside it too. `fonts` are
    /// the fonts the app gives Rune.
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
    /// A drawing rune-mermaid draws has no links, so it never tells `on_event`: see
    /// [`Engine::has_links`].
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
        // The width a fitted view has, as Rune last laid out its frame: none before its first
        // layout. The frame is the same whether the source draws or a drawing is kept.
        let room = options.fit.and_then(|_| cx.layout_with(self.frame_key(), |rect| rect[2]));
        let error = match self.draw_source(env, source, &options, room, on_event) {
            Ok(drawn) => {
                self.session.borrow_mut().outcome = Some(Outcome::Drew);
                return Ok(self.framed(options.fit, drawn));
            }
            Err(error) => error,
        };
        let kept = match error {
            ViewError::Option(_) => None,
            _ => self.session.borrow().drawn.clone(),
        };
        // The last source that drew, drawn again as these options and this room draw it. Its
        // layouts are held, so at the size it was drawn at nothing is laid out or lowered again.
        let again = match &kept {
            Some(Drawn::Pele { natural, .. }) => {
                self.draw_source(env, &natural.key.source, &options, room, on_event).ok()
            }
            Some(Drawn::RuneMermaid(drawing)) => {
                Some(self.redraw(drawing, env.system_dark, options.mermaid_width(room)))
            }
            None => None,
        };
        let Some(drawing) = again else {
            self.session.borrow_mut().outcome = Some(Outcome::Failed(error.clone()));
            return Err(error);
        };
        let palette = options.palette.clone().unwrap_or_else(|| Palette::from_tokens(&tok::theme(env.system_dark)));
        let critical = lower::colour(palette.core().role(Role::Critical));
        let font_size = options.layout.typography.normal.size as f32;
        let shown = options.kept.el(drawing, &self.id, &error.to_string(), font_size, critical);
        self.session.borrow_mut().outcome = Some(Outcome::Kept(error));
        Ok(self.framed(options.fit, shown))
    }

    /// `source` drawn, natively or by rune-mermaid, at the size its options and a fitted view's
    /// `room` give it.
    fn draw_source<S: 'static>(
        &self,
        env: Env,
        source: &str,
        options: &ViewOptions,
        room: Option<f32>,
        on_event: Option<Handler<S>>,
    ) -> Result<El, ViewError> {
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if !options.layout.max_width.is_none_or(positive) {
            return Err(ViewError::Option("width"));
        }
        if !positive(options.layout.typography.normal.size) {
            return Err(ViewError::Option("font size"));
        }
        if !positive(f64::from(options.zoom)) {
            return Err(ViewError::Option("zoom"));
        }
        if !options.fit.is_none_or(|fit| positive(f64::from(fit.smallest_text))) {
            return Err(ViewError::Option("smallest text size"));
        }
        let sized = match self.sized(source, options, env.scale, room)? {
            Ok(sized) => sized,
            Err(why) => return self.rune_mermaid(source, env.system_dark, options.mermaid_width(room), why),
        };
        let lowered = self.lowered(&sized.laid, env.scale, sized.zoom);
        let drawing = match &lowered.drawing {
            Ok(drawing) => drawing,
            Err(why) => {
                let why = Fallback::NotLowered(why.clone());
                return self.rune_mermaid(source, env.system_dark, options.mermaid_width(room), why);
            }
        };
        let palette = options.palette.clone().unwrap_or_else(|| Palette::from_tokens(&tok::theme(env.system_dark)));
        self.session.borrow_mut().drawn = Some(Drawn::Pele { natural: sized.natural, lowered: lowered.clone() });
        Ok(self.paint(&lowered, drawing, &palette, env.system_dark, on_event))
    }

    /// `lowered`'s `drawing` in `palette`, painted again only when the drawing, the colours or
    /// the handler change.
    fn paint<S: 'static>(
        &self,
        lowered: &Lowered,
        drawing: &Drawing,
        palette: &Palette,
        dark: bool,
        on_event: Option<Handler<S>>,
    ) -> El {
        let paints = lowered.layout.scene.resolve_paints_with(palette.core(), dark, palette.theme);
        // The handler is part of what is painted, so another one paints again.
        let handler = on_event.map(|handler| handler as usize);
        memo(self.id.as_str(), (lowered.id, &paints, handler), || {
            let started = Instant::now();
            let painted = drawing.el(&paints, on_event).key(self.id.as_str());
            let mut session = self.session.borrow_mut();
            session.stats.paints += 1;
            session.times.paint = started.elapsed();
            painted
        })
    }

    /// The key of a fitted view's frame, whose width it fits: the id, as the drawing inside it
    /// is keyed, so the view is one element keyed with the id in every mode. Rune finds the frame
    /// as the first element so keyed, and no other element of the window may be keyed so.
    fn frame_key(&self) -> &str {
        &self.id
    }

    /// A fitted view's drawing in its frame: as wide as its parent, and scrolling sideways where
    /// the drawing is wider.
    fn framed(&self, fit: Option<Fit>, drawn: El) -> El {
        match fit {
            Some(_) => el().key(self.frame_key()).w_full().scroll_x().child(drawn),
            None => drawn,
        }
    }

    /// The layout a view draws and the zoom it draws it at, or why it is not drawn natively.
    /// A fitted view with `room` follows the steps [`ViewOptions::fit`] gives, in order.
    fn sized(
        &self,
        source: &str,
        options: &ViewOptions,
        scale: f32,
        room: Option<f32>,
    ) -> Result<Result<Sized, Fallback>, ViewError> {
        let natural = match self.layout(source, &options.layout, options.metrics, scale)? {
            Ok(layout) => layout,
            Err(why) => return Ok(Err(why)),
        };
        let room = room.map(f64::from).filter(|room| room.is_finite() && *room > 0.0);
        let (Some(fit), Some(room)) = (options.fit, room) else {
            return Ok(Ok(Sized { laid: natural.clone(), natural, zoom: options.zoom }));
        };
        if natural.scene.size().width <= room {
            return Ok(Ok(Sized { laid: natural.clone(), natural, zoom: options.zoom }));
        }
        let plain = options.layout.typography.normal.size;
        // The least zoom that draws plain text at the smallest size, or 1 where that is more.
        let least = (f64::from(fit.smallest_text) / plain).min(1.0);
        let (mut laid, mut laid_least) = (natural.clone(), least);
        if self.narrows(source, options, scale, &natural) {
            let step = fit::step_below(room);
            // A diagram that can run down turns wherever across, at the least zoom, would still
            // be wider than the step, so that it would scroll.
            let at =
                LayoutOptions { max_width: Some(step), turn_wider_than: Some(step / least), ..options.layout.clone() };
            let narrower = match self.layout(source, &at, options.metrics, scale)? {
                Ok(layout) => layout,
                Err(why) => return Ok(Err(why)),
            };
            // A narrower layout may set plain text smaller itself. It is drawn only where that
            // text still reaches the smallest size at a zoom of at most 1.
            let shrink =
                *narrower.shrink.get_or_init(|| fit::plain_text_shrink(&natural.scene, &narrower.scene, plain));
            if shrink >= least {
                (laid, laid_least) = (narrower, least / shrink);
            }
        }
        let zoom = fit::zoom_into(room, laid.scene.size().width, laid_least) as f32;
        Ok(Ok(Sized { natural, laid, zoom: zoom * options.zoom }))
    }

    /// Whether the diagram of `natural` draws itself narrower when it has less room, as Pele's
    /// `mount` asks: laid out for a width of 1, it differs from its own layout.
    fn narrows(&self, source: &str, options: &ViewOptions, scale: f32, natural: &Layout) -> bool {
        if let Some(&narrows) = natural.narrows.get() {
            return narrows;
        }
        let squeezed = LayoutOptions { max_width: Some(1.0), ..options.layout.clone() };
        let narrows = match self.layout(source, &squeezed, options.metrics, scale) {
            Ok(Ok(probe)) => {
                probe.scene.size() != natural.scene.size() || !probe.scene.paint().eq(natural.scene.paint())
            }
            // A diagram that cannot be laid out with no room is drawn as it lays itself out.
            _ => false,
        };
        *natural.narrows.get_or_init(|| narrows)
    }

    /// `layout` lowered at `zoom`, from the cache or lowered now.
    fn lowered(&self, layout: &Rc<Layout>, scale: f32, zoom: f32) -> Rc<Lowered> {
        let mut session = self.session.borrow_mut();
        if let Some(at) =
            session.lowered.iter().position(|lowered| lowered.layout.id == layout.id && lowered.zoom == zoom)
        {
            let lowered = session.lowered.remove(at);
            session.lowered.push(lowered.clone());
            return lowered;
        }
        let started = Instant::now();
        let measure = session.measure(&self.fonts, scale);
        let drawing = Drawing::zoomed(layout.kind, &layout.scene, &mut measure.baselines(), scale, zoom);
        session.times.lower = started.elapsed();
        session.stats.lowerings += 1;
        let lowered = Rc::new(Lowered { layout: layout.clone(), zoom, id: unique_id(), drawing });
        if session.lowered.len() == KEPT {
            session.lowered.remove(0);
        }
        session.lowered.push(lowered.clone());
        lowered
    }

    /// Why the drawing the last view showed was left to rune-mermaid, where it was.
    pub fn fallback(&self) -> Option<Fallback> {
        match self.session.borrow().shown() {
            Some(Drawn::RuneMermaid(drawing)) => Some(drawing.why.clone()),
            Some(Drawn::Pele { .. }) | None => None,
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
            Some(Drawn::Pele { lowered, .. }) => lowered.drawing.as_ref().ok().map(Drawing::census),
            Some(Drawn::RuneMermaid(_)) | None => None,
        }
    }

    /// The zoom the drawing the last view showed was drawn at, where rune-pele drew it: 1 at the
    /// size the diagram is laid out at.
    pub fn zoom(&self) -> Option<f32> {
        match self.session.borrow().shown() {
            Some(Drawn::Pele { lowered, .. }) => Some(lowered.zoom),
            Some(Drawn::RuneMermaid(_)) | None => None,
        }
    }

    pub fn status(&self) -> RenderStatus {
        let session = self.session.borrow();
        match (&session.outcome, &session.drawn) {
            (Some(Outcome::Drew), Some(Drawn::Pele { .. })) => RenderStatus::Pele,
            (Some(Outcome::Drew), Some(Drawn::RuneMermaid(_))) => RenderStatus::RuneMermaid,
            (Some(Outcome::Kept(_)), _) => RenderStatus::Kept,
            (Some(Outcome::Failed(_)) | None, _) | (Some(Outcome::Drew), None) => RenderStatus::Unrendered,
        }
    }

    /// Who drew the drawing the last view showed, and so what it can do, whether its source drew
    /// it or the view kept it ([`RenderStatus::Kept`]). `None` when the view showed none.
    /// [`Diagram::status`] says whether the source drew, and [`Diagram::fallback`] why
    /// rune-mermaid did.
    pub fn engine(&self) -> Option<Engine> {
        match self.session.borrow().shown()? {
            Drawn::Pele { .. } => Some(Engine::Pele),
            Drawn::RuneMermaid(_) => Some(Engine::RuneMermaid),
        }
    }

    /// The Scene behind the drawing the last view showed, where rune-pele drew it.
    pub fn scene(&self) -> Option<Rc<Scene>> {
        match self.session.borrow().shown() {
            Some(Drawn::Pele { lowered, .. }) => Some(lowered.layout.scene.clone()),
            Some(Drawn::RuneMermaid(_)) | None => None,
        }
    }

    pub fn stats(&self) -> ViewStats {
        let session = self.session.borrow();
        let shaped = session.measure.as_ref().map_or(0, |m| m.measure.baselines().shaped());
        ViewStats { measurements: session.shaped_before + shaped, ..session.stats }
    }

    /// The layout for these inputs, from the cache or the last drawing, or laid out now, or why
    /// they are not drawn natively.
    fn layout(
        &self,
        source: &str,
        options: &LayoutOptions,
        metrics: Metrics,
        scale: f32,
    ) -> Result<Result<Rc<Layout>, Fallback>, ViewError> {
        let font_epoch = rune::text::font_epoch();
        let key = LayoutKey { source: source.to_owned(), options: options.clone(), metrics, scale, font_epoch };
        let mut session = self.session.borrow_mut();
        if let Some(at) = session.layouts.iter().position(|layout| layout.key == key) {
            let layout = session.layouts.remove(at);
            session.layouts.push(layout.clone());
            return Ok(Ok(layout));
        }
        // The cache may have dropped it for sources that failed since.
        if let Some(layout) = session.drawn.as_ref().and_then(|drawn| drawn.layout(&key)) {
            return Ok(Ok(layout));
        }
        if let Some((_, why)) = session.refused.as_ref().filter(|(refused, _)| *refused == key) {
            return Ok(Err(why.clone()));
        }
        let measure = session.measure(&self.fonts, scale);
        let started = Instant::now();
        let laid_out = match pele_core::prepare(source, ParseOptions::default()) {
            Err(RenderError::NotPorted) => Err(Fallback::NotPorted),
            Err(error) => return Err(ViewError::Core(error)),
            // No Scene is laid out for a kind that is not drawn from one.
            Ok(prepared) if !lower::NATIVE.contains(&prepared.kind()) => {
                Err(Fallback::NotLowered(Unsupported::Kind(prepared.kind())))
            }
            Ok(prepared) => {
                let scene = match metrics {
                    Metrics::Drawn => prepared.scene(NativeInputs { measure, options }),
                    // A parity draw pins an unset `now` to Pele's corpus clock. Metrics are not
                    // the clock: with none given, the view takes the system's, as with its fonts.
                    Metrics::Pele => {
                        let options =
                            LayoutOptions { now: options.now.or_else(|| Some(system_now())), ..options.clone() };
                        pele_core::parity_scene_with(source, ParseOptions::default(), &options)
                            .map(|parity| parity.scene().clone())
                    }
                };
                Ok((prepared.kind(), scene.map_err(ViewError::Core)?))
            }
        };
        let (kind, scene) = match laid_out {
            Ok(laid_out) => laid_out,
            Err(why) => {
                session.refused = Some((key, why.clone()));
                return Ok(Err(why));
            }
        };
        session.times.layout = started.elapsed();
        session.stats.layouts += 1;
        let layout = Rc::new(Layout {
            key,
            id: unique_id(),
            kind,
            scene: Rc::new(scene),
            narrows: OnceCell::new(),
            shrink: OnceCell::new(),
        });
        if session.layouts.len() == KEPT {
            session.layouts.remove(0);
        }
        session.layouts.push(layout.clone());
        Ok(Ok(layout))
    }

    /// `source` drawn by rune-mermaid, for `why` it is not drawn natively, at most `max_width`
    /// wide.
    fn rune_mermaid(&self, source: &str, dark: bool, max_width: Option<f32>, why: Fallback) -> Result<El, ViewError> {
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

    /// A kept rune-mermaid drawing, drawn again only for a change of dark mode or width:
    /// rune-mermaid has no colours to change without drawing again.
    fn redraw(&self, drawing: &MermaidDrawing, dark: bool, max_width: Option<f32>) -> El {
        if drawing.dark == dark && drawing.max_width == max_width {
            return drawing.el.clone();
        }
        let Some(el) = self.by_rune_mermaid(&drawing.source, dark, max_width) else {
            return drawing.el.clone();
        };
        let again = MermaidDrawing {
            source: drawing.source.clone(),
            max_width,
            why: drawing.why.clone(),
            dark,
            el: el.clone(),
        };
        self.session.borrow_mut().drawn = Some(Drawn::RuneMermaid(Rc::new(again)));
        el
    }
}
