//! The parsed model and its schedule, as the oracle's `gantt_front` op prints Pele's.

use super::date::JsDate;
use super::{GanttModel, Scheduler};
use crate::input::JsText;
use crate::{ErrorCode, RenderError};

/// `JSON.stringify` of a string: quotes, the short escapes, `\u00xx` for other controls and
/// `\udxxx` for a lone surrogate.
fn string(out: &mut String, text: &[u16]) {
    out.push('"');
    for item in char::decode_utf16(text.iter().copied()) {
        match item {
            Ok('"') => out.push_str("\\\""),
            Ok('\\') => out.push_str("\\\\"),
            Ok('\u{8}') => out.push_str("\\b"),
            Ok('\u{c}') => out.push_str("\\f"),
            Ok('\n') => out.push_str("\\n"),
            Ok('\r') => out.push_str("\\r"),
            Ok('\t') => out.push_str("\\t"),
            Ok(c) if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            Ok(c) => out.push(c),
            Err(lone) => out.push_str(&format!("\\u{:04x}", lone.unpaired_surrogate())),
        }
    }
    out.push('"');
}

fn optional(out: &mut String, text: Option<&[u16]>) {
    match text {
        Some(text) => string(out, text),
        None => out.push_str("null"),
    }
}

fn strings(out: &mut String, items: &[JsText]) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        string(out, item);
    }
    out.push(']');
}

/// A date as the 16 hex digits of its time value, `"NaN"` when invalid, or `null` when unset.
fn date(out: &mut String, date: Option<JsDate>) {
    match date {
        Some(date) if date.is_valid() => out.push_str(&format!("\"{:016x}\"", date.time().to_bits())),
        Some(_) => out.push_str("\"NaN\""),
        None => out.push_str("null"),
    }
}

fn code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Unsupported => "unsupported-diagram",
        ErrorCode::Syntax => "syntax",
        ErrorCode::Semantic => "semantic",
        ErrorCode::Limit => "limit",
    }
}

/// The model, then each task's schedule against `now`, or the error scheduling fails with.
pub(crate) fn front_json(model: &GanttModel, now: f64) -> String {
    let mut out = String::from("{\"title\":");
    optional(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    optional(&mut out, model.acc_title.as_deref());
    out.push_str(",\"accDescr\":");
    optional(&mut out, model.acc_descr.as_deref());
    out.push_str(",\"dateFormat\":");
    string(&mut out, &model.date_format);
    out.push_str(",\"axisFormat\":");
    string(&mut out, &model.axis_format);
    out.push_str(",\"tickInterval\":");
    optional(&mut out, model.tick_interval.as_deref());
    out.push_str(",\"todayMarker\":");
    string(&mut out, &model.today_marker);
    out.push_str(",\"includes\":");
    strings(&mut out, &model.includes);
    out.push_str(",\"excludes\":");
    strings(&mut out, &model.excludes);
    out.push_str(&format!(",\"inclusiveEndDates\":{},\"topAxis\":{}", model.inclusive_end_dates, model.top_axis));
    out.push_str(",\"displayMode\":");
    string(&mut out, &model.display_mode);
    out.push_str(",\"weekday\":");
    match model.weekday {
        Some(day) => out.push_str(&format!("\"{day}\"")),
        None => out.push_str("null"),
    }
    out.push_str(&format!(",\"weekend\":\"{}\",\"sections\":", model.weekend.name()));
    strings(&mut out, &model.sections);
    out.push_str(",\"links\":[");
    for (index, (id, href)) in model.links.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('[');
        string(&mut out, id);
        out.push(',');
        string(&mut out, href);
        out.push(']');
    }
    out.push_str("],\"tasks\":[");
    for (index, task) in model.tasks.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        string(&mut out, &task.id);
        out.push_str(",\"task\":");
        string(&mut out, &task.task);
        out.push_str(",\"section\":");
        string(&mut out, &task.section);
        let tags = task.tags;
        out.push_str(&format!(
            ",\"active\":{},\"done\":{},\"crit\":{},\"milestone\":{},\"vert\":{},\"order\":{}",
            tags.active,
            tags.done,
            tags.crit,
            tags.milestone,
            tags.vert,
            task.order.map_or(-1, |order| order as i64)
        ));
        out.push_str(",\"classes\":");
        strings(&mut out, &task.classes);
        out.push_str(",\"prevTaskId\":");
        optional(&mut out, task.prev_task_id.as_deref());
        out.push_str(",\"raw\":{\"data\":");
        string(&mut out, &task.raw.data);
        out.push_str(",\"start\":");
        optional(&mut out, task.raw.start.as_deref());
        out.push_str(",\"end\":");
        string(&mut out, &task.raw.end);
        out.push_str("}}");
    }
    out.push_str("],\"schedule\":");
    let mut scheduler = Scheduler::new(model, now);
    match scheduler.run() {
        Ok(()) => {
            out.push('[');
            for (index, task) in scheduler.tasks.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push('[');
                date(&mut out, task.start);
                out.push(',');
                date(&mut out, task.end);
                out.push(',');
                date(&mut out, task.render_end);
                out.push_str(&format!(",{},{}]", task.manual_end, task.processed));
            }
            out.push(']');
        }
        Err(RenderError::Diagnostic(diagnostic)) => {
            out.push_str("{\"error\":");
            string(&mut out, &format!("PeleError: {}", diagnostic.message).encode_utf16().collect::<Vec<_>>());
            out.push_str(&format!(",\"code\":\"{}\"}}", code_name(diagnostic.code)));
        }
        Err(RenderError::NotPorted) => unreachable!("scheduling knows only its own errors"),
    }
    out.push('}');
    out
}
