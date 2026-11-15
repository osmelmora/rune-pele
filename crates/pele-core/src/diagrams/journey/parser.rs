//! Pele's journey lexer and parser (`journey/lexer.ts`, `parser.ts`), which follow the parser
//! Mermaid generates from `journey.jison`: the same tokens, the same calls into the model and the
//! same `Expecting …` line on a syntax error.

use super::JourneyModel;
use crate::diagrams::scan::{AccPart, accessibility, keyword_at, skip_trivia};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{Units, is_space, is_word};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    End,
    Eof,
    Invalid,
    Newline,
    Journey,
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Section,
    TaskName,
    TaskData,
    Colon,
}

impl Kind {
    /// Pele's `TOKEN_NAMES`.
    fn name(self) -> &'static str {
        match self {
            Kind::End => "$end",
            Kind::Eof => "EOF",
            Kind::Invalid => "INVALID",
            Kind::Newline => "NEWLINE",
            Kind::Journey => "journey",
            Kind::Title => "title",
            Kind::AccTitle => "acc_title",
            Kind::AccTitleValue => "acc_title_value",
            Kind::AccDescr => "acc_descr",
            Kind::AccDescrValue => "acc_descr_value",
            Kind::AccDescrMultilineValue => "acc_descr_multiline_value",
            Kind::Section => "section",
            Kind::TaskName => "taskName",
            Kind::TaskData => "taskData",
            Kind::Colon => ":",
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

/// Pele's `textEnd`: `#`, a line break and `;` end every kind of text, and a colon also ends a
/// task name or a section.
fn text_end(src: &[u16], from: usize, colon: bool) -> usize {
    src.run(from, |c| !(c == 35 || c == 10 || c == 59 || (colon && c == 58)))
}

/// Pele's `tokenize`.
fn tokenize(src: &[u16]) -> Vec<Token> {
    let n = src.len();
    let mut out: Vec<Token> = Vec::new();
    let word_at = |at: usize| src.at(at).is_some_and(is_word);
    let space_at = |at: usize| src.at(at).is_some_and(is_space);
    let emit = |out: &mut Vec<Token>, kind: Kind, p: &mut usize, end: usize| {
        out.push(Token { kind, start: *p, end });
        *p = end;
    };
    let mut p = 0;
    loop {
        p = skip_trivia(src, p);
        if p >= n {
            break;
        }
        let c = src[p];
        if c == 10 {
            let e = src.run(p + 1, |c| c == 10);
            emit(&mut out, Kind::Newline, &mut p, e);
            continue;
        }
        if keyword_at(src, p, "journey") && !word_at(p + 7) {
            let e = p + 7;
            emit(&mut out, Kind::Journey, &mut p, e);
            continue;
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
                let e = text_end(src, p + 6, false);
                if e > p + 6 {
                    emit(&mut out, Kind::Title, &mut p, e);
                    continue;
                }
            }
        }
        let e = accessibility(src, p, |part, start, end| out.push(Token { kind: Kind::of(part), start, end }));
        if e != p {
            p = e;
            continue;
        }
        // Like Mermaid, a bare section uses the next line as its name.
        if keyword_at(src, p, "section") && space_at(p + 7) {
            let e = text_end(src, p + 8, true);
            if e > p + 8 {
                emit(&mut out, Kind::Section, &mut p, e);
                continue;
            }
        }
        if c == 58 {
            let e = text_end(src, p + 1, false);
            if e > p + 1 {
                emit(&mut out, Kind::TaskData, &mut p, e);
            } else {
                let e = p + 1;
                emit(&mut out, Kind::Colon, &mut p, e);
            }
        } else if c == 59 {
            let e = p + 1;
            emit(&mut out, Kind::Invalid, &mut p, e);
        } else {
            let e = text_end(src, p + 1, true);
            emit(&mut out, Kind::TaskName, &mut p, e);
        }
    }
    // Jison marks its lexer done when a rule runs on empty input, so an accessibility value that
    // matches nothing at the very end of the text is followed by no EOF token.
    if out.last().is_none_or(|token| token.start < n) {
        out.push(Token { kind: Kind::Eof, start: p, end: n });
    }
    out.push(Token { kind: Kind::End, start: p, end: n });
    out
}

const STATEMENT: &str =
    "'EOF', 'SPACE', 'NEWLINE', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'section', 'taskName'";

/// Pele's `parseJourney`, into the model `db`.
pub(crate) fn parse(src: &[u16], db: &mut JourneyModel) -> Result<(), SyntaxError> {
    let tokens = tokenize(src);
    let text = |i: usize| &src[tokens[i].start..tokens[i].end];
    let fail = |i: usize, expected: &str| {
        let got = if tokens[i].kind == Kind::End { "1" } else { tokens[i].kind.name() };
        SyntaxError { at: tokens[i].start, detail: format!("Expecting {expected}, got '{got}'"), lexical: false }
    };
    let from = |text: &[u16], at: usize| -> Vec<u16> { text.get(at..).unwrap_or_default().to_vec() };

    if tokens[0].kind != Kind::Journey {
        return Err(fail(0, "'journey'"));
    }
    let mut i = 1;
    loop {
        match tokens[i].kind {
            Kind::Newline => {}
            Kind::Title => db.title = Some(from(text(i), 6).into()),
            Kind::AccTitle => {
                i += 1;
                db.set_acc_title(text(i).trim_js());
            }
            Kind::AccDescr => {
                i += 1;
                db.set_acc_description(text(i).trim_js());
            }
            Kind::AccDescrMultilineValue => db.set_acc_description(text(i).trim_js()),
            Kind::Section => db.add_section(from(text(i), 8).into()),
            Kind::TaskName => {
                let name = text(i);
                i += 1;
                if tokens[i].kind != Kind::TaskData {
                    return Err(fail(i, "'taskData'"));
                }
                db.add_task(name.into(), text(i));
            }
            Kind::Eof => return Ok(()),
            _ => return Err(fail(i, STATEMENT)),
        }
        i += 1;
    }
}
