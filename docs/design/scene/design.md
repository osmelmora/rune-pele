# A Scene with one drawing and two interpreters

## Usage (caller's view)

Create one viewer per diagram instance. Share the font configuration with the Rune app at construction; ordinary view calls supply source, width, typography and colours. `growing` accepts a snapshot, retains the last successful drawing while its next statement is incomplete, and settles when switched to `complete`.

```rust
use rune::{Cx, El, text};
use rune_pele::{Diagram, Event, FontCatalog, Palette, Role, Source, ViewOptions};

// During app construction, use these same bytes/families in App's font setup.
let preview = Diagram::new("mermaid-preview",
    FontCatalog::new(APP_FONTS, "Inter", "IBM Plex Mono"));

fn view(app: &AppState, cx: &mut Cx<AppState>) -> El {
    let colours = Palette::from_tokens(&rune::tok::theme(cx.system_dark()))
        .with(Role::Accent, app.theme.accent);
    let source = if app.receiving { Source::growing(&app.mermaid) }
                 else { Source::complete(&app.mermaid) };
    app.preview.view(cx, source,
        ViewOptions::new(app.diagram_width)
            .palette(colours).typography(app.diagram_font.clone()),
        |app, event, tx| match event {
            Event::Activate(link) => { let _ = tx.open_url(link.href()); }
            Event::Select(part) => app.selected = Some(part),
        })
        .unwrap_or_else(|error| text(error.to_string()))
}
```

The viewer includes pan, zoom controls, tooltips, keyboard navigation and related-part hover. Changing only `colours` reuses the measured Scene; source edits keep semantic element keys. Its `RenderStatus` accessor distinguishes a native Pele port, Rune's temporary fallback, and a retained streaming frame.

```rust
// pele-cli's existing corpus/oracle loop; exact comparisons remain unchanged.
let ours = match pele_core::parity_scene(source) {
    Ok(scene) => pele_core::svg::write(&scene)?,
    Err(pele_core::RenderError::NotPorted) => { tally.not_ported += 1; continue; }
    Err(error) => return Err(error.into()),
};
let identical = ours.svg == oracle_svg
    && ours.width == oracle_width && ours.height == oracle_height;
// Existing pele_core::render(source) remains this same parity_scene + svg::write path.
```

```rust
// Inside pele-core's diagrams/pie.rs. No SVG strings or Rune types.
fn draw(model: &PieModel, cx: &DrawContext<'_>, attempt: PieAttempt)
    -> Result<Scene, RenderError>
{
    let layout = layout(model, cx, attempt)?;
    let mut out = SceneBuilder::new(cx, layout.size, model.accessible());
    out.title(&layout.title);
    out.layer(LayerSpec::class("pele-slices")
        .translate(layout.centre).stroke(Paint::role(Role::Background))
        .stroke_width(Number::raw(1.0)).join(LineJoin::Round), |out| {
        for slice in &layout.slices {
            let part = out.part(Entity::pie_section(&slice.label));
            out.element(part, Facet::Shape)
                .class("pele-slice").data_id()
                .path(paths::pie_slice(slice.start, slice.end,
                                      layout.radius, layout.inner_radius))
                .fill(Paint::series(slice.series)).finish();
        }
    });
    draw_labels_and_legend(&mut out, model, &layout, cx)?;
    out.finish()
}
```

## Problem

One native Rust implementation must preserve Pele 1.13.0's actual strings and expose a useful interactive drawing to Rune. The existing `render(&str) -> Result<Rendered, RenderError>`, `js` number rules and headless CLI remain usable. SVG normalization loses parity; grouping everything by semantic entity loses Pele's layer order; using Rune's shapes indiscriminately loses holes, joins and alpha. These are representation decisions, not backend polish.

## Shape

**Scene owns paint order; parts own identity.** An immutable arena contains ordered groups and drawing nodes: path, rect, circle, ellipse, polygon, baseline text, image and resolved external visual. A root's ordered child vectors define layers, including deliberately empty layers. Semantic parts are a separate indexed table; one edge can own fragments in the casing, edge, arrowhead and edge-label layers. `PartKey = entity + facet`, independent of paint position, geometry, source revision and theme. Explicit Mermaid ids win; otherwise keys use diagram scope, semantic identity and duplicate rank among equivalent entities. Pie section keys use the label; flowchart edges use explicit edge ids or endpoints/kind/duplicate rank, independently of Pele's serialized `L_*` ids. Ambiguous duplicates have a stated stability limit.

`finish` builds the paint walk, part-to-fragment lists, bounds and graph adjacency together. Hit testing visits indexed candidates in reverse paint order and checks real geometry; related hover uses adjacency. Accessibility reads the part table in semantic order, without reshuffling paint. These are present structures, not proposed future caches, per foundational data-structure discipline.

**Fidelity is private information attached to the drawing, not a second drawing.** Each typed field has one value. A private ordered recipe references those fields to emit attributes, omitted defaults, transforms and text children; it never copies a path, colour or label into an independent SVG tree. Numeric fields retain `num`, raw JS string or `to_fixed` intent; a typed path program retains command case, separators and numeric intent. Its effective numeric projection also drives route fill sampling, removing the rounding sidecar. Every numeric spelling, including constants and root dimensions, uses `pele_core::js`. External Scene readers see domain draw visits and metadata, never XML attributes or recipes, per boundary-discipline and encode-lessons-in-structure.

The crate-private builder records attribute slots in call order. Shared shape/label/marker helpers own spelling and the historical rewrites: prepend rounded `rx` declarations, replace datastore stroke, substitute hollow-marker fills, remove casing marker classes, derive casing width and hoist shared text emphasis. Style declarations retain their validated lexical spelling and order; typed interpretation is derived from them. Rewrites consume a fragment and return a distinct rewritten type, preventing a second accidental application. Ports select and order typed operations; they do not implement escaping or string surgery.

`pele-svg` is **`pele_core::svg`**, a module rather than another crate. This keeps the current entry point and avoids a core↔writer dependency cycle. It serializes the body first, scans those exact bytes for `var(--_(r|tw|hw|w))`, then emits the root in Pele's fit/base/weights/font/palette order. No flags can drift from emitted content. Single plain labels and rich labels remain distinct forms, including blank-line tspans and icon/math siblings. `translate` stays a group transform; local shape defaults remain absent. A tooltip node references one metadata value but can appear first in a linked group or inside a shape. Links retain their exact wrapper scope. Separate edge-label layers are ordinary ordered children. This directly follows [root.ts](/Users/osmel/Developer/rune-pele/vendor/pele/src/svg/root.ts:20), [builder.ts](/Users/osmel/Developer/rune-pele/vendor/pele/src/svg/builder.ts:51) and the read grounding.

**Colours remain expressions.** Paint refers to background/foreground/line/surface/border/accent roles, series slots, literals, contrast, gradients and CSS colour expressions. Source palette overrides retain their accepted spelling for SVG and override the app's corresponding series tokens; app tokens otherwise override Pele defaults. Pele's private aliases and public fallback strings belong to the writer, not the public API. Native contrast expressions can improve label readability while their parity spelling preserves Pele's conditional ink attributes. CSS interpolation uses its declared colour space and premultiplied alpha; conversion to Rune's linear sRGB happens afterwards. Geometry-affecting font, weight, radius and spacing are typography/layout inputs, excluded from the colour-only palette.

**Each diagram owns parse → model → layout → draw in one module.** One declarative catalog owns the 33 kinds, ordered detectors/aliases, config section, comment/math policy, palette mapping and optional port module. A macro derives kind enums, typed prepared-model dispatch and support reporting; fallback asks `rune_mermaid::supported`, never a copied list of its 17 types. Preprocessing/config coercion, UTF-16 counts, limits, source maps and URL validation live at the input boundary. The public `TextMeasurer` supplies width/line metrics with an identity; parity fixes Arial tables and `2026-01-15T12:00:00Z`, while product injects Rune measurement. No process-global palette, layout or icon context survives the port.

Width adaptation redraws from the same immutable model and previous-layout snapshot. Per-type policies reproduce candidate order and winner rules, rather than scaling an already drawn Scene. Source inspection corrects the brief's shared “four” assumption: flowchart has up to four draws, but pie can have **five** (natural, narrow, three shrink draws), per [fit-width.ts](/Users/osmel/Developer/rune-pele/vendor/pele/src/diagrams/common/fit-width.ts:7). Attempts have private builders and scratch state; only the selected result is published. Product anchor constraints apply to each candidate, then a completed stream settles to the unanchored layout. Repeated identical snapshots are no-ops, per make-operations-idempotent and separate-before-serializing-shared-state.

**`rune-pele` owns all Rune adaptation.** It alone depends on Rune and rune-mermaid. Its viewer owns a bounded per-instance session; the core is pure. Layout keys contain source/config, geometry options, typography, measurement identity, font epoch and prior-anchor identity. Paint `memo` keys additionally contain the actual colour palette, Scene identity, interaction settings and density. Zoom changes tessellation/raster density, not layout; colour changes rebuild materials, not measurements. Each paint fragment becomes a keyed sibling in Scene order; grouping all text last is forbidden. Shared semantic hover/animation connects fragments across layers.

Public Rune shapes handle simple solid contours. Arcs become adaptive cubics; dashes are cut by path length; strokes become outlines when caps/joins/alpha need it. Compound fills preserve nonzero/evenodd winding. **When Rune cannot express coverage correctly, lower that paint atom to a transparent CPU coverage tile and mount it with `ImageCache::rgba`/`pixels`.** This covers miter limits, holes, intersecting outlines, translucent strokes and path gradients without fake hole backgrounds, guessed backdrop colours or gradient averages. Groups with compositing effects can similarly become one shape-only tile; text remains native except inside such an inseparable compositing island. Tiles are keyed and rebuilt at device-scale×zoom, budgeted and padded at boundaries. This is a local native image element, not an SVG-string or whole-diagram fallback. The need is explicit in [shape.rs](/Users/osmel/Developer/rune-pele/vendor/rune/crates/rune/src/shape.rs:1).

Native text uses ordered, already wrapped baseline runs; anchor conversion is `left = x − anchor_fraction × advance`, `top = baseline − actual_run_baseline`. Rune publicly exposes `TextSystem::layout_rich` and its returned line metrics, beyond the grounding's simpler free-function API ([rich_layout.rs](/Users/osmel/Developer/rune-pele/vendor/rune/crates/rune/src/text/rich_layout.rs:62)). One `FontCatalog`, shared with the app, supplies the same fonts/families; initialization is inside `with_families` so its `note_fonts` side effect cannot mutate another viewer's environment. Width uses Rune's public measurer; baseline shaping uses the same catalog, style, text and scale. Sub/sup use separately measured smaller runs with explicit offsets. The adapter never asks Rune to rewrap a core-broken line.

Only `NotPorted` invokes Rune's public streaming/still renderer. Syntax, semantic, resource and limit errors do not silently select another grammar. Fallback status exposes its weaker fidelity; parity always reports `NotPorted`. Strict completed sources report errors; an incomplete growing source can retain the last valid Scene with a diagnostic. Core limits reproduce Pele's 50,000 UTF-16 input units, 4,000,000 output units and 5,000 edges; independent native geometry/tile/hit budgets produce typed resource errors.

**Module map and interface depth.** `pele-core/lib.rs` owns input, catalog and orchestration; `measure.rs` owns labels/metrics; `scene.rs` owns drawing, private fidelity and theme expressions; `svg.rs` owns bytes; `diagrams/{pie,flowchart}.rs` own their complete pipelines; `graph.rs` owns shared graph geometry. `rune-pele/lib.rs` owns viewer state/interactions, `text.rs` owns measurement/baselines, and `lower.rs` owns material/geometry lowering. Existing oracle and CLI remain thin. The app learns one viewer operation, the CLI one parity operation, and a porter one drawing vocabulary; XML compatibility, fallback dispatch, cache invalidation and geometry coverage stay behind those interfaces. No public plug-in framework or separate parse/layout/render crates are introduced, per laziness-protocol and minimize-reader-load.

## Synthesis decision

**Base: candidate B, "one drawing and two interpreters"** (this document and `sketch.rs`). Candidate A, "Pele's SVG, typed", is kept in `candidates/` for reference. The cross-judge (GPT-6 Astra, read-only) recommended B. The orchestrator's own reading against the same rubric agreed.

| Criterion | A | B | Deciding evidence |
|---|---|---|---|
| Parity fidelity | 4 | 4 | Both absorb root-style scanning, attribute order, the two text forms, numeric intent and the layer split. A's always-self-closing shape rule is wrong for sankey's `<path>…<title/></path>` (`sankey/render.ts:227`). A's single path spelling misses radar's spaced commands (`radar/render.ts:42`). |
| Porter interface depth | 2 | 4 | A's porters push about 50 SVG attribute kinds by hand, and invalid tag/attribute pairs compile. B's porters call geometry builders and facets, and shared helpers own spelling and rewrites. |
| Rune product quality | 2 | 4 | A flattens group opacity into items, and it guesses baselines from a claim that there is "no public ascent API". That claim is false: `TextSystem::layout_rich` returns line metrics (`vendor/rune/crates/rune/src/text/rich_layout.rs:62`). B keeps compositing boundaries and uses the real line metrics. |
| Single source of truth | 3 | 4 | A keeps the type registry and the detectors as two lists. B derives both from one catalog. |
| Path to first milestone | 4 | 3 | A's chain from parse to draw to serialize reaches pie parity with less machinery. B front-loads sessions, anchors and coverage tiles. |
| Honesty about risk | 2 | 4 | A states as facts several things it never checked. These include the baseline API, "invisible" miter differences, and a global `idPrefix` default of `"pele"`, which is wrong for sankey (`sankey/render.ts:203`). |

**Grafts from A, folded into B:**

1. **Shared width-adaptation helpers that take draw closures.** These are `fit::{fit_width, turn_to_fit, tighten, title_room}`, as in A. Every port reuses the exact attempt order and winner rule from `fit-width.ts`. B's pie needs five attempts there (natural, narrow, three shrink), and the judge confirmed that count.
2. **Explicit style termination.** A's `Decls { terminated }` records whether a declaration list ends with `;`. That is what separates `rounded()`'s bare `style="rx:var(--_r)"` from the terminated output of `resolveStyle`. B's style spelling carries this flag.
3. **The translucent-stroke fast path.** A translucent stroke over nothing it overlaps is drawn opaque inside an element with `.opacity(a)`. Rune supports that coverage behaviour (`shape.rs:8`). B's CPU coverage tiles are kept only for what that cannot express: holes over unknown backdrops, miters thick enough to see, and gradients on paths.
4. **A's sequencing.** Prove the Scene and the SVG writer on pie at byte parity first. Then build the Rune interpreter. B's streaming anchors (`LayoutMemory`), hit-test index, coverage tiles and resource import stay as typed seams in the sketch, but nothing builds them until a test needs them. This follows the laziness protocol.

**Rejected from A:** the closed SVG `Tag`/`Attr` vocabulary as the porter surface (see the table), and baseline placement by `mid − 0.35·size` inversion.

**Rejected from B:** nothing outright. Its streaming session and tile budget are deferred, not removed.

## Tradeoffs accepted

- We accept private lexical recipes in exchange for exact bytes without exposing SVG transport objects to consumers.
- We accept local, scale-aware bitmap islands in exchange for correct coverage through Rune's public API; requiring every primitive to remain a Rune vector would sacrifice fidelity.
- We accept explicit shared font configuration in exchange for baselines derived from the actual shaper rather than empirical offsets.
- We accept incomplete-stream frame retention and ambiguous-duplicate identity limits in exchange for a strict parity parser and predictable updates.

## Alternatives considered

- **SVG as the Scene:** hides byte production but makes the native caller implement XML/CSS interpretation, semantic reconstruction and text placement. Its public interface is small but its actual interface is the whole SVG language.
- **Clean semantic Scene plus independently maintained SVG output:** hides rendering from the app but exposes a second coordinated drawing to every porter; geometry, rewrites and layer order can diverge.
- **Rune-only vector lowering:** looks simpler but pushes unsupported holes, joins and gradient decisions into individual diagrams or callers. Local coverage tiles hide that complexity once while preserving the semantic Scene.

## Open questions and risks

- Can the proposed public-API baseline path be verified against Rune's committed text placement for fallback fonts, variable fonts and device-scale snapping before claiming native text fidelity?
- Is per-atom coverage rasterization acceptable for complex primitives, or does the product require an all-vector path that Rune's current public API cannot guarantee?
- Which user-facing distinction should identify a retained streaming frame and rune-mermaid fallback in the host's UI?
- What identity should a host expect when it inserts or reorders semantically indistinguishable anonymous duplicate edges?
- Do all corpus trig results survive the chosen fdlibm-equivalent implementation, and do later ports reveal additional lexical policies the private recipe vocabulary must absorb?

## Next implementation step

Build pie end to end through the ordered Scene and parity writer, including empty/full donut paths, rich title text, palette overrides and all five width attempts, and verify its bytes against the pinned oracle before adding the Rune interpreter.
