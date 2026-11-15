//! Where a group's title sits, ported from Pele's `src/svg/cluster.ts`: along the group's top
//! edge, clear of the edges that enter there where it can be. A title that an edge still runs
//! through is drawn over the edges, on an outline in the colour behind it.

use crate::js;
use crate::layout::{CNode, RoutePoint};
use crate::measure::Label;
use crate::scene::{Facet, LabelLook, LayerSpec, LineJoin, Number, Paint, PartId, Point, Role, SceneBuilder};
use std::collections::HashMap;

/// The x of every route point, by its rounded y: the edges that cross a group's top border.
#[derive(Default)]
pub(crate) struct Crossings {
    by_y: HashMap<u64, Vec<f64>>,
}

/// A rounded y as a map key, where `-0` is `0` and every NaN is one key, as in a JavaScript `Map`.
fn key(y: f64) -> u64 {
    let rounded = js::round(y);
    if rounded.is_nan() { f64::NAN.to_bits() } else { (rounded + 0.0).to_bits() }
}

impl Crossings {
    pub(crate) fn mark(&mut self, route: &[RoutePoint]) {
        for point in route {
            self.by_y.entry(key(point.y)).or_default().push(point.x);
        }
    }

    fn at(&self, y: f64) -> Option<&Vec<f64>> {
        self.by_y.get(&key(y))
    }

    /// The x of every route point at the rounded `y`, in the order the routes were marked,
    /// for a diagram type that places its titles by a rule of its own.
    pub(crate) fn xs(&self, y: f64) -> Option<&[f64]> {
        self.at(y).map(Vec::as_slice)
    }
}

/// Pele's `clusterTitleX`: where a group's title, `width` wide, sits along the top edge so that
/// edges entering there do not cross it.
pub(crate) fn cluster_title_x(c: &CNode, width: f64, crossings: &Crossings, pad: f64) -> f64 {
    if width == 0.0 {
        return c.x;
    }
    let top = c.y - c.h / 2.0;
    let left = c.x - c.w / 2.0;
    let half = width / 2.0;
    let candidates = [left + pad - 6.0 + half, c.x, left + c.w - pad + 6.0 - half];
    let Some(xs) = crossings.at(top) else {
        return candidates[0];
    };
    let mut best = candidates[0];
    let mut best_gap = -1.0;
    for x in candidates {
        let mut gap = f64::INFINITY;
        for &cx in xs {
            if cx > left && cx < left + c.w {
                gap = js::min(gap, (cx - x).abs() - half);
            }
        }
        if gap >= 6.0 {
            return x;
        }
        if gap > best_gap + 0.5 {
            best_gap = gap;
            best = x;
        }
    }
    best
}

/// Pele's `titleCrossed`: whether an edge that enters at the top runs through a title placed at `x`.
pub(crate) fn title_crossed(c: &CNode, width: f64, x: f64, crossings: &Crossings) -> bool {
    let Some(xs) = (if width > 0.0 { crossings.at(c.y - c.h / 2.0) } else { None }) else {
        return false;
    };
    let left = c.x - c.w / 2.0;
    xs.iter().any(|&cx| cx > left && cx < left + c.w && (cx - x).abs() < width / 2.0 + 4.0)
}

/// Pele's `struckTitle`: a title that an edge runs through, drawn over the edges. The text sits
/// on an outline in the colour of the group behind it, so that only the line is seen to stop
/// short of the letters. `depth` is how many groups deep the title is, each of which is half its
/// fill over what is behind it.
pub(crate) fn draw_struck_title(
    out: &mut SceneBuilder,
    part: PartId,
    label: &Label,
    at: Point,
    look: LabelLook,
    depth: i32,
) {
    let percent = 100.0 * (1.0 - 0.5f64.powi(depth));
    let behind = Paint::Mix { tint: Role::SurfaceAlt, percent, base: Role::Background };
    out.layer(LayerSpec::class("pele-cluster-title").part(part, Facet::Title).data_id(), |out| {
        let halo = LabelLook::default().fill(Some(Paint::None)).stroke(behind, Number::raw(5.0), LineJoin::Round);
        out.label(part, Facet::Halo, label, at, halo);
        out.label(part, Facet::Title, label, at, look);
    });
}
