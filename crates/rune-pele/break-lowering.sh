#!/bin/sh
# Breaks one lowering at a time and shows that its feature test then fails.
# usage: crates/rune-pele/break-lowering.sh [name...]   (from the workspace root, under `mise exec --`)
# Each break is one exact edit of a source file, undone before the next. With names, only those
# breaks run. Exits 1 if a break does not apply any more, or if the tests still pass with it.
set -u
cd "$(dirname "$0")/../.."
src=crates/rune-pele/src
logs=out/verify/rune-graph/break
mkdir -p "$logs"
failed=0
only="$*"

# Whether the break named $1 is to run.
chosen() {
  [ -z "$only" ] || printf ' %s ' "$only" | grep -q " $1 "
}

# Puts a file back as it was. cargo goes by modification times, and the copy kept aside is
# older than the build of the broken file, so without the touch cargo would keep that build.
restore() {
  mv "$1.unbroken" "$1"
  touch "$1"
}

# break <name> <file> <test filter> <from> <to> [test binary, `lowering` unless given]
break_one() {
  name=$1 file=$src/$2 filter=$3 from=$4 to=$5 binary=${6:-lowering}
  chosen "$name" || return
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
  'frame.placed(el().key(self.key.as_str()), origin).children(children).layer_opacity(self.opacity);' \
  'frame.placed(el().key(self.key.as_str()), origin).children(children).dim(self.opacity);'
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
  'if self.narrows(source, options, scale, &natural) {' \
  'if false && self.narrows(source, options, scale, &natural) {' \
  fit_view
break_one fit-at-every-width lib.rs a_resize \
  'let step = fit::step_below(room);' \
  'let step = room;' \
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
  'let shown = options.kept.el(drawing, &self.id, &error.to_string(), font_size, critical);
        self.session.borrow_mut().outcome = Some(Outcome::Kept(error));
        Ok(self.framed(options.fit, shown))' \
  'let shown = options.kept.el(self.framed(options.fit, drawing), &self.id, &error.to_string(), font_size, critical);
        self.session.borrow_mut().outcome = Some(Outcome::Kept(error));
        Ok(shown)' \
  kept_fit
break_one kept-layouts-not-held lib.rs sources_that_lay_out_and_then_fail \
  'if let Some(layout) = session.drawn.as_ref().and_then(|drawn| drawn.layout(&key)) {' \
  'if let Some(layout) = session.drawn.as_ref().and_then(|drawn| drawn.layout(&key)).filter(|_| false) {' \
  kept_fit
break_one engine-fallback-as-pele lib.rs the_engine_says \
  'Drawn::RuneMermaid(_) => Some(Engine::RuneMermaid),' \
  'Drawn::RuneMermaid(_) => Some(Engine::Pele),' \
  engine_view
break_one engine-fallback-with-links lib.rs the_engine_says \
  'pub fn has_links(self) -> bool {
        match self {
            Engine::Pele => true,
            Engine::RuneMermaid => false,' \
  'pub fn has_links(self) -> bool {
        match self {
            Engine::Pele => true,
            Engine::RuneMermaid => true,' \
  engine_view
break_one drawing-numbered-per-diagram lib.rs a_diagram_that_replaces_another \
  'let lowered = Rc::new(Lowered { layout: layout.clone(), zoom, id: unique_id(), drawing });' \
  'let lowered = Rc::new(Lowered { layout: layout.clone(), zoom, id: session.stats.lowerings, drawing });' \
  view_identity
break_one frame-keyed-from-the-id lib.rs a_fitted_view_reads_its_own_frame \
  'fn frame_key(&self) -> &str {
        &self.id' \
  'fn frame_key(&self) -> &str {
        Box::leak(format!("{}~fit", self.id).into_boxed_str())' \
  view_identity
break_one pele-metrics-pin-the-clock lib.rs pele_metrics_measure_text \
  'now: options.now.or_else(|| Some(system_now()))' \
  'now: options.now.or_else(|| Some(system_now()).filter(|_| false))' \
  view_identity
break_one fit-under-the-smallest-drawn-text lib.rs fitted_text_keeps_to_the_smallest_size \
  'if shrink >= least {' \
  'if shrink >= 0.0 {' \
  fit_text
break_one fit-turns-only-under-640 lib.rs a_fitted_flowchart_that_runs_across \
  'turn_wider_than: Some(step / least),' \
  'turn_wider_than: None,' \
  fit_text

break_one zero-radius-drawn lower.rs a_point_of_radius_zero \
  'if !draw.geometry.renders() {' \
  'if false && !draw.geometry.renders() {' \
  zero_radius
break_one invalid-fill-unread ../../pele-core/src/scene/css.rs a_colour_no_css_reads \
  '(Grammar::Paint, [one]) => colour(*one),' \
  '(Grammar::Paint, [one]) => true,' \
  invalid_colour
break_one spaces-not-collapsed ../../pele-core/src/scene/walk.rs a_run_of_spaces \
  '_ => collapsed(runs),' \
  '_ => runs,' \
  whitespace
break_one figure-unnamed lower.rs the_diagram_is_named \
  'if let Some(title) = &self.title {' \
  'if let Some(title) = self.title.as_ref().filter(|_| false) {' \
  accessibility
break_one link-named-by-its-address lower.rs a_link_is_a_tab_stop \
  '.label(link.name.as_str())' \
  '.label(link.to.href.as_str())' \
  accessibility
break_one link-out-of-the-tab-order lower.rs a_link_is_a_tab_stop \
  '.focusable(true)' \
  '.focusable(false)' \
  accessibility
break_one link-deaf-to-enter lower.rs a_link_is_a_tab_stop \
  'let follows = key.key == Key::Enter && key.mods == Mods::default();' \
  'let follows = key.key == Key::Escape && key.mods == Mods::default();' \
  accessibility
break_one half-typed-length-applied ../../pele-core/src/scene/css.rs every_prefix_of_each_fixture \
  'Token::Dimension(n, unit) => LENGTH_UNITS.contains(&unit) && (signed || n >= 0.0),' \
  'Token::Dimension(n, _) => signed || n >= 0.0,' \
  keep_view
break_one dropped-value-unread ../../pele-core/src/scene/walk.rs a_value_its_property_does_not_take \
  'if page == Page::Drops || meaning.read[index] {' \
  'if meaning.read[index] {' \
  invalid_value
break_one dropped-value-drawn ../../pele-core/src/scene/style.rs a_value_its_property_does_not_take \
  'if page == Page::Drops {
                continue;' \
  'if page == Page::Drops && false {
                continue;' \
  invalid_value
break_one read-by-an-earlier-declaration ../../pele-core/src/scene/style.rs a_value_its_property_does_not_take \
  'let found = read.is_some();' \
  'let found = read.is_some() || slot.is_some();' \
  invalid_value
break_one open-value-takes-in-nothing ../../pele-core/src/scene/css.rs a_function_left_open \
  'swallowed = followed && value.open.is_some();' \
  'swallowed = false;' \
  invalid_value
break_one calc-of-no-length-applied ../../pele-core/src/scene/css.rs a_length_typed_into_an_edge_style \
  'LENGTH_UNITS.contains(&unit) || degrees(1.0, unit).is_some() || OTHER_UNITS.contains(&unit)' \
  'true' \
  invalid_value

break_one icon-not-drawn lower.rs an_icon_the_app_gives \
  'Some(Some(icon)) => icon.el(brush.paints, origin, &slot.key, slot.opacity),' \
  'Some(Some(_)) => None,' \
  icons
break_one icon-fitted-to-cover icon.rs an_icon_the_app_gives \
  'let scale = (size.width / of.width).min(size.height / of.height);' \
  'let scale = (size.width / of.width).max(size.height / of.height);' \
  icons
break_one icon-uncentred icon.rs an_icon_the_app_gives \
  'let top = origin.y + (size.height - of.height * scale) / 2.0 - from.y * scale;' \
  'let top = origin.y - from.y * scale;' \
  icons
break_one icon-current-not-foreground lower.rs an_icon_the_app_gives \
  'IconPaint::Current => paints.get(Paint::Role(Role::Foreground)).map(colour),' \
  'IconPaint::Current => paints.get(Paint::Role(Role::Muted)).map(colour),' \
  icons
break_one icon-arc-unturned icon.rs an_icon_the_app_gives \
  'if arc.rotation == 0.0 || rx == ry {' \
  'if true {' \
  icons
break_one icon-unknown-name-drops-glyph lower.rs a_name_the_app_has_no_icon \
  'drawn[slot.index] = icons(&slot.name).map(|icon| slot.drawn(&icon));' \
  'drawn[slot.index] = Some(icons(&slot.name).map_or(DrawnIcon { frame: None, coats: Vec::new() }, |icon| slot.drawn(&icon)));' \
  icons
break_one icon-stroke-unzoomed lower.rs an_icon_follows_the_zoom \
  'width: width * self.zoom,' \
  'width,' \
  icons
break_one icons-not-in-the-memo-key lib.rs another_icons_function \
  'memo(self.id.as_str(), (lowered.id, &paints, handler, drawer), || {' \
  'memo(self.id.as_str(), (lowered.id, &paints, handler, drawer.map(|_| 0)), || {' \
  icons
break_one icon-outside-its-link lower.rs a_link_and_a_tooltip_are_found \
  'let frame = self.frame.into_iter().chain(icons.iter().copied()).reduce(union)?;' \
  'let frame = self.frame.into_iter().reduce(union)?;' \
  icons
break_one icon-off-the-tooltip-s-spots lower.rs a_link_and_a_tooltip_are_found \
  'None if tooltip.is_some() => self.spots.iter().chain(&icons).copied().collect(),' \
  'None if tooltip.is_some() => self.spots.clone(),' \
  icons
break_one icon-smooth-handle-unmirrored icon.rs path_data_is_read \
  'Some((handle, _)) => [2.0 * here[0] - handle[0], 2.0 * here[1] - handle[1]],' \
  'Some((handle, _)) => handle,' \
  icons
break_one icon-subpath-after-close-unmoved icon.rs path_data_is_read \
  'if closed && upper != b'"'"'M'"'"' && upper != b'"'"'Z'"'"' {' \
  'if false && closed && upper != b'"'"'M'"'"' && upper != b'"'"'Z'"'"' {' \
  icons
break_one butt-joints-left-open stroke.rs a_thick_dashed_stroke_on_a_rounded_box \
  'if self.half * self.half * turn / 2.0 >= VISIBLE * self.pixel * self.pixel {' \
  'if false && self.half * self.half * turn / 2.0 >= VISIBLE * self.pixel * self.pixel {' \
  dash_corners
break_one butt-joint-patched-from-the-point stroke.rs a_dash_round_a_curve \
  'along(at, inner, reach)' \
  'along(at, inner, 0.0 * reach)' \
  dash_corners
break_one hairline-notch-filled stroke.rs a_dash_round_a_curve \
  'if self.half * self.half * turn / 2.0 >= VISIBLE * self.pixel * self.pixel {' \
  'if self.half * self.half * turn / 2.0 > 0.0 {' \
  dash_corners
break_one dash-starts-at-the-corner lower.rs the_dashes_along_each_side \
  'Segment::Move([x + rx, y]),
        Segment::Line([x + w - rx, y]),
        arc([x + w, y + ry]),
        Segment::Line([x + w, y + h - ry]),
        arc([x + w - rx, y + h]),
        Segment::Line([x + rx, y + h]),
        arc([x, y + h - ry]),
        Segment::Line([x, y + ry]),
        arc([x + rx, y]),' \
  'Segment::Move([x, y + ry]),
        arc([x + rx, y]),
        Segment::Line([x + w - rx, y]),
        arc([x + w, y + ry]),
        Segment::Line([x + w, y + h - ry]),
        arc([x + w - rx, y + h]),
        Segment::Line([x + rx, y + h]),
        arc([x, y + h - ry]),' \
  dash_corners
break_one corner-radii-clamped-together lower.rs a_box_with_a_radius_over_half_its_height \
  '[r.min(w / 2.0), r.min(h / 2.0)]' \
  '[r.min(w / 2.0).min(h / 2.0); 2]' \
  dash_corners
break_one elliptical-corners-boxed lower.rs a_box_with_a_radius_over_half_its_height \
  'if radius != ry {
            return None;
        }' \
  'if radius != ry && false {
            return None;
        }' \
  dash_corners

# break_doc <name> <file> <from> <to>: a doc comment rustdoc warns about, which `mise run check`
# then refuses.
break_doc() {
  name=$1 file=$src/$2
  chosen "$name" || return
  cp "$file" "$file.unbroken"
  FROM=$3 TO=$4 perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"
  if [ $? -ne 0 ]; then
    restore "$file"
    echo "break $name: NOT APPLIED, the line to break is gone from $file"
    failed=1
    return
  fi
  if RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps -p rune-pele > "$logs/$name.log" 2>&1; then
    echo "break $name: rustdoc still PASSES with it"
    failed=1
  else
    echo "break $name: rustdoc fails, as it should:"
    grep -E "^error" "$logs/$name.log" | head -2 | sed 's/^/    /'
  fi
  restore "$file"
}

break_doc doc-private-link lower.rs \
  'or wider so that at most 128 lie along
    /// the gradient.' \
  'or wider where that is more than
    /// [`MOST_BANDS`].'

cargo build -q --release -p rune-pele --tests 2>/dev/null
exit $failed
