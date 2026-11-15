//! Pele's `parseGantt`: Mermaid's gantt grammar over the tokens of [`super::lexer`], calling the
//! db as the grammar's actions do.

use super::db::GanttDb;
use super::lexer::{T, Token, tokenize};
use crate::RenderError;
use crate::catalog::ParseContext;
use crate::diagrams::tokens::SyntaxError;

const LINE: &str = "'EOF', 'SPACE', 'NL', 'weekday_monday', 'weekday_tuesday', 'weekday_wednesday', 'weekday_thursday', \
'weekday_friday', 'weekday_saturday', 'weekday_sunday', 'weekend_friday', 'weekend_saturday', 'dateFormat', \
'inclusiveEndDates', 'topAxis', 'axisFormat', 'tickInterval', 'excludes', 'includes', 'todayMarker', 'title', \
'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'section', 'taskTxt', 'click'";

/// Tokens that can start a line, which are also the only ones that can follow a complete statement.
fn starts_line(kind: T) -> bool {
    matches!(
        kind,
        T::Weekday(_)
            | T::WeekendFriday
            | T::WeekendSaturday
            | T::Eof
            | T::Nl
            | T::DateFormat
            | T::InclusiveEndDates
            | T::TopAxis
            | T::AxisFormat
            | T::TickInterval
            | T::Excludes
            | T::Includes
            | T::TodayMarker
            | T::Title
            | T::AccTitle
            | T::AccDescr
            | T::AccDescrMultilineValue
            | T::Section
            | T::TaskTxt
            | T::Click
    )
}

struct Parser<'a> {
    cx: &'a ParseContext<'a>,
    tokens: Vec<Token>,
    i: usize,
}

impl Parser<'_> {
    fn kind(&self) -> T {
        self.tokens[self.i].kind
    }

    fn text(&self, index: usize) -> &[u16] {
        let token = self.tokens[index];
        &self.cx.text[token.start..token.end]
    }

    fn fail(&self, expected: &str) -> RenderError {
        let token = self.tokens[self.i];
        let got = if token.kind == T::End { "1" } else { token.kind.name() };
        self.cx.syntax(SyntaxError {
            at: token.start,
            detail: format!("Expecting {expected}, got '{got}'"),
            lexical: false,
        })
    }

    /// Mermaid's parser runs a statement's action only after seeing that the next token can follow it.
    fn end(&self, also: &str) -> Result<(), RenderError> {
        if starts_line(self.kind()) { Ok(()) } else { Err(self.fail(&format!("{LINE}{also}"))) }
    }

    fn take(&mut self) -> usize {
        self.i += 1;
        self.i - 1
    }
}

pub(super) fn parse(cx: &ParseContext<'_>, db: &mut GanttDb) -> Result<(), RenderError> {
    let tokens = tokenize(cx.text);
    let mut p = Parser { cx, tokens, i: 0 };
    if p.kind() != T::Gantt {
        return Err(p.fail("'gantt'"));
    }
    p.i += 1;

    loop {
        let t = p.kind();
        if !starts_line(t) {
            return Err(p.fail(LINE));
        }
        let at = p.take();
        let text = |p: &Parser<'_>, skip: usize| p.text(at)[skip..].to_vec();
        match t {
            T::Eof => {
                if p.kind() == T::End {
                    return Ok(());
                }
                p.end("")?;
            }
            T::Nl => {}
            T::DateFormat => {
                p.end("")?;
                db.model.date_format = text(&p, 11).into();
            }
            T::InclusiveEndDates => {
                p.end("")?;
                db.model.inclusive_end_dates = true;
            }
            T::TopAxis => {
                p.end("")?;
                db.model.top_axis = true;
            }
            T::AxisFormat => {
                p.end("")?;
                db.model.axis_format = text(&p, 11).into();
            }
            T::TickInterval => {
                p.end("")?;
                db.model.tick_interval = Some(text(&p, 13).into());
            }
            T::Excludes => {
                p.end("")?;
                db.set_excludes(&text(&p, 9));
            }
            T::Includes => {
                p.end("")?;
                db.set_includes(&text(&p, 9));
            }
            T::TodayMarker => {
                p.end("")?;
                db.model.today_marker = text(&p, 12).into();
            }
            T::WeekendFriday | T::WeekendSaturday => {
                p.end("")?;
                db.model.weekend =
                    if t == T::WeekendFriday { super::Weekend::Friday } else { super::Weekend::Saturday };
            }
            T::Title => {
                p.end("")?;
                db.model.title = Some(text(&p, 6).into());
            }
            T::AccTitle | T::AccDescr => {
                let value = if t == T::AccTitle { T::AccTitleValue } else { T::AccDescrValue };
                if p.kind() != value {
                    return Err(p.fail(&format!("'{}'", value.name())));
                }
                let value = p.take();
                p.end("")?;
                let value = p.text(value).to_vec();
                if t == T::AccTitle {
                    db.set_acc_title(&value);
                } else {
                    db.set_acc_description(&value);
                }
            }
            T::AccDescrMultilineValue => {
                p.end("")?;
                db.set_acc_description(&text(&p, 0));
            }
            T::Section => {
                p.end("")?;
                db.add_section(text(&p, 8).into());
            }
            T::TaskTxt => {
                if p.kind() != T::TaskData {
                    return Err(p.fail("'taskData'"));
                }
                let data = p.take();
                p.end("")?;
                let data = p.text(data).to_vec();
                db.add_task(cx, &text(&p, 0), &data)?;
            }
            T::Click => {
                let mut name = false;
                let mut args: Option<usize> = None;
                let mut href: Option<usize> = None;
                if p.kind() == T::Callbackname {
                    p.take();
                    name = true;
                    if p.kind() == T::Callbackargs {
                        args = Some(p.take());
                    }
                    if p.kind() == T::Href {
                        href = Some(p.take());
                        p.end("")?;
                    } else {
                        p.end(if args.is_none() { ", 'callbackargs', 'href'" } else { ", 'href'" })?;
                    }
                } else if p.kind() == T::Href {
                    href = Some(p.take());
                    if p.kind() == T::Callbackname {
                        p.take();
                        name = true;
                        if p.kind() == T::Callbackargs {
                            p.take();
                            p.end("")?;
                        } else {
                            p.end(", 'callbackargs'")?;
                        }
                    } else {
                        p.end(", 'callbackname'")?;
                    }
                } else {
                    return Err(p.fail("'callbackname', 'href'"));
                }
                let ids = text(&p, 0);
                if name {
                    db.set_class(&ids, "clickable");
                }
                if let Some(href) = href {
                    db.set_link(&ids, p.text(href));
                }
            }
            T::Weekday(day) => {
                p.end("")?;
                db.model.weekday = Some(
                    ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"][usize::from(day)],
                );
            }
            _ => unreachable!("every token that starts a line has a statement"),
        }
    }
}
