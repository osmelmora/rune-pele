//! Pele's `formatter` (`treemap/format.ts`): the parts of d3-format's specifier that treemap values
//! use. An optional `$`, `,` for thousands separators, a precision, `~` to trim zeros, and a type
//! of `f`, `%`, `d`, `e` or none. Fill, alignment and width are read and ignored. A specifier
//! Pele cannot read is `,`.

use crate::input::{Units, is_digit, is_line_end, string_to_number};
use crate::js;

/// What a specifier asks of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Notation {
    /// `f`: `toFixed`.
    Fixed,
    /// `%`: a hundred times the value, `toFixed`, then `%`.
    Percent,
    /// `d`: rounded to an integer.
    Integer,
    /// `e`: `toExponential`.
    Exponent,
    /// Any other type, or none: `toPrecision`, read back as a number.
    Shortest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Formatter {
    plus: bool,
    dollar: bool,
    comma: bool,
    precision: Option<usize>,
    trim: bool,
    notation: Notation,
}

/// The captures of `([-+( ])?(\$)?0?\d*(,)?(?:\.(\d+))?(~)?([a-zA-Z%])?$` matched from the start of
/// `spec`.
fn read_rest(spec: &[u16]) -> Option<Formatter> {
    let mut at = 0;
    let take = |unit: &dyn Fn(u16) -> bool, at: &mut usize| {
        let found = spec.at(*at).filter(|&c| unit(c));
        *at += usize::from(found.is_some());
        found
    };
    let sign = take(&|c| matches!(c, 45 | 43 | 40 | 32), &mut at);
    let dollar = take(&|c| c == 36, &mut at).is_some();
    take(&|c| c == 48, &mut at);
    at = spec.run(at, is_digit);
    let comma = take(&|c| c == 44, &mut at).is_some();
    let mut precision = None;
    if spec.at(at) == Some(46) && spec.at(at + 1).is_some_and(is_digit) {
        let end = spec.run(at + 1, is_digit);
        precision = Some(&spec[at + 1..end]);
        at = end;
    }
    let trim = take(&|c| c == 126, &mut at).is_some();
    let kind = take(&|c| c == 37 || crate::input::is_letter(c), &mut at);
    if at != spec.len() {
        return None;
    }
    let notation = match kind {
        Some(102) => Notation::Fixed,
        Some(37) => Notation::Percent,
        Some(100) => Notation::Integer,
        Some(101) => Notation::Exponent,
        _ => Notation::Shortest,
    };
    // `Math.min(Number(digits), 20)`
    let precision = precision.map(|digits| js::min(string_to_number(digits), 20.0) as usize);
    Some(Formatter { plus: sign == Some(43), dollar, comma, precision, trim, notation })
}

/// Pele's `RE_SPEC`: `^(?:.?[<>=^])?` and then the rest, trying the longer fill and alignment first.
fn read(spec: &[u16]) -> Option<Formatter> {
    let align = |c: Option<u16>| c.is_some_and(|c| matches!(c, 60 | 62 | 61 | 94));
    let mut skips = Vec::with_capacity(3);
    if spec.at(0).is_some_and(|c| !is_line_end(c)) && align(spec.at(1)) {
        skips.push(2);
    }
    if align(spec.at(0)) {
        skips.push(1);
    }
    skips.push(0);
    skips.into_iter().find_map(|skip| read_rest(&spec[skip..]))
}

impl Formatter {
    /// Pele's `formatter(spec)`. Mermaid documents `$0,0` for a dollar amount with thousands
    /// separators.
    pub(crate) fn new(spec: &[u16]) -> Self {
        let dollars: Vec<u16> = "$,".encode_utf16().collect();
        let spec = if spec.is_str("$0,0") { &dollars } else { spec };
        read(spec).or_else(|| read(&[44])).expect("`,` is a specifier")
    }

    pub(crate) fn format(&self, value: f64) -> String {
        let abs = value.abs();
        let mut suffix = "";
        let mut body = match self.notation {
            Notation::Fixed => js::to_fixed(abs, self.precision.unwrap_or(6)),
            Notation::Percent => {
                suffix = "%";
                js::to_fixed(abs * 100.0, self.precision.unwrap_or(6))
            }
            Notation::Integer => js::to_fixed(js::round(abs), 0),
            Notation::Exponent => js::to_exponential(abs, self.precision.unwrap_or(6)),
            Notation::Shortest => {
                let digits: Vec<u16> =
                    js::to_precision(abs, self.precision.unwrap_or(12).max(1)).encode_utf16().collect();
                js::to_string(string_to_number(&digits))
            }
        };
        if self.trim && body.contains('.') && !body.contains('e') {
            body = trim_zeros(&body);
        }
        if self.comma && !body.contains('e') {
            body = match body.find('.') {
                None => group(&body),
                Some(dot) => group(&body[..dot]) + &body[dot..],
            };
        }
        let sign = if value < 0.0 {
            "-"
        } else if self.plus {
            "+"
        } else {
            ""
        };
        format!("{sign}{}{body}{suffix}", if self.dollar { "$" } else { "" })
    }
}

/// `body.replace(/\.?0+$/, '')`
fn trim_zeros(body: &str) -> String {
    let kept = body.trim_end_matches('0');
    if kept.len() == body.len() {
        return body.to_owned();
    }
    kept.strip_suffix('.').unwrap_or(kept).to_owned()
}

/// Pele's `group`: commas between each three digits from the right. It reads code units, so
/// `Infinity` is grouped too.
fn group(digits: &str) -> String {
    let units: Vec<u16> = digits.encode_utf16().collect();
    let mut parts = Vec::new();
    let mut end = units.len();
    while end > 0 {
        let start = end.saturating_sub(3);
        parts.push(String::from_utf16_lossy(&units[start..end]));
        end = start;
    }
    parts.reverse();
    parts.join(",")
}
