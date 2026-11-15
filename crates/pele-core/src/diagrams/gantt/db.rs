//! Pele's `GanttDb`: what the parser records, and the scheduling that resolves each task's dates.

use super::date::JsDate;
use super::dates::{Unit, WEEKDAYS, add, add_days, format, iso_date, parse_strict};
use super::{GanttModel, Raw, Tags, Task, Weekend};
use crate::catalog::ParseContext;
use crate::input::url::safe_url;
use crate::input::{JsText, Units, decimal, is_space, is_word, string_to_number, to_lowercase};
use crate::{Diagnostic, ErrorCode, RenderError};
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};

/// `^(\d+(?:\.\d+)?)([Mdhmswy]|ms)$` on the trimmed text: the amount and its unit, or NaN and
/// milliseconds when the text is no duration.
pub(crate) fn parse_duration(text: &[u16]) -> (f64, Unit) {
    let text = text.trim_js();
    let digits = |from: usize| text[from..].iter().take_while(|&&c| (48..=57).contains(&c)).count();
    let whole = digits(0);
    let mut end = whole;
    if whole > 0 && text.get(end) == Some(&u16::from(b'.')) && digits(end + 1) > 0 {
        end += 1 + digits(end + 1);
    }
    let unit = match String::from_utf16_lossy(&text[end..]).as_str() {
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

/// `txt.toLowerCase().split(/[\s,]+/)` added to `existing`, each token once, in order.
fn merge_tokens(existing: &[JsText], text: &[u16]) -> Vec<JsText> {
    let mut merged: IndexSet<JsText> = existing.iter().cloned().collect();
    let lower = to_lowercase(text);
    for token in lower.split(|&c| is_space(c) || c == u16::from(b',')) {
        if !token.is_empty() {
            merged.insert(token.into());
        }
    }
    merged.into_iter().collect()
}

/// The parser's side of Pele's `GanttDb`.
pub(crate) struct GanttDb {
    pub(crate) model: GanttModel,
    current_section: JsText,
    task_count: usize,
    last_order: usize,
    last_task_id: Option<JsText>,
}

impl GanttDb {
    pub(crate) fn new() -> Self {
        GanttDb {
            model: GanttModel {
                title: None,
                acc_title: None,
                acc_descr: None,
                date_format: JsText::default(),
                axis_format: JsText::default(),
                tick_interval: None,
                today_marker: JsText::default(),
                includes: Vec::new(),
                excludes: Vec::new(),
                inclusive_end_dates: false,
                top_axis: false,
                display_mode: JsText::default(),
                weekday: None,
                weekend: Weekend::Saturday,
                sections: Vec::new(),
                links: IndexMap::new(),
                tasks: Vec::new(),
                by_id: HashMap::new(),
            },
            current_section: JsText::default(),
            task_count: 0,
            last_order: 0,
            last_task_id: None,
        }
    }

    pub(crate) fn set_includes(&mut self, text: &[u16]) {
        self.model.includes = merge_tokens(&self.model.includes, text);
    }

    pub(crate) fn set_excludes(&mut self, text: &[u16]) {
        self.model.excludes = merge_tokens(&self.model.excludes, text);
    }

    pub(crate) fn set_acc_title(&mut self, text: &[u16]) {
        self.model.acc_title = Some(text.trim_js().trim_start_js().into());
    }

    /// `txt.trim().replace(/\n\s+/g, '\n')`.
    pub(crate) fn set_acc_description(&mut self, text: &[u16]) {
        let text = text.trim_js();
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            out.push(text[at]);
            if text[at] == 10 {
                let spaces = text[at + 1..].iter().take_while(|&&c| is_space(c)).count();
                at += spaces;
            }
            at += 1;
        }
        self.model.acc_descr = Some(out.as_slice().into());
    }

    pub(crate) fn add_section(&mut self, text: JsText) {
        self.current_section = text.clone();
        self.model.sections.push(text);
    }

    pub(crate) fn add_task(
        &mut self,
        cx: &ParseContext<'_>,
        descr: &[u16],
        data_str: &[u16],
    ) -> Result<(), RenderError> {
        let body = data_str.strip_prefix(&[u16::from(b':')]).unwrap_or(data_str);
        let mut data: Vec<&[u16]> = body.split(|&c| c == u16::from(b',')).collect();
        let mut tags = Tags::default();
        while let Some(tag) = data.first().and_then(|field| tag(field)) {
            match tag {
                "active" => tags.active = true,
                "done" => tags.done = true,
                "crit" => tags.crit = true,
                "milestone" => tags.milestone = true,
                _ => tags.vert = true,
            }
            data.remove(0);
        }
        let name = || String::from_utf16_lossy(descr.trim_js());
        if data.is_empty() {
            return Err(cx.semantic(format!("Task \"{}\" needs a duration or an end date.", name())));
        }
        if data.len() > 3 {
            return Err(cx.semantic(format!(
                "Task \"{}\" has too many fields. A task takes an id, a start, and an end or duration.",
                name()
            )));
        }
        let data: Vec<&[u16]> = data.into_iter().map(|field| field.trim_js()).collect();
        let id: JsText = if data.len() == 3 {
            data[0].into()
        } else {
            self.task_count += 1;
            format!("task{}", self.task_count).as_str().into()
        };
        let start = (data.len() > 1).then(|| data[data.len() - 2].into());
        let order = (!tags.vert).then(|| {
            self.last_order += 1;
            self.last_order - 1
        });
        let task = Task {
            id: id.clone(),
            own_id: data.len() == 3,
            task: descr.into(),
            section: self.current_section.clone(),
            tags,
            order,
            classes: Vec::new(),
            prev_task_id: self.last_task_id.replace(id.clone()),
            raw: Raw { data: data_str.into(), start, end: data[data.len() - 1].into() },
        };
        self.model.tasks.push(task);
        self.model.by_id.insert(id, self.model.tasks.len() - 1);
        Ok(())
    }

    pub(crate) fn set_link(&mut self, ids: &[u16], link: &[u16]) {
        let href = safe_url(link);
        for id in ids.split(|&c| c == u16::from(b',')) {
            if self.model.by_id.contains_key(id) {
                self.model.links.insert(id.into(), href.clone());
            }
        }
        self.set_class(ids, "clickable");
    }

    pub(crate) fn set_class(&mut self, ids: &[u16], class: &str) {
        for id in ids.split(|&c| c == u16::from(b',')) {
            if let Some(&index) = self.model.by_id.get(id) {
                self.model.tasks[index].classes.push(class.into());
            }
        }
    }
}

/// `^\s*(active|done|crit|milestone|vert)\s*$`.
fn tag(field: &[u16]) -> Option<&'static str> {
    let word = String::from_utf16_lossy(field.trim_js());
    ["active", "done", "crit", "milestone", "vert"].into_iter().find(|&tag| tag == word)
}

const WORK_LIMIT: f64 = 4e8;
const DAY_STEP: f64 = 400.0;
const DATED_STEP: f64 = 200.0;
const FORMAT_STEP: f64 = 400.0;

/// A task's dates as scheduling resolves them. A task that waits on one that never resolves keeps
/// what it had.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Scheduled {
    pub(crate) start: Option<JsDate>,
    pub(crate) end: Option<JsDate>,
    /// Where the bar ends when excluded days pushed the task's end out.
    pub(crate) render_end: Option<JsDate>,
    pub(crate) manual_end: bool,
    pub(crate) processed: bool,
}

fn semantic(message: &str) -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message.to_owned()))
}

/// `^keyword\s+([\d\w- ]+)`: the list of ids after `after` or `until`, as the regular
/// expression's backtracking finds it.
fn reference_list<'a>(text: &'a [u16], keyword: &str) -> Option<&'a [u16]> {
    let start = keyword.len();
    if !text.starts_with(&keyword.encode_utf16().collect::<Vec<_>>()) {
        return None;
    }
    let spaces = start + text[start..].iter().take_while(|&&c| is_space(c)).count();
    let id = |c: u16| is_word(c) || c == u16::from(b'-') || c == 32;
    (start + 1..=spaces).rev().find_map(|from| {
        let end = from + text[from..].iter().take_while(|&&c| id(c)).count();
        (end > from).then(|| &text[from..end])
    })
}

/// Pele's `getTasks` and `isInvalidDate`, which share their count of work: scheduling against a
/// time, and then the days a render asks about.
pub(crate) struct Scheduler<'a> {
    model: &'a GanttModel,
    now: f64,
    work: f64,
    include: HashSet<Vec<u16>>,
    exclude: HashSet<Vec<u16>>,
    excluded_days: [bool; 7],
    dated_tokens: bool,
    pub(crate) tasks: Vec<Scheduled>,
    checked: Vec<Option<Checked>>,
}

/// `/^[YMDHhmsSZXx\-/.:, T]*$/`.
fn numeric_format(template: &[u16]) -> bool {
    template.iter().all(|&c| "YMDHhmsSZXx-/.:, T".encode_utf16().any(|allowed| allowed == c))
}

impl<'a> Scheduler<'a> {
    pub(crate) fn new(model: &'a GanttModel, now: f64) -> Self {
        let include: HashSet<Vec<u16>> = model.includes.iter().map(|token| token.to_vec()).collect();
        let exclude: HashSet<Vec<u16>> = model.excludes.iter().map(|token| token.to_vec()).collect();
        let weekend_start = match model.weekend {
            Weekend::Friday => 5,
            Weekend::Saturday => 6,
        };
        let named = |name: &str| exclude.contains(&name.encode_utf16().collect::<Vec<_>>());
        let weekends = named("weekends");
        let excluded_days = std::array::from_fn(|day| {
            let iso = if day == 0 { 7 } else { day };
            (weekends && (iso == weekend_start || iso == weekend_start + 1)) || named(&WEEKDAYS[day].to_lowercase())
        });
        let day_name = |token: &JsText| WEEKDAYS.iter().any(|day| token.is_str(&day.to_lowercase()));
        let dated_tokens = !model.includes.is_empty()
            || !numeric_format(model.date_format.trim_js())
            || model.excludes.iter().any(|token| !token.is_str("weekends") && !day_name(token));
        Scheduler {
            model,
            now,
            work: 0.0,
            include,
            exclude,
            excluded_days,
            dated_tokens,
            tasks: vec![Scheduled::default(); model.tasks.len()],
            checked: vec![None; model.tasks.len()],
        }
    }

    /// Resolves every task's start and end. A task may refer to tasks defined after it, so, like
    /// Mermaid, this makes up to eleven passes.
    pub(crate) fn run(&mut self) -> Result<(), RenderError> {
        let mut all = self.compile_tasks()?;
        for _ in 0..10 {
            if all {
                break;
            }
            all = self.compile_tasks()?;
        }
        Ok(())
    }

    fn spend(&mut self, units: f64) -> Result<(), RenderError> {
        self.work += units;
        if self.work > WORK_LIMIT {
            let message = "The gantt chart takes too much work to schedule.".to_owned();
            return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, message)));
        }
        Ok(())
    }

    /// Whether a day is excluded: by its weekday, by its date as `YYYY-MM-DD`, or by its date in
    /// the chart's own format.
    pub(crate) fn is_invalid_date(&mut self, date: JsDate) -> Result<bool, RenderError> {
        let weekday = self.excluded_days[date.day() as usize];
        if !self.dated_tokens {
            return Ok(weekday);
        }
        let template = self.model.date_format.trim_js();
        let date_only = iso_date(date);
        self.spend(DATED_STEP)?;
        let formatted = if template.is_str("YYYY-MM-DD") {
            None
        } else {
            self.spend(FORMAT_STEP + template.len() as f64 * 8.0)?;
            format(date, template)
        };
        let listed =
            |set: &HashSet<Vec<u16>>| set.contains(&date_only) || formatted.as_ref().is_some_and(|f| set.contains(f));
        if listed(&self.include) {
            return Ok(false);
        }
        Ok(weekday || listed(&self.exclude))
    }

    fn parse(&mut self, text: &[u16], template: &[u16]) -> Result<Option<JsDate>, RenderError> {
        self.spend((template.len() as f64 + 1.0) * (text.len() as f64 + 1.0))?;
        Ok(parse_strict(text, template, JsDate::from_time(self.now)))
    }

    fn today(&self) -> JsDate {
        let mut today = JsDate::from_time(self.now);
        today.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0));
        today
    }

    fn compile_tasks(&mut self) -> Result<bool, RenderError> {
        let mut all = true;
        for index in 0..self.model.tasks.len() {
            self.compile_task(index)?;
            all = all && self.tasks[index].processed;
        }
        Ok(all)
    }

    fn compile_task(&mut self, index: usize) -> Result<(), RenderError> {
        let model = self.model;
        let task = &model.tasks[index];
        match &task.raw.start {
            None => {
                // Mermaid fails with a TypeError when the first task has no start. It starts today here.
                let previous = task.prev_task_id.as_ref().and_then(|id| model.by_id.get(id));
                self.tasks[index].start = match previous {
                    Some(&previous) => self.tasks[previous].end,
                    None => Some(self.today()),
                };
            }
            Some(start) => {
                if let Some(start) = self.start_date(start)? {
                    self.tasks[index].start = Some(start);
                }
            }
        }
        let Some(start) = self.tasks[index].start else {
            return Ok(());
        };
        let end = self.end_date(start, &task.raw.end)?;
        self.tasks[index].end = end;
        if end.is_none() {
            return Ok(());
        }
        self.tasks[index].processed = true;
        self.tasks[index].manual_end =
            self.parse(&task.raw.end, &"YYYY-MM-DD".encode_utf16().collect::<Vec<_>>())?.is_some();
        self.check_task_dates(index)
    }

    /// The tasks a list of ids names, skipping ids no task has.
    fn references(&self, list: &[u16]) -> Vec<usize> {
        list.split(|&c| c == 32)
            .filter(|id| !id.is_empty())
            .filter_map(|id| self.model.by_id.get(id).copied())
            .collect()
    }

    fn start_date(&mut self, raw: &[u16]) -> Result<Option<JsDate>, RenderError> {
        let text = raw.trim_js();
        let template = self.model.date_format.trim_js();
        let invalid = || semantic(&format!("Invalid date:{}", String::from_utf16_lossy(text)));

        if (template.is_str("x") || template.is_str("X"))
            && !text.is_empty()
            && text.iter().all(|&c| (48..=57).contains(&c))
        {
            let date = JsDate::from_time(string_to_number(text));
            return if date.is_valid() { Ok(Some(date)) } else { Err(invalid()) };
        }

        if let Some(list) = reference_list(text, "after") {
            let mut latest: Option<usize> = None;
            for task in self.references(list) {
                let later = match latest {
                    None => true,
                    Some(latest) => after(self.tasks[task].end, self.tasks[latest].end),
                };
                if later {
                    latest = Some(task);
                }
            }
            return Ok(match latest {
                Some(latest) => self.tasks[latest].end,
                None => Some(self.today()),
            });
        }

        if let Some(parsed) = self.parse(text, template)? {
            return Ok(Some(parsed));
        }

        // What does not fit the format gets a second chance with the JavaScript date parser, as in Mermaid.
        let date = JsDate::parse(text);
        let year = date.full_year();
        if !date.is_valid() || year < -10000.0 || year > 10000.0 {
            return Err(invalid());
        }
        Ok(Some(date))
    }

    fn end_date(&mut self, start: JsDate, raw: &[u16]) -> Result<Option<JsDate>, RenderError> {
        let text = raw.trim_js();
        if let Some(list) = reference_list(text, "until") {
            let mut earliest: Option<usize> = None;
            for task in self.references(list) {
                let sooner = match earliest {
                    None => true,
                    Some(earliest) => after(self.tasks[earliest].start, self.tasks[task].start),
                };
                if sooner {
                    earliest = Some(task);
                }
            }
            return Ok(match earliest {
                Some(earliest) => self.tasks[earliest].start,
                None => Some(self.today()),
            });
        }

        let template = self.model.date_format.trim_js();
        if let Some(parsed) = self.parse(text, template)? {
            return Ok(Some(if self.model.inclusive_end_dates { add_days(parsed, 1.0) } else { parsed }));
        }

        let (value, unit) = parse_duration(text);
        if value.is_nan() {
            return Ok(Some(start));
        }
        let end = add(start, value, unit);
        Ok(Some(if end.is_valid() { end } else { start }))
    }

    /// Pushes the end of a task out by one day for every excluded day it covers.
    fn check_task_dates(&mut self, index: usize) -> Result<(), RenderError> {
        if self.model.excludes.is_empty() || self.tasks[index].manual_end {
            return Ok(());
        }
        let (Some(start), Some(end)) = (self.tasks[index].start, self.tasks[index].end) else {
            unreachable!("a task is checked once it has both ends");
        };
        // Each pass checks a task again. The check reads only the task's two ends, so one done
        // before for the same ends gives the same dates and costs the same work, which can only
        // pass the limit if it ends past it.
        let key = (start.time().to_bits(), end.time().to_bits());
        if let Some(done) = self.checked[index].filter(|done| done.key == key) {
            self.spend(done.work)?;
            self.tasks[index].end = Some(done.end);
            self.tasks[index].render_end = done.render_end;
            return Ok(());
        }
        let before = self.work;
        let mut cursor = add_days(start, 1.0);
        let mut end_time = end;
        let max_end_time = add_days(end_time, 10000.0).time();
        let mut invalid = false;
        let mut render_end: Option<f64> = None;
        while cursor.time() <= end_time.time() {
            self.spend(DAY_STEP)?;
            if !invalid {
                render_end = Some(end_time.time());
            }
            invalid = self.is_invalid_date(cursor)?;
            if invalid {
                end_time.set_date(end_time.date() + 1.0);
                if end_time.time() > max_end_time {
                    return Err(semantic(
                        "Failed to find a valid date that was not excluded by `excludes` after 10,000 iterations.",
                    ));
                }
            }
            cursor.set_date(cursor.date() + 1.0);
        }
        let render_end = render_end.map(JsDate::from_time);
        self.tasks[index].end = Some(end_time);
        self.tasks[index].render_end = render_end;
        self.checked[index] = Some(Checked { key, end: end_time, render_end, work: self.work - before });
        Ok(())
    }
}

/// What checking a task's excluded days gave for the ends in `key`, and the work it took.
#[derive(Clone, Copy)]
struct Checked {
    key: (u64, u64),
    end: JsDate,
    render_end: Option<JsDate>,
    work: f64,
}

/// `a > b` for two dates that may be unset, as JavaScript compares them.
fn after(a: Option<JsDate>, b: Option<JsDate>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.time() > b.time(),
        _ => false,
    }
}
