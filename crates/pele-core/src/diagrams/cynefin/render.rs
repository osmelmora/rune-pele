//! Pele's `renderCynefin` (`cynefin/render.ts`): four quadrants around the ellipse of Confusion,
//! each domain's name, descriptions and items centred in its place, and the transitions as
//! curves that bow away from the middle.

use super::boundaries::{
    FOLD, HORIZON, WaveOptions, backward, cliff, curve_path, ellipse_path, first, forward, last, resolve_seed, wave,
};
use super::{CynefinModel, Domain};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::graph::edges::{Marker, MarkerUse, Tip, draw_marker};
use crate::input::{JsText, JsValue};
use crate::layout::num::hypot;
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, Number, Paint, PartId, PathProgram, Point,
    Role, Scene, SceneBuilder, SeriesSlot, Size, Style, TextWeight, TitleLayout,
};
use crate::{RenderError, js};
use std::collections::HashMap;

/// The four quadrants: the domain, its column and row, and its series colour.
const QUADRANTS: [(Domain, f64, f64, usize); 4] = [
    (Domain::Complex, 0.0, 0.0, 0),
    (Domain::Complicated, 1.0, 0.0, 1),
    (Domain::Chaotic, 0.0, 1.0, 3),
    (Domain::Clear, 1.0, 1.0, 2),
];
const CONFUSION_COLOR: usize = 4;

/// Each domain's name, decision model and kind of practice.
fn meta(domain: Domain) -> [&'static str; 3] {
    match domain {
        Domain::Complex => ["Complex", "Probe \u{2192} Sense \u{2192} Respond", "Emergent Practices"],
        Domain::Complicated => ["Complicated", "Sense \u{2192} Analyse \u{2192} Respond", "Good Practices"],
        Domain::Clear => ["Clear", "Sense \u{2192} Categorise \u{2192} Respond", "Best Practices"],
        Domain::Chaotic => ["Chaotic", "Act \u{2192} Sense \u{2192} Respond", "Novel Practices"],
        Domain::Confusion => ["Confusion", "", "Disorder"],
    }
}

const MAX_CONFUSION_ITEMS: usize = 3;
const ITEM_MAX_WIDTH: f64 = 200.0;
const ITEM_PAD_X: f64 = 10.0;
const ITEM_PAD_Y: f64 = 4.0;
const ITEM_GAP: f64 = 6.0;
const HEAD_GAP: f64 = 10.0;
const MARGIN: f64 = 20.0;
const CLEARANCE: f64 = 10.0;
const STEPS: usize = 48;

/// One item of a domain, or the count of those Confusion leaves out.
struct Badge {
    text: JsText,
    label: Label,
    w: f64,
    h: f64,
    overflow: bool,
}

/// A domain's name, descriptions and items, laid out as one block.
struct Block {
    domain: Domain,
    title: Label,
    subtitles: Vec<Label>,
    badges: Vec<Badge>,
    w: f64,
    h: f64,
    cx: f64,
    cy: f64,
}

/// Pele's `setting`: a number of the `cynefin` section of the config, if it lies in range.
fn setting(cx: &DrawContext<'_>, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    let value = cx.config.get("cynefin").and_then(JsValue::as_record).and_then(|section| section.get(key));
    match value.and_then(JsValue::as_number) {
        Some(value) if value >= min && value <= max => value,
        _ => fallback,
    }
}

/// A transition as it is drawn.
struct EdgeView {
    from: Domain,
    to: Domain,
    path: PathProgram,
    head: Tip,
    label: Option<(Label, Point)>,
}

fn pair(x: f64, y: f64) -> [Number; 2] {
    [Number::num(x), Number::num(y)]
}

fn wave_with(o: WaveOptions, pin_start: bool, pin_end: bool) -> WaveOptions {
    WaveOptions { segments: 4, pin_start, pin_end, ..o }
}

#[allow(clippy::too_many_lines, reason = "one pass of Pele's renderCynefin, whose steps share its sizes")]
fn draw(model: &CynefinModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.8125);
    let item_size = js::round(size * 0.875);
    let measurer = cx.measure;
    let pad = cx.options.padding_or_default();
    let show_descriptions =
        cx.config.get("cynefin").and_then(JsValue::as_record).and_then(|section| section.get("showDomainDescriptions"))
            != Some(&JsValue::Bool(false));
    let amplitude = setting(cx, "boundaryAmplitude", 8.0, 0.0, 50.0);
    let id: Vec<u16> = cx.options.id_prefix.as_deref().unwrap_or("pele").encode_utf16().collect();
    let seed = resolve_seed(setting(cx, "seed", 0.0, -1e15, 1e15), &id);
    let item_line = js::round(item_size * 1.5);

    let badge = |text: JsText, overflow: bool| {
        let label = layout_label(&text, false, measurer, item_size, ITEM_MAX_WIDTH, Emphasis::default());
        let (w, h) = (label.width + 2.0 * ITEM_PAD_X, js::max(label.height, item_line) + 2.0 * ITEM_PAD_Y);
        Badge { text, label, w, h, overflow }
    };
    let block = |domain: Domain| {
        let [name, decision, practice] = meta(domain);
        let units = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
        let title = layout_label(&units(name), false, measurer, size, 4000.0, Emphasis::BOLD);
        let italic = Emphasis { italic: true, ..Emphasis::default() };
        let subtitles: Vec<Label> = if show_descriptions {
            [decision, practice]
                .into_iter()
                .filter(|text| !text.is_empty())
                .map(|text| layout_label(&units(text), false, measurer, small, 4000.0, italic))
                .collect()
        } else {
            Vec::new()
        };
        let items = model.items(domain);
        let shown = if domain == Domain::Confusion && items.len() > MAX_CONFUSION_ITEMS {
            MAX_CONFUSION_ITEMS
        } else {
            items.len()
        };
        let mut badges: Vec<Badge> = items[..shown].iter().map(|item| badge(item.clone(), false)).collect();
        if shown < items.len() {
            badges.push(badge(format!("+{} more", items.len() - shown).as_str().into(), true));
        }
        let (mut w, mut h) = (title.width, title.height);
        for subtitle in &subtitles {
            w = js::max(w, subtitle.width);
            h += subtitle.height;
        }
        if !badges.is_empty() {
            h += HEAD_GAP - ITEM_GAP;
        }
        for badge in &badges {
            w = js::max(w, badge.w);
            h += ITEM_GAP + badge.h;
        }
        Block { domain, title, subtitles, badges, w, h, cx: 0.0, cy: 0.0 }
    };

    let mut quadrants: Vec<Block> = QUADRANTS.iter().map(|&(domain, ..)| block(domain)).collect();
    let mut confusion = block(Domain::Confusion);
    let rx = js::max(88.0, confusion.w * 0.71 + 8.0);
    let ry = js::max(64.0, confusion.h * 0.71 + 6.0);

    let mut quad_w = setting(cx, "width", 640.0, 100.0, 10000.0) / 2.0;
    let mut quad_h = setting(cx, "height", 440.0, 100.0, 10000.0) / 2.0;
    for b in &quadrants {
        quad_w = js::max(quad_w, b.w + 2.0 * MARGIN);
        quad_h = js::max(quad_h, b.h + 2.0 * MARGIN);
    }
    // A block sits in the middle of its quadrant, so its inner corner must stay clear of the ellipse.
    for b in &quadrants {
        let dx = (quad_w - b.w) / 2.0;
        let dy = (quad_h - b.h) / 2.0;
        if js::pow(dx / (rx + CLEARANCE), 2.0) + js::pow(dy / (ry + CLEARANCE), 2.0) >= 1.0 {
            continue;
        }
        let wider = b.w + 2.0 * (rx + CLEARANCE);
        let taller = b.h + 2.0 * (ry + CLEARANCE);
        if wider * quad_h <= quad_w * taller {
            quad_w = js::max(quad_w, wider);
        } else {
            quad_h = js::max(quad_h, taller);
        }
    }

    let title_text = model.title.as_deref().unwrap_or_default();
    let title = layout_label(title_text, false, measurer, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let (width, height) = (2.0 * quad_w, 2.0 * quad_h);
    let x0 = pad;
    let y0 = pad + title_height;
    let (centre_x, centre_y) = (x0 + quad_w, y0 + quad_h);
    let (right, bottom) = (x0 + width, y0 + height);

    let fold = wave(y0, centre_y, centre_x, seed, amplitude, wave_with(FOLD, false, true));
    let west = wave(x0, centre_x, centre_y, seed + 100.0, amplitude, wave_with(HORIZON, false, true));
    let east = wave(centre_x, right, centre_y, seed + 200.0, amplitude, wave_with(HORIZON, true, false));
    let drop = cliff(centre_x, centre_y, bottom, width * 0.03);
    let start = |at: [Number; 2]| PathProgram::default().m(at[0], at[1]);
    let line = |path: PathProgram, at: [Number; 2]| path.l(at[0], at[1]);
    let region = |domain: Domain| match domain {
        Domain::Complex => backward(forward(line(start(pair(x0, y0)), first(&fold)), &fold), &west).z(),
        Domain::Complicated => {
            let path = line(line(start(first(&fold)), pair(right, y0)), last(&east));
            backward(backward(path, &east), &fold).z()
        }
        Domain::Chaotic => line(forward(forward(start(first(&west)), &west), &drop), pair(x0, bottom)).z(),
        Domain::Clear => {
            let path = line(line(forward(start(first(&east)), &east), pair(right, bottom)), last(&drop));
            backward(path, &drop).z()
        }
        Domain::Confusion => ellipse_path(centre_x, centre_y, rx, ry),
    };

    for (b, &(_, col, row, _)) in quadrants.iter_mut().zip(&QUADRANTS) {
        b.cx = x0 + quad_w * (col + 0.5);
        b.cy = y0 + quad_h * (row + 0.5);
    }
    confusion.cx = centre_x;
    confusion.cy = centre_y;
    let blocks: Vec<&Block> = quadrants.iter().chain([&confusion]).collect();
    let block_of = |domain: Domain| *blocks.iter().find(|b| b.domain == domain).expect("every domain has a block");

    let inside = |b: &Block, x: f64, y: f64| {
        if b.domain == Domain::Confusion {
            js::pow((x - centre_x) / (rx + 4.0), 2.0) + js::pow((y - centre_y) / (ry + 4.0), 2.0) <= 1.0
        } else {
            (x - b.cx).abs() <= b.w / 2.0 + 8.0 && (y - b.cy).abs() <= b.h / 2.0 + 8.0
        }
    };
    let mut edges = Vec::new();
    let mut seen: HashMap<(&str, &str), f64> = HashMap::new();
    for transition in &model.transitions {
        let (from, to) = (block_of(transition.from), block_of(transition.to));
        let (a, b) = (transition.from.name(), transition.to.name());
        let key = if a < b { (a, b) } else { (b, a) };
        let repeat = seen.get(&key).copied().unwrap_or(0.0);
        seen.insert(key, repeat + 1.0);

        let dx = to.cx - from.cx;
        let dy = to.cy - from.cy;
        let len = match hypot(dx, dy) {
            0.0 => 1.0,
            len => len,
        };
        let mx = (from.cx + to.cx) / 2.0;
        let my = (from.cy + to.cy) / 2.0;
        // Curves bow away from the middle of the diagram, which keeps them off the ellipse.
        let outward = if -dy * (mx - centre_x) + dx * (my - centre_y) >= 0.0 { 1.0 } else { -1.0 };
        let nx = (-dy / len) * outward;
        let ny = (dx / len) * outward;
        // A diagonal runs through the middle, so it bows far enough to pass the ellipse.
        let diagonal =
            from.domain != Domain::Confusion && to.domain != Domain::Confusion && dx.abs() > 1.0 && dy.abs() > 1.0;
        let bow = (if diagonal { 2.0 * (hypot(rx * nx, ry * ny) + 16.0) } else { len * 0.15 }) + repeat * 56.0;
        let px = mx + nx * bow;
        let py = my + ny * bow;
        let point = |t: f64| {
            let u = 1.0 - t;
            (u * u * from.cx + 2.0 * u * t * px + t * t * to.cx, u * u * from.cy + 2.0 * u * t * py + t * t * to.cy)
        };
        let crossing = |b: &Block, mut lo: f64, mut hi: f64, leaving: bool| {
            for _ in 0..16 {
                let mid = (lo + hi) / 2.0;
                let (x, y) = point(mid);
                if inside(b, x, y) == leaving {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) / 2.0
        };
        let at = |i: usize| i as f64 / STEPS as f64;
        let mut i = 1;
        while i <= STEPS && {
            let (x, y) = point(at(i));
            inside(from, x, y)
        } {
            i += 1;
        }
        if i > STEPS {
            continue;
        }
        let t0 = crossing(from, at(i - 1), at(i), true);
        while i <= STEPS && {
            let (x, y) = point(at(i));
            !inside(to, x, y)
        } {
            i += 1;
        }
        let t1 = if i > STEPS { 1.0 } else { crossing(to, at(i - 1), at(i), false) };

        let (sx, sy) = point(t0);
        let (mut ex, mut ey) = point(t1);
        let blossom = |a: f64, b: f64, c: f64| {
            (1.0 - t0) * (1.0 - t1) * a + ((1.0 - t0) * t1 + t0 * (1.0 - t1)) * b + t0 * t1 * c
        };
        let qx = blossom(from.cx, px, to.cx);
        let qy = blossom(from.cy, py, to.cy);
        let el = match hypot(ex - qx, ey - qy) {
            0.0 => 1.0,
            el => el,
        };
        let ux = (ex - qx) / el;
        let uy = (ey - qy) / el;
        let head = Tip { x: ex, y: ey, dx: ux, dy: uy };
        ex -= ux * Marker::Point.trim();
        ey -= uy * Marker::Point.trim();

        let label = transition.label.as_ref().map(|text| {
            let label = layout_label(text, false, measurer, small, ITEM_MAX_WIDTH, Emphasis::default());
            let (lx, ly) = point((t0 + t1) / 2.0);
            let away = (nx.abs() * label.width) / 2.0 + (ny.abs() * label.height) / 2.0 + 6.0;
            let half = label.width / 2.0 + 4.0;
            let tx = js::min(js::max(lx + nx * away, x0 + half), js::max(right - half, x0 + half));
            (label, Point { x: tx, y: ly + ny * away })
        });
        let path = PathProgram::default().m(Number::num(sx), Number::num(sy));
        let mut path = path;
        path.steps.push(crate::scene::PathStep::Quad([pair(qx, qy), pair(ex, ey)]));
        edges.push(EdgeView { from: transition.from, to: transition.to, path, head, label });
    }

    let total = Size {
        width: (js::max(width, title.width) + 2.0 * pad).ceil(),
        height: (height + title_height + 2.0 * pad).ceil(),
    };
    let accessible = Accessible { title: model.acc_title.clone(), description: model.acc_descr.clone() };
    let mut out = SceneBuilder::new(cx, total, accessible);
    let title_at = Point { x: x0 + width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at: title_at });

    let domain_part = |out: &mut SceneBuilder, domain: Domain| out.part(Entity::group(&domain.name().into()));
    out.layer(LayerSpec::class("pele-cynefin-regions"), |out| {
        for &(domain, _, _, colour) in &QUADRANTS {
            let part = domain_part(out, domain);
            out.element(part, Facet::Shape)
                .class("pele-cynefin-region")
                .data_id()
                .path(region(domain))
                .fill(Paint::Series(SeriesSlot::new(colour)))
                .fill_opacity(Number::raw(0.14))
                .finish();
        }
    });
    let lines = LayerSpec::class("pele-cynefin-boundaries")
        .fill(Paint::None)
        .stroke(Paint::Role(Role::Border))
        .dash(&[Number::raw(6.0), Number::raw(3.0)]);
    out.layer(lines, |out| {
        for curve in [&fold, &west, &east] {
            out.decoration().path(curve_path(curve)).finish();
        }
        out.decoration()
            .class("pele-cynefin-cliff")
            .path(curve_path(&drop))
            .stroke(Paint::Role(Role::Line))
            .stroke_width(Number::raw(2.0))
            .undashed()
            .cap(LineCap::Round)
            .finish();
    });
    out.decoration().path(region(Domain::Confusion)).fill(Paint::Role(Role::Background)).finish();
    let part = domain_part(&mut out, Domain::Confusion);
    out.element(part, Facet::Shape)
        .class("pele-cynefin-region")
        .data_id()
        .path(region(Domain::Confusion))
        .fill(Paint::Series(SeriesSlot::new(CONFUSION_COLOR)))
        .fill_opacity(Number::raw(0.18))
        .stroke(Paint::Role(Role::Border))
        .dash(&[Number::raw(4.0), Number::raw(2.0)])
        .finish();

    let mut ranks: HashMap<JsText, u32> = HashMap::new();
    for b in &blocks {
        let part = domain_part(&mut out, b.domain);
        out.layer(LayerSpec::class("pele-cynefin-domain").part(part, Facet::Label).data_id(), |out| {
            let mut y = b.cy - b.h / 2.0;
            let look = LabelLook::class("pele-label").weight(TextWeight::Heading);
            out.label(part, Facet::Label, &b.title, Point { x: b.cx, y: y + b.title.height / 2.0 }, look);
            y += b.title.height;
            for subtitle in &b.subtitles {
                let look = LabelLook::class("pele-cynefin-subtitle")
                    .font_size(Number::raw(small))
                    .italic()
                    .fill(Some(Paint::Role(Role::Muted)));
                out.label(part, Facet::Annotation, subtitle, Point { x: b.cx, y: y + subtitle.height / 2.0 }, look);
                y += subtitle.height;
            }
            y += HEAD_GAP - ITEM_GAP;
            for item in &b.badges {
                y += ITEM_GAP;
                let (class, item_part, facet) = if item.overflow {
                    (LayerSpec::class("pele-node pele-cynefin-overflow"), part, Facet::Row)
                } else {
                    let rank = ranks.entry(item.text.clone()).or_insert(0);
                    let item_part = out.part(Entity::node(&item.text).ranked(*rank));
                    *rank += 1;
                    (LayerSpec::class("pele-node").part(item_part, Facet::Shape).data_id(), item_part, Facet::Shape)
                };
                out.layer(class, |out| {
                    let rect = out
                        .element(item_part, facet)
                        .rect(pair(b.cx - item.w / 2.0, y), pair(item.w, item.h), None)
                        .fill(Paint::Role(Role::Background))
                        .stroke(Paint::Role(Role::Border));
                    let rect = if item.overflow { rect.dash(&[Number::raw(3.0), Number::raw(2.0)]) } else { rect };
                    rect.style(Some(Style::rounded(None))).finish();
                    let look = LabelLook::class("pele-label")
                        .font_size(Number::raw(item_size))
                        .fill(item.overflow.then_some(Paint::Role(Role::Muted)));
                    let label_facet = if item.overflow { Facet::Row } else { Facet::Label };
                    out.label(item_part, label_facet, &item.label, Point { x: b.cx, y: y + item.h / 2.0 }, look);
                });
                y += item.h;
            }
        });
    }

    let mut edge_ranks: HashMap<EntityName, u32> = HashMap::new();
    let edge_parts: Vec<PartId> = edges
        .iter()
        .map(|edge| {
            let (from, to) = (edge.from.name(), edge.to.name());
            let name = EntityName::Between {
                from: from.encode_utf16().collect(),
                to: to.encode_utf16().collect(),
                kind: String::new(),
            };
            let rank = edge_ranks.entry(name.clone()).or_insert(0);
            let part = out.part(Entity::edge(&format!("{from}-{to}").as_str().into(), name, *rank));
            *rank += 1;
            let (a, b) = (domain_part(&mut out, edge.from), domain_part(&mut out, edge.to));
            out.relation(part, a, b);
            part
        })
        .collect();
    out.layer(LayerSpec::class("pele-edges"), |out| {
        for (edge, &part) in edges.iter().zip(&edge_parts) {
            out.layer(LayerSpec::class("pele-edge").part(part, Facet::Shape).data_id(), |out| {
                out.element(part, Facet::Shape)
                    .path(edge.path.clone())
                    .fill(Paint::None)
                    .stroke(Paint::Role(Role::Line))
                    .finish();
                draw_marker(
                    out,
                    part,
                    Facet::EndMarker,
                    Marker::Point,
                    edge.head,
                    MarkerUse::Edge(Paint::Role(Role::Line)),
                );
                if let Some((label, at)) = &edge.label {
                    let look = LabelLook::class("pele-edge-label").font_size(Number::raw(small));
                    out.label(part, Facet::Label, label, *at, look);
                }
            });
        }
    });
    out.finish()
}

pub(crate) struct Cynefin;

impl Diagram for Cynefin {
    type Model = CynefinModel;

    fn parse(cx: &ParseContext<'_>) -> Result<CynefinModel, RenderError> {
        super::parse(cx)
    }

    fn scene(model: &CynefinModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        draw(model, cx)
    }
}
