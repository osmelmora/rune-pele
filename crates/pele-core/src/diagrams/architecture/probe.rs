//! An entry point that lets `tests/architecture_layout.rs` hold `placeOnGrid` to Pele on its own,
//! before anything is drawn. It answers in plain values, so the port's types stay private.

use crate::{ParseOptions, RenderError};

/// Pele's `Grid`, with a parent that is the root as -1 and an empty group as 1, as Pele's typed
/// arrays hold them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridProbe {
    pub col: Vec<i64>,
    pub row: Vec<i64>,
    pub group_col: Vec<i64>,
    pub group_row: Vec<i64>,
    pub group_cols: Vec<i64>,
    pub group_rows: Vec<i64>,
    pub group_parent: Vec<i64>,
    pub group_empty: Vec<i64>,
    pub node_parent: Vec<i64>,
}

/// Pele's `placeOnGrid` of the model a source describes. A source that is no architecture
/// diagram, or that Pele rejects, answers its error.
pub fn grid(source: &str) -> Result<GridProbe, RenderError> {
    let options = ParseOptions::default();
    let front = crate::front(source, &options)?;
    let origin = crate::Origin { source };
    let model = super::parse(&front.context(&origin, options.limits))?;
    let grid = super::layout::place_on_grid(&model);
    let parents = |list: &[Option<usize>]| list.iter().map(|p| p.map_or(-1, |p| p as i64)).collect();
    Ok(GridProbe {
        group_parent: parents(&grid.group_parent),
        node_parent: parents(&grid.node_parent),
        group_empty: grid.group_empty.iter().map(|&empty| i64::from(empty)).collect(),
        col: grid.col,
        row: grid.row,
        group_col: grid.group_col,
        group_row: grid.group_row,
        group_cols: grid.group_cols,
        group_rows: grid.group_rows,
    })
}
