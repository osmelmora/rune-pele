//! JavaScript's `Date` as V8 14.6 implements it (`src/date/date.cc`, `src/builtins/builtins-date.cc`
//! and `src/date/dateparser*`), for the time zone Pele's parity runs use, UTC. Local time and UTC
//! are then the same, so every getter and setter reads one time value.
//!
//! A date is its time value: whole milliseconds since 1970 within ±8.64e15, or NaN.

const MS_PER_SECOND: f64 = 1000.0;
const MS_PER_MINUTE: f64 = 60_000.0;
const MS_PER_HOUR: f64 = 3_600_000.0;
const MS_PER_DAY: i64 = 86_400_000;
const MAX_TIME: f64 = 8.64e15;
/// V8's `kMaxTimeBeforeUTCInMs`: a local time this far out still converts, then clips.
const MAX_TIME_BEFORE_UTC: f64 = 8.64e15 + 2_592_000_000.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct JsDate(f64);

/// V8's `DoubleToInteger`.
fn integer(x: f64) -> f64 {
    if x.is_nan() {
        0.0
    } else if x.is_infinite() {
        x
    } else {
        x.trunc()
    }
}

/// V8's `TryTimeClip`, with NaN for a value it clips.
fn time_clip(time: f64) -> f64 {
    if (-MAX_TIME..=MAX_TIME).contains(&time) { time.trunc() + 0.0 } else { f64::NAN }
}

/// V8's `MakeDay`.
fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !((-1_000_000.0..=1_000_000.0).contains(&year)
        && (-10_000_000.0..=10_000_000.0).contains(&month)
        && date.is_finite())
    {
        return f64::NAN;
    }
    let mut y = year as i64;
    let mut m = month as i64;
    y += m / 12;
    m %= 12;
    if m < 0 {
        m += 12;
        y -= 1;
    }
    const YEAR_DELTA: i64 = 399_999;
    let days_before =
        |y: i64| 365 * (y + YEAR_DELTA) + (y + YEAR_DELTA) / 4 - (y + YEAR_DELTA) / 100 + (y + YEAR_DELTA) / 400;
    let mut day_from_year = days_before(y) - days_before(1970);
    let common = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];
    let table = if y % 4 != 0 || (y % 100 == 0 && y % 400 != 0) { common } else { leap };
    day_from_year += table[m as usize];
    (day_from_year - 1) as f64 + integer(date)
}

/// V8's `MakeTime`.
fn make_time(hour: f64, minute: f64, second: f64, ms: f64) -> f64 {
    if !(hour.is_finite() && minute.is_finite() && second.is_finite() && ms.is_finite()) {
        return f64::NAN;
    }
    integer(hour) * MS_PER_HOUR + integer(minute) * MS_PER_MINUTE + integer(second) * MS_PER_SECOND + integer(ms)
}

/// V8's `MakeDate`.
fn make_date(day: f64, time: f64) -> f64 {
    if day.is_finite() && time.is_finite() { time + day * MS_PER_DAY as f64 } else { f64::NAN }
}

/// V8's `SetLocalDateValue`, in UTC: a local time as the value a date holds.
fn local_value(time: f64) -> f64 {
    if (-MAX_TIME_BEFORE_UTC..=MAX_TIME_BEFORE_UTC).contains(&time) { time_clip(time as i64 as f64) } else { f64::NAN }
}

/// The year of `new Date(y, …)` and `Date.UTC(y, …)`: a whole year from 0 to 99 is in the 1900s.
fn full_year(year: f64) -> f64 {
    if year.is_nan() {
        return year;
    }
    let whole = integer(year);
    if (0.0..=99.0).contains(&whole) { 1900.0 + whole } else { year }
}

/// The days since 1970 and the milliseconds into the day, for a valid time value.
fn split(time: f64) -> (i64, i64) {
    let ms = time as i64;
    let days = ms.div_euclid(MS_PER_DAY);
    (days, ms - days * MS_PER_DAY)
}

/// The year, the month from 0 and the day of the month from 1 of a day counted from 1970.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 2 } else { mp - 10 };
    let year = yoe + era * 400 + i64::from(month < 2);
    (year, month, day)
}

impl JsDate {
    /// `new Date(time)` for a number.
    pub(crate) fn from_time(time: f64) -> Self {
        JsDate(time_clip(time))
    }

    /// `new Date(year, month, date, hours, minutes, seconds, ms)`.
    pub(crate) fn local(year: f64, month: f64, date: f64, hours: f64, minutes: f64, seconds: f64, ms: f64) -> Self {
        let day = make_day(full_year(year), month, date);
        JsDate(local_value(make_date(day, make_time(hours, minutes, seconds, ms))))
    }

    /// `Date.UTC(year, month, date, hours, minutes, seconds, ms)`.
    pub(crate) fn utc(year: f64, month: f64, date: f64, hours: f64, minutes: f64, seconds: f64, ms: f64) -> f64 {
        let day = make_day(full_year(year), month, date);
        time_clip(make_date(day, make_time(hours, minutes, seconds, ms)))
    }

    /// `new Date(text)` for a string, read by V8's date parser.
    pub(crate) fn parse(text: &[u16]) -> Self {
        let Some(fields) = parser::parse(text) else {
            return JsDate(f64::NAN);
        };
        let day = make_day(fields.year, fields.month, fields.day);
        let mut date = make_date(day, make_time(fields.hour, fields.minute, fields.second, fields.millisecond));
        match fields.offset {
            None if (-MAX_TIME_BEFORE_UTC..=MAX_TIME_BEFORE_UTC).contains(&date) => date = date as i64 as f64,
            None => return JsDate(f64::NAN),
            Some(seconds) => date -= seconds * 1000.0,
        }
        JsDate(time_clip(date))
    }

    /// `getTime()`, which is also `+date`.
    pub(crate) fn time(self) -> f64 {
        self.0
    }

    pub(crate) fn is_valid(self) -> bool {
        !self.0.is_nan()
    }

    /// The days since 1970 and the milliseconds into the day, for a valid date.
    fn fields(self) -> Option<(i64, i64)> {
        (!self.0.is_nan()).then(|| split(self.0))
    }

    /// The year, the month from 0 and the day of the month, for a valid date.
    pub(crate) fn calendar(self) -> Option<(i64, i64, i64)> {
        self.fields().map(|(days, _)| civil(days))
    }

    fn in_day(self) -> Option<i64> {
        self.fields().map(|(_, in_day)| in_day)
    }

    pub(crate) fn full_year(self) -> f64 {
        self.calendar().map_or(f64::NAN, |(year, ..)| year as f64)
    }

    /// From 0.
    pub(crate) fn month(self) -> f64 {
        self.calendar().map_or(f64::NAN, |(_, month, _)| month as f64)
    }

    /// The day of the month, from 1.
    pub(crate) fn date(self) -> f64 {
        self.calendar().map_or(f64::NAN, |(.., day)| day as f64)
    }

    /// The day of the week, from 0 for Sunday.
    pub(crate) fn day(self) -> f64 {
        self.fields().map_or(f64::NAN, |(days, _)| (days + 4).rem_euclid(7) as f64)
    }

    pub(crate) fn hours(self) -> f64 {
        self.in_day().map_or(f64::NAN, |ms| (ms / 3_600_000) as f64)
    }

    pub(crate) fn minutes(self) -> f64 {
        self.in_day().map_or(f64::NAN, |ms| (ms / 60_000 % 60) as f64)
    }

    pub(crate) fn seconds(self) -> f64 {
        self.in_day().map_or(f64::NAN, |ms| (ms / 1000 % 60) as f64)
    }

    pub(crate) fn milliseconds(self) -> f64 {
        self.in_day().map_or(f64::NAN, |ms| (ms % 1000) as f64)
    }

    /// `getTimezoneOffset()`, which is 0 in UTC.
    pub(crate) fn timezone_offset(self) -> f64 {
        if self.is_valid() { 0.0 } else { f64::NAN }
    }

    pub(crate) fn set_time(&mut self, time: f64) {
        self.0 = time_clip(time);
    }

    pub(crate) fn set_date(&mut self, date: f64) {
        let Some((days, in_day)) = self.fields() else {
            return;
        };
        let (year, month, _) = civil(days);
        self.0 = local_value(make_date(make_day(year as f64, month as f64, date), in_day as f64));
    }

    /// `setMonth(month)`, or `setMonth(month, date)`.
    pub(crate) fn set_month(&mut self, month: f64, date: Option<f64>) {
        let Some((days, in_day)) = self.fields() else {
            return;
        };
        let (year, _, day) = civil(days);
        self.0 = local_value(make_date(make_day(year as f64, month, date.unwrap_or(day as f64)), in_day as f64));
    }

    /// `setFullYear(year)`, or with a month, or with a month and a date. An invalid date becomes
    /// the first of January of that year.
    pub(crate) fn set_full_year(&mut self, year: f64, month: Option<f64>, date: Option<f64>) {
        let (mut m, mut dt, mut in_day) = (0.0, 1.0, 0.0);
        if let Some((days, ms)) = self.fields() {
            let (_, month, day) = civil(days);
            (m, dt, in_day) = (month as f64, day as f64, ms as f64);
        }
        if let Some(month) = month {
            m = month;
            if let Some(date) = date {
                dt = date;
            }
        }
        self.0 = local_value(make_date(make_day(year, m, dt), in_day));
    }

    /// `setHours(hours)`, with as many of minutes, seconds and milliseconds as are given.
    pub(crate) fn set_hours(&mut self, hours: f64, minutes: Option<f64>, seconds: Option<f64>, ms: Option<f64>) {
        let Some((days, in_day)) = self.fields() else {
            return;
        };
        let time = make_time(
            hours,
            minutes.unwrap_or((in_day / 60_000 % 60) as f64),
            seconds.unwrap_or((in_day / 1000 % 60) as f64),
            ms.unwrap_or((in_day % 1000) as f64),
        );
        self.0 = local_value(make_date(days as f64, time));
    }
}

/// V8's legacy and ES5 date string parser.
mod parser {
    use crate::input::is_space;

    /// What a string gave, as `DateParser::Parse` fills its output: the month from 0, and the
    /// offset from UTC in seconds when the string names one.
    pub(super) struct Fields {
        pub(super) year: f64,
        pub(super) month: f64,
        pub(super) day: f64,
        pub(super) hour: f64,
        pub(super) minute: f64,
        pub(super) second: f64,
        pub(super) millisecond: f64,
        pub(super) offset: Option<f64>,
    }

    const NONE: i32 = i32::MAX;
    const MAX_SIGNIFICANT_DIGITS: usize = 9;
    const SMI_MAX: u32 = (1 << 30) - 1;

    fn between(x: i32, lo: i32, hi: i32) -> bool {
        (x.wrapping_sub(lo) as u32) <= (hi.wrapping_sub(lo) as u32)
    }

    /// `IsWhiteSpace`: white space that is no line terminator.
    fn is_white_space(c: u32) -> bool {
        u16::try_from(c).is_ok_and(|c| is_space(c) && !matches!(c, 10 | 13 | 0x2028 | 0x2029))
    }

    struct Reader<'a> {
        text: &'a [u16],
        index: usize,
        ch: u32,
    }

    impl<'a> Reader<'a> {
        fn new(text: &'a [u16]) -> Self {
            let mut reader = Reader { text, index: 0, ch: 0 };
            reader.next();
            reader
        }

        fn position(&self) -> usize {
            self.index
        }

        fn next(&mut self) {
            self.ch = self.text.get(self.index).map_or(0, |&c| u32::from(c));
            self.index += 1;
        }

        fn is_digit(&self) -> bool {
            (48..=57).contains(&self.ch)
        }

        fn is_alpha_or_above(&self) -> bool {
            self.ch >= 65
        }

        fn read_unsigned_numeral(&mut self) -> i32 {
            let mut n: i32 = 0;
            let mut i = 0;
            while self.ch == 48 {
                self.next();
            }
            while self.is_digit() {
                if i < MAX_SIGNIFICANT_DIGITS {
                    n = n * 10 + (self.ch - 48) as i32;
                }
                i += 1;
                self.next();
            }
            n
        }

        fn read_word(&mut self, prefix: &mut [u32; 3]) -> usize {
            let mut length = 0;
            while self.is_alpha_or_above() && !is_white_space(self.ch) {
                if length < 3 {
                    prefix[length] = self.ch | 0x20;
                }
                self.next();
                length += 1;
            }
            for slot in prefix.iter_mut().skip(length) {
                *slot = 0;
            }
            length
        }

        fn skip(&mut self, c: u32) -> bool {
            if self.ch == c {
                self.next();
                return true;
            }
            false
        }

        fn skip_white_space(&mut self) -> bool {
            if u16::try_from(self.ch).is_ok_and(is_space) {
                self.next();
                return true;
            }
            false
        }

        fn skip_parentheses(&mut self) -> bool {
            if self.ch != 40 {
                return false;
            }
            let mut balance = 0;
            loop {
                if self.ch == 41 {
                    balance -= 1;
                } else if self.ch == 40 {
                    balance += 1;
                }
                self.next();
                if !(balance > 0 && self.ch != 0) {
                    break;
                }
            }
            true
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Keyword {
        Invalid,
        MonthName,
        TimeZoneName,
        TimeSeparator,
        AmPm,
    }

    #[derive(Clone, Copy)]
    enum Token {
        Invalid,
        Unknown,
        WhiteSpace,
        Number { value: i32, length: usize },
        Symbol(u8),
        EndOfInput,
        Keyword { kind: Keyword, value: i32, length: usize },
    }

    impl Token {
        fn is_number(self) -> bool {
            matches!(self, Token::Number { .. })
        }

        fn is_fixed_length_number(self, length: usize) -> bool {
            matches!(self, Token::Number { length: l, .. } if l == length)
        }

        fn number(self) -> i32 {
            match self {
                Token::Number { value, .. } => value,
                _ => unreachable!("only a number token has a value"),
            }
        }

        fn is_symbol(self, symbol: u8) -> bool {
            matches!(self, Token::Symbol(s) if s == symbol)
        }

        fn is_sign(self) -> bool {
            self.is_symbol(b'-') || self.is_symbol(b'+')
        }

        fn sign(self) -> i32 {
            match self {
                Token::Symbol(s) => 44 - i32::from(s),
                _ => unreachable!("only a sign has a sign"),
            }
        }

        fn is_keyword(self, kind: Keyword) -> bool {
            matches!(self, Token::Keyword { kind: k, .. } if k == kind)
        }

        fn is_z(self) -> bool {
            matches!(self, Token::Keyword { kind: Keyword::TimeZoneName, value: 0, length: 1 })
        }
    }

    const KEYWORDS: [([u8; 3], Keyword, i32); 27] = [
        (*b"jan", Keyword::MonthName, 1),
        (*b"feb", Keyword::MonthName, 2),
        (*b"mar", Keyword::MonthName, 3),
        (*b"apr", Keyword::MonthName, 4),
        (*b"may", Keyword::MonthName, 5),
        (*b"jun", Keyword::MonthName, 6),
        (*b"jul", Keyword::MonthName, 7),
        (*b"aug", Keyword::MonthName, 8),
        (*b"sep", Keyword::MonthName, 9),
        (*b"oct", Keyword::MonthName, 10),
        (*b"nov", Keyword::MonthName, 11),
        (*b"dec", Keyword::MonthName, 12),
        (*b"am\0", Keyword::AmPm, 0),
        (*b"pm\0", Keyword::AmPm, 12),
        (*b"ut\0", Keyword::TimeZoneName, 0),
        (*b"utc", Keyword::TimeZoneName, 0),
        (*b"z\0\0", Keyword::TimeZoneName, 0),
        (*b"gmt", Keyword::TimeZoneName, 0),
        (*b"cdt", Keyword::TimeZoneName, -5),
        (*b"cst", Keyword::TimeZoneName, -6),
        (*b"edt", Keyword::TimeZoneName, -4),
        (*b"est", Keyword::TimeZoneName, -5),
        (*b"mdt", Keyword::TimeZoneName, -6),
        (*b"mst", Keyword::TimeZoneName, -7),
        (*b"pdt", Keyword::TimeZoneName, -7),
        (*b"pst", Keyword::TimeZoneName, -8),
        (*b"t\0\0", Keyword::TimeSeparator, 0),
    ];

    fn lookup(prefix: &[u32; 3], length: usize) -> (Keyword, i32) {
        KEYWORDS
            .iter()
            .find(|(word, kind, _)| {
                word.iter().zip(prefix).all(|(&a, &b)| u32::from(a) == b)
                    && (length <= 3 || *kind == Keyword::MonthName)
            })
            .map_or((Keyword::Invalid, 0), |&(_, kind, value)| (kind, value))
    }

    struct Tokenizer<'a> {
        input: Reader<'a>,
        next: Token,
    }

    impl<'a> Tokenizer<'a> {
        fn new(text: &'a [u16]) -> Self {
            let mut input = Reader::new(text);
            let next = scan(&mut input);
            Tokenizer { input, next }
        }

        fn next(&mut self) -> Token {
            let result = self.next;
            self.next = scan(&mut self.input);
            result
        }

        fn peek(&self) -> Token {
            self.next
        }

        fn skip_symbol(&mut self, symbol: u8) -> bool {
            if self.next.is_symbol(symbol) {
                self.next = scan(&mut self.input);
                return true;
            }
            false
        }
    }

    fn scan(input: &mut Reader<'_>) -> Token {
        let start = input.position();
        if input.ch == 0 {
            return Token::EndOfInput;
        }
        if input.is_digit() {
            let value = input.read_unsigned_numeral();
            return Token::Number { value, length: input.position() - start };
        }
        for symbol in *b":-+.)" {
            if input.skip(u32::from(symbol)) {
                return Token::Symbol(symbol);
            }
        }
        if input.is_alpha_or_above() && !is_white_space(input.ch) {
            let mut prefix = [0; 3];
            let length = input.read_word(&mut prefix);
            let (kind, value) = lookup(&prefix, length);
            return Token::Keyword { kind, value, length };
        }
        if input.skip_white_space() {
            return Token::WhiteSpace;
        }
        if input.skip_parentheses() {
            return Token::Unknown;
        }
        input.next();
        Token::Unknown
    }

    struct TimeZone {
        sign: i32,
        hour: i32,
        minute: i32,
    }

    impl TimeZone {
        fn set(&mut self, offset_in_hours: i32) {
            self.sign = if offset_in_hours < 0 { -1 } else { 1 };
            self.hour = offset_in_hours * self.sign;
            self.minute = 0;
        }

        fn set_sign(&mut self, sign: i32) {
            self.sign = if sign < 0 { -1 } else { 1 };
        }

        fn is_expecting(&self, n: i32) -> bool {
            self.hour != NONE && self.minute == NONE && between(n, 0, 59)
        }

        fn is_utc(&self) -> bool {
            self.hour == 0 && self.minute == 0
        }

        fn is_empty(&self) -> bool {
            self.hour == NONE
        }

        /// The offset in seconds, `None` where the string names no zone, or `Err` where the
        /// offset is out of range.
        fn write(&mut self) -> Result<Option<f64>, ()> {
            if self.sign == NONE {
                return Ok(None);
            }
            if self.hour == NONE {
                self.hour = 0;
            }
            if self.minute == NONE {
                self.minute = 0;
            }
            let total = (self.hour as u32).wrapping_mul(3600).wrapping_add((self.minute as u32).wrapping_mul(60));
            if total > SMI_MAX {
                return Err(());
            }
            let total = total as i32;
            Ok(Some(f64::from(if self.sign < 0 { -total } else { total })))
        }
    }

    struct Time {
        comp: [i32; 4],
        index: usize,
        hour_offset: i32,
    }

    impl Time {
        fn is_empty(&self) -> bool {
            self.index == 0
        }

        fn is_expecting(&self, n: i32) -> bool {
            (self.index == 1 && between(n, 0, 59))
                || (self.index == 2 && between(n, 0, 59))
                || (self.index == 3 && between(n, 0, 999))
        }

        fn add(&mut self, n: i32) -> bool {
            if self.index < 4 {
                self.comp[self.index] = n;
                self.index += 1;
                return true;
            }
            false
        }

        fn add_final(&mut self, n: i32) -> bool {
            if !self.add(n) {
                return false;
            }
            while self.index < 4 {
                self.comp[self.index] = 0;
                self.index += 1;
            }
            true
        }

        fn write(&mut self) -> Option<[i32; 4]> {
            while self.index < 4 {
                self.comp[self.index] = 0;
                self.index += 1;
            }
            let [mut hour, minute, second, millisecond] = self.comp;
            if self.hour_offset != NONE {
                if !between(hour, 0, 12) {
                    return None;
                }
                hour %= 12;
                hour += self.hour_offset;
            }
            if !(between(hour, 0, 23)
                && between(minute, 0, 59)
                && between(second, 0, 59)
                && between(millisecond, 0, 999))
                && (hour != 24 || minute != 0 || second != 0 || millisecond != 0)
            {
                return None;
            }
            Some([hour, minute, second, millisecond])
        }
    }

    struct Day {
        comp: [i32; 3],
        index: usize,
        named_month: i32,
        iso: bool,
    }

    impl Day {
        fn is_empty(&self) -> bool {
            self.index == 0
        }

        fn add(&mut self, n: i32) -> bool {
            if self.index < 3 {
                self.comp[self.index] = n;
                self.index += 1;
                return true;
            }
            false
        }

        /// The year, the month from 0 and the day.
        fn write(&mut self) -> Option<[i32; 3]> {
            if self.index < 1 {
                return None;
            }
            while self.index < 3 {
                self.comp[self.index] = 1;
                self.index += 1;
            }
            let is_day = |x: i32| between(x, 1, 31);
            let mut year = 0;
            let (month, day);
            if self.named_month == NONE {
                if self.iso || (self.index == 3 && !is_day(self.comp[0])) {
                    year = self.comp[0];
                    month = self.comp[1];
                    day = self.comp[2];
                } else {
                    month = self.comp[0];
                    day = self.comp[1];
                    if self.index == 3 {
                        year = self.comp[2];
                    }
                }
            } else {
                month = self.named_month;
                if self.index == 1 {
                    day = self.comp[0];
                } else if !is_day(self.comp[0]) {
                    year = self.comp[0];
                    day = self.comp[1];
                } else {
                    day = self.comp[0];
                    year = self.comp[1];
                }
            }
            if !self.iso {
                if between(year, 0, 49) {
                    year += 2000;
                } else if between(year, 50, 99) {
                    year += 1900;
                }
            }
            if !between(month, 1, 12) || !is_day(day) {
                return None;
            }
            Some([year, month - 1, day])
        }
    }

    /// `ReadMilliseconds`: the first three significant digits of a numeral.
    fn read_milliseconds(token: Token) -> i32 {
        let Token::Number { value, length } = token else {
            unreachable!("milliseconds are a number");
        };
        let mut number = value;
        let mut length = length;
        if length == 1 {
            number *= 100;
        } else if length == 2 {
            number *= 10;
        } else if length > 3 {
            length = length.min(MAX_SIGNIFICANT_DIGITS);
            let mut factor = 1;
            loop {
                factor *= 10;
                length -= 1;
                if length <= 3 {
                    break;
                }
            }
            number /= factor;
        }
        number
    }

    /// `ParseES5DateTime`: the token the legacy parser goes on from, `EndOfInput` when the
    /// string is a whole ES5 date, or `Invalid` when it cannot be a date at all.
    fn es5(scanner: &mut Tokenizer<'_>, day: &mut Day, time: &mut Time, tz: &mut TimeZone) -> Token {
        if scanner.peek().is_sign() {
            let sign_token = scanner.next();
            if !scanner.peek().is_fixed_length_number(6) {
                return sign_token;
            }
            let sign = sign_token.sign();
            let year = scanner.next().number();
            if sign < 0 && year == 0 {
                return sign_token;
            }
            day.add(sign * year);
        } else if scanner.peek().is_fixed_length_number(4) {
            day.add(scanner.next().number());
        } else {
            return scanner.next();
        }
        if scanner.skip_symbol(b'-') {
            if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().number(), 1, 12) {
                return scanner.next();
            }
            day.add(scanner.next().number());
            if scanner.skip_symbol(b'-') {
                if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().number(), 1, 31) {
                    return scanner.next();
                }
                day.add(scanner.next().number());
            }
        }
        if !scanner.peek().is_keyword(Keyword::TimeSeparator) {
            if !matches!(scanner.peek(), Token::EndOfInput) {
                return scanner.next();
            }
        } else {
            scanner.next();
            if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().number(), 0, 24) {
                return Token::Invalid;
            }
            let hour_is_24 = scanner.peek().number() == 24;
            time.add(scanner.next().number());
            if !scanner.skip_symbol(b':') {
                return Token::Invalid;
            }
            if !scanner.peek().is_fixed_length_number(2)
                || !between(scanner.peek().number(), 0, 59)
                || (hour_is_24 && scanner.peek().number() > 0)
            {
                return Token::Invalid;
            }
            time.add(scanner.next().number());
            if scanner.skip_symbol(b':') {
                if !scanner.peek().is_fixed_length_number(2)
                    || !between(scanner.peek().number(), 0, 59)
                    || (hour_is_24 && scanner.peek().number() > 0)
                {
                    return Token::Invalid;
                }
                time.add(scanner.next().number());
                if scanner.skip_symbol(b'.') {
                    if !scanner.peek().is_number() || (hour_is_24 && scanner.peek().number() > 0) {
                        return Token::Invalid;
                    }
                    time.add(read_milliseconds(scanner.next()));
                }
            }
            if scanner.peek().is_z() {
                scanner.next();
                tz.set(0);
            } else if scanner.peek().is_sign() {
                tz.set_sign(if scanner.next().is_symbol(b'+') { 1 } else { -1 });
                if scanner.peek().is_fixed_length_number(4) {
                    let hourmin = scanner.next().number();
                    let (hour, min) = (hourmin / 100, hourmin % 100);
                    if !between(hour, 0, 23) || !between(min, 0, 59) {
                        return Token::Invalid;
                    }
                    tz.hour = hour;
                    tz.minute = min;
                } else {
                    if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().number(), 0, 23) {
                        return Token::Invalid;
                    }
                    tz.hour = scanner.next().number();
                    if !scanner.skip_symbol(b':') {
                        return Token::Invalid;
                    }
                    if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().number(), 0, 59) {
                        return Token::Invalid;
                    }
                    tz.minute = scanner.next().number();
                }
            }
            if !matches!(scanner.peek(), Token::EndOfInput) {
                return Token::Invalid;
            }
        }
        if tz.is_empty() && time.is_empty() {
            tz.set(0);
        }
        day.iso = true;
        Token::EndOfInput
    }

    /// `DateParser::Parse`.
    pub(super) fn parse(text: &[u16]) -> Option<Fields> {
        let mut scanner = Tokenizer::new(text);
        let mut tz = TimeZone { sign: NONE, hour: NONE, minute: NONE };
        let mut time = Time { comp: [0; 4], index: 0, hour_offset: NONE };
        let mut day = Day { comp: [0; 3], index: 0, named_month: NONE, iso: false };

        let mut token = es5(&mut scanner, &mut day, &mut time, &mut tz);
        if matches!(token, Token::Invalid) {
            return None;
        }
        let mut has_read_number = !day.is_empty();
        loop {
            match token {
                Token::EndOfInput => break,
                Token::Number { value: n, .. } => {
                    has_read_number = true;
                    if scanner.skip_symbol(b':') {
                        if scanner.skip_symbol(b':') {
                            if !time.is_empty() {
                                return None;
                            }
                            time.add(n);
                            time.add(0);
                        } else {
                            if !time.add(n) {
                                return None;
                            }
                            if scanner.peek().is_symbol(b'.') {
                                scanner.next();
                            }
                        }
                    } else if scanner.skip_symbol(b'.') && time.is_expecting(n) {
                        time.add(n);
                        if !scanner.peek().is_number() {
                            return None;
                        }
                        let ms = read_milliseconds(scanner.next());
                        if ms < 0 {
                            return None;
                        }
                        time.add_final(ms);
                    } else if tz.is_expecting(n) {
                        tz.minute = n;
                    } else if time.is_expecting(n) {
                        time.add_final(n);
                        let peek = scanner.peek();
                        if !matches!(peek, Token::EndOfInput | Token::WhiteSpace) && !peek.is_z() && !peek.is_sign() {
                            return None;
                        }
                    } else {
                        if !day.add(n) {
                            return None;
                        }
                        scanner.skip_symbol(b'-');
                    }
                }
                Token::Keyword { kind, value, .. } => {
                    if kind == Keyword::AmPm && !time.is_empty() {
                        time.hour_offset = value;
                    } else if kind == Keyword::MonthName {
                        day.named_month = value;
                        scanner.skip_symbol(b'-');
                    } else if kind == Keyword::TimeZoneName && has_read_number {
                        tz.set(value);
                    } else {
                        if has_read_number || scanner.peek().is_number() {
                            return None;
                        }
                    }
                }
                _ if token.is_sign() && (tz.is_utc() || !time.is_empty()) => {
                    tz.set_sign(token.sign());
                    let (mut n, mut length) = (0, 0);
                    if scanner.peek().is_number() {
                        let next = scanner.next();
                        if let Token::Number { value, length: l } = next {
                            (n, length) = (value, l);
                        }
                    }
                    has_read_number = true;
                    if scanner.peek().is_symbol(b':') {
                        tz.hour = n;
                        tz.minute = NONE;
                    } else if length == 2 || length == 1 {
                        tz.hour = n;
                        tz.minute = 0;
                    } else if length == 4 || length == 3 {
                        tz.hour = n / 100;
                        tz.minute = n % 100;
                    } else {
                        return None;
                    }
                }
                _ if (token.is_sign() || token.is_symbol(b')')) && has_read_number => return None,
                _ => {}
            }
            token = scanner.next();
        }

        let [year, month, day] = day.write()?;
        let [hour, minute, second, millisecond] = time.write()?;
        let offset = tz.write().ok()?;
        Some(Fields {
            year: f64::from(year),
            month: f64::from(month),
            day: f64::from(day),
            hour: f64::from(hour),
            minute: f64::from(minute),
            second: f64::from(second),
            millisecond: f64::from(millisecond),
            offset,
        })
    }
}
