//! What a page's CSS makes of the declarations in a `style` attribute.
//!
//! A browser parses each declaration's value by its property's grammar, and drops a declaration
//! whose value does not parse: the element keeps what it has without it. A value that leaves a
//! function, a bracket or a string open takes in the rest of the attribute, and with it every
//! declaration after it. The walk draws nothing of a declaration a page drops, as a typo or a
//! value still streaming in has it (`stroke-width:2p`, `opacity:0.`, `font-weight:bol`).
//!
//! Where browsers disagree, a value counts as one a page applies if one of them applies it, so
//! that nothing a page may draw is dropped here. So a number with no unit is a length to every
//! property, as Chrome reads it in an SVG element's `style` and Firefox does not for
//! `font-size`, and a function this does not know is left to the page.

use super::palette::{PAGE_PAINTS, css_colour};
use super::style::{Declaration, Style};

/// What a page does with one declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// The value is none the property takes, so the declaration is dropped and the element
    /// keeps what it has without it.
    Drops,
    /// The page applies the value.
    Applies,
}

/// The values a property takes.
#[derive(Clone, Copy)]
enum Grammar {
    /// `none`, a colour, or a context paint.
    Paint,
    /// A length, percentage or number, which may be negative only where `signed`, or one of
    /// `keywords`.
    Length { signed: bool, keywords: &'static [&'static str] },
    /// A number or a percentage.
    Alpha,
    /// `none`, or lengths, percentages and numbers that are not negative, in groups between
    /// commas.
    Dashes,
    /// `normal`, `bold`, `bolder`, `lighter`, or a number from 1 to 1000.
    Weight,
    /// `normal`, `italic`, or `oblique` with an angle of at most 90 degrees either way.
    Slant,
    /// Family names, each a string or words, between commas.
    Families,
    /// A line, a style, a colour and a thickness, each at most once, in any order.
    Decoration,
    /// One of these words.
    Keyword(&'static [&'static str]),
}

const FONT_SIZES: &[&str] = &[
    "xx-small",
    "x-small",
    "small",
    "medium",
    "large",
    "x-large",
    "xx-large",
    "xxx-large",
    "larger",
    "smaller",
    "math",
];

/// Each property the walk meets in a `style`, and the values a page takes for it. CSS's own
/// grammars, as Chrome and Firefox parse them.
const GRAMMARS: [(&str, Grammar); 19] = [
    ("fill", Grammar::Paint),
    ("stroke", Grammar::Paint),
    ("stroke-width", Grammar::Length { signed: false, keywords: &[] }),
    ("stroke-dashoffset", Grammar::Length { signed: true, keywords: &[] }),
    ("stroke-dasharray", Grammar::Dashes),
    ("stroke-linecap", Grammar::Keyword(&["butt", "round", "square"])),
    ("stroke-linejoin", Grammar::Keyword(&["miter", "miter-clip", "round", "bevel", "arcs"])),
    ("opacity", Grammar::Alpha),
    ("fill-opacity", Grammar::Alpha),
    ("stroke-opacity", Grammar::Alpha),
    ("rx", Grammar::Length { signed: false, keywords: &["auto"] }),
    ("ry", Grammar::Length { signed: false, keywords: &["auto"] }),
    ("font-size", Grammar::Length { signed: false, keywords: FONT_SIZES }),
    ("font-weight", Grammar::Weight),
    ("font-style", Grammar::Slant),
    ("font-family", Grammar::Families),
    ("text-decoration", Grammar::Decoration),
    ("letter-spacing", Grammar::Length { signed: true, keywords: &["normal"] }),
    ("word-spacing", Grammar::Length { signed: true, keywords: &["normal"] }),
];

const WIDE_KEYWORDS: [&str; 5] = ["inherit", "initial", "unset", "revert", "revert-layer"];

/// Functions whose arguments are checked: CSS's maths.
const MATHS: [&str; 20] = [
    "calc", "min", "max", "clamp", "round", "mod", "rem", "abs", "sign", "sin", "cos", "tan", "asin", "acos", "atan",
    "atan2", "pow", "sqrt", "hypot", "log",
];

/// Functions that make any value one a page takes when it parses it: it is only checked once
/// what they stand for is known.
const SUBSTITUTIONS: [&str; 3] = ["var", "env", "attr"];

/// Words maths takes: its constants, and how `round()` rounds.
const MATHS_WORDS: [&str; 9] = ["e", "pi", "infinity", "-infinity", "nan", "nearest", "up", "down", "to-zero"];

const LENGTH_UNITS: [&str; 51] = [
    "px", "cm", "mm", "q", "in", "pt", "pc", "em", "rem", "ex", "rex", "cap", "rcap", "ch", "rch", "ic", "ric", "lh",
    "rlh", "vw", "vh", "vi", "vb", "vmin", "vmax", "svw", "svh", "svi", "svb", "svmin", "svmax", "lvw", "lvh", "lvi",
    "lvb", "lvmin", "lvmax", "dvw", "dvh", "dvi", "dvb", "dvmin", "dvmax", "cqw", "cqh", "cqi", "cqb", "cqmin",
    "cqmax", "x", "dppx",
];

/// Each angle unit, and how many degrees one is.
const ANGLE_UNITS: [(&str, f64); 4] =
    [("deg", 1.0), ("grad", 0.9), ("rad", 180.0 / std::f64::consts::PI), ("turn", 360.0)];

const OTHER_UNITS: [&str; 5] = ["s", "ms", "hz", "khz", "dpi"];

impl Style {
    /// Each declaration with what a page does with it, in order.
    pub(crate) fn on_a_page(&self) -> impl Iterator<Item = (&Declaration, Page)> {
        let last = self.declarations.len().saturating_sub(1);
        let mut swallowed = false;
        self.declarations.iter().enumerate().map(move |(index, declaration)| {
            if swallowed {
                return (declaration, Page::Drops);
            }
            let text = unescaped(&declaration.written).to_ascii_lowercase();
            let value = tokens(text.trim_matches(is_white));
            // Something follows in the attribute: the next declaration, or the `;` that ends it.
            let followed = index < last || self.terminated;
            swallowed = followed && value.open.is_some();
            let page = if followed && value.open == Some(Opened::Block) {
                // The `;` it takes in is in its function or bracket, which none of these takes.
                Page::Drops
            } else {
                page(&declaration.property, &value.tokens)
            };
            (declaration, page)
        })
    }
}

/// What a page does with `property:value`, given the value's tokens.
fn page(property: &str, tokens: &[Token<'_>]) -> Page {
    let Some(&(_, grammar)) = GRAMMARS.iter().find(|(name, _)| *name == property) else {
        return Page::Applies;
    };
    let wide = matches!(tokens, [Token::Ident(word)] if WIDE_KEYWORDS.contains(word));
    if wide || substitutes(tokens) || takes(grammar, tokens) { Page::Applies } else { Page::Drops }
}

fn takes(grammar: Grammar, tokens: &[Token<'_>]) -> bool {
    match (grammar, tokens) {
        (Grammar::Paint, [Token::Ident("none" | "context-fill" | "context-stroke")]) => true,
        (Grammar::Paint, [one]) => colour(*one),
        (Grammar::Paint, [Token::Function("url", _), fallback]) => {
            matches!(fallback, Token::Ident("none")) || colour(*fallback)
        }
        (Grammar::Length { keywords, .. }, [Token::Ident(word)]) => keywords.contains(word),
        (Grammar::Length { signed, .. }, [one]) => length(*one, signed),
        (Grammar::Alpha, [Token::Number(_) | Token::Percentage(_)]) => true,
        (Grammar::Alpha, [Token::Function(name, arguments)]) => function(name, arguments),
        (Grammar::Dashes, [Token::Ident("none")]) => true,
        (Grammar::Dashes, _) => {
            groups(tokens).all(|group| !group.is_empty() && group.iter().all(|&t| length(t, false)))
        }
        (Grammar::Weight, [Token::Ident("normal" | "bold" | "bolder" | "lighter")]) => true,
        (Grammar::Weight, [Token::Number(weight)]) => (1.0..=1000.0).contains(weight),
        (Grammar::Weight, [Token::Function(name, arguments)]) => function(name, arguments),
        (Grammar::Slant, [Token::Ident("normal" | "italic" | "oblique")]) => true,
        (Grammar::Slant, [Token::Ident("oblique"), Token::Dimension(angle, unit)]) => {
            degrees(*angle, unit).is_some_and(|degrees| degrees.abs() <= 90.0)
        }
        (Grammar::Slant, [Token::Ident("oblique"), Token::Function(name, arguments)]) => function(name, arguments),
        (Grammar::Families, _) => groups(tokens).all(family),
        (Grammar::Decoration, _) => decoration(tokens),
        (Grammar::Keyword(words), [Token::Ident(word)]) => words.contains(word),
        _ => false,
    }
}

/// Whether a value holds `var()` or another function that stands for a value only a page
/// knows. A page takes the value at parse time, whatever its grammar.
fn substitutes(tokens: &[Token<'_>]) -> bool {
    tokens.iter().any(|token| match token {
        Token::Function(name, _) if SUBSTITUTIONS.contains(name) => true,
        Token::Function(_, inside) | Token::Block(inside) => substitutes(&tokens_of(inside)),
        _ => false,
    })
}

fn colour(token: Token<'_>) -> bool {
    match token {
        Token::Ident(word) => PAGE_PAINTS.split(' ').any(|paint| paint == word) || readable(word),
        Token::Hash(digits) => [3, 4, 6, 8].contains(&digits.len()) && digits.bytes().all(|c| c.is_ascii_hexdigit()),
        // A colour function this does not read may be one a page does.
        Token::Function(..) => true,
        _ => false,
    }
}

fn readable(word: &str) -> bool {
    css_colour(&word.encode_utf16().collect::<Vec<_>>(), false).is_some()
}

fn length(token: Token<'_>, signed: bool) -> bool {
    match token {
        Token::Number(n) | Token::Percentage(n) => signed || n >= 0.0,
        Token::Dimension(n, unit) => LENGTH_UNITS.contains(&unit) && (signed || n >= 0.0),
        Token::Function(name, arguments) => function(name, arguments),
        _ => false,
    }
}

fn degrees(angle: f64, unit: &str) -> Option<f64> {
    ANGLE_UNITS.iter().find(|(name, _)| *name == unit).map(|(_, degrees)| angle * degrees)
}

/// A function in a place for a number, length or angle: maths whose terms are numbers and
/// dimensions CSS knows, or a function this leaves to the page.
fn function(name: &str, arguments: &str) -> bool {
    !MATHS.contains(&name) || maths(arguments)
}

fn maths(arguments: &str) -> bool {
    let value = tokens(arguments);
    !value.tokens.is_empty()
        && value.open.is_none()
        && value.tokens.iter().all(|&token| match token {
            Token::Number(_) | Token::Percentage(_) | Token::Comma | Token::Delim('+' | '-' | '*' | '/') => true,
            Token::Dimension(_, unit) => {
                LENGTH_UNITS.contains(&unit) || degrees(1.0, unit).is_some() || OTHER_UNITS.contains(&unit)
            }
            Token::Ident(word) => MATHS_WORDS.contains(&word),
            Token::Function(name, arguments) => function(name, arguments),
            Token::Block(inside) => maths(inside),
            _ => false,
        })
}

/// The tokens between commas.
fn groups<'t, 'a>(tokens: &'t [Token<'a>]) -> impl Iterator<Item = &'t [Token<'a>]> {
    tokens.split(|&token| token == Token::Comma)
}

/// One family: a string, or one or more words, or a function such as `generic()`.
fn family(group: &[Token<'_>]) -> bool {
    match group {
        [Token::String] | [Token::Function(..)] => true,
        words => !words.is_empty() && words.iter().all(|token| matches!(token, Token::Ident(_))),
    }
}

const LINES: [&str; 6] = ["underline", "overline", "line-through", "blink", "spelling-error", "grammar-error"];
const LINE_STYLES: [&str; 5] = ["solid", "double", "dotted", "dashed", "wavy"];

/// `text-decoration`: its line (`none`, or line words in a row, each once), its style, its
/// colour and its thickness, each at most once.
fn decoration(tokens: &[Token<'_>]) -> bool {
    let (mut line, mut style, mut colour_given, mut thickness) = (false, false, false, false);
    let mut at = 0;
    while at < tokens.len() {
        let token = tokens[at];
        let once = |given: &mut bool| !std::mem::replace(given, true);
        let ok = match token {
            Token::Ident("none") => once(&mut line),
            Token::Ident(word) if LINES.contains(&word) => {
                let run: Vec<&str> = tokens[at..]
                    .iter()
                    .map_while(|token| match token {
                        Token::Ident(word) if LINES.contains(word) => Some(*word),
                        _ => None,
                    })
                    .collect();
                at += run.len() - 1;
                run.iter().enumerate().all(|(index, word)| !run[..index].contains(word)) && once(&mut line)
            }
            Token::Ident(word) if LINE_STYLES.contains(&word) => once(&mut style),
            Token::Ident("auto" | "from-font") => once(&mut thickness),
            Token::Number(_) | Token::Percentage(_) | Token::Dimension(..) if length(token, true) => {
                once(&mut thickness)
            }
            Token::Function(name, arguments) if MATHS.contains(&name) => maths(arguments) && once(&mut thickness),
            _ if colour(token) => once(&mut colour_given),
            _ => false,
        };
        if !ok {
            return false;
        }
        at += 1;
    }
    at > 0
}

/// The attribute's text as a page reads it: Pele escapes `&`, `<`, `>` and `"` for XML.
fn unescaped(written: &str) -> String {
    written.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// One of CSS's tokens at the top level of a value. Text is in lowercase.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Token<'a> {
    Ident(&'a str),
    Number(f64),
    Percentage(f64),
    Dimension(f64, &'a str),
    /// What follows a `#`.
    Hash(&'a str),
    /// A function by its name, and what is between its parentheses.
    Function(&'a str, &'a str),
    /// What is between a `(` and its `)`, or a `[` and its `]`.
    Block(&'a str),
    String,
    Comma,
    Delim(char),
}

/// What is left open where a value ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opened {
    /// A function or a bracket.
    Block,
    String,
}

/// A value's tokens, each open one read as though the value's end closed it, as the end of the
/// attribute does.
struct Value<'a> {
    tokens: Vec<Token<'a>>,
    open: Option<Opened>,
}

fn tokens_of(text: &str) -> Vec<Token<'_>> {
    tokens(text).tokens
}

fn is_white(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

fn tokens(text: &str) -> Value<'_> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while let Some(c) = text[at..].chars().next() {
        if is_white(c) {
            at += c.len_utf8();
            continue;
        }
        let rest = &text[at..];
        if c == '"' || c == '\'' {
            tokens.push(Token::String);
            match rest[1..].find(c) {
                Some(end) => at += end + 2,
                None => return Value { tokens, open: Some(Opened::String) },
            }
        } else if let Some(end) = number_length(rest) {
            let number: f64 = rest[..end].parse().expect("a CSS number is a Rust one");
            let after = &rest[end..];
            let unit = ident_length(after);
            if after.starts_with('%') {
                tokens.push(Token::Percentage(number));
                at += end + 1;
            } else if unit > 0 {
                tokens.push(Token::Dimension(number, &after[..unit]));
                at += end + unit;
            } else {
                tokens.push(Token::Number(number));
                at += end;
            }
        } else if ident_length(rest) > 0 {
            let name = ident_length(rest);
            if rest[name..].starts_with('(') {
                let (inside, length, open) = block(&rest[name..]);
                tokens.push(Token::Function(&rest[..name], inside));
                if open.is_some() {
                    return Value { tokens, open: Some(Opened::Block) };
                }
                at += name + length;
            } else {
                tokens.push(Token::Ident(&rest[..name]));
                at += name;
            }
        } else if c == '#' && name_length(&rest[1..]) > 0 {
            let digits = name_length(&rest[1..]);
            tokens.push(Token::Hash(&rest[1..1 + digits]));
            at += 1 + digits;
        } else if c == '(' || c == '[' {
            let (inside, length, open) = block(rest);
            tokens.push(Token::Block(inside));
            if open.is_some() {
                return Value { tokens, open: Some(Opened::Block) };
            }
            at += length;
        } else {
            tokens.push(if c == ',' { Token::Comma } else { Token::Delim(c) });
            at += c.len_utf8();
        }
    }
    Value { tokens, open: None }
}

/// The bracket that `text` opens: what is inside it, how long it is with its closing bracket,
/// and what is still open where `text` ends.
fn block(text: &str) -> (&str, usize, Option<Opened>) {
    let mut closers = Vec::new();
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => closers.push(')'),
            (None, '[') => closers.push(']'),
            (None, _) if closers.last() == Some(&c) => {
                closers.pop();
                if closers.is_empty() {
                    return (&text[1..at], at + 1, None);
                }
            }
            _ => {}
        }
    }
    (&text[1..], text.len(), Some(Opened::Block))
}

/// How long the CSS number at the start of `text` is: `[+-]?(\d+(\.\d+)?|\.\d+)([eE][+-]?\d+)?`.
fn number_length(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let digits = |from: usize| bytes[from.min(bytes.len())..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let whole = digits(at);
    at += whole;
    if bytes.get(at) == Some(&b'.') && digits(at + 1) > 0 {
        at += 1 + digits(at + 1);
    } else if whole == 0 {
        return None;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(at + 1), Some(b'+' | b'-')));
        let exponent = digits(at + 1 + sign);
        if exponent > 0 {
            at += 1 + sign + exponent;
        }
    }
    Some(at)
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_name(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '-'
}

fn name_length(text: &str) -> usize {
    text.find(|c: char| !is_name(c)).unwrap_or(text.len())
}

/// How long the identifier at the start of `text` is, or 0.
fn ident_length(text: &str) -> usize {
    let mut chars = text.chars();
    let starts = match (chars.next(), chars.next()) {
        (Some('-'), Some(second)) => second == '-' || is_name_start(second),
        (Some(first), _) => is_name_start(first),
        _ => false,
    };
    if starts { name_length(text) } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on_a_page(property: &str, value: &str) -> Page {
        let mut style = Style::terminated();
        style.declare(property, value);
        style.on_a_page().next().expect("one declaration").1
    }

    /// Each property with values a page applies and values it drops, from CSS's grammars.
    const CASES: [(&str, &[&str], &[&str]); 19] = [
        (
            "fill",
            &[
                "#036",
                "#0000",
                "#0369",
                "red",
                "Lime",
                "Grey",
                "none",
                "transparent",
                "currentColor",
                "inherit",
                "Canvas",
                "context-fill",
                "rgb(1 2 3)",
                "rgb(1 2)",
                "color-mix(in srgb, red, blue)",
                "var(--x)",
                "url(#a) red",
            ],
            &[
                "#",
                "#0",
                "#00",
                "#03",
                "#00000",
                "#0000000",
                "#12345g",
                "blu",
                " Blu ",
                "red red",
                "0",
                "\"red\"",
                "rgb(1 2 3",
                "red)",
            ],
        ),
        ("stroke", &["#f96", "transparent", "light-dark(red, blue)"], &["#f9", "f96", "1px"]),
        (
            "stroke-width",
            &["2", "2px", ".5", "1e1px", "+2px", "2PX", "2pt", "2em", "50%", "calc(2px + 3px)", "calc(2em + 3px)"],
            &["2p", "2e", "2.", "1.5.2", "-1px", "px", "thick", "calc(2p + 3px)", "calc(", "calc(2px", "2px 3px"],
        ),
        ("stroke-dashoffset", &["900", "-5px", "0"], &["9p", "a", "5 5"]),
        (
            "stroke-dasharray",
            &["5 5", "9", "none", "5,5", "5 , 5", "5px 2em", "0"],
            &["5,", ",5", "5,,5", "-5", "5 -5", "a", "none 5", "5p"],
        ),
        ("stroke-linecap", &["butt", "round", "square", "ROUND"], &["b", "rou", "squar", "butt round"]),
        ("stroke-linejoin", &["miter", "round", "bevel", "miter-clip", "arcs"], &["mit", "miter-", "bev"]),
        (
            "opacity",
            &["0", "0.5", ".5", "50%", "1e-1", "2", "-1", "calc(1 / 2)"],
            &["0.", "0.5.", "a", "0.5px", "50 %"],
        ),
        ("fill-opacity", &["1"], &["1."]),
        ("stroke-opacity", &["0.25"], &["0.2.5"]),
        (
            "rx",
            &["5", "5px", "auto", "var(--_r)", "min(var(--_r),4px)", "10%"],
            &["5p", "-5", "au", "min(var(--_r),4px"],
        ),
        ("ry", &["5px", "auto"], &["5p", "aut"]),
        (
            "font-size",
            &["20px", "20", "2em", "large", "smaller", "0", "250px", "120%"],
            &["20p", "2e", "-2px", "larg", "20px 2px"],
        ),
        (
            "font-weight",
            &["bold", "Bolder", "lighter", "normal", "600", "1", "1000", "700.5", "7e2"],
            &["b", "bo", "bol", "0", "1001", "700.", "6 00", "bold bold"],
        ),
        (
            "font-style",
            &["italic", "normal", "oblique", "oblique 10deg", "oblique -90deg"],
            &["i", "itali", "oblique 91deg", "obliqu"],
        ),
        (
            "font-family",
            &["monospace", "m", "monospac", "Arial", "\"Times New Roman\", serif", "Source Code Pro", "\"Ari"],
            &["1a", "a,", ",a", "a,,b", "#a"],
        ),
        (
            "text-decoration",
            &[
                "underline",
                "none",
                "line-through",
                "underline overline",
                "underline red",
                "red",
                "wavy underline 2px",
                "underline auto",
            ],
            &[
                "u",
                "underlin",
                "l",
                "line-",
                "line-throug",
                "underline underline",
                "none underline",
                "underline red overline",
                "red blue",
            ],
        ),
        ("letter-spacing", &["normal", "0", "0px", "2px", "-1px", "2"], &["norma", "2p", "0."]),
        ("word-spacing", &["normal", "1em"], &["n", "1e"]),
    ];

    #[test]
    fn a_page_drops_a_value_its_property_does_not_take() {
        assert_eq!(CASES.len(), GRAMMARS.len(), "a case for every property");
        for (property, applied, dropped) in CASES {
            for value in applied {
                assert_eq!(on_a_page(property, value), Page::Applies, "{property}:{value}");
            }
            for value in dropped {
                assert_eq!(on_a_page(property, value), Page::Drops, "{property}:{value}");
            }
        }
    }

    #[test]
    fn a_wide_keyword_or_a_substitution_is_left_to_the_page() {
        for value in ["inherit", "initial", "unset", "revert", "revert-layer", "var(--x)", "calc(var(--x) * 2)"] {
            for property in ["stroke-width", "font-weight", "text-decoration", "stroke-dasharray"] {
                assert_eq!(on_a_page(property, value), Page::Applies, "{property}:{value}");
            }
        }
        assert_eq!(on_a_page("cursor", "pointer"), Page::Applies, "a property this does not know");
    }

    #[test]
    fn a_value_left_open_takes_in_the_declarations_after_it() {
        let pages = |declarations: &[(&str, &str)], terminated: bool| -> Vec<Page> {
            let mut style = if terminated { Style::terminated() } else { Style::unterminated() };
            for (property, value) in declarations {
                style.declare(property, *value);
            }
            style.on_a_page().map(|(_, page)| page).collect()
        };
        let open = [("fill", "rgb(1 2 3"), ("stroke", "red")];
        assert_eq!(pages(&open, true), [Page::Drops, Page::Drops], "a function takes in the next declaration");
        assert_eq!(pages(&open[..1], true), [Page::Drops], "and the `;` that ends the list");
        assert_eq!(pages(&open[..1], false), [Page::Applies], "the end of the attribute closes it");
        let quoted = [("font-family", "\"Ari"), ("stroke", "red")];
        assert_eq!(pages(&quoted, true), [Page::Applies, Page::Drops], "a string takes in the rest");
        assert_eq!(pages(&[("fill", "\"red"), ("stroke", "red")], true), [Page::Drops, Page::Drops]);
    }

    #[test]
    fn a_value_is_read_as_the_page_reads_it_unescaped() {
        assert_eq!(on_a_page("font-family", "\"Ari\""), Page::Applies);
        assert_eq!(on_a_page("stroke", "a&fill:navy"), Page::Drops);
    }
}
