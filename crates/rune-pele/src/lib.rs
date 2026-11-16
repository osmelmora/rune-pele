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
//! Layout is cached by source, layout options, device scale and Rune's font epoch, lowering by
//! layout and zoom, and painting is a `memo` keyed by the lowered drawing and the resolved
//! colours. So a palette or dark-mode change repaints without parsing, laying out or measuring
//! anything, and a zoom lowers again without laying out.
//!
//! [`ViewOptions::fit`] fits a diagram to the width its view has, which it reads from Rune's
//! layout.

pub mod bands;
pub mod capture;
pub mod fill;
mod fit;
pub mod geometry;
pub mod lower;
pub mod stroke;
pub mod text;

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pele_core::scene::{Rgba, ThemeSeries};
use pele_core::{DiagramKind, LayoutOptions, NativeInputs, ParseOptions, RenderError, Scene};
use rune::{Color, Cx, El, Tx, el, memo, tok};

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
        ViewOptions { layout, palette: None, metrics: Metrics::Drawn, zoom: 1.0, fit: None }
    }

    /// A view as wide as its parent, which fits the diagram to that width. A diagram that fits
    /// is drawn at its own size. One that does not, and draws itself narrower when it has less
    /// room, is laid out again for the width, as Pele's `mount` does. What is still too wide is
    /// drawn smaller, but never with plain text under [`ViewOptions::smallest_text`]: at that
    /// size the view scrolls sideways instead. A fit never draws a diagram larger than its size.
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
    /// the view's least zoom this over the plain text size, or 1 where that is less.
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
}

/// A layout lowered at one zoom, or why it is not drawn natively.
struct Lowered {
    layout: u64,
    zoom: f32,
    /// Names this drawing in the paint memo's version.
    id: u64,
    drawing: Result<Drawing, Unsupported>,
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
    lowered: Vec<Rc<Lowered>>,
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
    last_drawing: Option<Rc<Lowered>>,
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
        if !positive(f64::from(options.zoom)) {
            return Err(ViewError::Option("zoom"));
        }
        if !options.fit.is_none_or(|fit| positive(f64::from(fit.smallest_text))) {
            return Err(ViewError::Option("smallest text size"));
        }
        // The width a fitted view has, as Rune last laid it out: none before its first layout.
        let room = options.fit.and_then(|_| cx.layout_with(&self.frame_key(), |rect| rect[2]));
        let (layout, zoom) = match self.sized(source, &options, env.scale, room)? {
            Ok(sized) => sized,
            Err(why) => return self.rune_mermaid(source, env.system_dark, &options, room, why),
        };
        let lowered = self.lowered(&layout, env.scale, zoom);
        let drawing = match &lowered.drawing {
            Ok(drawing) => drawing,
            Err(why) => {
                let why = Fallback::NotLowered(why.clone());
                return self.rune_mermaid(source, env.system_dark, &options, room, why);
            }
        };
        let palette = options.palette.unwrap_or_else(|| Palette::from_tokens(&tok::theme(env.system_dark)));
        let paints = layout.scene.resolve_paints_with(palette.core(), env.system_dark, palette.theme);
        {
            let mut session = self.session.borrow_mut();
            session.status = Some(RenderStatus::Pele);
            session.fallback = None;
            session.last_scene = Some(layout.scene.clone());
            session.last_drawing = Some(lowered.clone());
        }
        // The handler is part of what is painted, so another one paints again.
        let handler = on_event.map(|handler| handler as usize);
        let painted = memo(self.id.as_str(), (lowered.id, &paints, handler), || {
            let started = Instant::now();
            let painted = drawing.el(&paints, on_event).key(self.id.as_str());
            let mut session = self.session.borrow_mut();
            session.stats.paints += 1;
            session.times.paint = started.elapsed();
            painted
        });
        Ok(self.framed(options.fit, painted))
    }

    /// The key of a fitted view's frame, whose width it fits.
    fn frame_key(&self) -> String {
        format!("{}~fit", self.id)
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
    ) -> Result<Result<(Rc<Layout>, f32), Fallback>, ViewError> {
        let natural = match self.layout(source, &options.layout, options.metrics, scale)? {
            Ok(layout) => layout,
            Err(why) => return Ok(Err(why)),
        };
        let room = room.map(f64::from).filter(|room| room.is_finite() && *room > 0.0);
        let (Some(fit), Some(room)) = (options.fit, room) else {
            return Ok(Ok((natural, options.zoom)));
        };
        if natural.scene.size().width <= room {
            return Ok(Ok((natural, options.zoom)));
        }
        let laid = if self.narrows(source, options, scale, &natural) {
            let at = LayoutOptions { max_width: Some(fit::step_below(room)), ..options.layout.clone() };
            match self.layout(source, &at, options.metrics, scale)? {
                Ok(layout) => layout,
                Err(why) => return Ok(Err(why)),
            }
        } else {
            natural
        };
        let least = f64::from(fit.smallest_text) / options.layout.typography.normal.size;
        let zoom = fit::zoom_into(room, laid.scene.size().width, least) as f32;
        Ok(Ok((laid, zoom * options.zoom)))
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
    fn lowered(&self, layout: &Layout, scale: f32, zoom: f32) -> Rc<Lowered> {
        let mut session = self.session.borrow_mut();
        if let Some(at) = session.lowered.iter().position(|lowered| lowered.layout == layout.id && lowered.zoom == zoom)
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
        session.next_id += 1;
        let lowered = Rc::new(Lowered { layout: layout.id, zoom, id: session.next_id, drawing });
        if session.lowered.len() == KEPT {
            session.lowered.remove(0);
        }
        session.lowered.push(lowered.clone());
        lowered
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
        session.last_drawing.as_ref()?.drawing.as_ref().ok().map(Drawing::census)
    }

    /// The zoom the last native view was drawn at: 1 at the size the diagram is laid out at.
    pub fn zoom(&self) -> Option<f32> {
        self.session.borrow().last_drawing.as_ref().map(|lowered| lowered.zoom)
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
                    Metrics::Pele => pele_core::parity_scene_with(source, ParseOptions::default(), options)
                        .map(|parity| parity.scene().clone()),
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
        session.next_id += 1;
        let layout =
            Rc::new(Layout { key, id: session.next_id, kind, scene: Rc::new(scene), narrows: OnceCell::new() });
        if session.layouts.len() == KEPT {
            session.layouts.remove(0);
        }
        session.layouts.push(layout.clone());
        Ok(Ok(layout))
    }

    /// `source` drawn by rune-mermaid, for `why` it is not drawn natively, at most as wide as
    /// the view's width or a fitted view's `room`.
    fn rune_mermaid(
        &self,
        source: &str,
        dark: bool,
        options: &ViewOptions,
        room: Option<f32>,
        why: Fallback,
    ) -> Result<El, ViewError> {
        let mut style = rune_mermaid::DiagramStyle::neutral(dark);
        if let Some(width) = options.layout.max_width.map(|width| width as f32).or(room) {
            style.max_width = width;
        }
        let drawn = rune_mermaid::render(source, &style).ok_or(match &why {
            Fallback::NotPorted => ViewError::NoFallback,
            Fallback::NotLowered(unsupported) => ViewError::Geometry(unsupported.clone()),
        })?;
        let mut session = self.session.borrow_mut();
        session.status = Some(RenderStatus::RuneMermaid);
        session.fallback = Some(why);
        session.last_scene = None;
        session.last_drawing = None;
        Ok(self.framed(options.fit, drawn.key(self.id.as_str())))
    }
}

impl Session {
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
