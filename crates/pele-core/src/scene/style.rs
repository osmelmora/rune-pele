//! Style declarations: the ones Mermaid's `style` and `classDef` statements give a node, as Pele
//! writes them into `style` attributes, and what they say about the fill under a label. Ported
//! from Pele's `src/svg/theme.ts` and `src/svg/ink.ts`.
//!
//! Pele reads its own attribute text back with regular expressions. The questions it asks that
//! way are asked here of the same text, [`Style::css`], so each answer is Pele's even where a
//! value holds what looks like a second declaration.

use super::palette::css_colour;
use super::{EVEN, Paint, Properties, Role, amount, color, esc, js_trim, lowercase, xml_drops};
use crate::input::{JsText, Units, is_space, string_to_number, to_lowercase};
use indexmap::IndexMap;

/// CSS declarations as they are written into a `style` attribute, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Style {
    pub(crate) declarations: Vec<Declaration>,
    /// Whether a `;` follows the last declaration. Pele writes some lists with one and some
    /// without, and code that adds to a list has to know which it has.
    pub(crate) terminated: bool,
}

/// One declaration. The value is kept as it is written, escaped for XML, so that what was
/// checked is what is written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Declaration {
    pub(crate) property: String,
    pub(crate) written: String,
}

impl Style {
    pub(crate) fn unterminated() -> Self {
        Style::default()
    }

    /// A list that ends in `;`, as `resolveStyle` writes its lists.
    pub(crate) fn terminated() -> Self {
        Style { declarations: Vec::new(), terminated: true }
    }

    pub(crate) fn declare(&mut self, property: &str, value: impl Into<JsText>) {
        let mut written = String::new();
        esc(&value.into(), &mut written);
        self.declare_written(property, written);
    }

    /// Adds a declaration whose value is already escaped, such as one read from another style.
    pub(crate) fn declare_written(&mut self, property: &str, written: String) {
        self.declarations.push(Declaration { property: property.to_owned(), written });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    /// Pele's `rounded`: the theme's corner radius declared before `style`'s own declarations,
    /// so that they can override it. With no style to add to, the list is the radius alone.
    pub(crate) fn rounded(style: Option<&Style>) -> Style {
        let mut rounded = Style { declarations: Vec::new(), terminated: style.is_some() };
        rounded.declare("rx", "var(--_r)");
        rounded.declarations.extend(style.into_iter().flat_map(|style| style.declarations.iter().cloned()));
        rounded
    }

    /// The value of the `style` attribute.
    pub(crate) fn write(&self, out: &mut String) {
        for (index, declaration) in self.declarations.iter().enumerate() {
            if index > 0 {
                out.push(';');
            }
            out.push_str(&declaration.property);
            out.push(':');
            out.push_str(&declaration.written);
        }
        if self.terminated {
            out.push(';');
        }
    }

    pub(crate) fn css(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    /// Whether the style gives a fill. Pele's `setsFill`.
    pub(crate) fn sets_fill(&self) -> bool {
        !declared(&self.css(), &["fill"]).is_empty()
    }

    /// Whether the style declares `property`, as Pele asks `/[";]property:/` of its text.
    pub(crate) fn sets(&self, property: &str) -> bool {
        !declared(&self.css(), &[property]).is_empty()
    }

    /// Text style declarations with `text-decoration:underline` added after them, as Pele adds it
    /// to an underlined member's: the list is no longer terminated.
    pub(crate) fn underlined(style: Option<&Style>) -> Style {
        let mut underlined = Style { declarations: Vec::new(), terminated: false };
        underlined.declarations.extend(style.into_iter().flat_map(|style| style.declarations.iter().cloned()));
        underlined.declare("text-decoration", "underline");
        underlined
    }

    /// Whether the style gives a stroke.
    pub(crate) fn sets_stroke(&self) -> bool {
        !declared(&self.css(), &["stroke"]).is_empty()
    }

    /// The fill the style gives, trimmed, as Pele's `hollowOn` reads a shape style: what follows
    /// the first `fill:`, or nothing where that is empty.
    pub(crate) fn fill_value(&self) -> Option<String> {
        let css = self.css();
        let found = declared(&css, &["fill"]).into_iter().next()?;
        Some(js_trim(found.value).to_owned()).filter(|value| !value.is_empty())
    }

    /// The first value the style declares for `property` that is not empty, trimmed. This is
    /// how Pele's `casing` reads a line style.
    pub(crate) fn first(&self, property: &str) -> Option<String> {
        let css = self.css();
        declared(&css, &[property])
            .into_iter()
            .find(|found| !found.value.is_empty() && found.closed)
            .map(|found| js_trim(found.value).to_owned())
    }
}

/// One match of `/[";]name:([^;"]*)/`.
struct Declared<'a> {
    value: &'a str,
    /// Whether a `;` ends the value.
    closed: bool,
}

/// Every match of `/[";](?:name|name):([^;"]*)/g` in ` style="css"`, for the names given.
fn declared<'a>(css: &'a str, names: &[&str]) -> Vec<Declared<'a>> {
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let rest = &css[at..];
        let name = names.iter().find(|name| rest.strip_prefix(**name).is_some_and(|rest| rest.starts_with(':')));
        if let Some(name) = name {
            let from = at + name.len() + 1;
            let end = css[from..].find(';').map_or(css.len(), |length| from + length);
            out.push(Declared { value: &css[from..end], closed: end < css.len() });
            at = end;
        }
        match css[at..].find(';') {
            Some(next) => at += next + 1,
            None => return out,
        }
    }
}

fn css_of(style: Option<&Style>) -> String {
    style.map(Style::css).unwrap_or_default()
}

/// Pele's `opacity`: how opaque a style's `fill-opacity` and `opacity` leave its fill, or NaN
/// when either is not a plain number.
fn opacity(css: &str) -> f64 {
    let mut product = 1.0;
    for found in declared(css, &["fill-opacity", "opacity"]) {
        product *= amount(&js_trim(found.value).to_lowercase(), 1.0);
    }
    product
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Pele's `ink`: the text colour that contrasts with the fill a shape style gives, unless the
/// text style sets a colour of its own. A fill below half opacity depends on an unknown
/// background, so it has none, and neither has a fill that is not a colour Pele can read.
pub(crate) fn ink_on(shape: Option<&Style>, text: Option<&Style>) -> Option<Role> {
    let shape = css_of(shape);
    let fill = declared(&shape, &["fill"]).into_iter().next()?;
    if text.is_some_and(Style::sets_fill) {
        return None;
    }
    let (lum, alpha) = color(&units(fill.value))?;
    // An opacity that isn't a plain number, such as calc() or var(), may still be low.
    if (alpha * opacity(&shape)).partial_cmp(&0.5).is_none_or(|order| order.is_lt()) {
        return None;
    }
    Some(if lum > *EVEN { Role::OnLight } else { Role::OnDark })
}

/// Pele's `paints`: whether a shape style fills its shape with something that shows. A fill
/// colour or an opacity Pele cannot read counts as opaque.
pub(crate) fn paints(shape: Option<&Style>) -> bool {
    let css = css_of(shape);
    let Some(found) = declared(&css, &["fill"]).into_iter().next() else {
        return false;
    };
    let fill = lowercase(units(found.value).trim_js());
    if fill.is_empty() || fill == "none" || fill == "transparent" {
        return false;
    }
    let scale = opacity(&css);
    color(&units(&fill)).map_or(1.0, |(_, alpha)| alpha) * (if scale.is_nan() { 1.0 } else { scale }) > 0.0
}

const SHAPE_PROPS: [&str; 12] = [
    "fill",
    "fill-opacity",
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-opacity",
    "opacity",
    "rx",
    "ry",
];

const TEXT_PROPS: [&str; 9] = [
    "color",
    "font-size",
    "font-family",
    "font-weight",
    "font-style",
    "text-decoration",
    "letter-spacing",
    "word-spacing",
    "opacity",
];

fn caseless_at(text: &[u16], at: usize, word: &str) -> bool {
    text.get(at..at + word.len()).is_some_and(|found| {
        found.iter().zip(word.bytes()).all(|(&c, b)| u8::try_from(c).is_ok_and(|c| c.eq_ignore_ascii_case(&b)))
    })
}

/// `/url\s*\(|expression\s*\(|javascript:|[<>{};\\]|@import|image-set|\/\*/i`
fn is_unsafe(value: &[u16]) -> bool {
    (0..value.len()).any(|at| {
        let call = |name: &str| {
            caseless_at(value, at, name) && value.at(value.run(at + name.len(), is_space)) == Some(u16::from(b'('))
        };
        u8::try_from(value[at]).is_ok_and(|c| b"<>{};\\".contains(&c))
            || call("url")
            || call("expression")
            || ["javascript:", "@import", "image-set", "/*"].iter().any(|word| caseless_at(value, at, word))
    })
}

/// Pele's `RE_INVALID_XML`: a character XML cannot hold, or a surrogate with no partner.
fn has_invalid_xml(value: &[u16]) -> bool {
    char::decode_utf16(value.iter().copied()).any(|c| c.map_or(true, xml_drops))
}

/// `value.replace(/!important/gi, '')`
fn without_important(value: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(value.len());
    let mut at = 0;
    while at < value.len() {
        if caseless_at(value, at, "!important") {
            at += "!important".len();
        } else {
            out.push(value[at]);
            at += 1;
        }
    }
    out
}

/// `parseFloat` of text that matches `/^[\d.]+(px)?$/`, or `None` where the text does not. The
/// number is NaN where the text holds no digit.
pub(crate) fn px(text: &str) -> Option<f64> {
    let number = text.strip_suffix("px").unwrap_or(text);
    if number.is_empty() || !number.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return None;
    }
    // `parseFloat` reads the longest prefix that is a number: digits, and one point with its digits.
    let whole = number.bytes().take_while(u8::is_ascii_digit).count();
    let fraction = match number[whole..].strip_prefix('.') {
        Some(rest) => rest.bytes().take_while(u8::is_ascii_digit).count(),
        None => 0,
    };
    if whole + fraction == 0 {
        return Some(f64::NAN);
    }
    let end = whole + if fraction > 0 { fraction + 1 } else { 0 };
    Some(crate::input::decimal(&units(&number[..end])))
}

/// What Pele's `resolveStyle` makes of a node's style declarations: one list for its shape, one
/// for the lines inside the shape and one for its text, and what the text's declarations mean
/// for measuring it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ResolvedStyle {
    pub(crate) shape: Option<Style>,
    pub(crate) line: Option<Style>,
    pub(crate) text: Option<Style>,
    /// The stroke colour, as written.
    pub(crate) stroke: Option<JsText>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// In pixels.
    pub(crate) font_size: Option<f64>,
}

fn terminated<'a>(declarations: impl Iterator<Item = (&'a String, &'a JsText)>) -> Option<Style> {
    let mut style = Style::terminated();
    for (property, value) in declarations {
        style.declare(property, value.clone());
    }
    (!style.is_empty()).then_some(style)
}

/// Pele's `resolveStyle`: splits Mermaid style declarations into what applies to a shape and
/// what applies to its text. A later declaration of a property replaces the value of an earlier
/// one and keeps its place.
pub(crate) fn resolve_style<'a>(declarations: impl IntoIterator<Item = &'a JsText>) -> ResolvedStyle {
    let mut shape: IndexMap<String, JsText> = IndexMap::new();
    let mut text: IndexMap<String, JsText> = IndexMap::new();
    for declaration in declarations {
        let Some(at) = declaration.find_unit(u16::from(b':'), 0).filter(|&at| at > 0) else {
            continue;
        };
        let property = String::from_utf16_lossy(&to_lowercase(declaration[..at].trim_js()));
        let value = without_important(&declaration[at + 1..]);
        let value = value.trim_js();
        if value.is_empty() || is_unsafe(value) || has_invalid_xml(value) {
            continue;
        }
        if SHAPE_PROPS.contains(&property.as_str()) {
            shape.insert(property.clone(), value.into());
        }
        if TEXT_PROPS.contains(&property.as_str()) {
            text.insert(if property == "color" { "fill".to_owned() } else { property }, value.into());
        }
    }
    let weight = text.get("font-weight");
    let size = text.get("font-size").and_then(|size| px(&size.to_display()));
    ResolvedStyle {
        stroke: shape.get("stroke").cloned(),
        bold: weight.is_some_and(|weight| {
            weight.is_str("bold") || weight.is_str("bolder") || string_to_number(weight) >= 600.0
        }),
        italic: text.get("font-style").is_some_and(|style| style.is_str("italic")),
        font_size: size.filter(|&px| px > 0.0 && px <= 200.0),
        line: terminated(shape.iter().filter(|(property, _)| !matches!(property.as_str(), "fill" | "fill-opacity"))),
        shape: terminated(shape.iter()),
        text: terminated(text.iter()),
    }
}

/// The class names an author gives, cleaned as Pele's `classNames` cleans them: words of
/// letters, digits, `_` and `-` with one space between them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AuthorClasses(String);

impl AuthorClasses {
    pub(crate) fn new(raw: &[u16]) -> Self {
        let word = |c: u16| crate::input::is_word(c) || c == u16::from(b'-');
        let cleaned: String = raw.iter().map(|&c| if word(c) { char::from(c as u8) } else { ' ' }).collect();
        AuthorClasses(cleaned.split(' ').filter(|name| !name.is_empty()).collect::<Vec<_>>().join(" "))
    }

    /// The names after Pele's own, as one `class` attribute.
    pub(crate) fn after(&self, own: &str) -> String {
        if self.0.is_empty() { own.to_owned() } else { format!("{own} {}", self.0) }
    }

    pub(crate) fn has(&self, name: &str) -> bool {
        self.0.split(' ').any(|own| own == name)
    }
}

/// The corner radius a style gives a rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Radius {
    /// The theme's.
    Theme,
    Px(f64),
}

/// What a node's style declarations mean to an interpreter that has no CSS: each value it can
/// read without a page. A declaration it cannot read leaves what the node's attributes say.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Meaning {
    pub(crate) fill: Option<Paint>,
    pub(crate) stroke: Option<Paint>,
    pub(crate) stroke_width: Option<f64>,
    pub(crate) opacity: Option<f64>,
    pub(crate) fill_opacity: Option<f64>,
    pub(crate) stroke_opacity: Option<f64>,
    pub(crate) dash: Option<Vec<f64>>,
    pub(crate) radius: Option<Radius>,
    pub(crate) font_size: Option<f64>,
}

impl Style {
    /// Reads the declarations in order, so that a later one wins as it does in CSS. `colour`
    /// keeps a colour the source writes and answers the paint that stands for it.
    pub(crate) fn meaning(&self, properties: &Properties, colour: &mut dyn FnMut(&[u16], Role) -> Paint) -> Meaning {
        let role = |paint: Option<Paint>| match paint {
            Some(Paint::Role(role)) => role,
            _ => Role::Foreground,
        };
        let mut meaning = Meaning::default();
        for Declaration { property, written } in &self.declarations {
            let value = &units(written);
            let lower = lowercase(value.trim_js());
            let mut paint = |instead_of: Role| {
                if lower == "none" {
                    Some(Paint::None)
                } else {
                    let readable = css_colour(value, false).is_some() && css_colour(value, true).is_some();
                    readable.then(|| colour(value, instead_of))
                }
            };
            let share = || Some(amount(&lower, 1.0)).filter(|share| !share.is_nan());
            match property.as_str() {
                "fill" => meaning.fill = paint(role(properties.fill)).or(meaning.fill),
                "stroke" => meaning.stroke = paint(role(properties.stroke)).or(meaning.stroke),
                "stroke-width" => meaning.stroke_width = px(written).filter(|w| !w.is_nan()).or(meaning.stroke_width),
                "opacity" => meaning.opacity = share().or(meaning.opacity),
                "fill-opacity" => meaning.fill_opacity = share().or(meaning.fill_opacity),
                "stroke-opacity" => meaning.stroke_opacity = share().or(meaning.stroke_opacity),
                "stroke-dasharray" => meaning.dash = dashes(&lower).or(meaning.dash.take()),
                "rx" if lower == "var(--_r)" => meaning.radius = Some(Radius::Theme),
                "rx" => meaning.radius = px(written).filter(|r| !r.is_nan()).map(Radius::Px).or(meaning.radius),
                "font-size" => {
                    meaning.font_size = px(written).filter(|&size| size > 0.0 && size <= 200.0).or(meaning.font_size);
                }
                _ => {}
            }
        }
        meaning
    }
}

/// A dash pattern: lengths in pixels between spaces or commas, or `none`.
fn dashes(lower: &str) -> Option<Vec<f64>> {
    if lower == "none" {
        return Some(Vec::new());
    }
    lower
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| px(part).filter(|length| !length.is_nan()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(declarations: &[&str]) -> ResolvedStyle {
        let declarations: Vec<JsText> = declarations.iter().map(|&text| text.into()).collect();
        resolve_style(&declarations)
    }

    fn css(style: &Option<Style>) -> String {
        css_of(style.as_ref())
    }

    #[test]
    fn a_later_declaration_keeps_the_place_of_the_one_it_replaces() {
        let style = resolved(&["fill:red", "stroke: blue ", "FILL:green !important", "color:white", "x", ":y"]);
        assert_eq!(css(&style.shape), "fill:green;stroke:blue;");
        assert_eq!(css(&style.line), "stroke:blue;");
        assert_eq!(css(&style.text), "fill:white;");
        assert_eq!(style.stroke.map(|stroke| stroke.to_display()), Some("blue".to_owned()));
    }

    #[test]
    fn rounding_keeps_whether_the_list_is_terminated() {
        assert_eq!(Style::rounded(None).css(), "rx:var(--_r)");
        assert_eq!(Style::rounded(resolved(&["fill:red"]).shape.as_ref()).css(), "rx:var(--_r);fill:red;");
    }

    #[test]
    fn a_value_that_escapes_to_a_second_declaration_reads_as_pele_reads_it() {
        let style = resolved(&["stroke:a&fill:navy"]);
        assert_eq!(css(&style.shape), "stroke:a&amp;fill:navy;");
        assert!(style.shape.as_ref().is_some_and(Style::sets_fill));
        assert_eq!(ink_on(style.shape.as_ref(), None), Some(Role::OnDark));
    }

    #[test]
    fn ink_needs_a_readable_fill_that_is_mostly_opaque() {
        let ink = |declarations: &[&str]| {
            let style = resolved(declarations);
            ink_on(style.shape.as_ref(), style.text.as_ref())
        };
        assert_eq!(ink(&["fill:#000"]), Some(Role::OnDark));
        assert_eq!(ink(&["fill:#fff", "opacity:0.6"]), Some(Role::OnLight));
        assert_eq!(ink(&["fill:#fff", "opacity:0.6", "fill-opacity:50%"]), None);
        assert_eq!(ink(&["fill:#fff", "opacity:calc(1)"]), None);
        assert_eq!(ink(&["fill:#fff", "color:red"]), None);
        assert_eq!(ink(&["stroke:#fff"]), None);
        assert!(paints(resolved(&["fill:#fff", "opacity:calc(1)"]).shape.as_ref()));
        assert!(!paints(resolved(&["fill:#fff", "opacity:0"]).shape.as_ref()));
        assert!(!paints(resolved(&["fill:Transparent"]).shape.as_ref()));
        assert!(paints(resolved(&["fill:var(--x)"]).shape.as_ref()));
    }

    #[test]
    fn unsafe_values_are_dropped() {
        for bad in ["fill:url (x)", "fill:EXPRESSION(1)", "fill:a;b", "fill:java\u{7f}script", "fill:/* x", "fill:"] {
            assert_eq!(resolved(&[bad]), ResolvedStyle::default(), "{bad}");
        }
    }

    #[test]
    fn text_declarations_say_how_the_label_is_measured() {
        let style = resolved(&["font-weight:700", "font-style:italic", "font-size:20px"]);
        assert!(style.bold && style.italic);
        assert_eq!(style.font_size, Some(20.0));
        assert_eq!(resolved(&["font-size:1.5.2"]).font_size, Some(1.5));
        assert_eq!(resolved(&["font-size:201"]).font_size, None);
        assert_eq!(resolved(&["font-size:2em"]).font_size, None);
        assert!(!resolved(&["font-weight:500"]).bold);
    }

    #[test]
    fn author_classes_are_cleaned_words() {
        assert_eq!(AuthorClasses::new(&units(" a.b  c\u{e9}d internal-link ")).after("x"), "x a b c d internal-link");
        assert!(AuthorClasses::new(&units("a internal-link")).has("internal-link"));
        assert!(!AuthorClasses::new(&units("internal-links")).has("internal-link"));
        assert_eq!(AuthorClasses::new(&units(" \u{e9} ")).after("x"), "x");
    }
}
