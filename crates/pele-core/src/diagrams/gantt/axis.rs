//! Pele's `axis.ts`: the time axis as d3-time and d3-time-format draw it for Mermaid (English
//! locale, local time): tick intervals with d3's `every` rule, its choice of automatic ticks, and
//! the `axisFormat` specifiers.

use super::date::JsDate;
use super::dates::{MONTHS, WEEKDAYS};
use crate::js;

const SECOND: f64 = 1000.0;
const MINUTE: f64 = 60.0 * SECOND;
const HOUR: f64 = 60.0 * MINUTE;
const DAY: f64 = 24.0 * HOUR;
const WEEK: f64 = 7.0 * DAY;
const MONTH: f64 = 30.0 * DAY;
const YEAR: f64 = 365.0 * DAY;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TickUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    /// Weeks that start on this day, from 0 for Sunday.
    Week(u8),
    Month,
    Year,
}

fn start_of_day(d: &mut JsDate) {
    d.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0));
}

impl TickUnit {
    fn floor(self, d: &mut JsDate) {
        match self {
            TickUnit::Millisecond => {}
            TickUnit::Second => d.set_time(d.time() - d.milliseconds()),
            TickUnit::Minute => d.set_time(d.time() - d.milliseconds() - d.seconds() * SECOND),
            TickUnit::Hour => {
                d.set_time(d.time() - d.milliseconds() - d.seconds() * SECOND - d.minutes() * MINUTE);
            }
            TickUnit::Day => start_of_day(d),
            TickUnit::Week(start) => {
                d.set_date(d.date() - (d.day() + 7.0 - f64::from(start)) % 7.0);
                start_of_day(d);
            }
            TickUnit::Month => {
                d.set_date(1.0);
                start_of_day(d);
            }
            TickUnit::Year => {
                d.set_month(0.0, Some(1.0));
                start_of_day(d);
            }
        }
    }

    fn offset(self, d: &mut JsDate, step: f64) {
        match self {
            TickUnit::Millisecond => d.set_time(d.time() + step),
            TickUnit::Second => d.set_time(d.time() + step * SECOND),
            TickUnit::Minute => d.set_time(d.time() + step * MINUTE),
            TickUnit::Hour => d.set_time(d.time() + step * HOUR),
            TickUnit::Day => d.set_date(d.date() + step),
            TickUnit::Week(_) => d.set_date(d.date() + step * 7.0),
            TickUnit::Month => d.set_month(d.month() + step, None),
            TickUnit::Year => d.set_full_year(d.full_year() + step, None, None),
        }
    }

    /// The value d3 tests against a step: a tick falls where it is a multiple of the step.
    fn field(self, d: JsDate) -> f64 {
        match self {
            TickUnit::Second => d.seconds(),
            TickUnit::Minute => d.minutes(),
            TickUnit::Hour => d.hours(),
            TickUnit::Day => d.date() - 1.0,
            TickUnit::Month => d.month(),
            TickUnit::Millisecond | TickUnit::Week(_) | TickUnit::Year => unreachable!("no field steps these"),
        }
    }

    /// How many values `field` cycles through, which bounds how far apart filtered ticks can be.
    fn cycle(self) -> f64 {
        match self {
            TickUnit::Second | TickUnit::Minute => 60.0,
            TickUnit::Hour => 24.0,
            TickUnit::Day => 31.0,
            TickUnit::Month => 12.0,
            TickUnit::Millisecond | TickUnit::Week(_) | TickUnit::Year => f64::INFINITY,
        }
    }

    fn ms(self) -> f64 {
        match self {
            TickUnit::Millisecond => 1.0,
            TickUnit::Second => SECOND,
            TickUnit::Minute => MINUTE,
            TickUnit::Hour => HOUR,
            TickUnit::Day => DAY,
            TickUnit::Week(_) => WEEK,
            TickUnit::Month => MONTH,
            TickUnit::Year => YEAR,
        }
    }
}

/// Whole units between two floored dates, discounting a change of UTC offset between them.
fn span(start: JsDate, end: JsDate, unit: f64) -> f64 {
    ((end.time() - start.time() - (end.timezone_offset() - start.timezone_offset()) * MINUTE) / unit).floor()
}

fn modulo(a: f64, b: f64) -> f64 {
    ((a % b) + b) % b
}

/// The interval d3's `unit.every(step)` gives.
#[derive(Clone, Copy, Debug)]
struct Calendar {
    unit: TickUnit,
    step: f64,
}

impl Calendar {
    fn every(unit: TickUnit, step: f64) -> Self {
        Calendar { unit, step }
    }

    fn plain(self) -> bool {
        self.step <= 1.0
    }

    fn test(self, d: JsDate) -> bool {
        self.unit.field(d) % self.step == 0.0
    }

    fn floor(self, d: &mut JsDate) {
        if self.plain() {
            return self.unit.floor(d);
        }
        match self.unit {
            TickUnit::Millisecond => d.set_time((d.time() / self.step).floor() * self.step),
            TickUnit::Year => {
                d.set_full_year((d.full_year() / self.step).floor() * self.step, None, None);
                self.unit.floor(d);
            }
            TickUnit::Week(_) => {
                let mut epoch = JsDate::from_time(0.0);
                self.unit.floor(&mut epoch);
                self.unit.floor(d);
                self.unit.offset(d, -modulo(span(epoch, *d, WEEK), self.step));
            }
            _ => {
                // Date overflow makes the field NaN, which can never match the interval.
                self.unit.floor(d);
                while d.time().is_finite() && !self.test(*d) {
                    d.set_time(d.time() - 1.0);
                    self.unit.floor(d);
                }
            }
        }
    }

    fn offset(self, d: &mut JsDate, n: f64) {
        if self.plain() {
            return self.unit.offset(d, n);
        }
        match self.unit {
            TickUnit::Millisecond => d.set_time(d.time() + n * self.step),
            TickUnit::Year => d.set_full_year(d.full_year() + n * self.step, None, None),
            TickUnit::Week(_) => self.unit.offset(d, n * self.step),
            _ => {
                let mut i = 0.0;
                while i < n && d.time().is_finite() {
                    loop {
                        self.unit.offset(d, 1.0);
                        if !(d.time().is_finite() && !self.test(*d)) {
                            break;
                        }
                    }
                    i += 1.0;
                }
            }
        }
    }

    /// Ticks from start to stop inclusive, or `None` when there would be more than `limit`.
    fn range(self, start: f64, stop: f64, limit: f64) -> Option<Vec<f64>> {
        let mut d = JsDate::from_time(start - 1.0);
        self.floor(&mut d);
        self.offset(&mut d, 1.0);
        self.floor(&mut d);
        let mut out = Vec::new();
        let mut previous = f64::NEG_INFINITY;
        while d.time() <= stop && d.time() > previous {
            if out.len() as f64 >= limit {
                return None;
            }
            previous = d.time();
            out.push(previous);
            self.offset(&mut d, 1.0);
            self.floor(&mut d);
        }
        Some(out)
    }
}

fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let step = (stop - start) / js::max(0.0, count);
    let power = libm::log10(step).floor();
    let error = step / js::pow(10.0, power);
    let factor = if error >= 50f64.sqrt() {
        10.0
    } else if error >= 10f64.sqrt() {
        5.0
    } else if error >= std::f64::consts::SQRT_2 {
        2.0
    } else {
        1.0
    };
    js::pow(10.0, power) * factor
}

const AUTO: [(TickUnit, f64, f64); 18] = [
    (TickUnit::Second, 1.0, SECOND),
    (TickUnit::Second, 5.0, 5.0 * SECOND),
    (TickUnit::Second, 15.0, 15.0 * SECOND),
    (TickUnit::Second, 30.0, 30.0 * SECOND),
    (TickUnit::Minute, 1.0, MINUTE),
    (TickUnit::Minute, 5.0, 5.0 * MINUTE),
    (TickUnit::Minute, 15.0, 15.0 * MINUTE),
    (TickUnit::Minute, 30.0, 30.0 * MINUTE),
    (TickUnit::Hour, 1.0, HOUR),
    (TickUnit::Hour, 3.0, 3.0 * HOUR),
    (TickUnit::Hour, 6.0, 6.0 * HOUR),
    (TickUnit::Hour, 12.0, 12.0 * HOUR),
    (TickUnit::Day, 1.0, DAY),
    (TickUnit::Day, 2.0, 2.0 * DAY),
    (TickUnit::Week(0), 1.0, WEEK),
    (TickUnit::Month, 1.0, MONTH),
    (TickUnit::Month, 3.0, 3.0 * MONTH),
    (TickUnit::Year, 1.0, YEAR),
];

/// About `count` ticks at round times, chosen the way d3's time scale chooses them.
pub(crate) fn auto_ticks(start: f64, stop: f64, count: f64) -> Vec<f64> {
    let target = (stop - start) / count;
    let i = AUTO.iter().take_while(|(_, _, ms)| *ms <= target).count();
    let calendar = if i == AUTO.len() {
        Calendar::every(TickUnit::Year, js::max(1.0, tick_step(start / YEAR, stop / YEAR, count)))
    } else if i == 0 {
        Calendar::every(TickUnit::Millisecond, js::max(tick_step(start, stop, count), 1.0))
    } else {
        let pick = if target / AUTO[i - 1].2 < AUTO[i].2 / target { i - 1 } else { i };
        Calendar::every(AUTO[pick].0, AUTO[pick].1)
    };
    calendar.range(start, stop, 4.0 * count + 8.0).unwrap_or_default()
}

const WEEK_STARTS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

/// `^([1-9]\d*)(millisecond|second|minute|hour|day|week|month)$`: the step and its unit, with
/// weeks starting on `week_start`.
fn interval(text: &[u16], week_start: u8) -> Option<(f64, TickUnit)> {
    let digits = text.iter().take_while(|&&c| (48..=57).contains(&c)).count();
    if digits == 0 || text[0] == u16::from(b'0') {
        return None;
    }
    let unit = match String::from_utf16_lossy(&text[digits..]).as_str() {
        "millisecond" => TickUnit::Millisecond,
        "second" => TickUnit::Second,
        "minute" => TickUnit::Minute,
        "hour" => TickUnit::Hour,
        "day" => TickUnit::Day,
        "week" => TickUnit::Week(week_start),
        "month" => TickUnit::Month,
        _ => return None,
    };
    Some((crate::input::decimal(&text[..digits]), unit))
}

/// Ticks for a `tickInterval` such as `1week`, or `None` when it is not one or asks for more
/// than `limit`.
pub(crate) fn interval_ticks(text: &[u16], weekday: &[u16], start: f64, stop: f64, limit: f64) -> Option<Vec<f64>> {
    let named = WEEK_STARTS.iter().position(|day| day.encode_utf16().eq(weekday.iter().copied()));
    let (step, unit) = interval(text, named.unwrap_or(0) as u8)?;
    let calendar = Calendar::every(unit, step);
    if (stop - start) / (js::min(step, unit.cycle()) * unit.ms()) > limit {
        return None;
    }
    calendar.range(start, stop, limit)
}

fn floored(date: JsDate, unit: TickUnit) -> JsDate {
    let mut d = date;
    unit.floor(&mut d);
    d
}

/// Weeks since the last `week_start` day before the year began, which is d3's `%U` and `%W`.
fn week_of_year(date: JsDate, week_start: u8) -> f64 {
    let week = TickUnit::Week(week_start);
    let year_start = floored(date, TickUnit::Year);
    span(floored(JsDate::from_time(year_start.time() - 1.0), week), floored(date, week), WEEK)
}

/// The Thursday that decides which ISO week and ISO year a date belongs to.
fn iso_thursday(date: JsDate) -> JsDate {
    let thursday = TickUnit::Week(4);
    let day = date.day();
    let on = day >= 4.0 || day == 0.0;
    let mut d = floored(if on { date } else { JsDate::from_time(date.time() - 1.0) }, thursday);
    if !on {
        thursday.offset(&mut d, 1.0);
        thursday.floor(&mut d);
    }
    d
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// d3's `pad`: the sign, then the digits filled to `width` with `fill`.
fn pad(value: f64, fill: &[u16], width: usize) -> Vec<u16> {
    let negative = value < 0.0;
    let digits = units(&js::to_string(if negative { -value } else { value }));
    let mut out = Vec::new();
    if negative {
        out.push(u16::from(b'-'));
    }
    if digits.len() < width {
        for _ in 0..width - digits.len() {
            out.extend_from_slice(fill);
        }
    }
    out.extend(digits);
    out
}

/// Formats a date with a d3 time format specifier such as `%Y-%m-%d`.
pub(crate) fn time_format(specifier: &[u16], date: JsDate) -> Vec<u16> {
    let mut out = Vec::new();
    let mut from = 0;
    let n = specifier.len();
    let mut i = 0;
    while i < n {
        if specifier[i] != u16::from(b'%') {
            i += 1;
            continue;
        }
        out.extend_from_slice(&specifier[from..i]);
        i += 1;
        let mut c = specifier.get(i).copied();
        let fill: Vec<u16> = match c.map(|c| c as u32) {
            Some(0x2d) => units(""),
            Some(0x5f) => units(" "),
            Some(0x30) => units("0"),
            _ => units(if c == Some(u16::from(b'e')) { " " } else { "0" }),
        };
        if matches!(c, Some(0x2d | 0x5f | 0x30)) {
            i += 1;
            c = specifier.get(i).copied();
        }
        out.extend(directive(c, date, &fill));
        from = i + 1;
        i += 1;
    }
    out.extend_from_slice(&specifier[from.min(n)..]);
    out
}

fn directive(c: Option<u16>, d: JsDate, p: &[u16]) -> Vec<u16> {
    let Some(c) = c else {
        return Vec::new();
    };
    let number = |value: f64| units(&js::to_string(value));
    let weekday = || WEEKDAYS[d.day() as usize];
    let month = || MONTHS[d.month() as usize];
    let twelve = || if d.hours() % 12.0 == 0.0 { 12.0 } else { d.hours() % 12.0 };
    match char::from_u32(u32::from(c)).unwrap_or('\u{fffd}') {
        'a' => units(&weekday()[..3]),
        'A' => units(weekday()),
        'b' => units(&month()[..3]),
        'B' => units(month()),
        'c' => time_format(&units("%x, %X"), d),
        'd' | 'e' => pad(d.date(), p, 2),
        'f' => {
            let mut out = pad(d.milliseconds(), p, 3);
            out.extend(units("000"));
            out
        }
        'g' => pad(iso_thursday(d).full_year() % 100.0, p, 2),
        'G' => pad(iso_thursday(d).full_year() % 10000.0, p, 4),
        'H' => pad(d.hours(), p, 2),
        'I' => pad(twelve(), p, 2),
        'j' => pad(1.0 + span(floored(d, TickUnit::Year), floored(d, TickUnit::Day), DAY), p, 3),
        'L' => pad(d.milliseconds(), p, 3),
        'm' => pad(d.month() + 1.0, p, 2),
        'M' => pad(d.minutes(), p, 2),
        'p' => units(if d.hours() >= 12.0 { "PM" } else { "AM" }),
        'q' => number(1.0 + (d.month() / 3.0).floor()),
        'Q' => number(d.time()),
        's' => number((d.time() / 1000.0).floor()),
        'S' => pad(d.seconds(), p, 2),
        'u' => number(if d.day() == 0.0 { 7.0 } else { d.day() }),
        'U' => pad(week_of_year(d, 0), p, 2),
        'V' => {
            let thursday = iso_thursday(d);
            let year_start = floored(thursday, TickUnit::Year);
            let first = floored(year_start, TickUnit::Week(4));
            pad(span(first, thursday, WEEK) + if year_start.day() == 4.0 { 1.0 } else { 0.0 }, p, 2)
        }
        'w' => number(d.day()),
        'W' => pad(week_of_year(d, 1), p, 2),
        'x' => time_format(&units("%-m/%-d/%Y"), d),
        'X' => time_format(&units("%-I:%M:%S %p"), d),
        'y' => pad(d.full_year() % 100.0, p, 2),
        'Y' => pad(d.full_year() % 10000.0, p, 4),
        'Z' => {
            let z = d.timezone_offset();
            let abs = z.abs();
            let mut out = units(if z > 0.0 { "-" } else { "+" });
            out.extend(pad((abs / 60.0).floor(), &units("0"), 2));
            out.extend(pad(abs % 60.0, &units("0"), 2));
            out
        }
        '%' => units("%"),
        _ => vec![c],
    }
}
