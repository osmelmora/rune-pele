# Decisions

Newest last. Each entry gives the decision, why, and the evidence.

## 2026-11-14 Start with the number rules, the oracle and the parity scoreboard

Every later step compares against Pele, so the comparison tooling comes first. That tooling is the oracle, the `pele parity` scoreboard and the number helpers every coordinate goes through. No diagram type is ported yet.

Evidence:

- The differential tests pass on 212,038 doubles per op for `round`, `num`, `to_string`, and `to_fixed` at 6 precisions.
- The baseline parity run reports 522 corpus sources across 33 types, all not ported, with 0 Pele errors.

## 2026-11-14 The oracle runs Pele's TypeScript source, bundled by esbuild, in pinned Node

The npm build is minified and does not export `metricsMeasurer`, so the oracle uses the source instead. The oracle passes the same options Pele's own corpus test uses: `metricsMeasurer`, a fixed `now`, and `TZ=UTC`. The bundle's first line stamps the sha256 of `oracle.ts` and the Pele commit, so a stale bundle is detectable. Mtimes proved unreliable for this, because whole-second granularity passed a stale bundle.

## 2026-11-14 Kani for formal verification of the number helpers

The question is whether this Rust code matches JavaScript for every double, which is a property of the code. There is no concurrency or event ordering to model, so a model checker over a design (TLA+) does not fit. Kani reasons about `f64` bit-precisely on the real code.

- Kani proves `round` against the ECMAScript definition, and `num`'s branch choice and digit split against a transcription of Pele's double arithmetic.
- Each property was shown to fail on a broken variant. Using `f64::round` fails with input `-0.5` in 0.3 s. Computing `whole` with a float round fails with input `n = 2097152` in 1.3 s.
- The first split proof, over a symbolic `f64`, took 278 s. Splitting it per sign, quantifying over `u64`, and using kissat gave about 150 s and 214 s in parallel.

## 2026-11-14 Tools pinned in mise.toml at the latest releases

The pins are Rust 1.100.0, Node 26.13.1, Kani 0.68.0, and esbuild 0.28.2 through `harness/package-lock.json`. These were the latest versions `mise latest` reported on 2026-11-14.

## 2026-11-14 Design the Scene before porting pie

The renderer-neutral Scene is the one-way-door decision. All 33 types will emit it, and both the SVG writer and the Rune adapter will consume it. It is designed with the architect workflow before any diagram is ported.

- Three read-only explorers produced the grounding in `docs/grounding/`: Pele's pipeline, Pele's drawing vocabulary, and Rune's drawing surface. Rune was read at commit 165f87a.
- The how explainer pass was skipped. The three angles barely overlap, and the design runners read the raw findings directly.
- Two architect runners on different models design the Scene independently. A cross-judge on a third model scores them, then synthesis picks a base and grafts from the other.

## 2026-11-14 Scene design: candidate B as the base, with four grafts from A

The full record is `docs/design/scene/design.md` ("Synthesis decision"), with both candidates in `docs/design/scene/candidates/`.

- The Scene is one immutable ordered drawing. Semantic parts form a separate table. Private recipes keep Pele's exact spelling.
- The SVG writer is `pele_core::svg`.
- `rune-pele` alone depends on Rune.
- The cross-judge and the orchestrator agreed on B. Grafted from A: shared fit helpers, explicit style termination, the translucent-stroke fast path, and pie-parity-first sequencing.

## 2026-11-14 Two parallel units after the Scene decision

The two units touch disjoint crates, so each runs as its own leaf worker in its own worktree.

- **Pie at byte parity through the Scene.** Branch `pie-parity`, role `hardest tasks` (Claude Fable 5.2).
  - Done when `pele parity --type pie` shows 2 of 2 identical.
  - Also done when a harvested differential test of every pie source in Pele's test suite is byte-equal.
- **Rune adapter spike.** Branch `rune-spike`, role `feature, refactoring` (Claude Opus 5.5).
  - It settles three risks: Rune builds as a dependency on Rust 1.100, text lands on a baseline from public metrics, and a headless PNG can be captured.

Each diff is reviewed and each done check rerun before merging.

## 2026-11-14 Rune spike merged: all three risks verified

`rune-spike` was fast-forwarded into main at `b65f042`. The parent reran the tests and clippy and inspected the PNGs.

- **Rune builds as a git dependency on Rust 1.100.0 (measured).** A cold build took 31 s. `rune-pele` is the only crate that depends on `rune`.
- **Baselines (measured).** Text placed at `top = y − line.metrics().baseline / scale`, using `TextSystem::layout_rich`, lands on the rule's own pixel row in all 24 cases: sizes 12, 14 and 16, scales 1× and 2×, H and x, integer and fractional y. The worst error against the unrounded `y × scale` is 0.40 device px, because Rune rounds baselines to whole pixels.
- **Headless PNG capture works (measured).** `rune_pele::capture::Capturer` produces it.

Findings that change the Scene design:

- **Flatten curves in `lower` and pass points.** For zoom-adaptive curves, `lower` flattens them itself and passes points to `Shape::fill` / `stroke_with`. Rune's `Path` flattens at a fixed 0.1 pt before scaling.
- **Anchor advance must come from the same device-scale layout as the baseline.** `rune::text::measure` rounds widths to 1/64 pt.
- **Capturing with app fonts needs `Runtime::with_text`.**

## 2026-11-14 Pie merged at byte parity; trig stays on libm

`pie-parity` was merged at `e80f691`. The parent reran the done check before merging.

- Corpus: pie identical on 2 of 2.
- Differential test: 8,368 inputs (harvested, hand-written cases, Pele's mutation runs and seeded random pies), with 0 differences and 3,198 rejected with Pele's exact message.
- After the merge, `mise run check` passes, 38 of 38 workspace tests pass, and the doctor reports ok.
- The worktree evidence was copied to `out/verify/pie-parity/` before the worktree was removed.

**Decision: keep `js::sin`/`cos`/`pow` on the `libm` crate.**

- On darwin arm64, Node's V8 fdlibm uses fused multiply-adds and `Math.pow` uses the platform `pow`. So libm is within one ulp of the pinned Node, not bit-equal (measured: 300,834 of 304,334 `sin` samples are exact).
- That gap reached no SVG output in 31.4 million pie coordinates printed through `num()` (measured). Matching V8 bit for bit would tie the port to one CPU and build of Node.
- Revisit if a parity diff ever traces to a trig ulp. The oracle tests already print the exact-match counts that would show such a drift.

## 2026-11-14 Pie drawn natively in Rune merged

`pie-rune` was fast-forwarded at `1b35660`. The parent reran `mise run check` (exit 0), `pie_view` (6 passed) and pie parity (2 of 2), and inspected the side-by-side PNG.

- **Measured areas.** Slice areas are within 0.35% of the Scene's analytic areas, and within 0.33% of resvg's raster of Pele's own SVG, at 1× and 2×.
- **Theme change without relayout (measured).** A palette change gives layouts 1 → 1, measurements 16 → 16, and paints 1 → 2.
- **Product-mode differences from Pele's look** are listed in `DIVERGENCES.md`. A difference not listed there is a bug.
- **Capture fix.** A renderer per capture lost all text after the first capture, so `Capturer::window` keeps one renderer.

## 2026-11-14 Flowchart cost pilot, wave 1: front end and layout in parallel

Flowchart is split along its seams, by line count of Pele's TypeScript:

- the front end, about 2.6k lines: lexer, parser, db and graph;
- the compound layout, about 3.2k lines: layered, compound and components, which seven types share;
- drawing, about 1.7k lines: shapes, edges, clusters and render.

Wave 1 runs the first two in parallel worktrees with disjoint write fences. Drawing follows once both merge.

- **`flow-front`** (Claude Opus 5.5). Done when its model JSON is byte-equal to Pele's over every harvested flowchart source and seeded random inputs, with errors matching.
- **`flow-layout`** (Claude Fable 5.2). Real `compoundLayout` calls are recorded in the Node oracle across all seven types. Done when the Rust port replays every one of them bit-identically.

When wave 1 merges, the porting cost (lines, hours and findings) is re-estimated for the remaining 31 types.

## 2026-11-14 Flowchart front end merged; keep the `url` crate

`flow-front` was fast-forwarded at `130b1cd`. The parent reran `flowchart_front.rs` (5 passed) and `mise run check` (exit 0). Pie parity is unchanged.

- **Measured.** 14,323 inputs (harvested, Pele's mutation runs, seeded random, hand-written, and edge limits) gave 0 differences in model JSON. Every rejection matches Pele's message, code, line, column and snippet. 278 harvested inputs with lone surrogates are out of reach of `&str`.
- **Size measured.** Pele's front end is 2,661 TS lines, and the Rust port is 3,402 lines plus 726 lines of JSON view and URL policy.
- **Decision.** Keep the `url` crate (WHATWG) for Pele's `new URL`. 91 of 91 URLs in the unit test match Node, and so does every click URL in the corpus. Unusual hostnames could still differ (inferred). Add any such case to the URL test when one is found.

## 2026-11-14 Compound layout merged, bit-identical to Pele

`flow-layout` was fast-forwarded at `b4e956c`. The parent reran `layout_replay`, `flowchart_front`, `mise run check` and the doctor, all green.

- **Recorded calls (measured).** The oracle recorded every real layout call Pele makes over 13,301 sources (each once as written and once at `maxWidth` 320). After dedupe that is 1,810 distinct calls across agentflow, class, er, flowchart, requirement, state, swimlane and usecase. 1,810 of 1,810 replay double for double.
- **Live random graphs (measured).** 1,872 random graphs in the default run and 31,200 in the long run, plus 12 edge-case graphs and 400 `wrapComponents` calls, all identical. Mantissa-filling sizes were needed for the tests to catch reordered sums.
- **No trig in layout.** `Math.hypot` follows V8's algorithm (`layout/num.rs`), not libm's.
- **Speed (measured, release).** On the largest recorded graph, 41 µs in Rust against 53 µs in Node.
- **Known detection gap (inferred).** Reordering `shift[w] + otherDx - ownDx` is not caught by any current test. Catching it needs aligned chains of three or more nodes with fractional offsets.

Decisions on the worker's questions:

- `pele_core::layout` is `#[doc(hidden)] pub` so integration tests can reach it, the same convention as `flowchart_front_json`. Accepted.
- The esbuild tap in `harness/build.mjs` routes Pele's layout functions through a recorder in the oracle bundle only. Accepted. The cost is one global check per call, in the oracle only.
- The worker's rebase resolution of `harness/oracle.ts` (both blocks kept) was checked by rerunning both tests.

## 2026-11-14 Flowchart pilot complete; re-estimate for the other 31 types

`flow-draw` was fast-forwarded at `bbf8799`. The parent reran parity, `flowchart_parity`, `pie_parity` and `mise run check`, all green.

**Results (measured).**

- Corpus: flowchart 114 of 114 identical, pie 2 of 2. The scoreboard reads `522 116 0 406 0`.
- `flowchart_parity.rs`: 25,462 inputs, 0 different. This covers the harvest as written and at maxWidth 320/480/640/900, Pele's mutation inputs, 4,000 seeded random flowcharts with a 30-path coverage gate, and 80 cases.

**Decision.** The Rune view keeps flowchart on the rune-mermaid fallback until a flowchart adapter unit lowers dashes, curves, polygons, fill-opacity and the text styles. Accepted.

**What the pilot cost.**

| Unit | Pele TS lines | Rust lines (code + tests) | Worker runs |
|---|---|---|---|
| Pie, plus the shared input, measure, Scene and SVG writer | about 3,000 | about 6,000 | 1 |
| Flowchart front end | 2,661 | about 4,100 | 1 |
| Compound layout (shared by 7 types) | about 3,200 | layout plus replay | 1 |
| Flowchart drawing, plus shared shapes, edges, clusters, styles and markdown | about 1,860 | 5,420 | 1 |

That is roughly 2,500–3,000 TS lines per worker run, each verified against the oracle with differential tests. The workspace holds 25,216 Rust lines.

**What remains (measured with `wc -l`).** 39,185 TS lines across 31 types, with the per-type directories ranging from agentflow (3,781) to info (121). Also left: `layout/lanes.ts` (629, swimlane), and the parts of `svg/edges.ts` flowchart did not reach (`hollowOn` and others).

**Estimate (inferred from the pilot rate).** About 14–17 more worker runs to reach parity for all types. Graph types reuse shapes, edges, layout and styles, so they should run cheaper per line than the pilot. Add about 3 runs for the Rune adapter's general lowering (dashes, curves, polygons, opacity, text styles, links, tooltips and hover). After that, per-type Rune work is mostly generic.

Grouped into waves of 3–4 parallel workers on disjoint types, that is about 6 waves.

## 2026-11-15 Wave 1 started; class merged

**Start.** The user approved wave 1, the graph-family types. Two helpers that class, er and agentflow share (`parseGenericTypes`, `isUnicodeLetter`) were ported to `pele_core::common` on main first, so parallel workers reuse one copy. They match Pele on 20,010 inputs and on every UTF-16 code unit. Four workers then ran in parallel: state, class, usecase, and requirement plus swimlane. Er and agentflow follow in wave 1b.

**Class merged (measured, rerun by the parent after the merge).**
- Corpus: class 38 of 38 identical, flowchart 114 of 114, pie 2 of 2. Total `522 154 0 368 0`.
- `class_front.rs`: 30,364 inputs, 0 different. `class_parity.rs`: 79,751 inputs, 0 different, with a 35-path coverage gate on random diagrams.
- Cost: 2,317 TS lines in one run, giving 3,614 Rust lines of class code, 1,031 of tests and about 200 of shared growth. That is at the pilot rate, and cheaper per line than the pilot because layout, edges, clusters and the writer already existed.

**Decision.** The one real change to shared code is in `scene/walk.rs`. When one part draws several texts in one facet, their lines now number on from each other. No earlier type draws two texts in one part and facet, so no existing key changes. Accepted.

**Fix.** The oracle bundle's stamp hashed only `oracle.ts`, so editing a per-type op file such as `harness/class.ts` left a stale bundle that the doctor passed. The stamp now hashes every `harness/*.ts` in name order, and the doctor checks the same. A deliberate edit to `harness/class.ts` now fails the doctor.

## 2026-11-15 State merged

**Result (measured, rerun by the parent after the merge).**
- Corpus: state 22 of 22, with class 38 of 38, flowchart 114 of 114 and pie 2 of 2. Total `522 176 0 346 0`.
- `state_front.rs`: 14,209 inputs, 0 different. `state_parity.rs`: 25,058 inputs, 0 different, including Pele's 29 stress cases and a 32-path coverage gate.
- Cost: 2,291 TS lines in one run. That gave 3,531 Rust lines and about 70 lines of shared growth, all additions.

**Merge.** Class and state each added the same `LabelLook::font_size`, so one copy was dropped. Every other conflict held two additions side by side, and both were kept. The parity feature doc now gives one harvest line for every ported type, and the oracle stamp gotcha now covers every `harness/*.ts`.

**Shared additions from state.**
- `Number::minus_num`, written as `-${num(x)}`. It differs from `num(-x)` on half-hundredth ties.
- Dashes on layers.
- A class and a corner radius placed after a rectangle's geometry.
- `over_fills_at_end` in `graph/edges.rs`, because state checks only the end tip against fills.

## 2026-11-15 Requirement and swimlane merged

**Result (measured, rerun by the parent after the merge).**
- Corpus: requirement 9 of 9, swimlane 13 of 13, with every earlier type unchanged. Total `522 198 0 324 0`.
- Front ends: `requirement_front` 17,394 inputs and `swimlane_front` 9,877, both 0 different.
- Lane layout: `lanes_replay` replays 1,413 recorded calls and 3,011 live graphs bit for bit.
- Renders: `requirement_parity` 30,242 inputs and `swimlane_parity` 21,601, both 0 different, each with a 20-path coverage gate.
- Flowchart's own 25,462-input test is still 0 different with the swimlane variant hook in place.
- Cost: 2,115 TS lines (requirement 1,406, swimlane 80, `lanes.ts` 629) in one run.

**Merge.** Class and this branch had each added the same four things: the heading weight, `LabelLook::font_size`, `ShapeBuilder::join` and `default_reach`. The bodies were identical, so one copy of each was kept.

**Decision.** Swimlane runs through flowchart's renderer with a variant hook, mirroring Pele's `FlowVariant`. It is a small table of functions, not branches spread through flowchart. Accepted.

**Known gap for the Rune lowering unit.** Lane titles use a new rotate transform. The Scene walk reports a group's rotation but does not rotate what is inside it, so a Rune interpreter must apply it. This does not matter yet, because the Rune view sends every type except pie to the fallback.

**Pele quirks ported as they are.**
- A `title` line in a requirement diagram is a syntax error.
- Trailing spaces after `requirementDiagram` are a syntax error.
- A swimlane parse error names `flowchart` as its type.

## 2026-11-15 Usecase merged; one key rule for parts with several labels

**Result (measured, rerun by the parent after the merge).**
- Corpus: usecase 24 of 24, with every earlier type unchanged. Total `522 222 0 300 0`.
- `usecase_front.rs`: 21,922 inputs, 0 different, covering all four of Pele's error kinds.
- `usecase_parity.rs`: 39,688 inputs, 0 different, with a 50-path coverage gate.
- Cost: 3,099 TS lines in one run, at the top of the pilot rate. About 680 of the Rust lines reproduce V8's `JSON.parse` messages.

**Decision.** Class and usecase each needed several labels in one part and facet, and each gave them distinct keys a different way.
- Class numbered lines on from one label to the next. A label that wrapped differently would then shift the keys of every label after it.
- Usecase counts labels: the first keeps `/{line}.{run}`, and later ones get `/{label}/{line}.{run}`.

Usecase's rule was kept for both types, because a key should not move when an earlier sibling rewraps. Class's member keys changed to match, for example `node:Shape/attribute/1/0.0`. Byte parity does not read keys, so no SVG changed.

**Finding (usecase).** Pele's JSON errors carry V8's own `JSON.parse` wording. A Node pin change can change those messages, so rerun `usecase_front` after any Node bump.

## 2026-11-15 ER merged

**Result (measured, rerun by the parent after the merge).**
- Corpus: er 25 of 25, with every earlier type unchanged. Total `522 247 0 275 0`.
- `er_front.rs`: 19,398 inputs, 0 different. `er_parity.rs`: 31,201 inputs, 0 different, with a 38-path coverage gate.
- `er_walk.rs` passes under the label-count key rule. ER table cells use `Facet::Attribute`, and each cell gets its own key.
- Cost: 1,823 TS lines in one run. That gave 2,742 Rust lines and about 115 of shared growth.

**Merge.** State and ER had each changed the private fill check to take its marker tips as a slice. The two edits were identical, and state's version was kept.

**Shared additions from ER.**
- A group `opacity` property, written by the SVG writer and read by the walk.
- `row_rect`, a rectangle written as x, width, y, height.
- `label_from`, Pele's left-aligned `textLeft`.
- `fill_under_path`.
- Path sampling for H and V steps. These used to hit `unreachable!`.

## 2026-11-15 Wave 2 approved: the 24 standalone types

**Approval.** The user approved wave 2. It starts when agentflow and `rune-graph` have merged.

**Size (measured with `wc -l` and the corpus files).** 24 types, 24,393 TS lines and 263 doc sources. The largest is sequence (2,800 lines, 38 sources) and the smallest is info (121 lines, 2 sources).

**Shared helpers not yet ported (measured).** About 1,080 lines in all:

| Helper | Lines | Types that import it |
|---|---|---|
| `svg/palette.ts` | 138 | git, treemap, timeline |
| `common/literal-label.ts` | 101 | sequence, c4, eventmodeling, treeview, ishikawa, packet |
| `common/outline.ts`, `common/icon.ts` | 349 | kanban, mindmap |
| `common/scan.ts`, `common/fit.ts` | 137 | timeline, journey, git |
| `common/fit-text.ts` | 43 | treemap, packet |
| `common/accDescr.ts` | 26 | wardley, eventmodeling, treeview |
| `common/caseless.ts` | 10 | xychart, quadrant |

**Plan.**
1. **Foundation.** One unit ports these helpers first, each checked against the oracle, as the generics and unicode helpers were before wave 1. Parallel workers then reuse one copy.
2. **Three waves of four parallel workers**, each about 1,500 to 2,800 TS lines. Worker groups are listed below.

| Wave | Worker groups |
|---|---|
| 2a | sequence · gantt · block · c4 |
| 2b | railroad + info · architecture + cynefin · wardley + xychart · venn + quadrant |
| 2c | git + sankey + packet · eventmodeling + treeview + radar · treemap + timeline + journey · kanban + mindmap + ishikawa |

That is 13 runs in all (inferred from the wave 1 rate). Wave 1 held at 2,100 to 3,100 TS lines per run, and every run merged at full parity.

## 2026-11-15 Agentflow merged; wave 1 complete

**Result (measured, rerun by the parent after the merge).**
- Corpus: agentflow 12 of 12, with every earlier type unchanged. Total `522 259 0 263 0`.
- `agentflow_front.rs`: 22,786 inputs, 0 different, with a 28-feature model gate. It covers Pele's `agentflow-reference.json`.
- `agentflow_parity.rs`: 41,864 inputs, 0 different, with a 43-path drawing gate.
- Every other type's front and render tests pass after the merge. Agentflow changed the input layer, so they were all rerun.
- Cost: 3,781 TS lines, Pele's largest type, in one run. It was cheaper than its size suggests, because agentflow's lexer, parser and renderer are close copies of flowchart's.

**Shared changes.**
- The input boundary gains a comment-keeping preprocess and a line offset. Agentflow is Pele's only `keepComments` type.
- `Origin::locate` maps error positions through the comment-kept steps for agentflow only.
- Looks carry an optional fill-opacity.
- A relative arc, `PathStep::ArcBy`, sits beside usecase's absolute one.

**Wave 1 total.** Nine types (class, state, er, requirement, usecase, agentflow, swimlane, plus the pilot's flowchart and pie) are byte-identical to Pele. Wave 1 took six worker runs for about 15,000 TS lines, with no run failing parity at merge.

## 2026-11-15 Wave 2 foundation merged: shared helpers

**Result (measured, rerun by the parent after the merge).**
- `tests/common.rs`: 18 tests, 0 different. Each helper is checked against seeded random inputs and against the inputs Pele's own renders pass it. A new tap records those inputs over 1,656 harvested sources, at default, 320 px and 640 px, and with an icon host.
- `scripts/break-common.mjs` breaks seven helpers, and every break turns its test red.
- Corpus unchanged at `522 259 0 263 0`. Every type's front and render tests pass. `mise run check` exits 0.

**What exists now.** Each wave 2 worker reuses these instead of porting its own. Locations and signatures are in "Shared helpers for wave 2" in `.agents/skills/verify-rune-pele/features/parity.md`.
- Palette rules held by the Scene, with no module-level state as Pele has.
- `literal_label`, `break_label` and `left_label`.
- `diagrams/outline.rs`.
- `diagrams/scan.rs` and `acc_descr_named`.
- `plain_label`, `fit_label` and `fit_text`.
- `cap_width`.
- `titleBefore` and Chevrotain's `longer` alternatives in the tokenizer.
- `is_letter` and `is_hex`.
- `Role::Accent` and `Role::Critical`.

**Routing decisions.**
- Kanban and wardley each write text their own way (`textStyle`, `sharedStyle`, `spanStyle`), and both may need to grow the Scene's text forms. They sit in different waves (2c and 2b), so they never run in parallel.
- Sankey's `replacements` stays with the sankey worker.
- `rune-pele` must map `Role::Accent` and `Role::Critical` to app tokens. `rune-graph` branched before these roles existed, so the parent checks that at the `rune-graph` merge.

## 2026-11-15 Graph types drawn natively in Rune (`rune-graph` merged)

**Result (measured, rerun by the parent after the merge).**
- `mise run check`: 255 passed, 0 failed. `pele parity` is unchanged at `522 259 0 263 0`.
- `mise run rune-corpus` (release): 237 of 239 corpus sources of the eight graph types are drawn natively. Every one is within the preset 2× limits: largest unexplained patch 12 px, 5 unexplained px per 10,000, flat-colour error 4 levels. Text boxes are masked, since Rune draws Inter and resvg draws Arial.
- The two flowchart sources that fall back use a picture and a `stroke-dashoffset`.
- `break-lowering.sh` breaks six lowerings (dash, arc direction, hole, translucent stroke, group fade, text turn), and each one's test fails.
- For every opened type, keys are unique and survive an `idPrefix` change and a value edit. A palette change keeps layouts and measurements and adds one paint.
- Links reach the app's handler, and tooltips show on hover.
- Speed: a 401-node flowchart takes 3.5 ms to lay out, 1.1 ms to lower and 0.8 ms to paint. A state diagram of 3,000 nested composites takes 16.3 ms, 6.2 ms and 5.0 ms.

**Decisions.**
- **1× hairlines accepted.** At 1×, 146 sources exceed the preset limits, only along slanted or curved 1 px lines. Rune takes a stroke's coverage from distance to its centre line, and resvg from area. At 2× every source passes. 2× stays the gate. 1× is held to a looser bound (patch ≤ 30 px), listed in `DIVERGENCES.md`.
- **Agentflow opened at the merge.** rune-mermaid has no agentflow, so keeping it closed would mean an error. Its 12 corpus sources pass the 2× limits, and its keys and theme test pass with a new fixture, `tests/fixtures/rune/agentflow.mmd`.
- **Accent and critical roles.** The Rune palette maps `Role::Accent` to Rune's first accent at the theme's lightness. Rune has no danger colour, so `Role::Critical` keeps Pele's red.
- **Release corpus run.** It is the `rune-corpus` mise task and stays out of `mise run check`, which runs debug, where the comparison takes minutes.
- **Icons.** A host hook on `ViewOptions` is an API decision, deferred. Icons draw nothing, as Pele's SVG leaves the box empty. Done: `ViewOptions::icons`, in "2026-11-18 Follow-ups".

**Merge.** The wave 2 foundation had moved rich and line text into a shared `line_runs`. `rune-graph` had changed how each run's font is chosen from its own emphasis over the shared emphasis. Both changes are kept, in one `line_runs` that takes the shared emphasis.

**What later types will need from the lowering** (from `rune-graph`'s survey of Pele's drawing code):
- xychart and quadrant need a group with both translate and rotate.
- Wardley needs a rotate on a text element.
- Git turns labels by ±45°.
- Sankey uses `linearGradient` on paths.
- Sankey, treemap, journey and c4 put a `<title>` inside a shape.
- Sequence and architecture use icons.
- Math labels have no walk geometry yet.

## 2026-11-15 Wave 2a: block and C4 merged

**Result (measured, rerun by the parent after each merge).** Corpus: block 30 of 30 and c4 6 of 6, with every earlier type unchanged. Total `522 295 0 227 0`. `mise run check` passes and the doctor is ok.
- **Block.**
  - `block_front` covers 15,147 inputs.
  - `block_layout` replays 2,947 recorded and 3,000 random grid layouts bit for bit.
  - `block_parity` covers 25,527 inputs, with a 50-path coverage gate.
  - All 0 different.
  - `break-block.mjs` turns all 7 of its breaks red when the parent reruns it.
  - Cost: 2,149 TS lines in one run.
- **C4.**
  - `c4_front` covers 20,586 inputs.
  - `c4_layout` checks about 300,000 layouts and 85,000 routes bit for bit.
  - `c4_parity` covers 31,746 inputs, with a 63-path coverage gate.
  - All 0 different.
  - Six deliberate breaks.
  - Cost: 2,112 TS lines in one run.

**Findings.**
- `atan2` and `asin` (block) come from libm, as `sin` and `cos` do. Expect the same ulp-level risk. The tests show no differences.
- C4's random generator never reaches Pele's "careless" routing mode, which starts at about 800 relations. One Pele worst case of 2,450 relations covers it, identically.
- C4 has no walk test of Scene keys yet. Add one before C4's Rune gate opens. Done: `c4_walk`, in "2026-11-18 Follow-ups".
- Two workers (c4 and sequence) ended a turn to wait on a background job, which never resumes a delegated turn. Both were sent back. `wave2-common.md` now tells workers to run long commands in the foreground.

**Doc rule.** The whole-corpus recipe in `features/parity.md` no longer lists each type's row. It says that every ported row reads `<n> <n> 0 0 0` and gives the current total.

## 2026-11-15 Wave 2a: sequence merged

**Result (measured, rerun by the parent after the merge).**
- Corpus: sequence 38 of 38, with every earlier type unchanged. Total `522 333 0 189 0`.
- `sequence_front`: 25,895 inputs. `sequence_parity`: 38,279 renders, with a 60-path coverage gate. All 0 different.
- `sequence_walk` holds message and note keys across text edits and earlier statements.
- `break-c4.mjs` exits 0 on main after the C4 merge, which means none of its six breaks went unnoticed.
- Cost: 2,800 TS lines in one run.

**Decisions.**
- **Known limit, accepted.** Sequence menu JSON (`links`, `properties`) nested past 512 levels is a limit error here, where Pele parses it. This follows the input limits in `input.rs`, and a test pins it. Parity mode equals Pele below that depth.
- **Not-ported fixture.** `tests/fixtures/not-ported.mmd` is a mindmap that stands for "a type this port lacks" in the CLI and Rune tests. Mindmap is ported last, in wave 2c, and that worker must point the fixture at a type still unported, if any is left.
- **The Rune gate stays closed for sequence.** `graph_view` asserts `NotLowered(Kind(Sequence))`.

## 2026-11-15 Wave 2a complete: gantt merged

**Result (measured, rerun by the parent after the merge).**
- Corpus: gantt 11 of 11, with every earlier type unchanged. Total `522 344 0 178 0`, from 13 types.
- `gantt_dates`: 96,078 inputs, 0 different. It covers V8's date-string parser, `Date` constructors and accessors, dayjs parse and format, d3 ticks and `timeFormat`.
- `gantt_front`: 14,910 inputs. `gantt_parity`: 23,907 renders, with a 35-path coverage gate. All 0 different.
- `mise run check` passes, including the Rune crate, which builds with the new `LayoutOptions.now`.
- Cost: 2,159 TS lines in one run. 974 Rust lines of that are V8's `Date`.

**Decisions.**
- **`LayoutOptions.now`.** It is Pele's `now` option. Unset means the system clock. `parity_scene_with` pins Pele's corpus clock, 2026-01-15T12:00Z. When gantt's Rune gate opens, the view must pass an explicit `now`, or a cached layout will not move when the day changes.
- **UTC only.** Pele schedules gantt in local time, and this port assumes offset 0. It matches Pele under `TZ=UTC`, which parity pins. Under another zone, bars, ticks, excluded spans and the today marker would move. This is inferred, not tested.
- **Unnamed tasks** are keyed by their trimmed label, not by Pele's `task1` counter, so a key survives an edit above it. Pele's `data-id` is unchanged.
- Tick steps use `libm::log10` and `js::pow`. Every tick input matched, but that is not a proof.

**Wave 2a cost.** Four types (sequence, gantt, block, c4), about 9,220 TS lines, in four runs. Two runs needed a resend after ending a turn to wait on a background job.

## 2026-11-15 Wave 2b: railroad and info merged

**Result (measured, rerun by the parent after the merge).**
- Corpus: railroad 14 of 14 and info 2 of 2, with every earlier type unchanged. Total `522 360 0 162 0`, from 15 types.
- `railroad_front`: 15,068 inputs, one line per notation. `railroad_parity`: 26,675 renders, with a 42-path coverage gate. `railroad_walk`: Scene keys are unique.
- `info_front`: 2,942 inputs. `info_parity`: 3,071 inputs.
- All 0 different.
- Cost: 1,789 TS lines in one run.

**Notes.**
- Railroad has four notations, not three: Pele's own constructor calls (`railroad-beta`), EBNF, ABNF and PEG. All four lower to one rule model, and each has its own front-end test.
- **Key policy, accepted.** Each rule is a group entity, ranked by duplicate name. Every reference to nonterminal X shares `node:X`. Terminals, specials and counts are parts of their rule.
- **Thin coverage.** A break of the `\r` escape in railroad is caught by only 1 case and 1 harvested source.
- **Merge.** Sequence and railroad each added `TextStart::weight` with the same body, and one was kept.
- The "uncommitted edit in `c4/render.rs`" the worker saw on main was the C4 break script running at the time. The tree was clean afterwards.

## 2026-11-15 Wave 2b: architecture, cynefin, wardley and xychart merged

**Result (measured, rerun by the parent after each merge).**
- Corpus: architecture 6 of 6, cynefin 3 of 3, wardley 19 of 19 and xychart 8 of 8, with every earlier type unchanged. Total `522 396 0 126 0`, from 19 types.
- `mise run check`, every test of the four types and the doctor all pass.
- `mise run rune-corpus` is unchanged. That shows the new icon glyph field and the turned-text reporting leave the eight native graph types as they were.

| Test | Inputs | Gate |
|---|---|---|
| `architecture_front` | 15,766 | |
| `architecture_layout` | 14,746 grid placements | 9 of 9 placement rules |
| `architecture_parity` | 23,741 | 33 paths |
| `cynefin_front` | 9,545 | |
| `cynefin_parity` | 10,238 | 21 paths |
| `wardley_front` | 10,030 | |
| `wardley_parity` | 14,464 | 38 paths |
| `xychart_front` | 14,256 | |
| `xychart_ticks` | about 72,000 tick computations, compared as raw bits | |
| `xychart_parity` | 20,423 | 34 paths |

All are 0 different, except one `log10` result (below).

- Cost: 1,983 TS lines (architecture and cynefin) and 2,263 TS lines (wardley and xychart), one run each.

**Decisions.**
- **xychart `log10`, accepted.** V8's fdlibm `log10` is ported into `diagrams/xychart/ticks.rs`, and it is 1 ulp off Node on 1 of 24,432 inputs. Only the integer part reaches the output, so no SVG differs. The `libm` crate's version was off on 49. A Node bump could move it. It is outside `js.rs`, so the Kani skill does not cover it.
- **Icon glyphs.** Pele's five built-in architecture icons are a named enum (`scene/glyph.rs`). `Geometry::Icon` carries the glyph. The writer holds Pele's path spelling, which the Scene's path type cannot reproduce.
- **Turned text.** `Transform::TranslateRotate` is new. The walk reports a drawing with its own rotate inside a turned group. The Rune lowering applies rotation only, so xychart's translate+rotate must also be lowered before xychart's gate opens.

**Merges.** Duplicate additions were kept once: `PathStep::VerticalBy` with its writer and walk arms (railroad and wardley), and `ShapeElement::cap` (cynefin and wardley). In `svg.rs`, railroad's `write_text(scene, …)` and architecture's icon glyph were combined in one arm.

## 2026-11-16 Wave 2b complete: venn and quadrant merged; trig stays on libm

**Result (measured, rerun by the parent after the merge).**
- Corpus: venn 8 of 8 and quadrant 3 of 3, with every earlier type unchanged. Total `522 407 0 115 0`, from 21 types.
- `venn_front` 24,678 inputs. `venn_layout` covers 3,137 diagrams and 20,000 random outlines bit for bit. `venn_parity` 31,575 renders, 34-path gate.
- `quadrant_front` 14,134 inputs. `quadrant_parity` 18,655 renders, 35-path gate.
- `mise run check`, the doctor and `mise run rune-corpus` all pass, and the corpus is unchanged.

**Decision: trig stays on libm (inferred, not measured).**
- **What reaches the SVG.** Venn's layout calls `acos` and `atan2` as well as `sin` and `cos`. libm is one ulp off the pinned Node on about 1 input in 500 to 900. That reaches the SVG in about 1 random venn diagram in 1,500, as a flipped large-arc flag or a 0.7 px drift in an unsettled 60-set layout.
- **Everything else is exact.** Replaying the layout with Node's own trig answers makes every one bit-equal, so all the other arithmetic is Pele's.
- **Why the pinned Node differs.** Its arm64 build contracts multiply-adds inside V8's fdlibm (recorded with `js::sin`).
- **What follows.** Pele's own output for these inputs most likely differs between Apple Silicon and Intel machines, and libm, plain fdlibm, likely matches Intel Node. Porting fdlibm with arm64's fused operations would tie parity to one CPU family.
- **How to measure it.** Run the oracle under an x86-64 Node, through Rosetta or a container. Neither is running on this machine. Offered to the user.
- **Until then.** `venn_parity` lets through only differences it proves are libm's last bit, by replaying them with Node's answers, and prints each one.

**Merge.** Wardley/xychart and venn/quadrant each added `Transform::TranslateRotate`, under different field names. One shape (`translate: [Number; 2]`) and one builder were kept. Combined as they stood, the walk would have applied the translation twice. Each transform now moves the origin once. All four types' walk tests pass, along with xychart's break test of the turn centre.

**Oracle client.** `pele-oracle` turns on serde_json's `float_roundtrip`. The default parser had read a 7.6e150 height one ulp off.

## 2026-11-16 All 33 types ported: corpus 522 of 522

**Result (measured, rerun by the parent after the final merge at `5066616`).** `pele parity` reads `522 522 0 0 0`. Every type of Pele 1.13.0 is ported, and every doc source is byte-identical to Pele. `mise run check`, the doctor, `pele-core`'s type tests, the Rune view tests and `mise run rune-corpus` pass.

**Wave 2c merges** (each run by one worker, figures from the workers and rerun by the parent).

| Unit | Types | Front inputs | Render inputs | Notes |
|---|---|---|---|---|
| evtreerad | eventmodeling, treeview, radar | about 28,700 | about 44,300 | Coverage gates of 39, 30 and 27 paths. |
| gitsankpack | git, sankey, packet | about 43,500 | about 57,800 | `sankey_layout` checks 27,406 calls. Coverage gates of 32, 17 and 14 paths. |
| kanmindish | kanban, mindmap, ishikawa | about 51,700 | about 75,900 | Coverage gates of 35, 32 and 24 paths. |
| maptimejour | treemap, timeline, journey | about 39,800 | about 54,200 | `treemap_layout` checks 482,542 boxes bit for bit, and 22,655 formatted values byte for byte. |

**Merge decisions.**
- **One `IconGlyph`.** `scene/glyph.rs` holds architecture's five built-in icons and treeview's folder and file. Each glyph carries its stroke colour, `var(--_fg)` or `var(--_m)`.
- **One "tooltip inside a shape" flag**, `titled`, shared by sankey's links and journey's actor dots. The walk now wraps any such shape in a group carrying its tooltip.
- **Two decoded flags.** Sankey's `decoded` (label and tooltip) and treemap's `tooltip_decoded` (tooltip only) are kept apart, because one flag would change how treemap escapes its `data-id`. `EntityRecord::tooltip_text` reads either.
- Text forms `Painted` (railroad) and `Tailed` (sankey) both stay. The walk gives each its own run fills.
- **The "not ported" path is unreachable at Pele 1.13.0.** The catalog's `_ => NotPorted` arms stay, with `allow(unreachable_patterns)`, for a type a later pin adds. `tests/fixtures/not-ported.mmd` is removed. The Rune fallback tests now use sequence, which is ported but not lowered (`NotLowered(Kind(Sequence))`).

**Follow-ups, not done.**
- **Padding.** `LayoutOptions.padding_given` sits beside `padding: f64`, and the two must agree. Folding them into `Option<f64>` touches 77 call sites. Deferred, because the current form is correct. Done: `padding: Option<f64>`, in "2026-11-18 Follow-ups".
- **Number helpers.** Treemap's `toExponential` and `toPrecision`, and xychart's fdlibm `log10`, live in their types, not in `js.rs`. They are checked against the oracle, but not proved with Kani.
- **Walk tests.** C4, git, sankey and packet have no walk test of Scene keys yet. Add them before their Rune gates open. Done: `c4_walk`, `git_walk`, `sankey_walk` and `packet_walk`, in "2026-11-18 Follow-ups".
- **Rune gates.** The Rune view draws pie and the eight graph types natively. The 24 newer types fall back to rune-mermaid. The known lowering gaps are:
  - translate+rotate groups (xychart, quadrant);
  - gradients and shape tooltips (sankey);
  - glyphs in icon boxes;
  - math labels.
- **Trig.** The Intel Node check on trig is offered to the user (see the venn entry).

## 2026-11-16 Every type drawn natively in Rune

**Decision.** The Rune view draws all 33 types natively. `lower::NATIVE` lists every kind, and a test fails if a later pin adds one that is not in it. The fallback to rune-mermaid remains for a source whose Scene the lowering refuses (`Style`, `Image`, `CrossingFill`, `Gradient`).

**Why it was cheap.** A probe opened both gates for the 24 newer types and ran the corpus side by side. All 268 sources lowered with no `Unsupported`. 16 types already passed. The other 8 failed for four shared causes, not per type. So one unit opened the 16, and four units fixed one cause each, all in parallel from `ea0c0a6`.

| Unit | Types | Cause and fix |
|---|---|---|
| rune-open16 | 16 types | None needed. `ViewOptions::now` pins gantt's today marker. 1x overs classified by `tests/hairlines.rs`. |
| rune-icons | architecture, treeview | Built-in glyphs were not drawn. `IconGlyph::drawn_in` gives the path the SVG draws. |
| rune-turn | xychart, quadrant | A group that moved and turned used only the turn. `lower::group_map` composes move then turn, as SVG does, and `text_boxes` uses it too. |
| rune-theme | git, radar, timeline | The view was right. The test helper `as_a_page_reads` took every `var()` fallback, not the root's declared value. |
| rune-gradient | sankey | The corpus has no gradients (they need an `idPrefix`). The patches were polygon fills: collinear corners, independent flattening of shared edges, and per-triangle softening. Gradients lower to bands of one colour. |

**Decision: source theme colours.** With `Palette::pele()`, a source's `themeVariables` series colours win, as in Pele. With any other palette, the app's palette wins. `Palette` carries the choice as `ThemeSeries`. Listed in `DIVERGENCES.md`.

**Merge decisions.**
- The fallback tests use `tests/fixtures/rune/unlowered.mmd` (an unread style). `closed.mmd` is removed: no kind stays closed.
- A shape's tooltip is found in a row of 16 point boxes along it, so one tooltip may be carried by several elements. Tests check the distinct texts.
- The probe's "source theme colours" cause was an inference, and it was wrong. Two workers measured the real cause independently.

**Open, then decided.** Whether the view should draw sankey gradients without an `idPrefix`. Decided in the next entry.

## 2026-11-16 A view lays out under Pele's default id prefix

**Decision.** When the app gives no `ViewOptions::id_prefix`, the view lays out under `pele`. A sankey's links are then gradients by default, as Pele draws them in a page. A prefix the app gives still decides, so one Pele cannot name an id with, such as `two words`, still gives links of one colour, as in Pele.

**Why.**
- Pele's own default is `linkColor: 'gradient'` (`sankey/render.ts:78`). It turns gradients off without a prefix only because two SVGs in one page would share a gradient's id, and Chrome then paints a hidden or differently themed copy's gradient (`render.ts:201`). A view declares no ids, so that cause does not apply.
- Pele's `mount` always gives a prefix (`mount.ts:141`), so in a page Pele's links are gradients.
- `id_prefix` was documented as naming ids only, yet it changed how a sankey looked. Now it does so only where Pele's would too.
- `pele` is the prefix Pele seeds a cynefin's boundaries from when it has none (`cynefin/render.ts:72`), so cynefin is drawn as before. `mount`'s own prefix, `pele-<copy>-<n>`, would change a cynefin's boundaries with the order of mounting.

**Not changed.** `pele parity` and the SVG writer, which draw as `render` does: no prefix, links of one colour. The side by side now draws Pele's SVG under `idPrefix` `pele` for every type, so sankey's gradients are compared by default. `tests/lowering.rs` checks the default, an app's prefix and a prefix Pele rejects; `break-lowering.sh` removes the default and that test fails.

## 2026-11-17 Streamed sources and fitting the width

**Why.** A main use is diagrams a language model writes, streamed into a chat, on phones and on wide screens. A spike measured (on branch `spike-fit`): about a third of the character prefixes of the 32 fixtures draw; after a first drawing the view returned an error 316 times, for up to 177 characters in a row. Rune's `El::scale` keeps the unscaled size in layout and puts snapped edges between device pixels.

**Decisions.**
- **Keep the last good drawing** while a source fails (`rune-keep`). By default as Pele's `mount` shows it: dimmed to 0.4 with the error over its bottom edge (`KeptLook::DimmedWithError`). Opt-in, for streaming: unchanged, with the error read from `Diagram::error` (`KeptLook::Unchanged`). Chosen by the user: Pele's look by default, the quiet one opt-in. Option errors stay errors.
- **Zoom in the lowering** (`ViewOptions::zoom`, `rune-fit`): geometry is multiplied before it is flattened, snapped and cut, so edges land on the final pixel grid. A zoom change lowers again and lays nothing out. Every corpus source of the 32 graph types passes at 0.75 and 0.5.
- **Fit the width** (`ViewOptions::fit`, `smallest_text`): natural size if it fits; else laid out narrower where the type can narrow; else zoomed down to 11 point text; else scrolls. Never enlarges. Re-lays out at steps a twelfth of an octave apart, so a window drag does not lay out every frame.
- **A kept drawing follows the fit** (merge): the last good source is drawn again at the new width or zoom, as Pele's `mount` draws its last parsed text again. The failing source is never laid out. Both looks sit in the one frame whose width the fit reads.

**Findings that corrected the spike.** Its `El::scale` numbers were inflated by a mask bug in the spike itself, and its "17 px text drift" was node borders next to text, not text moving (measured by `rune-fit`).

**Open.** Pele turns a left-to-right flowchart only under 640 points, so a fitted one between about 640 and 780 points shrinks to the smallest text and scrolls where turned it would fit. Kept as Pele does pending the user's choice.

**Settled** (`fix-view`, chosen by the user). In a fit, a diagram that runs across also turns to run down wherever across, at the smallest text, would scroll. Outside a fit, Pele's 640 points still decide. The fit lays out its step with pele-core's `LayoutOptions::turn_wider_than` set to the step over the least zoom, so it still lays out once a step, and `pele parity` never sets it. Measured: the across fixture turns at 797 points and scrolls only at 260 points and under. The rule is Pele's `turnToFit`, which 14 types share, so it turns them all alike: the packet fixture is laid out 678 points wide at 720, where across it scrolled between 710 and 640 points. `DIVERGENCES.md` has the row.

**The smallest text, as drawn** (`fix-view`). The smallest text size holds for the text drawn. A narrower layout can set plain text smaller itself, as treemap does in small cells (16 to 9 points at 240 points of room), so a fit takes a narrower layout only where its plain text can still be drawn at the smallest size at a zoom of at most 1. Elsewhere it draws the diagram's own layout at the least zoom, and scrolls. Measured over every fixture from 900 points to 200: no plain text under 11 points; treemap scrolls from 270.

## 2026-11-17 Adversarial review fixes

An adversarial review of `main` at `e44a742` found 16 defects. Four units fixed them on `fix-gates`, `fix-consumer`, `fix-view` and `fix-draw`, merged on `merge-review`. One line per finding: the fix, then the test that fails without it. Each test also has a break, in `.agents/skills/verify-rune-pele/scripts/break-gates.sh` or `crates/rune-pele/break-lowering.sh`.

- **Parity passed on an incomplete corpus and kept stale evidence** (P1). The corpus is every `<type>-docs.json` in the Pele pin's git tree, and each must be a JSON array of sources (exit 66). A source not ported, a Pele rejection beyond `--expect-pele-errors` (0 by default), or a run that compares nothing fails the gate (exit 1). Each run empties its `--out` first, only where `pele parity` wrote it. An oracle crash exits 70. Tests: the corpus, gate and `--out` tests of `crates/pele-cli/tests/cli.rs`, such as `a_missing_corpus_file_fails_the_run_and_is_named`, and `an_oracle_that_crashes_is_an_error_with_status_70_in_render_and_parity`.
- **The oracle could run another Node than the doctor checked** (P2). Both run what `mise which node` names, and the oracle refuses a version other than `mise.toml`'s, naming both (exit 70). Tests: `the_oracle_runs_the_node_mise_names_not_the_first_on_path`, `the_oracle_refuses_a_node_other_than_the_pinned_version_and_names_both`.
- **Unknown options and options without values succeeded** (P2). They exit 64 with the usage text, in `render` and `parity`. Test: `unknown_options_and_options_without_values_are_usage_errors`.
- **`parity --out` at a file panicked** (P2). It is an I/O error, exit 73. Test: `an_out_path_that_cannot_be_a_folder_is_an_io_error`.
- **The output-limit error lost Mermaid's alias** (P3). `ParityScene` carries the type and alias detection found to the SVG writer. The oracle's new `render_error` op answers Pele's whole error, and the test compares every field: message, code, type, alias, line, column and snippet. Test: `output_limit`'s `the_output_limit_error_is_pele_s_in_every_field_for_every_alias`, 523 inputs and 37 aliases.
- **A consumer could not resolve rune-pele** (P1). `examples/consumer` is an app in a workspace of its own with Rune's four patches, the README's "Using rune-pele in your app" says what an app needs and why, and `mise run check` runs `mise run consumer`. Tests: `examples/consumer/tests/app.rs`.
- **A fallback silently lost links and zoom** (P2, decision 1). `Diagram::engine()` says who drew the drawing shown, kept or not, and `Engine::has_links`, `has_tooltips` and `zooms` what it can do. The `Diagram`, `ViewOptions` and `DIVERGENCES.md` docs say rune-mermaid draws no links and ignores the zoom. Test: `engine_view`'s `the_engine_says_what_the_drawing_shown_can_do`.
- **A public doc linked to a private item** (P3). It says the band limit in words, and `mise run check` denies rustdoc warnings. The merge extends that to pele-core and fixes its four: `IconGlyph::stroke`, `Trig`, `layout_sankey` and `Number`. Break: `doc-private-link`.
- **A replacement `Diagram` showed the old drawing and fired the old link** (P1). Layouts and lowerings are numbered across the process, so the paint memo never takes one diagram's drawing for another's. Test: `view_identity`'s `a_diagram_that_replaces_another_under_its_id_draws_and_links_its_own_source`.
- **The fit's frame key could collide** (P2). The frame is keyed with the id itself, and Rune reads the first element so keyed, which is the frame. Test: `a_fitted_view_reads_its_own_frame_whatever_its_neighbour_is_keyed`. Since the merge, the frame, the dimmed look's wrapper and the drawing are all keyed with the id. `accessibility`'s `a_fitted_view_is_one_named_figure_whether_drawn_or_kept` holds the frame first and as wide as the box, and one named figure, in both kept looks.
- **Pele's metrics moved a gantt chart's today** (P3). With no `now`, a view takes the system clock under Pele's metrics too. Tests held to Pele's SVG pass `PELE_CORPUS_NOW`. Test: `pele_metrics_measure_text_and_leave_the_clock_to_the_app`.
- **Fit drew text below the smallest size** (P2). The floor holds for the text drawn: a narrower layout that sets plain text smaller is used only where that text can still reach the smallest size. Test: `fit_text`'s `fitted_text_keeps_to_the_smallest_size_at_every_width`.
- **A zero-radius point drew its stroke** (P2). A rectangle, circle or ellipse with a side or radius that is not positive draws nothing, as SVG renders nothing. Tests: `zero_radius`.
- **An invalid colour switched renderers** (P2). A fill or stroke that is no colour, such as `fill:#0` or `fill:blu`, is dropped, and the element keeps the colour it has without it. Tests: `invalid_colour`, with every prefix of a colour typed into the flowchart fixture drawn natively.
- **Runs of spaces kept their width** (P3, decision 3). Text collapses spaces as SVG's default `xml:space` does, and keeps them where Pele writes `xml:space="preserve"`. Tests: `whitespace`.
- **The native tree dropped the diagram's name and links skipped the keyboard** (P2). The drawing is a figure named by `accTitle` and described by `accDescr`. Each link is a Tab stop named by its label, and Return follows it as a press does. Tests: `accessibility`.

**Decisions.**
- **The turning rule covers all 14 types, by the user's choice.** In a fit, a diagram that runs across turns wherever across, at the smallest text, would scroll (`LayoutOptions::turn_wider_than`). Pele's `turnToFit` is shared by flowchart, state, class, er, requirement, usecase, packet, timeline, git, journey, agentflow, ishikawa, mindmap and eventmodeling, so all 14 turn alike. Test: `a_fitted_flowchart_that_runs_across_turns_where_across_would_scroll`.
- **An invalid colour keeps the attribute's colour, as a browser does.** A page's CSS drops a declaration it cannot parse. resvg instead paints such a fill black and draws no such stroke. The browser is Pele's host, and resvg is only the reference the tests draw Pele's SVG with, so the view follows the browser. The unit brief's "as resvg does" was wrong. `invalid_colour` compares with resvg's drawing of Pele's SVG with the declaration taken out, and `DIVERGENCES.md` notes the choice.

**Open, then done.** A half-typed length, such as `stroke-width:2p`, is a style the walk does not read, so that prefix still falls back to rune-mermaid while it streams. `keep_view`'s flowchart row counts it: 1 prefix drawn by rune-mermaid. Done: the walk drops it as a page does, in "2026-11-18 Follow-ups".

## 2026-11-18 Pele on Intel measured: the same drawings, other trig

**Why.** The venn entry of 2026-11-16 inferred that Pele on an Intel Mac matches libm, and kept libm. Nobody had measured it. A user who runs Pele on an Intel Mac makes the SVG this port must match.

**How.** `mise run intel-node` installs Node 26.13.1 for x64 through mise (`MISE_ARCH=x64`, its own data folder under `out/intel-node`). mise checks the tarball against nodejs.org's `SHASUMS256.txt` and that file's signature. The script checks the tarball against `SHASUMS256.txt` again. Rosetta 2 runs it. `MISE_NODE_VERSION=path:<that folder>` makes the oracle and the doctor run it, with no change to the oracle. The oracle still refuses any other version (exit 70), `pele parity` prints the path it ran, and the doctor now prints the Node's architecture and names the variable when it is set. `mise run intel-trig [-- <seed> <count>]` runs it all and writes `out/verify/intel-trig/`.

**The maths (measured, seed `0x1e7e1`, 10,000,000 random inputs a function).** Each cell counts answers that differ in any bit. Every difference is one ulp, except log10's random ones, which are at most two.

| JS | port | x64 vs port | arm64 vs port | x64 vs arm64 |
| --- | --- | --- | --- | --- |
| `Math.sin` | `js::sin` (libm) | 73,939 | 117,165 | 48,228 |
| `Math.cos` | `js::cos` (libm) | 78,052 | 113,466 | 39,946 |
| `Math.acos` | `libm::acos` (venn) | 0 | 19,458 | 19,458 |
| `Math.asin` | `libm::asin` (block) | 0 | 46,882 | 46,882 |
| `Math.atan2` | `libm::atan2`, the 8,003,040 with y/x within ±2^60 | 0 | 2,278 | 2,278 |
| `Math.atan2` | `libm::atan2`, the other 1,996,960 | 22,262 | 22,262 | 0 |
| `Math.pow` | `js::pow` (libm) | 519,564 | 526,908 | 8,016 |
| `Math.log10` | `libm::log10` (gantt, treemap) | 201,150 | 200,405 | 11,348 |
| `Math.log10` | `xychart_ticks::log10` | 0 | 11,348 | 11,348 |
| `Math.log2` | `libm::log2` (packet) | 0 | 0 | 0 |
| `Math.hypot` | `layout::num::hypot` | 0 | 0 | 0 |
| `Math.sqrt` | `f64::sqrt` | 0 | 0 | 0 |

The edge inputs, grouped by the call that takes them, are in `math.md`. Each Node run with `--jitless` gives its own answers on every edge input, so the interpreter and the compilers agree.

**Conclusions (measured).**
- **Intel Pele matches libm** for `acos`, `asin`, `log2`, and for `atan2` wherever y/x is within ±2^60. It also matches the port's own `hypot` and xychart's `log10`. arm64 Pele matches libm for `log2`, `hypot` and `sqrt` only.
- **Intel Pele matches neither libm nor arm64 Pele** for `sin` and `cos`. The inference of 2026-11-16 held for `acos`, `asin` and `atan2`, and was wrong for `sin` and `cos`. The libm crate follows FreeBSD's later msun (its sources say so), and V8 follows the older fdlibm, so they round apart even without fused multiply-adds. arm64's fused operations add more. xychart's port of V8's `log10` matching x64 Node on all 10,108,438 inputs shows the same thing from the other side.
- **`pow` is the platform's.** V8's `--use-std-math-pow` is on by default, and `math::pow` calls `_pow`, with fdlibm's behind the flag (read from both binaries). On every power Pele's own code takes, Intel and arm64 Pele agree: sRGB channels to 2.4, 0.99 to a pass, 10, 2 and 0.5 to a whole power, and squares. On random powers they part in 8 of 10,000. libm's `pow` is off from both on 5% of random powers, on 0.99^19 and 100 of 0.99^0..1000, and on 10^k for 64 of k = -330..330, among them -5, -11 and 29. Sankey takes 0.99^0..5, which all agree. xychart takes 10^k through `ten_to`, which agrees on all 661. Gantt takes `js::pow(10, k)`, which agrees for k = -4..28.
- **Pele's `ink` in parity mode.** `scene.rs` raises sRGB channels to 2.4 with `js::pow`, which is off from both Nodes on 34 of the 256 channel values of a `#rrggbb` colour. It picks Pele's ink for all 16,777,216 such colours on both Nodes. The closest luminance to the turn between the two inks is 3.5e-8 away, and an ulp moves it by about 1e-16.
- **`log10`.** libm's `log10` is off from both Nodes on 2% of inputs. Within three ulps of every power of ten it is off on 4, and none of them crosses a whole number, so gantt's `floor` holds.

**What reaches a drawing (measured).**
- `pele parity` with x64 Node as the oracle reads `522 522 0 0 0`.
- Pele's own answers are byte-identical on x64 and arm64 Node for 379,778 of 379,779 sources: the corpus's 522 and every source of every fixture. The one that differs is `venn_layout`'s unsettled relaxation, which the port draws like neither.
- Random venn diagrams, made by the tests: 1,880 of 2,000 geometries are bit-equal to Pele on x64, against 1,848 on arm64. 1 of 3,000 drawings differs only by libm's last bit on x64, against 2 on arm64. `acos` and `atan2` are bit-equal to x64 Node on all of venn's 600,009 and 600,020 inputs. In the oracle test, `sin` is bit-equal on 302,127 of 304,334 on x64, against 300,834 on arm64.

**Caveats.**
- Rosetta 2 stands in for an Intel Mac (inferred). V8's fdlibm is plain SSE2 arithmetic, which Rosetta computes exactly.
- Rosetta reports SSE4.2 and no AVX or FMA, so Apple's x86 `pow` ran the path for such a CPU. An Intel Mac with FMA may take another path in Apple's libm (guess).
- Pele on Linux or Windows calls another `pow` (not measured).

**Decision.** None yet: trig stays on libm. These are proposed, not made:
1. Port V8's fdlibm `sin` and `cos` into `js.rs`, as `xychart_ticks::log10` ports its `log10`. Expected (inferred from `log10`): Intel Pele bit for bit, and arm64 Pele apart only where its fused operations round (0.48% of random `sin` inputs, against 1.17% now). This goes through the prove-js-numerics skill.
2. Move xychart's `log10` and `ten_to` into `js.rs`, and use them in gantt and treemap.
3. `js::pow(±1, ±∞)` and `js::pow(1, NaN)` return 1, where `Math.pow` returns NaN. No call passes them (read from the port's eleven calls). One guard fixes it. Done, in "2026-11-18 Follow-ups".
- `js::sin`'s doc gives fused multiply-adds as the reason for the gap. That is only part of it. Done, in "2026-11-18 Follow-ups".

**Checks.** `mise run intel-trig` fails if a port marked equal to x64 Node (`same_on_x64`) stops being so, if the port picks another ink than Pele for a `#rrggbb` colour, or if Pele draws a corpus or fixture source apart on the two Nodes. The recipe and its breaks are in `features/numerics.md`.

## 2026-11-18 Follow-ups

Five units cleared the follow-ups the review round left, each from `main` at `d254b06`, merged on `merge-follow` in this order. One paragraph per unit and its tests, then what the merge added.

**One padding option** (`core-cleanup`). `LayoutOptions::padding` is `Option<f64>`, and `padding_given` is gone, so the two can no longer disagree. `None` is Pele's left-out `padding`: each type takes its own default, 8 pixels or a treemap's `diagramPadding`, and `padding_or_default` is Pele's `options.padding ?? 8` in one place. Treemap's `toExponential` and `toPrecision` moved into `js.rs` as `to_exponential` and `to_precision`. Their one rounding step, `round_significant`, is proved with Kani for every run of 1 to 21 digits; their digits and layout are held to Node through the oracle's new `exponential` and `precision` ops, 215,776 and 210,952 inputs with 0 different. Tests: `treemap_parity` (a `diagramPadding` of 50 with no padding and with `padding: 0`), the oracle tests and the Kani harness. Breaks: 7 in `break-numerics.sh`, and two padding breaks in `break-maptimejour.mjs`.

**Walk tests** (`walk-tests`). `c4_walk`, `git_walk`, `sankey_walk` and `packet_walk` read each type's corpus, Rune fixture and harvest at the default width and at 320 points: every drawing has a key of its own, the Scene's entities are exactly the model's, and a key survives an edit elsewhere still drawing its own text. They found one defect: a packet field split across rows was keyed by each piece's range, so its key changed with the width a fit lays it out at. It is now keyed by the field's range, the further pieces ranked after it, and Pele's `data-id` is unchanged. Breaks: 8 in the new `break-walks.sh`.

**Pele on Intel** (`intel-trig`). The entry above has the measurements. Intel Pele draws the corpus and every fixture as arm64 Pele does, and `pele parity` with x64 Node reads `522 522 0 0 0`, but their `sin`, `cos` and `pow` part in the last bit, and libm is off from both on `sin` and `cos`. Test: `mise run intel-trig`, with the doctor naming the Node's architecture.

**A value a page drops** (`fix-lengths`). `scene/css.rs` holds the grammar of each of the 19 properties the walk meets, as Chrome and Firefox parse them, and `no_paint` is gone. A declaration whose value its property does not take is dropped, as a page drops it, and the element keeps what it had: `stroke-width:2p`, `opacity:0.` or `font-weight:bol` while a source streams in is drawn natively. A value only a page can resolve, such as `calc()`, `var()` or a length in `em` or `%`, is applied by a page and not drawn by the walk, so it falls back to rune-mermaid: dropping it would draw something other than Pele's page without saying so. Where browsers disagree, a value counts as applied if one of them applies it, so a number with no unit is a length to every property. A value that leaves a function or a string open takes in the declarations after it, as on a page. The tests found the `rx` defect: the walk asked whether a property ended with a value, not whether each declaration was read, so a theme's `rx:var(--_r)` before a source's `rx:auto` or `rx:5em` drew the theme's corner where a page draws the source's. `Meaning` now records which declarations it read. Tests: the new `invalid_value`, which holds each value type and every half-typed prefix of the corpus's values to resvg's drawing of Pele's SVG without what a page drops; `keep_view`, which streams all 34 fixtures and holds each fixture drawn natively whole to no prefix drawn by rune-mermaid; and `invalid_colour`. Breaks: 6 new in `break-lowering.sh`, and `invalid-fill-unread` re-pointed at the paint grammar.

**The app's icons** (`icon-hook`). `ViewOptions::icons` takes a plain `fn(&str) -> Option<Icon>`, as `view_with` takes a plain `fn` handler: a function pointer compares by address, so the paint memo keys on it, and another function repaints with no layout. An app that loads its icons keeps them where the function reads them, such as a `static`. An `Icon` is SVG path data on a view box, filled or stroked, as an Iconify icon is, read into the Scene's own path commands and lowered at paint time on the device pixel grid like every other shape. `IconPaint::Current` is the palette's foreground, which is what `currentColor` is in Pele's SVG, and `IconPaint::Fixed` is an icon's own colour, drawn as it is in every palette. It fills the boxes of architecture's services and groups, treeview's rows, flowchart's icon shapes and usecase's icon actors. It cannot fill a label's `fa:` icon, a mindmap node's or a kanban card's, because Pele makes room for those only for a host that has the icon and the view lays out as for a host with none; a fill whose contours cross, which the lowering does not cut, so that icon is not drawn; and it draws whole an icon that reaches outside its box, where SVG cuts it. Pele's SVG and parity are unchanged, and `DIVERGENCES.md` has the row. Tests: the new `icons`, beside Pele's SVG with the same icons as markup in its boxes, at 2×, at zooms 0.75 and 0.5, in a fit, and on a repaint. Breaks: 12 in `break-lowering.sh`, and `group-faded-piecewise` re-pointed.

**The merge** (`merge-follow`). Only `rune-view.md` conflicted, and both sides are kept. `break-lowering.sh` holds 63 breaks, with `icon-hook`'s twelve moved before `break_doc` so that `doc-private-link` stays the last line. These are new:
- **Git keys.** A commit with no id was keyed by the id Pele makes up, `{seq}-{hash}`, which counts every commit before it. So an insertion anywhere earlier re-keyed it, and an old key could name another commit. A commit is now named by its own id, or with none by its branch in quotes, and ranked among the commits named alike before it: the third commit with no id on `develop` is `node:"develop"#2`, and an edge names it `"develop"#2`. An insertion re-keys only the later commits with no id on its own branch. An id can be written with quotes too, so commits and edges are ranked by name, and such an id still keys apart. `Commit::own_id` replaces `custom_id`, and the front end reads a merge's `customId` from it. Pele's `data-id` and the SVG are unchanged: `git_front`, `git_parity` and the corpus read 0 different. `git_walk` drops the cherry-pick exemption, which the new key covers, and adds an insertion on each of three branches that moves no key of the others. Breaks: `git-unnamed-keyed-by-made-up-id` and `git-unnamed-ranked-across-branches` in `break-walks.sh`, and `git-commit-keyed-by-place` re-pointed.
- **`js::pow` and NaN.** `js::pow` answers NaN for any base to a NaN power and for ±1 to ±∞, as `Math.pow` does, where C's `pow` answers 1. No call in the port passes them, so no drawing changes. `pow_matches_math_pow_where_it_names_the_answer` holds 189 pairs of special and exact powers to Node's bits; without the guard 5 differ. Breaks: `pow-of-one-to-nan-is-one` and `pow-of-one-to-infinity-is-one` in `break-numerics.sh`. `js::sin`'s doc names both reasons it parts from V8: msun against fdlibm, and arm64's fused multiply-adds.
- **Interactions** (measured). A flowchart whose icon shapes carry `stroke-width:2p`, `fill:blu` and `opacity:0.` draws natively with the app's icons, each as Pele's SVG draws it without those values, pixel for pixel the drawing without them (`icons`'s `a_value_a_page_drops_leaves_the_app_s_icons_drawn_natively`). `keep_view` still reads 0 prefixes drawn by rune-mermaid on every fixture drawn natively, and `unlowered`'s two. `treemap_parity` holds a `diagramPadding` with no padding and with `padding: 0`. `mise run rune-corpus`'s ten reports are byte-identical to a fresh run of `d254b06`.

**Open.**
- ~~Port V8's fdlibm `sin` and `cos` into `js.rs`, through the prove-js-numerics skill (proposal 1 of the Intel entry).~~ Done: "2026-11-18 V8's trig and log10".
- ~~Move xychart's `log10` and `ten_to` into `js.rs`, and use them in gantt and treemap (proposal 2).~~ Done: the same entry.
- ~~A thick dashed stroke on a rounded box: `fix-lengths` measured 12.9 unexplained pixels in 10,000 against resvg, where the limit is 5. Not explained yet.~~ Explained and fixed, except a corner tighter than half the stroke: "2026-11-18 Dashes round corners".

## 2026-11-18 V8's trig and log10

**Why.** Proposals 1 and 2 of "2026-11-18 Pele on Intel measured", the first two open items of "2026-11-18 Follow-ups": `js::sin` and `js::cos` called the libm crate, off from Node on both machines, and gantt and treemap called `libm::log10`, off from both on 2% of inputs.

**What changed** (`fdlibm-trig`).
- `js::sin` and `js::cos` are V8's fdlibm: `sin`, `cos`, `__ieee754_rem_pio2`, `__kernel_rem_pio2`, `__kernel_sin` and `__kernel_cos` of `src/base/ieee754.cc` in V8 14.6.202.34, the V8 of Node 26.13.1, line for line in plain double arithmetic, in a private `fdlibm` module of `js.rs`. Node builds V8 without `V8_USE_LIBM_TRIG_FUNCTIONS`, so its `Math.sin` is this code and not glibc's.
- xychart's port of V8's `log10`, with its `log`, and `ten_to` moved to `js::log10` and `js::ten_to`. Xychart, gantt's tick step and treemap's crowding read them. Gantt's step also takes `js::ten_to` where it took `js::pow(10, k)`.

**Parity (measured).** `pele parity` on a fresh `--out` after each step reads `522 522 0 0 0`, and every one of the 522 `ours.svg` is byte-identical to a run before the change. With x64 Node as the oracle it also reads `522 522 0 0 0`.

**The maths (measured, `mise run intel-trig`, seed `0x1e7e1`, 10,000,000 random inputs a function).** Each cell counts answers that differ in any bit. The libm rows are the port before, measured in the same run.

| JS | port | x64 vs port | arm64 vs port | x64 vs arm64 |
| --- | --- | --- | --- | --- |
| `Math.sin` | `js::sin` (V8's fdlibm) | 0 | 48,228 | 48,228 |
| `Math.sin` | `libm::sin`, before | 73,939 | 117,165 | 48,228 |
| `Math.cos` | `js::cos` (V8's fdlibm) | 0 | 39,946 | 39,946 |
| `Math.cos` | `libm::cos`, before | 78,052 | 113,466 | 39,946 |
| `Math.log10` | `js::log10` (xychart, gantt, treemap) | 0 | 11,348 | 11,348 |
| `Math.log10` | `libm::log10`, gantt and treemap before | 201,150 | 200,405 | 11,348 |
| `Math.pow` | `js::ten_to`, 10^k for k = -330..330 | 0 | 0 | 0 |

The other rows of the Intel entry are unchanged. `js::sin`, `js::cos` and `js::log10` are 0 apart from x64 Node on every edge group too: the groups of `tests/js/fdlibm_edges.rs`, which add the kernels' and reductions' bounds, n·π/2, 26,340 huge arguments, subnormals, powers of two and the bounds of `log`'s series to the Intel entry's sets. intel-trig now marks them `same_on_x64`, so it fails if that stops being true.

**Fused multiply-adds (measured).** A scratch copy of the port with `f64::mul_add` wherever clang's default `-ffp-contract=on` fuses (a product added or subtracted in the same expression, the left product first) is arm64 Node bit for bit: 0 apart on all 10,000,000 random `sin`, `cos` and `log10` inputs and on every edge group, against 48,228, 39,946 and 11,348 for the plain port. So every arm64 difference comes from fusion. The copy was not kept: a port that fuses would match arm64 Node and part from x64 Node, which ties parity to one CPU family. That choice is open.

**What reaches a drawing (measured).**
- Pele's answers on x64 and arm64 Node are byte-identical for 379,778 of 379,779 sources, as before. The one, `venn_layout`'s unsettled relaxation, the port now draws as Pele on x64 does, where before it drew like neither.
- Random venn diagrams: 1,921 of 2,000 geometries bit-equal to Pele on arm64 (1,848 before), and all 2,000 on x64 (1,880 before). 1 of 3,000 random drawings differs from arm64 Pele only by a last bit (2 before), and none from x64 Pele (1 before).
- `oracle.rs`'s `sin` is bit-equal to the pinned Node on 302,863 of 304,334 angles (300,834 before), and `cos` on 303,117.

**Proofs and tests.**
- Four Kani harnesses, each for every double it names: `sin` and `cos` NaN for NaN and the infinities (6.4 s alone); `sin(x)` is `x` and `cos(x)` is 1 under 2^-27 (6.3 s); both in [-1, 1] up to π/4 (116 s); `log10`'s ECMAScript special values (2.0 s). Their arguments reach no loop, so each unwinds once and Kani's unwinding checks prove it. `mise run prove` passes all 9 harnesses in 195 s.
- Kept out of Kani, each stopped at 9 minutes with three harnesses running at once: `sin` odd with `cos` even, up to π/4 and from there to 2^19·π/2, with CaDiCaL; each of the two alone up to π/4, with Kissat; and both in [-1, 1] from π/4 to 2^19·π/2. `tests/fdlibm_properties.rs` samples them over the edges and a million doubles of any bits, and holds `log10(ten_to(k))` to `k` for every normal power of ten, 1e-307 to 1e308.
- `oracle.rs`'s three `*_where_fdlibm_changes_course` tests hold `sin`, `cos` and `log10` to the pinned Node on the edge groups, through the oracle's new `log10` op: bit for bit where no rounding takes part, within one ulp elsewhere, and the same `floor` everywhere.
- `break-numerics.sh` has 13 new breaks, each caught on its own. A last bit of `sin`'s `S1` passes the arm64 oracle test, whose bit-equal counts only drop (23,781 to 23,169 on the π/4 multiples), and only intel-trig's x64 comparison catches it. `mise run check` does not run that comparison.

**Open.**
- Whether to match arm64 Node with a fused port, or keep x64's (above).
- `mise run check` cannot see a last-bit error in the fdlibm port. `mise run intel-trig` can, in about a minute, but needs Rosetta 2 and a download.

## 2026-11-18 Dashes round corners

**Why.** The last open item of "2026-11-18 Follow-ups": `style A stroke-width:6px,stroke-dasharray:5 5` on a rounded flowchart box differed from resvg at its corners. The source behind the recorded 12.9 was not kept; this one reads 14.59 unexplained pixels in 10,000 at 2× on `de2f97b` (measured), against a limit of 5.

**The cause (measured, one change at a time).** Not the dash's start, which is at `(x + rx, y)` as SVG puts it: the dash ends on the top side are resvg's. Not the corners' arc length, whose phase drift after three corners is 0.12 pt, a quarter pixel. The joins inside a dash: Rune strokes a butt-ended polyline as one box per segment, so a dash bent round a corner left a wedge-shaped notch at each joint of the flattened curve, seen as pale spokes. Finer flattening, with more joints, raised it to 57.39; closing the notches took it to 0.00.

**What changed** (`dash-corners`).
- **Joins.** `stroke.rs` gives each joint inside a butt piece the round join a round stroke has there, as a patch that starts inside both boxes. A joint whose notch covers under a twentieth of a device pixel stays open, so 1× hairlines are unchanged. `stroke()` takes the device pixel in the lines' units in place of a fixed `visible`.
- **Rect radii.** `rx` and `ry` are each clamped to half their own side, as SVG does, not together. A rect with elliptical corners is drawn as a path; `Figure::boxed` answers `None` for it.
- **Dashed circles and ellipses** start at three o'clock, where SVG starts them. Undashed ones keep their start, which only moves how Rune splits the fill.

**Measured.** The brief's source: 14.59 → 0.00 at 2×, and 6.74 at 1×, all on the solid node's corners, the known hairline divergence. A widened set of 60 shapes (widths 1, 2, 6, 12; dash arrays `5 5`, `2 8`, `10 3 2 3`; radii 0, 5, 30; circles; ellipses): every 2× row under the limit but the three radius-5, width-12 boxes, at 14.3 to 16.9 (59 to 184 before). `mise run rune-corpus` changes only c4's rows, lower: its external person heads are dashed circles, and their worst 2× patch falls from 8 px to 0. Tests: the new `dash_corners`, 5 tests. Breaks: 8 in `break-lowering.sh`, and one recipe in `rune-view.md`.

**New divergence.** "A dash round a corner tighter than half its stroke": the normals of the flattened curve cross inside it, and each segment's box reaches past them by up to half a segment. Flattening 5 times finer gives 3.1 to 7.7, 25 times finer 1.5 to 5.6 (measured).

**The merge** (`merge-last`). Both branches merged with no conflict. Venn's `Trig` implementation `Libm` is renamed `Ported`, and its comments name the port, since its `sin` and `cos` are V8's now.

**Open.**
- Flatten a thick stroke's curves by its width, which would close the divergence above and move every thick curved stroke in the corpus.

## 2026-11-20 A public API apps can rely on

**Why.** `v0.1.0` is released, apps pin a tag, and release-plz runs cargo-semver-checks against the last release, so every breaking change from now on costs a version. Breaking changes are cheapest before apps depend on `v0.1`.

**One rule for what is not API** (`use-api`). An item an app should not call stays public only where another crate of the workspace or the tests need it, and is `#[doc(hidden)]` with a doc line that says who it is public for. An item nothing outside its crate uses is private: rune-pele's `bands` and `fill`. The rule is the same for both crates, and chosen over a `testing` feature for three reasons:
- cargo-semver-checks leaves `#[doc(hidden)]` items out of the API it compares. Its default feature selection turns on every feature but `unstable`, `nightly`, `bench`, `no_std` and names starting `_`, `unstable_` or `unstable-` (its `--help`), so a `testing` feature's items would be compared as API, and a change to a probe would cost a version.
- pele-core already hid its probes and front ends so, and one mark covers both reasons an item is public: rune-pele needs it, or the tests do.
- A crate's own integration tests cannot turn on its own feature without a dependency on itself or `required-features`, which skips a test silently when the feature is off (inferred, not tried).

**Hide items, not modules, where a module holds anything an app uses (measured).** `missing_docs` skips every item under a `#[doc(hidden)]` module, also one re-exported at a visible path. With pele-core's `scene` and rune-pele's `text` hidden whole, `FontCatalog::new` had no doc and the doc gate passed. So a module is hidden whole only where nothing in it is for apps (`js`, `common`, `layout`, the probes, `geometry`, `stroke`), and elsewhere each item an app should not call is hidden on its own. The gate then fails on a missing doc of `FontCatalog::new` or of a `LinkRef` field. Break: `doc-missing-on-a-root-re-export`. Items nothing outside its crate calls are `pub(crate)`, three of them only for the crate's own unit tests, and `Affine::is_translation`, which nothing called, is gone.

**What an app relies on.**
- pele-core: `render`, `parity_scene`, `parity_scene_with`, `ParityScene` and `svg::write` to draw SVG; `prepare`, `Prepared::kind`; the options `ParseOptions`, `HostConfig`, `Limits`, `LayoutOptions`, `Typography`, `Font`, `FontFamily`, `FontWeight`, `MonospacePart` and `ResourcePolicy`; the errors `RenderError`, `Diagnostic`, `Location` and `ErrorCode`; `DiagramKind`, `MermaidType`; and `Scene` with `size`, `accessible_title` and `accessible_description`, with `Size`, `LinkRef` and `Role` at the root.
- rune-pele: `Diagram`, `ViewOptions`, `Palette`, `KeptLook`, `Icon`, `IconPaint`, `IconError`, `Event`, `Link`, `Role`, `FontCatalog`, the status and error types, `ViewStats`, `ViewTimes`, `capture` (an app's tests draw its view headless with it, as `examples/consumer` and `mise run consumer-tag` do), `lower::NATIVE`, `lower::Unsupported` and `lower::Census`, and the re-exports `rune` and, new, `pele_core`, which names the types rune-pele's signatures use with no `pele-core` dependency of the app's own.
- Hidden for rune-pele: the Scene's walk and palette types in pele-core's `scene`, `TextMeasurer`, `TextMeasure`, `TextRef`, `Script`, `NativeInputs`, `Prepared::scene`, `ParityScene::scene` and the Scene's walk methods. Hidden for the tests: pele-core's `js`, `common`, `layout`, probes and front ends; rune-pele's `geometry` and `stroke`, all of `text` but `FontCatalog`, the lowering's `Drawing`, `Figure`, `TextRun` and `group_map`, and `Palette::core`.
- `lower::NATIVE` is a `&[DiagramKind]`, so a type joining it changes no type.

**Room to grow.** `#[non_exhaustive]`: pele-core's `LayoutOptions`, `ParseOptions`, `Limits`, `ResourcePolicy`, `Typography`, `Rendered`, `Diagnostic`, `Location`, `LinkRef`, `RenderError`, `DiagramKind`, `MonospacePart` and `Role` (`ErrorCode` already was); rune-pele's `Engine`, `RenderStatus`, `Fallback`, `ViewError`, `Unsupported`, `KeptLook`, `IconPaint`, `IconError`, `ViewStats`, `ViewTimes` and `Census` (`Event` already was). Pele's own options and `Rendered` show what will come: `fontFamily`, `fontFamilyMono` and `links` have no port yet. Each option of pele-core has a builder method named after its field, as `ViewOptions` has: `LayoutOptions::default().max_width(640.0).padding(4.0)`. Left exhaustive: `Font`, `FontFamily` and `FontWeight`, which describe a run completely, `Size` and `Point`, and `capture::Pixels`, an image that the tests build from resvg's.

**Docs.** Both crates warn on `missing_docs`, and the doc gate in `scripts/check.sh` runs rustdoc with `-D warnings -D missing_docs`, which fails on a public item with no doc even without the crates' lint (measured). Each crate's docs open with examples that run as doctests: pele-core renders a flowchart, with options and with a syntax error; rune-pele draws one fitted in an app's view, headless.

**The oldest Rust (measured).** Bisected over released Rust with `mise exec rust@<version> -- cargo check -p <crate> --ignore-rust-version`: pele-core and rune-pele, each with its default features, build on 1.95. 1.94 cannot read the `if let` guards of pele-core (`scene/style.rs`) and of Rune (`edit/input.rs`, `shortcuts/service.rs`), so Rune and pele-core bound rune-pele alike. The dependencies ask for at most 1.88 (pele-core, through `url`'s ICU crates) and 1.89 (rune-pele, through `uuid`). Both crates name `rust-version = "1.95"`; `pele-cli` and `pele-oracle` keep the workspace's 1.100. `mise run msrv` checks both on the Rust they name, without `--ignore-rust-version`, so a dependency that asks for more fails it too. It takes 0.8 s warm, 2.0 s after a change to pele-core and 26 s cold once Rust 1.95 is installed (9 s), so `mise run check` runs it.

**Tests and breaks.** `pele-core/tests/options.rs` draws each of 18 builders as Pele draws the source with the same render option through the oracle, and holds that the option changes the drawing; `turn_wider_than`, which Pele lacks, turns a flowchart over Pele's breakpoint as Pele turns it under its own. `rune-pele/tests/api.rs` reads a syntax error's line and column and a Scene's size through `rune_pele::pele_core`. `grows.rs` in each crate holds `compile_fail` doctests with the error each must give, E0639 for a struct literal and E0004 for a match with no wildcard: 14 in pele-core, 12 in rune-pele. `break-lowering.sh` has 13 new breaks, each caught on its own. Four breaks moved and are re-pointed: `gradients-only-with-a-prefix`, `pele-metrics-pin-the-clock` and `fit-turns-only-under-640` at the builder calls that replaced their struct literals in `lib.rs`, and `doc-private-link` at `Unsupported::Gradient`'s doc, since `Figure::along`, whose doc it edited, is hidden and rustdoc checks no link of a hidden item.

**Against `v0.1.0` (measured, cargo-semver-checks 0.51.0).** pele-core: 8 lints fail, all meant: the `#[non_exhaustive]` options, errors, `DiagramKind`, `MonospacePart`, `Role` and `LinkRef`; the Scene's walk and palette types, `js`, the text measurer, `NativeInputs` and the hidden `Scene`, `Prepared` and `ParityScene` methods now hidden; `js::write_num` private. rune-pele: 10 lints fail, all meant: its eight `#[non_exhaustive]` enums and three counters; `bands` and `fill` private; `geometry`, `stroke` and all of `text` and the lowering but what an app uses now hidden; `Palette::core` hidden; `MOST_DASHES` private. It does not report `lower::NATIVE` turning from `[DiagramKind; 33]` into `&[DiagramKind]`, which is breaking too. Each is in the `BREAKING CHANGE:` footer of its commit.

**cargo-semver-checks and rune-pele (measured).** `cargo semver-checks --baseline-rev v0.1.0 -p pele-core -p rune-pele` checks pele-core and prints nothing for rune-pele, and `-p rune-pele` alone stops while resolving: it builds rune-pele in a project of its own, which the workspace's `[patch.crates-io]` does not reach, and Cargo cannot resolve `rune` (`parley_engine` lacks `line-dictionaries`). `main`'s `b857ebc` gives CI the patches through Cargo's home config (`scripts/cargo-home-patches.sh`). With them in a throwaway Cargo home, `-p rune-pele` reports the 10 lints above; rustdoc JSON built in this workspace and in a copy of the `v0.1.0` tree, compared with `cargo semver-checks check-release --baseline-rustdoc … --current-rustdoc …`, lists the same 43 items.

**Checks (measured, on the code of `994b509`).** `pele parity` on a fresh `--out` reads `522 522 0 0 0`. `mise run rune-corpus`'s ten reports, `side-by-side.txt` among them, are byte-identical to `main`'s. `mise run consumer-tag` passes on `v0.1.0` and on a throwaway tag of the branch, `mise run consumer` and `mise run msrv` pass, and the doctor is ok. Every break script passes in full under `mise exec --`.

## 2026-11-20 CI on GitHub Actions

**Why.** Nothing ran the tests in CI: `release-plz.yml` released on pushes to main and `commits.yml` checked commit messages. The repository is to be made public, and contributors need the checks to run on their pull requests.

**What runs** (`oss-ci`). The tests run locally on macOS and in CI on ordinary Linux runners, the user's choice (2026-11-20) for a project with few users so far; wider testing waits until it has more. The job runs one script, which a developer runs the same way.
- `CI` (`ci.yml`), on every pull request, every push to main, and by hand, on `ubuntu-24.04-arm`: `mise run ci` (`scripts/ci.sh`) lints the workflows, runs `scripts/setup.sh` and `scripts/check.sh`, and holds `pele parity` on a fresh folder to `522 522 0 0 0`. arm64 rather than x64: the whole job was measured passing on arm64 Linux, drawing tests included, while on x64 they were never seen to draw (below), and Pele's maths on arm64 matches the Node every measurement here was made with. A newer run cancels an older one of the same pull request, or of main for the same event. Pele's checkout is cached by its pin, Cargo's registry and the dependencies in `target/` by rust-cache, saved from main only.
- `Prove` (`prove.yml`), weekly and by hand: `mise run prove`.
- `Release-plz` takes a token from the release GitHub App when `RELEASE_APP_ID` is set, and `GITHUB_TOKEN` while it is not. A step's `if` cannot read `secrets`, so a workflow-wide `RELEASE_APP: ${{ secrets.RELEASE_APP_ID != '' }}` gates the token step, and release-plz gets `${{ steps.app.outputs.token || secrets.GITHUB_TOKEN }}`. Measured with act on a copy of the two expressions with the token step stubbed: without the secrets the step is skipped and release-plz gets `GITHUB_TOKEN`; with them it gets the App's token.
- actionlint 1.7.12 and ShellCheck 0.11.0 are pinned, and `mise run actionlint` lints every workflow. `scripts/break-ci.sh` has two breaks, each caught.

**What each check needs (measured on an M3 Pro, 11 cores).**

| Check | Pele and Node | GPU | Fonts | macOS | Cold | Warm |
| --- | --- | --- | --- | --- | --- | --- |
| notices, fmt | no | no | no | no | 2 s | 2 s |
| clippy, both feature sets | no | no | no | no | 26 s | 1 s |
| `cargo test` | yes: the differential tests | rune-pele's | Arial's metrics for resvg | no | 83 s build, 870 s run | 870 s |
| rune-pele without default features | yes | yes | yes | no | 48 s build, 112 s run | 80 s |
| doc gate | no | no | no | no | 16 s | 1 s |
| `mise run msrv` | no | no | no | no | 32 s | 1 s |
| `scripts/consumer.sh` | no | yes | no | no | 45 s | 1 s |
| `pele parity` | yes | no | no | no | 12 s build, 1 s run | 1 s |
| `mise run rune-corpus` | yes | yes | yes | no | 88 s build, 42 s run | 42 s |
| `mise run prove` | no | no | no | no | 199 s, 185 s of it proving | not measured |

On macOS the GPU is Metal and the font is `/System/Library/Fonts/Supplemental/Arial.ttf`. On Linux the GPU is Vulkan, which Mesa's lavapipe provides in software (`mesa-vulkan-drivers`), and the font Liberation Sans (`fonts-liberation`), which `tests/common` already looks for. Rune links xkbcommon and Wayland.

**Linux (measured, Ubuntu 24.04 containers under OrbStack, before the user ruled out local Linux containers).**
- x64, under Rosetta: every build and check passes, `pele parity` reads `522 522 0 0 0`, and every test of pele-cli and pele-oracle passes. Eight tests of pele-core fail because Pele answers differently there, none because of a drawing, and are ignored on that platform with the reason:
  - x64 Node sets the sign bit of a NaN it computes, where arm64 Node and the port do not: `common`'s `cap_width_matches_pele`, `sankey_layout`'s random flows, `xychart_ticks`' value axes, and `oracle`'s `sin` and `cos` of the infinities. JavaScript shows no NaN's bits.
  - Node on Linux takes glibc's `pow`, which rounds 10^23, 10^126 and 10^210 a bit apart from Apple's, which the port matches: `xychart_ticks`' powers of ten (3 of 806) and tick steps (357 of 38,875). 10^23 lies halfway between two doubles.
  - `venn_layout`'s unsettled relaxation holds that the port draws one source apart from arm64 Pele. On x64 Pele draws it as the port does, as the test's own doc says.
- x64, rune-pele: every test that draws fails at Rune's first shader under Rosetta. Rosetta has no AVX, so Mesa turns F16C off, lavapipe then offers no `shaderFloat16`, and wgpu takes `unpack2x16float` away from llvmpipe without it (read from Mesa 25.2.8's `u_cpu_detect.c` and `lp_bld_limits.h`, and Rune's vendored wgpu-hal). GitHub's x64 runners have AVX and F16C (inferred), so lavapipe should draw there.
- arm64, where Mesa gives lavapipe float16 on any CPU: all 25 rune-pele test binaries pass on lavapipe. `scripts/ci-linux.sh` runs to the end on 2 CPUs in 1,429 s with the test dependencies built: 876 passed, 0 failed, 21 ignored, parity `522 522 0 0 0`.
- Disk: a runner has 14 GB. After `cargo test`, `target/` held 32 GB with debug info (x64), 18 GB with line tables only and 4.8 GB with none (arm64). With none, the whole Linux job left 7.8 GB, besides the consumer's build, which was not measured apart. CI builds with `CARGO_PROFILE_DEV_DEBUG=0`. A panic still names its file and line, and debug assertions stay on.

**Budget (GitHub's docs, read 2026-11-20).** A private repository on the Free plan gets $12 of compute a month for all its jobs: Linux arm64 at $0.005 a minute, about 2,400 minutes, and x64 at $0.006. Standard runners are free for public repositories. Estimated, not measured: a run takes 35 to 60 minutes on 2 runner CPUs (1,429 s on 2 M3 cores, times 1.5 to 2.5), so 40 to 70 runs a month fit while private. Past the allowance, with no payment method, jobs stop until the next month. `Prove`, weekly, takes about 60 Linux minutes a month.

**Findings.**
- `mise run <task>` installs every tool `mise.toml` pins that is missing, Kani's among them. CI sets `MISE_TASK_RUN_AUTO_INSTALL=false`; `mise exec rust@1.95`, which `mise run msrv` runs, still installs only that Rust.
- rust-cache 2.9.3 keeps a folder of `target/*/build` whose name, less its last `-` part, is a dependency's: `rune-pele` passes as Rune's. It holds the test binaries, so CI removes it before the cache is saved.
- `examples/consumer/Cargo.lock` still names `pele-core` and `rune-pele` 0.1.0, so every `mise run check` rewrites it to 0.2.0. release-plz does not bump a lock outside the workspace.
- mise 2026.11.10 calls `actionlint` deprecated in favour of `jactionlint`, jdx's fork; actionlint 1.7.12, of 2026-03-30, is its latest release.

**Open.**
- Compare a NaN with any NaN in the differential tests (`common.rs`, `sankey_layout.rs`, `oracle.rs`, `xychart_ticks.rs`), so that the five NaN tests would run on x64 too.
- Only GitHub can show: lavapipe and Liberation Sans on the arm64 runner, the job's time and the disk left, and the release App's token end to end.

## 2026-11-21 Optimised test builds

**Why.** `mise run check` took about 17 minutes warm, and CI's first run 54 minutes. Timing each of the 135 test programs alone showed the parity tests against Pele, not the drawing tests, taking most of it, in the port's own code built unoptimised: `agentflow_parity` ran 129 s on one core.

**Change.** The workspace's `Cargo.toml` builds tests with `[profile.dev] opt-level = 1` and dependencies with `[profile.dev.package."*"] opt-level = 3`. Debug assertions and overflow checks stay on. Apps that depend on the crates are not affected: Cargo reads profiles from the app's own workspace.

**Measured (M3 Pro, each program run alone).**

| Build | Build time | Test run |
| --- | --- | --- |
| Unoptimised, as before | 84 s | 771 s |
| Dependencies at `opt-level = 3` | 154 s | 652 s |
| That, and the workspace at `opt-level = 1` | 162 s | 227 s |

All 135 programs pass in each. `agentflow_parity` went from 129 s to 5.5 s, `graph_view` from 36.8 s to 2.5 s. `mise run ci` then took 810 s from a clean build and 216 s warm, with `pele parity` reading `522 522 0 0 0`; before, it took 848 s warm with `mise run rune-corpus`, which it no longer runs.

## 2026-11-21 A gallery on Rune's Playground, natively and on the web

**Why.** People should see and try rune-pele without cloning it: a gallery on Rune's built-in Playground, run natively and built for the web.

**The gallery** (`examples/gallery`, `mise run gallery`). An app in a workspace of its own, as `examples/consumer` is, building into the workspace's one `target/`: an app's own manifest and lock, Rune's default features, and no change to the workspace's members, its lock, `cargo test` or release-plz, which releases every member with `git_only` unless told not to. It is not an example of `crates/rune-pele`, whose dev-dependencies (the oracle, resvg) would build for the web too. Its Playground has a story for each file of `tests/fixtures/rune` and pie's from `tests/fixtures/pie-pets.mmd`, so all 33 types show, grouped as Graphs, Time and process, Charts, Maps and trees and Notation; `across.mmd` and `unlowered.mmd` sit under Behaviour, as a flowchart wider than most windows and one that falls back to rune-mermaid. Try it/Live editor is Rune's multi-line `text_input` over the drawing: while what is typed fails, the view keeps the last drawing that drew (`RenderStatus::Kept`), and a line beside it says Drawn, Fallback, Kept or Error. Each story's side column sets its zoom and whether it is drawn in the stage's appearance, light or dark. `.enabled(true)` shows the Playground in release builds and on the web, and the app opens it on its first frame. `mise run gallery-test` holds that every fixture has a story that draws natively but the one that falls back, that every `DiagramKind` has a story, that a paste that does not parse keeps the drawing, and that the side column's keys step the zoom and the theme.

**Not rune-knobs.** rune-knobs' `inline` panel always shows a Random button (`rune-knobs/src/view.rs:157`), whose first press seeds from `SystemTime::now()` (`lib.rs:569`), which panics on the web, and the seed is private (`lib.rs:285`), so an app cannot seed it first. The gallery draws its own two controls instead, with buttons and ↑↓←→ through `Story::side_keys`. They cost the panel's Random, presets, undo, copy and slider dragging, which a zoom and a theme do not need.

**Clocks on the web** (measured). Both crates compiled for wasm32-unknown-unknown already (`cargo check`, 46 s cold); the standard library's clocks compile there and panic when called. rune-pele now reads `rune::time`, the standard library's natively and the page's on the web, so it gains no dependency. pele-core reads `js_sys::Date::now()`, as Pele reads `Date.now()`, as a dependency of `cfg(all(target_arch = "wasm32", target_os = "unknown"))` alone: WASI has a clock, and native builds read `SystemTime` as before. Natively nothing changes: `pele parity` reads `522 522 0 0 0` on a fresh `--out`, and the ten reports of `mise run rune-corpus` are byte-identical to `main`'s. `tests/web_clock.rs` holds that a gantt chart with no `now` draws as with `now` set to the platform's clock, natively and in Node through wasm-bindgen-test.

**The check** (`mise run web-check`, in `scripts/check.sh`). A compile check cannot see a clock that panics, so clippy lints pele-core, rune-pele and the gallery for wasm32-unknown-unknown with `examples/gallery/web/clippy.toml` as `CLIPPY_CONF_DIR`, which disallows `Instant::now` and `elapsed`, `SystemTime::now` and `elapsed`, `thread::spawn` and `thread::sleep`. No other clippy run reads that file, so native code and the tests call them freely. The task also formats the gallery and runs the clock test in Node, which fails on a std clock even where the lint is silenced. 5 s warm. `examples/gallery/web/break-web-check.sh` has four breaks, each caught. Lesson: a break restored with `mv` keeps the file's old time, and cargo then keeps the broken build; the script touches the file, as `break-lowering.sh` does.

**The web build** (`mise run gallery-web`, measured in T3 Code's preview, Electron's Chrome 152 on an M3 Pro). Rune's `scripts/web-build.sh`, run from the gallery's workspace, builds a release wasm in 87 s (2 s warm): 11,656,952 bytes, 4,192,032 gzipped. The script copies Inter's full faces only when the build's dep-info names them, which it does for path dependencies alone, so with Rune from git it copied none; `build.sh` copies them from the same checkout, and the page fetches all three. It starts, shows the Playground's list, filter, size picker, appearance button and side column, opens a story by click or by the filter, steps the zoom and theme by button and by key, types into the editor, keeps and redraws, streams, falls back to rune-mermaid, and goes Back and back again, with no console message. At a phone's 430 points the Playground's two columns leave the stage about 100 points. A link cannot open a story: Rune reads no arguments on the web (`tooling/args.rs:112`).

**Open.** Hosting on GitHub Pages, a static fallback for browsers without WebGPU, a story chosen from the page's URL (`Tx::open_playground_at` from the app's own reading of the location), and the wasm's size (`--opt` needs binaryen's wasm-opt, not pinned).
