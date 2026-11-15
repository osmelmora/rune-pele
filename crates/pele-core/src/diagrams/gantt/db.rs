//! Pele's `GanttDb`: what the parser records, and the scheduling that resolves each task's dates.

use super::dates::Unit;
use crate::input::decimal;

/// `^(\d+(?:\.\d+)?)([Mdhmswy]|ms)$` on the trimmed text: the amount and its unit, or NaN and
/// milliseconds when the text is no duration.
pub(crate) fn parse_duration(text: &[u16]) -> (f64, Unit) {
    use crate::input::Units;
    let text = text.trim_js();
    let digits = |from: usize| text[from..].iter().take_while(|&&c| (48..=57).contains(&c)).count();
    let whole = digits(0);
    let mut end = whole;
    if whole > 0 && text.get(end) == Some(&u16::from(b'.')) && digits(end + 1) > 0 {
        end += 1 + digits(end + 1);
    }
    let unit = match String::from_utf16_lossy(&text[end.min(text.len())..]).as_str() {
        _ if whole == 0 => None,
        "M" => Some(Unit::Month),
        "d" => Some(Unit::Day),
        "h" => Some(Unit::Hour),
        "m" => Some(Unit::Minute),
        "s" => Some(Unit::Second),
        "w" => Some(Unit::Week),
        "y" => Some(Unit::Year),
        "ms" => Some(Unit::Millisecond),
        _ => None,
    };
    match unit {
        Some(unit) => (decimal(&text[..end]), unit),
        None => (f64::NAN, Unit::Millisecond),
    }
}
