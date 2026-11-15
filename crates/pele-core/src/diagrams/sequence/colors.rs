//! Whether the first word of a box title is a colour, ported from Pele's
//! `src/diagrams/sequence/colors.ts`. Mermaid asks the browser. Pele knows the colour keywords and
//! the `rgb()` and `hsl()` notations, and no other function.

use crate::input::{Units, is_digit, is_space, split_unit, to_lowercase};

const KEYWORDS: &str = "aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood \
cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray \
darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen \
darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue \
firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew \
hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan \
lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray \
lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue \
mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred \
midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid \
palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple \
rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue \
slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white \
whitesmoke yellow yellowgreen transparent currentcolor inherit initial unset revert \
accentcolor accentcolortext activetext buttonborder buttonface buttontext canvas canvastext field fieldtext \
graytext highlight highlighttext linktext mark marktext selecteditem selecteditemtext visitedtext \
activeborder activecaption appworkspace background buttonhighlight buttonshadow captiontext inactiveborder \
inactivecaption inactivecaptiontext infobackground infotext menu menutext scrollbar threeddarkshadow threedface \
threedhighlight threedlightshadow threedshadow window windowframe windowtext";

/// What one argument of a colour function is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Term {
    Bad,
    Number,
    Percent,
    Angle,
    None,
}

fn lower(c: u16) -> u16 {
    if (65..=90).contains(&c) { c + 32 } else { c }
}

fn is_word_caseless(text: &[u16], word: &str) -> bool {
    text.len() == word.len() && text.iter().zip(word.bytes()).all(|(&c, w)| lower(c) == u16::from(w))
}

/// `/^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?(%|deg|grad|rad|turn)?$/i`
fn term(text: &[u16]) -> Term {
    if is_word_caseless(text, "none") {
        return Term::None;
    }
    let is = |at: usize, c: u8| text.at(at) == Some(u16::from(c));
    let mut at = usize::from(is(0, b'+') || is(0, b'-'));
    let whole = text.run(at, is_digit);
    if whole > at {
        at = whole;
        let fraction = text.run(at + 1, is_digit);
        if is(at, b'.') && fraction > at + 1 {
            at = fraction;
        }
    } else {
        let fraction = text.run(at + 1, is_digit);
        if !is(at, b'.') || fraction == at + 1 {
            return Term::Bad;
        }
        at = fraction;
    }
    if text.at(at).map(lower) == Some(u16::from(b'e')) {
        let digits = at + 1 + usize::from(is(at + 1, b'+') || is(at + 1, b'-'));
        let exponent = text.run(digits, is_digit);
        if exponent > digits {
            at = exponent;
        }
    }
    let unit = &text[at..];
    if unit.is_empty() {
        Term::Number
    } else if unit.is_str("%") {
        Term::Percent
    } else if ["deg", "grad", "rad", "turn"].iter().any(|name| is_word_caseless(unit, name)) {
        Term::Angle
    } else {
        Term::Bad
    }
}

fn is_alpha(text: &[u16], modern: bool) -> bool {
    matches!(term(text), Term::Number | Term::Percent) || (modern && term(text) == Term::None)
}

/// `text.split(/\s+/)`
fn split_spaces(text: &[u16]) -> Vec<&[u16]> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < text.len() {
        if is_space(text[at]) {
            parts.push(&text[start..at]);
            at = text.run(at, is_space);
            start = at;
        } else {
            at += 1;
        }
    }
    parts.push(&text[start..]);
    parts
}

fn is_function(color: &[u16]) -> bool {
    let hsl = color.starts_at(0, "hsl");
    let open = color.find_unit(u16::from(b'('), 0).expect("a function has a parenthesis");
    let name = &color[..open];
    if !["rgb", "rgba", "hsl", "hsla"].iter().any(|known| name.is_str(known)) {
        return false;
    }
    if color.len() > 120 || color.find_unit(u16::from(b')'), 0) != Some(color.len() - 1) {
        return false;
    }
    let inner = color[open + 1..color.len() - 1].trim_js();
    let legacy = inner.contains(&u16::from(b','));
    let mut alpha = None;
    let mut parts: Vec<&[u16]>;
    if legacy {
        parts = split_unit(inner, u16::from(b',')).into_iter().map(|part| part.trim_js()).collect();
        if parts.len() == 4 {
            alpha = parts.pop();
        }
    } else {
        let slash = split_unit(inner, u16::from(b'/'));
        if slash.len() > 2 {
            return false;
        }
        parts = split_spaces(slash[0].trim_js());
        if slash.len() == 2 {
            alpha = Some(slash[1].trim_js());
        }
    }
    if parts.len() != 3 || alpha.is_some_and(|alpha| !is_alpha(alpha, !legacy)) {
        return false;
    }
    let kinds = [term(parts[0]), term(parts[1]), term(parts[2])];
    if legacy {
        if hsl {
            return matches!(kinds[0], Term::Number | Term::Angle)
                && kinds[1] == Term::Percent
                && kinds[2] == Term::Percent;
        }
        return matches!(kinds[0], Term::Number | Term::Percent) && kinds[1] == kinds[0] && kinds[2] == kinds[0];
    }
    kinds.iter().enumerate().all(|(i, &kind)| {
        matches!(kind, Term::Number | Term::None)
            || if hsl && i == 0 { kind == Term::Angle } else { kind == Term::Percent }
    })
}

/// Pele's `isCssColor`.
pub(super) fn is_css_color(color: &[u16]) -> bool {
    if color.contains(&u16::from(b'(')) {
        return is_function(color);
    }
    let name = String::from_utf16_lossy(&to_lowercase(color));
    KEYWORDS.split(' ').any(|keyword| keyword == name)
}
