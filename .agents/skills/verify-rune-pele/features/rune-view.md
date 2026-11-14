# Native Rune view

A Rune app calls `rune_pele::Diagram::view` with Mermaid source and gets the diagram as native Rune elements: the same Scene the SVG writer reads, laid out with Rune's own text measurement and drawn as keyed shapes and text in Pele's paint order. Pie is drawn this way today. A palette or dark-mode change repaints without parsing, laying out or measuring again. A type pele-core has not ported is drawn by Rune's own `rune-mermaid`; any other error comes back as an error.

## Sub-features

- `rune-snap` renders the `pie` example headless with Rune's `--snap`, in the system's light or dark theme.
- `rune-palette` switches the example's palette with its button, by a scripted tap or the `hues` scene.
- `rune-product` checks pie-pets through the public API at 1× and 2×: each slice's area against the Scene's angles and radius, ink in every label's box, a palette change with no layout, keys that survive an `idPrefix` and a value edit, a whole donut, an empty pie, and the `rune-mermaid` fallback.
- `rune-side-by-side` rasterises Pele's own SVG with resvg beside the native PNG and compares the slice areas.
- `rune-scene` checks the Scene's read model the Rune view is built on: paint order, keys, inherited style and palette resolution.

## How to get to it (user POV)

- An app calls `Diagram::new(id, FontCatalog::…)` once and `diagram.view(cx, source, ViewOptions::new(width))` in its view.
- `cargo run -p rune-pele --example pie` opens the example window. Its button reads `Palette: Pele` or `Palette: hues`.
- The same example takes Rune's tooling flags: `--snap <png>`, `--theme light|dark`, `--scene hues`, `--script <json>`, `--size WxH`, `--scale N`.
- A crate user reads the Scene through `pele_core::Scene::paint` and `Scene::resolve_paints`.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`. The doctor also built `target/debug/examples/pie`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.
- A GPU adapter is available. Headless runs use it without a window.

- **Snap, light and dark.** Run `$C $R rune-snap target/debug/examples/pie --snap out/verify/$R/rune-snap/pie-light.png --theme light`, then again with `--theme dark` into `pie-dark.png`. Each exits 0, and stderr ends in a line starting `rune: animating frames`. Open both PNGs (960×960 at the default 2×). Each shows the `Palette: Pele` button above the pie titled `Pets adopted by volunteers`. The slices are `79%` blue, `17%` orange and `3%` green, and the legend reads Dogs, Cats, Rats. The light one has a pale background with white seams between slices, and the dark one a near-black background with dark seams and light text.
- **Palette by scene.** Run `$C $R rune-palette target/debug/examples/pie --snap out/verify/$R/rune-palette/hues.png --theme light --scene hues`. Exit 0. The button reads `Palette: hues` and the slices are salmon, amber and olive. The layout is the same as `pie-light.png`, and only colours differ.
- **Palette by button.** Write `[{"t": 0.1, "type": "tap", "key": "palette"}]` to `out/verify/$R/rune-palette/tap.json`. Run `$C $R rune-palette target/debug/examples/pie --script out/verify/$R/rune-palette/tap.json --snap out/verify/$R/rune-palette/tapped.png --theme light`. Exit 0. Then `cmp out/verify/$R/rune-palette/tapped.png out/verify/$R/rune-palette/hues.png` prints nothing: the tap and the scene draw the same pixels.
- **Product test.** Run `$C $R rune-product env RUNE_PELE_OUT=$PWD/out/verify/$R/rune-product cargo test -q -p rune-pele --test pie_view -- --nocapture --test-threads=1`. Exit 0 and `6 passed`. Stdout has an area table with one row per scale and slice. Each row's `error` is within ±0.005, and the line `unexplained plot pixels: 0` follows each scale. One line per text run gives `<key>: <n> of <m> px inked`. The theme lines read `before palette change: ViewStats { layouts: 1, measurements: <n>, paints: 1 }`, then `after palette change:` with the same `layouts` and `measurements` and `paints: 2`. `out/verify/$R/rune-product/` holds `pie-pets@1x.png`, `pie-pets@2x.png`, `pie-pets.other-palette@2x.png`, `donut-whole@2x.png`, `pie-empty@2x.png` and the reports `areas-rune.txt` and `theme-change.txt`. Open the PNGs: the whole donut is one unbroken blue ring around a white hole, and the empty pie is a grey rim under its title.
- **Side by side.** The same run writes `pie-pets.side-by-side@1x.png` and `@2x.png`, with the native view on the left and Pele's SVG through resvg on the right, and `areas-side-by-side.txt`. That report's first line names the font faces resvg drew with. Its `rune-resvg` column is within ±0.01 for every slice. Open the 2× image: the slices, title and legend line up. Text is Inter on the left and Arial on the right. `17%` is dark on the left and white on the right, as the next gotcha explains.
- **Scene read model.** Run `$C $R rune-scene cargo test -q -p pele-core --test scene_walk`. Exit 0 and `9 passed`.

## Gotchas

- Text on a series colour contrasts with that colour in the Rune view, by Pele's own luminance rule. Pele's SVG does so only where the source sets the series colour, so its `17%` on orange is white. That difference is intended. It is not a parity failure; parity is the SVG writer's, under `parity.md`.
- resvg has no CSS variables and does not read the root's `font` shorthand. The side-by-side resolves each `var()` as a page with no CSS of its own would, then sets the root's `font-size` from that shorthand. A raw `resvg` of Pele's SVG draws black slices and a 12px title.
- The side-by-side needs Arial or a metric-compatible face: Arial on macOS or Windows, or Liberation Sans on Linux. `RUNE_PELE_RESVG_FONT=<face.ttf>` names another face, such as the Inter Rune draws with.
- Without `RUNE_PELE_OUT`, the product test writes to `out/verify/pie-rune/`, and the spike tests to `out/verify/rune-spike/`.
- The rune-pele view lays out with Inter widths, so its legend and width differ from Pele's Arial layout. The plot's centre and radius do not depend on text, so the slices line up.
- zsh passes an unquoted `${x:+--scene $x}` as one word, and Rune ignores an argument it does not know. Write `--scene hues` as two literal words.
- A tap at `t: 0.1` needs a frame laid out first. The script runner provides one, so the tap finds the `palette` key.
