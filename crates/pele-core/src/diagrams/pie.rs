//! Pie charts: parse, model, layout and draw, ported from Pele's `src/diagrams/pie`.

use super::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, Scanner, SyntaxError,
    TITLE, TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword, string_value, title_value, token,
    tokenize,
};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::{JsText, JsValue, Units, decimal, is_digit, is_space};
use crate::measure::{Emphasis, Label, layout_label, width};
use crate::scene::{
    Accessible, AnchorKind, Entity, Facet, LabelLook, LayerSpec, LineJoin, Number, Paint, Point, Role, Scene,
    SceneBuilder, SeriesSlot, Size, TitleLayout, paths,
};
use crate::{RenderError, fit, js};
use indexmap::IndexMap;
use std::f64::consts::PI;

const SHOW_DATA: usize = 0;
const PIE: usize = 1;
const COLON: usize = 2;
const NUMBER: usize = 3;
const ACC_DESCR_KIND: usize = 4;
const ACC_TITLE_KIND: usize = 5;
const TITLE_KIND: usize = 6;
const STRING_KIND: usize = 7;
const NEWLINE_KIND: usize = 8;

/// Answers as `NUMBER_PIE`'s pattern does:
/// `(?:-?[0-9]+\.[0-9]+(?!\.))|(?:-?(0|[1-9][0-9]*)(?!\.))`.
/// The pattern reads a whole run of digits before it finds there is no fraction, and would do so
/// again from every digit of a long run of zeros, so the run is remembered.
fn number_scanner() -> Scanner {
    let dot = Some(u16::from(b'.'));
    let (mut from, mut end, mut fraction) = (0, 0, None);
    Box::new(move |src, p| {
        let at = p + usize::from(src.at(p) == Some(u16::from(b'-')));
        if at < from || at >= end {
            from = at;
            end = src.run(at, is_digit);
            fraction = None;
            if end > at && src.at(end) == dot {
                let stop = src.run(end + 1, is_digit);
                let length = stop - end - 1;
                if length > 0 && src.at(stop) != dot {
                    fraction = Some(stop);
                } else if length > 1 {
                    fraction = Some(stop - 1);
                }
            }
        }
        if end == at {
            return None;
        }
        if fraction.is_some() {
            return fraction;
        }
        if src.at(at) == Some(u16::from(b'0')) {
            return (src.at(at + 1) != dot).then_some(at + 1);
        }
        if src.at(end) != dot {
            return Some(end);
        }
        (end - 1 > at).then_some(end - 1)
    })
}

static PIE_TOKENS: [TokenType; 13] = [
    keyword("showData"),
    keyword("pie"),
    token(":", Matcher::Literal(":")),
    token("NUMBER_PIE", Matcher::Scanner(number_scanner)),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Default)]
struct PieAst {
    show_data: bool,
    title: Option<Vec<u16>>,
    acc_title: Option<Vec<u16>>,
    acc_descr: Option<Vec<u16>>,
    sections: Vec<(Vec<u16>, f64)>,
}

/// Pele's `parsePie`.
fn parse_pie(src: &[u16]) -> Result<PieAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &PIE_TOKENS)?, &PIE_TOKENS);
    let mut ast = PieAst::default();

    fn end_of_line(r: &mut Reader<'_>) -> Result<(), SyntaxError> {
        match r.kind() {
            None => Ok(()),
            Some(NEWLINE_KIND) => {
                while r.accept(NEWLINE_KIND) {}
                Ok(())
            }
            Some(_) => Err(r.fail("a line break or the end of input")),
        }
    }

    while r.accept(NEWLINE_KIND) {}
    r.expect(PIE)?;
    ast.show_data = r.accept(SHOW_DATA);
    while let Some(kind) = r.kind() {
        match kind {
            NEWLINE_KIND => r.skip(),
            ACC_DESCR_KIND => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                end_of_line(&mut r)?;
            }
            ACC_TITLE_KIND => {
                ast.acc_title = Some(acc_title_value(r.take()));
                end_of_line(&mut r)?;
            }
            TITLE_KIND => {
                ast.title = Some(title_value(r.take()));
                end_of_line(&mut r)?;
            }
            STRING_KIND => {
                let label = string_value(r.take());
                r.expect(COLON)?;
                let value = decimal(r.expect(NUMBER)?);
                ast.sections.push((label, value));
                end_of_line(&mut r)?;
            }
            _ => return Err(r.fail("a section, a title, or a line break")),
        }
    }
    Ok(ast)
}

pub(crate) struct PieModel {
    /// Slices in source order. Of two sections with one label, the first is kept.
    sections: IndexMap<JsText, f64>,
    show_data: bool,
    title: Option<JsText>,
    accessible: Accessible,
}

/// Pele's `buildPie`. The error is its message for a negative value.
fn build_pie(ast: PieAst, front_title: Option<&JsText>) -> Result<PieModel, String> {
    let mut sections = IndexMap::new();
    for (label, value) in ast.sections {
        if value < 0.0 {
            return Err(format!(
                "\"{}\" has invalid value: {}. Negative values are not allowed in pie charts. All slice values must be >= 0.",
                String::from_utf16_lossy(&label),
                js::to_string(value)
            ));
        }
        sections.entry(JsText::from(label)).or_insert(value);
    }
    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    // `accDescr.replace(/\n\s+/g, '\n')`
    let unindent = |text: Vec<u16>| {
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            out.push(text[at]);
            let spaces = text.run(at + 1, is_space);
            at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
        }
        out
    };
    Ok(PieModel {
        sections,
        show_data: ast.show_data,
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        accessible: Accessible {
            title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
            description: filled(ast.acc_descr).map(|text| unindent(text).into()),
        },
    })
}

const RADIUS: f64 = 120.0;
const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const GAP: f64 = 32.0;
const OUTSIDE: f64 = 28.0;

/// One draw of the chart: the plot's diameter, and whether the chart did not fit the width it
/// has with its legend beside it.
#[derive(Clone, Copy)]
struct PieAttempt {
    diameter: f64,
    narrow: bool,
}

/// Where `pie.legendPosition` puts the legend. Any other value draws it under the plot without
/// making room for it, as Pele does.
#[derive(Clone, Copy, PartialEq)]
enum LegendPosition {
    Left,
    Right,
    Center,
    Top,
    Bottom,
    Unknown,
}

#[derive(Clone, Copy, PartialEq)]
enum SliceLabelPlacement {
    Inside,
    /// Outside the rim, for a slice too thin for its percentage.
    Outside,
}

struct SliceLayout {
    section: JsText,
    series: usize,
    start: f64,
    end: f64,
    percent: Label,
    /// The baseline point of the percentage, relative to the centre.
    label_at: Point,
    placement: SliceLabelPlacement,
}

struct LegendLayout {
    section: JsText,
    series: usize,
    label: Label,
    swatch: Point,
    /// The centre of the label.
    at: Point,
}

struct PieLayout {
    size: Size,
    centre: Point,
    radius: f64,
    inner_radius: f64,
    /// The font size of percentages and legend labels.
    small: f64,
    title: TitleLayout,
    slices: Vec<SliceLayout>,
    legend: Vec<LegendLayout>,
}

/// The geometry of one attempt. Pele's `draw` up to where it starts writing.
fn layout(model: &PieModel, cx: &DrawContext<'_>, attempt: PieAttempt) -> PieLayout {
    let radius = attempt.diameter / 2.0;
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let pad = cx.options.padding_or_default();
    let plain = Emphasis::default();

    let option = |key: &str| cx.config.get("pie")?.as_record()?.get(key);
    let number = |key: &str| option(key).and_then(JsValue::as_number);
    let text_position = number("textPosition").filter(|&position| (0.0..=1.0).contains(&position)).unwrap_or(0.75);
    let inner_radius = number("donutHole").filter(|&hole| hole > 0.0 && hole <= 0.9).map_or(0.0, |hole| hole * radius);
    let placed = match option("legendPosition") {
        None | Some(JsValue::Null) => JsText::from("right"),
        Some(value) => value.to_js_string(),
    };
    let position = match placed.to_display().as_str() {
        "left" | "right" if attempt.narrow => LegendPosition::Bottom,
        "left" => LegendPosition::Left,
        "right" => LegendPosition::Right,
        "center" => LegendPosition::Center,
        "top" => LegendPosition::Top,
        "bottom" => LegendPosition::Bottom,
        _ => LegendPosition::Unknown,
    };

    let mut sum = 0.0;
    for value in model.sections.values() {
        sum += value;
    }

    let labels: Vec<Label> = model
        .sections
        .iter()
        .map(|(label, &value)| {
            let mut text = label.clone();
            if model.show_data {
                text.push_str(&format!(" [{}]", js::to_string(value)));
            }
            layout_label(&text, false, cx.measure, small, 4000.0, plain)
        })
        .collect();
    let mut legend_width = 0.0;
    for label in &labels {
        legend_width = js::max(legend_width, label.width);
    }
    legend_width += if labels.is_empty() { 0.0 } else { SWATCH + 8.0 };
    let legend_height = labels.len() as f64 * ROW;

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, cx.measure, size, fit::title_room(cx.options), Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };

    let percent = |share: f64| -> JsText {
        let mut text = String::new();
        Number::fixed(share * 100.0, 0).write(&mut text);
        (text + "%").as_str().into()
    };
    // A slice under one percent is not drawn, though it keeps its series colour.
    let shares: Vec<f64> = model
        .sections
        .values()
        .map(|&value| {
            let share = if sum > 0.0 { value / sum } else { 0.0 };
            if share * 100.0 >= 1.0 && !percent(share).is_str("0%") { share } else { 0.0 }
        })
        .collect();
    // A slice too thin for its percentage gets the label outside the rim, which needs a margin.
    let narrow: Vec<bool> = shares
        .iter()
        .map(|&share| {
            if share == 0.0 || share >= 0.5 {
                return false;
            }
            let chord = 2.0 * radius * text_position * js::sin(share * PI);
            chord < width(cx.measure, &percent(share), small, plain) + 6.0
        })
        .collect();
    let margin = if narrow.contains(&true) { OUTSIDE } else { 0.0 };

    let diameter = (radius + margin) * 2.0;
    let mut plot_width = diameter;
    let mut height = diameter;
    match position {
        LegendPosition::Left | LegendPosition::Right => {
            plot_width += if legend_width > 0.0 { GAP + legend_width } else { 0.0 };
            height = js::max(height, legend_height);
        }
        LegendPosition::Top | LegendPosition::Bottom => {
            plot_width = js::max(plot_width, legend_width);
            height += if legend_height > 0.0 { 20.0 + legend_height } else { 0.0 };
        }
        LegendPosition::Center | LegendPosition::Unknown => {}
    }
    let plot_width = js::max(plot_width, title.width);

    let centre_x = pad
        + match position {
            LegendPosition::Left => legend_width + GAP + diameter / 2.0,
            LegendPosition::Right => diameter / 2.0,
            _ => plot_width / 2.0,
        };
    let body_top = pad + title_height;
    let centre_y = body_top
        + match position {
            LegendPosition::Top => legend_height + 20.0 + diameter / 2.0,
            LegendPosition::Bottom => diameter / 2.0,
            _ => height / 2.0,
        };

    let mut slices = Vec::new();
    let mut angle = 0.0;
    let rim = radius + OUTSIDE / 2.0;
    let mut placed_outside: Vec<(Point, f64)> = Vec::new();
    for (index, label) in model.sections.keys().enumerate() {
        let share = shares[index];
        if share == 0.0 {
            continue;
        }
        let end = angle + share * PI * 2.0;
        let mid = (angle + end) / 2.0;
        let text = percent(share);
        let text_width = width(cx.measure, &text, small, plain);
        let on_rim = |at: f64| Point { x: js::sin(at) * rim, y: -js::cos(at) * rim };
        let (label_at, placement) = if narrow[index] {
            let clear = |point: Point| {
                placed_outside.iter().all(|(other, other_width)| {
                    (point.x - other.x).abs() >= (text_width + other_width) / 2.0 + 2.0
                        || (point.y - other.y).abs() >= small + 2.0
                })
            };
            let mut at = mid;
            let mut step = 0;
            while step < 360 && !clear(on_rim(at)) {
                at += 2.0 / rim;
                step += 1;
            }
            let point = on_rim(at);
            placed_outside.push((point, text_width));
            (Point { x: point.x, y: point.y + small * 0.35 }, SliceLabelPlacement::Outside)
        } else {
            let r = radius * text_position;
            (Point { x: js::sin(mid) * r, y: -js::cos(mid) * r + small * 0.35 }, SliceLabelPlacement::Inside)
        };
        slices.push(SliceLayout {
            section: label.clone(),
            series: index,
            start: angle,
            end,
            percent: Label::single(&text, plain, text_width, small, js::round(small * 1.5)),
            label_at,
            placement,
        });
        angle = end;
    }

    let centred = pad + (plot_width - legend_width) / 2.0;
    let (legend_x, legend_y) = match position {
        LegendPosition::Right => (centre_x + diameter / 2.0 + GAP, body_top + (height - legend_height) / 2.0),
        LegendPosition::Left => (pad, body_top + (height - legend_height) / 2.0),
        LegendPosition::Center => (centre_x - legend_width / 2.0, centre_y - legend_height / 2.0),
        LegendPosition::Top => (centred, body_top),
        LegendPosition::Bottom | LegendPosition::Unknown => (centred, body_top + diameter + 20.0),
    };
    let legend = model
        .sections
        .keys()
        .zip(labels)
        .enumerate()
        .map(|(index, (section, label))| {
            let y = legend_y + index as f64 * ROW + ROW / 2.0;
            LegendLayout {
                section: section.clone(),
                series: index,
                swatch: Point { x: legend_x, y: y - SWATCH / 2.0 },
                at: Point { x: legend_x + SWATCH + 8.0 + label.width / 2.0, y },
                label,
            }
        })
        .collect();

    let total_width = (plot_width + 2.0 * pad).ceil();
    let total_height = (height + title_height + 2.0 * pad).ceil();
    PieLayout {
        size: Size { width: total_width, height: total_height },
        centre: Point { x: centre_x, y: centre_y },
        radius,
        inner_radius,
        small,
        title: TitleLayout { at: Point { x: total_width / 2.0, y: pad + title.height / 2.0 }, label: title },
        slices,
        legend,
    }
}

/// One attempt as a Scene: the title, the slices, the percentages inside and outside, the legend.
fn draw(model: &PieModel, cx: &DrawContext<'_>, attempt: PieAttempt) -> Result<Scene, RenderError> {
    let layout = layout(model, cx, attempt);
    let mut out = SceneBuilder::new(cx, layout.size, model.accessible.clone());
    out.title(&layout.title);

    let slices = LayerSpec::class("pele-slices")
        .translate(layout.centre)
        .stroke(Paint::Role(Role::Background))
        .stroke_width(Number::raw(1.0))
        .join(LineJoin::Round);
    out.layer(slices, |out| {
        for slice in &layout.slices {
            let part = out.part(Entity::pie_section(&slice.section));
            out.element(part, Facet::Shape)
                .class("pele-slice")
                .data_id()
                .path(paths::pie_slice(slice.start, slice.end, layout.radius, layout.inner_radius))
                .fill(Paint::Series(SeriesSlot::new(slice.series)))
                .finish();
        }
        if layout.slices.is_empty() {
            out.decoration()
                .class("pele-slice")
                .circle(Number::raw(layout.radius))
                .fill(Paint::None)
                .stroke(Paint::Role(Role::Border))
                .finish();
        }
    });

    let percentages = |out: &mut SceneBuilder, placement: SliceLabelPlacement| {
        let layer = LayerSpec::class("pele-slice-labels")
            .translate(layout.centre)
            .font_size(Number::raw(layout.small))
            .anchor(AnchorKind::Middle);
        let inside = placement == SliceLabelPlacement::Inside;
        let layer = if inside { layer.fill(Paint::Role(Role::Background)) } else { layer };
        out.layer(layer, |out| {
            for slice in layout.slices.iter().filter(|slice| slice.placement == placement) {
                let part = out.part(Entity::pie_section(&slice.section));
                let at = [Number::num(slice.label_at.x), Number::num(slice.label_at.y)];
                let ink = inside.then(|| Paint::OnSeries(SeriesSlot::new(slice.series)));
                out.baseline_text(part, slice.percent.clone(), at, ink);
            }
        });
    };
    percentages(&mut out, SliceLabelPlacement::Inside);
    percentages(&mut out, SliceLabelPlacement::Outside);

    out.layer(LayerSpec::class("pele-legend").font_size(Number::raw(layout.small)), |out| {
        for item in &layout.legend {
            let part = out.part(Entity::pie_section(&item.section));
            out.layer(LayerSpec::class("pele-legend-item").part(part, Facet::Legend).data_id(), |out| {
                out.element(part, Facet::Legend)
                    .rect(
                        [Number::num(item.swatch.x), Number::num(item.swatch.y)],
                        [Number::raw(SWATCH), Number::raw(SWATCH)],
                        Some(Number::raw(2.0)),
                    )
                    .fill(Paint::Series(SeriesSlot::new(item.series)))
                    .finish();
                out.label(part, Facet::Legend, &item.label, item.at, LabelLook::default());
            });
        }
    });
    out.finish()
}

pub(crate) struct Pie;

impl Diagram for Pie {
    type Model = PieModel;

    fn parse(cx: &ParseContext<'_>) -> Result<PieModel, RenderError> {
        let ast = parse_pie(cx.text).map_err(|error| cx.syntax(error))?;
        build_pie(ast, cx.title).map_err(|message| cx.semantic(message))
    }

    /// Pele's `renderPie`: at its own size, then with the legend below, then with a smaller plot.
    fn scene(model: &PieModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        fit::fit_width(cx.options, RADIUS * 2.0, 140.0, |diameter, narrow| {
            draw(model, cx, PieAttempt { diameter, narrow })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{JsValue, parse_json};

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn text(value: Option<&JsText>) -> String {
        value.map_or("null".to_owned(), |text| format!("{text:?}"))
    }

    /// Our outcome for one source in the words of the oracle's `model` op.
    fn ours(source: &str) -> String {
        match crate::prepare_pie_for_test(source) {
            Ok(model) => {
                let sections: Vec<String> =
                    model.sections.iter().map(|(label, value)| format!("{label:?}={:016x}", value.to_bits())).collect();
                format!(
                    "title {} accTitle {} accDescr {} showData {} sections {}",
                    text(model.title.as_ref()),
                    text(model.accessible.title.as_ref()),
                    text(model.accessible.description.as_ref()),
                    model.show_data,
                    sections.join(",")
                )
            }
            Err(error) => format!("PeleError: {error}"),
        }
    }

    fn theirs(answer: &str) -> String {
        let JsValue::Object(answer) = parse_json(&units(answer)).ok().expect("the oracle answers JSON") else {
            panic!("the oracle answers an object")
        };
        if let Some(error) = answer.get("error") {
            return error.to_js_string().to_display();
        }
        let field = |name: &str| text(answer.get(name).and_then(JsValue::as_text));
        let JsValue::Array(sections) = answer.get("sections").expect("a pie model has sections") else {
            panic!("sections is an array")
        };
        let sections: Vec<String> = sections
            .iter()
            .map(|pair| {
                let JsValue::Array(pair) = pair else { panic!("a section is a pair") };
                format!("{:?}={}", pair[0].as_text().expect("a label"), pair[1].to_js_string().to_display())
            })
            .collect();
        format!(
            "title {} accTitle {} accDescr {} showData {} sections {}",
            field("title"),
            field("accTitle"),
            field("accDescr"),
            answer.get("showData").is_some_and(JsValue::truthy),
            sections.join(",")
        )
    }

    fn sources() -> Vec<String> {
        let mut out: Vec<String> = [
            "pie",
            "  pie  ",
            "\tpie\tshowData\t",
            "\n\n  pie showData title T\n\"a\":1\n\"b\":2.5",
            "pie title Pets adopted by volunteers\n    \"Dogs\" : 386\n    \"Cats\" : 85\n    \"Rats\" : 15\n",
            "pie\n accTitle: at\n accDescr { multi\n line }\n title t\n \"a\" : 1",
            "pie\n accTitle :   spaced   out   \n accDescr:   one   line   %% comment\n title   a    title   %% c\n",
            "pie\naccDescr {\n   first line  \n\n\n   second   line\t\tthird  \n \r\n  }\n\"a\": 1",
            "pie\naccDescr{}\naccTitle:\ntitle\n\"a\":1",
            "pie\naccDescr {never closed\n\"a\":1",
            "pie\naccDescr\n\"a\":1",
            "---\ntitle: front\n---\npie\n\"a\":1",
            "---\ntitle: front\n---\npie title own\n\"a\":1",
            "---\ntitle: front\n---\npie title\n\"a\":1",
            "%%{init: {\"theme\":\"dark\"}}%%\npie title A %% c\n\"a\":1 %% d\n",
            "pie showData\n \"A\" : 1\n \"A\" : 2\n \"B\" : 3",
            "pie\n 'single' : 1\n \"esc \\\" \\n \\t \\\\ \\x \\0 end\" : 2\n \"multi\nline\" : 3",
            "pie\n \"#35;x &amp;y\" : -1",
            "pie\n \"dogs\" : -60.67",
            "pie\n \"zero\" : -0\n \"big\" : 123456789012345678901234567890\n \"small\" : 0.000001\n \"frac\" : 10.50",
            "pie\n \"a\" : 01",
            "pie\n \"a\" : 1.",
            "pie\n \"a\" : .5",
            "pie\n \"a\" : 1e3",
            "pie\n \"a\" : 1.5.2",
            "pie\n \"a\" : 1..2",
            "pie\n \"a\" : 0.5.",
            "pie\n \"a\" : 00",
            "pie\n \"a\" : 0.0",
            "pie\n \"a\" : --1",
            "pie\n \"a\" : 1 2",
            "pie\n \"a\" 1",
            "pie\n \"a\" :",
            "pie\n \"a\"",
            "pie\n \"a\" : 1 \"b\" : 2",
            "pie \"a\" : 1",
            "pie title x \"a\" : 1",
            "pie showData showData",
            "pie showDatax",
            "piex",
            "pie%% comment\n\"a\":1",
            "pie x",
            "pie\n x",
            "pie\n\"unterminated",
            "pie\n\"bad \\\nescape\" : 1",
            "pie\n: 1",
            "pie\n1",
            "pie\ntitlex",
            "pie\n title\ttabbed\n\"a\":1",
            "pie\n\"a\":1\n\n\n",
            "pie\n\"a\":1\n  %% trailing comment\n\n",
            "pie\n\"a\":1 %% then\n\"b\"",
            "pie\n---\nyaml: here\n---\n\"a\":1",
            "pie\n---\nnever closed\n\"a\":1",
            "pie\n--- x\n\"a\":1",
            "pie\n%%{ directive }%%\n\"a\":1",
            "pie\n  %%{ unclosed\n\"a\":1",
            "showData pie",
            "\"a\": 1",
            "pie title #35; is a hash; and #quot;quoted#quot;\n\"Issue #35;\": 1\n\"A #amp; B\": 2",
            "pie title style a: b #c; d; e\n\"x\": 1",
            "pie\n\"\u{1f600} emoji\" : 1\n\"\u{e9}\" x",
            "pie\r\n\"a\" : 1\r\"b\" : 2\r\n",
            "pie\n\u{a0}\"a\": 1",
            "pie\n\"a\"\u{a0}: 1",
            "pie\n  \n\t\n\"a\":1\n  ",
            "pie accTitle: on the pie line\n\"a\":1",
            "pie title one\ntitle two\naccTitle: one\naccTitle: two\n\"a\":1",
            "pie\n\"a\":1\n\"b\":2\n\"c\"",
            "pie\n\"a\":1\n}",
            "%% only a comment\n",
            "pie\n\"a\":1\n\"b\": x #35; y",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        out.push(format!("pie\n\"a\": {}", "0".repeat(300)));
        out.push(format!("pie\n\"a\": {}", "9".repeat(400)));
        out
    }

    #[test]
    fn pie_model_matches_pele() {
        let sources = sources();
        let requests: Vec<String> =
            sources.iter().map(|source| format!("model\t{}", crate::json_string(source))).collect();
        for (source, answer) in sources.iter().zip(pele_oracle::ask(&requests)) {
            assert_eq!(ours(source), theirs(&answer), "model of {source:?}");
        }
    }
}
