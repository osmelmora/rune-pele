//! The db as the oracle's `xychart_front` op prints Pele's: each field in a fixed order, an
//! undefined one as `null`, and a number JSON cannot hold, or `-0`, as `{"$number":"…"}`.

use super::db::{Orientation, XAxis, XyChartDb};
use crate::js;

/// `JSON.stringify` of a string.
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

fn number(out: &mut String, value: f64) {
    if value == 0.0 && value.is_sign_negative() {
        out.push_str("{\"$number\":\"-0\"}");
    } else if value.is_finite() {
        out.push_str(&js::to_string(value));
    } else {
        out.push_str(&format!("{{\"$number\":\"{}\"}}", js::to_string(value)));
    }
}

fn text(out: &mut String, value: Option<&[u16]>) {
    match value {
        Some(value) => string(out, value),
        None => out.push_str("null"),
    }
}

fn word(out: &mut String, value: &str) {
    out.push('"');
    out.push_str(value);
    out.push('"');
}

fn list<T>(out: &mut String, items: &[T], mut write: impl FnMut(&mut String, &T)) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write(out, item);
    }
    out.push(']');
}

fn range(out: &mut String, title: &[u16], min: f64, max: f64) {
    out.push_str("{\"type\":\"linear\",\"title\":");
    string(out, title);
    out.push_str(",\"min\":");
    number(out, min);
    out.push_str(",\"max\":");
    number(out, max);
    out.push('}');
}

pub(crate) fn front_json(db: &XyChartDb) -> String {
    let mut out = String::from("{\"model\":{\"title\":");
    text(&mut out, db.title.as_deref());
    out.push_str(",\"accTitle\":");
    text(&mut out, db.acc_title.as_deref());
    out.push_str(",\"accDescr\":");
    text(&mut out, db.acc_descr.as_deref());
    out.push_str(",\"orientation\":");
    match db.orientation {
        Some(Orientation::Vertical) => word(&mut out, "vertical"),
        Some(Orientation::Horizontal) => word(&mut out, "horizontal"),
        None => out.push_str("null"),
    }
    out.push_str(",\"xAxis\":");
    match &db.x_axis {
        XAxis::Band { title, categories } => {
            out.push_str("{\"type\":\"band\",\"title\":");
            string(&mut out, title);
            out.push_str(",\"categories\":");
            list(&mut out, categories, |out, category| string(out, category));
            out.push('}');
        }
        XAxis::Linear { title, min, max } => range(&mut out, title, *min, *max),
    }
    out.push_str(",\"yAxis\":");
    range(&mut out, &db.y_axis.title, db.y_axis.min, db.y_axis.max);
    out.push_str(&format!(",\"hasSetXAxis\":{},\"hasSetYAxis\":{},\"plots\":", db.has_set_x_axis, db.has_set_y_axis));
    list(&mut out, &db.plots, |out, plot| {
        out.push_str("{\"type\":");
        word(out, if plot.line { "line" } else { "bar" });
        out.push_str(",\"title\":");
        string(out, &plot.title);
        out.push_str(",\"data\":");
        list(out, &plot.data, |out, (place, value)| {
            out.push('[');
            string(out, place);
            out.push(',');
            match value {
                Some(value) => number(out, *value),
                None => out.push_str("null"),
            }
            out.push(']');
        });
        out.push_str(",\"pointLabels\":");
        match &plot.point_labels {
            Some(labels) => list(out, labels, |out, label| string(out, label)),
            None => out.push_str("null"),
        }
        out.push('}');
    });
    out.push_str("}}");
    out
}
