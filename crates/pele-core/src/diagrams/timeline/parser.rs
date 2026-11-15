//! Pele's timeline lexer and parser (`timeline/lexer.ts`, `parser.ts`), which follow the parser
//! Mermaid generates from `timeline.jison`: the same tokens, the same calls into the model and
//! the same `Expecting …` line on a syntax error.

use super::{Direction, TimelineModel};
use crate::diagrams::scan::{AccPart, accessibility, keyword_at, line_end, skip_trivia};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units, is_space, is_word};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    End,
    Eof,
    Invalid,
    Newline,
    Timeline,
    TimelineLr,
    TimelineTd,
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Section,
    Period,
    Event,
}

impl Kind {
    /// Pele's `TOKEN_NAMES`.
    fn name(self) -> &'static str {
        match self {
            Kind::End => "$end",
            Kind::Eof => "EOF",
            Kind::Invalid => "INVALID",
            Kind::Newline => "NEWLINE",
            Kind::Timeline => "timeline",
            Kind::TimelineLr => "timeline_lr",
            Kind::TimelineTd => "timeline_td",
            Kind::Title => "title",
            Kind::AccTitle => "acc_title",
            Kind::AccTitleValue => "acc_title_value",
            Kind::AccDescr => "acc_descr",
            Kind::AccDescrValue => "acc_descr_value",
            Kind::AccDescrMultilineValue => "acc_descr_multiline_value",
            Kind::Section => "section",
            Kind::Period => "period",
            Kind::Event => "event",
        }
    }

    fn of(part: AccPart) -> Kind {
        match part {
            AccPart::Title => Kind::AccTitle,
            AccPart::TitleValue => Kind::AccTitleValue,
            AccPart::Descr => Kind::AccDescr,
            AccPart::DescrValue => Kind::AccDescrValue,
            AccPart::DescrMultilineValue => Kind::AccDescrMultilineValue,
        }
    }
}

struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// Pele's `tokenize`.
fn tokenize(src: &[u16]) -> Vec<Token> {
    let n = src.len();
    let mut out: Vec<Token> = Vec::new();
    let word_at = |at: usize| src.at(at).is_some_and(is_word);
    let space_at = |at: usize| src.at(at).is_some_and(is_space);
    let mut p = 0;
    loop {
        p = skip_trivia(src, p);
        if p >= n {
            break;
        }
        let c = src[p];
        let mut emit = |kind: Kind, end: usize, p: &mut usize| {
            out.push(Token { kind, start: *p, end });
            *p = end;
        };
        if c == 10 {
            let e = src.run(p + 1, |c| c == 10);
            emit(Kind::Newline, e, &mut p);
            continue;
        }
        if keyword_at(src, p, "timeline") {
            let e = src.run(p + 8, |c| c == 32 || c == 9);
            let lr = keyword_at(src, e, "lr");
            if e > p + 8 && (lr || keyword_at(src, e, "td")) && !word_at(e + 2) {
                emit(if lr { Kind::TimelineLr } else { Kind::TimelineTd }, e + 2, &mut p);
                continue;
            }
            if !word_at(p + 8) {
                emit(Kind::Timeline, p + 8, &mut p);
                continue;
            }
        }
        if keyword_at(src, p, "title") {
            // Unlike Mermaid, ignore bare titles.
            let e = src.run(p + 5, |c| c != 10 && is_space(c));
            if e == n || src[e] == 10 {
                p = e;
                continue;
            }
            let sep = src[p + 5];
            if sep != 13 && is_space(sep) {
                emit(Kind::Title, line_end(src, p + 6), &mut p);
                continue;
            }
        }
        let e = accessibility(src, p, |part, start, end| out.push(Token { kind: Kind::of(part), start, end }));
        if e != p {
            p = e;
            continue;
        }
        let mut emit = |kind: Kind, end: usize, p: &mut usize| {
            out.push(Token { kind, start: *p, end });
            *p = end;
        };
        // Like Mermaid, a bare section uses the next line as its name.
        if keyword_at(src, p, "section") && space_at(p + 7) {
            let e = src.run(p + 8, |c| c != 58 && c != 10);
            if e > p + 8 {
                emit(Kind::Section, e, &mut p);
                continue;
            }
        }
        if c != 58 {
            let e = src.run(p + 1, |k| k != 35 && k != 58 && k != 10);
            emit(Kind::Period, e, &mut p);
            continue;
        }
        // An event runs to the end of the line or to a colon that is followed by whitespace.
        if space_at(p + 1) {
            let mut e = p + 2;
            while e < n && !(src[e] == 10 || (src[e] == 58 && space_at(e + 1))) {
                e += 1;
            }
            if e > p + 2 {
                emit(Kind::Event, e, &mut p);
                continue;
            }
        }
        emit(Kind::Invalid, p + 1, &mut p);
    }
    // Jison marks its lexer done when a rule runs on empty input, so an accessibility value that
    // matches nothing at the very end of the text is followed by no EOF token.
    if out.last().is_none_or(|token| token.start < n) {
        out.push(Token { kind: Kind::Eof, start: p, end: n });
    }
    out.push(Token { kind: Kind::End, start: p, end: n });
    out
}

/// Why a source is not a timeline.
pub(crate) enum Failure {
    Syntax(SyntaxError),
    Semantic(&'static str),
}

const STATEMENT: &str = "'EOF', 'SPACE', 'NEWLINE', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'section', 'period', 'event'";

/// Pele's `parseTimeline`, into the model `db`.
pub(crate) fn parse(src: &[u16], db: &mut TimelineModel) -> Result<(), Failure> {
    let tokens = tokenize(src);
    let text = |i: usize| &src[tokens[i].start..tokens[i].end];
    let fail = |i: usize, expected: &str| {
        let got = if tokens[i].kind == Kind::End { "1" } else { tokens[i].kind.name() };
        Failure::Syntax(SyntaxError {
            at: tokens[i].start,
            detail: format!("Expecting {expected}, got '{got}'"),
            lexical: false,
        })
    };
    let from = |text: &[u16], at: usize| -> JsText { text.get(at..).unwrap_or_default().into() };

    match tokens[0].kind {
        Kind::Timeline => {}
        Kind::TimelineLr => db.direction = Direction::Across,
        Kind::TimelineTd => db.direction = Direction::Down,
        _ => return Err(fail(0, "'timeline', 'timeline_lr', 'timeline_td'")),
    }
    let mut i = 1;
    loop {
        match tokens[i].kind {
            Kind::Newline => {}
            Kind::Title => db.title = Some(from(text(i), 6)),
            Kind::AccTitle => {
                i += 1;
                db.set_acc_title(text(i).trim_js());
            }
            Kind::AccDescr => {
                i += 1;
                db.set_acc_description(text(i).trim_js());
            }
            Kind::AccDescrMultilineValue => db.set_acc_description(text(i).trim_js()),
            Kind::Section => db.add_section(from(text(i), 8)),
            Kind::Period => db.add_task(text(i).into()),
            Kind::Event => db.add_event(from(text(i), 2)).map_err(Failure::Semantic)?,
            Kind::Eof => return Ok(()),
            _ => return Err(fail(i, STATEMENT)),
        }
        i += 1;
    }
}
