//! Gantt charts, ported from Pele's `src/diagrams/gantt`. The lexer and the parser fill Pele's
//! `GanttDb`, scheduling resolves each task's dates with JavaScript's `Date` as V8 runs it in
//! UTC, and `render` draws the schedule on a time axis as a Scene.

mod axis;
mod date;
mod dates;
mod db;
pub mod probe;
