//! The Scene: one ordered drawing that every interpreter reads.
//!
//! Paint order lives in the node tree. Identity lives in a separate table of entities, so one
//! section can own a slice in one layer and a legend row in another. Each number, path and text
//! is stored once, with the spelling Pele gives it kept beside the value: a number remembers
//! its number rule, a path remembers its commands, and each node remembers the order its
//! attributes were set in. Only this module builds a Scene, and `crate::svg` only reads one.

use crate::catalog::DrawContext;
use crate::input::{JsObject, JsText, JsValue, Units, decimal, decode_entities, is_digit, is_space, is_word};
use crate::measure::{Emphasis, Label, LabelLine, LabelRun};
use crate::{DiagramKind, RenderError, js};
use indexmap::IndexMap;
use std::sync::LazyLock;

mod glyph;
mod palette;
mod style;
mod walk;

pub use glyph::IconGlyph;

pub(crate) use palette::PELE_SERIES;
pub use palette::{Palette, ResolvedPaints, Rgba, ThemeSeries};
pub(crate) use style::{AuthorClasses, ResolvedStyle, Style, ink_on, paints, px, resolve_style};
pub use walk::{
    BaselineRun, Draw, FragmentDetail, FragmentKey, Geometry, Group, ImageFit, LinearGradient, LinkRef, PaintStyle,
    PaintVisit, PartKey, PathCommand, Rotation, StrokeStyle,
};

/// Whether XML can't hold `c`, so that Pele drops it from text: C0 controls but tab, line feed and
/// carriage return, DEL, and the two noncharacters U+FFFE and U+FFFF.
pub(crate) fn xml_drops(c: char) -> bool {
    matches!(c, '\0'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}' | '\u{fffe}' | '\u{ffff}')
}

/// Pele's `esc`: escapes text for XML and drops the characters XML cannot hold, DEL and any
/// surrogate that has no partner.
pub(crate) fn esc(text: &[u16], out: &mut String) {
    for c in char::decode_utf16(text.iter().copied()).flatten() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ if xml_drops(c) => {}
            _ => out.push(c),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// A theme colour by what it is for. The interpreter decides what colour that is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Background,
    /// Text and anything else the diagram draws in its own ink.
    Foreground,
    Border,
    /// Text on a light fill.
    OnLight,
    /// Text on a dark fill.
    OnDark,
    /// Edges and their arrowheads.
    Line,
    /// The fill of a node.
    Surface,
    /// The fill of a group of nodes.
    SurfaceAlt,
    /// Text that stands back, such as a group's title.
    Muted,
    /// What a diagram picks out, such as a gantt's tasks or a treeview's selected row.
    Accent,
    /// What a diagram marks as critical, such as a gantt's critical tasks. It is no series
    /// colour, as a theme may put those in any order.
    Critical,
}

/// One of the eight series colours. A position past the eighth wraps around.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeriesSlot(u8);

impl SeriesSlot {
    /// The slot of the series at `position`, counted from 0.
    pub fn new(position: usize) -> Self {
        SeriesSlot((position % 8) as u8)
    }

    /// From 0 to 7.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// A colour as an expression, so that a theme change needs no new layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    None,
    Role(Role),
    Series(SeriesSlot),
    /// Text on a series colour. Pele sets it only where the source overrides that series with a
    /// colour whose contrast is known, and otherwise leaves the text to inherit.
    OnSeries(SeriesSlot),
    /// A colour the source writes. [`Scene::resolve_paints`] reads it.
    Source(SourceColourId),
    /// `percent` of `tint` over `base`, mixed in sRGB.
    Mix {
        tint: Role,
        percent: f64,
        base: Role,
    },
}

/// One of the colours a source writes, by its place among those the Scene holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceColourId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    /// SVG's default join, with its miter limit of 4.
    Miter,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    /// SVG's default: a stroke ends where its path does.
    Butt,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorKind {
    Start,
    Middle,
    End,
}

/// The weight of text that a diagram sets as a whole, each a theme token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextWeight {
    Title,
    Bold,
    /// The weight of a heading inside a node, such as a class's name.
    Heading,
}

/// Which part of an entity a node draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Facet {
    Shape,
    Label,
    Legend,
    Title,
    /// The wide stroke in the page's colour under an edge that crosses a filled border.
    Casing,
    /// The arrowhead where an edge starts.
    StartMarker,
    /// The arrowhead where an edge ends.
    EndMarker,
    /// The outline in the colour behind a title that edges run through.
    Halo,
    /// A stereotype above a node's name, such as a class's `«interface»`.
    Annotation,
    /// One attribute of a node, one line each.
    Attribute,
    /// One method of a node, one line each.
    Method,
    /// The text where an edge starts, such as a cardinality.
    StartLabel,
    /// The text where an edge ends.
    EndLabel,
    /// The kind an element is tagged with, such as `«service»`, beside its label.
    Stereotype,
    /// The cells of a table inside a node, such as the keys and values of a JSON object.
    Row,
}

/// Where a diagram type's theme variables set its series colours, as Pele's `PALETTES` says.
#[derive(Clone, Copy)]
pub(crate) enum PaletteRule {
    /// The type has no series colours.
    None,
    /// One variable per series: the name before the number, and the number of the variable for
    /// the first series.
    Slots(&'static str, usize),
    /// As `Slots`, but the colours that are set repeat in order to fill all eight series, as
    /// venn's do.
    Repeated(&'static str, usize),
    /// xychart's `xyChart.plotColorPalette`: colours between commas, repeated to fill all eight.
    PlotColors,
}

/// A series colour the source sets, as it was accepted and as it is written.
#[derive(Clone, Debug)]
pub(crate) struct SourceColour {
    pub(crate) spelling: JsText,
    /// The text colour that contrasts with it, where that is known.
    pub(crate) ink: Option<Role>,
}

/// The series colours a source overrides through `themeVariables`.
#[derive(Clone, Debug, Default)]
pub(crate) struct SourcePalette {
    slots: [Option<SourceColour>; 8],
}

impl SourcePalette {
    /// Pele's `palette`: an entry that is not a usable colour leaves its series in the theme's colour.
    pub(crate) fn from_config(rule: PaletteRule, config: &JsObject) -> Self {
        let variables = config.get("themeVariables").and_then(JsValue::as_record);
        let numbered = |prefix: &str, first: usize| -> Vec<Option<JsValue>> {
            (0..8).map(|index| variables?.get(&format!("{prefix}{}", first + index)).cloned()).collect()
        };
        // The entries for the series in turn, repeated when there are fewer than eight.
        let list = match rule {
            PaletteRule::None => return SourcePalette::default(),
            PaletteRule::Slots(prefix, first) => numbered(prefix, first),
            PaletteRule::Repeated(prefix, first) => numbered(prefix, first)
                .into_iter()
                .filter(|entry| match entry {
                    None | Some(JsValue::Null) => false,
                    Some(JsValue::String(text)) => !text.is_empty(),
                    Some(_) => true,
                })
                .collect(),
            PaletteRule::PlotColors => {
                let list = variables
                    .and_then(|variables| variables.get("xyChart")?.as_record()?.get("plotColorPalette")?.as_text());
                let Some(list) = list else {
                    return SourcePalette::default();
                };
                split_colours(list).into_iter().map(|part| Some(JsValue::String(part.into()))).collect()
            }
        };
        if list.is_empty() {
            return SourcePalette::default();
        }
        let slot = |index: usize| {
            let spelling = safe_color(list[index % list.len()].as_ref()?)?;
            let ink = ink(&spelling);
            Some(SourceColour { spelling, ink })
        };
        SourcePalette { slots: std::array::from_fn(slot) }
    }

    pub(crate) fn slots(&self) -> impl Iterator<Item = (usize, &SourceColour)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| Some((index, slot.as_ref()?)))
    }

    pub(crate) fn ink(&self, slot: SeriesSlot) -> Option<Role> {
        self.slots[slot.index()].as_ref()?.ink
    }
}

/// Pele's `splitList`: the colours between the commas of a list, not those inside `rgb()` and
/// other functions, each trimmed.
fn split_colours(list: &[u16]) -> Vec<&[u16]> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (index, &c) in list.iter().enumerate() {
        if c == u16::from(b'(') {
            depth += 1;
        } else if c == u16::from(b')') {
            depth -= 1;
        } else if c == u16::from(b',') && depth == 0 {
            parts.push(&list[start..index]);
            start = index + 1;
        }
    }
    parts.push(&list[start..]);
    parts.into_iter().map(|part| part.trim_js()).collect()
}

const FUNCTIONS: [&str; 13] =
    ["rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color", "color-mix", "light-dark", "var"];

fn lowercase(text: &[u16]) -> String {
    text.iter().map(|&c| char::from_u32(u32::from(c)).map_or('\u{fffd}', |c| c.to_ascii_lowercase())).collect()
}

/// Pele's `safeColor`: a colour that can't load a resource, end its declaration or read the
/// page's own variables. Values outside functions are checked as colours.
fn safe_color(value: &JsValue) -> Option<JsText> {
    let color = value.as_text()?.trim_js();
    // /^[\w\s#%.,+\-/()]{1,200}$/
    let allowed = |c: u16| is_word(c) || is_space(c) || u8::try_from(c).is_ok_and(|c| b"#%.,+-/()".contains(&c));
    if color.is_empty() || color.len() > 200 || !color.iter().all(|&c| allowed(c)) {
        return None;
    }
    let (open, close) = (u16::from(b'('), u16::from(b')'));
    if !color.contains(&open) {
        return plain_color(color).then(|| color.into());
    }
    let mut depth = 0i32;
    for &c in color {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth < 0 {
                return None;
            }
        }
    }
    if depth != 0 {
        return None;
    }
    let name_unit = |c: u16| is_word(c) || c == u16::from(b'-');
    // /([\w-]*)\s*\(/g
    let mut at = 0;
    while at < color.len() {
        let name_end = color.run(at, name_unit);
        let paren = color.run(name_end, is_space);
        if color.at(paren) == Some(open) {
            if !FUNCTIONS.contains(&lowercase(&color[at..name_end]).as_str()) {
                return None;
            }
            at = paren + 1;
        } else {
            at += 1;
        }
    }
    // /var\(\s*([\w-]*)/gi
    let mut at = 0;
    while at < color.len() {
        if lowercase(&color[at..(at + 4).min(color.len())]) == "var(" {
            let name = color.run(at + 4, is_space);
            if !color.starts_at(name, "--pele-") {
                return None;
            }
            at = color.run(name, name_unit);
        } else {
            at += 1;
        }
    }
    Some(color.into())
}

// Named colours that #fff contrasts with more than #1f1f1f does, and the rest. Copied from
// Pele's `src/svg/ink.ts`.
const DARK: &str = "black blue blueviolet brown crimson darkblue darkcyan darkgreen darkmagenta darkolivegreen \
darkorchid darkred darkslateblue darkslategray darkviolet dimgray firebrick forestgreen green \
indigo maroon mediumblue mediumslateblue mediumvioletred midnightblue navy olive purple \
rebeccapurple royalblue saddlebrown seagreen sienna slateblue steelblue teal";
const LIGHT: &str = "aliceblue antiquewhite aqua aquamarine azure beige bisque blanchedalmond burlywood cadetblue \
chartreuse chocolate coral cornflowerblue cornsilk cyan darkgoldenrod darkgray darkkhaki darkorange \
darksalmon darkseagreen darkturquoise deeppink deepskyblue dodgerblue floralwhite fuchsia gainsboro \
ghostwhite gold goldenrod gray greenyellow honeydew hotpink indianred ivory khaki lavender \
lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray \
lightgreen lightpink lightsalmon lightseagreen lightskyblue lightslategray lightsteelblue \
lightyellow lime limegreen linen magenta mediumaquamarine mediumorchid mediumpurple mediumseagreen \
mediumspringgreen mediumturquoise mintcream mistyrose moccasin navajowhite oldlace olivedrab orange \
orangered orchid palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink \
plum powderblue red rosybrown salmon sandybrown seashell silver skyblue slategray snow springgreen \
tan thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen";

fn named(list: &str, name: &str) -> bool {
    list.split(' ').any(|known| known == name)
}

/// `/^#(?:[\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i`
fn is_hex_color(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|digits| {
        matches!(digits.len(), 3 | 4 | 6 | 8) && digits.bytes().all(|digit| digit.is_ascii_hexdigit())
    })
}

/// Pele's `plainColor`: a hex colour or one of CSS's named colours.
fn plain_color(raw: &[u16]) -> bool {
    let value = lowercase(raw.trim_js());
    let name = value.replacen("grey", "gray", 1);
    is_hex_color(&value)
        || named(DARK, &name)
        || named(LIGHT, &name)
        || value == "transparent"
        || value == "currentcolor"
}

fn linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { js::pow((c + 0.055) / 1.055, 2.4) }
}

fn luminance(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// Luminance where #1f1f1f and #fff have equal contrast.
static EVEN: LazyLock<f64> =
    LazyLock::new(|| (1.05 * (luminance(31.0 / 255.0, 31.0 / 255.0, 31.0 / 255.0) + 0.05)).sqrt() - 0.05);

fn clamp(n: f64) -> f64 {
    js::min(js::max(n, 0.0), 1.0)
}

/// What `/^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?(%|deg|grad|rad|turn)?$/i` finds in lowercase
/// text: the number and its unit.
fn css_number(raw: &str) -> Option<(f64, &str)> {
    let units: Vec<u16> = raw.encode_utf16().collect();
    let sign = usize::from(matches!(units.first(), Some(43 | 45)));
    let whole = units.run(sign, is_digit);
    let mut end = whole;
    if whole > sign {
        let fraction = units.run(whole + 1, is_digit);
        if units.at(whole) == Some(u16::from(b'.')) && fraction > whole + 1 {
            end = fraction;
        }
    } else {
        end = units.run(sign + 1, is_digit);
        if units.at(sign) != Some(u16::from(b'.')) || end == sign + 1 {
            return None;
        }
    }
    if units.at(end) == Some(u16::from(b'e')) {
        let digits = end + 1 + usize::from(matches!(units.at(end + 1), Some(43 | 45)));
        let exponent = units.run(digits, is_digit);
        if exponent > digits {
            end = exponent;
        }
    }
    let unit = &raw[end..];
    ["", "%", "deg", "grad", "rad", "turn"].contains(&unit).then(|| (decimal(&units[..end]), unit))
}

fn amount(raw: &str, full: f64) -> f64 {
    match css_number(raw) {
        Some((n, "%")) => clamp(n / 100.0),
        Some((n, "")) => clamp(n / full),
        _ => f64::NAN,
    }
}

fn hue(raw: &str) -> f64 {
    match css_number(raw) {
        Some((n, "turn")) => n * 360.0,
        Some((n, "rad")) => (n * 180.0) / std::f64::consts::PI,
        Some((n, "grad")) => n * 0.9,
        Some((n, "" | "deg")) => n,
        _ => f64::NAN,
    }
}

/// The comma syntax takes no `none`, rgb() channels all or none in %, and hsl() saturation and lightness in %.
fn color_args(body: &str, hsl: bool) -> Option<Vec<String>> {
    let legacy = body.contains(',');
    let mut parts = if legacy { vec![body] } else { body.split('/').collect() };
    if parts.len() > 2 {
        return None;
    }
    let alpha = (parts.len() == 2).then(|| parts.remove(1));
    let main = js_trim(parts[0]);
    let mut list: Vec<String> =
        if legacy { main.split(',').map(|part| js_trim(part).to_owned()).collect() } else { split_spaces(main) };
    if if legacy { list.len() < 3 || list.len() > 4 } else { list.len() != 3 } {
        return None;
    }
    if let Some(alpha) = alpha {
        list.push(js_trim(alpha).to_owned());
    }
    if !legacy {
        return Some(list.into_iter().map(|c| if c == "none" { "0".to_owned() } else { c }).collect());
    }
    let percent = list[usize::from(hsl)..3].iter().filter(|c| c.ends_with('%')).count();
    (percent == if hsl { 2 } else { 3 } || (!hsl && percent == 0)).then_some(list)
}

fn is_space_char(c: char) -> bool {
    u16::try_from(u32::from(c)).is_ok_and(is_space)
}

fn js_trim(text: &str) -> &str {
    text.trim_matches(is_space_char)
}

/// `text.split(/\s+/)`
fn split_spaces(text: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut in_space = false;
    for c in text.chars() {
        if is_space_char(c) {
            in_space = true;
        } else {
            if in_space {
                parts.push(String::new());
            }
            in_space = false;
            parts.last_mut().expect("one part").push(c);
        }
    }
    if in_space {
        parts.push(String::new());
    }
    parts
}

/// Pele's `color`: luminance and alpha. Named colours use 0 or 1 to preserve the contrast choice.
fn color(raw: &[u16]) -> Option<(f64, f64)> {
    let value = lowercase(raw.trim_js());
    let name = value.replacen("grey", "gray", 1);
    if named(DARK, &name) {
        return Some((0.0, 1.0));
    }
    if named(LIGHT, &name) {
        return Some((1.0, 1.0));
    }
    let (rgb, alpha) = srgb(&value)?;
    let lum = luminance(rgb[0], rgb[1], rgb[2]);
    (!(lum + alpha).is_nan()).then_some((lum, alpha))
}

/// The sRGB channels from 0 to 1, and the alpha, of a lowercase hex colour or `rgb()`, `rgba()`,
/// `hsl()` or `hsla()`, as Pele's `color` reads them. A part that does not read is NaN.
fn srgb(value: &str) -> Option<([f64; 3], f64)> {
    if is_hex_color(value) {
        let digits: Vec<u32> = value[1..].chars().filter_map(|c| c.to_digit(16)).collect();
        let digits: Vec<u32> = if value.len() < 6 { digits.iter().flat_map(|&d| [d, d]).collect() } else { digits };
        let n = |i: usize| f64::from(digits[2 * i] * 16 + digits[2 * i + 1]) / 255.0;
        return Some(([n(0), n(1), n(2)], if digits.len() == 8 { n(3) } else { 1.0 }));
    }
    // /^(rgba?|hsla?)\(([^()]*)\)$/i
    let (function, rest) = value.split_once('(')?;
    let body = rest.strip_suffix(')')?;
    if !["rgb", "rgba", "hsl", "hsla"].contains(&function) || body.contains(['(', ')']) {
        return None;
    }
    let hsl = function.starts_with("hsl");
    let list = color_args(body, hsl)?;
    let alpha = if list.len() == 4 { amount(&list[3], 1.0) } else { 1.0 };
    let rgb = if hsl {
        let h = (((hue(&list[0]) % 360.0) + 360.0) % 360.0) / 30.0;
        let s = amount(&list[1], 100.0);
        let l = amount(&list[2], 100.0);
        let a = s * js::min(l, 1.0 - l);
        [0.0, 8.0, 4.0].map(|k| {
            let t = (k + h) % 12.0;
            l - a * js::max(-1.0, js::min(js::min(t - 3.0, 9.0 - t), 1.0))
        })
    } else {
        [amount(&list[0], 255.0), amount(&list[1], 255.0), amount(&list[2], 255.0)]
    };
    Some((rgb, alpha))
}

/// Pele's `ink` for a fill with no opacity of its own: the text colour that contrasts with it.
/// A fill below half opacity depends on an unknown background, so it has none.
fn ink(fill: &[u16]) -> Option<Role> {
    // `color` never answers NaN, so this is Pele's `!(alpha >= 0.5)`.
    let (lum, alpha) = color(fill)?;
    if alpha < 0.5 {
        return None;
    }
    Some(if lum > *EVEN { Role::OnLight } else { Role::OnDark })
}

/// One raw value and the rule that spells it. Every spelling goes through `crate::js`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Number {
    value: f64,
    form: NumberForm,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NumberForm {
    /// Pele's `num()`: hundredths.
    Num,
    /// `String(x)`.
    Raw,
    /// `x.toFixed(digits)`.
    Fixed(u8),
    /// A minus sign before Pele's `num()` of the value, as a template writes `-${num(x)}`. It
    /// differs from `num(-x)` where the hundredths end in exactly a half.
    MinusNum,
}

impl Number {
    pub(crate) fn num(value: f64) -> Self {
        Number { value, form: NumberForm::Num }
    }

    pub(crate) fn raw(value: f64) -> Self {
        Number { value, form: NumberForm::Raw }
    }

    pub(crate) fn fixed(value: f64, digits: u8) -> Self {
        Number { value, form: NumberForm::Fixed(digits) }
    }

    /// `-${num(value)}`, for a positive value.
    pub(crate) fn minus_num(value: f64) -> Self {
        Number { value, form: NumberForm::MinusNum }
    }

    pub(crate) fn write(&self, out: &mut String) {
        match self.form {
            NumberForm::Num => js::write_num(out, self.value),
            NumberForm::Raw => out.push_str(&js::to_string(self.value)),
            NumberForm::Fixed(digits) => out.push_str(&js::to_fixed(self.value, usize::from(digits))),
            NumberForm::MinusNum => {
                out.push('-');
                js::write_num(out, self.value);
            }
        }
    }

    /// The number `write` spells, so that what an interpreter draws is what the SVG says.
    pub(crate) fn effective(&self) -> f64 {
        match self.form {
            NumberForm::Num => {
                let hundredths = js::round(self.value * 100.0);
                if hundredths == 0.0 { 0.0 } else { hundredths / 100.0 }
            }
            NumberForm::Raw => self.value,
            NumberForm::Fixed(digits) => {
                js::to_fixed(self.value, usize::from(digits)).parse().expect("toFixed spells a number")
            }
            NumberForm::MinusNum => -Number::num(self.value).effective(),
        }
    }
}

/// A path as the commands Pele writes: no separator before a command, a comma inside a pair, a
/// space between pairs and around an arc's flags. A command whose letter Pele writes in
/// lowercase moves by its numbers instead of to them.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct PathProgram {
    pub(crate) steps: Vec<PathStep>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PathStep {
    Move([Number; 2]),
    Line([Number; 2]),
    Horizontal(Number),
    Vertical(Number),
    /// Two handles, then the end.
    Cubic([[Number; 2]; 3]),
    /// One handle, then the end.
    Quad([[Number; 2]; 2]),
    /// An unrotated elliptical arc.
    Arc {
        radii: [Number; 2],
        large: bool,
        sweep: bool,
        end: [Number; 2],
    },
    Close,
    LineBy([Number; 2]),
    HorizontalBy(Number),
    QuadBy([[Number; 2]; 2]),
    /// A quadratic curve whose handle mirrors the one before it.
    SmoothQuadBy([Number; 2]),
    /// An unrotated elliptical arc that ends `end` away from where it starts.
    ArcBy {
        radii: [Number; 2],
        large: bool,
        sweep: bool,
        end: [Number; 2],
    },
    VerticalBy(Number),
    /// Pele's lowercase `z`, which closes the subpath as `Z` does.
    CloseBy,
    /// [`PathStep::Arc`] with its two flags written as one pair, `0 1,1`, as cynefin's ellipse is.
    PairedArc {
        radii: [Number; 2],
        large: bool,
        sweep: bool,
        end: [Number; 2],
    },
    /// A space Pele writes between two commands, as radar's curves are written. It draws nothing.
    Space,
}

impl PathProgram {
    fn then(mut self, step: PathStep) -> Self {
        self.steps.push(step);
        self
    }

    pub(crate) fn m(self, x: Number, y: Number) -> Self {
        self.then(PathStep::Move([x, y]))
    }

    pub(crate) fn l(self, x: Number, y: Number) -> Self {
        self.then(PathStep::Line([x, y]))
    }

    pub(crate) fn h(self, x: Number) -> Self {
        self.then(PathStep::Horizontal(x))
    }

    pub(crate) fn v(self, y: Number) -> Self {
        self.then(PathStep::Vertical(y))
    }

    pub(crate) fn c(self, first: [Number; 2], second: [Number; 2], end: [Number; 2]) -> Self {
        self.then(PathStep::Cubic([first, second, end]))
    }

    pub(crate) fn q(self, handle: [Number; 2], end: [Number; 2]) -> Self {
        self.then(PathStep::Quad([handle, end]))
    }

    /// A space before the next command.
    pub(crate) fn space(self) -> Self {
        self.then(PathStep::Space)
    }

    /// An arc of less than half a turn.
    pub(crate) fn a(self, radii: [Number; 2], sweep: bool, end: [Number; 2]) -> Self {
        self.then(PathStep::Arc { radii, large: false, sweep, end })
    }

    /// An arc of more than half a turn.
    pub(crate) fn a_large(self, radii: [Number; 2], sweep: bool, end: [Number; 2]) -> Self {
        self.then(PathStep::Arc { radii, large: true, sweep, end })
    }

    pub(crate) fn z(self) -> Self {
        self.then(PathStep::Close)
    }

    pub(crate) fn l_by(self, x: Number, y: Number) -> Self {
        self.then(PathStep::LineBy([x, y]))
    }

    pub(crate) fn h_by(self, x: Number) -> Self {
        self.then(PathStep::HorizontalBy(x))
    }

    pub(crate) fn v_by(self, y: Number) -> Self {
        self.then(PathStep::VerticalBy(y))
    }

    pub(crate) fn q_by(self, handle: [Number; 2], end: [Number; 2]) -> Self {
        self.then(PathStep::QuadBy([handle, end]))
    }

    pub(crate) fn t_by(self, x: Number, y: Number) -> Self {
        self.then(PathStep::SmoothQuadBy([x, y]))
    }

    /// An arc whose flags are written as one pair.
    pub(crate) fn a_paired(self, radii: [Number; 2], large: bool, sweep: bool, end: [Number; 2]) -> Self {
        self.then(PathStep::PairedArc { radii, large, sweep, end })
    }

    /// An arc that ends `end` away from where it starts.
    pub(crate) fn a_by(self, radii: [Number; 2], large: bool, sweep: bool, end: [Number; 2]) -> Self {
        self.then(PathStep::ArcBy { radii, large, sweep, end })
    }
}

pub(crate) mod paths {
    use super::{Number, PathProgram, PathStep};
    use crate::js;
    use std::f64::consts::PI;

    fn point(angle: f64, radius: f64) -> [Number; 2] {
        [Number::num(js::sin(angle) * radius), Number::num(-js::cos(angle) * radius)]
    }

    fn arc(radius: f64, large: bool, sweep: bool, end: [Number; 2]) -> PathStep {
        PathStep::Arc { radii: [Number::num(radius); 2], large, sweep, end }
    }

    /// Pele's `slice`: a pie slice from `start` to `end`, in radians clockwise from the top, with
    /// a hole of radius `inner`. A full turn is two half rings, and its hole winds the other way.
    pub(crate) fn pie_slice(start: f64, end: f64, outer: f64, inner: f64) -> PathProgram {
        let mut steps = Vec::new();
        if end - start >= PI * 2.0 - 1e-9 {
            let mut ring = |r: f64, sweep: bool| {
                let top = [Number::num(0.0), Number::num(-r)];
                steps.extend([
                    PathStep::Move(top),
                    arc(r, true, sweep, [Number::num(0.0), Number::num(r)]),
                    arc(r, true, sweep, top),
                    PathStep::Close,
                ]);
            };
            ring(outer, true);
            if inner > 0.0 {
                ring(inner, false);
            }
            return PathProgram { steps };
        }
        let large = end - start > PI;
        if inner <= 0.0 {
            steps.extend([
                PathStep::Move([Number::num(0.0); 2]),
                PathStep::Line(point(start, outer)),
                arc(outer, large, true, point(end, outer)),
                PathStep::Close,
            ]);
        } else {
            steps.extend([
                PathStep::Move(point(start, outer)),
                arc(outer, large, true, point(end, outer)),
                PathStep::Line(point(end, inner)),
                arc(inner, large, false, point(start, inner)),
                PathStep::Close,
            ]);
        }
        PathProgram { steps }
    }
}

/// A label with its place. The label is the one layout made; nothing here lays text out again.
#[derive(Clone, Debug)]
pub(crate) struct PlacedText {
    pub(crate) label: Label,
    pub(crate) emission: TextEmission,
}

/// The forms Pele writes text in.
#[derive(Clone, Debug)]
pub(crate) enum TextEmission {
    /// One plain run, written as the text element's own content at one baseline point.
    Plain { x: Number, y: Number },
    /// Lines of runs. Each line's first run carries the line's point, and `shared` is the
    /// emphasis every run has, which the text element carries once.
    Rich { lines: Vec<[Number; 2]>, shared: Emphasis },
    /// One line of runs at the text element's own point, as `leftLabelSvg` writes each line: a
    /// run with no emphasis beyond `shared` is bare text, and any other is a `tspan`.
    Line { x: Number, y: Number, shared: Emphasis },
    /// One line of runs at the text element's own point, as railroad writes a special sequence:
    /// a run with a fill of its own is a `tspan` in that fill, and any other run is bare text.
    /// `fills` holds one entry per run, and `shared` is the emphasis the text element carries.
    Painted { x: Number, y: Number, fills: Vec<Option<Paint>>, shared: Emphasis },
    /// Plain runs at the text element's own point, the last of them in a `tspan` with a class
    /// and a fill of its own, as sankey writes a node's name and then its value.
    Tailed { x: Number, y: Number, class: &'static str, fill: Paint },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PartId(u32);

/// How a picture fills its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Aspect {
    /// Stretched to the box.
    Stretch,
    /// Whole and centred, at its own proportions.
    Meet,
}

/// Where an `<a>` leads. Every text is written as it was approved, so what was checked is
/// what is written.
#[derive(Clone, Debug, Default)]
pub(crate) struct Link {
    /// The address, where the host's policy lets the link be followed.
    pub(crate) href: Option<JsText>,
    pub(crate) target: Option<&'static str>,
    pub(crate) rel: Option<String>,
    /// The note a link inside the host names, which a host follows itself.
    pub(crate) note: Option<JsText>,
}

#[derive(Clone, Debug)]
pub(crate) enum NodeContent {
    Group(Vec<NodeId>),
    /// A group that is a link.
    Link {
        link: Link,
        children: Vec<NodeId>,
    },
    Path(PathProgram),
    Rect {
        x: Number,
        y: Number,
        width: Number,
        height: Number,
        rx: Option<Number>,
    },
    /// A circle, centred on its group's origin along each axis it gives no coordinate for.
    Circle {
        cx: Option<Number>,
        cy: Option<Number>,
        radius: Number,
    },
    /// An ellipse centred on its group's origin.
    Ellipse {
        rx: Number,
        ry: Number,
    },
    Polygon(Vec<[Number; 2]>),
    Text(PlacedText),
    Image {
        href: JsText,
        x: Number,
        y: Number,
        width: Number,
        height: Number,
        aspect: Aspect,
    },
    /// The box of an icon the host draws. In parity mode no host does, so the box is empty, or
    /// holds the glyph Pele draws for the name itself.
    Icon {
        name: JsText,
        x: Number,
        y: Number,
        size: Number,
        glyph: Option<IconGlyph>,
    },
    /// The tooltip of the node's entity, at the place Pele writes it.
    Tooltip,
    /// The definitions of the gradients the drawing paints with, which draw nothing themselves.
    Gradients(Vec<GradientId>),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Transform {
    Translate(Number, Number),
    /// A turn by `degrees` about `centre`.
    Rotate {
        degrees: Number,
        centre: [Number; 2],
    },
    /// A move by `translate`, then a turn by `degrees` about the point moved to, written
    /// `translate(x,y) rotate(degrees)`.
    TranslateRotate {
        translate: [Number; 2],
        degrees: Number,
    },
}

/// What a node sets. Each value is stored once; `Node::emission` gives the order they are written in.
#[derive(Clone, Debug, Default)]
pub(crate) struct Properties {
    /// Pele's own class names, then the author's.
    pub(crate) class: Option<String>,
    pub(crate) transform: Option<Transform>,
    pub(crate) fill: Option<Paint>,
    pub(crate) fill_opacity: Option<Number>,
    pub(crate) stroke: Option<Paint>,
    pub(crate) stroke_width: Option<Number>,
    pub(crate) dash: Vec<Number>,
    pub(crate) cap: Option<LineCap>,
    pub(crate) join: Option<LineJoin>,
    /// Whether the pointer passes through to what is underneath.
    pub(crate) untouchable: bool,
    /// Whether the node draws its stroke solid, though the group around it dashes it.
    pub(crate) undashed: bool,
    /// How opaque a group is as a whole, written as an attribute.
    pub(crate) opacity: Option<Number>,
    pub(crate) font_size: Option<Number>,
    pub(crate) weight: Option<TextWeight>,
    pub(crate) anchor: Option<AnchorKind>,
    pub(crate) style: Option<Style>,
    /// Whether the text inside is set in the monospace font.
    pub(crate) mono: bool,
    /// A gradient the fill runs along. The fill is then the colour drawn where the gradient
    /// cannot be, and the SVG writes both in the one `fill`.
    pub(crate) gradient: Option<GradientId>,
    /// Whether the shape holds its entity's tooltip as a child, as sankey's links and a journey's
    /// actor dots do: `<circle …><title>…</title></circle>`.
    pub(crate) titled: bool,
}

/// A reference to one value of a node, in the order Pele writes its attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Class,
    /// The author's name for the node's entity.
    DataId,
    Transform,
    Geometry(GeometryField),
    Fill,
    FillOpacity,
    /// The opacity of a group as a whole.
    Opacity,
    Stroke,
    StrokeWidth,
    DashArray,
    StrokeCap,
    StrokeJoin,
    PointerEvents,
    FontSize,
    FontWeight,
    /// Italic, where every run of a label is.
    FontStyle,
    /// Monospace, where every run of a label is.
    FontFamily,
    Anchor,
    Style,
    Href,
    Target,
    Rel,
    /// The note an internal link names.
    DataHref,
    /// `xml:space="preserve"`, where a template writes it among the other attributes.
    PreserveSpace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GeometryField {
    X,
    Y,
    Width,
    Height,
    Rx,
    Ry,
    Cx,
    Cy,
    Radius,
    D,
    Points,
    /// How a picture fills its box.
    Aspect,
    /// The name of an icon.
    Icon,
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub(crate) part: Option<PartId>,
    pub(crate) facet: Option<Facet>,
    pub(crate) content: NodeContent,
    pub(crate) properties: Properties,
    pub(crate) emission: Vec<Field>,
    /// What the node's style declarations mean to an interpreter that has no CSS.
    pub(crate) styled: style::Meaning,
}

/// What something in a diagram is, apart from where and how it is drawn. A source edit that
/// keeps an entity keeps its key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntityKey {
    kind: EntityKind,
    name: EntityName,
    duplicate: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityKind {
    /// The diagram as a whole, which owns its title and what belongs to no other entity.
    Diagram,
    /// A pie section or another series a legend names, such as a journey's actor, named by its label.
    Section,
    Node,
    /// A group of nodes, such as a flowchart's subgraph.
    Group,
    Edge,
    /// A note that stands beside the nodes, such as a class diagram's.
    Note,
}

/// What tells an entity from the others of its kind.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EntityName {
    /// The name the author gave it, as UTF-16 code units. A surrogate may stand alone.
    Named(Vec<u16>),
    /// An edge the author gave no name: the entities it joins and what kind of edge it is.
    Between { from: Vec<u16>, to: Vec<u16>, kind: String },
}

impl EntityKey {
    pub fn kind(&self) -> EntityKind {
        self.kind
    }

    pub fn name(&self) -> &EntityName {
        &self.name
    }

    /// The name the author gave the entity, as UTF-16 code units, or nothing for an edge with
    /// no name of its own.
    pub fn identity(&self) -> &[u16] {
        match &self.name {
            EntityName::Named(name) => name,
            EntityName::Between { .. } => &[],
        }
    }

    /// The entity's place among those with the same kind and name, counted from 0. Edges
    /// repeat: two edges may join the same nodes in the same way, or share an id. So may nodes of
    /// one diagram that a source names alike, such as a class and a lollipop interface.
    pub fn duplicate(&self) -> u32 {
        self.duplicate
    }
}

/// An entity as a diagram names it while drawing.
pub(crate) struct Entity {
    key: EntityKey,
    author_text: JsText,
    tooltip: Option<JsText>,
    decoded: bool,
    tooltip_decoded: bool,
}

impl Entity {
    fn named(kind: EntityKind, name: &JsText) -> Self {
        let key = EntityKey { kind, name: EntityName::Named(name.to_vec()), duplicate: 0 };
        Entity { key, author_text: name.clone(), tooltip: None, decoded: false, tooltip_decoded: false }
    }

    /// A series that a legend names, such as a journey's actor or an xychart's bars or line. It is
    /// the kind of a pie's section, whose label is its identity.
    pub(crate) fn series(label: &JsText) -> Self {
        Entity::named(EntityKind::Section, label)
    }

    /// A tooltip the diagram has already turned into text, as treemap's path of section names
    /// is, so that its entity codes are not read again.
    pub(crate) fn decoded_tooltip(mut self, tooltip: JsText) -> Self {
        self.tooltip = Some(tooltip);
        self.tooltip_decoded = true;
        self
    }

    pub(crate) fn diagram() -> Self {
        Entity::named(EntityKind::Diagram, &JsText::default())
    }

    /// A pie section. Its label is its identity, since a pie keeps one section per label.
    pub(crate) fn pie_section(label: &JsText) -> Self {
        Entity::named(EntityKind::Section, label)
    }

    pub(crate) fn node(id: &JsText) -> Self {
        Entity::named(EntityKind::Node, id)
    }

    /// A node keyed by `name` that Pele writes as `written`, such as a gantt task with no id of
    /// its own, which Pele numbers in source order.
    pub(crate) fn node_written_as(name: &JsText, written: &JsText) -> Self {
        Entity { author_text: written.clone(), ..Entity::named(EntityKind::Node, name) }
    }

    pub(crate) fn note(id: &JsText) -> Self {
        Entity::named(EntityKind::Note, id)
    }

    /// The entity's place among those with the same kind and name, where a diagram can have
    /// several, such as a class and a lollipop interface that share a name.
    pub(crate) fn ranked(mut self, duplicate: u32) -> Self {
        self.key.duplicate = duplicate;
        self
    }

    pub(crate) fn group(id: &JsText) -> Self {
        Entity::named(EntityKind::Group, id)
    }

    /// An edge. `written_id` is the id Pele writes, which counts edges and so changes when one
    /// is added before it. The key does not hold it: `name` is the id the author gave or the
    /// two ends, and `duplicate` the edge's place among those with the same name.
    pub(crate) fn edge(written_id: &JsText, name: EntityName, duplicate: u32) -> Self {
        let key = EntityKey { kind: EntityKind::Edge, name, duplicate };
        Entity { key, author_text: written_id.clone(), tooltip: None, decoded: false, tooltip_decoded: false }
    }

    /// A note the author gives no name, such as a sequence diagram's. `written_id` is the id
    /// Pele writes, which counts what comes before the note. The key does not hold it: `name`
    /// is what the note is attached to, and `duplicate` its place among those with the same name.
    pub(crate) fn note_between(written_id: &JsText, name: EntityName, duplicate: u32) -> Self {
        let key = EntityKey { kind: EntityKind::Note, name, duplicate };
        Entity { key, author_text: written_id.clone(), tooltip: None, decoded: false, tooltip_decoded: false }
    }

    /// A region of the diagram's area that several sets share, such as an overlap of a venn
    /// diagram's circles. `written_id` is the id Pele writes, which may be the same for two
    /// regions; `name` tells them apart.
    pub(crate) fn overlap(written_id: &JsText, name: &JsText) -> Self {
        let key = EntityKey { kind: EntityKind::Section, name: EntityName::Named(name.to_vec()), duplicate: 0 };
        Entity { key, author_text: written_id.clone(), tooltip: None, decoded: false, tooltip_decoded: false }
    }

    pub(crate) fn tooltip(mut self, tooltip: Option<&JsText>) -> Self {
        self.tooltip = tooltip.cloned();
        self
    }

    /// Marks the written name and the tooltip as text that already reads as the author meant
    /// it, its entity codes decoded, so that the SVG only escapes them. Sankey decodes the parts
    /// of a link's name and tooltip before it joins them.
    pub(crate) fn decoded(mut self) -> Self {
        self.decoded = true;
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EntityRecord {
    pub(crate) key: EntityKey,
    /// The name the author gave, which `data-id` carries.
    pub(crate) author_text: JsText,
    pub(crate) tooltip: Option<JsText>,
    /// Whether `author_text` and `tooltip` have their entity codes decoded already.
    pub(crate) decoded: bool,
    /// Whether `tooltip` alone is text already, rather than written with entity codes.
    pub(crate) tooltip_decoded: bool,
}

impl EntityRecord {
    /// The tooltip as it reads, its entity codes decoded: Pele's `escText` reads the codes of a
    /// tooltip as written, and a decoded one is text already.
    pub(crate) fn tooltip_text(&self) -> Option<Vec<u16>> {
        let tooltip = self.tooltip.as_ref()?;
        Some(if self.decoded || self.tooltip_decoded { tooltip.to_vec() } else { decode_entities(tooltip) })
    }
}

/// The accessible title and description a source gives its diagram.
#[derive(Clone, Debug, Default)]
pub(crate) struct Accessible {
    pub(crate) title: Option<JsText>,
    pub(crate) description: Option<JsText>,
}

/// A colour a source writes, kept as written, and the theme colour it is used in place of.
#[derive(Clone, Debug)]
pub(crate) struct WrittenColour {
    pub(crate) spelling: JsText,
    pub(crate) instead_of: Role,
}

/// A gradient, by its place among those the Scene holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GradientId(u32);

/// A paint that runs from one colour to another between two places along x, in the coordinates
/// of the shape it fills, as sankey's links run from the colour of their source to that of
/// their target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Gradient {
    pub(crate) x: [Number; 2],
    /// The colours at the start and at the end.
    pub(crate) stops: [Paint; 2],
}

/// What the root element is written from.
#[derive(Clone, Debug)]
pub(crate) struct Document {
    pub(crate) kind: DiagramKind,
    pub(crate) size: Size,
    pub(crate) font_size: f64,
    pub(crate) responsive: bool,
    pub(crate) id_prefix: Option<String>,
    pub(crate) accessible: Accessible,
    pub(crate) palette: SourcePalette,
    pub(crate) colours: Vec<WrittenColour>,
    pub(crate) limit_utf16: usize,
    pub(crate) gradients: Vec<Gradient>,
}

/// A finished drawing. Its nodes are in paint order and cannot change.
#[derive(Clone, Debug)]
pub struct Scene {
    pub(crate) document: Document,
    pub(crate) arena: Vec<Node>,
    pub(crate) roots: Vec<NodeId>,
    pub(crate) entities: Vec<EntityRecord>,
    /// Each edge with the two entities it joins.
    pub(crate) relations: Vec<[PartId; 3]>,
}

impl Scene {
    pub fn size(&self) -> Size {
        self.document.size
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.arena[id.0 as usize]
    }

    pub(crate) fn entity(&self, part: PartId) -> &EntityRecord {
        &self.entities[part.0 as usize]
    }

    pub(crate) fn colour(&self, id: SourceColourId) -> &WrittenColour {
        &self.document.colours[id.0 as usize]
    }

    pub(crate) fn gradient(&self, id: GradientId) -> &Gradient {
        &self.document.gradients[id.0 as usize]
    }

    /// Every entity the drawing holds, in the order the diagram first drew each.
    pub fn entities(&self) -> impl Iterator<Item = &EntityKey> {
        self.entities.iter().map(|record| &record.key)
    }

    /// The text shown when the pointer rests on an entity.
    pub fn tooltip(&self, entity: &EntityKey) -> Option<String> {
        let record = self.entities.iter().find(|record| record.key == *entity)?;
        record.tooltip_text().map(|text| walk::drawable(&text))
    }

    /// The entities an entity is joined to: an edge's two ends, and a node's edges.
    pub fn related(&self, entity: &EntityKey) -> Vec<&EntityKey> {
        let key = |part: PartId| &self.entity(part).key;
        let mut out = Vec::new();
        for &[edge, from, to] in &self.relations {
            if key(edge) == entity {
                out.extend([key(from), key(to)]);
            } else if key(from) == entity || key(to) == entity {
                out.push(key(edge));
            }
        }
        out
    }
}

/// Properties with the order they were set in. The builders below add to it at each call.
#[derive(Clone, Default)]
struct Attributes {
    part: Option<PartId>,
    facet: Option<Facet>,
    properties: Properties,
    emission: Vec<Field>,
}

impl Attributes {
    fn of(part: PartId, facet: Facet) -> Self {
        Attributes { part: Some(part), facet: Some(facet), ..Attributes::default() }
    }

    fn set(mut self, field: Field, set: impl FnOnce(&mut Properties)) -> Self {
        set(&mut self.properties);
        self.emission.push(field);
        self
    }

    fn geometry(mut self, fields: &[GeometryField]) -> Self {
        self.emission.extend(fields.iter().map(|&field| Field::Geometry(field)));
        self
    }

    /// Pele's attribute strings for a shape: its fill, its stroke and its style, each where set.
    fn look(mut self, look: Look) -> Self {
        if let Some(fill) = look.fill {
            self = self.set(Field::Fill, |p| p.fill = Some(fill));
        }
        if let Some(opacity) = look.fill_opacity {
            self = self.set(Field::FillOpacity, |p| p.fill_opacity = Some(opacity));
        }
        if let Some(stroke) = look.stroke {
            self = self.set(Field::Stroke, |p| p.stroke = Some(stroke));
        }
        if !look.dash.is_empty() {
            self = self.set(Field::DashArray, |p| p.dash = look.dash);
        }
        self.style(look.style)
    }

    fn style(self, style: Option<Style>) -> Self {
        match style {
            Some(style) => self.set(Field::Style, |p| p.style = Some(style)),
            None => self,
        }
    }
}

/// The paint a shape is written with: a fill, how opaque the fill is, a stroke, its dashes and
/// style declarations, in that order. Only [`ShapeLook`] and [`LineLook`] make one, so a look is
/// rewritten at most once.
#[derive(Clone, Debug)]
pub(crate) struct Look {
    fill: Option<Paint>,
    fill_opacity: Option<Number>,
    stroke: Option<Paint>,
    /// Empty for a solid stroke.
    dash: Vec<Number>,
    style: Option<Style>,
}

impl Look {
    /// No fill and no stroke: a shape that only holds its place.
    pub(crate) fn unseen() -> Self {
        Look { fill: Some(Paint::None), fill_opacity: None, stroke: Some(Paint::None), dash: Vec::new(), style: None }
    }
}

/// How a node's shape is painted before a shape picks its form of it: the theme's fill and
/// border, then the shape declarations of the node's style. Pele's `SHAPE_ATTRS + style.shape`.
#[derive(Clone, Debug)]
pub(crate) struct ShapeLook {
    fill: Paint,
    fill_opacity: Option<Number>,
    stroke: Paint,
    dash: Vec<Number>,
    style: Option<Style>,
}

impl ShapeLook {
    pub(crate) fn new(fill: Role, stroke: Role, style: Option<&Style>) -> Self {
        let (fill, stroke) = (Paint::Role(fill), Paint::Role(stroke));
        ShapeLook { fill, fill_opacity: None, stroke, dash: Vec::new(), style: style.cloned() }
    }

    /// One colour as an outline with a wash of the same colour inside, `wash` opaque. A fill
    /// the node's style sets replaces the wash and stays opaque.
    pub(crate) fn tinted(colour: Paint, wash: f64, style: Option<&Style>) -> Self {
        let fill_opacity = if style.is_some_and(Style::sets_fill) { None } else { Some(Number::raw(wash)) };
        ShapeLook { fill: colour, fill_opacity, stroke: colour, dash: Vec::new(), style: style.cloned() }
    }

    /// With its outline dashed, written after the stroke.
    pub(crate) fn dashed(mut self, pattern: &[Number]) -> Self {
        self.dash = pattern.to_vec();
        self
    }

    /// As it is.
    pub(crate) fn plain(&self) -> Look {
        Look {
            fill: Some(self.fill),
            fill_opacity: self.fill_opacity,
            stroke: Some(self.stroke),
            dash: self.dash.clone(),
            style: self.style.clone(),
        }
    }

    /// Pele's `rounded`: with the theme's corner radius, declared first so that the node's own
    /// style can override it.
    pub(crate) fn rounded(&self) -> Look {
        Look { style: Some(Style::rounded(self.style.as_ref())), ..self.plain() }
    }

    /// Without the border, for a shape that draws only some of its sides.
    pub(crate) fn borderless(&self) -> Look {
        Look { stroke: Some(Paint::None), ..self.plain() }
    }
}

/// How the lines inside a node's shape are painted: the theme's border, then the declarations
/// of the node's style that apply to a line. Pele's `LINE_ATTRS + style.line`.
#[derive(Clone, Debug)]
pub(crate) struct LineLook {
    stroke: Paint,
    dash: Vec<Number>,
    style: Option<Style>,
}

impl LineLook {
    pub(crate) fn new(stroke: Role, style: Option<&Style>) -> Self {
        LineLook { stroke: Paint::Role(stroke), dash: Vec::new(), style: style.cloned() }
    }

    /// A line in a colour that is no theme role, such as a series colour.
    pub(crate) fn painted(stroke: Paint, style: Option<&Style>) -> Self {
        LineLook { stroke, dash: Vec::new(), style: style.cloned() }
    }

    /// With the line dashed, written after the stroke.
    pub(crate) fn dashed(mut self, pattern: &[Number]) -> Self {
        self.dash = pattern.to_vec();
        self
    }

    /// A line with nothing inside it.
    pub(crate) fn open(&self) -> Look {
        self.filled(Paint::None)
    }

    /// A line around a fill of its own.
    pub(crate) fn filled(&self, fill: Paint) -> Look {
        let (stroke, dash, style) = (Some(self.stroke), self.dash.clone(), self.style.clone());
        Look { fill: Some(fill), fill_opacity: None, stroke, dash, style }
    }
}

/// A group of nodes that share paint order and inherited attributes.
pub(crate) struct LayerSpec {
    attributes: Attributes,
    keep_empty: bool,
}

impl LayerSpec {
    fn with(mut self, field: Field, set: impl FnOnce(&mut Properties)) -> Self {
        self.attributes = self.attributes.set(field, set);
        self
    }

    /// A group with no class of its own.
    pub(crate) fn group() -> Self {
        LayerSpec { attributes: Attributes::default(), keep_empty: false }
    }

    pub(crate) fn class(class: &str) -> Self {
        LayerSpec::group().with(Field::Class, |p| p.class = Some(class.to_owned()))
    }

    /// Pele's class names, then the author's.
    pub(crate) fn classes(own: &str, author: &AuthorClasses) -> Self {
        LayerSpec::group().with(Field::Class, |p| p.class = Some(author.after(own)))
    }

    /// Makes the group a part of an entity.
    pub(crate) fn part(mut self, part: PartId, facet: Facet) -> Self {
        self.attributes.part = Some(part);
        self.attributes.facet = Some(facet);
        self
    }

    pub(crate) fn data_id(self) -> Self {
        debug_assert!(self.attributes.part.is_some(), "only a part of an entity has the entity's name");
        self.with(Field::DataId, |_| {})
    }

    pub(crate) fn translate(self, point: Point) -> Self {
        let transform = Transform::Translate(Number::num(point.x), Number::num(point.y));
        self.with(Field::Transform, |p| p.transform = Some(transform))
    }

    pub(crate) fn stroke(self, paint: Paint) -> Self {
        self.with(Field::Stroke, |p| p.stroke = Some(paint))
    }

    /// Moves the group's drawing to `point` and turns it there by `degrees`, as an xychart turns
    /// its left axis title.
    pub(crate) fn translate_rotate(self, point: Point, degrees: f64) -> Self {
        let translate = [Number::num(point.x), Number::num(point.y)];
        let transform = Transform::TranslateRotate { translate, degrees: Number::raw(degrees) };
        self.with(Field::Transform, |p| p.transform = Some(transform))
    }

    /// Turns the group's drawing by `degrees` about `centre`.
    pub(crate) fn rotate(self, degrees: f64, centre: Point) -> Self {
        let transform =
            Transform::Rotate { degrees: Number::raw(degrees), centre: [Number::num(centre.x), Number::num(centre.y)] };
        self.with(Field::Transform, |p| p.transform = Some(transform))
    }

    pub(crate) fn stroke_width(self, width: Number) -> Self {
        self.with(Field::StrokeWidth, |p| p.stroke_width = Some(width))
    }

    pub(crate) fn cap(self, cap: LineCap) -> Self {
        self.with(Field::StrokeCap, |p| p.cap = Some(cap))
    }

    pub(crate) fn join(self, join: LineJoin) -> Self {
        self.with(Field::StrokeJoin, |p| p.join = Some(join))
    }

    /// Makes the group as a whole this opaque, composited once.
    pub(crate) fn opacity(self, opacity: Number) -> Self {
        self.with(Field::Opacity, |p| p.opacity = Some(opacity))
    }

    /// Lets the pointer through to what is under the group.
    pub(crate) fn untouchable(self) -> Self {
        self.with(Field::PointerEvents, |p| p.untouchable = true)
    }

    pub(crate) fn font_size(self, size: Number) -> Self {
        self.with(Field::FontSize, |p| p.font_size = Some(size))
    }

    pub(crate) fn anchor(self, anchor: AnchorKind) -> Self {
        self.with(Field::Anchor, |p| p.anchor = Some(anchor))
    }

    pub(crate) fn fill(self, paint: Paint) -> Self {
        self.with(Field::Fill, |p| p.fill = Some(paint))
    }

    /// How opaque the fills inside are, unless they say otherwise.
    pub(crate) fn fill_opacity(self, opacity: Number) -> Self {
        self.with(Field::FillOpacity, |p| p.fill_opacity = Some(opacity))
    }

    /// Sets the text inside in the monospace font.
    pub(crate) fn mono(self) -> Self {
        self.with(Field::FontFamily, |p| p.mono = true)
    }

    /// Dashes and the gaps between them, in turn, for what the group draws.
    pub(crate) fn dash(self, pattern: &[Number]) -> Self {
        self.with(Field::DashArray, |p| p.dash = pattern.to_vec())
    }

    pub(crate) fn style(mut self, style: Option<Style>) -> Self {
        self.attributes = self.attributes.style(style);
        self
    }

    /// Keeps the group in the drawing when nothing is drawn in it.
    pub(crate) fn keep_empty(mut self) -> Self {
        self.keep_empty = true;
        self
    }
}

/// How a label's text element looks, beyond the label's own emphasis.
#[derive(Default)]
pub(crate) struct LabelLook(Attributes);

impl LabelLook {
    pub(crate) fn class(class: &str) -> Self {
        LabelLook(Attributes::default().set(Field::Class, |p| p.class = Some(class.to_owned())))
    }

    pub(crate) fn weight(self, weight: TextWeight) -> Self {
        LabelLook(self.0.set(Field::FontWeight, |p| p.weight = Some(weight)))
    }

    /// Writes the name of the entity the label belongs to, as `data-id`.
    pub(crate) fn data_id(mut self) -> Self {
        self.0.emission.push(Field::DataId);
        self
    }

    pub(crate) fn font_size(self, size: Number) -> Self {
        LabelLook(self.0.set(Field::FontSize, |p| p.font_size = Some(size)))
    }

    /// Writes `font-style="italic"`. The label's runs say what an interpreter sets in italic.
    pub(crate) fn italic(mut self) -> Self {
        self.0.emission.push(Field::FontStyle);
        self
    }

    /// Sets the text in the monospace font.
    pub(crate) fn mono(self) -> Self {
        LabelLook(self.0.set(Field::FontFamily, |p| p.mono = true))
    }

    pub(crate) fn fill(self, paint: Option<Paint>) -> Self {
        match paint {
            Some(paint) => LabelLook(self.0.set(Field::Fill, |p| p.fill = Some(paint))),
            None => self,
        }
    }

    pub(crate) fn stroke(self, paint: Paint, width: Number, join: LineJoin) -> Self {
        let attributes = self
            .0
            .set(Field::Stroke, |p| p.stroke = Some(paint))
            .set(Field::StrokeWidth, |p| p.stroke_width = Some(width))
            .set(Field::StrokeJoin, |p| p.join = Some(join));
        LabelLook(attributes)
    }

    pub(crate) fn style(self, style: Option<&Style>) -> Self {
        LabelLook(self.0.style(style.cloned()))
    }

    /// A stroke with no width or join of its own, written where this is called, as a label
    /// inside a stroked group is written with `stroke="none"`.
    pub(crate) fn stroke_paint(self, paint: Paint) -> Self {
        LabelLook(self.0.set(Field::Stroke, |p| p.stroke = Some(paint)))
    }

    /// Turns the text element by `degrees` about `centre`, written where this is called, as a
    /// wardley map's visibility axis title is turned upright.
    pub(crate) fn rotate(self, degrees: Number, centre: [Number; 2]) -> Self {
        let transform = Transform::Rotate { degrees, centre };
        LabelLook(self.0.set(Field::Transform, |p| p.transform = Some(transform)))
    }
}

/// The emphasis of a run that its text element does not already carry: Pele's
/// `span.style & ~shared`.
pub(crate) fn own_emphasis(style: Emphasis, shared: Emphasis) -> Emphasis {
    Emphasis {
        bold: style.bold && !shared.bold,
        italic: style.italic && !shared.italic,
        mono: style.mono && !shared.mono,
        ..style
    }
}

/// A diagram title: its laid out label and the point it is centred on.
pub(crate) struct TitleLayout {
    pub(crate) label: Label,
    pub(crate) at: Point,
}

/// Builds one Scene. A draw attempt owns its builder, so a rejected attempt leaves nothing behind.
pub(crate) struct SceneBuilder {
    document: Document,
    arena: Vec<Node>,
    /// The child lists being filled: the roots, then one per open layer.
    open: Vec<Vec<NodeId>>,
    entities: IndexMap<EntityKey, EntityRecord>,
    relations: Vec<[PartId; 3]>,
}

impl SceneBuilder {
    pub(crate) fn new(cx: &DrawContext<'_>, size: Size, accessible: Accessible) -> Self {
        let document = Document {
            kind: cx.kind,
            size,
            font_size: cx.options.typography.normal.size,
            responsive: cx.responsive,
            id_prefix: cx.options.id_prefix.clone(),
            accessible,
            palette: cx.palette.clone(),
            colours: Vec::new(),
            limit_utf16: cx.limits.svg_utf16,
            gradients: Vec::new(),
        };
        SceneBuilder {
            document,
            arena: Vec::new(),
            open: vec![Vec::new()],
            entities: IndexMap::new(),
            relations: Vec::new(),
        }
    }

    fn push(&mut self, attributes: Attributes, content: NodeContent) -> NodeId {
        let Attributes { part, facet, properties, emission } = attributes;
        let styled = match &properties.style {
            Some(style) => style.meaning(&properties, &mut |spelling, instead_of| self.colour(spelling, instead_of)),
            None => style::Meaning::default(),
        };
        let id = NodeId(self.arena.len() as u32);
        self.arena.push(Node { part, facet, content, properties, emission, styled });
        self.open.last_mut().expect("the roots are always open").push(id);
        id
    }

    /// The part for an entity. Asking again for the same entity gives the same part.
    pub(crate) fn part(&mut self, entity: Entity) -> PartId {
        let entry = self.entities.entry(entity.key.clone());
        let index = entry.index();
        entry.or_insert(EntityRecord {
            key: entity.key,
            author_text: entity.author_text,
            tooltip: entity.tooltip,
            decoded: entity.decoded,
            tooltip_decoded: entity.tooltip_decoded,
        });
        PartId(index as u32)
    }

    /// Records that `edge` joins `from` and `to`.
    pub(crate) fn relation(&mut self, edge: PartId, from: PartId, to: PartId) {
        self.relations.push([edge, from, to]);
    }

    /// A colour the source writes, used in place of a theme colour.
    pub(crate) fn colour(&mut self, spelling: &[u16], instead_of: Role) -> Paint {
        let id = SourceColourId(self.document.colours.len() as u32);
        self.document.colours.push(WrittenColour { spelling: spelling.into(), instead_of });
        Paint::Source(id)
    }

    fn scope(&mut self, draw: impl FnOnce(&mut Self)) -> Vec<NodeId> {
        self.open.push(Vec::new());
        draw(self);
        self.open.pop().expect("the scope opened above")
    }

    /// Appends a group and draws its children in order. A layer that stays empty is kept only
    /// where its spec says so.
    pub(crate) fn layer(&mut self, spec: LayerSpec, draw: impl FnOnce(&mut Self)) {
        let children = self.scope(draw);
        if !children.is_empty() || spec.keep_empty {
            self.push(spec.attributes, NodeContent::Group(children));
        }
    }

    /// Pele's `linked`: draws inside an `<a>` to an address the host's policy has passed.
    pub(crate) fn link_to(
        &mut self,
        part: PartId,
        href: JsText,
        target: Option<&'static str>,
        rel: Option<String>,
        draw: impl FnOnce(&mut Self),
    ) {
        let children = self.scope(draw);
        let mut attributes = Attributes::of(part, Facet::Shape);
        attributes.emission.push(Field::Href);
        if target.is_some() {
            attributes.emission.push(Field::Target);
        }
        if rel.is_some() {
            attributes.emission.push(Field::Rel);
        }
        let link = Link { href: Some(href), target, rel, note: None };
        self.push(attributes, NodeContent::Link { link, children });
    }

    /// A link to a note inside the host, around what names the note. The host follows `note`
    /// itself; `href` is the same name where the host's policy lets a browser follow it too.
    pub(crate) fn link_inside(
        &mut self,
        part: PartId,
        note: JsText,
        followable: bool,
        rel: Option<String>,
        draw: impl FnOnce(&mut Self),
    ) {
        let children = self.scope(draw);
        let mut attributes =
            Attributes::of(part, Facet::Label).set(Field::Class, |p| p.class = Some("internal-link".to_owned()));
        let rel = rel.filter(|_| followable);
        if followable {
            attributes.emission.push(Field::Href);
        }
        if rel.is_some() {
            attributes.emission.push(Field::Rel);
        }
        attributes.emission.push(Field::DataHref);
        let link = Link { href: followable.then(|| note.clone()), target: None, rel, note: Some(note) };
        self.push(attributes, NodeContent::Link { link, children });
    }

    /// Starts a shape that is a part of an entity.
    pub(crate) fn element(&mut self, part: PartId, facet: Facet) -> ElementStart<'_> {
        ElementStart { builder: self, attributes: Attributes::of(part, facet) }
    }

    /// Starts a shape that belongs to no entity but the diagram itself.
    pub(crate) fn decoration(&mut self) -> ElementStart<'_> {
        let part = self.part(Entity::diagram());
        self.element(part, Facet::Shape)
    }

    /// A gradient to fill shapes with. It is defined where [`SceneBuilder::gradients`] is called.
    pub(crate) fn gradient(&mut self, gradient: Gradient) -> GradientId {
        self.document.gradients.push(gradient);
        GradientId(self.document.gradients.len() as u32 - 1)
    }

    /// The definitions of `gradients`, written here.
    pub(crate) fn gradients(&mut self, gradients: Vec<GradientId>) {
        self.push(Attributes::default(), NodeContent::Gradients(gradients));
    }

    /// The entity's tooltip, written here.
    pub(crate) fn tooltip(&mut self, part: PartId) {
        self.push(Attributes::of(part, Facet::Shape), NodeContent::Tooltip);
    }

    /// The box of an icon, `size` on each side, with its top left corner at `origin`.
    pub(crate) fn icon(&mut self, part: PartId, name: &JsText, origin: [Number; 2], size: Number) {
        self.icon_box(part, name, origin, size, None);
    }

    /// The box of an icon with the drawing it shows when no host draws the icon.
    pub(crate) fn icon_with_glyph(
        &mut self,
        part: PartId,
        name: &JsText,
        origin: [Number; 2],
        size: Number,
        glyph: IconGlyph,
    ) {
        self.icon_box(part, name, origin, size, Some(glyph));
    }

    fn icon_box(&mut self, part: PartId, name: &JsText, origin: [Number; 2], size: Number, glyph: Option<IconGlyph>) {
        use GeometryField::{Height, Icon, Width, X, Y};
        let attributes = Attributes::of(part, Facet::Shape)
            .set(Field::Class, |p| p.class = Some("pele-icon".to_owned()))
            .geometry(&[Icon, X, Y, Width, Height]);
        self.push(attributes, NodeContent::Icon { name: name.clone(), x: origin[0], y: origin[1], size, glyph });
    }

    /// [`SceneBuilder::icon`] with the glyph Pele draws in the box where no host draws the icon.
    pub(crate) fn icon_with(
        &mut self,
        part: PartId,
        name: &JsText,
        origin: [Number; 2],
        size: Number,
        glyph: Option<IconGlyph>,
    ) {
        self.icon_box(part, name, origin, size, glyph);
    }

    /// The root's font size, for a type whose config can set its own in place of the host's.
    pub(crate) fn root_font_size(mut self, size: f64) -> Self {
        self.document.font_size = size;
        self
    }

    /// A picture from an address the host's policy has passed.
    pub(crate) fn image(&mut self, part: PartId, href: JsText, origin: [Number; 2], size: [Number; 2], aspect: Aspect) {
        use GeometryField::{Aspect as Fit, Height, Width, X, Y};
        let mut attributes = Attributes::of(part, Facet::Shape);
        attributes.emission.push(Field::Href);
        let attributes = attributes.geometry(&[X, Y, Width, Height, Fit]);
        let content = NodeContent::Image { href, x: origin[0], y: origin[1], width: size[0], height: size[1], aspect };
        self.push(attributes, content);
    }

    pub(crate) fn title(&mut self, title: &TitleLayout) {
        let look = LabelLook::class("pele-title").weight(TextWeight::Title);
        let part = self.part(Entity::diagram());
        self.label(part, Facet::Title, &title.label, title.at, look);
    }

    /// Pele's `labelSvg`: a measured label as text centred on `at`. An empty label draws nothing.
    pub(crate) fn label(&mut self, part: PartId, facet: Facet, label: &Label, at: Point, look: LabelLook) {
        if label.lines.is_empty() {
            return;
        }
        let shift = label.size * 0.35;
        let mut top = at.y - label.height / 2.0;
        let mut attributes = Attributes { part: Some(part), facet: Some(facet), ..look.0 };
        let middle = |p: &mut Properties| p.anchor = Some(AnchorKind::Middle);

        let only = match label.lines.as_slice() {
            [line] if line.runs.len() == 1 => Some(&line.runs[0]),
            _ => None,
        };
        if only.is_some_and(|run| run.style.is_plain()) {
            let (x, y) = (Number::num(at.x), Number::num(top + label.line_height / 2.0 + shift));
            attributes = attributes.geometry(&[GeometryField::X, GeometryField::Y]);
            let text = PlacedText { label: label.clone(), emission: TextEmission::Plain { x, y } };
            self.push(attributes.set(Field::Anchor, middle), NodeContent::Text(text));
            return;
        }

        // The emphasis every run has is written once, on the text element, unless the look
        // already gives a weight of its own.
        let runs = || label.lines.iter().flat_map(|line| &line.runs).filter(|run| !run.text.is_empty());
        let shared = Emphasis {
            bold: runs().all(|run| run.style.bold),
            italic: runs().all(|run| run.style.italic),
            mono: runs().all(|run| run.style.mono),
            ..Emphasis::default()
        };
        let shared = if runs().next().is_some() { shared } else { Emphasis::default() };
        if shared.bold && attributes.properties.weight.is_none() {
            attributes = attributes.set(Field::FontWeight, |p| p.weight = Some(TextWeight::Bold));
        }
        if shared.italic && !attributes.emission.contains(&Field::FontStyle) {
            attributes.emission.push(Field::FontStyle);
        }
        if shared.mono {
            attributes.emission.push(Field::FontFamily);
        }
        let mut lines = Vec::with_capacity(label.lines.len());
        for _ in &label.lines {
            let mid = top + label.line_height / 2.0;
            lines.push([Number::num(at.x), Number::num(mid + shift)]);
            top += label.line_height;
        }
        let text = PlacedText { label: label.clone(), emission: TextEmission::Rich { lines, shared } };
        self.push(attributes.set(Field::Anchor, middle), NodeContent::Text(text));
    }

    /// Text whose lines start at `x`, its first line's box at `top`, as requirement fields are
    /// written: each line's first run carries the line's point, runs with no text are left out,
    /// and so is a line left with none. Nothing is drawn when no run is left.
    pub(crate) fn start_text(&mut self, part: PartId, facet: Facet, label: &Label, x: f64, top: f64, look: LabelLook) {
        let shift = label.size * 0.35;
        let mut kept = Label { lines: Vec::new(), ..label.clone() };
        let mut lines = Vec::new();
        for (index, line) in label.lines.iter().enumerate() {
            let runs: Vec<_> = line.runs.iter().filter(|run| !run.text.is_empty()).cloned().collect();
            if runs.is_empty() {
                continue;
            }
            let middle = top + (index as f64 + 0.5) * label.line_height;
            lines.push([Number::num(x), Number::num(middle + shift)]);
            kept.lines.push(crate::measure::LabelLine { runs, width: line.width });
        }
        if kept.lines.is_empty() {
            return;
        }
        let runs = || kept.lines.iter().flat_map(|line| &line.runs);
        let shared = Emphasis {
            bold: runs().all(|run| run.style.bold),
            italic: runs().all(|run| run.style.italic),
            mono: runs().all(|run| run.style.mono),
            ..Emphasis::default()
        };
        let mut attributes = Attributes { part: Some(part), facet: Some(facet), ..look.0 };
        if shared.bold && attributes.properties.weight.is_none() {
            attributes = attributes.set(Field::FontWeight, |p| p.weight = Some(TextWeight::Bold));
        }
        if shared.italic {
            attributes.emission.push(Field::FontStyle);
        }
        if shared.mono {
            attributes.emission.push(Field::FontFamily);
        }
        let start = |p: &mut Properties| p.anchor = Some(AnchorKind::Start);
        let text = PlacedText { label: kept, emission: TextEmission::Rich { lines, shared } };
        self.push(attributes.set(Field::Anchor, start), NodeContent::Text(text));
    }

    /// Pele's `leftLabelSvg` (`common/literal-label.ts`): a label with every line starting at `x`,
    /// the top of its first line at `top`. Each line is a text element of its own, anchored at
    /// its start and keeping its spaces, so that where it begins does not depend on how its text
    /// was measured. A line that writes no text is left out. The emphasis every run has is
    /// written once on each element, unless the look already writes that attribute.
    pub(crate) fn left_label(&mut self, part: PartId, facet: Facet, label: &Label, x: f64, top: f64, look: LabelLook) {
        let runs = || label.lines.iter().flat_map(|line| &line.runs).filter(|run| !run.text.is_empty());
        let shared = Emphasis {
            bold: runs().all(|run| run.style.bold),
            italic: runs().all(|run| run.style.italic),
            mono: runs().all(|run| run.style.mono),
            ..Emphasis::default()
        };
        let shared = if runs().next().is_some() { shared } else { Emphasis::default() };
        let mut attributes = Attributes { part: Some(part), facet: Some(facet), ..look.0 };
        let gives = |attributes: &Attributes, field: Field| attributes.emission.contains(&field);
        if shared.bold && !gives(&attributes, Field::FontWeight) {
            attributes = attributes.set(Field::FontWeight, |p| p.weight = Some(TextWeight::Bold));
        }
        if shared.italic && !gives(&attributes, Field::FontStyle) {
            attributes.emission.push(Field::FontStyle);
        }
        if shared.mono && !gives(&attributes, Field::FontFamily) {
            attributes = attributes.set(Field::FontFamily, |p| p.mono = true);
        }
        let attributes = attributes
            .geometry(&[GeometryField::X, GeometryField::Y])
            .set(Field::Anchor, |p| p.anchor = Some(AnchorKind::Start));
        for (index, line) in label.lines.iter().enumerate() {
            let writes = |run: &LabelRun| {
                let mut escaped = String::new();
                esc(&run.text, &mut escaped);
                !escaped.is_empty() || !own_emphasis(run.style, shared).is_plain()
            };
            if !line.runs.iter().any(writes) {
                continue;
            }
            let middle = top + (index as f64 + 0.5) * label.line_height;
            let (x, y) = (Number::num(x), Number::num(middle + label.size * 0.35));
            let one =
                Label { lines: vec![line.clone()], width: line.width, height: label.line_height, ..label.clone() };
            let text = PlacedText { label: one, emission: TextEmission::Line { x, y, shared } };
            self.push(attributes.clone(), NodeContent::Text(text));
        }
    }

    /// Pele's `textLeft` (`er/render.ts`): a measured label that starts at `x`, with the baseline
    /// of its first line at `y`. One plain run, or one run that is only `mono` when that is set,
    /// is written as plain text, unless monospace would collapse its repeated spaces. Anything
    /// else is written as runs, where a line places its first run that holds text.
    #[allow(clippy::too_many_arguments, reason = "the arguments of Pele's textLeft, plus the part it draws")]
    pub(crate) fn label_from(
        &mut self,
        part: PartId,
        facet: Facet,
        label: &Label,
        x: f64,
        y: f64,
        look: LabelLook,
        mono: bool,
    ) {
        if label.lines.is_empty() {
            return;
        }
        let mut attributes = Attributes { part: Some(part), facet: Some(facet), ..look.0 };
        let only = match label.lines.as_slice() {
            [line] if line.runs.len() == 1 => Some(&line.runs[0]),
            _ => None,
        };
        let collapses = |text: &[u16]| text.windows(2).any(|pair| pair == [32, 32]);
        if let Some(run) = only
            && run.style == (Emphasis { mono, ..Emphasis::default() })
            && !(mono && collapses(&run.text))
        {
            if mono {
                attributes = attributes.set(Field::FontFamily, |p| p.mono = true);
            }
            let attributes = attributes.geometry(&[GeometryField::X, GeometryField::Y]);
            let text = PlacedText {
                label: label.clone(),
                emission: TextEmission::Plain { x: Number::num(x), y: Number::num(y) },
            };
            self.push(attributes, NodeContent::Text(text));
            return;
        }
        let runs = || label.lines.iter().flat_map(|line| &line.runs).filter(|run| !run.text.is_empty());
        let shared = Emphasis {
            bold: runs().all(|run| run.style.bold),
            italic: runs().all(|run| run.style.italic),
            mono: runs().all(|run| run.style.mono),
            ..Emphasis::default()
        };
        let shared = if runs().next().is_some() { shared } else { Emphasis::default() };
        if shared.bold && attributes.properties.weight.is_none() {
            attributes = attributes.set(Field::FontWeight, |p| p.weight = Some(TextWeight::Bold));
        }
        if shared.italic {
            attributes.emission.push(Field::FontStyle);
        }
        if shared.mono {
            attributes = attributes.set(Field::FontFamily, |p| p.mono = true);
        }
        let shift = label.size * 0.35;
        let top = y - shift - label.line_height / 2.0;
        let mut placed = Label { lines: Vec::new(), ..label.clone() };
        let mut lines = Vec::new();
        for (i, line) in label.lines.iter().enumerate() {
            let runs: Vec<LabelRun> = line.runs.iter().filter(|run| !run.text.is_empty()).cloned().collect();
            if runs.is_empty() {
                continue;
            }
            let middle = top + (i as f64 + 0.5) * label.line_height;
            lines.push([Number::num(x), Number::num(middle + shift)]);
            placed.lines.push(LabelLine { runs, width: line.width });
        }
        let text = PlacedText { label: placed, emission: TextEmission::Rich { lines, shared } };
        self.push(attributes, NodeContent::Text(text));
    }

    /// Pele's `startLabel` (`wardley/render.ts`): a label whose lines start at `x`, with the
    /// baseline of its first line at `baseline` and each next one a line height lower. Each line's
    /// first run carries the line's point and a line with no runs writes nothing. No anchor is
    /// written, so the text starts where the groups around it say. The emphasis every run has is
    /// written once, unless the look already writes that attribute. Nothing is drawn when no line
    /// has a run.
    pub(crate) fn start_lines(
        &mut self,
        part: PartId,
        facet: Facet,
        label: &Label,
        x: f64,
        baseline: f64,
        look: LabelLook,
    ) {
        let mut kept = Label { lines: Vec::new(), ..label.clone() };
        let mut lines = Vec::new();
        let mut y = baseline;
        for line in &label.lines {
            if !line.runs.is_empty() {
                lines.push([Number::num(x), Number::num(y)]);
                kept.lines.push(line.clone());
            }
            y += label.line_height;
        }
        if kept.lines.is_empty() {
            return;
        }
        let runs = || label.lines.iter().flat_map(|line| &line.runs).filter(|run| !run.text.is_empty());
        let shared = Emphasis {
            bold: runs().all(|run| run.style.bold),
            italic: runs().all(|run| run.style.italic),
            mono: runs().all(|run| run.style.mono),
            ..Emphasis::default()
        };
        let shared = if runs().next().is_some() { shared } else { Emphasis::default() };
        let mut attributes = Attributes { part: Some(part), facet: Some(facet), ..look.0 };
        let gives = |attributes: &Attributes, field: Field| attributes.emission.contains(&field);
        if shared.bold && !gives(&attributes, Field::FontWeight) {
            attributes = attributes.set(Field::FontWeight, |p| p.weight = Some(TextWeight::Bold));
        }
        if shared.italic && !gives(&attributes, Field::FontStyle) {
            attributes.emission.push(Field::FontStyle);
        }
        if shared.mono && !gives(&attributes, Field::FontFamily) {
            attributes = attributes.set(Field::FontFamily, |p| p.mono = true);
        }
        let text = PlacedText { label: kept, emission: TextEmission::Rich { lines, shared } };
        self.push(attributes, NodeContent::Text(text));
    }

    /// Pele's eventmodeling `lines`: one text element whose lines each start at their own
    /// baseline point, written as `tspan`s with no anchor of the element's own. A line with no
    /// text is left out, and nothing is drawn when none is left. Every line is one run with the
    /// emphasis the look writes.
    pub(crate) fn rows(&mut self, part: PartId, facet: Facet, label: &Label, at: &[[Number; 2]], look: LabelLook) {
        let mut kept = Label { lines: Vec::new(), ..label.clone() };
        let mut lines = Vec::new();
        for (line, &point) in label.lines.iter().zip(at) {
            if line.runs.iter().all(|run| run.text.is_empty()) {
                continue;
            }
            kept.lines.push(line.clone());
            lines.push(point);
        }
        let Some(shared) = kept.lines.first().and_then(|line| line.runs.first()).map(|run| run.style) else {
            return;
        };
        let attributes = Attributes { part: Some(part), facet: Some(facet), ..look.0 };
        let text = PlacedText { label: kept, emission: TextEmission::Rich { lines, shared } };
        self.push(attributes, NodeContent::Text(text));
    }

    /// Plain text at a baseline point, for text whose group sets its anchor and size.
    pub(crate) fn baseline_text(&mut self, part: PartId, label: Label, at: [Number; 2], fill: Option<Paint>) {
        let mut attributes = Attributes::of(part, Facet::Label).geometry(&[GeometryField::X, GeometryField::Y]);
        if let Some(paint) = fill {
            attributes = attributes.set(Field::Fill, |p| p.fill = Some(paint));
        }
        let text = PlacedText { label, emission: TextEmission::Plain { x: at[0], y: at[1] } };
        self.push(attributes, NodeContent::Text(text));
    }

    /// Starts a text element of one plain run, whose attributes are written in the order they
    /// are given, as the text of a class's member is.
    pub(crate) fn text(&mut self, part: PartId, facet: Facet) -> TextStart<'_> {
        TextStart { builder: self, attributes: Attributes::of(part, facet), style: Emphasis::default(), at: None }
    }

    /// Seals the drawing. Nothing of an attempt is visible before this returns.
    pub(crate) fn finish(mut self) -> Result<Scene, RenderError> {
        let roots = self.open.pop().expect("the roots are always open");
        debug_assert!(self.open.is_empty(), "every layer is closed");
        Ok(Scene {
            document: self.document,
            arena: self.arena,
            roots,
            entities: self.entities.into_values().collect(),
            relations: self.relations,
        })
    }
}

/// A shape before it has its geometry.
pub(crate) struct ElementStart<'a> {
    builder: &'a mut SceneBuilder,
    attributes: Attributes,
}

impl<'a> ElementStart<'a> {
    pub(crate) fn class(mut self, class: &str) -> Self {
        self.attributes = self.attributes.set(Field::Class, |p| p.class = Some(class.to_owned()));
        self
    }

    pub(crate) fn data_id(mut self) -> Self {
        debug_assert!(self.attributes.part.is_some(), "only a part of an entity has the entity's name");
        self.attributes.emission.push(Field::DataId);
        self
    }

    /// A fill written before the geometry, where Pele's template puts it there.
    pub(crate) fn fill(mut self, paint: Paint) -> Self {
        self.attributes = self.attributes.set(Field::Fill, |p| p.fill = Some(paint));
        self
    }

    /// A stroke written before the geometry, where Pele's template puts it there.
    pub(crate) fn stroke(mut self, paint: Paint) -> Self {
        self.attributes = self.attributes.set(Field::Stroke, |p| p.stroke = Some(paint));
        self
    }

    fn shape(self, content: NodeContent, fields: &[GeometryField]) -> ShapeElement<'a> {
        ShapeElement { builder: self.builder, attributes: self.attributes.geometry(fields), content }
    }

    pub(crate) fn path(self, path: PathProgram) -> ShapeElement<'a> {
        self.shape(NodeContent::Path(path), &[GeometryField::D])
    }

    /// A rectangle at `origin`, with one corner radius where it has one of its own.
    pub(crate) fn rect(self, origin: [Number; 2], size: [Number; 2], rx: Option<Number>) -> ShapeElement<'a> {
        let content = NodeContent::Rect { x: origin[0], y: origin[1], width: size[0], height: size[1], rx };
        use GeometryField::{Height, Rx, Width, X, Y};
        let fields: &[GeometryField] = if rx.is_some() { &[X, Y, Width, Height, Rx] } else { &[X, Y, Width, Height] };
        self.shape(content, fields)
    }

    /// A rectangle whose top is its group's origin, written without a `y`.
    pub(crate) fn rect_from_top(self, x: Number, size: [Number; 2]) -> ShapeElement<'a> {
        let content = NodeContent::Rect { x, y: Number::raw(0.0), width: size[0], height: size[1], rx: None };
        use GeometryField::{Height, Width, X};
        self.shape(content, &[X, Width, Height])
    }

    /// A rectangle whose top left corner is its group's origin, written with neither `x` nor `y`,
    /// as a kanban column is.
    pub(crate) fn rect_at_origin(self, size: [Number; 2]) -> ShapeElement<'a> {
        let zero = Number::raw(0.0);
        let content = NodeContent::Rect { x: zero, y: zero, width: size[0], height: size[1], rx: None };
        self.shape(content, &[GeometryField::Width, GeometryField::Height])
    }

    /// A rectangle whose attributes are written x, width, y, height, as a row of an ER entity's
    /// table is.
    pub(crate) fn row_rect(self, origin: [Number; 2], size: [Number; 2]) -> ShapeElement<'a> {
        let content = NodeContent::Rect { x: origin[0], y: origin[1], width: size[0], height: size[1], rx: None };
        use GeometryField::{Height, Width, X, Y};
        self.shape(content, &[X, Width, Y, Height])
    }

    /// A circle centred on its group's origin.
    pub(crate) fn circle(self, radius: Number) -> ShapeElement<'a> {
        self.circle_at([None, None], radius)
    }

    /// A circle whose centre is off its group's origin along the axes given.
    pub(crate) fn circle_at(self, centre: [Option<Number>; 2], radius: Number) -> ShapeElement<'a> {
        let [cx, cy] = centre;
        let mut fields = Vec::new();
        fields.extend(cx.map(|_| GeometryField::Cx));
        fields.extend(cy.map(|_| GeometryField::Cy));
        fields.push(GeometryField::Radius);
        self.shape(NodeContent::Circle { cx, cy, radius }, &fields)
    }

    pub(crate) fn ellipse(self, rx: Number, ry: Number) -> ShapeElement<'a> {
        self.shape(NodeContent::Ellipse { rx, ry }, &[GeometryField::Rx, GeometryField::Ry])
    }

    pub(crate) fn polygon(self, points: Vec<[Number; 2]>) -> ShapeElement<'a> {
        self.shape(NodeContent::Polygon(points), &[GeometryField::Points])
    }
}

/// A text element of one run before it has its text. Its run's emphasis is what an interpreter
/// sets the text in; the attributes are where the SVG says so.
pub(crate) struct TextStart<'a> {
    builder: &'a mut SceneBuilder,
    attributes: Attributes,
    style: Emphasis,
    at: Option<[Number; 2]>,
}

impl TextStart<'_> {
    fn with(mut self, field: Field, set: impl FnOnce(&mut Properties)) -> Self {
        self.attributes = self.attributes.set(field, set);
        self
    }

    pub(crate) fn class(self, class: &str) -> Self {
        self.with(Field::Class, |p| p.class = Some(class.to_owned()))
    }

    /// The baseline point of the run.
    pub(crate) fn at(mut self, at: [Number; 2]) -> Self {
        self.at = Some(at);
        self.attributes = self.attributes.geometry(&[GeometryField::X, GeometryField::Y]);
        self
    }

    pub(crate) fn anchor(self, anchor: AnchorKind) -> Self {
        self.with(Field::Anchor, |p| p.anchor = Some(anchor))
    }

    pub(crate) fn font_size(self, size: Number) -> Self {
        self.with(Field::FontSize, |p| p.font_size = Some(size))
    }

    pub(crate) fn fill(self, paint: Option<Paint>) -> Self {
        match paint {
            Some(paint) => self.with(Field::Fill, |p| p.fill = Some(paint)),
            None => self,
        }
    }

    /// A weight for the run as a whole, written here as an attribute.
    pub(crate) fn weight(self, weight: TextWeight) -> Self {
        self.with(Field::FontWeight, |p| p.weight = Some(weight))
    }

    pub(crate) fn stroke(self, paint: Paint) -> Self {
        self.with(Field::Stroke, |p| p.stroke = Some(paint))
    }

    pub(crate) fn stroke_width(self, width: Number) -> Self {
        self.with(Field::StrokeWidth, |p| p.stroke_width = Some(width))
    }

    pub(crate) fn join(self, join: LineJoin) -> Self {
        self.with(Field::StrokeJoin, |p| p.join = Some(join))
    }

    /// Sets the run in italics, written here as an attribute.
    pub(crate) fn italic(mut self) -> Self {
        self.style.italic = true;
        self.attributes.emission.push(Field::FontStyle);
        self
    }

    /// Underlines the run, written as Pele writes it: `text-decoration:underline` after the text
    /// style declarations given, in the element's `style`.
    pub(crate) fn underlined(mut self, text: Option<&Style>) -> Self {
        self.style.underline = true;
        self.style(Some(Style::underlined(text)))
    }

    pub(crate) fn style(mut self, style: Option<Style>) -> Self {
        self.attributes = self.attributes.style(style);
        self
    }

    /// The run's text, measured `width` wide at `size` pixels.
    pub(crate) fn finish(self, text: &[u16], width: f64, size: f64) {
        let [x, y] = self.at.expect("a text element has a place");
        let label = Label::single(text, self.style, width, size, js::round(size * 1.5));
        self.builder
            .push(self.attributes, NodeContent::Text(PlacedText { label, emission: TextEmission::Plain { x, y } }));
    }

    /// Sets the run in the monospace font, written here as an attribute.
    pub(crate) fn mono(mut self) -> Self {
        self.style.mono = true;
        self.with(Field::FontFamily, |p| p.mono = true)
    }

    /// Keeps the spaces of the text as they are, written here as an attribute.
    pub(crate) fn preserve_space(mut self) -> Self {
        self.attributes.emission.push(Field::PreserveSpace);
        self
    }

    /// Several runs on one line, each `(text, width, fill)`: a run with a fill of its own is
    /// written as a `tspan` in it. Every run has the emphasis given so far.
    pub(crate) fn finish_painted(self, runs: &[(&[u16], f64, Option<Paint>)], size: f64) {
        let [x, y] = self.at.expect("a text element has a place");
        let mut width = 0.0;
        let mut line = LabelLine::default();
        for &(text, run_width, _) in runs {
            line.runs.push(LabelRun { text: text.into(), style: self.style, width: run_width });
            width += run_width;
        }
        line.width = width;
        let line_height = js::round(size * 1.5);
        let label = Label { lines: vec![line], width, height: line_height, size, line_height };
        let fills = runs.iter().map(|&(_, _, fill)| fill).collect();
        let emission = TextEmission::Painted { x, y, fills, shared: self.style };
        self.builder.push(self.attributes, NodeContent::Text(PlacedText { label, emission }));
    }

    /// Plain runs, the last of them in a `tspan` with `class` and `fill`. Each run is given with
    /// its width, and `width` is the width of the whole line as the diagram measured it.
    pub(crate) fn finish_tailed(self, runs: &[(&[u16], f64)], class: &'static str, fill: Paint, width: f64, size: f64) {
        let [x, y] = self.at.expect("a text element has a place");
        let runs = runs.iter().map(|&(text, width)| LabelRun { text: text.into(), style: self.style, width }).collect();
        let line = LabelLine { runs, width };
        let label =
            Label { lines: vec![line], width, height: js::round(size * 1.5), size, line_height: js::round(size * 1.5) };
        let emission = TextEmission::Tailed { x, y, class, fill };
        self.builder.push(self.attributes, NodeContent::Text(PlacedText { label, emission }));
    }
}

/// A shape with its geometry, taking paint until it is finished.
pub(crate) struct ShapeElement<'a> {
    builder: &'a mut SceneBuilder,
    attributes: Attributes,
    content: NodeContent,
}

impl ShapeElement<'_> {
    fn with(mut self, field: Field, set: impl FnOnce(&mut Properties)) -> Self {
        self.attributes = self.attributes.set(field, set);
        self
    }

    pub(crate) fn fill(self, paint: Paint) -> Self {
        self.with(Field::Fill, |p| p.fill = Some(paint))
    }

    pub(crate) fn fill_opacity(self, opacity: Number) -> Self {
        self.with(Field::FillOpacity, |p| p.fill_opacity = Some(opacity))
    }

    /// Fills the shape along `gradient`, with its fill drawn where the gradient cannot be.
    pub(crate) fn gradient(mut self, gradient: GradientId) -> Self {
        self.attributes.properties.gradient = Some(gradient);
        self
    }

    /// Writes the entity's tooltip inside the shape, as Pele writes sankey's links and a
    /// journey's actor dots: `<circle …><title>…</title></circle>`.
    pub(crate) fn titled(mut self) -> Self {
        self.attributes.properties.titled = true;
        self
    }

    /// Makes the whole shape this opaque, written where this is called.
    pub(crate) fn opacity(self, opacity: Number) -> Self {
        self.with(Field::Opacity, |p| p.opacity = Some(opacity))
    }

    /// A class written after the geometry, where Pele's template puts it there.
    pub(crate) fn class(self, class: &str) -> Self {
        self.with(Field::Class, |p| p.class = Some(class.to_owned()))
    }

    /// A rectangle's corner radius, written where this is called.
    pub(crate) fn rx(mut self, rx: Number) -> Self {
        if let NodeContent::Rect { rx: radius, .. } = &mut self.content {
            *radius = Some(rx);
        }
        self.attributes = self.attributes.geometry(&[GeometryField::Rx]);
        self
    }

    pub(crate) fn stroke(self, paint: Paint) -> Self {
        self.with(Field::Stroke, |p| p.stroke = Some(paint))
    }

    pub(crate) fn stroke_width(self, width: Number) -> Self {
        self.with(Field::StrokeWidth, |p| p.stroke_width = Some(width))
    }

    pub(crate) fn join(self, join: LineJoin) -> Self {
        self.with(Field::StrokeJoin, |p| p.join = Some(join))
    }

    pub(crate) fn cap(self, cap: LineCap) -> Self {
        self.with(Field::StrokeCap, |p| p.cap = Some(cap))
    }

    /// Dashes and the gaps between them, in turn.
    pub(crate) fn dash(self, pattern: &[Number]) -> Self {
        self.with(Field::DashArray, |p| p.dash = pattern.to_vec())
    }

    /// A solid stroke inside a group that dashes its strokes: `stroke-dasharray="none"`.
    pub(crate) fn undashed(self) -> Self {
        self.with(Field::DashArray, |p| p.undashed = true)
    }

    pub(crate) fn style(mut self, style: Option<Style>) -> Self {
        self.attributes = self.attributes.style(style);
        self
    }

    pub(crate) fn look(mut self, look: Look) -> Self {
        self.attributes = self.attributes.look(look);
        self
    }

    pub(crate) fn finish(self) {
        self.builder.push(self.attributes, self.content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> JsValue {
        JsValue::String(value.into())
    }

    #[test]
    fn safe_color_keeps_colours_and_drops_what_could_escape() {
        for good in [
            "red",
            " #AbC ",
            "#12345678",
            "Grey",
            "rgb(1, 2, 3)",
            "color-mix(in srgb, red 40%, blue)",
            "VAR(--pele-accent)",
        ] {
            assert!(safe_color(&text(good)).is_some(), "{good:?} is a colour");
        }
        let bad = [
            "",
            "notacolor",
            "#12",
            "red;",
            "url(x)",
            "rgb(1,2,3",
            "rgb)1(",
            "var(--page)",
            "var( --Pele-x)",
            "a\"b",
            "expression (1)",
            " (1)",
        ];
        for bad in bad {
            assert!(safe_color(&text(bad)).is_none(), "{bad:?} is not a safe colour");
        }
        assert!(safe_color(&JsValue::Number(1.0)).is_none());
        assert!(safe_color(&text(&"f".repeat(201))).is_none());
    }

    #[test]
    fn ink_contrasts_with_the_fill() {
        let ink = |fill: &str| ink(&fill.encode_utf16().collect::<Vec<_>>());
        assert_eq!(ink("navy"), Some(Role::OnDark));
        assert_eq!(ink("#ffeb3b"), Some(Role::OnLight));
        assert_eq!(ink("#000"), Some(Role::OnDark));
        assert_eq!(ink("rgb(255 255 255 / 0.6)"), Some(Role::OnLight));
        assert_eq!(ink("hsl(240, 100%, 25%)"), Some(Role::OnDark));
        assert_eq!(ink("rgba(0, 0, 0, 0.2)"), None);
        assert_eq!(ink("var(--pele-accent)"), None);
        assert_eq!(ink("rgb(1, 2)"), None);
    }
}
