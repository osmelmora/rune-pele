//! The markers at the ends of a class relation, ported from Pele's
//! `src/diagrams/class/markers.ts`. They inherit the edge's stroke, and a hollow one its fill of
//! `none`.

use super::RelationEnd;
use crate::graph::edges::{Marker, MarkerUse, Tip, draw_marker};
use crate::scene::{Facet, LineJoin, Number, Paint, PartId, PathProgram, SceneBuilder};

const TRIANGLE: f64 = 12.0;
const DIAMOND: f64 = 16.0;
const HALF: f64 = 6.0;
const RING: f64 = 5.0;

/// Pele's `classMarkerTrim`: how far the line stops short of the node, so that it meets the back
/// of the marker.
pub(super) fn trim(end: RelationEnd) -> f64 {
    match end {
        RelationEnd::Extension => TRIANGLE,
        RelationEnd::Composition | RelationEnd::Aggregation => DIAMOND,
        RelationEnd::Lollipop => 2.0 * RING,
        RelationEnd::Dependency => Marker::Point.trim(),
        RelationEnd::None => 0.0,
    }
}

fn pair(x: f64, y: f64) -> (Number, Number) {
    (Number::num(x), Number::num(y))
}

/// Pele's `classMarker`: the marker whose tip is at `tip`, pointing along its direction. A
/// composition is filled with `colour`.
pub(super) fn draw(out: &mut SceneBuilder, part: PartId, facet: Facet, end: RelationEnd, tip: Tip, colour: Paint) {
    let Tip { x, y, dx, dy } = tip;
    let nx = -dy;
    let ny = dx;
    let path = |points: &[(f64, f64)]| {
        let (x, y) = pair(points[0].0, points[0].1);
        let mut d = PathProgram::default().m(x, y);
        for &(px, py) in &points[1..] {
            let (px, py) = pair(px, py);
            d = d.l(px, py);
        }
        d.z()
    };
    match end {
        RelationEnd::Extension => {
            let bx = x - dx * TRIANGLE;
            let by = y - dy * TRIANGLE;
            let d = path(&[(x, y), (bx + nx * HALF, by + ny * HALF), (bx - nx * HALF, by - ny * HALF)]);
            out.element(part, facet).class("pele-marker").path(d).join(LineJoin::Round).finish();
        }
        RelationEnd::Composition | RelationEnd::Aggregation => {
            let mx = x - (dx * DIAMOND) / 2.0;
            let my = y - (dy * DIAMOND) / 2.0;
            let half = HALF - 1.0;
            let d = path(&[
                (x, y),
                (mx + nx * half, my + ny * half),
                (x - dx * DIAMOND, y - dy * DIAMOND),
                (mx - nx * half, my - ny * half),
            ]);
            let marker = out.element(part, facet).class("pele-marker").path(d).join(LineJoin::Round);
            if end == RelationEnd::Composition { marker.fill(colour) } else { marker }.finish();
        }
        RelationEnd::Lollipop => {
            let centre = [Some(Number::num(x - dx * RING)), Some(Number::num(y - dy * RING))];
            out.element(part, facet).class("pele-marker").circle_at(centre, Number::raw(RING)).finish();
        }
        RelationEnd::Dependency => draw_marker(out, part, facet, Marker::Point, tip, MarkerUse::Edge(colour)),
        RelationEnd::None => {}
    }
}
