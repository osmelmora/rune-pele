//! Packet diagrams, ported from Pele's `src/diagrams/packet`. The parser reads the fields of
//! Mermaid's grammar, `build_packet` checks that they run on from each other and splits each at
//! the ends of its rows, and `render` draws the rows of fields as a Scene.

pub(crate) mod json;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsObject, JsText, JsValue, Units, is_space};
use crate::js;
use crate::scene::Accessible;

pub(crate) use render::Packet;

/// Pele's `PacketBlock`: a field, or the piece of one that falls in one row. Its numbers can be
/// as large as a source writes, up to infinity.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Block {
    pub(crate) start: f64,
    pub(crate) end: f64,
    /// As Mermaid counts it: one less than the width for the pieces of a field split across rows.
    pub(crate) bits: f64,
    pub(crate) label: JsText,
}

/// Pele's `PacketModel`.
#[derive(Clone, Debug)]
pub(crate) struct PacketModel {
    pub(crate) title: Option<JsText>,
    pub(crate) accessible: Accessible,
    pub(crate) bits_per_row: f64,
    pub(crate) rows: Vec<Vec<Block>>,
}

const MAX_ROWS: usize = 10_000;

/// Pele's `packetOption`: a number of the `packet` config at least `min`, at most `max`.
pub(crate) fn packet_option(config: &JsObject, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match config.get("packet").and_then(JsValue::as_record).and_then(|packet| packet.get(key)) {
        Some(&JsValue::Number(value)) if value >= min => js::min(value, max),
        _ => fallback,
    }
}

/// `text.replace(/\n\s+/g, '\n')`
fn unindent(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        let spaces = text.run(at + 1, is_space);
        at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
    }
    out
}

/// Pele's `buildPacket`. The error is Pele's message for a field that does not run on.
fn build_packet(
    ast: parser::PacketAst,
    config: &JsObject,
    front_title: Option<&JsText>,
) -> Result<PacketModel, String> {
    let bits_per_row = packet_option(config, "bitsPerRow", 32.0, 1.0, 4096.0).floor();
    let n = js::to_string;
    let mut rows: Vec<Vec<Block>> = Vec::new();
    let mut last_bit = -1.0;
    let mut word: Vec<Block> = Vec::new();
    let mut row = 1.0;

    for block in ast.blocks {
        let label = JsText::from(block.label);
        if let (Some(start), Some(end)) = (block.start, block.end)
            && end < start
        {
            return Err(format!("Packet block {} - {} is invalid. End must be greater than start.", n(start), n(end)));
        }
        let mut start = block.start.unwrap_or(last_bit + 1.0);
        if start != last_bit + 1.0 {
            let end = block.end.unwrap_or(start);
            return Err(format!(
                "Packet block {} - {} is not contiguous. It should start from {}.",
                n(start),
                n(end),
                n(last_bit + 1.0)
            ));
        }
        if block.bits == Some(0.0) {
            return Err(format!("Packet block {} is invalid. Cannot have a zero bit field.", n(start)));
        }
        let end = block.end.unwrap_or(start + block.bits.unwrap_or(1.0) - 1.0);
        let mut bits = block.bits.unwrap_or(end - start + 1.0);
        last_bit = end;

        while word.len() as f64 <= bits_per_row + 1.0 && rows.len() < MAX_ROWS {
            let row_end = row * bits_per_row - 1.0;
            if end <= row_end {
                word.push(Block { start, end, bits, label: label.clone() });
                if end == row_end {
                    rows.push(std::mem::take(&mut word));
                    row += 1.0;
                }
                break;
            }
            word.push(Block { start, end: row_end, bits: row_end - start, label: label.clone() });
            rows.push(std::mem::take(&mut word));
            row += 1.0;
            start = row_end + 1.0;
            bits = end - start;
        }
    }
    if !word.is_empty() {
        rows.push(word);
    }

    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    Ok(PacketModel {
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        accessible: Accessible {
            title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
            description: filled(ast.acc_descr).map(|text| unindent(&text).into()),
        },
        bits_per_row,
        rows,
    })
}

pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<PacketModel, RenderError> {
    let ast = parser::parse_packet(cx.text).map_err(|error| cx.syntax(error))?;
    build_packet(ast, cx.config, cx.title).map_err(|message| cx.semantic(message))
}
