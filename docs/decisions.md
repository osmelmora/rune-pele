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
