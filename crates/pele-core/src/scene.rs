//! The Scene: one ordered drawing that every interpreter reads.
//!
//! Paint order lives in the node tree. Identity lives in a separate table of entities, so one
//! section can own a slice in one layer and a legend row in another. Each number, path and text
//! is stored once, with the spelling Pele gives it kept beside the value: [`Number`] remembers
//! its number rule, a path remembers its commands, and each node remembers the order its
//! attributes were set in. Only this module builds a Scene, and `crate::svg` only reads one.

use crate::catalog::DrawContext;
use crate::input::{JsObject, JsText, JsValue, Units, decimal, is_digit, is_space, is_word};
use crate::measure::{Emphasis, Label};
use crate::{DiagramKind, RenderError, js};
use indexmap::IndexMap;
use std::sync::LazyLock;

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
    Border,
    /// Text on a light fill.
    OnLight,
    /// Text on a dark fill.
    OnDark,
}

/// One of the eight series colours. A position past the eighth wraps around.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeriesSlot(u8);

impl SeriesSlot {
    pub(crate) fn new(position: usize) -> Self {
        SeriesSlot((position % 8) as u8)
    }

    pub(crate) fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// A colour as an expression, so that a theme change needs no new layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    None,
    Role(Role),
    Series(SeriesSlot),
    /// Text on a series colour. Pele sets it only where the source overrides that series with a
    /// colour whose contrast is known, and otherwise leaves the text to inherit.
    OnSeries(SeriesSlot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
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
}

/// Which part of an entity a node draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Facet {
    Shape,
    Label,
    Legend,
    Title,
}

/// Mermaid's palette theme variables for one diagram type: the name before the number, and the
/// number of the variable for the first series.
#[derive(Clone, Copy)]
pub(crate) enum PaletteRule {
    Slots(&'static str, usize),
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
        let PaletteRule::Slots(prefix, first) = rule;
        let variables = config.get("themeVariables").and_then(JsValue::as_record);
        let slot = |index: usize| {
            let spelling = safe_color(variables?.get(&format!("{prefix}{}", first + index))?)?;
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
    if is_hex_color(&value) {
        let digits: Vec<u32> = value[1..].chars().filter_map(|c| c.to_digit(16)).collect();
        let digits: Vec<u32> = if value.len() < 6 { digits.iter().flat_map(|&d| [d, d]).collect() } else { digits };
        let n = |i: usize| f64::from(digits[2 * i] * 16 + digits[2 * i + 1]) / 255.0;
        return Some((luminance(n(0), n(1), n(2)), if digits.len() == 8 { n(3) } else { 1.0 }));
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
    let lum = luminance(rgb[0], rgb[1], rgb[2]);
    (!(lum + alpha).is_nan()).then_some((lum, alpha))
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

    pub(crate) fn write(&self, out: &mut String) {
        match self.form {
            NumberForm::Num => js::write_num(out, self.value),
            NumberForm::Raw => out.push_str(&js::to_string(self.value)),
            NumberForm::Fixed(digits) => out.push_str(&js::to_fixed(self.value, usize::from(digits))),
        }
    }
}

/// A path as the commands Pele writes, in the one spelling pie uses: no separator before a
/// command, a comma inside a pair and spaces around an arc's flags.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathProgram {
    pub(crate) steps: Vec<PathStep>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PathStep {
    Move([Number; 2]),
    Line([Number; 2]),
    /// An unrotated elliptical arc.
    Arc {
        radii: [Number; 2],
        large: bool,
        sweep: bool,
        end: [Number; 2],
    },
    Close,
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

/// The two forms Pele writes text in.
#[derive(Clone, Debug)]
pub(crate) enum TextEmission {
    /// One plain run, written as the text element's own content at one baseline point.
    Plain { x: Number, y: Number },
    /// Lines of runs. Each line's first run carries the line's point, and `shared` is the
    /// emphasis every run has, which the text element carries once.
    Rich { lines: Vec<[Number; 2]>, shared: Emphasis },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PartId(u32);

#[derive(Clone, Debug)]
pub(crate) enum NodeContent {
    Group(Vec<NodeId>),
    Path(PathProgram),
    Rect {
        x: Number,
        y: Number,
        width: Number,
        height: Number,
        rx: Number,
    },
    /// A circle centred on its group's origin.
    Circle {
        radius: Number,
    },
    Text(PlacedText),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Transform {
    Translate(Number, Number),
}

/// What a node sets. Each value is stored once; `Node::emission` gives the order they are written in.
#[derive(Clone, Debug, Default)]
pub(crate) struct Properties {
    pub(crate) class: Option<&'static str>,
    pub(crate) transform: Option<Transform>,
    pub(crate) fill: Option<Paint>,
    pub(crate) stroke: Option<Paint>,
    pub(crate) stroke_width: Option<Number>,
    pub(crate) join: Option<LineJoin>,
    pub(crate) font_size: Option<Number>,
    pub(crate) weight: Option<TextWeight>,
    pub(crate) anchor: Option<AnchorKind>,
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
    Stroke,
    StrokeWidth,
    StrokeJoin,
    FontSize,
    FontWeight,
    /// Italic, where every run of a label is.
    FontStyle,
    /// Monospace, where every run of a label is.
    FontFamily,
    Anchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GeometryField {
    X,
    Y,
    Width,
    Height,
    Rx,
    Radius,
    D,
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub(crate) part: Option<PartId>,
    #[expect(dead_code, reason = "read by the Rune interpreter, which is not built yet")]
    pub(crate) facet: Option<Facet>,
    pub(crate) content: NodeContent,
    pub(crate) properties: Properties,
    pub(crate) emission: Vec<Field>,
}

/// What something in a diagram is, apart from where and how it is drawn.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct EntityKey {
    kind: EntityKind,
    identity: JsText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EntityKind {
    Section,
}

/// An entity as a diagram names it while drawing.
pub(crate) struct Entity {
    key: EntityKey,
    author_text: JsText,
}

impl Entity {
    /// A pie section. Its label is its identity, since a pie keeps one section per label.
    pub(crate) fn pie_section(label: &JsText) -> Self {
        Entity { key: EntityKey { kind: EntityKind::Section, identity: label.clone() }, author_text: label.clone() }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EntityRecord {
    /// The name the author gave, which `data-id` carries.
    pub(crate) author_text: JsText,
}

/// The accessible title and description a source gives its diagram.
#[derive(Clone, Debug, Default)]
pub(crate) struct Accessible {
    pub(crate) title: Option<JsText>,
    pub(crate) description: Option<JsText>,
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
    pub(crate) limit_utf16: usize,
}

/// A finished drawing. Its nodes are in paint order and cannot change.
#[derive(Clone, Debug)]
pub struct Scene {
    pub(crate) document: Document,
    pub(crate) arena: Vec<Node>,
    pub(crate) roots: Vec<NodeId>,
    pub(crate) entities: Vec<EntityRecord>,
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
}

/// Properties with the order they were set in. The builders below add to it at each call.
#[derive(Default)]
struct Attributes {
    part: Option<PartId>,
    facet: Option<Facet>,
    properties: Properties,
    emission: Vec<Field>,
}

impl Attributes {
    fn set(mut self, field: Field, set: impl FnOnce(&mut Properties)) -> Self {
        set(&mut self.properties);
        self.emission.push(field);
        self
    }

    fn node(self, content: NodeContent) -> Node {
        Node { part: self.part, facet: self.facet, content, properties: self.properties, emission: self.emission }
    }
}

/// A group of nodes that share paint order and inherited attributes.
pub(crate) struct LayerSpec(Attributes);

impl LayerSpec {
    pub(crate) fn class(class: &'static str) -> Self {
        LayerSpec(Attributes::default().set(Field::Class, |p| p.class = Some(class)))
    }

    /// Makes the group a part of an entity.
    pub(crate) fn part(mut self, part: PartId, facet: Facet) -> Self {
        self.0.part = Some(part);
        self.0.facet = Some(facet);
        self
    }

    pub(crate) fn data_id(self) -> Self {
        debug_assert!(self.0.part.is_some(), "only a part of an entity has the entity's name");
        LayerSpec(self.0.set(Field::DataId, |_| {}))
    }

    pub(crate) fn translate(self, point: Point) -> Self {
        let transform = Transform::Translate(Number::num(point.x), Number::num(point.y));
        LayerSpec(self.0.set(Field::Transform, |p| p.transform = Some(transform)))
    }

    pub(crate) fn stroke(self, paint: Paint) -> Self {
        LayerSpec(self.0.set(Field::Stroke, |p| p.stroke = Some(paint)))
    }

    pub(crate) fn stroke_width(self, width: Number) -> Self {
        LayerSpec(self.0.set(Field::StrokeWidth, |p| p.stroke_width = Some(width)))
    }

    pub(crate) fn join(self, join: LineJoin) -> Self {
        LayerSpec(self.0.set(Field::StrokeJoin, |p| p.join = Some(join)))
    }

    pub(crate) fn font_size(self, size: Number) -> Self {
        LayerSpec(self.0.set(Field::FontSize, |p| p.font_size = Some(size)))
    }

    pub(crate) fn anchor(self, anchor: AnchorKind) -> Self {
        LayerSpec(self.0.set(Field::Anchor, |p| p.anchor = Some(anchor)))
    }

    pub(crate) fn fill(self, paint: Paint) -> Self {
        LayerSpec(self.0.set(Field::Fill, |p| p.fill = Some(paint)))
    }
}

/// How a label's text element looks, beyond the label's own emphasis.
#[derive(Default)]
pub(crate) struct LabelLook(Attributes);

impl LabelLook {
    pub(crate) fn class(class: &'static str) -> Self {
        LabelLook(Attributes::default().set(Field::Class, |p| p.class = Some(class)))
    }

    pub(crate) fn weight(self, weight: TextWeight) -> Self {
        LabelLook(self.0.set(Field::FontWeight, |p| p.weight = Some(weight)))
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
            limit_utf16: cx.limits.svg_utf16,
        };
        SceneBuilder { document, arena: Vec::new(), open: vec![Vec::new()], entities: IndexMap::new() }
    }

    fn push(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.arena.len() as u32);
        self.arena.push(node);
        self.open.last_mut().expect("the roots are always open").push(id);
        id
    }

    /// The part for an entity. Asking again for the same entity gives the same part.
    pub(crate) fn part(&mut self, entity: Entity) -> PartId {
        let entry = self.entities.entry(entity.key);
        let index = entry.index();
        entry.or_insert(EntityRecord { author_text: entity.author_text });
        PartId(index as u32)
    }

    /// Appends a group and draws its children in order. A layer that stays empty is not kept.
    pub(crate) fn layer(&mut self, spec: LayerSpec, draw: impl FnOnce(&mut Self)) {
        self.open.push(Vec::new());
        draw(self);
        let children = self.open.pop().expect("the layer opened above");
        if !children.is_empty() {
            self.push(spec.0.node(NodeContent::Group(children)));
        }
    }

    /// Starts a shape that is a part of an entity.
    pub(crate) fn element(&mut self, part: PartId, facet: Facet) -> ElementStart<'_> {
        let attributes = Attributes { part: Some(part), facet: Some(facet), ..Attributes::default() };
        ElementStart { builder: self, attributes }
    }

    /// Starts a shape that belongs to no entity.
    pub(crate) fn decoration(&mut self) -> ElementStart<'_> {
        ElementStart { builder: self, attributes: Attributes::default() }
    }

    pub(crate) fn title(&mut self, title: &TitleLayout) {
        let look = LabelLook::class("pele-title").weight(TextWeight::Title);
        self.label(None, Facet::Title, &title.label, title.at, look);
    }

    /// Pele's `labelSvg`: a measured label as text centred on `at`. An empty label draws nothing.
    pub(crate) fn label(&mut self, part: Option<PartId>, facet: Facet, label: &Label, at: Point, look: LabelLook) {
        if label.lines.is_empty() {
            return;
        }
        let shift = label.size * 0.35;
        let mut top = at.y - label.height / 2.0;
        let mut attributes = Attributes { part, facet: Some(facet), ..look.0 };
        let middle = |p: &mut Properties| p.anchor = Some(AnchorKind::Middle);

        let only = match label.lines.as_slice() {
            [line] if line.runs.len() == 1 => Some(&line.runs[0]),
            _ => None,
        };
        if only.is_some_and(|run| run.style.is_plain()) {
            let (x, y) = (Number::num(at.x), Number::num(top + label.line_height / 2.0 + shift));
            attributes.emission.extend([Field::Geometry(GeometryField::X), Field::Geometry(GeometryField::Y)]);
            let text = PlacedText { label: label.clone(), emission: TextEmission::Plain { x, y } };
            self.push(attributes.set(Field::Anchor, middle).node(NodeContent::Text(text)));
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
        if shared.italic {
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
        self.push(attributes.set(Field::Anchor, middle).node(NodeContent::Text(text)));
    }

    /// Plain text at a baseline point, for text whose group sets its anchor and size.
    pub(crate) fn baseline_text(&mut self, part: PartId, label: Label, at: [Number; 2], fill: Option<Paint>) {
        let mut attributes = Attributes { part: Some(part), facet: Some(Facet::Label), ..Attributes::default() };
        attributes.emission.extend([Field::Geometry(GeometryField::X), Field::Geometry(GeometryField::Y)]);
        if let Some(paint) = fill {
            attributes = attributes.set(Field::Fill, |p| p.fill = Some(paint));
        }
        let text = PlacedText { label, emission: TextEmission::Plain { x: at[0], y: at[1] } };
        self.push(attributes.node(NodeContent::Text(text)));
    }

    /// Seals the drawing. Nothing of an attempt is visible before this returns.
    pub(crate) fn finish(mut self) -> Result<Scene, RenderError> {
        let roots = self.open.pop().expect("the roots are always open");
        debug_assert!(self.open.is_empty(), "every layer is closed");
        Ok(Scene { document: self.document, arena: self.arena, roots, entities: self.entities.into_values().collect() })
    }
}

/// A shape before it has its geometry.
pub(crate) struct ElementStart<'a> {
    builder: &'a mut SceneBuilder,
    attributes: Attributes,
}

impl<'a> ElementStart<'a> {
    pub(crate) fn class(mut self, class: &'static str) -> Self {
        self.attributes = self.attributes.set(Field::Class, |p| p.class = Some(class));
        self
    }

    pub(crate) fn data_id(mut self) -> Self {
        debug_assert!(self.attributes.part.is_some(), "only a part of an entity has the entity's name");
        self.attributes.emission.push(Field::DataId);
        self
    }

    fn shape(mut self, content: NodeContent, fields: &[GeometryField]) -> ShapeElement<'a> {
        self.attributes.emission.extend(fields.iter().map(|&field| Field::Geometry(field)));
        ShapeElement { builder: self.builder, attributes: self.attributes, content }
    }

    pub(crate) fn path(self, path: PathProgram) -> ShapeElement<'a> {
        self.shape(NodeContent::Path(path), &[GeometryField::D])
    }

    /// A rectangle with one corner radius, at `origin`.
    pub(crate) fn rect(self, origin: [Number; 2], size: [Number; 2], rx: Number) -> ShapeElement<'a> {
        let content = NodeContent::Rect { x: origin[0], y: origin[1], width: size[0], height: size[1], rx };
        use GeometryField::{Height, Rx, Width, X, Y};
        self.shape(content, &[X, Y, Width, Height, Rx])
    }

    pub(crate) fn circle(self, radius: Number) -> ShapeElement<'a> {
        self.shape(NodeContent::Circle { radius }, &[GeometryField::Radius])
    }
}

/// A shape with its geometry, taking paint until it is finished.
pub(crate) struct ShapeElement<'a> {
    builder: &'a mut SceneBuilder,
    attributes: Attributes,
    content: NodeContent,
}

impl ShapeElement<'_> {
    pub(crate) fn fill(mut self, paint: Paint) -> Self {
        self.attributes = self.attributes.set(Field::Fill, |p| p.fill = Some(paint));
        self
    }

    pub(crate) fn stroke(mut self, paint: Paint) -> Self {
        self.attributes = self.attributes.set(Field::Stroke, |p| p.stroke = Some(paint));
        self
    }

    pub(crate) fn finish(self) {
        self.builder.push(self.attributes.node(self.content));
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
