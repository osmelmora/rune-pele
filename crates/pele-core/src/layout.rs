//! Pele's layout of nodes, groups and edges, which seven diagram types share: flowchart,
//! agentflow, class, er, requirement, usecase and state.
//!
//! Every coordinate is the double Pele computes for the same input.

#![allow(
    clippy::needless_range_loop,
    reason = "loops keep Pele's index form, where one index often reads several parallel arrays, so the port can be read beside its source"
)]

pub mod layered;
pub mod num;

pub use layered::{EdgeStyle, LayeredOptions, ease};
