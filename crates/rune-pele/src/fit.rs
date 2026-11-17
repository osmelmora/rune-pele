//! What a fitted view draws for the width it has: the steps it lays a diagram out at, how much
//! smaller a narrower layout sets plain text, and the zoom that fits a laid-out diagram to that
//! width.

use std::collections::HashMap;

use pele_core::Scene;
use pele_core::scene::{Draw, FragmentKey, Geometry, PaintVisit};

/// Steps a view lays a diagram out at, in an octave. Laid out at the step under the width, a
/// diagram that narrows leaves at most 5.6% of the width unused, and a drag from 900 points to
/// 300 lays it out at 20 steps.
const STEPS_PER_OCTAVE: f64 = 12.0;

/// The widest step that is no wider than `room` points: a whole number of points a twelfth of an
/// octave from the next. Every width between two steps lays out at the lower, so a diagram is
/// laid out again only when the width crosses one.
pub(crate) fn step_below(room: f64) -> f64 {
    let step = |index: f64| (index / STEPS_PER_OCTAVE).exp2().floor();
    // A step is a whole number of points, under its exact width, and the logarithm can round
    // either way: the room is at most one index off.
    let index = (room.log2() * STEPS_PER_OCTAVE).floor();
    let below = [index + 1.0, index, index - 1.0].into_iter().map(step).find(|step| *step <= room);
    below.unwrap_or(1.0).max(1.0)
}

/// How much smaller `laid` draws plain text than `natural`, the same source at its own size: the
/// least share of its size there that `laid` draws a run at which `natural` draws at `plain`
/// points or more, matched by the run's key, and 1 where it draws none smaller.
pub(crate) fn plain_text_shrink(natural: &Scene, laid: &Scene, plain: f64) -> f64 {
    let own: HashMap<FragmentKey, f64> = text_runs(natural).filter(|(_, size)| *size >= plain).collect();
    text_runs(laid).filter_map(|(key, size)| own.get(&key).map(|own| size / own)).fold(1.0, f64::min)
}

/// Each run of text `scene` draws: its key and its size.
fn text_runs(scene: &Scene) -> impl Iterator<Item = (FragmentKey, f64)> + '_ {
    scene.paint().filter_map(|visit| match visit {
        PaintVisit::Draw(Draw { key, geometry: Geometry::Text(run), .. }) => Some((key, run.font.size)),
        _ => None,
    })
}

/// The zoom that draws a diagram `width` points wide in `room`: never more than 1, and never
/// less than `least` unless `least` is more than 1.
pub(crate) fn zoom_into(room: f64, width: f64, least: f64) -> f64 {
    (room / width).min(1.0).max(least.min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_the_widest_of_the_steps_at_or_under_the_room() {
        let steps: Vec<f64> = (80..130).map(|k| (f64::from(k) / 12.0).exp2().floor()).collect();
        for room in (300..=900).map(f64::from).chain([512.0, 511.9, 861.0, 860.99]) {
            let widest = steps.iter().copied().filter(|step| *step <= room).fold(0.0, f64::max);
            assert_eq!(step_below(room), widest, "for {room}");
        }
        let mut crossed: Vec<f64> = (300..=900).rev().map(|room| step_below(f64::from(room))).collect();
        crossed.dedup();
        assert_eq!(crossed.len(), 20, "a drag from 900 to 300 lays out at 20 steps: {crossed:?}");
    }

    #[test]
    fn a_zoom_shrinks_to_fit_but_never_enlarges_nor_goes_under_the_least() {
        assert_eq!(zoom_into(400.0, 800.0, 0.6875), 0.6875);
        assert_eq!(zoom_into(600.0, 800.0, 0.6875), 0.75);
        assert_eq!(zoom_into(900.0, 800.0, 0.6875), 1.0);
        assert_eq!(zoom_into(100.0, 800.0, 1.2), 1.0);
    }
}
