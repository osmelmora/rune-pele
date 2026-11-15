//! The model as the oracle's `quadrant_front` op prints Pele's: each text as `{text, type}` or
//! null, a point's style fields in a fixed order without the ones it does not set, and each
//! number as the bits of its double.

use super::{PointStyle, QuadrantModel, QuadrantText};

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

fn optional(out: &mut String, text: Option<&[u16]>) {
    match text {
        Some(text) => string(out, text),
        None => out.push_str("null"),
    }
}

fn text(out: &mut String, label: Option<&QuadrantText>) {
    match label {
        Some(label) => {
            out.push_str("{\"text\":");
            string(out, &label.text);
            out.push_str(if label.markdown { ",\"type\":\"markdown\"}" } else { ",\"type\":\"text\"}" });
        }
        None => out.push_str("null"),
    }
}

/// The fields a style sets, each after a comma.
fn style(out: &mut String, style: &PointStyle) {
    if let Some(radius) = style.radius {
        out.push_str(&format!(",\"radius\":\"{:016x}\"", radius.to_bits()));
    }
    for (name, value) in
        [("color", &style.color), ("strokeColor", &style.stroke_color), ("strokeWidth", &style.stroke_width)]
    {
        if let Some(value) = value {
            out.push_str(&format!(",\"{name}\":"));
            string(out, value);
        }
    }
}

pub(crate) fn front_json(model: &QuadrantModel) -> String {
    let mut out = String::from("{\"type\":\"quadrantChart\",\"title\":");
    optional(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    optional(&mut out, model.acc_title.as_deref());
    out.push_str(",\"accDescr\":");
    optional(&mut out, model.acc_descr.as_deref());
    out.push_str(",\"quadrants\":[");
    for (index, quadrant) in model.quadrants.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        text(&mut out, quadrant.as_ref());
    }
    out.push(']');
    for (name, label) in [
        ("xAxisLeft", &model.x_axis_left),
        ("xAxisRight", &model.x_axis_right),
        ("yAxisBottom", &model.y_axis_bottom),
        ("yAxisTop", &model.y_axis_top),
    ] {
        out.push_str(&format!(",\"{name}\":"));
        text(&mut out, label.as_ref());
    }
    out.push_str(",\"points\":[");
    for (index, point) in model.points.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"x\":\"{:016x}\",\"y\":\"{:016x}\"", point.x.to_bits(), point.y.to_bits()));
        style(&mut out, &point.style);
        out.push_str(",\"text\":");
        text(&mut out, Some(&point.text));
        out.push_str(",\"className\":");
        string(&mut out, &point.class_name);
        out.push('}');
    }
    out.push_str("],\"classes\":[");
    for (index, (name, shared)) in model.classes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('[');
        string(&mut out, name);
        out.push_str(",{");
        let mut fields = String::new();
        style(&mut fields, shared);
        out.push_str(fields.strip_prefix(',').unwrap_or(""));
        out.push_str("}]");
    }
    out.push_str("]}");
    out
}
