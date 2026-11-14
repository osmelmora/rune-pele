# rune-pele: Scene and pipeline (candidate 1, "Pele's SVG, typed")

## Problem

rune-pele must turn Mermaid source into two outputs from one pipeline. The first is SVG that is byte-identical to Pele 1.13.0, scored by `pele parity` over 522 corpus sources. The second is native Rune elements that theme live, animate between source edits, support hover and accessibility, and beat Pele's own interactive features.

The shape is not obvious because Pele has no drawing object. Between `CNode/CEdge` layout and output there are only template strings (grounding 02). Every quirk that parity depends on lives in those strings:

- attribute order per call site;
- `num()` versus `${}` numbers;
- `rounded()` splicing `rx:var(--_r);` into a style string;
- `replace('stroke="var(--_b)"', …)`;
- a root style computed by regex-scanning the finished body;
- two text forms;
- `<title>` tooltips placed differently per type;
- edge labels painted in a layer separate from their edges.

Rune has none of SVG's vocabulary. It has no arcs, miters, holes, path gradients, baseline shift, CSS or ascent API (grounding 03).

The constraints the design honours:

- `pele_core::render(&str) -> Result<Rendered, RenderError>` stays the CLI's parity entry.
- Every number reaching SVG goes through `pele_core::js`.
- Only `rune-pele` depends on `rune`, and only through Rune's public API.
- The registry must scale to 33 types with no hand-synced list.
- Pele's module globals (`withPalette`, `withMath`, `withIcons`, label `marks`) must become explicit.

## Usage (caller's view)

### 1. A Rune app developer: a chat reply whose Mermaid block streams in and themes live

```rust
use rune::{Cx, El, col, text};
use rune_pele::{Diagram, Fonts, Hover, Link, Theme};

fn reply_view(s: &Chat, cx: &Cx) -> El {
    // Colours only. Changing them repaints from the cached layout and never relays out.
    let theme = Theme::neutral(cx.system_dark()).accent(s.accent);
    col().children(s.blocks().map(|block| match block {
        Block::Mermaid { id, source } => Diagram::new(id, source)
            .theme(&theme)
            .fonts(Fonts::rune()) // measured with rune::text; changing fonts does relay out
            .streaming(!s.done) // draws the longest line prefix that parses; parts keep their keys
            .hover(Hover::Edges) // hovering a node lights its edges and their labels
            .on_link(|s: &mut Chat, link: &Link, tx| {
                if link.internal { s.notes.open(&link.href) } else { link.open(tx) }
            })
            .el(), // always an El: native, rune-mermaid for unported types, or an error card
        Block::Text(t) => text(t),
    }))
}
```

The developer never passes a width. `el()` wraps the drawing in a fill-width frame keyed `{id}/frame`, reads `rune::layout_size` for it, and gives that to Pele's `maxWidth` logic, so `fitWidth`, `turnToFit` and `tighten` adapt as they do in Pele. A settings screen that previews themes reuses the same layout:

```rust
let themes = [Theme::neutral(false), Theme::neutral(true), Theme::neutral(false).series(brand_palette)];
row.children(themes.iter().enumerate().map(|(i, t)| {
    Diagram::new(format!("preview-{i}"), SAMPLE).theme(t).el() // one cached layout, three paints
}))
```

### 2. The `pele` CLI parity path

```rust
// crates/pele-cli/src/main.rs: the parity loop keeps its shape. The new RenderError variants must be handled.
match pele_core::render(source) {
    Err(RenderError::NotPorted(_)) => tally.not_ported += 1,
    Err(RenderError::Pele(_)) => tally.different += 1, // Pele drew it and we refused
    Ok(ours) => { /* byte compare ours.svg, ours.width, ours.height, as today */ }
}

// crates/pele-core/src/lib.rs
pub fn render(source: &str) -> Result<Rendered, RenderError> {
    let options = Options::parity(); // fontSize 16, padding 8, idPrefix "pele", now = 2026-01-15T12:00Z
    let drawing = parse(source, &ParseOptions::default())?.draw(&options, &Metrics)?;
    let svg = drawing.to_svg(); // pele_core::svg, the one writer
    options.limits.check_output(&svg)?; // 4,000,000 UTF-16 units, as Pele
    Ok(Rendered { width: drawing.width(), height: drawing.height(), svg })
}
```

A product-mode test, with no GPU, reads the same drawing as a renderer-neutral `Picture`:

```rust
let picture = pele_core::parse(src, &ParseOptions::default())?.draw(&Options::default(), &Metrics)?.picture();
assert!(picture.items.iter().all(|i| i.layer < picture.layers.len()));
```

### 3. A porter writing pie's draw step against the Scene

The porter's whole contract is one `Port` impl. Draw code mirrors `render.ts` line by line. Attributes are pushed in Pele's literal order. Numbers say whether they go through `num()` (`num`) or a template literal (`raw`). Paints are tokens, and identity is a `PartId`.

```rust
// crates/pele-core/src/diagrams/pie.rs
pub(crate) struct Pie;
impl Port for Pie {
    type Model = PieModel;
    fn parse(src: &Source<'_>) -> Result<PieModel, PeleError> { build(parse_pie(src.text)?, src.title) }
    fn draw(m: &PieModel, cx: &DrawCx<'_>) -> Result<Drawing, PeleError> {
        Ok(fit::fit_width(cx, RADIUS * 2.0, 140.0, |diameter, tight| draw(m, cx, diameter / 2.0, tight)))
    }
}

fn draw(m: &PieModel, cx: &DrawCx<'_>, radius: f64, tight: bool) -> Drawing {
    let (size, pad) = (cx.font_size(), cx.padding());
    let small = js::round(size * 0.875);
    let opts = cx.section("pie");
    let text_position = opts.number("textPosition").filter(|p| (0.0..=1.0).contains(p)).unwrap_or(0.75);
    // … legend labels via cx.label(..), title, shares, narrow, margin, cx/cy: arithmetic copied from render.ts …
    let mut d = cx.sketch(Type::Pie); // one sketch per fitWidth attempt; losers are dropped whole
    let (mut slices, mut inside, mut outside) = (vec![], vec![], vec![]);
    for (index, (name, _)) in m.sections.iter().enumerate() {
        if shares[index] == 0.0 { continue; }
        let end = angle + shares[index] * js::PI * 2.0;
        let item = d.part(PartKind::Item, name); // idempotent: the legend item below gets the same id
        slices.push(el(Tag::Path).class("pele-slice").data_id(item).d(slice(angle, end, radius, inner)).fill(Paint::series(index)));
        let pct = Txt::verbatim(js::to_fixed(shares[index] * 100.0, 0) + "%");
        let mid = (angle + end) / 2.0;
        if narrow[index] {
            let (x, y) = nudge_clear(mid, rim, &mut placed, cx.measure().width(pct.as_str(), small, Style::empty()));
            outside.push(el(Tag::Text).belongs(item).x(num(x)).y(num(y + small * 0.35)).text(pct));
        } else {
            let r = radius * text_position;
            inside.push(el(Tag::Text).belongs(item).x(num(js::sin(mid) * r)).y(num(-js::cos(mid) * r + small * 0.35))
                .fill_opt(d.series_ink(index)).text(pct));
        }
        angle = end;
    }
    if slices.is_empty() {
        slices.push(el(Tag::Circle).class("pele-slice").r(raw(radius)).fill(Paint::None).stroke(Role::Border));
    }
    let at = translate(num(center_x), num(center_y));
    let mut body = vec![
        label(&title, total_w / 2.0, pad + title.height / 2.0, Attrs::new().class("pele-title").font_weight(Weight::Title)).layer("title"),
        el(Tag::G).class("pele-slices").transform(at.clone()).stroke(Role::Bg).stroke_width(raw(1.0))
            .stroke_linejoin(Join::Round).children(slices).layer("slices").into(),
    ];
    body.extend(el(Tag::G).class("pele-slice-labels").transform(at.clone()).font_size(raw(small))
        .text_anchor(Anchor::Middle).fill(Role::Bg).children(inside).layer("slice-labels").if_any());
    body.extend(el(Tag::G).class("pele-slice-labels").transform(at).font_size(raw(small))
        .text_anchor(Anchor::Middle).children(outside).layer("outside-labels").if_any());
    body.extend(el(Tag::G).class("pele-legend").font_size(raw(small)).children(legend_items(&mut d, m, &legend, lx, ly)).layer("legend").if_any());
    d.finish(total_w, total_h, m.acc.clone(), body)
}
```

The usage fixes the spec:

- Draw code never writes markup, escapes, `var()` strings, or the root element.
- Identity, layers and tooltips are data, not class-name conventions.
- The same `Drawing` yields SVG (`to_svg`) and a `Picture` for Rune.

## Shape

```text
source ─▶ source::prepare ─▶ types::detect ─▶ Port::parse ─▶ Model
                                                   │
                     DrawCx{options, &dyn Measure, config, palette, icons, math}
                                                   ▼
                    Port::draw (measure, lay out, build the Scene; fit/turn/tighten redraw) ─▶ Drawing
                                ┌──────────────────────────┴──────────────────────────┐
                    svg::write (serialize, parity)                  picture::flatten (SVG semantics, product)
                                ▼                                                     ▼
                         String (byte parity)                rune-pele: lower ─▶ Prepared (cached) ─▶ paint(theme) ─▶ El
```

### Decision 1: the Scene is Pele's SVG, typed

The Scene is the tree Pele writes, with every string replaced by a closed type. A `Node` is one of:

- `Elem { tag, attrs, children, meta }`;
- `Text(Txt)`;
- `Label` (labelSvg semantics);
- `Icon` (the fixed `pele-icon` nested svg).

`Tag` is the closed set Pele emits: g, a, title, path, rect, circle, ellipse, polygon, text, tspan, image, defs, linearGradient and stop. The survey found no line, polyline, marker, use, clipPath or style. `Attrs` is a `Vec<Attr>` in call order and is never sorted. Each `Attr` variant carries a typed value:

- `X(N)` and `D(PathData)`;
- `Fill(Paint)` and `Transform(Transform)`;
- `Style(Decls)`, `DataId(PartId)` and `FontWeight(Weight)`;
- about 45 more, all observed in a grep of the Pele source.

`N` is `Num(f64)` (written by `js::write_num`) or `Raw(f64)` (written by `js::to_string`). `PathData` is typed segments with per-number format. It replaces both the `d` string and the `tokens` sidecar that `fillUnder` replays. `Meta` is never written. It holds `part` (silent membership), `layer` (a static name) and `link` (an index into the sketch's link table).

Why this shape: parity is a property of exact bytes, and Pele's bytes are a tree of a few dozen element and attribute kinds. A typed mirror of that tree is the smallest structure that holds every byte-relevant decision without a string. Each decision becomes a variant, an order or a constructor (per **encode-lessons-in-structure**). The SVG writer is then mechanical. The Rune side gets typed geometry and tokens, not strings to re-parse (per **boundary-discipline**: we parse into domain types at construction instead of serializing and re-parsing).

### Decision 2: two derived views, one source of truth

`Drawing` is immutable and crate-private in its structure. It offers two derivations and nothing else:

- **`to_svg()` in `pele_core::svg` (a module, not a crate).** It writes the body into a buffer, scans the buffer with Pele's own `var\(--_(r|tw|hw|w)\)` regex, then writes the root. So root style is derived from the output exactly as Pele derives it, including any `var(--_r)` an author put in a `style`. Nothing has to be kept in sync. labelSvg's two forms, `textStyle` hoisting, icon and math siblings, `esc`, void versus container tags, and `svgDocument` all live here once.
  - It is a module because Scene and writer change together. Every new `Attr` needs a write arm, and an exhaustive match in the same crate makes that edit compulsory. `render()` already returns SVG.
  - A separate `pele-svg` crate would force a third `pele-scene` crate to break the dependency cycle, and there is no second writer to justify the seam.
- **`picture()` in `pele_core::picture`.** It applies SVG semantics once, in Rune-free code:
  - the presentation-attribute cascade, with `style` declarations overriding attributes;
  - transforms baked into absolute coordinates;
  - rect, circle, ellipse, polygon and arcs converted to line and cubic contours;
  - `<a>` scopes turned into item links, and `<title>` children into the enclosing part's tooltip;
  - `defs` gradients resolved, and CSS colour text parsed into `Ink`. Invalid values are dropped as a browser drops them, falling back to the attribute.

  The result is a flat `Vec<Item>` in paint order. Each item is tagged with part, layer, link, opacity and hit-testability. `Picture` is public and theme-independent. It is the renderer-neutral contract, so any future target (PDF, canvas, a tiny-skia test rasteriser) reuses it.

This is not a pass-through: tree to flat, inherited to explicit, local to absolute, SVG primitives to contours, strings to `Ink`. Each step changes the abstraction (per **minimize-reader-load**).

### Decision 3: identity is a table, data-id is the part

`Sketch::part(kind, raw_id)` interns `(PartKind, decoded id)` and returns a `PartId`. It is idempotent: the slice and its legend item get one id. `Sketch::edge_part(id, from, to)` also records the edge's ends, for hover and accessible names.

`Attr::DataId(PartId)` writes `esc(part.id)`, so Pele's `data-id` and our identity cannot drift. Elements with no data-id, such as pie's inside labels, use `belongs(part)`. An element's part is its own data-id part, else its `belongs` part, else its parent's.

Layers are `meta.layer` names on whichever node starts a layer. A name is needed, not an index, because Pele omits empty layers. Rune keys are `"{kind}:{id}@{layer}#{n}"`, stable across edits whenever Pele's ids are (flowchart edges are `L_{a}_{b}_{nth-of-pair}`). Edge, edge label and casing are one part in three layers, and paint order is never compromised to keep an edge in one subtree.

### Decision 4: paint is a closed token vocabulary resolved late

`Paint` has these variants:

- `None` and `Current`;
- `Role(Role)`, Pele's eight `--_x` aliases;
- `Series(u8)`;
- `Token(Critical | OnLight | OnDark | Error)`;
- `Mix { a, pct, b }` for the struck-title `color-mix`;
- `Url { id, fallback }`;
- `Css(CssText)`. `CssText` is a newtype defined in `kit` with a `pub(in crate::kit)` constructor, so only `kit::style::resolve_style` and `kit::palette::safe_color` can construct it. Unvalidated colour text cannot reach the tree.

`themeVariables` overrides become `Palette { series: [Option<CssText>; 8], inks }` on the Drawing.

- In SVG they are appended as `;--pele-series-N:…` after the font style, as `paletteStyle()` does.
- In `Picture` they replace `Series(i)`, because the diagram's own variables beat the host theme, as Pele's root-level CSS does.

`Theme` (core, sRGB) maps roles, series and tokens to colours plus `radius` and `dark`, and `Theme::PELE` is Pele's fallbacks. Fonts and weights are not in `Theme`: they change measurement, so they belong to the layout key. That split lets a colour change skip layout.

### Pele's quirks, absorbed once

| Quirk | Absorbed by | How |
|---|---|---|
| Root style computed from content | `svg` writer | Writes the body, regex-scans its own bytes, then writes the root. Root order (fit, ROOT_STYLE, weights, font, palette) is fixed in one function. |
| Per-call-site attribute order | the porter's call order | `Attrs` is insertion-ordered. Shared call sites (label, marker, shapes, edge label, casing, cluster, title) are ported once in `kit`, so a type never re-encodes them. |
| Two text forms | `Node::Label` | The writer applies labelSvg's fast-path test to the label's content. Hand-built `<text>`/`<tspan>` from er, railroad or sequence stays a plain `Elem`. |
| Shapes local to `translate()` | nothing special | The Scene keeps the transform as written. `Picture` composes it. Zero-valued attributes are simply not pushed, as in Pele. |
| Raw versus `num()` numbers | `N::{Num, Raw}` | Every number states its format at the call site, and both write through `pele_core::js`. Trig goes through `js::sin/cos/atan2` (fdlibm via `libm`) and `max`/`min` through `js::max` (NaN-propagating). |
| String-level style rewrites | typed Scene operations | `Attrs::rounded()` (splices `rx:var(--_r)` first, or appends an unterminated style). `Attrs::replace(Attr::Stroke(Paint::Role(Role::Border)), Attr::Stroke(Paint::None))` (datastore). `Node::recolor(Fill, Role::Bg → css)` (`hollowOn`). `Node::strip_class("pele-marker")` (casing). `Decls::get(Prop::StrokeWidth)` (casing). `Attrs::has(AttrName::FontWeight)` (`textStyle`). `Decls.terminated` reproduces the `;` after resolveStyle output. |
| Tooltip position per type | positional `Tag::Title` | The porter puts `<title>` where Pele does: first in the group, or inside the shape. `picture` attaches it to the enclosing part either way. |
| Separate edge-label layers | parts span layers | The same `PartId` appears in edges, edge-labels and casings layers. Hover and keys group by part, and paint order follows the tree. |
| `esc` versus `escText` | `Txt::decoded` / `Txt::verbatim` | Scene text is always final text. The writer always applies `esc`, and Rune shows the same string. |
| Module globals | `DrawCx` + `Sketch` | Palette, math, icons and label marks are explicit. Each fit attempt gets its own `Sketch`, so the redraws share no mutable state (per **separate-before-serializing-shared-state**). |

### Diagram contract and registry

`trait Port { type Model; const SECTION; const KEEP_COMMENTS; const MATH; fn parse(&Source) -> Result<Model>; fn draw(&Model, &DrawCx) -> Result<Drawing>; }` is Pele's `Diagram<Model>` with globals made explicit. Layout stays inside `draw`, as in Pele, where measurement, layout and drawing interleave. A separate per-type layout stage would be temporal decomposition.

One macro row per type defines `Type`, `Type::name()` (all 33 `svgDocument` literals equal the `DiagramType` names; checked), `Type::is_ported()` and dispatch: `diagram_types! { Pie = "pie" => diagrams::pie::Pie, Sequence = "sequence" => _, … }`. Porting a type means replacing `_` with the module. The exhaustive match compiles only if every row is handled.

Pele's ordered `DETECTORS` (37 rows, order-significant) is separate data that references `Type` variants. A unit test over `Type::ALL` fails the build if a type has no detector. `Parsed` erases the model (`Box<dyn Drawable>`), so rune-pele can parse once and draw at several widths.

The measurer is the only injected strategy: `trait Measure { fn width(&self, text, size, Style) -> f64; fn epoch(&self) -> u64; }`, matching Pele's `TextMeasurer`. `Metrics` (Arial tables, UTF-16 iteration) is the parity measurer. `RuneMeasure` (rune-pele) calls `rune::text::advance` and applies Pele's sub/sup `/1.2`. Line breaking, line heights (`round(size*1.5)`, wrapped `1.3`) and label geometry stay Pele's code in core. Rune only answers widths, so product text never re-wraps.

The other options split by when they are read:

- **`ParseOptions`:** limits and host config.
- **`Options`:** draw-time values. These are fontSize, padding, maxWidth, autoDirection, directionBreakpoint, idPrefix, responsive, link/image policy, fonts, monospace, `now` (no implicit clock in core), and the icon and math resolvers.
- **Errors:** `PeleError { code: UnsupportedDiagram | Syntax | Semantic | Limit, message, ty, mermaid_type, line, column, snippet }`, wrapped in `RenderError::{NotPorted(Type), Pele(PeleError)}`.
- **Limits:** source length in UTF-16 units, output length, `maxEdges`, and an explicit nesting depth. Rust cannot catch stack overflow the way Pele catches `RangeError`, so recursive layout counts its depth.

Width adaptation is `fit::{fit_width, turn_to_fit, tighten, cap_width, title_room}`. Each takes an `FnMut` that returns a whole `Drawing` and compares `drawing.width()`, as Pele compares `Rendered.width`. It is pure and identical in both modes. Core also guarantees, and tests over the corpus, that for any `maxWidth ≥ draw(None).width()` the output equals the unconstrained output. rune-pele uses that guarantee to relayout only when its frame is narrower than the free layout.

### rune-pele: the only `rune` dependent

`Diagram` is a builder: `new`, `theme`, `fonts`, `streaming`, `hover`, `on_link`, `on_error` and `el`. `el()` does the following:

1. **Detect and choose a path.** `pele_core::detect` decides between:
   - ported: the native path;
   - not ported and `rune_mermaid::supported`: `render_with_hover`, or `render_streaming`, with a `DiagramStyle` mapped from `Theme` and `Fonts`;
   - neither: the `on_error` card.
2. **Lay out with a cache.** `cache::prepared` is a content-addressed thread-local LRU. The key is the source hash, the layout options, `Fonts`, `rune::text::font_epoch()`, and either `None` or a quantised frame width. The value is `Rc<Prepared>`. Two diagrams writing the same key write the same value, so sharing is harmless. Entries are inserted only after a successful build inside `catch_unwind`. Under `streaming(true)`, the source is cut to complete lines, and the longest prefix that parses is drawn, searching back at most 8 lines. That is derived from the input, not from remembered state (per **make-operations-idempotent**).
3. **Lower.** `lower::prepare(&Picture)` applies Rune policy and is theme-independent. It produces pieces with local points and pre-parsed `Ink`, text runs, hit regions, a part-to-pieces index, and a node-to-edges index built from `ends`.
4. **Paint.** `paint::el(&Prepared, &Theme, &Look)` runs inside `memo("{id}", (prepared.key, theme.hash64(), hover), …)`. It resolves `Ink` in sRGB (`color-mix` as CSS defines it, not `Color::mix`'s linear light), converts to Rune's linear `Color`, and emits absolute, keyed sibling Els in paint order.
   - Consecutive shape items of one part and layer batch into one `canvas`.
   - Text is interleaved where the tree puts it. This fixes rune-mermaid's "labels above everything" gap.
   - It adds hover groups, `help(tip)` tooltips, `on_press_tx` links with `Cursor::Pointer`, and `role`/`label` accessibility. The root label is accTitle, then title, then type name. A part's label is its text, and an edge's label is "A to B: label".

| SVG feature | Rune lowering (policy in `rune-pele::lower`, maths in `pele_core::geom`) |
|---|---|
| Arcs, quads | Converted to cubics in `picture`, then flattened at Rune's 0.1 pt tolerance (`geom::flatten`) |
| Dashes | `geom::dash(polyline, array, offset)` makes segment strokes with the item's cap. Butt becomes `LineCap::Butt` |
| Miter joins | Closed strokes ≥ 2 pt become `geom::stroke_ring` fills. Thinner strokes take round joins (invisible at 1 pt) |
| Holes and multiple contours | `geom::simple_polygons` (non-zero) splits each hole with a vertical slab cut. Pieces overlap 0.25 pt across cuts, so antialiasing leaves no seam |
| Translucent stroke | Drawn opaque in its own canvas El with `.opacity(a)`, so joints never double-blend |
| Group `opacity` | `El::opacity` on the piece |
| Sub/sup | The run becomes its own text El at size/1.2, shifted −0.33 em or +0.2 em, and placed by the label's span widths |
| Gradient on a path | `geom::bands` clips the polygon into vertical strips of at most 4 pt (64 at most), each a solid sRGB-interpolated colour |
| `color-mix` | Computed in core sRGB at paint time |
| Text stroke (struck-title halo) | Eight offset copies in the halo colour at stroke-width/2 |
| Baseline to top-left | A Label's line box is known: top = mid − lh/2 with line height lh, and Rune centres glyphs in the box. Raw `<text>` inverts Pele's own convention: mid = y − 0.35·size. Anchor x uses `advance` |
| Icons, math | Rasterised from their SVG markup (`ImageCache::svg`, paths only). HTML math falls back to TeX text in mono |

### Invariants, validation, and what is deliberately absent

- **Encoded in types:**
  - closed `Tag`, `Attr` and `Prop`;
  - `N`'s format, and `Txt`'s escaping intent;
  - `CssText`, which only validators can construct;
  - `PartId`, which only a `Sketch` issues;
  - `Drawing`, which only `Sketch::finish` builds;
  - layer names, which are `&'static str`;
  - `Picture` paints, which are parsed `Ink` and never strings.
- **Validated at boundaries:**
  - source limits in `parse`;
  - config through JS-coercion accessors (`ConfigView::number`, `js_string`) at the point each type reads it, as Pele does;
  - author CSS in `resolve_style` and `safe_color`;
  - URLs in `kit::url`;
  - the output limit in `render`.
- **Trusted inside:** everything else.
- **Deliberately absent:**
  - no free-form attribute names or raw markup in the tree, except host-provided icon and math markup;
  - no sorting;
  - no prior-layout input to core (parity requires deterministic layout);
  - no per-type code in the writer, flattener or adapter;
  - no SVG parsing anywhere.

Interface depth: rune-pele exposes one builder and four small value types. Behind them sit:

- detection, parsing and Pele's width adaptation;
- caching, lowering, theming, hover, links, accessibility and streaming;
- the rune-mermaid fallback.

pele-core's public surface is the following. The Scene, the kit, the layouts and the 33 ports are `pub(crate)`, so nothing outside can import internals:

- `render`, `parse`, `Parsed::draw`;
- `Drawing::{width, height, links, to_svg, picture}`;
- the `Picture` data types;
- `Measure` and `Metrics`;
- `Options`, `ParseOptions` and `Limits`;
- `Type`, `PeleError`, `Theme` and `Ink`.

The deep part is the porter-facing Scene: about 20 builder methods buy byte parity and product semantics for every type.

## Synthesis decision

## Tradeoffs accepted

- We accept that porters write attributes in Pele's literal order by hand, and that `.r()` on a rect compiles. In exchange we get a 1:1 port the oracle checks byte for byte. Per-tag typestate builders would double the surface to forbid mistakes the corpus already catches.
- We accept closed `Tag`/`Attr`/`Prop` enums, where a newly found Pele attribute means editing `scene`, `svg` and `picture`. In exchange no type can smuggle markup past the writer, and the exhaustive matches make the three edits compulsory.
- We accept a regex pass over the finished body in the writer. In exchange there are no "uses radius or weight" flags to keep in sync with the content, and Pele's scan semantics are reproduced even for author CSS.
- We accept that `Picture` loses exact arcs, which become cubics. In exchange every non-SVG target, and every transform, sees one segment vocabulary.
- We accept a runtime check (`debug_assert` in `finish`) instead of a lifetime brand to stop a `PartId` crossing from a losing fit attempt into the winner. In exchange `Node` carries no lifetime parameter.
- We accept that product text uses Pele's line breaking with Rune widths, not Rune's wrapping. In exchange labels never re-wrap at paint time, and layout stays a pure core function.
- We accept a one-frame lag on first mount, wide then narrower, when the frame is narrower than the free layout. In exchange there is no relayout at any width at or above it, while Pele's mount redraws and compares strings on every resize.
- We accept one El per translucent stroke and per sub/sup run. In exchange compositing and baselines are correct.

## Alternatives considered

- **Port as strings, then parse the SVG back in rune-pele.** This is the fastest route to parity: a transliteration of the TS. It loses on interface depth. Roles, part identity, links and label spans would cross the boundary as a wire format, so rune-pele would re-derive them from `class`, `data-id` and `var()` text. That is information leakage, with transport types on the product path.
- **A semantic Scene** (`Node`, `Edge`, `Slice`, `Label` primitives, renderer-agnostic) **with a per-type Pele emitter.** It is pleasant for Rune but makes every type re-encode attribute order, layer splits and string quirks in its emitter. That is exactly what the task forbids, and it gives two descriptions of each diagram to keep in sync.
- **A flat Picture as the only Scene, with an "SVG hints" sidecar.** Pele's bytes depend on nesting (`<a>` inside `<g>`, `<title>` placement, inherited group attributes), so the sidecar would have to rebuild the tree. That is two representations under one name.
- **`pele-svg` as its own crate.** It needs a `pele-scene` crate under both to avoid a cycle. That makes three crates move in lockstep for each new `Attr`, with no second writer to justify the seam.

## Open questions and risks

- Does Rune centre a single-line text El's glyphs vertically in a box of height `lh`, with no public ascent API? If not, `paint` needs an empirical offset per `TextStyle`, measured once per `font_epoch`.
- Does Rune's private ear-clipper accept the 0.25 pt overlapping slab pieces without seams at fractional scales, or should holes over a known backdrop paint the backdrop instead?
- Should product mode improve on Pele where Pele is static? An example is choosing label ink against the live theme's series colour, not only for `themeVariables` overrides. That is a deliberate visual divergence from parity output.
- Does every width helper in all 33 types satisfy "maxWidth ≥ free width ⇒ identical output"? Titles wider than 4000 px are a known exception to verify. The rule is checkable with the oracle, but pie and flowchart are the only types traced so far.
- The path vocabulary (`M L H V C Q T A Z` and relative forms, `x,y` pairs separated by spaces) matched every `d=` in a grep. Is there any call site with another separator? If one appears, `PathData` gains a variant, not a raw string.
- What should the product default font size be? The default here is 14 pt, against Pele's 16 px and rune-mermaid's 13 pt.
- Should memoized El subtrees reuse old `on_link` closures? Rune's memo keeps the previous handlers when the version matches.
- Does building Rune at the pinned dev rev work on Rust 1.100?

## Next implementation step

Write `pele_core::scene` and `pele_core::svg` with pie's title, slices and legend. Then make `pele parity --type pie` byte-identical before adding `picture`, so the typed tree is proven against the oracle first.
