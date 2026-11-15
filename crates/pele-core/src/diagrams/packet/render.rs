//! Pele's `renderPacket` (`src/diagrams/packet/render.ts`): the fields in rows of bits, each label
//! fitted to its field, the bit numbers above, and the title under it all. On a narrow host the
//! rows are drawn shorter and the labels wrap.

use super::{Block, PacketModel, packet_option};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::{JsText, JsValue, Units, decode_entities};
use crate::measure::{Emphasis, Label, fit_text, layout_label, literal_label, width};
use crate::scene::{
    AnchorKind, Entity, Facet, LabelLook, LayerSpec, Number, Paint, Point, Role, Scene, SceneBuilder, Size, Style,
    TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;

/// What a field shows: its label laid out to wrap, or fitted to one line at a size, or nothing.
#[derive(Clone)]
enum Shown {
    Wrapped(Label),
    Fitted { text: JsText, size: f64, width: f64 },
    Nothing,
}

/// One field as it is drawn.
struct Field {
    block: Block,
    x: f64,
    y: f64,
    w: f64,
    height: f64,
    count: f64,
    shown: Shown,
}

fn draw(model: &PacketModel, cx: &DrawContext<'_>, down: bool) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let label_size = js::round(size * 0.875);
    let bit_size = js::round(size * 0.625);
    let pad = cx.options.padding;
    let plain = Emphasis::default();

    let packet = cx.config.get("packet").and_then(JsValue::as_record);
    let option = |key: &str| packet.and_then(|packet| packet.get(key));
    let row_height = packet_option(cx.config, "rowHeight", 32.0, 1.0, 400.0);
    let mut bit_width = packet_option(cx.config, "bitWidth", 32.0, 1.0, 400.0);
    let padding_x = packet_option(cx.config, "paddingX", 5.0, 0.0, 400.0);
    let padding_y = packet_option(cx.config, "paddingY", 5.0, 0.0, 400.0);
    let show_bits = option("showBits") != Some(&JsValue::Bool(false));
    let descending = matches!(option("bitOrder"), Some(JsValue::String(order)) if order.is_str("descending"));
    let mut bits_per_row = model.bits_per_row;
    let mut rows: Vec<Vec<Block>> = model.rows.clone();
    if down {
        let room = cx.options.max_width.unwrap_or(320.0) - 2.0 * pad;
        let count = js::max(1.0, (room / js::min(bit_width, 18.0)).floor());
        // A byte is the shortest row. Below that every label is one character to a line, and a
        // long packet is a row to a bit.
        bits_per_row = js::min(bits_per_row, js::max(8.0, js::pow(2.0, libm::log2(count).floor())));
        let last = model.rows.last().and_then(|row| row.last()).map_or(0.0, |block| block.end);
        if ((last + 1.0) / bits_per_row).ceil() > 10000.0 {
            bits_per_row = model.bits_per_row;
        }
        bit_width = js::min(bit_width, js::max(18.0, room / bits_per_row));
        // The model has split each field at its own rows. Its pieces are joined again first, so
        // that a field is only split where one of the new rows ends. A piece counts one bit short.
        let piece = |block: &Block| block.bits == block.end - block.start;
        let mut whole: Vec<Block> = Vec::new();
        for block in model.rows.iter().flatten() {
            match whole.last_mut() {
                Some(prev)
                    if piece(prev)
                        && piece(block)
                        && prev.label == block.label
                        && block.start == prev.end + 1.0
                        && block.start % model.bits_per_row == 0.0 =>
                {
                    prev.end = block.end;
                    prev.bits = prev.end - prev.start;
                }
                _ => whole.push(block.clone()),
            }
        }
        rows = Vec::new();
        for block in &whole {
            let mut start = block.start;
            while start <= block.end {
                let end = js::min(block.end, ((start / bits_per_row).floor() + 1.0) * bits_per_row - 1.0);
                let index = (start / bits_per_row).floor() as usize;
                if rows.len() <= index {
                    rows.resize_with(index + 1, Vec::new);
                }
                rows[index].push(Block { start, end, ..block.clone() });
                start = end + 1.0;
            }
        }
    }
    let label_of = |block: &Block| decode_entities(&block.label).trim_js().to_vec();
    let wrapped: Vec<Vec<Label>> = if down {
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|block| {
                        let room = js::max(
                            1.0,
                            (block.end - block.start + 1.0) * bit_width - js::min(padding_x, bit_width - 1.0) - 6.0,
                        );
                        literal_label(&label_of(block), cx.measure, label_size, room, plain)
                    })
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    // Bit numbers sit above each row and need room of their own.
    let numbers = if show_bits { bit_size + 4.0 } else { 0.0 };
    let row_heights: Vec<f64> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let mut height = row_height;
            if down {
                for label in &wrapped[index][..row.len()] {
                    height = js::max(height, label.height + 10.0);
                }
            }
            height
        })
        .collect();
    let mut tops = Vec::with_capacity(rows.len());
    let mut rows_height = 0.0;
    for height in &row_heights {
        tops.push(rows_height);
        rows_height += numbers + height + padding_y;
    }
    if !rows.is_empty() {
        rows_height -= padding_y;
    }

    let gap = js::min(padding_x, bit_width - 1.0);
    let packet_width = if rows.is_empty() { 0.0 } else { bits_per_row * bit_width - gap };
    let title_room = if down { fit::title_room(cx.options) } else { 4000.0 };
    let title =
        layout_label(model.title.as_deref().unwrap_or_default(), false, cx.measure, size, title_room, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + if rows_height > 0.0 { 12.0 } else { 0.0 } } else { 0.0 };

    // The pieces of a field split across rows share its label, so each width is fitted once.
    let mut fits: HashMap<(JsText, u64), Shown> = HashMap::new();
    let mut fitted = |label: &JsText, max: f64| -> Shown {
        fits.entry((label.clone(), max.to_bits()))
            .or_insert_with(|| {
                let text = decode_entities(label).trim_js().to_vec();
                match fit_text(&text, max, label_size, js::round(size * 0.5), cx.measure, plain) {
                    Some(fitted) => Shown::Fitted { text: fitted.text, size: fitted.size, width: fitted.width },
                    None => Shown::Nothing,
                }
            })
            .clone()
    };

    let left = pad + js::max(0.0, (title.width - packet_width) / 2.0);
    let mut fields = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let y = pad + tops[index] + numbers;
        let height = row_heights[index];
        for (place, block) in row.iter().enumerate() {
            let count = block.end - block.start + 1.0;
            let first = block.start % bits_per_row;
            let x = left + (if descending { bits_per_row - first - count } else { first }) * bit_width;
            let w = count * bit_width - gap;
            let shown = if down {
                Shown::Wrapped(wrapped[index][place].clone())
            } else {
                match fitted(&block.label, w - 6.0) {
                    Shown::Fitted { size: fitted, .. } if fitted > height => Shown::Nothing,
                    shown => shown,
                }
            };
            fields.push(Field { block: block.clone(), x, y, w, height, count, shown });
        }
    }

    let total_width = (js::max(packet_width, title.width) + 2.0 * pad).ceil();
    let total_height = (rows_height + title_height + 2.0 * pad).ceil();
    let mut out = SceneBuilder::new(cx, Size { width: total_width, height: total_height }, model.accessible.clone());
    let parts: Vec<_> = fields
        .iter()
        .map(|field| {
            let name = format!("{}-{}", js::to_string(field.block.start), js::to_string(field.block.end));
            out.part(Entity::node(&JsText::from(name.as_str())))
        })
        .collect();

    let field_layer =
        LayerSpec::class("pele-packet-fields").font_size(Number::raw(label_size)).anchor(AnchorKind::Middle);
    out.layer(field_layer, |out| {
        for (field, &part) in fields.iter().zip(&parts) {
            out.layer(LayerSpec::class("pele-node").part(part, Facet::Shape).data_id(), |out| {
                out.element(part, Facet::Shape)
                    .rect(
                        [Number::num(field.x), Number::num(field.y)],
                        [Number::num(field.w), Number::num(field.height)],
                        None,
                    )
                    .fill(Paint::Role(Role::Surface))
                    .stroke(Paint::Role(Role::Border))
                    .style(Some(Style::rounded(None)))
                    .finish();
                let centre = Point { x: field.x + field.w / 2.0, y: field.y + field.height / 2.0 };
                match &field.shown {
                    Shown::Wrapped(label) => out.label(part, Facet::Label, label, centre, LabelLook::default()),
                    Shown::Fitted { text, size, width } => {
                        let at = [Number::num(centre.x), Number::num(centre.y + size * 0.35)];
                        let start = out.text(part, Facet::Label).at(at);
                        let start = if *size == label_size { start } else { start.font_size(Number::num(*size)) };
                        start.finish(text, *width, *size);
                    }
                    Shown::Nothing => {}
                }
            });
        }
    });
    if show_bits {
        let bits_layer =
            LayerSpec::class("pele-packet-bits").font_size(Number::raw(bit_size)).fill(Paint::Role(Role::Muted));
        out.layer(bits_layer, |out| {
            for (field, &part) in fields.iter().zip(&parts) {
                let block = &field.block;
                let (leading, trailing) = if descending { (block.end, block.start) } else { (block.start, block.end) };
                let y = Number::num(field.y - 4.0);
                let number = |out: &mut SceneBuilder, value: f64, x: f64, anchor: Option<AnchorKind>| {
                    let text: Vec<u16> = js::to_string(value).encode_utf16().collect();
                    let advance = width(cx.measure, &text, bit_size, plain);
                    let start = out.text(part, Facet::Annotation).at([Number::num(x), y]);
                    let start = match anchor {
                        Some(anchor) => start.anchor(anchor),
                        None => start,
                    };
                    start.finish(&text, advance, bit_size);
                };
                if field.count == 1.0 {
                    number(out, leading, field.x + field.w / 2.0, Some(AnchorKind::Middle));
                } else {
                    number(out, leading, field.x, None);
                    number(out, trailing, field.x + field.w, Some(AnchorKind::End));
                }
            }
        });
    }
    // Mermaid puts a packet's title under it.
    let title_at = Point { x: total_width / 2.0, y: total_height - pad - title.height / 2.0 };
    out.title(&TitleLayout { label: title, at: title_at });
    out.finish()
}

pub(crate) struct Packet;

impl Diagram for Packet {
    type Model = PacketModel;

    fn parse(cx: &ParseContext<'_>) -> Result<PacketModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderPacket`: rows as wide as the config sets, or shorter ones on a narrow host.
    fn scene(model: &PacketModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        fit::turn_to_fit(cx.options, |down| draw(model, cx, down))
    }
}
