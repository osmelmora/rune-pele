//! Pele's gantt `tokenize`: the token types of Mermaid's `gantt.jison`, scanned by hand.

use crate::input::{is_space, is_word};

/// Pele's `T`, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum T {
    End,
    Error,
    OpenDirective,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Nl,
    Href,
    Callbackname,
    Callbackargs,
    Click,
    Gantt,
    DateFormat,
    InclusiveEndDates,
    TopAxis,
    AxisFormat,
    TickInterval,
    Includes,
    Excludes,
    TodayMarker,
    /// `weekday` and a day, from 0 for Monday.
    Weekday(u8),
    WeekendFriday,
    WeekendSaturday,
    Date,
    Title,
    AccDescription,
    Section,
    TaskTxt,
    TaskData,
    Colon,
    Eof,
}

impl T {
    /// Pele's `TOKEN_NAMES`.
    pub(super) fn name(self) -> &'static str {
        match self {
            T::End => "$end",
            T::Error => "INVALID",
            T::OpenDirective => "open_directive",
            T::AccTitle => "acc_title",
            T::AccTitleValue => "acc_title_value",
            T::AccDescr => "acc_descr",
            T::AccDescrValue => "acc_descr_value",
            T::AccDescrMultilineValue => "acc_descr_multiline_value",
            T::Nl => "NL",
            T::Href => "href",
            T::Callbackname => "callbackname",
            T::Callbackargs => "callbackargs",
            T::Click => "click",
            T::Gantt => "gantt",
            T::DateFormat => "dateFormat",
            T::InclusiveEndDates => "inclusiveEndDates",
            T::TopAxis => "topAxis",
            T::AxisFormat => "axisFormat",
            T::TickInterval => "tickInterval",
            T::Includes => "includes",
            T::Excludes => "excludes",
            T::TodayMarker => "todayMarker",
            T::Weekday(day) => [
                "weekday_monday",
                "weekday_tuesday",
                "weekday_wednesday",
                "weekday_thursday",
                "weekday_friday",
                "weekday_saturday",
                "weekday_sunday",
            ][usize::from(day)],
            T::WeekendFriday => "weekend_friday",
            T::WeekendSaturday => "weekend_saturday",
            T::Date => "date",
            T::Title => "title",
            T::AccDescription => "accDescription",
            T::Section => "section",
            T::TaskTxt => "taskTxt",
            T::TaskData => "taskData",
            T::Colon => ":",
            T::Eof => "EOF",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) kind: T,
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Href,
    Callbackname,
    Callbackargs,
    Click,
}

const DAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/// `word` at `at`, ignoring ASCII case, as the `i` flag reads a keyword.
fn word_at(src: &[u16], at: usize, word: &str) -> Option<usize> {
    let end = at + word.len();
    let found = src.get(at..end)?;
    found
        .iter()
        .zip(word.bytes())
        .all(|(&c, b)| u8::try_from(c).is_ok_and(|c| c.eq_ignore_ascii_case(&b)))
        .then_some(end)
}

fn run(src: &[u16], at: usize, keep: impl Fn(u16) -> bool) -> usize {
    at + src[at.min(src.len())..].iter().take_while(|&&c| keep(c)).count()
}

/// `\b` after a word: what follows is no word character.
fn boundary(src: &[u16], at: usize) -> bool {
    src.get(at).is_none_or(|&c| !is_word(c))
}

/// `[^\S\r\n]`: white space on the line.
fn is_inline_space(c: u16) -> bool {
    is_space(c) && c != 10 && c != 13
}

/// How a keyword statement goes on after its word.
#[derive(Clone, Copy)]
enum Rest {
    /// `\b`
    Boundary,
    /// `\s[^#\n;]+`
    Value,
    /// `\s[^\n;]+`
    ValueWithHash,
    /// `\s[^\n]+`
    Line,
    /// `[^\S\r\n][^\n]+`
    TitleLine,
}

/// The keyword statements, grouped by the keyword's first letter and kept in the grammar's order.
const KEYWORDS: &[(u8, &str, Rest, T)] = &[
    (b'a', "axisFormat", Rest::Value, T::AxisFormat),
    (b'a', "accDescription", Rest::Value, T::AccDescription),
    (b'g', "gantt", Rest::Boundary, T::Gantt),
    (b'd', "dateFormat", Rest::Value, T::DateFormat),
    (b'i', "inclusiveEndDates", Rest::Boundary, T::InclusiveEndDates),
    (b'i', "includes", Rest::Value, T::Includes),
    (b't', "topAxis", Rest::Boundary, T::TopAxis),
    (b't', "tickInterval", Rest::Value, T::TickInterval),
    (b't', "todayMarker", Rest::ValueWithHash, T::TodayMarker),
    (b't', "title", Rest::TitleLine, T::Title),
    (b'e', "excludes", Rest::Value, T::Excludes),
    (b's', "section", Rest::Line, T::Section),
];

fn keyword_end(src: &[u16], at: usize, word: &str, rest: Rest) -> Option<usize> {
    let end = word_at(src, at, word)?;
    let value = |end: usize, first: fn(u16) -> bool, keep: fn(u16) -> bool| {
        let first_ok = src.get(end).copied().is_some_and(first);
        let value_end = run(src, end + 1, keep);
        (first_ok && value_end > end + 1).then_some(value_end)
    };
    match rest {
        Rest::Boundary => boundary(src, end).then_some(end),
        Rest::Value => value(end, is_space, |c| !matches!(c, 35 | 10 | 59)),
        Rest::ValueWithHash => value(end, is_space, |c| !matches!(c, 10 | 59)),
        Rest::Line => value(end, is_space, |c| c != 10),
        Rest::TitleLine => value(end, is_inline_space, |c| c != 10),
    }
}

/// `word\s*<close>\s*`, as `accTitle\s*:\s*` and its kin read.
fn opener(src: &[u16], at: usize, word: &str, close: u16) -> Option<usize> {
    let end = run(src, word_at(src, at, word)?, is_space);
    (src.get(end) == Some(&close)).then(|| run(src, end + 1, is_space))
}

pub(super) fn tokenize(src: &[u16]) -> Vec<Token> {
    let n = src.len();
    let mut tokens = Vec::new();
    let mut state = State::Initial;
    let mut p = 0;
    let until = |ch: u16, from: usize| src[from.min(n)..].iter().position(|&c| c == ch).map_or(n, |at| from + at);

    loop {
        if p >= n {
            let kind = match state {
                State::Initial => T::Eof,
                State::AccTitle => T::AccTitleValue,
                State::AccDescr => T::AccDescrValue,
                State::AccDescrMultiline => T::AccDescrMultilineValue,
                State::Href => T::Href,
                State::Callbackname => T::Callbackname,
                State::Callbackargs => T::Callbackargs,
                State::Click => T::Click,
            };
            tokens.push(Token { kind, start: n, end: n });
            break;
        }
        let c = src[p];
        let mut emit = |kind: T, start: usize, end: usize, p: &mut usize| {
            tokens.push(Token { kind, start, end });
            *p = end;
        };
        match state {
            State::Initial => {
                let d = src.get(p + 1).copied();
                if c == 37 {
                    if d != Some(37) {
                        let mut e = until(10, p + 1);
                        while src.get(e) == Some(&10) {
                            e += 1;
                        }
                        p = e;
                    } else if src.get(p + 2) == Some(&123) {
                        // The grammar enters a lexer state it never defines, so nothing can follow.
                        emit(T::OpenDirective, p, p + 3, &mut p);
                        tokens.push(Token { kind: T::Error, start: p, end: p });
                        return tokens;
                    } else {
                        p = until(10, p + 2);
                    }
                } else if c == 10 {
                    let e = run(src, p + 1, |c| c == 10);
                    emit(T::Nl, p, e, &mut p);
                } else if is_space(c) {
                    p = run(src, p + 1, is_space);
                } else if let Some((kind, end, next)) = keyword(src, p) {
                    state = next.unwrap_or(state);
                    match kind {
                        Some(kind) => emit(kind, p, end, &mut p),
                        None => p = end,
                    }
                } else if (48..=57).contains(&c) && is_date(src, p) {
                    emit(T::Date, p, p + 10, &mut p);
                } else if c != 58 {
                    let e = run(src, p + 1, |c| c != 58 && c != 10);
                    emit(T::TaskTxt, p, e, &mut p);
                } else {
                    let e = run(src, p + 1, |c| !matches!(c, 35 | 10 | 59));
                    emit(if e > p + 1 { T::TaskData } else { T::Colon }, p, e, &mut p);
                }
            }
            State::AccTitle | State::AccDescr => {
                let kind = if state == State::AccTitle { T::AccTitleValue } else { T::AccDescrValue };
                state = State::Initial;
                emit(kind, p, until(10, p), &mut p);
            }
            State::AccDescrMultiline => {
                if c == 125 {
                    state = State::Initial;
                    p += 1;
                } else {
                    emit(T::AccDescrMultilineValue, p, until(125, p), &mut p);
                }
            }
            State::Href => {
                if c == 34 {
                    state = State::Initial;
                    p += 1;
                } else {
                    emit(T::Href, p, until(34, p), &mut p);
                }
            }
            State::Callbackname => {
                if c == 40 {
                    let inner = run(src, p + 1, is_space);
                    if src.get(inner) == Some(&41) {
                        state = State::Initial;
                        p = inner + 1;
                    } else {
                        state = State::Callbackargs;
                        p += 1;
                    }
                } else {
                    emit(T::Callbackname, p, until(40, p), &mut p);
                }
            }
            State::Callbackargs => {
                if c == 41 {
                    state = State::Initial;
                    p += 1;
                } else {
                    emit(T::Callbackargs, p, until(41, p), &mut p);
                }
            }
            State::Click => {
                if is_space(c) {
                    state = State::Initial;
                    p += 1;
                } else {
                    let e = run(src, p + 1, |c| !is_space(c));
                    emit(T::Click, p, e, &mut p);
                }
            }
        }
    }
    tokens.push(Token { kind: T::End, start: n, end: n });
    tokens
}

/// `\d\d\d\d-\d\d-\d\d\b`.
fn is_date(src: &[u16], p: usize) -> bool {
    let digit = |at: usize| src.get(at).is_some_and(|&c| (48..=57).contains(&c));
    let dash = |at: usize| src.get(at) == Some(&45);
    (p..p + 4).all(digit)
        && dash(p + 4)
        && digit(p + 5)
        && digit(p + 6)
        && dash(p + 7)
        && digit(p + 8)
        && digit(p + 9)
        && boundary(src, p + 10)
}

/// A statement that starts with a keyword at `p`: the token it emits, if any, where it ends, and
/// the state the lexer moves to, if it moves.
fn keyword(src: &[u16], p: usize) -> Option<(Option<T>, usize, Option<State>)> {
    let c = src[p] | 32;
    match c {
        97 => {
            if let Some(end) = opener(src, p, "accTitle", 58) {
                return Some((Some(T::AccTitle), end, Some(State::AccTitle)));
            }
            if let Some(end) = opener(src, p, "accDescr", 58) {
                return Some((Some(T::AccDescr), end, Some(State::AccDescr)));
            }
            if let Some(end) = opener(src, p, "accDescr", 123) {
                return Some((None, end, Some(State::AccDescrMultiline)));
            }
        }
        104 => {
            if let Some(end) = word_at(src, p, "href") {
                let spaces = run(src, end, is_space);
                if spaces > end && src.get(spaces) == Some(&34) {
                    return Some((None, spaces + 1, Some(State::Href)));
                }
            }
        }
        99 => {
            for (word, next) in [("call", State::Callbackname), ("click", State::Click)] {
                if let Some(end) = word_at(src, p, word) {
                    let spaces = run(src, end, is_space);
                    if spaces > end {
                        return Some((None, spaces, Some(next)));
                    }
                }
            }
        }
        116 => {
            if let Some(end) = word_at(src, p, "title") {
                let spaces = run(src, end, is_inline_space);
                if src.get(spaces).is_none_or(|&c| c == 13 || c == 10) {
                    return Some((None, spaces, None));
                }
            }
        }
        119 => {
            if let Some(end) = word_at(src, p, "weekday") {
                let spaces = run(src, end, is_space);
                if spaces > end {
                    for (day, name) in DAYS.iter().enumerate() {
                        if let Some(end) = word_at(src, spaces, name).filter(|&end| boundary(src, end)) {
                            return Some((Some(T::Weekday(day as u8)), end, None));
                        }
                    }
                }
            }
            if let Some(end) = word_at(src, p, "weekend") {
                let spaces = run(src, end, is_space);
                if spaces > end {
                    for (name, kind) in [("friday", T::WeekendFriday), ("saturday", T::WeekendSaturday)] {
                        if let Some(end) = word_at(src, spaces, name).filter(|&end| boundary(src, end)) {
                            return Some((Some(kind), end, None));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    KEYWORDS
        .iter()
        .filter(|(first, ..)| u16::from(*first) == c)
        .find_map(|&(_, word, rest, kind)| keyword_end(src, p, word, rest).map(|end| (Some(kind), end, None)))
}
