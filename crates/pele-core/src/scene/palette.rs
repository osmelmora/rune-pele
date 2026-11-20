//! Colours: the palette an interpreter brings, and a Scene's paints resolved against it.
//!
//! A Scene holds paints as expressions (a role, a series slot, text on a series), so a theme
//! change resolves them again and lays nothing out. The SVG writer never comes here: it spells
//! the expressions as Pele's CSS variables instead.

use super::{EVEN, Paint, Role, Scene, lowercase, srgb};
use crate::input::{JsText, Units};
use std::hash::{Hash, Hasher};

/// The series colours Pele falls back to, as its SVG spells them.
pub(crate) const PELE_SERIES: [&str; 8] =
    ["#4c78a8", "#f58518", "#54a24b", "#e45756", "#72b7b2", "#eeca3b", "#b279a2", "#9d755d"];

/// A colour in linear-light sRGB, with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub linear_srgb: [f64; 3],
    pub alpha: f64,
}

impl Hash for Rgba {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // `+ 0.0` hashes -0 as 0, which it equals.
        for c in [self.linear_srgb[0], self.linear_srgb[1], self.linear_srgb[2], self.alpha] {
            (c + 0.0).to_bits().hash(state);
        }
    }
}

fn decode(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(c: f64) -> f64 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

impl Rgba {
    /// A colour from sRGB-encoded channels and alpha, each from 0 to 1, as CSS writes them.
    pub fn from_srgb(channels: [f64; 3], alpha: f64) -> Self {
        Rgba { linear_srgb: channels.map(|c| decode(c.clamp(0.0, 1.0))), alpha: alpha.clamp(0.0, 1.0) }
    }

    /// `#rrggbb`.
    fn hex(spelling: &str) -> Self {
        let (rgb, alpha) = srgb(spelling).expect("a hex colour");
        Rgba::from_srgb(rgb, alpha)
    }

    /// The sRGB-encoded channels and alpha as bytes, as a PNG holds them.
    pub fn to_srgb8(self) -> [u8; 4] {
        let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let [r, g, b] = self.linear_srgb.map(encode);
        [byte(r), byte(g), byte(b), byte(self.alpha)]
    }

    /// Relative luminance, as Pele's `ink` weighs it.
    fn luminance(self) -> f64 {
        let [r, g, b] = self.linear_srgb;
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
}

const ROLES: [Role; 11] = [
    Role::Background,
    Role::Foreground,
    Role::Border,
    Role::OnLight,
    Role::OnDark,
    Role::Line,
    Role::Surface,
    Role::SurfaceAlt,
    Role::Muted,
    Role::Accent,
    Role::Critical,
];

fn index(role: Role) -> usize {
    ROLES.iter().position(|&known| known == role).expect("every role is listed")
}

/// The colours an interpreter draws a diagram's roles and series in. Only colours: nothing
/// here can change a layout.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Palette {
    roles: [Rgba; ROLES.len()],
    series: [Rgba; 8],
}

impl Default for Palette {
    fn default() -> Self {
        Palette::pele()
    }
}

impl Palette {
    /// Pele's own colours: the fallback each of its CSS variables has.
    pub fn pele() -> Self {
        // `ROOT_STYLE` in the SVG writer, and `--pele-fg-on-light` and `--pele-fg-on-dark`.
        let roles = ROLES.map(|role| match role {
            Role::Background | Role::OnDark => Rgba::hex("#fff"),
            Role::Foreground | Role::OnLight | Role::Line => Rgba::hex("#1f1f1f"),
            Role::Border => Rgba::hex("#8a8a8a"),
            Role::Surface => Rgba::hex("#f3f3f3"),
            Role::SurfaceAlt => Rgba::hex("#e6e6e6"),
            Role::Muted => Rgba::hex("#6e6e6e"),
            Role::Accent => Rgba::hex("#5b7bd5"),
            Role::Critical => Rgba::hex("#e45756"),
        });
        Palette { roles, series: PELE_SERIES.map(Rgba::hex) }
    }

    pub fn with(mut self, role: Role, colour: Rgba) -> Self {
        self.roles[index(role)] = colour;
        self
    }

    /// Sets the colour of the series at `position`, counted from 0. Positions wrap after eight.
    pub fn with_series(mut self, position: usize, colour: Rgba) -> Self {
        self.series[position % 8] = colour;
        self
    }

    pub fn role(&self, role: Role) -> Rgba {
        self.roles[index(role)]
    }

    pub fn series(&self, position: usize) -> Rgba {
        self.series[position % 8]
    }
}

/// A Scene's paints as colours. See [`Scene::resolve_paints`].
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct ResolvedPaints {
    roles: [Rgba; ROLES.len()],
    series: [Rgba; 8],
    /// The text colour on each series.
    ink: [Rgba; 8],
    /// The colours the source writes, each where it can be read without a page.
    written: Vec<Rgba>,
}

impl ResolvedPaints {
    /// The colour `paint` draws in, or `None` for no paint at all.
    pub fn get(&self, paint: Paint) -> Option<Rgba> {
        match paint {
            Paint::None => None,
            Paint::Role(role) => Some(self.roles[index(role)]),
            Paint::Series(slot) => Some(self.series[slot.index()]),
            Paint::OnSeries(slot) => Some(self.ink[slot.index()]),
            Paint::Source(id) => Some(self.written[id.0 as usize]),
            Paint::Mix { tint, percent, base } => {
                Some(mix(self.roles[index(tint)], self.roles[index(base)], percent / 100.0))
            }
        }
    }
}

/// Whose colour a series is drawn in where the source's theme config sets one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeSeries {
    /// The source's, as Pele's SVG declares it over the page's.
    Source,
    /// The palette's, as though the source set none.
    Palette,
}

impl Scene {
    /// The Scene's paints as colours. A series colour the source sets wins over `palette`'s, and
    /// `dark` picks the branch of one written with `light-dark()`. Text on a series is in
    /// whichever of the palette's on-light and on-dark colours contrasts with the series, by
    /// Pele's own rule. Pele's SVG does that only where the source sets the series; here it
    /// holds for every series, so labels stay readable in any palette. Nothing here measures or
    /// lays out. Not part of the API: public for rune-pele, which paints the Scene.
    #[doc(hidden)]
    pub fn resolve_paints(&self, palette: &Palette, dark: bool) -> ResolvedPaints {
        self.resolve_paints_with(palette, dark, ThemeSeries::Source)
    }

    /// [`Scene::resolve_paints`], with `theme` saying whose colour a series the source's theme
    /// config sets is drawn in. Text on it contrasts with that colour either way. Not part of the
    /// API: public for rune-pele, which paints the Scene.
    #[doc(hidden)]
    pub fn resolve_paints_with(&self, palette: &Palette, dark: bool, theme: ThemeSeries) -> ResolvedPaints {
        let mut series = palette.series;
        let themed = self.document.palette.slots().filter(|_| theme == ThemeSeries::Source);
        for (index, colour) in themed {
            if let Some(colour) = css_colour(&colour.spelling, dark) {
                series[index] = colour;
            }
        }
        let ink = series.map(|colour| {
            // A fill under half opacity shows an unknown backdrop, so the text keeps the colour
            // pie labels inherit.
            if colour.alpha < 0.5 {
                palette.role(Role::Background)
            } else if colour.luminance() > *EVEN {
                palette.role(Role::OnLight)
            } else {
                palette.role(Role::OnDark)
            }
        });
        // A colour that needs a page to be read is drawn as the theme colour it stands in for.
        let written = self
            .document
            .colours
            .iter()
            .map(|colour| css_colour(&colour.spelling, dark).unwrap_or(palette.role(colour.instead_of)))
            .collect();
        ResolvedPaints { roles: palette.roles, series, ink, written }
    }
}

/// CSS's `color-mix(in srgb, a share, b)`: the two colours' sRGB channels, each weighted by its
/// alpha, then by its share.
fn mix(a: Rgba, b: Rgba, share: f64) -> Rgba {
    let share = share.clamp(0.0, 1.0);
    let alpha = a.alpha * share + b.alpha * (1.0 - share);
    if alpha == 0.0 {
        return Rgba { linear_srgb: [0.0; 3], alpha: 0.0 };
    }
    let [a_rgb, b_rgb] = [a, b].map(|colour| colour.linear_srgb.map(encode));
    let channel = |i: usize| (a_rgb[i] * a.alpha * share + b_rgb[i] * b.alpha * (1.0 - share)) / alpha;
    Rgba { linear_srgb: [channel(0), channel(1), channel(2)].map(decode), alpha }
}

// CSS's named colours, in the spelling with "gray", which `color` also reads "grey" as.
const NAMED: &str = "aliceblue f0f8ff antiquewhite faebd7 aqua 00ffff aquamarine 7fffd4 azure f0ffff \
beige f5f5dc bisque ffe4c4 black 000000 blanchedalmond ffebcd blue 0000ff blueviolet 8a2be2 brown a52a2a \
burlywood deb887 cadetblue 5f9ea0 chartreuse 7fff00 chocolate d2691e coral ff7f50 cornflowerblue 6495ed \
cornsilk fff8dc crimson dc143c cyan 00ffff darkblue 00008b darkcyan 008b8b darkgoldenrod b8860b \
darkgray a9a9a9 darkgreen 006400 darkkhaki bdb76b darkmagenta 8b008b darkolivegreen 556b2f \
darkorange ff8c00 darkorchid 9932cc darkred 8b0000 darksalmon e9967a darkseagreen 8fbc8f \
darkslateblue 483d8b darkslategray 2f4f4f darkturquoise 00ced1 darkviolet 9400d3 deeppink ff1493 \
deepskyblue 00bfff dimgray 696969 dodgerblue 1e90ff firebrick b22222 floralwhite fffaf0 \
forestgreen 228b22 fuchsia ff00ff gainsboro dcdcdc ghostwhite f8f8ff gold ffd700 goldenrod daa520 \
gray 808080 green 008000 greenyellow adff2f honeydew f0fff0 hotpink ff69b4 indianred cd5c5c \
indigo 4b0082 ivory fffff0 khaki f0e68c lavender e6e6fa lavenderblush fff0f5 lawngreen 7cfc00 \
lemonchiffon fffacd lightblue add8e6 lightcoral f08080 lightcyan e0ffff lightgoldenrodyellow fafad2 \
lightgray d3d3d3 lightgreen 90ee90 lightpink ffb6c1 lightsalmon ffa07a lightseagreen 20b2aa \
lightskyblue 87cefa lightslategray 778899 lightsteelblue b0c4de lightyellow ffffe0 lime 00ff00 \
limegreen 32cd32 linen faf0e6 magenta ff00ff maroon 800000 mediumaquamarine 66cdaa mediumblue 0000cd \
mediumorchid ba55d3 mediumpurple 9370db mediumseagreen 3cb371 mediumslateblue 7b68ee \
mediumspringgreen 00fa9a mediumturquoise 48d1cc mediumvioletred c71585 midnightblue 191970 \
mintcream f5fffa mistyrose ffe4e1 moccasin ffe4b5 navajowhite ffdead navy 000080 oldlace fdf5e6 \
olive 808000 olivedrab 6b8e23 orange ffa500 orangered ff4500 orchid da70d6 palegoldenrod eee8aa \
palegreen 98fb98 paleturquoise afeeee palevioletred db7093 papayawhip ffefd5 peachpuff ffdab9 \
peru cd853f pink ffc0cb plum dda0dd powderblue b0e0e6 purple 800080 rebeccapurple 663399 red ff0000 \
rosybrown bc8f8f royalblue 4169e1 saddlebrown 8b4513 salmon fa8072 sandybrown f4a460 seagreen 2e8b57 \
seashell fff5ee sienna a0522d silver c0c0c0 skyblue 87ceeb slateblue 6a5acd slategray 708090 \
snow fffafa springgreen 00ff7f steelblue 4682b4 tan d2b48c teal 008080 thistle d8bfd8 tomato ff6347 \
turquoise 40e0d0 violet ee82ee wheat f5deb3 white ffffff whitesmoke f5f5f5 yellow ffff00 yellowgreen 9acd32";

fn named_hex(name: &str) -> Option<&'static str> {
    let mut words = NAMED.split(' ');
    while let (Some(known), Some(hex)) = (words.next(), words.next()) {
        if known == name {
            return Some(hex);
        }
    }
    None
}

/// A source's series colour as a colour, where it is one this can know without a page: a hex,
/// named, `rgb()` or `hsl()` colour, or `light-dark()` of two. `var()`, `color-mix()` and the
/// other functions Pele lets through need a browser, so they leave the palette's colour.
pub(super) fn css_colour(raw: &[u16], dark: bool) -> Option<Rgba> {
    let value = lowercase(raw.trim_js());
    if let Some(pair) = value.strip_prefix("light-dark(").and_then(|rest| rest.strip_suffix(')')) {
        let (light, dark_side) = split_top_comma(pair)?;
        let chosen: JsText = (if dark { dark_side } else { light }).into();
        return css_colour(&chosen, dark);
    }
    if value == "transparent" {
        return Some(Rgba { linear_srgb: [0.0; 3], alpha: 0.0 });
    }
    let name = value.replacen("grey", "gray", 1);
    if let Some(hex) = named_hex(&name) {
        return Some(Rgba::hex(&format!("#{hex}")));
    }
    let (rgb, alpha) = srgb(&value)?;
    (!rgb.iter().chain([&alpha]).any(|c| c.is_nan())).then(|| Rgba::from_srgb(rgb, alpha))
}

/// The words a page reads as a paint that this cannot read without one: CSS's keywords for a
/// value from elsewhere, and the system's colours.
pub(super) const PAGE_PAINTS: &str = "currentcolor inherit initial unset revert revert-layer context-fill context-stroke \
accentcolor accentcolortext activetext buttonborder buttonface buttontext canvas canvastext field fieldtext graytext \
highlight highlighttext linktext mark marktext selecteditem selecteditemtext visitedtext activeborder activecaption \
appworkspace background buttonhighlight buttonshadow captiontext inactiveborder inactivecaption inactivecaptiontext \
infobackground infotext menu menutext scrollbar threeddarkshadow threedface threedhighlight threedlightshadow \
threedshadow window windowframe windowtext";

/// The two sides of the one comma outside parentheses.
fn split_top_comma(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut at = None;
    for (index, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                if at.is_some() {
                    return None;
                }
                at = Some(index);
            }
            _ => {}
        }
    }
    let at = at?;
    Some((&text[..at], &text[at + 1..]))
}

#[cfg(test)]
mod tests {
    use super::super::{DARK, LIGHT, named};
    use super::*;

    fn colour(text: &str, dark: bool) -> Option<[u8; 4]> {
        css_colour(&text.encode_utf16().collect::<Vec<_>>(), dark).map(Rgba::to_srgb8)
    }

    #[test]
    fn every_name_pele_knows_has_a_colour_and_no_other() {
        let ours: Vec<&str> = NAMED.split(' ').step_by(2).collect();
        let pele: Vec<&str> = DARK.split(' ').chain(LIGHT.split(' ')).collect();
        for name in &pele {
            assert!(named_hex(name).is_some(), "{name} has a colour");
        }
        for name in &ours {
            assert!(named(DARK, name) || named(LIGHT, name), "Pele knows {name}");
        }
        assert_eq!(ours.len(), pele.len());
    }

    #[test]
    fn source_colours_read_as_css_reads_them() {
        assert_eq!(colour("#4c78a8", false), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(colour(" #ABC ", false), Some([0xaa, 0xbb, 0xcc, 255]));
        assert_eq!(colour("#11223380", false), Some([0x11, 0x22, 0x33, 0x80]));
        assert_eq!(colour("Grey", false), Some([0x80, 0x80, 0x80, 255]));
        assert_eq!(colour("rebeccapurple", false), Some([0x66, 0x33, 0x99, 255]));
        assert_eq!(colour("rgb(255, 0, 0)", false), Some([255, 0, 0, 255]));
        assert_eq!(colour("rgb(0 128 255 / 50%)", false), Some([0, 128, 255, 128]));
        assert_eq!(colour("hsl(120, 100%, 25%)", false), Some([0, 128, 0, 255]));
        assert_eq!(colour("transparent", false), Some([0, 0, 0, 0]));
        assert_eq!(colour("light-dark(#fff, rgb(1, 2, 3))", false), Some([255, 255, 255, 255]));
        assert_eq!(colour("light-dark(#fff, rgb(1, 2, 3))", true), Some([1, 2, 3, 255]));
        for unknown in ["var(--pele-accent)", "color-mix(in srgb, red, blue)", "currentcolor", "rgb(1, 2)", "nope"] {
            assert_eq!(colour(unknown, false), None, "{unknown}");
        }
    }

    #[test]
    fn pele_palette_round_trips_its_own_spellings() {
        let pele = Palette::pele();
        assert_eq!(pele.series(0).to_srgb8(), [0x4c, 0x78, 0xa8, 255]);
        assert_eq!(pele.series(9).to_srgb8(), [0xf5, 0x85, 0x18, 255], "positions wrap after eight");
        assert_eq!(pele.role(Role::Border).to_srgb8(), [0x8a, 0x8a, 0x8a, 255]);
    }
}
