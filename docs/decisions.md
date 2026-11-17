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

- **Pie at byte parity through the Scene.** Branch `pie-parity`, worktree `../rune-pele-wt/pie-parity`, role `hardest tasks` (Claude Fable 5.2).
  - Done when `pele parity --type pie` shows 2 of 2 identical.
  - Also done when a harvested differential test of every pie source in Pele's test suite is byte-equal.
- **Rune adapter spike.** Branch `rune-spike`, worktree `../rune-pele-wt/rune-spike`, role `feature, refactoring` (Claude Opus 5.5).
  - It settles three risks: Rune builds as a dependency on Rust 1.100, text lands on a baseline from public metrics, and a headless PNG can be captured.

The briefs are in the session scratch area (`briefs/`). The parent reviews each diff and reruns each done check before merging.

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
- **Icons.** A host hook on `ViewOptions` is an API decision, deferred. Icons draw nothing, as Pele's SVG leaves the box empty.

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
- C4 has no walk test of Scene keys yet. Add one before C4's Rune gate opens.
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
- **Padding.** `LayoutOptions.padding_given` sits beside `padding: f64`, and the two must agree. Folding them into `Option<f64>` touches 77 call sites. Deferred, because the current form is correct.
- **Number helpers.** Treemap's `toExponential` and `toPrecision`, and xychart's fdlibm `log10`, live in their types, not in `js.rs`. They are checked against the oracle, but not proved with Kani.
- **Walk tests.** C4, git, sankey and packet have no walk test of Scene keys yet. Add them before their Rune gates open.
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
