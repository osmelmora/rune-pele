//! Text placed as SVG places it: by a point on its alphabetic baseline and a `text-anchor`.
//!
//! Rune lays a text element out from its box's top-left. Its painter puts the first line's
//! baseline at `round(top_px + line.baseline)` device pixels, where `line.baseline` is the shaped
//! layout's own line metric. So a box at `top = y - line.baseline / scale` lands that baseline on
//! `round(y * scale)`, with no per-font constant.

use rune::text::TextSystem;
use rune::{El, TextStyle, text};

/// SVG's `text-anchor`: which point of the run's advance sits at `x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn fraction(self) -> f32 {
        match self {
            Anchor::Start => 0.0,
            Anchor::Middle => 0.5,
            Anchor::End => 1.0,
        }
    }
}

/// Where an SVG `<text x y text-anchor>` puts its run, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BaselinePoint {
    pub x: f32,
    pub y: f32,
    pub anchor: Anchor,
}

/// One shaped line, in points from the top-left of the box a text element lays out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    pub baseline: f32,
    pub advance: f32,
}

/// Shapes runs with the fonts and families of the window that draws them, at its device scale.
pub struct Baselines {
    system: TextSystem,
    scale: f32,
}

impl Baselines {
    /// `fonts` and the families are the ones the app gives Rune; empty `fonts` is Rune's own.
    pub fn new(fonts: &[&'static [u8]], sans: &str, mono: &str, scale: f32) -> Self {
        // `TextSystem::new` notes its fonts as the ones `rune::text::measure` uses. Inside this
        // guard the note lands in a thread-scoped copy that the guard discards.
        let _scoped = rune::text::with_families(sans, mono);
        Baselines { system: TextSystem::new(fonts, (sans, mono)), scale }
    }

    pub fn line(&mut self, content: &str, style: TextStyle) -> LineMetrics {
        let layout = self.system.layout_rich(content, &[], style, self.scale, None);
        let baseline = layout.lines().next().map_or(0.0, |line| line.metrics().baseline);
        LineMetrics { baseline: baseline / self.scale, advance: layout.full_width() / self.scale }
    }

    /// A one-line text element whose baseline lands on `at.y`, anchored at `at.x`.
    pub fn place(&mut self, content: &str, style: TextStyle, at: BaselinePoint) -> El {
        let line = self.line(content, style);
        text(content).font(style).absolute(at.x - at.anchor.fraction() * line.advance, at.y - line.baseline)
    }
}
