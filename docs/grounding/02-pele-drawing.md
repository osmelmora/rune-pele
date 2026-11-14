# Grounding 2: Pele's drawing vocabulary, flowchart in depth

Read-only exploration of `vendor/pele` at 3a2f075 (Pele 1.13.0), 2026-11-14.

## Components

| Name | File | What it does |
|---|---|---|
| `num()` | `src/svg/builder.ts:33` | Every coordinate, in hundredths. `-0` is written as `0`. |
| `esc()` / `escText()` | `src/svg/builder.ts:17,24` | XML escaping. `escText` decodes entities first and is used for tooltips, `data-id` and accessible names. |
| `labelSvg()` | `src/svg/builder.ts:49` | Turns a measured Label into `<text>`. A single plain line takes the fast path. Otherwise it writes `<tspan>`s. Icons are nested `<svg class="pele-icon">` and math is `<svg class="pele-math">` or `<foreignObject>`, both emitted after `</text>`. |
| `spanStyle` / `sharedStyle` / `textStyle` | `src/svg/builder.ts:139-179` | Bold, italic, mono, underline, sub and sup as attributes. Shared emphasis is hoisted to `<text>`. |
| `svgDocument()` | `src/svg/root.ts:23` | Root `<svg>`: class, viewBox, w/h, a long inline style, role, aria, title/desc. |
| `ROOT_STYLE`, `rounded()`, `resolveStyle()`, `classNames()`, `seriesColor()` | `src/svg/theme.ts` | Private CSS vars `--_bg --_fg --_m --_l --_s --_a --_b --_c --_r --_fm --_w/_tw/_hw`. Mermaid `style`/`classDef` is split into shape, line and text style strings. |
| `ink()`, `inkFill()`, `paints()` | `src/svg/ink.ts:102-131` | Contrasting label colour from a style's fill. |
| `palette()` / `paletteStyle()` | `src/svg/palette.ts:104,136` | `--pele-series-N` overrides, kept in module-level state during render. |
| `SHAPES`, `drawShape`, `shapeSize`, `shapeInset`, `shapeSpan`, `insetRoute`, `endSide` | `src/svg/shapes.ts` | About 55 node shapes. Each has a sizer, a drawer (SVG strings centred on 0,0), an inset (where an edge meets the outline) and a span (port spread). |
| `routePath()` | `src/svg/edges.ts:53` | Turns route triples (x, y, axis) into path `d` using only M, L, C and Q. Returns the ends and directions for markers. |
| `marker()` | `src/svg/edges.ts:204` | Arrowheads as shapes, not `<marker>`: point (triangle path), circle, or cross (two-segment path). |
| `fillUnder` / `overFills` / `casing` / `casingsSvg` / `hollowOn` | `src/svg/edges.ts:408-495` | Contrast for edges over filled clusters. A "casing" is a wide page-colour stroke drawn under edges that cross a fill border. |
| `edgeLabelSvg()`, `loopPath()` | `src/svg/edges.ts:498,512` | Edge label on a background patch (`rect rx=3`), and self-loops. |
| `clusterTitleX`, `titleCrossed`, `struckTitle` | `src/svg/cluster.ts` | Moves a cluster title clear of entering edges. A title that is still crossed is redrawn on a `color-mix()` halo stroke. |
| `withTitle()` | `src/svg/title.ts:18` | Diagram title above content wrapped in `translate()`. |
| `CNode` / `CEdge` | `src/layout/compound.ts:10,28` | **The only intermediate geometry.** Nodes have centre x/y, w/h, parent, group pad and span. Edges have `route` triples and `labelX/Y`. There is no drawing object between layout and the SVG strings. |
| `LNode` / `LEdge`, `layered()` | `src/layout/layered.ts:37,53,242` | Sugiyama-style layered layout wrapped by the compound layout. `ease()` (`:1059`) is shared by layout and drawing. |
| `compoundLayout`, `shiftLayout`, `inheritCrossedDirections` | `src/layout/compound.ts:419,389,374` | Nested-group layout with a direction per group. |
| `laneLayout` | `src/layout/lanes.ts:25` | Swimlane only. |
| `wrapComponents` | `src/layout/components.ts:13` | Wraps disconnected components into rows. |
| `renderFlowchart` / `draw` / `FlowVariant` | `src/diagrams/flowchart/render.ts:36,60,66` | Flowchart end to end. Swimlane reuses it through `FlowVariant`. |
| `buildFlowGraph` | `src/diagrams/flowchart/graph.ts:92` | Turns the parsed db into drawn nodes and edges: groups, collapsed subgraphs, compiled classDefs, edge defaults. |
| `canonicalShape` | `src/diagrams/flowchart/shapes.ts:106` | Maps aliases to one shape name, which goes into the `pele-shape-*` class. |
| `fitWidth` / `turnToFit` / `tighten` | `src/diagrams/common/fit-width.ts` | Width adaptation by drawing several times and comparing. |
| `linked`, `safeUrl`, `imageUrl`, `internalLink`, `relAttr` | `src/util/url.ts:120,75,105,112,127` | Link and image URL policy, and the `<a>` wrapper. |

## Flow for flowchart

1. `renderFlowchart` (`render.ts:60`). If LR/RL, `turnToFit` draws across, then possibly again as TB when `maxWidth < 640` and `autoDirection !== false`. Each attempt goes through `tighten`, which may redraw with smaller spacing and `groupPad = 12`. That makes up to 4 full renders, and the narrowest wins. "Turned" means `rootDir = 'TB'`, a real relayout.
2. `buildFlowGraph` (`graph.ts:92`). Subgraphs are pushed in reverse declaration order (`graph.ts:182`), then vertices in map order. This order decides paint order. Edge ids default to `L_${start}_${end}_${index}`.
3. Node sizing (`render.ts:84-130`). Each node goes through `resolveStyle`, then `layoutLabel`, then `shapeSize` (or the image/icon box), and becomes a `cnode(w,h)`. Groups get pad, minW and dir. A self-loop grows its node's box.
4. `compoundLayout` (or `laneLayout`) mutates `CNode.x/y/w/h` and `CEdge.route/labelX/labelY`. Then `shiftLayout` adds padding and the title offset.
5. Clusters and nodes are written in node index order into three string buffers: `clusters`, `struckTitles` and `nodesOut`.
6. `sideReach` runs a pre-pass over edges for marker clearance.
7. Edges: `insetRoute`, then `routePath` (or `loopPath`). `overFills` picks the stroke. The output goes to `casings`, `edgesOut` and `labelsOut`.
8. `svgDocument` writes a fixed layer order:
   1. title `<text>`
   2. `g.pele-clusters`
   3. `g.pele-casings`
   4. `g.pele-edges[fill=none stroke=var(--_l) stroke-linecap=round]`
   5. `g.pele-cluster-titles`
   6. `g.pele-edge-labels[font-size]`
   7. `g.pele-nodes`

   `g.pele-nodes` is always present. The other layers are omitted when empty.

### Per-element shapes

- **Node:** `<g class="pele-node pele-shape-{shape}{classes}" data-id transform="translate(x,y)">`, then `[<a href target? rel>]`, `[<title>]`, the shape markup, and `<text class="pele-label" fill? style?>`. The `<title>` sits inside the `<a>`. An `internal-link` wraps only the text.
- **Cluster:** `<g class="pele-cluster…" data-id><rect … fill="var(--_a)" [fill-opacity="0.5"] stroke="var(--_b)" style="rx:var(--_r);…"/>`, then the label.
- **Edge:** `<g class="pele-edge" data-id [stroke=over] [style=line]><path d [stroke-width="2.5"|stroke-dasharray="3 4"]/>`, then the start marker and the end marker. User edge classes are never written. `animate` is parsed but not drawn.

## Survey across all 33 types (grep counts of literal markup)

Never emitted anywhere: `<line>`, `<polyline>`, `<marker>`, `<clipPath>`, `<mask>`, `<pattern>`, `<use>`, `<textPath>`, `<style>`.

| Feature | Types |
|---|---|
| `<path>` | almost all, plus shared shapes, edges and markers |
| `<rect>` | all except info and venn |
| `<circle>` | architecture, class, er, flowchart, git, journey, pie, quadrant, radar, railroad, requirement, sequence, state, timeline, usecase, venn, wardley, xychart |
| `<ellipse>` | usecase, plus the shapes.ts ellipse |
| `<polygon>` | block, radar, state, plus shapes.ts (diamond, hex, …) |
| Arc `A`/`a` | pie, venn, block, ishikawa, cynefin, sequence actors, railroad (relative `a`), plus shapes.ts (cyl, delay, h-cyl, bucket, cloud, curv-trap, bow-rect, lin-cyl). Always unrotated. |
| `Q`/`q`/`t` | routePath corners, plus flag/wave/brace shapes |
| `transform="rotate(…)"` | git (±45° commit labels), quadrant, xychart (`translate() rotate(-90)`), wardley (rotate in a text's attrs), swimlane (`rotate(-90 cx cy)`) |
| `translate()` | most types |
| `stroke-dasharray` | agentflow, block, c4, class, cynefin, er, flowchart, gantt, git, journey, railroad, requirement, sequence, state, usecase, wardley |
| `<linearGradient>` + `<defs>` | **sankey only**. The fill is `url(#id) fallback`, the id is a content hash, and gradients are written only when `idPrefix` matches `^[\w-]+$`. |
| `<foreignObject>` | **math only** (`builder.ts:125`) |
| Nested `<svg>` | icons (flowchart, architecture, er, treeview, usecase, labelSvg) and math. The inner markup comes from the host. |
| `<image>` | **flowchart only** |
| `<a>` | flowchart, agentflow, er, kanban, class, state, gantt, c4 (via `linked`) |
| `<title>` tooltip | flowchart, agentflow, class, state, git and c4 prepend it in the group. sankey, journey and treemap put it inside the shape element. |
| `<tspan>` | builder, er, eventmodeling, railroad, requirement, sankey, wardley |
| `opacity` / `fill-opacity` | many (cluster 0.5, sankey 0.4, venn, cynefin, er) |
| Inline `style=` | from `resolveStyle` and `rounded()` everywhere |
| CSS `color-mix()` | struck cluster titles only (`cluster.ts:61`) |
| `pointer-events` | `g.pele-casings` only |
| `data-*` | `data-id` (every type except info), `data-icon`, `data-href` (flowchart, agentflow, er) |

The types that use CNode/CEdge with compound or lane layout are flowchart, swimlane, agentflow, class, er, state, requirement and usecase. The types that use `routePath` or `marker()` without CNode are architecture, block, c4, cynefin, eventmodeling, ishikawa, sequence, timeline and wardley. `drawShape` is used by flowchart, agentflow, block, c4 and mindmap. Every other type builds strings straight from its own layout arithmetic.

## Boundaries

- **In:** the parsed db, `Config` and `RenderOptions`. The output-affecting options are fontSize, measurer, padding, maxWidth, autoDirection, directionBreakpoint, idPrefix, links, linkSchemes, images, imageSchemes, linkRel, icons, responsive, fontFamily and fontFamilyMono.
- **Measurement** (`text/label.ts`, `text/measurer.ts`) decides every size.
- **Out:** the `svg` string plus `links`.
  - `mount.ts` inserts it with `innerHTML` or a host `sanitize`. It redraws on width change, and compares SVG **strings** to decide whether a diagram adapts to width and whether to skip a redraw. It uses `idPrefix = pele-{copy}-{n}`.
  - `zoom.ts` expects the first child to be `<svg>` with numeric width/height. It rewrites `viewBox`, queries all `text` elements, and swallows a click at the end of a drag.
- **Rune has no DOM.** Links come from `LinkInfo` and `data-id`, and tooltips from `<title>`.

## Constraints on a generic Scene-to-SVG writer

1. **Style is handled as strings.**
   - `rounded()` inserts `rx:var(--_r);` at the start of an existing style or appends `style="rx:var(--_r)"`.
   - The datastore shape does `replace('stroke="var(--_b)"','stroke="none"')`.
   - `hollowOn` regex-replaces `fill="var(--_bg)"`.
   - `casing` strips `class="pele-marker"` and parses `stroke-width` out of the line style.
   - `textStyle` checks `attrs.includes(' font-weight="')`, and `mathLook` regex-parses label attrs.

   A Scene needs ordered attributes plus a raw style string, with these rewrites done as Scene operations.
2. **Attribute order is fixed per call site.**
   - Node `<g>`: class, data-id, transform.
   - Edge `<g>`: class, data-id, stroke?, style?.
   - Marker: class, d, fill, stroke. The cross marker has neither fill nor stroke.
   - Single-line text: `{attrs} x y text-anchor`.
   - Multi-line text: `{attrs} text-anchor xml:space`.

   Keep insertion order and never sort.
3. **Root style depends on content.** `svgDocument` scans the body for `var(--_r|tw|hw|w)`.
4. **Two text forms.** The single plain span form has no `xml:space`. A blank line is written as `<tspan> </tspan>`. Icons and math are siblings after `</text>`.
5. **Shapes are local.** They are centred on 0,0 inside `translate(x,y)`, while edges, clusters and labels are absolute. Zero values are omitted, so a circle is written with no cx/cy.
6. **Raw numbers.** Some values are written raw rather than through `num()`: `rx="${…?8:0}"`, `stroke-width="2.5"`, `r="3.5"`, the sequence actor's `A14,5`. Venn's `regionPath` takes a `format` function.
7. **Edge path rounding sidecar.** The path's `tokens` sidecar (`edges.ts:141`) replays `num()` rounding so the fill test samples exactly what was written. It uses the same De Casteljau order (`edges.ts:315`).
8. **No `<marker>`.** Arrowheads are siblings after the path inside the edge `<g>`, with an explicit `fill`. Class markers inherit their fill instead.
9. **Module state.** `withPalette` is global. `ink()` keeps a cache.
10. **Ids.** The sankey gradient id is `${idPrefix}-sankey-${contentId(stops)}`. The accessible ids are `${prefix}-title` and `${prefix}-desc`.
11. **Paint order beats nesting.** Struck cluster titles are a layer above the edges. Edge labels are a layer of their own, separate from their edges, so an edge is not one Scene subtree.
12. **Per-type tooltip position.** Some types put the tooltip `<title>` first in the group, others inside the shape element.
13. **Directions and turning.** Nested group directions affect layout only. Turn-to-fit is a full relayout.
14. **Escaping depends on the call site.** `escText` decodes first, `esc` does not.

## Open questions

- The bodies of `compoundLayout`, `layered.ts` and `lanes.ts` were not read, so it is unknown whether layout itself rounds anything.
- Per-type renderers beyond grep counts were not catalogued: order and quirks inside sequence, state, class, er and block, and how mindmap uses `drawShape`.
- `contentId` (sankey) was not read.
