# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/osmelmora/rune-pele/releases/tag/v0.1.0) - 2026-11-19

The first release. `pele-core` and `rune-pele` share this version.

### pele-core

- All 33 diagram types of Pele 1.13.0, ported to Rust. Their SVG is byte-identical to Pele's on the 522 sources of Pele's documentation corpus, checked by `pele parity`, and on every source of theirs in Pele's own tests.
- One Scene per diagram, which both the SVG writer and the Rune adapter draw.
- Stable keys for every drawn element, from the source's own names where it has them, so a drawing keeps its elements while a source streams in.
- JavaScript's number rules in `js`: `Math.round`, Pele's `num()`, `toExponential` and `toPrecision` are proved with Kani for every double. `Math.sin`, `Math.cos` and `Math.log10` are V8's fdlibm, bit for bit Node on x64.
- `LayoutOptions` with an optional padding, and `turn_wider_than`, which turns any of the 14 types that can turn when that avoids scrolling or small text.

### rune-pele

- `Diagram::view` draws all 33 types as native Rune elements, with links, tooltips, zoom, dark mode and accessible names. A source it cannot draw goes to Rune's rune-mermaid, and `Diagram::engine()` says which drew it.
- `ViewOptions::fit` fits the width the view has, and `ViewOptions::zoom` draws larger or smaller.
- While a streamed source fails, the view keeps the last good drawing. A style value a page would drop is dropped, as a browser drops it, so a half-typed value does not fall back.
- `ViewOptions::icons` lets the app supply icons for the boxes Pele leaves empty without an icon pack.
- `DIVERGENCES.md` lists where the native view differs from Pele's SVG in a browser.
