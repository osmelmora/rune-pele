//! Drawing a quadrant chart, ported from Pele's `src/diagrams/quadrant/render.ts`: the four
//! quadrants with their labels, the axis labels, the points, and each point's label in the first
//! of five places that is inside the plot and free.

use super::{QuadrantModel, QuadrantText};
use crate::RenderError;
use crate::catalog::DrawContext;
use crate::input::{JsText, JsValue, Units, decimal};
use crate::js;
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, AuthorClasses, Entity, Facet, LabelLook, LayerSpec, Number, Paint, Point, Role, Scene, SceneBuilder,
    SeriesSlot, Size, Style, TextWeight, resolve_style,
};
use crate::{fit, input::JsObject};
use std::collections::HashMap;

const GAP: f64 = 4.0;
const CAREFUL: usize = 300;

struct Dot {
    cx: f64,
    cy: f64,
    reach: f64,
    text: Label,
    part: usize,
}

/// The chart's settings in `quadrantChart` of the config.
struct Settings<'a> {
    section: Option<&'a JsObject>,
}

impl Settings<'_> {
    fn get(&self, key: &str) -> Option<&JsValue> {
        self.section?.get(key)
    }

    /// Pele's `length`: a number that is not negative, at most `max`, or `fallback`.
    fn length(&self, key: &str, fallback: f64, max: f64) -> f64 {
        match self.get(key) {
            Some(&JsValue::Number(value)) if value >= 0.0 => js::min(value, max),
            _ => fallback,
        }
    }

    /// Pele's `textSize`: Mermaid's quadrantChart font size keys; without them the sizes follow
    /// fontSize.
    fn text_size(&self, key: &str, fallback: f64) -> f64 {
        match self.get(key) {
            Some(&JsValue::Number(value)) if value.is_finite() => js::min(72.0, js::max(4.0, value)),
            _ => fallback,
        }
    }

    fn is(&self, key: &str, text: &str) -> bool {
        matches!(self.get(key), Some(JsValue::String(value)) if value.is_str(text))
    }
}

/// A colour a style names, with the `#` Pele adds where it has none.
fn hex(color: &[u16]) -> JsText {
    if color.first() == Some(&35) {
        return color.into();
    }
    let mut out = JsText::from("#");
    out.push_units(color);
    out
}

/// The digits of a number of pixels, such as `2px`, as `parseInt` reads them.
fn pixels(width: &[u16]) -> f64 {
    decimal(&width[..width.len() - 2])
}

pub(super) fn draw(model: &QuadrantModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let settings = Settings { section: cx.config.get("quadrantChart").and_then(JsValue::as_record) };
    let size = cx.options.typography.normal.size;
    let title_size = settings.text_size("titleFontSize", size);
    let quadrant_size = settings.text_size("quadrantLabelFontSize", size);
    let x_axis_size = settings.text_size("xAxisLabelFontSize", js::round(size * 0.875));
    let y_axis_size = settings.text_size("yAxisLabelFontSize", js::round(size * 0.875));
    let point_size = settings.text_size("pointLabelFontSize", js::round(size * 0.75));
    let measurer = cx.measure;
    let natural = js::round(js::max(100.0, settings.length("chartWidth", 500.0, 4000.0)));
    let width = js::round(fit::cap_width(cx.options, natural, 280.0, 0.0));
    let height = js::round((js::max(100.0, settings.length("chartHeight", 500.0, 4000.0)) * width) / natural);
    let outer = settings.length("quadrantPadding", cx.options.padding, 200.0);
    let x_pad = settings.length("xAxisLabelPadding", 5.0, 200.0);
    let y_pad = settings.length("yAxisLabelPadding", 5.0, 200.0);
    let has_points = !model.points.is_empty();

    let label = |text: Option<&QuadrantText>, font_size: f64, max: f64| -> Label {
        let raw = text.map_or(&[][..], |text| &text.text[..]);
        let markdown = text.is_some_and(|text| text.markdown);
        layout_label(raw, markdown, measurer, font_size, js::max(max, 40.0), Emphasis::default())
    };

    let title_text = model.title.as_deref().unwrap_or_default();
    let title =
        layout_label(title_text, false, measurer, title_size, js::max(width - 2.0 * outer, 40.0), Emphasis::BOLD);
    let title_pad = settings.length("titlePadding", 10.0, 200.0);
    let title_space = if title.height > 0.0 { title.height + title_pad } else { 0.0 };

    let shows = |text: &Option<QuadrantText>| text.as_ref().is_some_and(|text| !text.text.is_empty());
    let has_x = shows(&model.x_axis_left) || shows(&model.x_axis_right);
    let has_y = shows(&model.y_axis_bottom) || shows(&model.y_axis_top);
    // Mermaid moves the x axis labels below the chart as soon as there are points.
    let x_below = has_points || settings.is("xAxisPosition", "bottom");
    let y_right = settings.is("yAxisPosition", "right");

    let side = width - 2.0 * outer;
    let x_left = label(
        model.x_axis_left.as_ref(),
        x_axis_size,
        if model.x_axis_right.is_some() { side / 2.0 - 16.0 } else { side },
    );
    let x_right = label(model.x_axis_right.as_ref(), x_axis_size, side / 2.0 - 16.0);
    let x_height = js::max(x_left.height, x_right.height);
    let x_space = if has_x { x_height + x_pad } else { 0.0 };
    let tall = height - 2.0 * outer - title_space - x_space;
    let y_bottom = label(
        model.y_axis_bottom.as_ref(),
        y_axis_size,
        if model.y_axis_top.is_some() { tall / 2.0 - 16.0 } else { tall },
    );
    let y_top = label(model.y_axis_top.as_ref(), y_axis_size, tall / 2.0 - 16.0);
    let y_height = js::max(y_bottom.height, y_top.height);
    let y_space = if has_y { y_height + y_pad } else { 0.0 };

    let left = outer + if y_right { 0.0 } else { y_space };
    let top = outer + title_space + if x_below { 0.0 } else { x_space };
    let w = js::max(0.0, side - y_space);
    let h = js::max(0.0, tall);
    let tile_w = js::max(0.0, (w - GAP) / 2.0);
    let tile_h = js::max(0.0, (h - GAP) / 2.0);

    // Boxes a point label keeps clear of, as x0, y0, x1, y1: the gaps between quadrants, the
    // quadrant labels, the points, and the labels placed before it.
    let mut taken: Vec<[f64; 4]> =
        vec![[left + tile_w, top, left + w - tile_w, top + h], [left, top + tile_h, left + w, top + h - tile_h]];
    let is_clear = |taken: &[[f64; 4]], from: usize, x0: f64, y0: f64, x1: f64, y1: f64| {
        taken[from..].iter().all(|b| {
            // Empty quadrant labels must not block point labels.
            b[0] >= b[2] || b[1] >= b[3] || !(x0 < b[2] && x1 > b[0] && y0 < b[3] && y1 > b[1])
        })
    };

    let text_top = settings.length("quadrantTextTopPadding", 5.0, 200.0);
    // Each quadrant's tile and label.
    let mut tiles = Vec::with_capacity(4);
    for (k, quadrant) in model.quadrants.iter().enumerate() {
        let x = if k == 0 || k == 3 { left + w - tile_w } else { left };
        let y = if k < 2 { top } else { top + h - tile_h };
        let text = label(quadrant.as_ref(), quadrant_size, tile_w - 16.0);
        let cx = x + tile_w / 2.0;
        let cy = if has_points { y + text_top + text.height / 2.0 } else { y + tile_h / 2.0 };
        taken.push([cx - text.width / 2.0, cy - text.height / 2.0, cx + text.width / 2.0, cy + text.height / 2.0]);
        tiles.push((x, y, text, Point { x: cx, y: cy }));
    }

    let radius = settings.length("pointRadius", 5.0, 200.0);
    let text_pad = settings.length("pointTextPadding", 5.0, 200.0);
    let fill = Paint::Series(SeriesSlot::new(0));
    let mut dots: Vec<Dot> = Vec::new();
    // Each point's entity, then its circle: its classes, centre and radius, whether it takes the
    // series colour as its stroke, and its style.
    let mut entities: Vec<Entity> = Vec::new();
    let mut circles: Vec<(String, [f64; 3], bool, Option<Style>)> = Vec::new();
    let mut ranks: HashMap<&JsText, u32> = HashMap::new();
    for point in &model.points {
        if !((0.0..=1.0).contains(&point.x) && (0.0..=1.0).contains(&point.y)) {
            continue;
        }
        let shared = model.classes.get(&point.class_name);
        let own = &point.style;
        let r = js::min(own.radius.or(shared.and_then(|shared| shared.radius)).unwrap_or(radius), 2000.0);
        let color = own.color.as_ref().or(shared.and_then(|shared| shared.color.as_ref()));
        let stroke_color = own.stroke_color.as_ref().or(shared.and_then(|shared| shared.stroke_color.as_ref()));
        let stroke_width = own.stroke_width.as_ref().or(shared.and_then(|shared| shared.stroke_width.as_ref()));
        let mut styles: Vec<JsText> = Vec::new();
        let declare = |name: &str, value: &[u16]| {
            let mut declaration = JsText::from(name);
            declaration.push_units(value);
            declaration
        };
        if let Some(color) = color {
            styles.push(declare("fill:", &hex(color)));
        }
        if let Some(stroke_width) = stroke_width {
            if let Some(stroke_color) = stroke_color {
                styles.push(declare("stroke:", &hex(stroke_color)));
            }
            styles.push(declare("stroke-width:", stroke_width));
        }
        let cx = left + point.x * w;
        let cy = top + (1.0 - point.y) * h;
        let rank = ranks.entry(&point.text.text).or_insert(0);
        entities.push(Entity::node(&point.text.text).ranked(*rank));
        *rank += 1;
        dots.push(Dot {
            cx,
            cy,
            reach: r + stroke_width.map_or(0.0, |width| js::min(pixels(width), 400.0) / 2.0),
            // Mermaid keeps point labels on one line; wrap only what is wider than the plot.
            text: label(Some(&point.text), point_size, w),
            part: circles.len(),
        });
        let classes = AuthorClasses::new(&point.class_name).after("pele-point");
        let unstroked = stroke_width.is_some() && stroke_color.is_none();
        circles.push((classes, [cx, cy, r], unstroked, resolve_style(&styles).shape));
    }

    let mut out = SceneBuilder::new(
        cx,
        Size { width, height },
        Accessible {
            title: model.acc_title.clone().filter(|text| !text.is_empty()),
            description: model.acc_descr.clone().filter(|text| !text.is_empty()),
        },
    );
    let diagram = out.part(Entity::diagram());
    let font_size = |look: LabelLook, own: f64, inherited: f64| {
        if own == inherited { look } else { look.font_size(Number::raw(own)) }
    };
    let title_at = Point { x: width / 2.0, y: title_pad / 2.0 + title.height / 2.0 };
    let title_look = font_size(LabelLook::class("pele-title"), title_size, size).weight(TextWeight::Title);
    out.label(diagram, Facet::Title, &title, title_at, title_look);

    let quadrants =
        LayerSpec::class("pele-quadrants").fill(Paint::Role(Role::Surface)).stroke(Paint::Role(Role::Border));
    out.layer(quadrants, |out| {
        for (k, (x, y, text, at)) in tiles.iter().enumerate() {
            let part = out.part(Entity::group(&JsText::from(format!("{}", k + 1).as_str())));
            out.layer(LayerSpec::class("pele-quadrant").part(part, Facet::Shape).data_id(), |out| {
                out.element(part, Facet::Shape)
                    .rect([Number::num(*x), Number::num(*y)], [Number::num(tile_w), Number::num(tile_h)], None)
                    .style(Some(Style::rounded(None)))
                    .finish();
                let look = font_size(LabelLook::class("pele-label"), quadrant_size, size)
                    .fill(Some(Paint::Role(Role::Foreground)))
                    .stroke_paint(Paint::None);
                out.label(part, Facet::Label, text, *at, look);
            });
        }
    });

    let axes = LayerSpec::class("pele-axis-labels").font_size(Number::raw(x_axis_size)).fill(Paint::Role(Role::Muted));
    out.layer(axes, |out| {
        if has_x {
            let part = out.part(Entity::group(&JsText::from("x-axis")));
            let y = if x_below { top + h + x_pad + x_height / 2.0 } else { top - x_pad - x_height / 2.0 };
            let x = if x_right.height > 0.0 { left + w / 4.0 } else { left + x_left.width / 2.0 };
            out.label(part, Facet::StartLabel, &x_left, Point { x, y }, LabelLook::default());
            out.label(part, Facet::EndLabel, &x_right, Point { x: left + (w * 3.0) / 4.0, y }, LabelLook::default());
        }
        if has_y {
            let part = out.part(Entity::group(&JsText::from("y-axis")));
            let x = if y_right { left + w + y_pad + y_height / 2.0 } else { left - y_pad - y_height / 2.0 };
            let bottom_y = if y_top.height > 0.0 { top + (h * 3.0) / 4.0 } else { top + h - y_bottom.width / 2.0 };
            for (text, y, facet) in [(&y_bottom, bottom_y, Facet::StartLabel), (&y_top, top + h / 4.0, Facet::EndLabel)]
            {
                if text.height <= 0.0 {
                    continue;
                }
                let mut turned = LayerSpec::group().translate_rotate(Point { x, y }, -90.0);
                if y_axis_size != x_axis_size {
                    turned = turned.font_size(Number::raw(y_axis_size));
                }
                out.layer(turned, |out| {
                    out.label(part, facet, text, Point { x: 0.0, y: 0.0 }, LabelLook::default());
                });
            }
        }
    });

    if dots.is_empty() {
        return out.finish();
    }
    let parts: Vec<_> = entities.into_iter().map(|entity| out.part(entity)).collect();
    // Mermaid puts each new point under the ones before it.
    out.layer(LayerSpec::class("pele-points").fill(fill), |out| {
        for (place, (classes, [cx, cy, r], unstroked, style)) in circles.iter().enumerate().rev() {
            let mut circle = out
                .element(parts[place], Facet::Shape)
                .class(classes)
                .data_id()
                .circle_at([Some(Number::num(*cx)), Some(Number::num(*cy))], Number::num(*r));
            if *unstroked {
                circle = circle.stroke(fill);
            }
            circle.style(style.clone()).finish();
        }
    });

    // A label goes below its point, or above, right or left of it, or below its quadrant's
    // title, when that is the first place that is inside the chart and free. A second round lets
    // it cross the gaps between quadrants. Large charts skip the search for a free place.
    let careful = dots.len() <= CAREFUL;
    if careful {
        for dot in &dots {
            taken.push([dot.cx - dot.reach, dot.cy - dot.reach, dot.cx + dot.reach, dot.cy + dot.reach]);
        }
    }
    let labels = LayerSpec::class("pele-point-labels").font_size(Number::raw(point_size)).keep_empty();
    out.layer(labels, |out| {
        for dot in &dots {
            let Dot { cx, cy, reach, ref text, part } = *dot;
            let half_w = text.width / 2.0;
            let half_h = text.size * 0.6 + (text.height - text.line_height) / 2.0;
            let away = reach + text_pad;
            let inset = js::min(away, js::max(0.0, (tile_w - text.width) / 2.0));
            let quadrant = if cx < left + w / 2.0 {
                if cy < top + h / 2.0 { 1 } else { 2 }
            } else if cy < top + h / 2.0 {
                0
            } else {
                3
            };
            let below = js::max(cy + away, taken[2 + quadrant][3] + text_pad) + half_h;
            let spots = [
                [cx, cy + away + half_h],
                [cx, cy - away - half_h],
                [cx + away + half_w, cy],
                [cx - away - half_w, cy],
                [cx, below],
            ];
            let (mut tx, mut ty) = (cx, spots[0][1]);
            let mut found = false;
            for round in 0..10 {
                let spot = round % 5;
                // A label moved below a title keeps clear of the plot's edge.
                let margin = if spot == 4 { inset } else { 0.0 };
                let x = if (spot < 2 || spot == 4) && text.width < w {
                    js::min(js::max(spots[spot][0], left + half_w + margin), left + w - half_w - margin)
                } else {
                    spots[spot][0]
                };
                let y = spots[spot][1];
                if x - half_w < left || x + half_w > left + w || y - half_h < top || y + half_h > top + h {
                    continue;
                }
                let from = if round < 5 { 0 } else { 2 };
                let free = !careful || is_clear(&taken, from, x - half_w, y - half_h, x + half_w, y + half_h);
                if !found || free {
                    tx = x;
                    ty = y;
                    found = true;
                }
                if free {
                    break;
                }
            }
            if careful {
                taken.push([tx - half_w, ty - half_h, tx + half_w, ty + half_h]);
            }
            out.label(parts[part], Facet::Label, text, Point { x: tx, y: ty }, LabelLook::class("pele-label"));
        }
    });
    out.finish()
}
