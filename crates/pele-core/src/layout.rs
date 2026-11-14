//! Pele's layout of nodes, groups and edges, which seven diagram types share: flowchart,
//! agentflow, class, er, requirement, usecase and state.
//!
//! A renderer builds one [`CNode`] per node or group and one [`CEdge`] per edge, calls
//! [`compound_layout`], and reads back each node's centre and size and each edge's route and
//! label place. [`shift_layout`] then moves the whole by the diagram's padding and title.
//!
//! Every coordinate is the double Pele computes for the same input.
//! `tests/layout_replay.rs` holds the port to that on the calls recorded from Pele by
//! `scripts/record-layout.mjs`.

#![allow(
    clippy::needless_range_loop,
    reason = "loops keep Pele's index form, where one index often reads several parallel arrays, so the port can be read beside its source"
)]

pub mod components;
pub mod compound;
pub mod layered;
pub mod num;

pub use components::wrap_components;
pub use compound::{
    Axis, CEdge, CNode, Dir, RoutePoint, Size, compound_layout, inherit_crossed_directions, shift_layout,
};
pub use layered::{EdgeStyle, LayeredOptions, ease};
