# Divergences from Pele in product mode

Parity mode (`pele_core::render`, `pele parity`) is byte-identical to Pele 1.13.0 and has no divergences.

Product mode (`rune_pele::Diagram::view`) draws the same Scene natively in Rune. Each deliberate difference from Pele's look is listed here with its reason. A difference that is not listed is a bug.

The 32 types `rune_pele::lower::NATIVE` lists are drawn this way: pie, the eight graph types (flowchart, class, state, er, requirement, usecase, swimlane and agentflow), and sequence, gantt, block, c4, railroad, info, architecture, cynefin, wardley, xychart, venn, quadrant, git, packet, eventmodeling, treemap, radar, mindmap, timeline, journey, kanban, ishikawa and treeview. Every other type, which is sankey for now, is drawn by Rune's own `rune-mermaid`, and so is a source of these types that the last table lists.

## What is drawn differently

| Area | Pele | rune-pele product mode | Why |
| --- | --- | --- | --- |
| Label ink on series colours | Contrasting ink only for series a source overrides. Otherwise labels inherit the background colour, so a 17% label on orange is white. | Ink contrasts with the resolved series colour for every series. | Readability on any palette, including app themes Pele never sees. |
| Fonts | Arial metrics in parity mode, the browser's font in a page. | The app's Rune fonts, measured by Rune at device scale. Monospace text is in the mono family the app gives its `FontCatalog`. | Labels fit the font that is actually drawn. |
| Bold, heading and title weights | `font-weight: bold` (700), and theme weights for a heading inside a node (600) and for a title. | Weight 700 requested for all three. Rune's embedded Inter tops out at SemiBold. | Rune's font set, and the Scene's walk reports one weight. Unverified on other fonts. |
| Sub/superscript shift | `baseline-shift` sub/super. | Smaller runs shifted by 0.3 and −0.45 of the smaller size. | Rune has no baseline-shift. The values are guesses until a type with sub/sup is checked side by side. |
| Series colours a source sets in its theme config | The colours of `themeVariables` such as `git0`, `cScale0`, `pie1` or `xyChart.plotColorPalette` are declared on the SVG's root, so they win over any `--pele-series-*` the page sets. | With `Palette::pele()`, the source's colours, as in Pele. With any other palette, including the window's Rune theme when none is given, the palette's own series, and the source's are not read. Colours a source writes into the diagram itself, such as `style` and `classDef`, win in every palette. | An app's palette is the app's look, so its diagrams agree with one another. `Palette::pele()` is the look the side-by-side holds to Pele's SVG. |
| Default colours | Its own greys, as the fallback of each `--pele-*` variable. | With no palette given, the window's Rune theme: nodes are filled as cards, groups a step from that towards the text, edges and text in the text colour, borders and muted text in the secondary text colour. `Palette::pele()` gives Pele's own. | A diagram in an app looks like the app, in light and dark. |
| Outline behind a title an edge runs through | The title's glyphs stroked 5 wide with round joins, in the colour behind them. | Twelve copies of the title in that colour, set round a circle 2.5 across, under the title. | Rune strokes no glyph outlines. The outline is within a tenth of a point of the stroked one. |
| Hairlines at one device pixel a point | A browser's coverage of a 1 point stroke, by the area of each pixel it covers. resvg, the reference the tests compare with, samples coverage in quarter pixels, and at 1× draws a stroke one device pixel wide or thinner as a hairline with no joins, so a mitred corner has no tip. | Rune's coverage, by each pixel's distance from the stroke's centre line. A slanted or curved hairline is up to a tenth of full coverage darker along its middle. Upright and level ones, and everything at 2×, are the same. | Rune's renderer. Measured over the corpus: `side-by-side.txt` lists the sources where it shows. |
| Curved strokes | One stroke along the curve. | A segment for each stretch of the curve that stays within a quarter of a device pixel of it, each antialiased on its own. Where two meet, a pixel the edge half covers is up to a quarter of full coverage darker. | Rune's `Shape` strokes polylines. Rectangles and circles are Rune boxes instead, which have no joints. |
| Subpaths of one stroke that meet | One stroke over all of a path's subpaths, so where two meet or run along each other, as a railroad branch leaves its track, each pixel is covered once. | Each subpath is stroked and antialiased on its own. At 1× a pixel two of them half cover is darker, by up to a third of full coverage over the few pixels where a branch leaves its track. At 2× the corpus comparison finds no pixel it cannot explain. | Rune paints each stroke on its own and has no union of shapes. Measured on railroad's corpus: `hairlines.txt` lists the pixels. |
| Icons | The page's host draws the named icon in its box. Where no host does, the box shows Pele's built-in glyph for the name, or else stays empty. Architecture knows `database`, `server`, `disk`, `internet` and `cloud`. Treeview puts its folder or its file in every icon box, whatever icon the source names. | The built-in glyph is drawn as Pele's SVG draws it. A box with none is left empty. | No host fills a box yet. An app cannot give `ViewOptions` its icons so far. |
| Where a link is pressed | Anywhere on the shapes and text inside the link. | Anywhere in the box around them. | Rune hit-tests an element's box. |
| Tooltips | The browser's, from the SVG's `<title>`. | Rune's `help` tooltip in the system's look, after the system's hover delay. | Rune's own tooltip. |
| Following a link | The browser follows `href` into `target`. | The view opens nothing. The app's handler is told the link, its target and whether it names a note of the host. | The app decides where a link may lead. |

## What is drawn by rune-mermaid instead

`Diagram::status()` is then `RenderStatus::RuneMermaid`, and `Diagram::fallback()` says why. Where rune-mermaid cannot draw the source either, the view is an error and draws nothing.

| Source | Why it is not drawn natively |
| --- | --- |
| A diagram type pele-core has not ported | There is no Scene to draw. |
| A style declaration the Scene's walk does not read: a colour only a page can resolve (`var()`, `color-mix()`, `currentcolor`), a font family other than `monospace`, `text-decoration` other than `underline`, a letter or word spacing, a dash offset, a line cap or join other than butt, round and miter, a `ry` unlike `rx`, or a font size not in pixels | Pele writes the declaration into the SVG for the page to apply. Read without a page, the drawing would differ from Pele's without saying so. |
| A node with a picture | The picture is the host's to load, and the view loads nothing. |
| A fill whose contours cross each other | What it fills depends on where they cross, which the lowering does not work out. A contour that crosses only itself, as an hourglass does, is drawn. No corpus or harvested source has such a fill. |
