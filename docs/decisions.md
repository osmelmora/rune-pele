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
