//! Entry points that let `tests/gantt_dates.rs` hold the date helpers to Pele. Each takes plain
//! values, calls the gantt module's own helper, and answers in plain values. A time is a time
//! value in milliseconds, NaN for an invalid date.

use super::axis;
use super::date::JsDate;
use super::dates::{self, Unit};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn text(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// `new Date(text).getTime()`.
pub fn new_date(source: &str) -> f64 {
    JsDate::parse(&units(source)).time()
}

/// `new Date(year, month, date, hours, minutes, seconds, ms).getTime()`.
pub fn local_date([year, month, date, hours, minutes, seconds, ms]: [f64; 7]) -> f64 {
    JsDate::local(year, month, date, hours, minutes, seconds, ms).time()
}

/// `Date.UTC(year, month, date, hours, minutes, seconds, ms)`.
pub fn utc_date([year, month, date, hours, minutes, seconds, ms]: [f64; 7]) -> f64 {
    JsDate::utc(year, month, date, hours, minutes, seconds, ms)
}

/// The time after calling one of the setters Pele uses on `new Date(time)`. `args` are the
/// arguments given, so a setter called with fewer leaves the rest to the date.
pub fn set(time: f64, method: &str, args: &[f64]) -> f64 {
    let mut date = JsDate::from_time(time);
    let arg = |i: usize| args.get(i).copied();
    match method {
        "setTime" => date.set_time(args[0]),
        "setDate" => date.set_date(args[0]),
        "setMonth" => date.set_month(args[0], arg(1)),
        "setFullYear" => date.set_full_year(args[0], arg(1), arg(2)),
        "setHours" => date.set_hours(args[0], arg(1), arg(2), arg(3)),
        other => panic!("{other} is not a setter Pele uses"),
    }
    date.time()
}

/// The getters Pele reads, in order: full year, month, date, day, hours, minutes, seconds,
/// milliseconds and the time zone offset.
pub fn fields(time: f64) -> [f64; 9] {
    let d = JsDate::from_time(time);
    [
        d.full_year(),
        d.month(),
        d.date(),
        d.day(),
        d.hours(),
        d.minutes(),
        d.seconds(),
        d.milliseconds(),
        d.timezone_offset(),
    ]
}

/// `parseStrict(input, template, new Date(now))`, as its time.
pub fn parse_strict(input: &str, template: &str, now: f64) -> Option<f64> {
    dates::parse_strict(&units(input), &units(template), JsDate::from_time(now)).map(JsDate::time)
}

/// `format(new Date(time), template)`.
pub fn format(time: f64, template: &str) -> Option<String> {
    dates::format(JsDate::from_time(time), &units(template)).map(|out| text(&out))
}

fn unit(name: &str) -> Unit {
    match name {
        "ms" => Unit::Millisecond,
        "s" => Unit::Second,
        "m" => Unit::Minute,
        "h" => Unit::Hour,
        "d" => Unit::Day,
        "w" => Unit::Week,
        "M" => Unit::Month,
        "y" => Unit::Year,
        other => panic!("{other} is not a unit"),
    }
}

/// `add(new Date(time), amount, unit)`.
pub fn add(time: f64, amount: f64, name: &str) -> f64 {
    dates::add(JsDate::from_time(time), amount, unit(name)).time()
}

/// `isoDate(new Date(time))`.
pub fn iso_date(time: f64) -> String {
    text(&dates::iso_date(JsDate::from_time(time)))
}

/// `parseDuration(text)`: the amount and the unit.
pub fn parse_duration(source: &str) -> (f64, &'static str) {
    let (amount, unit) = super::db::parse_duration(&units(source));
    (amount, unit.name())
}

/// `autoTicks(start, stop, count)`.
pub fn auto_ticks(start: f64, stop: f64, count: f64) -> Vec<f64> {
    axis::auto_ticks(start, stop, count)
}

/// `intervalTicks(interval, weekday, start, stop, limit)`.
pub fn interval_ticks(interval: &str, weekday: &str, start: f64, stop: f64, limit: f64) -> Option<Vec<f64>> {
    axis::interval_ticks(&units(interval), &units(weekday), start, stop, limit)
}

/// `timeFormat(specifier, new Date(time))`.
pub fn time_format(specifier: &str, time: f64) -> String {
    text(&axis::time_format(&units(specifier), JsDate::from_time(time)))
}
