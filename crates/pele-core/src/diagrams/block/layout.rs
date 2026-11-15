//! The block diagram's grid layout, ported from Pele's `src/diagrams/block/layout.ts`: every
//! container lays its children out on a grid, sized from the bottom of the tree up and placed
//! from the top down.

use crate::js;
use crate::{Diagnostic, ErrorCode, RenderError};

/// One block or container of the tree. The tree is an arena: `children` are positions in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub children: Vec<usize>,
    /// Grid width of this container: a positive count, or -1 to put every child on one row.
    pub columns: f64,
    pub span: f64,
    pub space: bool,
    /// Height reserved above a container's grid for its title.
    pub head: f64,
    /// Size the content needs. Set by the caller for leaves, computed here for containers.
    pub nw: f64,
    pub nh: f64,
    /// Placement: grid position, then centre and size in pixels.
    pub col: f64,
    pub row: f64,
    pub cols: f64,
    pub heights: Vec<f64>,
    /// Extra space after each row or column. Pele's arrays may have holes, which count as 0.
    pub row_gaps: Vec<f64>,
    pub col_gaps: Vec<f64>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const MAX_SPAN: f64 = 1000.0;

/// Pele's `cell`.
pub fn cell(nw: f64, nh: f64, span: f64) -> Cell {
    Cell {
        children: Vec::new(),
        columns: -1.0,
        span,
        space: false,
        head: 0.0,
        nw,
        nh,
        col: 0.0,
        row: 0.0,
        cols: 0.0,
        heights: Vec::new(),
        row_gaps: Vec::new(),
        col_gaps: Vec::new(),
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    }
}

fn semantic(message: String) -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message))
}

fn is_integer(value: f64) -> bool {
    value.is_finite() && value.trunc() == value
}

/// Pele's `calculateBlockPosition`: the column and row of the cell at `position`.
fn position(columns: f64, position: f64) -> Result<(f64, f64), RenderError> {
    if columns == 0.0 || !is_integer(columns) {
        return Err(semantic("Columns must be an integer !== 0.".to_owned()));
    }
    if position < 0.0 || !is_integer(position) {
        return Err(semantic(format!("Position must be a non-negative integer.{}", js::to_string(position))));
    }
    if columns < 0.0 {
        return Ok((position, 0.0));
    }
    Ok((position % columns, (position / columns).floor()))
}

fn total(gaps: &[f64]) -> f64 {
    let mut sum = 0.0;
    for gap in gaps {
        sum += gap;
    }
    sum
}

fn gap_at(gaps: &[f64], at: f64) -> f64 {
    gaps.get(at as usize).copied().unwrap_or(0.0)
}

/// Puts each child in its grid cell and sizes the container. Every column is as wide as the
/// widest block needs. Every row that holds a plain block is as tall as the tallest block in
/// the container, as in Mermaid; a row of nothing but containers is only as tall as they need.
fn measure(cells: &mut [Cell], at: usize, gap: f64, inset: f64) -> Result<(), RenderError> {
    let columns = cells[at].columns;
    let mut heights: Vec<f64> = Vec::new();
    let mut position_at = 0.0;
    let mut cols = 0.0;
    let mut cw = 0.0;
    let mut tallest = 0.0;
    for k in 0..cells[at].children.len() {
        let child = &mut cells[cells[at].children[k]];
        let (px, py) = position(columns, position_at)?;
        // Mermaid lets a block wider than the rest of its row stick out of the grid. Here it stops
        // at the last column.
        let mut span = if child.span >= 1.0 { js::min(child.span, MAX_SPAN) } else { 1.0 };
        if columns > 0.0 && span > columns - px {
            span = columns - px;
        }
        child.span = span;
        child.col = px;
        child.row = py;
        position_at += span;
        if px + span > cols {
            cols = px + span;
        }
        let row = py as usize;
        if row == heights.len() {
            heights.push(0.0);
        }
        if child.children.is_empty() {
            heights[row] = -1.0;
        } else if heights[row] >= 0.0 && child.nh > heights[row] {
            heights[row] = child.nh;
        }
        if child.space {
            continue;
        }
        let width = (child.nw - gap * (span - 1.0)) / span;
        if width > cw {
            cw = width;
        }
        if child.nh > tallest {
            tallest = child.nh;
        }
    }
    let cell = &mut cells[at];
    let mut height = (heights.len() as f64 - 1.0) * gap + total(&cell.row_gaps);
    for row in &mut heights {
        if *row < 0.0 {
            *row = tallest;
        }
        height += *row;
    }
    cell.cols = cols;
    cell.heights = heights;
    cell.nw = js::max(cell.nw, cols * cw + (cols - 1.0) * gap + total(&cell.col_gaps) + 2.0 * inset);
    cell.nh = height + 2.0 * inset + cell.head;
    Ok(())
}

/// Gives each child its cell, sharing out any room the container has beyond what it needs.
fn arrange(cells: &mut [Cell], at: usize, gap: f64, inset: f64) {
    let cell = &cells[at];
    let (cols, rows) = (cell.cols, cell.heights.len() as f64);
    let heights = cell.heights.clone();
    let (row_gaps, col_gaps) = (cell.row_gaps.clone(), cell.col_gaps.clone());
    let cw = (cell.w - 2.0 * inset - (cols - 1.0) * gap - total(&col_gaps)) / cols;
    let room = cell.h - 2.0 * inset - cell.head - (rows - 1.0) * gap - total(&row_gaps);
    let mut offsets: Vec<f64> = Vec::new();
    if !col_gaps.is_empty() {
        let mut sum = 0.0;
        let mut col = 0.0;
        while col < cols {
            offsets.push(sum);
            sum += gap_at(&col_gaps, col);
            col += 1.0;
        }
    }
    let mut needed = 0.0;
    for height in &heights {
        needed += height;
    }
    let scale = if needed > 0.0 { room / needed } else { 0.0 };
    let share = if needed > 0.0 { 0.0 } else { room / rows };
    let left = cell.x - cell.w / 2.0 + inset;
    let mut top = cell.y - cell.h / 2.0 + inset + cell.head;
    let mut row = 0;
    for k in 0..cells[at].children.len() {
        let child = &mut cells[cells[at].children[k]];
        while (row as f64) < child.row {
            top += heights[row] * scale + share + gap + gap_at(&row_gaps, row as f64);
            row += 1;
        }
        let before = gap_at(&offsets, child.col);
        child.w = cw * child.span + gap * (child.span - 1.0) + gap_at(&offsets, child.col + child.span - 1.0) - before;
        child.h = heights[row] * scale + share;
        child.x = left + child.col * (cw + gap) + before + child.w / 2.0;
        child.y = top + child.h / 2.0;
    }
}

/// Pele's `layout`: lays out the tree under cell 0 in place, and answers its width and height.
/// The root has no border, so its grid starts at the origin.
pub fn layout(cells: &mut [Cell], gap: f64) -> Result<(f64, f64), RenderError> {
    let mut order = vec![0];
    let mut i = 0;
    while i < order.len() {
        order.extend(cells[order[i]].children.iter().copied());
        i += 1;
    }
    for (i, &at) in order.iter().enumerate().rev() {
        if !cells[at].children.is_empty() {
            measure(cells, at, gap, if i == 0 { 0.0 } else { gap })?;
        }
    }
    let root = &mut cells[0];
    root.w = root.nw;
    root.h = root.nh;
    root.x = root.w / 2.0;
    root.y = root.h / 2.0;
    for (i, &at) in order.iter().enumerate() {
        if !cells[at].children.is_empty() {
            arrange(cells, at, gap, if i == 0 { 0.0 } else { gap });
        }
    }
    Ok((cells[0].w, cells[0].h))
}
