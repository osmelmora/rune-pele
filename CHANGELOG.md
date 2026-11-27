# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/osmelmora/rune-pele/compare/v0.3.0...v0.3.1) - 2026-11-27

### Added

- implement Debug for every public type
- mark every builder method #[must_use]

### Fixed

- stop the break scripts and the doctor when their cd fails

### Other

- count a lint break only on its own diagnostic, and break both Debug tests
- clear ShellCheck's remaining findings in the tracked scripts
- deny pele-core f64::clamp, which keeps a -0 Math.max makes +0
- deny pele-core the f64 maths that answers otherwise than V8

## [0.3.0](https://github.com/osmelmora/rune-pele/compare/v0.2.2...v0.3.0) - 2026-11-24

### Fixed

- *(rune-pele)* measure monospace labels in the face Rune now draws them in

### Other

- *(rune)* [**breaking**] move every pin to dev's 761555829 for four fixes

## [0.2.2](https://github.com/osmelmora/rune-pele/compare/v0.2.1...v0.2.2) - 2026-11-22

### Fixed

- read the page's clock on the web, where the standard library's panics

## [0.2.1](https://github.com/osmelmora/rune-pele/compare/v0.2.0...v0.2.1) - 2026-11-21

### Other

- skip on Linux x64 the eight pele-core tests whose Pele answers differently there

## [0.2.0](https://github.com/osmelmora/rune-pele/compare/v0.1.0...v0.2.0) - 2026-11-20

### Added

- [**breaking**] let rune-pele's enums and counters grow
- [**breaking**] build pele-core's options with methods and let its types grow
- put the rune-mermaid fallback behind a default fallback feature
- re-export Rune as rune_pele::rune and forward its embedded fonts

### Fixed

- name rune-mermaid in a view error only when the build has it

### Other

- re-point four breaks the builder calls and the hidden lowering moved
- [**breaking**] make private what nothing outside its crate calls
- hide the Scene's and text's internals item by item, so the doc gate sees what an app sees
- name pele-core's types through rune-pele and break the API one way at a time
- name the oldest Rust that builds each crate, 1.95, and check it
- document every public item and deny a missing doc in the doc gate
- [**breaking**] keep rune-pele's documented API to what an app calls
- wrap the fallback's doc comments at the crate's width
- [**breaking**] keep pele-core's documented API to what an app calls

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
