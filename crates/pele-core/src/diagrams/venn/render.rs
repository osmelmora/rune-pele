//! Drawing a venn diagram, ported from Pele's `src/diagrams/venn/render.ts`: the circles of the
//! sets, the outlines of the overlaps a style paints, and the text of each region on the spot
//! the layout found for it, all scaled until each region holds what is written in it.

use super::layout::{Circle, OutlineStep, Spot, Trig, find_spot, layout_circles, region_path};
use super::{VennModel, VennSubset, normalize_text};
use crate::RenderError;
use crate::catalog::DrawContext;
use crate::input::JsText;
use crate::js;
use crate::layout::num::hypot;
use crate::measure::{Emphasis, Label, layout_label};
use crate::scene::{
    Accessible, Entity, Facet, LabelLook, LayerSpec, Number, Paint, PartId, PathProgram, PathStep, Point,
    ResolvedStyle, Role, Scene, SceneBuilder, SeriesSlot, Size, Style, TitleLayout, ink_on, resolve_style,
};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

const RADIUS: f64 = 100.0;
const MAX_GROWTH: f64 = 1.6;

/// One line written in a region: its label, or one of its texts by the text's place in the model.
struct Line {
    text: Option<usize>,
    raw: JsText,
    size: f64,
    base: Emphasis,
    label: Label,
}

struct Region {
    /// The sets of the region, each once, in the order the statement names them.
    sets: Vec<JsText>,
    inside: Vec<usize>,
    spot: Spot,
    /// What is written in the region: its label first, then its texts.
    lines: Vec<Line>,
    style: ResolvedStyle,
    /// Whether its text is written across other circles.
    across: bool,
}

impl Region {
    fn widest(&self) -> f64 {
        self.lines.iter().fold(0.0, |wide, line| js::max(wide, line.label.width))
    }

    fn tall(&self) -> f64 {
        self.lines.iter().fold(0.0, |tall, line| tall + line.label.height)
    }
}

/// The key of a region or a style: the sorted ids of a set or overlap, or the id of a text.
type Key = Vec<JsText>;

/// A stable name for a list of ids. Ids hold no `"`, so the list as JSON names it once.
fn key_name(sets: &[JsText]) -> JsText {
    let mut name = JsText::from("[");
    for (index, id) in sets.iter().enumerate() {
        if index > 0 {
            name.push(u16::from(b','));
        }
        name.push(u16::from(b'"'));
        name.push_units(id);
        name.push(u16::from(b'"'));
    }
    name.push(u16::from(b']'));
    name
}

/// What the region's label and texts are drawn on: the overlap drawn last over all of it, or its
/// own circle when its style sets an opacity, or nothing.
fn under<'a>(region: &'a Region, painted: &[&'a Region]) -> Option<&'a Style> {
    if region.across {
        return None;
    }
    let own = region.style.shape.as_ref().filter(|shape| region.sets.len() == 1 && shape.sets("fill-opacity"));
    let mut fill = own;
    for other in painted {
        if other.sets.iter().all(|id| region.sets.contains(id)) {
            fill = other.style.shape.as_ref();
        }
    }
    fill
}

fn outline(steps: &[OutlineStep]) -> PathProgram {
    let pair = |[x, y]: [f64; 2]| [Number::num(x), Number::num(y)];
    let steps = steps
        .iter()
        .map(|step| match *step {
            OutlineStep::Move(to) => PathStep::Move(pair(to)),
            OutlineStep::Line(to) => PathStep::Line(pair(to)),
            OutlineStep::Arc { r, large, end } => {
                PathStep::Arc { radii: [Number::num(r); 2], large, sweep: true, end: pair(end) }
            }
            OutlineStep::Close => PathStep::Close,
        })
        .collect();
    PathProgram { steps }
}

/// Draws the diagram with `trig` answering the `Math` calls of the layout.
pub(super) fn draw(model: &VennModel, cx: &DrawContext<'_>, trig: &dyn Trig) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.875);
    let tiny = js::round(size * 0.8125);
    let measurer = cx.measure;
    let pad = cx.options.padding;

    let circles = layout_circles(&model.subsets, trig);
    let index: HashMap<&JsText, usize> = circles.iter().enumerate().map(|(i, c)| (&c.id, i)).collect();

    // Styles for the same targets add up, later declarations winning.
    let mut styles: IndexMap<&Key, IndexMap<&JsText, &JsText>> = IndexMap::new();
    for style in &model.styles {
        let merged = styles.entry(&style.targets).or_default();
        for (property, value) in &style.styles {
            merged.insert(property, value);
        }
    }
    let style_of = |key: &[JsText]| -> ResolvedStyle {
        let Some(declared) = styles.get(&key.to_vec()) else {
            return ResolvedStyle::default();
        };
        let list: Vec<JsText> = declared
            .iter()
            .map(|(property, value)| {
                let mut declaration = (*property).clone();
                declaration.push(u16::from(b':'));
                declaration.push_units(value);
                declaration
            })
            .collect();
        resolve_style(&list)
    };

    // Circles that touch each circle; only those can cut into a region of it.
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); circles.len()];
    for a in 0..circles.len() {
        for b in a + 1..circles.len() {
            if hypot(circles[a].x - circles[b].x, circles[a].y - circles[b].y) < circles[a].r + circles[b].r {
                neighbors[a].push(b);
                neighbors[b].push(a);
            }
        }
    }

    // One region for each set and each stated overlap; a later statement about the same sets
    // replaces an earlier one.
    let mut stated: IndexMap<&Key, &VennSubset> = IndexMap::new();
    for subset in &model.subsets {
        stated.insert(&subset.sets, subset);
    }
    let mut regions: IndexMap<&Key, Region> = IndexMap::new();
    for (&key, subset) in &stated {
        let mut seen = HashSet::new();
        let sets: Vec<JsText> = subset.sets.iter().filter(|id| seen.insert(*id)).cloned().collect();
        let inside: Vec<usize> = sets.iter().map(|id| index[id]).collect();
        let mut smallest = inside[0];
        for &i in &inside {
            if circles[i].r < circles[smallest].r {
                smallest = i;
            }
        }
        let style = style_of(key);
        let text = subset.label.clone().or_else(|| (subset.sets.len() == 1).then(|| subset.sets[0].clone()));
        let base = Emphasis { bold: style.bold, italic: style.italic, ..Emphasis::default() };
        let others: Vec<usize> = neighbors[smallest].iter().copied().filter(|i| !inside.contains(i)).collect();
        let mut spot = find_spot(&circles, &inside, &others, trig);
        // A region that other circles cover, or all but cover, is written across them instead.
        let across = !others.is_empty() && spot.margin < circles[smallest].r * 0.2;
        if across {
            spot = find_spot(&circles, &inside, &[], trig);
        }
        let mut lines = Vec::new();
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            let font_size = if sets.len() == 1 { size } else { small };
            let label = layout_label(&text, false, measurer, font_size, 4000.0, base);
            lines.push(Line { text: None, raw: text, size: font_size, base, label });
        }
        regions.insert(key, Region { sets, inside, spot, lines, style, across });
    }
    for (place, node) in model.text_nodes.iter().enumerate() {
        let raw = node.label.clone().unwrap_or_else(|| node.id.clone());
        if let Some(region) = regions.get_mut(&node.sets) {
            let label = layout_label(&raw, false, measurer, tiny, 4000.0, Emphasis::default());
            region.lines.push(Line { text: Some(place), raw, size: tiny, base: Emphasis::default(), label });
        }
    }

    // Lengths so far are in units of the layout. The scale gives the largest circle a set
    // radius, and grows, within limits, until every region holds what is written in it.
    let mut largest = 0.0;
    for c in &circles {
        if c.r > largest {
            largest = c.r;
        }
    }
    let mut scale = if largest > 0.0 { RADIUS / largest } else { 1.0 };
    let mut growth: f64 = 1.0;
    for region in regions.values() {
        if region.spot.margin <= 0.0 {
            continue;
        }
        let (mut width, mut height) = (0.0, 0.0);
        for line in &region.lines {
            width = js::max(width, line.label.width);
            height += line.label.height;
        }
        let need = hypot(width, height) / 2.0 + 4.0;
        growth = js::max(growth, js::min(need / (region.spot.margin * scale), MAX_GROWTH));
    }
    scale *= growth;

    // What is still too wide for its region is wrapped to it.
    for region in regions.values_mut() {
        let room = js::max(region.spot.margin * scale * 1.8, 72.0);
        for line in &mut region.lines {
            if line.label.width > room {
                line.label = layout_label(&line.raw, false, measurer, line.size, room, line.base);
            }
        }
    }

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for c in &circles {
        min_x = js::min(min_x, (c.x - c.r) * scale);
        min_y = js::min(min_y, (c.y - c.r) * scale);
        max_x = js::max(max_x, (c.x + c.r) * scale);
        max_y = js::max(max_y, (c.y + c.r) * scale);
    }
    for region in regions.values() {
        let (wide, tall) = (region.widest(), region.tall());
        min_x = js::min(min_x, region.spot.x * scale - wide / 2.0);
        max_x = js::max(max_x, region.spot.x * scale + wide / 2.0);
        min_y = js::min(min_y, region.spot.y * scale - tall / 2.0);
        max_y = js::max(max_y, region.spot.y * scale + tall / 2.0);
    }
    if circles.is_empty() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 0.0, 0.0);
    }

    let title_text = model.title.as_deref().map(normalize_text).unwrap_or_default();
    let title = layout_label(&title_text, false, measurer, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = js::max(max_x - min_x, title.width);
    let ox = pad + (inner - (max_x - min_x)) / 2.0 - min_x;
    let oy = pad + title_height - min_y;
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let placed: Vec<Circle> = circles
        .iter()
        .map(|c| Circle { id: c.id.clone(), x: c.x * scale + ox, y: c.y * scale + oy, r: c.r * scale })
        .collect();

    let mut out = SceneBuilder::new(cx, Size { width, height }, Accessible::default());
    out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });

    let set_part = |out: &mut SceneBuilder, id: &JsText| out.part(Entity::node(id));
    out.layer(LayerSpec::class("pele-venn-sets").keep_empty(), |out| {
        for (i, c) in placed.iter().enumerate() {
            let colour = Paint::Series(SeriesSlot::new(i));
            let style = regions.get(&vec![c.id.clone()]).and_then(|region| region.style.shape.clone());
            let part = set_part(out, &c.id);
            out.element(part, Facet::Shape)
                .class("pele-venn-set")
                .data_id()
                .circle_at([Some(Number::num(c.x)), Some(Number::num(c.y))], Number::num(c.r))
                .fill(colour)
                .fill_opacity(Number::raw(0.2))
                .stroke(colour)
                .style(style)
                .finish();
        }
    });

    let painted: Vec<&Region> =
        regions.values().filter(|region| region.sets.len() > 1 && region.style.shape.is_some()).collect();
    let region_part = |out: &mut SceneBuilder, key: &Key, region: &Region| -> PartId {
        if key.len() == 1 {
            return set_part(out, &key[0]);
        }
        let mut written = JsText::default();
        for (index, id) in region.sets.iter().enumerate() {
            if index > 0 {
                written.push(u16::from(b'|'));
            }
            written.push_units(id);
        }
        out.part(Entity::overlap(&written, &key_name(key)))
    };

    out.layer(LayerSpec::class("pele-venn-overlaps"), |out| {
        for (&key, region) in &regions {
            // An overlap is only drawn when a style gives it a look of its own; otherwise the
            // circles show it.
            if region.sets.len() < 2 || region.style.shape.is_none() {
                continue;
            }
            let shared: Vec<Circle> = region.inside.iter().map(|&i| placed[i].clone()).collect();
            if let Some(steps) = region_path(&shared, trig) {
                let part = region_part(out, key, region);
                out.element(part, Facet::Shape)
                    .class("pele-venn-overlap")
                    .data_id()
                    .path(outline(&steps))
                    .fill(Paint::None)
                    .style(region.style.shape.clone())
                    .finish();
            }
        }
    });

    let mut ranks: HashMap<&JsText, u32> = HashMap::new();
    let text_parts: Vec<Entity> = model
        .text_nodes
        .iter()
        .map(|node| {
            let rank = ranks.entry(&node.id).or_insert(0);
            *rank += 1;
            Entity::note(&node.id).ranked(*rank - 1)
        })
        .collect();
    let mut text_parts: Vec<Option<Entity>> = text_parts.into_iter().map(Some).collect();
    out.layer(LayerSpec::class("pele-venn-labels"), |out| {
        for (&key, region) in &regions {
            let total = region.tall();
            if total == 0.0 {
                continue;
            }
            let x = region.spot.x * scale + ox;
            let mut y = region.spot.y * scale + oy - total / 2.0;
            let fill = under(region, &painted);
            let part = region_part(out, key, region);
            out.layer(LayerSpec::class("pele-venn-region").part(part, Facet::Label).data_id(), |out| {
                for line in &region.lines {
                    let at = Point { x, y: y + line.label.height / 2.0 };
                    match line.text {
                        None => {
                            let own = region.style.text.as_ref();
                            let colour = ink_on(fill, own).map(Paint::Role);
                            let mut look = LabelLook::class("pele-label");
                            if line.size != size {
                                look = look.font_size(Number::raw(line.size));
                            }
                            out.label(part, Facet::Label, &line.label, at, look.fill(colour).style(own));
                        }
                        Some(place) => {
                            let own = style_of(std::slice::from_ref(&model.text_nodes[place].id));
                            let colour = ink_on(fill, own.text.as_ref()).unwrap_or(Role::Muted);
                            let entity = text_parts[place].take().expect("a text is drawn once");
                            let text = out.part(entity);
                            let look = LabelLook::class("pele-venn-text")
                                .data_id()
                                .font_size(Number::raw(tiny))
                                .fill(Some(Paint::Role(colour)))
                                .style(own.text.as_ref());
                            out.label(text, Facet::Label, &line.label, at, look);
                        }
                    }
                    y += line.label.height;
                }
            });
        }
    });
    out.finish()
}
