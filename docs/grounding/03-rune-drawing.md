# Grounding 3: Rune's drawing surface and rune-mermaid

Read-only exploration of `vendor/rune` at 165f87a (branch dev), 2026-11-14. Paths are relative to `vendor/rune/`. Rune takes no PRs, so rune-pele can use only Rune's public API.

## Vector drawing (`crates/rune/src/shape.rs`)

- **`LineCap`** (`:15`) has two values. `Butt` draws a plain box per segment, with notched corners and no join. `Round` (the default) rounds caps and every joint. There is no `LineJoin`, no miter, and no miterlimit.
- **`Shape`** (`:49`) is an ordered list of private `Op::Stroke{points, closed, width, cap, color}` and `Op::Fill{points, color}`. It paints over the element's fill and under the element's text. Builders:
  - `stroke`, `stroke_with(points, closed, width, cap, color)`, `line`
  - `fill(points, color)`, documented as "simple polygon (either winding; not self-intersecting)"
  - `stroke_path`
  - `fill_path`, documented as "fills every subpath on its own (no holes between subpaths)"
  - `view_box(w,h)`, a non-uniform stretch in which stroke widths stay in points
  - `drawn(t)` and `trimmed(start,end)`, for animation
- **`Path`** (`:243`) offers `move_to`, `line_to`, `quad_to`, `cubic_to` and `close`.
  - It is flattened at build time with `TOLERANCE = 0.1` pt and 1–256 segments.
  - It has **no `arc_to`** and no SVG path-string parser, and its flattened points can't be read back.
- **`triangulate`** (`:339`, private) ear-clips each fill. A non-simple polygon "falls back to a fan" (`:378`) and paints the wrong area.

## Element-level effects (`crates/rune/src/el/visuals.rs`, `el.rs`)

- `canvas(shape) -> El` (`el.rs:328`), and `El::shape(shape)`.
- Text elements:
  - `text(content)` (`el.rs:92`) does not wrap by default.
  - `rich(content, Vec<Span>)` forces wrap.
  - `El::spans` adds styled ranges without forcing wrap.
- Gradients cover the element's own rect only: `gradient(top,bottom)`, `radial_gradient(center, radius, ≤16 stops)`, `linear_gradient(angle_rad, ≤16 stops)`.
- Dashes exist on **borders only**: `border_dash(dash,gap)` and `border_dash_phase`, for rect and rounded-rect borders.
- Opacity: `opacity`, and `group_opacity(rest, hovered)`.
- Transforms: `scale(k)`, `rotate(radians)` (clockwise about `origin`, in its own layer), `origin(x,y)`, `offset`, `absolute`.
- Clips: `clip_rounded`, `clip_corners`, `clip_fade`/`clip_soft`, `cutout_circle`. All are rect-based, with no path clip.
- `pixels` / `pixels_fit` for images.

## Text

- **`rune::text::measure`.** The signature is `rune::text::measure(text, spans, TextStyle, max_width: Option<f32>) -> (f32, f32)` (`text/measure.rs:198`). There is also `measure_with(…, RichLayoutOptions)`.
  - Widths round **up to 1/64 pt** (`runtime/commit/text_layout.rs:17`), even though the doc comment says whole points.
  - Height is unrounded.
- **Related helpers:** `advance` (unrounded single-line width), `content_widths`, and `font_epoch()` as a cache key.
- **`TextStyle`** is `{size, weight, family, line_height, tracking, variations, features, optical_size, italic}`, and it is `Hash`. `Family` is `{Sans, Mono, Named(&'static str), SystemMono}`.
- **`Span`** is `{start, end, weight, mono, size, color, bg, bg_pad, underline, italic}`. It has **no baseline-shift**, and adding a field would break apps that build it literally.
- Text also supports `strike`, `align`, `wrap`, `single_line` and `max_lines`.
- There is **no public ascent or baseline API**. Converting an SVG baseline `y` into Rune's top-left placement may need an empirical offset per `TextStyle`. This is unverified.

## Colour, theme and caching

- `Color` is **linear sRGB with straight alpha**. Helpers include `oklch`, `oklcha`, `hex(&str)`, `mix` (in linear light), `to_oklab` and `from_oklab`.
- `App::srgb_blend` defaults to on, so blending is in gamma space, as in browsers.
- `tok::theme(dark)` provides the core tokens, and `theme::Theme` provides JSON themes. `Cx::system_dark()` reports dark mode.
- `memo(id, version: impl Hash, || El)` combines the version with the window's environment: dark mode, scale, reduced motion, accent and blend mode. App theme state must go into `version`. Measured layouts must also be keyed on `font_epoch()`.

## Images

- `ImageCache::svg(bytes, size_pt, scale)` rasterises with `usvg::Options::default()`.
- That raster path has an **empty font database**, and usvg is built without `fontdb`, so SVG `<text>` renders nothing and `foreignObject` is never supported.
- An SVG-string fallback therefore works for shapes but **not for labels**. It would need its own resvg with text support, plus `ImageCache::rgba`.

## Headless tooling

- `Runtime::new(state, view, viewport, Env)` (`runtime.rs:891`), `Runtime::frame(dt)`, `Runtime::elements()`, `accessibility_json`.
- `Gpu::headless()`, `render::write_png(path,w,h,rgba)`, and `Renderer`/`Offscreen` readback (pattern in `crates/rune/tests/border_dash.rs:36-45`).
- CLI flags `--snap`, `--tree`, `--at`, `--size`, `--scale` and `--theme` (`tooling/args.rs:138-177`). Release builds need the `tooling` feature for these.

## Depending on Rune

- Use a git dependency on a `dev` rev.
- Patch `winit`, `parley_engine`, `wgpu-hal` and `accesskit_windows` from the same rev (`crates/rune/README.md:2433-2450`). A dependency's `[patch]` does not reach the dependent workspace.
- Rune is edition 2024 with no `rust-version`. Its `rust-toolchain.toml` pins 1.99.1, which applies only inside Rune's checkout. Building rune-pele's adapter on Rust 1.100 is unverified.

## rune-mermaid (`crates/rune-mermaid`, depends on `rune` with `default-features = false`)

### Public API and internal types

- **Public API:** `render`, `render_with_hover`, `render_streaming`, `render_streamed`, `render_drawing` (returning `Option<El>` or `Drawing{el, size, surface, surface_border}`), plus `supported`, `may_draw`, `DiagramStyle` (`Hash`, with `neutral(dark)` built from `tok::theme`) and `Hover`.
- **Internal scene types:**
  - `canvas::Op` mirrors Rune's Op.
  - `Label{text, at (top-left), size, font, color, wrap, align}`.
  - `Canvas` holds ops, labels, hovers and part marks.
  - `Scene` holds size, scale, keyed `Piece`s, labels and hovers.
- **`parts.rs`:** `Kind{Node, Edge, Frame}` gives keys `n:`, `e:` and `g:`. Edge names are `A>B`, then `A>B~1` for a repeat. A `Piece` is one `canvas()` per keyed part.
- **Other modules:**
  - `ink.rs::flatten` makes translucent strokes opaque over the fills beneath.
  - `steady.rs` keeps streaming layouts stable.
  - `scenes.rs` is a 16-entry thread-local cache keyed with `font_epoch`.
  - `hover.rs` provides `Spot` hit rectangles.
- **Geometry helpers in `canvas.rs`:** `dashed` (one on/off pair), `measure_balanced`, `rounded_rect`, `arc`, `ellipse`, `arrowhead`, `diamond_head`, `smooth`.

### Flow

1. `scenes::built` hashes the source, style, hover and `font_epoch`. On a cache miss it runs `scene()` inside `catch_unwind`.
2. `parse` runs, then a per-type `draw` paints into a `Canvas`. Text is measured with `rune::text::measure`, ops are in diagram points, and labels are top-left boxes.
3. `Canvas::finish` runs `ink::flatten`, computes bounds (held to the prior origin when streaming), splits the ops into keyed pieces with local `Shape` + `view_box`, and computes the fit scale.
4. `Scene::el` builds the card, then one `canvas(piece.shape).absolute().offset().size()` per piece in order, then **all labels** as `text().font().color().align().w(size+2).absolute().offset().origin(0,0).scale(scale)`, then hover groups. Labels therefore always paint above all shapes, a deliberate fidelity gap.
5. Elements are keyed, so they spring between rebuilds. `Transition::draw()` draws edges on.

### Limits it hit

- 400 items and nesting depth 8.
- 20,000 dash pieces per call.
- 1,200 edge hit rects.
- `catch_unwind` around layout.
- Streaming needed `steady.rs`.

## Pele SVG features against Rune

| SVG feature | Rune | How |
|---|---|---|
| Lines and M/L/Q/C/Z paths | **Direct** | `Path` + `stroke_path`/`fill_path`, or pre-flattened points |
| Arcs (`A`) | **Lower** | No `arc_to`. Convert to cubics in core, or to a polyline |
| `stroke-dasharray` | **Lower** | Split polylines into dashes with butt caps. Rect borders can use `border_dash` |
| `stroke-linejoin: miter` | **Cannot** | Round joins only. A closed rect can be a fill ring instead |
| Holes, evenodd, multiple contours | **Lower** | Tessellate in core into simple polygons (keyhole bridges, or ring as fill-minus-inner). Seams are a risk |
| Translucent stroke | **Lower** | Double-blends at joints. Flatten to opaque over the backdrop, or use group opacity |
| Group `opacity` | **Direct** | `El::opacity` |
| translate/scale transforms | **Lower** | Bake into coordinates |
| Rotated text | **Direct** | `text().rotate(rad).origin(..)` |
| text-anchor and baseline `y` | **Lower** | Rune places the top-left. Convert the anchor to x, and the baseline to top using ascent (unverified) |
| `baseline-shift` sub/sup | **Cannot directly** | Separate smaller `text()` elements offset in y, positioned with `advance()` |
| tspan runs (bold, colour, size) | **Direct** | `Span` weight/size/color/italic/underline, plus `strike` |
| `linearGradient` on a rect | **Direct** | `El::linear_gradient` |
| `linearGradient` on a path | **Lower or approximate** | Slice into solid bands, or use the average colour |
| `foreignObject` (HTML labels) | **Lower** | `text()` / `rich()` |
| CSS variables and classes | **Lower** | Resolve in core to concrete colours per theme. Rune has no CSS |
| `color-mix()` | **Lower** | Compute in CSS's colour space. `Color::mix` is linear-light |
| Arrowheads | **Lower** | Filled polygons |
| Non-rect `clipPath` | **Cannot** | Rect or rounded-rect clips only |

## Non-obvious things

- `rune-mermaid` adds 2 pt of label slack (`.w(size+2)`), so 1/64 rounding never re-wraps a label.
- Under `view_box` or `El::scale`, curves are scaled after flattening, so flattening tolerance degrades when magnified.
- Abutting `fill`s antialias their own edges, so seams can show.
- An element's text always paints above that element's shapes. Interleaving shapes and text needs sibling elements in paint order.
- `Color` is linear, so Pele's hex colours must be decoded from sRGB.
- `rune-mermaid` uses public `rune` APIs only. Its `pub(crate)` modules can be copied (MIT) but not called.
