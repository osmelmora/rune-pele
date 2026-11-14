//! Rune adapter for rune-pele. This is the only workspace crate that depends on Rune.
//!
//! A [`Diagram`] draws Mermaid source as native Rune elements: the Scene pele-core lays out,
//! measured with Rune's own shaper, lowered to keyed shapes and text in Pele's paint order.
//!
//! ```ignore
//! let pie = Diagram::new("pets", FontCatalog::rune());
//! fn view(app: &App, cx: &mut Cx<App>) -> El {
//!     app.pie.view(cx, &app.source, ViewOptions::new(480.0))
//!         .unwrap_or_else(|error| text(error.to_string()))
//! }
//! ```
//!
//! Layout is cached by source, layout options, device scale and Rune's font epoch, and painting
//! is a `memo` keyed by the layout and the resolved colours. So a palette or dark-mode change
//! repaints without parsing, laying out or measuring anything.

pub mod capture;
pub mod lower;
pub mod text;

use std::cell::RefCell;
use std::rc::Rc;

use pele_core::scene::Rgba;
use pele_core::{LayoutOptions, NativeInputs, ParseOptions, RenderError, Scene};
use rune::{Color, Cx, El, memo, tok};

pub use pele_core::scene::Role;
pub use text::FontCatalog;

use lower::Drawing;
use text::RuneMeasure;

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

    /// The roles in a Rune theme's colours, and Pele's series. Text on a light series is Rune's
    /// light-mode text colour, and on a dark one its dark-mode text colour.
    pub fn from_tokens(theme: &tok::Theme) -> Self {
        Palette::pele()
            .with(Role::Background, theme.bg)
            .with(Role::Foreground, theme.text)
            .with(Role::Border, theme.text_secondary)
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
}

impl ViewOptions {
    /// A view at most `width` points wide. A diagram that can draw itself narrower does.
    pub fn new(width: f32) -> Self {
        let layout = LayoutOptions { max_width: Some(f64::from(width)), ..LayoutOptions::default() };
        ViewOptions { layout, palette: None }
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
    /// The drawing has a fill Rune's shapes cannot cover correctly.
    Geometry(lower::Unsupported),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::Core(error) => write!(f, "{error}"),
            ViewError::NoFallback => f.write_str("this diagram type is not ported, and rune-mermaid cannot draw it"),
            ViewError::Option(name) => write!(f, "the {name} must be a positive number"),
            ViewError::Geometry(lower::Unsupported::CompoundFill) => {
                f.write_str("a fill with a hole cannot be drawn yet")
            }
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
    /// Rune's own rune-mermaid, for a type pele-core has not ported.
    RuneMermaid,
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

/// Everything that decides a layout. Colours are not part of it.
#[derive(Clone, Debug, PartialEq)]
struct LayoutKey {
    source: String,
    options: LayoutOptions,
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
    measure: Option<Measure>,
    /// Runs shaped by measurers since dropped.
    shaped_before: u64,
    next_id: u64,
    status: Option<RenderStatus>,
    last_scene: Option<Rc<Scene>>,
    stats: ViewStats,
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

    /// `source` drawn as native elements for this window. A type pele-core has not ported is drawn
    /// by rune-mermaid; any other failure is an error.
    pub fn view<S: 'static>(&self, cx: &mut Cx<S>, source: &str, options: ViewOptions) -> Result<El, ViewError> {
        let env = cx.env();
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if !options.layout.max_width.is_none_or(positive) {
            return Err(ViewError::Option("width"));
        }
        if !positive(options.layout.typography.normal.size) {
            return Err(ViewError::Option("font size"));
        }
        let layout = match self.layout(source, &options.layout, env.scale) {
            Ok(layout) => layout,
            Err(ViewError::Core(RenderError::NotPorted)) => return self.fallback(source, env.system_dark, &options),
            Err(error) => return Err(error),
        };
        let palette = options.palette.unwrap_or_else(|| Palette::from_tokens(&tok::theme(env.system_dark)));
        let paints = layout.scene.resolve_paints(palette.core(), env.system_dark);
        {
            let mut session = self.session.borrow_mut();
            session.status = Some(RenderStatus::Pele);
            session.last_scene = Some(layout.scene.clone());
        }
        Ok(memo(self.id.as_str(), (layout.id, &paints), || {
            self.session.borrow_mut().stats.paints += 1;
            layout.drawing.el(&paints).key(self.id.as_str())
        }))
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

    /// The layout for these inputs, from the cache or laid out now.
    fn layout(&self, source: &str, options: &LayoutOptions, scale: f32) -> Result<Rc<Layout>, ViewError> {
        let font_epoch = rune::text::font_epoch();
        let key = LayoutKey { source: source.to_owned(), options: options.clone(), scale, font_epoch };
        let mut session = self.session.borrow_mut();
        if let Some(at) = session.layouts.iter().position(|layout| layout.key == key) {
            let layout = session.layouts.remove(at);
            session.layouts.push(layout.clone());
            return Ok(layout);
        }
        if session.measure.as_ref().is_none_or(|m| m.scale != scale) {
            let before = session.measure.take().map_or(0, |m| m.measure.baselines().shaped());
            session.shaped_before += before;
            session.measure = Some(Measure { scale, measure: RuneMeasure::new(self.fonts.baselines(scale)) });
        }
        let measure = &session.measure.as_ref().expect("made above").measure;
        let prepared = pele_core::prepare(source, ParseOptions::default()).map_err(ViewError::Core)?;
        let scene = prepared.scene(NativeInputs { measure, options }).map_err(ViewError::Core)?;
        let drawing = Drawing::new(&scene, &mut measure.baselines(), scale).map_err(ViewError::Geometry)?;
        session.stats.layouts += 1;
        session.next_id += 1;
        let layout = Rc::new(Layout { key, id: session.next_id, scene: Rc::new(scene), drawing });
        if session.layouts.len() == KEPT {
            session.layouts.remove(0);
        }
        session.layouts.push(layout.clone());
        Ok(layout)
    }

    fn fallback(&self, source: &str, dark: bool, options: &ViewOptions) -> Result<El, ViewError> {
        let mut style = rune_mermaid::DiagramStyle::neutral(dark);
        if let Some(width) = options.layout.max_width {
            style.max_width = width as f32;
        }
        let drawn = rune_mermaid::render(source, &style).ok_or(ViewError::NoFallback)?;
        let mut session = self.session.borrow_mut();
        session.status = Some(RenderStatus::RuneMermaid);
        session.last_scene = None;
        Ok(drawn.key(self.id.as_str()))
    }
}
