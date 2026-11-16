#!/bin/sh
# Breaks one lowering at a time and shows that its feature test then fails.
# usage: crates/rune-pele/break-lowering.sh   (from the workspace root, under `mise exec --`)
# Each break is one exact edit of a source file, undone before the next. Exits 1 if a break
# does not apply any more, or if the tests still pass with it.
set -u
cd "$(dirname "$0")/../.."
src=crates/rune-pele/src
logs=out/verify/rune-graph/break
mkdir -p "$logs"
failed=0

# Puts a file back as it was. cargo goes by modification times, and the copy kept aside is
# older than the build of the broken file, so without the touch cargo would keep that build.
restore() {
  mv "$1.unbroken" "$1"
  touch "$1"
}

# break <name> <file> <test filter> <from> <to> [test binary, `lowering` unless given]
break_one() {
  name=$1 file=$src/$2 filter=$3 from=$4 to=$5 binary=${6:-lowering}
  cp "$file" "$file.unbroken"
  FROM=$from TO=$to perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"
  if [ $? -ne 0 ]; then
    restore "$file"
    echo "break $name: NOT APPLIED, the line to break is gone from $file"
    failed=1
    return
  fi
  if cargo test -q --release -p rune-pele --test "$binary" "$filter" > "$logs/$name.log" 2>&1; then
    echo "break $name: tests still PASS with it; $filter does not catch it"
    failed=1
  else
    echo "break $name: $filter fails, as it should:"
    grep -E "panicked|^[a-z_]+ --- FAILED|assertion|: [a-z].*: \[" "$logs/$name.log" | head -4 | sed 's/^/    /'
  fi
  restore "$file"
}

break_one dash-pattern stroke.rs dashes_keep \
  '*left = pattern[*index];' \
  '*left = pattern[0];'
break_one arc-direction geometry.rs arcs_go_round \
  'let sign = if arc.large == arc.sweep { -1.0 } else { 1.0 };' \
  'let sign = if arc.large == arc.sweep { 1.0 } else { -1.0 };'
break_one hole-ignored fill.rs a_hole_shows \
  '.filter(|&h| around[h].contains(&i) && !filled_inside(h) && filled_around(h))' \
  '.filter(|&h| false && around[h].contains(&i) && !filled_inside(h) && filled_around(h))'
break_one stroke-blended-twice lower.rs a_translucent_stroke \
  'let once = coat.overlaps && colour.a < 1.0;' \
  'let once = false && coat.overlaps && colour.a < 1.0;'
break_one group-faded-piecewise lower.rs a_group_fades \
  'self.frame.placed(el().key(self.key.as_str()), origin).children(children).layer_opacity(self.opacity);' \
  'self.frame.placed(el().key(self.key.as_str()), origin).children(children).dim(self.opacity);'
break_one text-turned-the-other-way lower.rs turned_text_turns \
  'element.rotate(turn.radians).origin(pivot[0], pivot[1])' \
  'element.rotate(-turn.radians).origin(pivot[0], pivot[1])'
break_one glyph-stroke-unfitted ../../pele-core/src/scene/walk.rs a_built_in_glyph \
  'width: GLYPH_STROKE * scale,' \
  'width: GLYPH_STROKE,'
break_one group-turned-without-its-move lower.rs a_group_that_moves_and_turns \
  'Affine::rotate_about(turn.degrees, [turn.centre.x, turn.centre.y]).then(&moved),' \
  'Affine::rotate_about(turn.degrees, [turn.centre.x, turn.centre.y]),'
break_one corners-in-a-line-kept fill.rs a_fill_with_corners \
  'off_line(ring[corner], before, after) <= hair)' \
  'off_line(ring[corner], before, after) < 0.0)'
break_one bare-fill-cut-coarse lower.rs a_curved_edge \
  'let flatness = if look.is_some() { FLATNESS } else { BARE_FLATNESS };' \
  'let flatness = FLATNESS;'
break_one curved-fill-left-whole lower.rs a_curved_edge \
  'if !curved(&polygon) {' \
  'if true {'
break_one gradient-one-colour lower.rs a_fill_along \
  'let step = |from: f32, to: f32| from + (to - from) * along;' \
  'let step = |from: f32, to: f32| from + (to - from) * 0.5;'
break_one gradient-bands-misplaced lower.rs a_fill_along \
  'places.push(((band.middle - from) / (to - from)).clamp(0.0, 1.0) as f32);' \
  'places.push(((band.middle - from) / (to - from)).clamp(0.0, 0.5) as f32);'
break_one gradients-only-with-a-prefix lib.rs a_sankey_fills_its_links \
  'id_prefix: Some(DEFAULT_ID_PREFIX.to_owned()),' \
  'id_prefix: None,'
break_one kept-drawing-dropped lib.rs the_unchanged_look_shows \
  '_ => self.session.borrow().drawn.clone(),' \
  '_ => None,' \
  keep_view
break_one kept-error-unreported lib.rs every_prefix \
  'Some(Outcome::Kept(error) | Outcome::Failed(error)) => Some(error.clone()),' \
  'Some(Outcome::Kept(_)) => None, Some(Outcome::Failed(error)) => Some(error.clone()),' \
  keep_view
break_one kept-faded-piecewise kept.rs the_dimmed_look_fades \
  'col().key(key).child(el().layer_opacity(DIMMED).child(drawing)).child(edge)' \
  'col().key(key).child(el().child(drawing).dim(DIMMED)).child(edge)' \
  keep_view
break_one kept-error-below-the-drawing kept.rs the_dimmed_look_fades \
  '.pin(Some(0.0), Some(-line), Some(0.0), None)' \
  '.pin(Some(0.0), Some(0.0), Some(0.0), None)' \
  keep_view
break_one kept-rune-mermaid-dark-unchanged lib.rs a_palette_or_dark_mode \
  'if drawing.dark == dark && drawing.max_width == max_width {' \
  'if true {' \
  keep_view
break_one zoom-geometry-unzoomed lower.rs a_link_and_a_tooltip_at_half_size \
  'let mut open = vec![Open { to_drawing: Affine::scale(zoom), element: None, children: Vec::new() }];' \
  'let mut open = vec![Open { to_drawing: Affine::IDENTITY, element: None, children: Vec::new() }];' \
  fit_view
break_one zoom-box-unzoomed lower.rs a_link_and_a_tooltip_at_half_size \
  'size.map(|side| side * zoom), radius * zoom,' \
  'size, radius * zoom,' \
  fit_view
break_one zoom-stroke-unzoomed lower.rs a_stroke_is_zoomed \
  'width: stroke.width * zoom,' \
  'width: stroke.width,' \
  fit_view
break_one zoom-text-unzoomed lower.rs text_at_each_zoom \
  'TextStyle { size: style.size * zoom as f32,' \
  'TextStyle { size: style.size,' \
  fit_view
break_one fit-enlarges fit.rs a_fitted_view \
  '(room / width).min(1.0).max(least.min(1.0))' \
  '(room / width).max(least.min(1.0))' \
  fit_view
break_one fit-under-the-smallest-text fit.rs a_fitted_view \
  '(room / width).min(1.0).max(least.min(1.0))' \
  '(room / width).min(1.0)' \
  fit_view
break_one fit-never-narrower lib.rs a_fitted_view \
  'let laid = if self.narrows(source, options, scale, &natural) {' \
  'let laid = if false && self.narrows(source, options, scale, &natural) {' \
  fit_view
break_one fit-at-every-width lib.rs a_resize \
  'max_width: Some(fit::step_below(room)),' \
  'max_width: Some(room),' \
  fit_view
break_one kept-drawn-unfitted lib.rs a_kept_drawing_follows \
  'self.draw_source(env, &natural.key.source, &options, room, on_event).ok()' \
  'self.draw_source(env, &natural.key.source, &options, None, on_event).ok()' \
  kept_fit
break_one kept-outside-the-frame lib.rs a_kept_drawing_follows \
  'Ok(self.framed(options.fit, shown))' \
  'Ok(shown)' \
  kept_fit
break_one frame-inside-the-dimmed-look lib.rs a_kept_drawing_follows \
  'let shown = options.kept.el(drawing, &self.id,' \
  'let shown = options.kept.el(self.framed(options.fit, drawing), &self.id,' \
  kept_fit
break_one kept-layouts-not-held lib.rs sources_that_lay_out_and_then_fail \
  'if let Some(layout) = session.drawn.as_ref().and_then(|drawn| drawn.layout(&key)) {' \
  'if let Some(layout) = session.drawn.as_ref().and_then(|drawn| drawn.layout(&key)).filter(|_| false) {' \
  kept_fit

cargo build -q --release -p rune-pele --tests 2>/dev/null
exit $failed
