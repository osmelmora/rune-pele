//! Crow's foot cardinality markers, ported from Pele's `src/diagrams/er/markers.ts`.

use super::Cardinality;
use crate::graph::edges::Tip;
use crate::scene::{Facet, Number, Paint, PartId, PathProgram, SceneBuilder};

/// How far a marker reaches back along its line from the box it touches.
pub(super) const MARKER_LENGTH: f64 = 18.0;

const BAR: f64 = 5.0;
const FOOT: f64 = 6.0;
const FOOT_LENGTH: f64 = 10.0;
const RING: f64 = 3.5;

/// Where a marker is drawn, and what fills its rings and its diamond.
#[derive(Clone, Copy)]
pub(super) enum MarkerUse {
    /// On its edge, with the class marker styles apply to, hollow in `inside`: the page's colour,
    /// or the colour of the fill the edge lies on.
    Edge { inside: Paint },
    /// Under its edge as part of the edge's casing, with no class so that marker styles skip it.
    Casing,
}

/// Pele's `erMarker`: the marker for `kind` over the end of a line that meets a box at the tip,
/// travelling along the tip's direction.
pub(super) fn draw(out: &mut SceneBuilder, part: PartId, facet: Facet, kind: Cardinality, tip: Tip, used: MarkerUse) {
    let Tip { x, y, dx, dy } = tip;
    let at = |back: f64, side: f64| [Number::num(x - dx * back - dy * side), Number::num(y - dy * back + dx * side)];
    let bar = |path: PathProgram, back: f64| {
        let [ax, ay] = at(back, -BAR);
        let [bx, by] = at(back, BAR);
        path.m(ax, ay).l(bx, by)
    };
    let foot = || {
        let [ax, ay] = at(0.0, -FOOT);
        let [bx, by] = at(FOOT_LENGTH, 0.0);
        let [cx, cy] = at(0.0, FOOT);
        PathProgram::default().m(ax, ay).l(bx, by).l(cx, cy)
    };
    let (class, inside) = match used {
        MarkerUse::Edge { inside } => (true, inside),
        MarkerUse::Casing => (false, Paint::Role(crate::scene::Role::Background)),
    };
    let path = |out: &mut SceneBuilder, d: PathProgram, filled: bool| {
        let element = out.element(part, facet);
        let element = if class { element.class("pele-marker") } else { element };
        let shape = element.path(d);
        if filled { shape.fill(inside) } else { shape }.finish();
    };
    let ring = |out: &mut SceneBuilder, back: f64| {
        let element = out.element(part, facet);
        let element = if class { element.class("pele-marker") } else { element };
        let centre = [Some(Number::num(x - dx * back)), Some(Number::num(y - dy * back))];
        element.circle_at(centre, Number::raw(RING)).fill(inside).finish();
    };
    match kind {
        Cardinality::OnlyOne => path(out, bar(bar(PathProgram::default(), 6.0), 10.0), false),
        Cardinality::ZeroOrOne => {
            path(out, bar(PathProgram::default(), 6.0), false);
            ring(out, 13.5);
        }
        Cardinality::OneOrMore => path(out, bar(foot(), FOOT_LENGTH + 3.5), false),
        Cardinality::ZeroOrMore => {
            path(out, foot(), false);
            ring(out, FOOT_LENGTH + RING + 0.5);
        }
        Cardinality::MdParent => {
            let [ax, ay] = at(0.0, 0.0);
            let [bx, by] = at(6.0, 4.0);
            let [cx, cy] = at(12.0, 0.0);
            let [ex, ey] = at(6.0, -4.0);
            path(out, PathProgram::default().m(ax, ay).l(bx, by).l(cx, cy).l(ex, ey).z(), true);
        }
    }
}
