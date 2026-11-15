//! Differential test for the gantt chart's date helpers on their own: JavaScript's `Date` as V8
//! runs it in UTC (the string parser, the constructors, the setters and getters Pele uses), and
//! Pele's dayjs and d3 ports (`parseStrict`, `format`, `add`, `isoDate`, `parseDuration`, the
//! tick intervals and `timeFormat`). Every answer must equal the pinned Node's, through the
//! oracle's `gantt_date` op (`harness/gantt.ts`).
//!
//! The seeded inputs lean on what dates get wrong: leap years and the days around them, month
//! ends, the far ends of the time range, years before 1000 and before 0, the 1900s rule for years
//! under 100, fractions, and V8's legacy string formats.

use pele_core::gantt_probe as probe;
use serde_json::{Value, json};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
    fn int(&mut self, lo: i64, hi: i64) -> f64 {
        (lo + (self.next() % (hi - lo + 1) as u64) as i64) as f64
    }
}

fn bits(value: f64) -> String {
    if value.is_nan() { "NaN".to_owned() } else { format!("{:016x}", value.to_bits()) }
}

fn time_json(value: f64) -> Value {
    Value::String(bits(value))
}

const DAY: f64 = 86_400_000.0;
const MAX_TIME: f64 = 8.64e15;

/// The time value of a UTC calendar day, computed here without the code under test.
fn days_from_civil(year: i64, month: i64, day: i64) -> f64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as f64 * DAY
}

/// A time that dates get wrong: around leap days, month ends and year ends, in the years a chart
/// uses, before 1000 and before 0, at the far ends of the range, or anywhere in it.
fn interesting_time(rng: &mut Rng) -> f64 {
    let in_day = |rng: &mut Rng| match rng.below(4) {
        0 => 0.0,
        1 => rng.int(0, 86_399_999),
        2 => rng.int(0, 23) * 3_600_000.0,
        _ => DAY - 1.0,
    };
    match rng.below(12) {
        0 => rng.int(-8_640_000_000_000_000, 8_640_000_000_000_000),
        1 => *rng.pick(&[MAX_TIME, -MAX_TIME, MAX_TIME - DAY, -MAX_TIME + DAY, MAX_TIME - 1.0, 0.0, -1.0, 1.0]),
        2 => {
            let year = *rng.pick(&[1900, 2000, 2004, 2024, 2100, 1600, 0, -4, 4, 400, 1, 99, 100, 1970, 1969]);
            days_from_civil(year, 2, rng.int(27, 29) as i64) + in_day(rng)
        }
        3 => {
            let year = rng.int(1, 9999) as i64;
            let month = rng.int(1, 12) as i64;
            let next = days_from_civil(year + i64::from(month == 12), month % 12 + 1, 1);
            next - rng.int(1, 3) * DAY + in_day(rng)
        }
        4 => days_from_civil(rng.int(-10_000, 0) as i64, rng.int(1, 12) as i64, rng.int(1, 28) as i64) + in_day(rng),
        5 => days_from_civil(rng.int(0, 999) as i64, rng.int(1, 12) as i64, rng.int(1, 28) as i64) + in_day(rng),
        6 => days_from_civil(rng.int(-271_000, 275_000) as i64, rng.int(1, 12) as i64, 1) + in_day(rng),
        _ => days_from_civil(rng.int(1950, 2100) as i64, rng.int(1, 12) as i64, rng.int(1, 31) as i64) + in_day(rng),
    }
}

/// An argument to a constructor or setter: whole or not, small or out of range, or not a number.
fn argument(rng: &mut Rng, around: f64) -> f64 {
    match rng.below(14) {
        0 => f64::NAN,
        1 => *rng.pick(&[f64::INFINITY, f64::NEG_INFINITY, -0.0, 1e300, -1e300]),
        2 => around + rng.int(-1000, 1000) / 7.0,
        3 => around + rng.int(-20_000_000, 20_000_000),
        4 => rng.int(-1_000_001, 1_000_001),
        _ => around + rng.int(-40, 40),
    }
}

#[derive(Default)]
struct Tally {
    calls: Vec<Value>,
    ours: Vec<Value>,
}

impl Tally {
    fn push(&mut self, call: Value, ours: Value) {
        self.calls.push(call);
        self.ours.push(ours);
    }

    /// How many of the calls of `kind` answered something other than `none`.
    fn answered(&self, kind: &str, none: &Value) -> usize {
        self.calls.iter().zip(&self.ours).filter(|(call, ours)| call["fn"] == kind && *ours != none).count()
    }

    fn check(self, what: &str) {
        let requests: Vec<String> = self
            .calls
            .iter()
            .map(|call| {
                format!(
                    "gantt_date\t{}",
                    call.to_string().replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029")
                )
            })
            .collect();
        let answers = pele_oracle::ask(&requests);
        let mut failures = Vec::new();
        for ((call, ours), theirs) in self.calls.iter().zip(&self.ours).zip(&answers) {
            let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
            if *ours != theirs {
                failures.push(format!("{call}\n    ours: {ours}\n    pele: {theirs}"));
            }
        }
        println!("{what}: {} inputs, {} different", self.calls.len(), failures.len());
        assert!(
            failures.is_empty(),
            "{} of {} {what} differ from Pele:\n{}",
            failures.len(),
            self.calls.len(),
            failures[..failures.len().min(15)].join("\n")
        );
    }
}

/// Strings V8's date parser reads, and strings it nearly reads.
const DATE_STRINGS: &[&str] = &[
    "2014-01-06",
    "2014-01-06T10:00",
    "2014-01-06T10:00:30.5Z",
    "2014-01-06T24:00",
    "2014-01-06T24:00:01",
    "2014-01-06T10:00+05:30",
    "2014-01-06T10:00-0530",
    "+002014-01-06",
    "-000000-01-01",
    "-000001-01-01T00:00Z",
    "2014-1-6",
    "2014/01/06",
    "01/06/2014",
    "1/6/14",
    "6 Jan 2014",
    "Jan 6, 2014 10:00 PM",
    "January 6 2014 10:00:00 GMT+0200",
    "Mon, 06 Jan 2014 10:00:00 GMT",
    "Mon Jan 06 2014 10:00:00 GMT-0800 (PST)",
    "2014-01-06 10:00",
    "2014-01-06 10:00:00.123",
    "2014-01-06 10:00 PM",
    "12:00 am 2014-01-06",
    "task 1",
    "hello 2014",
    "foo1",
    "1",
    "12",
    "123",
    "99999",
    "2014",
    "x2014-01-06",
    "2014-01-06x",
    "after a",
    "",
    " ",
    "(comment) 2014-01-06",
    "2014-01-06 (comment (nested))",
    "2014-01-06)",
    "Tue 2014-01-06",
    "2014-13-01",
    "2014-02-30",
    "2014-02-29",
    "2016-02-29",
    "0000-01-01",
    "0099-12-31",
    "10000-01-01",
    "2014-01-06T10:00 EST",
    "10:00 2014-01-06 UTC+5",
    "10:00 2014-01-06 z",
    "Thu, 01 Jan 1970 00:00:00 GMT+12345",
    "2014.01.06",
    "06.01.2014",
    "2014-01-06T10:00:00.1234567",
    "1:2:3 1/2/3",
    "1::2 1/2/3",
    "1.5 1/2/3",
    "Dec 25 1999 11:59:59.999 pm",
    "\u{a0}2014-01-06",
    "2014-01-06\u{2028}",
    "sept 3 2001",
    "Septembre 3 2001",
    "3 2001 mar",
    "2001 mar 3",
    "32 jan 2001",
    "jan 2001 32",
    "1 2 3 4",
    "10 PM",
    "13 PM 1/1/2000",
    "1/1/2000 13:00 PM",
    "1/1/2000 12:00 -",
    "1/1/2000 12:00 +",
    "1/1/2000 12:00 +1",
    "1/1/2000 12:00 +12:",
    "1/1/2000 12:00 +12:30",
    "1/1/2000 12:00 +999999999:",
    "1/1/2000 GMT-8",
    "1/1/2000 utc+0530",
];

/// Pieces random date strings are made of.
const DATE_PIECES: &[&str] = &[
    "2014",
    "01",
    "1",
    "6",
    "06",
    "12",
    "31",
    "29",
    "02",
    "0",
    "00",
    "000",
    "0000",
    "123",
    "99",
    "50",
    "49",
    "1000000",
    "123456789012",
    "-",
    "/",
    ":",
    ".",
    ",",
    " ",
    "  ",
    "T",
    "t",
    "Z",
    "z",
    "+",
    "(",
    ")",
    "(x)",
    "Jan",
    "feb",
    "MARCH",
    "sept",
    "dec",
    "am",
    "PM",
    "GMT",
    "UTC",
    "ut",
    "est",
    "pdt",
    "Mon",
    "Tuesday",
    "x",
    "abc",
    "task",
    "\u{a0}",
    "\t",
    "\u{3000}",
    "é",
    "[",
    "_",
];

fn random_date_string(rng: &mut Rng) -> String {
    (0..1 + rng.below(8)).map(|_| *rng.pick(DATE_PIECES)).collect()
}

#[test]
fn date_strings_parse_as_v8_parses_them() {
    let mut tally = Tally::default();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut strings: Vec<String> = DATE_STRINGS.iter().map(|&s| s.to_owned()).collect();
    for _ in 0..12_000 {
        strings.push(random_date_string(&mut rng));
    }
    for _ in 0..2000 {
        let time = interesting_time(&mut rng);
        let year = probe::fields(time)[0];
        if year.is_nan() {
            continue;
        }
        strings.push(format!("{:04}-{:02}-{:02}", year.abs(), rng.int(0, 13), rng.int(0, 32)));
    }
    for text in &strings {
        tally.push(json!({ "fn": "new", "text": text }), time_json(probe::new_date(text)));
    }
    let valid = tally.answered("new", &json!("NaN"));
    println!("{valid} of the date strings are dates");
    assert!(valid > 2000, "the strings reach V8's date paths");
    tally.check("date strings read by new Date");
}

#[test]
fn constructors_setters_and_getters_match_v8() {
    let mut tally = Tally::default();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for _ in 0..6000 {
        let time = interesting_time(&mut rng);
        let fields = probe::fields(time);
        let year = if fields[0].is_nan() { 2000.0 } else { fields[0] };
        let mut args = [year, fields[1], fields[2], fields[4], fields[5], fields[6], fields[7]];
        for (i, arg) in args.iter_mut().enumerate() {
            if rng.chance(30) {
                *arg = argument(&mut rng, if arg.is_nan() { 0.0 } else { *arg });
            }
            if i == 0 && rng.chance(10) {
                *arg = rng.int(-2, 101);
            }
        }
        let list: Vec<Value> = args.iter().map(|&a| time_json(a)).collect();
        tally.push(json!({ "fn": "local", "args": list }), time_json(probe::local_date(args)));
        tally.push(json!({ "fn": "utc", "args": list }), time_json(probe::utc_date(args)));
        let ours: Vec<Value> = probe::fields(time).iter().map(|&f| time_json(f)).collect();
        tally.push(json!({ "fn": "fields", "time": bits(time) }), Value::Array(ours));

        let time = if rng.chance(5) { f64::NAN } else { time };
        let (method, count, around) = *rng.pick(&[
            ("setTime", 1, time),
            ("setDate", 1, fields[2]),
            ("setMonth", 1, fields[1]),
            ("setMonth", 2, fields[1]),
            ("setFullYear", 1, fields[0]),
            ("setFullYear", 2, fields[0]),
            ("setFullYear", 3, fields[0]),
            ("setHours", 1, fields[4]),
            ("setHours", 4, fields[4]),
            ("setHours", 2, fields[4]),
        ]);
        let around = if around.is_nan() { 0.0 } else { around };
        let args: Vec<f64> =
            (0..count).map(|i| if i == 0 { argument(&mut rng, around) } else { argument(&mut rng, 10.0) }).collect();
        let list: Vec<Value> = args.iter().map(|&a| time_json(a)).collect();
        tally.push(
            json!({ "fn": "set", "time": bits(time), "method": method, "args": list }),
            time_json(probe::set(time, method, &args)),
        );
    }
    tally.check("Date constructors, setters and getters");
}

/// Format tokens dayjs reads and writes, and text around them.
const FORMAT_PIECES: &[&str] = &[
    "YYYY", "YY", "Y", "M", "MM", "MMM", "MMMM", "D", "DD", "Do", "d", "dd", "ddd", "dddd", "H", "HH", "h", "hh", "m",
    "mm", "s", "ss", "S", "SS", "SSS", "a", "A", "Z", "ZZ", "Q", "X", "x", "k", "kk", "W", "WW", "GGGG", "w", "ww",
    "wo", "gggg", "z", "zzz", "L", "l", "-", "/", ":", ".", ",", " ", "T", "[at]", "[", "]", "[x]y]", "q", "(", ")",
    "_", "é",
];

fn random_template(rng: &mut Rng) -> String {
    match rng.below(10) {
        0 => (*rng.pick(&[
            "YYYY-MM-DD",
            "x",
            "X",
            "",
            "YYYY-MM-DD HH:mm",
            "DD.MM.YYYY",
            "MMM D, YYYY",
            "Do MMMM YYYY",
            "h:mm a",
            "HH:mm",
        ]))
        .to_owned(),
        _ => (0..1 + rng.below(6)).map(|_| *rng.pick(FORMAT_PIECES)).collect(),
    }
}

/// An input for a template: one formatted from a date, changed a little, or made of pieces.
fn input_for(rng: &mut Rng, template: &str, time: f64) -> String {
    let formatted = probe::format(time, template).unwrap_or_default();
    match rng.below(6) {
        0 => random_date_string(rng),
        1 if !formatted.is_empty() => {
            let mut units: Vec<char> = formatted.chars().collect();
            let at = rng.below(units.len());
            match rng.below(3) {
                0 => {
                    units.remove(at);
                }
                1 => units.insert(at, *rng.pick(&['0', '1', '9', ' ', '-', 'x'])),
                _ => units[at] = *rng.pick(&['0', '1', '9', ' ', '-', 'x']),
            }
            units.into_iter().collect()
        }
        _ => formatted,
    }
}

#[test]
fn strict_parsing_and_formatting_match_pele() {
    let mut tally = Tally::default();
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let now = 1_768_478_400_000.0;
    for _ in 0..12_000 {
        let template = random_template(&mut rng);
        let time = if rng.chance(70) {
            days_from_civil(rng.int(1, 9999) as i64, rng.int(1, 12) as i64, rng.int(1, 28) as i64)
                + rng.int(0, 86_399_999)
        } else {
            interesting_time(&mut rng)
        };
        let time = if rng.chance(3) { f64::NAN } else { time };
        tally.push(
            json!({ "fn": "format", "time": bits(time), "template": template }),
            probe::format(time, &template).map_or(Value::Null, Value::String),
        );
        let input = input_for(&mut rng, &template, time);
        let now = if rng.chance(20) { interesting_time(&mut rng) } else { now };
        tally.push(
            json!({ "fn": "parse", "input": input, "template": template, "now": bits(now) }),
            probe::parse_strict(&input, &template, now).map_or(Value::Null, time_json),
        );
    }
    let accepted = tally.answered("parse", &Value::Null);
    println!("{accepted} of the strict parses accept their input");
    assert!(accepted > 2000, "the inputs reach dayjs's accepting path");
    tally.check("strict parses and formats");
}

#[test]
fn calendar_arithmetic_matches_pele() {
    let mut tally = Tally::default();
    let mut rng = Rng(0x5851_f42d_4c95_7f2d);
    for _ in 0..8000 {
        let time = interesting_time(&mut rng);
        let unit = *rng.pick(&["ms", "s", "m", "h", "d", "w", "M", "y"]);
        let amount = match rng.below(6) {
            0 => rng.int(-1000, 1000) / 4.0,
            1 => rng.int(-100_000_000, 100_000_000),
            2 => *rng.pick(&[0.5, 1.5, 2.5, -0.5, 1e20, 0.0, 12.0, 13.0, 1e8]),
            _ => rng.int(-30, 400),
        };
        tally.push(
            json!({ "fn": "add", "time": bits(time), "amount": bits(amount), "unit": unit }),
            time_json(probe::add(time, amount, unit)),
        );
        tally.push(json!({ "fn": "iso", "time": bits(time) }), Value::String(probe::iso_date(time)));
    }
    let durations = [
        "3d",
        "1.5w",
        " 2h ",
        "10ms",
        "1M",
        "2y",
        "5m",
        "5s",
        "1.d",
        "d",
        "1.5.5d",
        "07d",
        "1e3d",
        "12",
        "1 d",
        "x",
        "",
        "0.0000001ms",
        "99999999999999999999d",
    ];
    for _ in 0..2000 {
        let text: String = if rng.chance(50) {
            (*rng.pick(&durations)).to_owned()
        } else {
            (0..1 + rng.below(4))
                .map(|_| *rng.pick(&["1", "2", "0", ".", "5", "d", "w", "M", "ms", "m", "s", "y", "h", " ", "x"]))
                .collect()
        };
        let (amount, unit) = probe::parse_duration(&text);
        tally.push(json!({ "fn": "duration", "text": text }), json!([bits(amount), unit]));
    }
    tally.check("added dates, ISO dates and durations");
}

/// A span a chart might cover, from milliseconds to centuries.
fn span(rng: &mut Rng) -> (f64, f64) {
    let start = if rng.chance(80) {
        days_from_civil(rng.int(1900, 2100) as i64, rng.int(1, 12) as i64, rng.int(1, 28) as i64)
            + rng.int(0, 86_399_999)
    } else {
        interesting_time(&mut *rng)
    };
    let length = match rng.below(8) {
        0 => rng.int(0, 5000),
        1 => rng.int(0, 600_000),
        2 => rng.int(0, 86_400_000),
        3 => rng.int(0, 40) * DAY,
        4 => rng.int(0, 800) * DAY,
        5 => rng.int(0, 40_000) * DAY,
        6 => rng.int(0, 10_000_000) * DAY,
        _ => rng.int(0, 120) * DAY + rng.int(0, 86_399_999),
    };
    (start, js_min(start + length, MAX_TIME))
}

fn js_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

#[test]
fn ticks_match_d3() {
    let mut tally = Tally::default();
    let mut rng = Rng(0x1405_7b7e_f767_814f);
    for _ in 0..4000 {
        let (start, stop) = span(&mut rng);
        let count = rng.int(2, 10);
        let ours: Vec<Value> = probe::auto_ticks(start, stop, count).into_iter().map(time_json).collect();
        tally.push(
            json!({ "fn": "auto", "start": bits(start), "stop": bits(stop), "count": bits(count) }),
            Value::Array(ours),
        );
        let step = match rng.below(5) {
            0 => rng.int(1, 3).to_string(),
            1 => rng.int(1, 100).to_string(),
            2 => (*rng.pick(&["0", "01", "99999999", "123456789012345678901234567890", ""])).to_owned(),
            _ => "1".to_owned(),
        };
        let unit =
            *rng.pick(&["millisecond", "second", "minute", "hour", "day", "week", "month", "year", "days", "Day"]);
        let interval = format!("{step}{unit}");
        let weekday =
            *rng.pick(&["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "Monday", "x"]);
        let limit = *rng.pick(&[80.0, 107.0, 214.0, 300.0, 5.0, 1.0]);
        let ours = probe::interval_ticks(&interval, weekday, start, stop, limit)
            .map_or(Value::Null, |ticks| Value::Array(ticks.into_iter().map(time_json).collect()));
        tally.push(
            json!({ "fn": "interval", "interval": interval, "weekday": weekday, "start": bits(start), "stop": bits(stop), "limit": bits(limit) }),
            ours,
        );
    }
    let (auto, interval) = (tally.answered("auto", &json!([])), tally.answered("interval", &Value::Null));
    println!("{auto} automatic tick lists hold ticks, {interval} intervals give ticks");
    assert!(auto > 2000 && interval > 500, "the spans reach d3's tick paths");
    tally.check("auto and interval ticks");
}

const DIRECTIVES: &str = "aAbBcdefgGHIjLmMpqQsSuUVwWxXyYZ%k";

#[test]
fn time_formats_match_d3() {
    let mut tally = Tally::default();
    let mut rng = Rng(0x3c6e_f372_fe94_f82b);
    let directives: Vec<char> = DIRECTIVES.chars().collect();
    for _ in 0..8000 {
        let mut specifier = String::new();
        for _ in 0..1 + rng.below(5) {
            match rng.below(8) {
                0 => specifier.push_str(rng.pick(&[" ", "-", "/", "x", "é", "%"])),
                _ => {
                    specifier.push('%');
                    if rng.chance(25) {
                        specifier.push(*rng.pick(&['-', '_', '0']));
                    }
                    specifier.push(*rng.pick(&directives));
                }
            }
        }
        // d3 reads month and weekday names by index, which an invalid date has none of.
        let time = interesting_time(&mut rng);
        let time = if probe::fields(time)[0].is_nan() { 0.0 } else { time };
        tally.push(
            json!({ "fn": "timeFormat", "specifier": specifier, "time": bits(time) }),
            Value::String(probe::time_format(&specifier, time)),
        );
    }
    tally.check("d3 time formats");
}
