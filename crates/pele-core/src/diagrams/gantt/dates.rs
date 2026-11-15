//! Pele's `dates.ts`: the parts of dayjs 1.11 Mermaid's gantt code relies on, with the plugins it
//! loads and the English locale. A date is accepted only when formatting what was read gives back
//! the input, so the quirks of reading and of writing are both kept.

use super::date::JsDate;
use crate::input::{is_digit, is_space, string_to_number};
use crate::js;

pub(crate) const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
pub(crate) const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// A unit a duration counts in: Pele's `Unit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl Unit {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Unit::Millisecond => "ms",
            Unit::Second => "s",
            Unit::Minute => "m",
            Unit::Hour => "h",
            Unit::Day => "d",
            Unit::Week => "w",
            Unit::Month => "M",
            Unit::Year => "y",
        }
    }
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// `String(value)` with zeros in front up to `length` units, a minus sign counted as one.
fn pad(value: f64, length: usize) -> Vec<u16> {
    let text = units(&js::to_string(value));
    let mut out = vec![u16::from(b'0'); length.saturating_sub(text.len())];
    out.extend(text);
    out
}

fn ordinal(n: f64) -> String {
    // `s[(v - 20) % 10] || s[v] || s[0]` with `s = ['th', 'st', 'nd', 'rd']`.
    let v = n % 100.0;
    let pick = |i: f64| [1.0, 2.0, 3.0].iter().position(|&k| k == i).map(|k| ["st", "nd", "rd"][k]);
    let suffix = pick((v - 20.0) % 10.0).or_else(|| pick(v)).unwrap_or("th");
    format!("{}{suffix}", js::to_string(n))
}

pub(crate) fn add_days(date: JsDate, days: f64) -> JsDate {
    let mut next = date;
    next.set_date(next.date() + days);
    next
}

fn days_in_month(date: JsDate) -> f64 {
    let mut last = JsDate::local(date.full_year(), date.month() + 1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    last.set_hours(23.0, Some(59.0), Some(59.0), Some(999.0));
    last.date()
}

/// Sets the month or the year, keeping the day of the month unless the new month is shorter.
fn set_calendar(date: JsDate, apply: impl FnOnce(&mut JsDate)) -> JsDate {
    let mut next = date;
    next.set_date(1.0);
    apply(&mut next);
    next.set_date(js::min(date.date(), days_in_month(next)));
    next
}

pub(crate) fn add(date: JsDate, amount: f64, unit: Unit) -> JsDate {
    let by = |ms: f64| JsDate::from_time(date.time() + amount * ms);
    match unit {
        Unit::Month => set_calendar(date, |d| d.set_month(date.month() + amount, None)),
        Unit::Year => set_calendar(date, |d| d.set_full_year(date.full_year() + amount, None, None)),
        Unit::Day => add_days(date, js::round(amount)),
        Unit::Week => add_days(date, js::round(7.0 * amount)),
        Unit::Hour => by(3_600_000.0),
        Unit::Minute => by(60_000.0),
        Unit::Second => by(1000.0),
        Unit::Millisecond => JsDate::from_time(date.time() + amount),
    }
}

fn iso_weekday(date: JsDate) -> f64 {
    let day = date.day();
    if day == 0.0 { 7.0 } else { day }
}

fn week_thursday(date: JsDate) -> JsDate {
    add_days(date, 4.0 - iso_weekday(date))
}

fn iso_week(date: JsDate) -> f64 {
    let thursday = week_thursday(date);
    let jan1 = JsDate::local(thursday.full_year(), 0.0, 1.0, 0.0, 0.0, 0.0, 0.0);
    let weekday = iso_weekday(jan1);
    let first = add_days(jan1, 4.0 - weekday + if weekday > 4.0 { 7.0 } else { 0.0 });
    let weeks = (thursday.time() - first.time()) / 604_800_000.0;
    let whole = if weeks < 0.0 { weeks.ceil() + 0.0 } else { weeks.floor() };
    whole + 1.0
}

/// One match of a token list at `at`, as the end of the match.
type Matcher = fn(&[u16], usize) -> Option<usize>;

/// `text.replace(re, f)` for a global `re` that never matches nothing.
fn replace_all(text: &[u16], matcher: Matcher, mut replace: impl FnMut(&[u16]) -> Vec<u16>) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        match matcher(text, at) {
            Some(end) => {
                out.extend(replace(&text[at..end]));
                at = end;
            }
            None => {
                out.push(text[at]);
                at += 1;
            }
        }
    }
    out
}

/// `\[([^\]]+)]`.
fn bracketed(text: &[u16], at: usize) -> Option<usize> {
    if text.get(at) != Some(&u16::from(b'[')) {
        return None;
    }
    let close = text[at + 1..].iter().position(|&c| c == u16::from(b']'))?;
    (close > 0).then_some(at + close + 2)
}

/// Up to `most` of `letter` at `at`, at least one.
fn repeated(text: &[u16], at: usize, letter: u8, most: usize) -> Option<usize> {
    let count = text[at..].iter().take(most).take_while(|&&c| c == u16::from(letter)).count();
    (count > 0).then_some(at + count)
}

fn literal(text: &[u16], at: usize, word: &str) -> Option<usize> {
    let end = at + word.len();
    (text.get(at..end)? == units(word).as_slice()).then_some(end)
}

/// `RE_ADVANCED`.
fn advanced_token(text: &[u16], at: usize) -> Option<usize> {
    if let Some(end) = bracketed(text, at) {
        return Some(end);
    }
    for word in ["Q", "wo", "ww", "w", "WW", "W", "zzz", "z", "gggg", "GGGG", "Do", "X", "x"] {
        if let Some(end) = literal(text, at, word) {
            return Some(end);
        }
    }
    repeated(text, at, b'k', 2).or_else(|| literal(text, at, "S"))
}

/// `RE_FORMAT`.
fn format_token(text: &[u16], at: usize) -> Option<usize> {
    bracketed(text, at)
        .or_else(|| literal(text, at, "YYYY"))
        .or_else(|| literal(text, at, "YY"))
        .or_else(|| repeated(text, at, b'M', 4))
        .or_else(|| repeated(text, at, b'D', 2))
        .or_else(|| repeated(text, at, b'd', 4))
        .or_else(|| repeated(text, at, b'H', 2))
        .or_else(|| repeated(text, at, b'h', 2))
        .or_else(|| literal(text, at, "a"))
        .or_else(|| literal(text, at, "A"))
        .or_else(|| repeated(text, at, b'm', 2))
        .or_else(|| repeated(text, at, b's', 2))
        .or_else(|| repeated(text, at, b'Z', 2))
        .or_else(|| literal(text, at, "SSS"))
}

/// The offset from UTC as `+HH:mm`, which is always `+00:00` in UTC.
fn zone(date: JsDate) -> String {
    let offset = -js::round(date.timezone_offset() / 15.0) * 15.0;
    let offset = if offset == 0.0 { 0.0 } else { offset };
    let minutes = offset.abs();
    let two = |value: f64| String::from_utf16_lossy(&pad(value, 2));
    format!("{}{}:{}", if offset >= 0.0 { '+' } else { '-' }, two((minutes / 60.0).floor()), two(minutes % 60.0))
}

const DEFAULT_FORMAT: &str = "YYYY-MM-DDTHH:mm:ssZ";

/// Pele's `format`: `None` for the week-of-year and time zone name tokens, which need dayjs
/// plugins Mermaid does not load.
pub(crate) fn format(date: JsDate, template: &[u16]) -> Option<Vec<u16>> {
    if !date.is_valid() {
        return Some(units("Invalid Date"));
    }
    let default = units(DEFAULT_FORMAT);
    let template = if template.is_empty() { default.as_slice() } else { template };
    if template == units("YYYY-MM-DD").as_slice() {
        return Some(iso_date(date));
    }
    let mut unsupported = false;
    let advanced = replace_all(template, advanced_token, |token| {
        let text = String::from_utf16_lossy(token);
        let number = |value: f64| units(&js::to_string(value));
        match text.as_str() {
            "Q" => number(((date.month() + 1.0) / 3.0).ceil()),
            "Do" => units(&format!("[{}]", ordinal(date.date()))),
            "GGGG" => number(week_thursday(date).full_year()),
            "W" => number(iso_week(date)),
            "WW" => pad(iso_week(date), 2),
            "k" => number(if date.hours() == 0.0 { 24.0 } else { date.hours() }),
            "kk" => pad(if date.hours() == 0.0 { 24.0 } else { date.hours() }, 2),
            "X" => number((date.time() / 1000.0).floor()),
            "x" => number(date.time()),
            "gggg" | "wo" | "w" | "ww" | "z" | "zzz" => {
                unsupported = true;
                token.to_vec()
            }
            _ => token.to_vec(),
        }
    });
    if unsupported {
        return None;
    }
    let hours = date.hours();
    let twelve = if hours % 12.0 == 0.0 { 12.0 } else { hours % 12.0 };
    Some(replace_all(&advanced, format_token, |token| {
        if token[0] == u16::from(b'[') {
            return token[1..token.len() - 1].to_vec();
        }
        let number = |value: f64| units(&js::to_string(value));
        let month = date.month() as usize;
        let weekday = WEEKDAYS[date.day() as usize];
        match String::from_utf16_lossy(token).as_str() {
            "YY" => {
                let year = units(&js::to_string(date.full_year()));
                year[year.len().saturating_sub(2)..].to_vec()
            }
            "YYYY" => pad(date.full_year(), 4),
            "M" => number(date.month() + 1.0),
            "MM" => pad(date.month() + 1.0, 2),
            "MMM" => units(&MONTHS[month][..3]),
            "MMMM" => units(MONTHS[month]),
            "D" => number(date.date()),
            "DD" => pad(date.date(), 2),
            "d" => number(date.day()),
            "dd" => units(&weekday[..2]),
            "ddd" => units(&weekday[..3]),
            "dddd" => units(weekday),
            "H" => number(hours),
            "HH" => pad(hours, 2),
            "h" => number(twelve),
            "hh" => pad(twelve, 2),
            "a" => units(if hours < 12.0 { "am" } else { "pm" }),
            "A" => units(if hours < 12.0 { "AM" } else { "PM" }),
            "m" => number(date.minutes()),
            "mm" => pad(date.minutes(), 2),
            "s" => number(date.seconds()),
            "ss" => pad(date.seconds(), 2),
            "SSS" => pad(date.milliseconds(), 3),
            "Z" => units(&zone(date)),
            _ => units(&zone(date).replacen(':', "", 1)),
        }
    }))
}

/// Pele's `isoDate`: `YYYY-MM-DD`, with a year outside four digits padded as `String` writes it.
pub(crate) fn iso_date(date: JsDate) -> Vec<u16> {
    let (year, month, day) = (date.full_year(), date.month() + 1.0, date.date());
    let mut out = if (1000.0..=9999.0).contains(&year) { units(&js::to_string(year)) } else { pad(year, 4) };
    out.extend(units(if month < 10.0 { "-0" } else { "-" }));
    out.extend(units(&js::to_string(month)));
    out.extend(units(if day < 10.0 { "-0" } else { "-" }));
    out.extend(units(&js::to_string(day)));
    out
}

/// What a reader looks for in the rest of the input: Pele's `MATCH_*` patterns, which are not
/// anchored, so each finds its leftmost match.
#[derive(Clone, Copy, Debug)]
enum Pattern {
    /// `\d{n}`
    Digits(usize),
    /// `\d\d?`
    OneOrTwo,
    /// `[+-]?\d+`
    Signed,
    /// `[+-]\d\d:?(\d\d)?|Z`
    Offset,
    /// `\d*[^-_:/,()\s\d]+`
    Word,
}

fn digit_at(text: &[u16], at: usize) -> bool {
    text.get(at).copied().is_some_and(is_digit)
}

fn digit_run(text: &[u16], at: usize) -> usize {
    at + text[at..].iter().take_while(|&&c| is_digit(c)).count()
}

fn is_sign(c: u16) -> bool {
    c == u16::from(b'+') || c == u16::from(b'-')
}

impl Pattern {
    /// The end of a match that starts at `at`.
    fn at(self, text: &[u16], at: usize) -> Option<usize> {
        match self {
            Pattern::Digits(n) => (0..n).all(|i| digit_at(text, at + i)).then_some(at + n),
            Pattern::OneOrTwo => digit_at(text, at).then(|| if digit_at(text, at + 1) { at + 2 } else { at + 1 }),
            Pattern::Signed => {
                let from =
                    if text.get(at).copied().is_some_and(is_sign) && digit_at(text, at + 1) { at + 1 } else { at };
                digit_at(text, from).then(|| digit_run(text, from))
            }
            Pattern::Offset => {
                if text.get(at).copied().is_some_and(is_sign) && digit_at(text, at + 1) && digit_at(text, at + 2) {
                    let mut end = at + 3;
                    if text.get(end) == Some(&u16::from(b':')) {
                        end += 1;
                    }
                    if digit_at(text, end) && digit_at(text, end + 1) {
                        end += 2;
                    }
                    Some(end)
                } else {
                    (text.get(at) == Some(&u16::from(b'Z'))).then_some(at + 1)
                }
            }
            Pattern::Word => {
                let word = |c: u16| !is_digit(c) && !is_space(c) && !units("-_:/,()").contains(&c);
                let from = digit_run(text, at);
                let end = from + text[from..].iter().take_while(|&&c| word(c)).count();
                (end > from).then_some(end)
            }
        }
    }

    /// The leftmost match.
    fn find(self, text: &[u16]) -> Option<(usize, usize)> {
        let mut at = 0;
        while at < text.len() {
            if let Some(end) = self.at(text, at) {
                return Some((at, end));
            }
            // A word that cannot start at `at` cannot start anywhere in the digits after it.
            at = if matches!(self, Pattern::Word) { digit_run(text, at) + 1 } else { at + 1 };
        }
        None
    }
}

/// A field a reader sets.
#[derive(Clone, Copy, Debug)]
enum Field {
    Seconds,
    Minutes,
    Hours,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Clone, Copy, Debug)]
enum Read {
    /// `A` or `a`: afternoon when the input is this word.
    Afternoon(&'static str),
    Quarter,
    /// Milliseconds as the input times this.
    Milliseconds(f64),
    Field(Field),
    /// `Do`: a day with its ordinal suffix.
    Ordinal,
    /// A month by its name, short or long.
    MonthName(bool),
    /// `YY`: a year in the 1900s past 68, otherwise in the 2000s.
    ShortYear,
    Offset,
}

/// One step of a compiled format: Pele's `Step`.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Text of this many units, which reading skips.
    Literal(usize),
    Read(Pattern, Read),
}

fn reader(token: &str) -> Option<(Pattern, Read)> {
    use Pattern::{Digits, OneOrTwo, Signed, Word};
    Some(match token {
        "A" => (Word, Read::Afternoon("PM")),
        "a" => (Word, Read::Afternoon("pm")),
        "Q" => (Digits(1), Read::Quarter),
        "S" => (Digits(1), Read::Milliseconds(100.0)),
        "SS" => (Digits(2), Read::Milliseconds(10.0)),
        "SSS" => (Digits(3), Read::Milliseconds(1.0)),
        "s" | "ss" => (OneOrTwo, Read::Field(Field::Seconds)),
        "m" | "mm" => (OneOrTwo, Read::Field(Field::Minutes)),
        "H" | "h" | "HH" | "hh" => (OneOrTwo, Read::Field(Field::Hours)),
        "D" => (OneOrTwo, Read::Field(Field::Day)),
        "DD" => (Digits(2), Read::Field(Field::Day)),
        "Do" => (Word, Read::Ordinal),
        "w" => (OneOrTwo, Read::Field(Field::Week)),
        "ww" => (Digits(2), Read::Field(Field::Week)),
        "M" => (OneOrTwo, Read::Field(Field::Month)),
        "MM" => (Digits(2), Read::Field(Field::Month)),
        "MMM" => (Word, Read::MonthName(true)),
        "MMMM" => (Word, Read::MonthName(false)),
        "Y" => (Signed, Read::Field(Field::Year)),
        "YY" => (Digits(2), Read::ShortYear),
        "YYYY" => (Digits(4), Read::Field(Field::Year)),
        "Z" | "ZZ" => (Pattern::Offset, Read::Offset),
        _ => return None,
    })
}

/// `RE_TOKENS`: a literal in brackets, a run of separators, or a format token.
fn format_part(text: &[u16], at: usize) -> Option<usize> {
    if text[at] == u16::from(b'[') {
        let open = text[at + 1..].iter().position(|&c| c == u16::from(b'[')).map_or(text.len(), |i| at + 1 + i);
        if let Some(close) = text[at + 1..open].iter().rposition(|&c| c == u16::from(b']')) {
            return Some(at + 2 + close);
        }
    }
    let separator = |c: u16| is_space(c) || units("-_:/.,()").contains(&c);
    let run = text[at..].iter().take_while(|&&c| separator(c)).count();
    if run > 0 {
        return Some(at + run);
    }
    for (letter, most) in [(b'A', 1), (b'a', 1), (b'Q', 1)] {
        if let Some(end) = repeated(text, at, letter, most) {
            return Some(end);
        }
    }
    literal(text, at, "YYYY")
        .or_else(|| repeated(text, at, b'Y', 2))
        .or_else(|| repeated(text, at, b'w', 2))
        .or_else(|| repeated(text, at, b'M', 4))
        .or_else(|| literal(text, at, "Do"))
        .or_else(|| repeated(text, at, b'D', 2))
        .or_else(|| repeated(text, at, b'h', 2))
        .or_else(|| repeated(text, at, b'H', 2))
        .or_else(|| repeated(text, at, b'm', 2))
        .or_else(|| repeated(text, at, b's', 2))
        .or_else(|| repeated(text, at, b'S', 3))
        .or_else(|| literal(text, at, "z"))
        .or_else(|| repeated(text, at, b'Z', 2))
}

/// Pele's `compile`: `None` for a format dayjs cannot read with.
fn compile(template: &[u16]) -> Option<Vec<Step>> {
    let unbracketed = replace_all(template, bracketed, |_| Vec::new());
    if unbracketed.iter().any(|&c| c == u16::from(b'L') || c == u16::from(b'l')) {
        return None;
    }
    let mut steps = Vec::new();
    let mut at = 0;
    while at < template.len() {
        match format_part(template, at) {
            Some(end) => {
                let token = &template[at..end];
                steps.push(match reader(&String::from_utf16_lossy(token)) {
                    Some((pattern, read)) => Step::Read(pattern, read),
                    None => {
                        let mut length = token.len();
                        if token.first() == Some(&u16::from(b'[')) {
                            length -= 1;
                        }
                        if length > 0 && token.last() == Some(&u16::from(b']')) {
                            length -= 1;
                        }
                        Step::Literal(length)
                    }
                });
                at = end;
            }
            None => at += 1,
        }
    }
    (!steps.is_empty()).then_some(steps)
}

/// The fields a format read, each unset until a reader sets it.
#[derive(Default)]
struct Fields {
    year: Option<f64>,
    month: Option<f64>,
    day: Option<f64>,
    day_given: bool,
    hours: Option<f64>,
    minutes: Option<f64>,
    seconds: Option<f64>,
    milliseconds: Option<f64>,
    week: Option<f64>,
    offset: Option<f64>,
    afternoon: Option<bool>,
}

/// JavaScript's truthiness of a number that may be undefined.
fn truthy(value: Option<f64>) -> bool {
    value.is_some_and(|value| value != 0.0 && !value.is_nan())
}

fn read(fields: &mut Fields, read: Read, input: &[u16]) -> bool {
    let number = || string_to_number(input);
    match read {
        Read::Afternoon(word) => fields.afternoon = Some(input == units(word).as_slice()),
        Read::Quarter => fields.month = Some((number() - 1.0) * 3.0 + 1.0),
        Read::Milliseconds(scale) => fields.milliseconds = Some(number() * scale),
        Read::Field(field) => {
            let slot = match field {
                Field::Seconds => &mut fields.seconds,
                Field::Minutes => &mut fields.minutes,
                Field::Hours => &mut fields.hours,
                Field::Day => &mut fields.day,
                Field::Week => &mut fields.week,
                Field::Month => &mut fields.month,
                Field::Year => &mut fields.year,
            };
            *slot = Some(number());
        }
        Read::Ordinal => {
            let Some(start) = input.iter().position(|&c| is_digit(c)) else {
                return false;
            };
            fields.day = Some(string_to_number(&input[start..digit_run(input, start)]));
            fields.day_given = true;
            for i in 1..=31 {
                if units(&ordinal(f64::from(i))).as_slice() == input {
                    fields.day = Some(f64::from(i));
                }
            }
        }
        Read::MonthName(short) => {
            let name = String::from_utf16_lossy(input);
            let Some(index) = MONTHS.iter().position(|month| if short { month[..3] == name } else { *month == name })
            else {
                return false;
            };
            fields.month = Some((index + 1) as f64);
        }
        Read::ShortYear => {
            let year = number();
            fields.year = Some(year + if year > 68.0 { 1900.0 } else { 2000.0 });
        }
        Read::Offset => {
            if input == [u16::from(b'Z')] {
                fields.offset = Some(0.0);
                return true;
            }
            let mut parts: Vec<&[u16]> = Vec::new();
            let mut at = 0;
            while at < input.len() {
                if is_sign(input[at]) {
                    parts.push(&input[at..at + 1]);
                    at += 1;
                } else if digit_at(input, at) && digit_at(input, at + 1) {
                    parts.push(&input[at..at + 2]);
                    at += 2;
                } else {
                    at += 1;
                }
            }
            let rest = parts.get(2).map_or(f64::NAN, |part| string_to_number(part));
            let minutes = string_to_number(parts[1]) * 60.0 + if truthy(Some(rest)) { rest } else { 0.0 };
            fields.offset = Some(if minutes == 0.0 {
                0.0
            } else if parts[0] == [u16::from(b'+')] {
                -minutes
            } else {
                minutes
            });
        }
    }
    true
}

/// `rest.replace(found, '')`: the first occurrence of `found` taken out.
fn remove_first(rest: &mut Vec<u16>, found: &[u16]) {
    if let Some(at) = rest.windows(found.len()).position(|window| window == found) {
        rest.drain(at..at + found.len());
    }
}

/// Pele's `loose`: reads `input` with `template`, taking what the format leaves out from `now`.
fn loose(input: &[u16], template: &[u16], now: JsDate) -> Option<JsDate> {
    if template == [u16::from(b'x')] || template == [u16::from(b'X')] {
        let scale = if template[0] == u16::from(b'X') { 1000.0 } else { 1.0 };
        return Some(JsDate::from_time(scale * string_to_number(input)));
    }
    let steps = compile(template)?;
    let mut f = Fields::default();
    let mut rest = input.to_vec();
    let mut start = 0;
    for step in steps {
        match step {
            Step::Literal(length) => start += length,
            Step::Read(pattern, reading) => {
                let tail = rest.get(start..).unwrap_or_default();
                let (from, to) = pattern.find(tail)?;
                let found = tail[from..to].to_vec();
                if !read(&mut f, reading, &found) {
                    return None;
                }
                remove_first(&mut rest, &found);
            }
        }
    }
    if let (Some(afternoon), Some(hours)) = (f.afternoon, f.hours) {
        if afternoon {
            if hours < 12.0 {
                f.hours = Some(hours + 12.0);
            }
        } else if hours == 12.0 {
            f.hours = Some(0.0);
        }
    }
    let day = if f.day_given || truthy(f.day) {
        f.day.expect("a day that was given")
    } else if !truthy(f.year) && !truthy(f.month) {
        now.date()
    } else {
        1.0
    };
    let year = if truthy(f.year) { f.year.expect("a year") } else { now.full_year() };
    let mut month = 0.0;
    if !(truthy(f.year) && !truthy(f.month)) {
        month = match f.month {
            Some(m) if m > 0.0 => m - 1.0,
            _ => now.month(),
        };
    }
    let or_zero = |value: Option<f64>| if truthy(value) { value.expect("a value") } else { 0.0 };
    let (hours, minutes, seconds, ms) =
        (or_zero(f.hours), or_zero(f.minutes), or_zero(f.seconds), or_zero(f.milliseconds));
    if let Some(offset) = f.offset {
        return Some(JsDate::from_time(JsDate::utc(year, month, day, hours, minutes, seconds, ms + offset * 60_000.0)));
    }
    if truthy(f.week) {
        return None;
    }
    Some(JsDate::local(year, month, day, hours, minutes, seconds, ms))
}

/// Pele's `parseStrict`: reads `input` as `dayjs(input, template, true)` does.
pub(crate) fn parse_strict(input: &[u16], template: &[u16], now: JsDate) -> Option<JsDate> {
    let date = loose(input, template, now)?;
    (date.is_valid() && format(date, template).as_deref() == Some(input)).then_some(date)
}
