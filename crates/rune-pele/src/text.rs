//! Text placed as SVG places it: by a point on its alphabetic baseline and a `text-anchor`.
//!
//! Rune lays a text element out from its box's top-left. Its painter puts the first line's
//! baseline at `round(top_px + line.baseline)` device pixels, where `line.baseline` is the shaped
//! layout's own line metric. So a box at `top = y - line.baseline / scale` lands that baseline on
//! `round(y * scale)`, with no per-font constant.
//!
//! Layout measures widths with the same shaper at the same device scale, so the advance a label
//! was laid out with is the advance it is drawn with.
//!
//! An app takes [`FontCatalog`] from here or from the crate's root. The rest is public for the
//! workspace's tests and examples, and is no part of the API.

use std::cell::RefCell;

use pele_core::{Font, FontFamily, FontWeight, TextMeasure, TextMeasurer, TextRef};
use rune::text::TextSystem;
use rune::{El, TextStyle, text};

/// SVG's `text-anchor`: which point of the run's advance sits at `x`.
/// Not part of the API: public for the workspace's tests and examples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    pub(crate) fn fraction(self) -> f32 {
        match self {
            Anchor::Start => 0.0,
            Anchor::Middle => 0.5,
            Anchor::End => 1.0,
        }
    }
}

/// Where an SVG `<text x y text-anchor>` puts its run, in points.
/// Not part of the API: public for the workspace's tests and examples.
#[derive(Clone, Copy, Debug, PartialEq)]
#[doc(hidden)]
pub struct BaselinePoint {
    pub x: f32,
    pub y: f32,
    pub anchor: Anchor,
}

/// One shaped line, in points from the top-left of the box a text element lays out.
/// Not part of the API: public for the workspace's tests and examples.
#[derive(Clone, Copy, Debug, PartialEq)]
#[doc(hidden)]
pub struct LineMetrics {
    pub baseline: f32,
    pub advance: f32,
    /// The height of the box the line is laid out in.
    pub height: f32,
}

/// Shapes runs with the fonts and families of the window that draws them, at its device scale.
/// Not part of the API: public for the workspace's tests and examples.
#[doc(hidden)]
pub struct Baselines {
    system: TextSystem,
    scale: f32,
    shaped: u64,
}

impl std::fmt::Debug for Baselines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Baselines").field("scale", &self.scale).field("shaped", &self.shaped).finish_non_exhaustive()
    }
}

impl Baselines {
    /// `fonts` and the families are the ones the app gives Rune; empty `fonts` is Rune's own.
    pub fn new(fonts: &[&'static [u8]], sans: &str, mono: &str, scale: f32) -> Self {
        // `TextSystem::new` notes its fonts as the ones `rune::text::measure` uses. Inside this
        // guard the note lands in a thread-scoped copy that the guard discards.
        let _scoped = rune::text::with_families(sans, mono);
        Baselines { system: TextSystem::new(fonts, (sans, mono)), scale, shaped: 0 }
    }

    pub fn line(&mut self, content: &str, style: TextStyle) -> LineMetrics {
        self.shaped += 1;
        let layout = self.system.layout_rich(content, &[], style, self.scale, None);
        let baseline = layout.lines().next().map_or(0.0, |line| line.metrics().baseline);
        LineMetrics {
            baseline: baseline / self.scale,
            advance: layout.full_width() / self.scale,
            height: layout.height() / self.scale,
        }
    }

    /// How many runs these baselines have shaped.
    pub(crate) fn shaped(&self) -> u64 {
        self.shaped
    }

    /// A one-line text element whose baseline lands on `at.y`, anchored at `at.x`.
    pub fn place(&mut self, content: &str, style: TextStyle, at: BaselinePoint) -> El {
        let line = self.line(content, style);
        text(content).font(style).absolute(at.x - at.anchor.fraction() * line.advance, at.y - line.baseline)
    }
}

/// The fonts a viewer measures and draws with. Give it the font bytes and families the app gives
/// Rune, so that labels are laid out in the fonts they are drawn in.
#[derive(Clone, Debug)]
pub struct FontCatalog {
    fonts: &'static [&'static [u8]],
    sans: String,
    mono: String,
}

impl FontCatalog {
    /// The fonts the app gives Rune in `AppSpec::fonts`, and the families its sans and monospace
    /// text is set in. Empty `fonts` is Rune's own.
    pub fn new(fonts: &'static [&'static [u8]], sans: &str, mono: &str) -> Self {
        FontCatalog { fonts, sans: sans.to_owned(), mono: mono.to_owned() }
    }

    /// The fonts and families a window has when the app gives none in its `AppSpec`: Inter, which
    /// Rune embeds with its `embedded-fonts` feature, and for monospace the platform's monospaced
    /// UI face natively and IBM Plex Mono on the web.
    pub fn rune() -> Self {
        // Read from `AppSpec::new`, so the families follow Rune's defaults from one Rune to the next.
        fn nothing(_: &(), _: &mut rune::Cx<()>) -> El {
            rune::el()
        }
        let (sans, mono) = rune::AppSpec::new("", (0.0, 0.0), || (), nothing).families;
        FontCatalog::new(&[], sans, mono)
    }

    pub(crate) fn baselines(&self, scale: f32) -> Baselines {
        Baselines::new(self.fonts, &self.sans, &self.mono, scale)
    }
}

/// The Rune style a Scene font is drawn in.
/// Not part of the API: public for the workspace's tests and examples.
#[doc(hidden)]
pub fn style(font: &Font) -> TextStyle {
    let weight = match font.weight {
        FontWeight::Normal => 400.0,
        FontWeight::Bold => 700.0,
    };
    let style = TextStyle::new(font.size as f32, weight);
    let style = if font.family == FontFamily::Mono { style.mono() } else { style };
    if font.italic { style.italic() } else { style }
}

/// Widths for layout, from Rune's shaper at the window's device scale.
pub(crate) struct RuneMeasure {
    baselines: RefCell<Baselines>,
}

impl RuneMeasure {
    pub(crate) fn new(baselines: Baselines) -> Self {
        RuneMeasure { baselines: RefCell::new(baselines) }
    }

    pub(crate) fn baselines(&self) -> std::cell::RefMut<'_, Baselines> {
        self.baselines.borrow_mut()
    }
}

impl TextMeasurer for RuneMeasure {
    fn measure(&self, text: TextRef<'_>, font: &Font) -> TextMeasure {
        let content = String::from_utf16_lossy(text.utf16_units());
        TextMeasure { width: f64::from(self.baselines().line(&content, style(font)).advance) }
    }
}
