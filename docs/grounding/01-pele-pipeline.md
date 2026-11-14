# Grounding 1: Pele's shared render pipeline, traced through pie

Read-only exploration of `vendor/pele` at 3a2f075 (Pele 1.13.0), 2026-11-14. The SVG excerpts are real output from running the source with `metricsMeasurer` and TZ=UTC.

## Components

| Name | File | What it does |
|---|---|---|
| `render` / `parse` / `parseSource` | `src/core.ts:162`, `:152`, `:83` | Entry point. Applies limits, preprocesses, detects, parses, renders inside the palette/math/icon contexts, checks the output limit and tags errors. |
| `Diagram<Model>` | `src/types.ts:76` | Plug-in contract: `type`, optional `section`, `keepComments`, `math`, `parse(source, config, title, lineOffset, limits)`, and `render(model, config, options) → Rendered`. |
| `RenderOptions`, `Rendered`, `RenderResult` | `src/types.ts:15`, `:57`, `:64` | Host options (measurer, fontSize, idPrefix, responsive, maxWidth, padding, limits, config, now). The result is `{svg, width, height, links}` plus `type` and `mermaidType`. |
| `preprocess` | `src/preprocess.ts:138` | Normalises line breaks, rewrites HTML attribute quotes, reads YAML front matter and `%%{init}%%` directives, merges config, strips comments. |
| `encodeEntities` / `decodeEntities` | `src/preprocess.ts:237`, `src/text/entities.ts:69` | Hides Mermaid `#name;` codes behind placeholders before parsing and turns them back at output. |
| `parseYaml` | `src/util/yaml.ts:452` | A js-yaml (JSON schema) subset for front matter. |
| `detectDiagram` | `src/detect.ts:85` | Mermaid's ordered regex detectors. The first match wins. |
| `parsePie`, `buildPie`, `PieModel` | `src/diagrams/pie/parser.ts:92`, `model.ts:13`, `:4` | Hand-written parser that matches Langium. Sections are a `Map<string, number>`, and the first occurrence of a label wins. |
| `renderPie` / `draw` / `slice` | `src/diagrams/pie/render.ts:42`, `:47`, `:26` | All pie geometry and SVG string building. |
| `fitWidth`, `titleRoom` | `src/diagrams/common/fit-width.ts:7`, `:27` | Redraw loop that shrinks the plot to fit `maxWidth`, and the width available to a title before it wraps. |
| `svgDocument` | `src/svg/root.ts:20` | Root `<svg>`, style attribute, and accessibility title/desc. |
| `ROOT_STYLE`, `fontStyle`, `seriesColor`, `resolveStyle` | `src/svg/theme.ts:6`, `:41`, `:148`, `:99` | Theme tokens as CSS custom properties with fallbacks. |
| `palette`, `withPalette`, `paletteStyle`, `seriesInkFill` | `src/svg/palette.ts:104`, `:122`, `:132`, `:135` | `themeVariables.pieN` becomes `--pele-series-N` declarations plus per-series label inks. |
| `ink`, `plainColor` | `src/svg/ink.ts:102`, `:53` | Contrast label colour for a fill (`ON_LIGHT` / `ON_DARK`). |
| `Label`, `Span`, `layoutLabel`, `wrap` | `src/text/label.ts:15`, `:7`, `:311`, `:236` | Label layout: lines of styled spans with measured widths. |
| `TextMeasurer`, `metricsMeasurer`, `defaultMeasurer`, `Style` | `src/text/measurer.ts:15`, `:53`, `:146`, `:3` | Width oracle. `metricsMeasurer` reads an Arial advance table (`metrics.ts`). `Style` is a bit flag set. |
| `esc`, `escText`, `num`, `labelSvg`, `spanStyle`, `sharedStyle`, `textStyle` | `src/svg/builder.ts:16`, `:24`, `:33`, `:51`, `:146`, `:158`, `:172` | Serialization primitives. |
| `withTitle` / `titleLabel` | `src/svg/title.ts:18`, `:7` | Shared diagram-title placement. Pie places its own title instead. |

## Flow for pie (`render(src, {measurer: metricsMeasurer})`)

1. `render` (`core.ts:162`) wraps everything in `guarded`, which turns a `RangeError` into `PeleError('limit')`. It then calls `parsedFrom` and `parseSource`.
2. `parseSource` (`core.ts:83`):
   - `text.length > limit` (default 50000 UTF-16 units) throws `limit`.
   - Preprocess (`preprocess.ts:138`):
     - `\r\n?` becomes `\n`.
     - `<tag a="x">` becomes `a='x'`, up to the last `>`.
     - `matchFrontMatter` (`:53`) runs, then `parseYaml`. A truthy `title` becomes `String(title)`. `config` is deep-merged by `merge` (`:102`), which skips `__proto__`, `constructor` and `prototype`. `displayMode` goes to `gantt`.
     - `lineOffset` counts the front-matter newlines.
     - `directives` (`:114`) applies `text.trim().replace(/'/g,'"')` and runs `RE_DIRECTIVE`. `wrap` sets `config.wrap = true`. `init`/`initialize` bodies go through `JSON.parse` and are deep-merged, and invalid JSON is silently ignored. All directives are removed.
     - `cleanupComments` drops `%%` lines (but not `%%{`), then `trimStart`.
   - Detect runs `detectDiagram`, which checks `/^\s*pie/` after `info`.
   - Config is the shallow spread `{...extra, ...pre.config}`, so a source's `pie` section replaces the host's whole `pie` section (`core.ts:101`).
   - `encodeEntities(pre.text) + '\n'`.
   - `diagram.parse` runs inside `tagErrors` and `locatingIn`.
3. `pie.parse` calls `buildPie(parsePie(source), pre.title)`.
   - Values are `Number(token)`. A token is digits plus an optional fraction, with no exponent and no leading zeros.
   - `titleValue`/`accTitleValue` collapse runs of 2 or more spaces or tabs.
   - A negative value throws `semantic`. For duplicate labels the first value and first position are kept. `title: ast.title || title`.
4. Back in `render`:
   - It reads `config[diagram.section ?? type]`. If `useMaxWidth === false`, `responsive` defaults to false.
   - The render runs inside `withPalette(palette('pie', config))`, `withMath`, and `withIcons`.
   - `svg.length > outputLimit` (4,000,000) throws `limit`.
   - It returns `{type, mermaidType, svg, width, height, links}`.
5. `palette` (`palette.ts:104`) reads `themeVariables.pie1…pie8`. Each colour is checked by `safeColor`: allowed characters, balanced parentheses, allow-listed function names, and `var(--pele-*)` only. Each accepted colour emits `;--pele-series-N:color` and `inks[i] = ink(';fill:'+color)`. The style string is `esc`'d.
6. `renderPie` calls `fitWidth(options, 240, 140, draw)`. That is `draw(240,false)`. If `maxWidth` is exceeded, it tries `draw(240,true)`, which moves a side legend to the bottom. Then it shrinks the plot up to 3 more times.
7. `draw` (`render.ts:47`):
   - `size = fontSize ?? 16`, `small = Math.round(size*.875)` = 14, and `pad = padding ?? 8`.
   - Options from `config.pie`:
     - `textPosition` must be a number in [0,1], else 0.75.
     - `donutHole` must be in (0,0.9], and is multiplied by the radius.
     - `legendPosition` is `String(v ?? 'right')`.
   - Legend labels are `layoutLabel(showData ? `${label} [${value}]` : label, false, measurer, small, 4000)`. `legendWidth = max + 20` and `legendHeight = n*26`.
   - The title is `layoutLabel(title, …, size, titleRoom, Style.Bold)`, and `titleHeight = h + 16`.
   - A slice is drawn only if `share*100 >= 1 && (share*100).toFixed(0) !== '0'`. A slice is narrow when `2*r*textPosition*sin(share*π) < measure(pct+'%') + 6`. If any slice is narrow, the margin is 28.
   - Inside labels sit at `radius*textPosition`. Outside labels sit at `rim = radius + 14` and are nudged by `2/rim` radians, up to 360 steps, until they clear the labels already placed. If nothing is drawn, the output is `<circle r="${radius}" fill="none" stroke="var(--_b)">`.
   - Each legend item is a `<g>` with a `<rect>` and `labelSvg`.
   - Totals are `Math.ceil(width+2*pad)` and `Math.ceil(height+titleHeight+2*pad)`, followed by `svgDocument(...)`.
8. `svgDocument` (`root.ts:20`):
   - `prefix = esc(idPrefix ?? 'pele')`.
   - It writes the accessibility `<title id="P-title">` and `<desc>`.
   - It scans the finished content for `var(--_(r|tw|hw|w))` to decide which `--_r/--_tw/--_hw/--_w` declarations to emit.
   - The style is the concatenation fit + ROOT_STYLE + weights + `esc(fontStyle)` + `paletteStyle()`, in that order.

### Real output, corpus sample 1 (`pie title Pets adopted by volunteers / Dogs 386 / Cats 85 / Rats 15`), 397×352

```
<svg xmlns=… class="pele pele-pie" viewBox="0 0 397 352" width="397" height="352"
 style="max-width:100%;height:auto;--_bg:var(--pele-bg,#fff);…;--_c:var(--pele-accent,#5b7bd5);color:var(--_fg);--_tw:var(--pele-title-weight,bold);--_fm:var(--pele-font-mono,monospace);font:16px sans-serif;letter-spacing:normal;word-spacing:normal;text-transform:none;direction:ltr;font-family:var(--pele-font,sans-serif)"
 fill="var(--_fg)" role="graphics-document document" aria-roledescription="pie">
<text class="pele-title" font-weight="var(--_tw)" text-anchor="middle" xml:space="preserve"><tspan x="198.5" y="25.6">Pets adopted by volunteers</tspan></text>
<g class="pele-slices" transform="translate(156,196)" stroke="var(--_bg)" stroke-width="1" stroke-linejoin="round">
  <path class="pele-slice" data-id="Dogs" d="M0,0L0,-120A120,120 0 1 1 -115.39,-32.93Z" fill="var(--pele-series-1,#4c78a8)"/> …
</g>
<g class="pele-slice-labels" transform="translate(156,196)" font-size="14" text-anchor="middle" fill="var(--_bg)"><text x="54.21" y="76.74">79%</text>…</g>
<g class="pele-slice-labels" transform="translate(156,196)" font-size="14" text-anchor="middle"><text x="-12.97" y="-128.47">3%</text></g>
<g class="pele-legend" font-size="14"><g class="pele-legend-item" data-id="Dogs"><rect x="336" y="164" width="12" height="12" rx="2" fill="var(--pele-series-1,#4c78a8)"/><text x="372.34" y="174.9" text-anchor="middle">Dogs</text></g>…</g>
</svg>
```

The 3% slice is narrow, so the margin is 28. The title takes the tspan path because its span style is Bold, not 0 (`builder.ts:60`). Sample 2 (`showData`, `donutHole 0.2`) gives donut paths `M…A…L…A…Z` and legend text like `Calcium [42.96]`.

## Boundaries

- **In:** source text or a parsed model, `RenderOptions`, and the host `TextMeasurer`. The measurer drives legend widths, title wrapping and narrow-slice detection.
- **Out:** `RenderResult`. `links` is always `[]` for pie.
- **Theme lives in CSS, not geometry.** The only theme input from config is `themeVariables.pieN`, which becomes `--pele-series-N` in the root style plus ink fills on inside labels. Mermaid's `theme:` is never read. Everything else is a constant `var(--pele-*, fallback)`. Pie output contains no `<style>` element and no `color-mix`.
- **Global mutable context:** `withPalette`, `withMath`, `withIcons`, and the `marks` in `label.ts` are module globals. A Rust port passes them explicitly.

## Non-obvious things

### Root style and labels

- Weight variables come from scanning the output. `svgDocument` regex-scans the finished content (`root.ts:41`), so the root style depends on what was emitted. A Scene needs "uses bold, title weight, or radius" flags, or the writer must collect them.
- Style attribute order is fixed: fit, ROOT_STYLE, weights, font, palette. The palette comes last.
- Ink fills follow palette overrides only. Inside labels inherit `fill="var(--_bg)"` from their group. Only series with a palette override get `fill="var(--pele-fg-on-dark|light,…)"`.
- The series colour follows the section's position, not the drawn slice count. A skipped slice under 1% still uses up its series number.
- `labelSvg` fast path. A single style-0 span with no icon and no math becomes one `<text x y text-anchor="middle">`. Anything else becomes `<text … text-anchor="middle" xml:space="preserve">` with `<tspan>`s. Only the first span of a line gets x/y. A shared style is hoisted unless `attrs` already contains a weight.
- Legend labels honour HTML tags but not markdown. `data-id` is `escText` of the raw label.
- An odd `legendPosition` (for example `5`) draws the legend outside the viewBox. Parity must reproduce that.
- Wrapped title lines use `round(size*1.3)`.

### JS number semantics

- `num()` uses JS `Math.round` (ties toward +∞), and the ≥1e11 fallback is `String(n/100)`. This is handled by `pele_core::js`.
- `toFixed(0)` on a tie picks the larger n: `(2.5).toFixed(0)` is `"3"`, while Rust's `{:.0}` gives `"2"`. Ties happen (1:7 gives 12.5%). This is handled by `pele_core::js::to_fixed`.
- `${value}` uses `Number.prototype.toString`, so 1e21 prints as `1e+21`. This is handled by `pele_core::js::to_string`.
- Some values are written raw without `num()`: `${radius}` (can be `.5`), `${small}`, `${width}`, `${SWATCH}`.
- `String(option)`: `true` becomes `"true"`, an array becomes `"a,b"`, and `null` becomes `'right'` via `??`.
- Accumulation order matters. `sum += value` runs in Map insertion order, and the angle is chained across slices.
- Trig: `Math.sin`/`cos` in V8 come from an fdlibm port. Use an fdlibm-equivalent (the `libm` crate) in Rust, and generate goldens with Node.

### Strings and iteration

- Strings are UTF-16. `metricsMeasurer` iterates `charCodeAt`, so an astral character is a wide high surrogate (1000) plus a zero-width low surrogate. The `limit`/`outputLimit` checks use `.length`.
- Regex classes differ. JS `\w` is ASCII-only, and Rust `\w` is Unicode unless `(?-u:\w)`. JS `\s` and `trim` include U+FEFF.
- `esc` drops C0 controls, DEL, U+FFFE/U+FFFF and lone surrogates. It escapes `& < > "` and does not escape `'`.
- Entities: `B #35; c` is written as `B # c`.
- Maps: sections use `Map` (use IndexMap in Rust). JS objects list integer-like keys first, in ascending order.
- Directives: every `'` in the trimmed text becomes `"` before `JSON.parse`.

## Open questions

- The tokenizer internals (`common/tokens.ts`) were not traced.
- YAML block, flow and folding rules were not traced fully.
- Error positions were not traced.
- `wrap()` with soft hyphens, CJK and math heights was read but not exercised.
