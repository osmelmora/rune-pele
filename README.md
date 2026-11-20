# rune-pele

rune-pele draws [Mermaid](https://mermaid.js.org) diagrams natively in [Rune](https://github.com/anomalyco/rune) apps. It is a Rust port of [Pele](https://github.com/obsidianmd/pele) 1.13.0, the Mermaid to SVG renderer, and the SVG it writes is Pele's, byte for byte.

- **All 33 diagram types** of Pele 1.13.0, from flowcharts and sequence diagrams to gantt charts, sankey diagrams and mind maps.
- **Native drawing.** `rune-pele` draws each diagram as Rune shapes and text, with links, tooltips, accessible names and the app's fonts. `pele-core` writes the same diagram as SVG, with no Rune at all.
- **Streaming sources.** While a source streams in, as a language model writes it into a chat, the view keeps the last drawing that drew until the source draws again.
- **Live themes.** A dark-mode or palette change repaints the drawing without laying it out again.
- **Fitting the width.** A view fits the width it has, from a phone to a wide screen. It lays a diagram out narrower where it can, shrinks it down to 11 point text, and scrolls sideways past that.

![The same state diagram twice, side by side and nearly alike: a start dot, a state named IsPositive, a choice diamond, and two edges labelled "if n < 0" and "if n >= 0" that lead to the states False and True. The left one is drawn natively by rune-pele in Rune, the right one is Pele's SVG.](docs/images/native-beside-pele.png)

A state diagram from Pele's documentation. Left, drawn natively by rune-pele. Right, Pele's SVG drawn by resvg. The native drawing is laid out with Pele's text metrics, as the pixel tests lay it out. An app's view measures its own fonts.

## Status

- **Pele 1.13.0**, at commit `3a2f075`, which `scripts/pele-pin.txt` names.
- **Parity.** `pele_core::render` writes the SVG Pele writes, byte for byte, with Pele's default options and its Arial metrics. `pele parity` holds the two to each other on all 522 sources of Pele's documentation corpus, by running Pele itself, and reads 522 identical. Each type's differential tests do the same for every source of that type in Pele's own tests. Pele's maths differs in the last bit between Node on Apple Silicon and on x64, and `pele parity` reads 522 identical on both. [`DIVERGENCES.md`](DIVERGENCES.md) has the detail.
- **The native view** draws the same layout as Rune elements. Where it differs from Pele's SVG in a browser, as in its fonts, its tooltips and how a link is followed, the difference is deliberate and [`DIVERGENCES.md`](DIVERGENCES.md) lists it with its reason. Any other difference is a bug. Of the 520 corpus sources the pixel tests compare, it draws 518 natively. The other two use what the view does not draw, a picture in a node and `text-decoration:line-through`, and Rune's rune-mermaid draws them instead. The pixel tests draw each source beside Pele's SVG, and at 2× allow at most 5 pixels in 10,000, outside text, that no known difference explains.
- **Platforms.** The tests run on macOS on Apple Silicon, where every measurement in this repository was made, and in CI on Linux arm64. They are not run on Windows. On Linux, Rune needs the system libraries its README lists under "Linux".

## Using rune-pele in your app

rune-pele is not on crates.io. An app depends on it from git at a release tag, the newest on the Releases page, with Rune's patches:

```toml
[dependencies]
rune-pele = { git = "https://github.com/osmelmora/rune-pele", tag = "v0.2.0", features = ["embedded-fonts"] }

[patch.crates-io]
winit = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
parley_engine = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
wgpu-hal = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
accesskit_windows = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
```

- **Rune's revision** is the one `crates/rune-pele/Cargo.toml` pins. `Diagram::view` takes that Rune's `Cx` and returns its `El`. An app's `rune` at another revision is a second `rune`, whose types rune-pele's do not take and whose features miss the Rune rune-pele draws with, so bump the two together.
- **Rune from rune-pele.** rune-pele re-exports its Rune as `rune_pele::rune`, and its `embedded-fonts` feature turns on Rune's. An app that names Rune only as `rune_pele::rune` always has rune-pele's Rune, and needs no `rune` line. `examples/consumer` does this. On `v0.1.0`, which has no re-export, an app names Rune with a line of its own at the same revision:

  ```toml
  rune = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21", default-features = false, features = ["embedded-fonts"] }
  ```

  Cargo sets a dependency's features only through the features that dependency declares, and rune-pele declares one for Rune, `embedded-fonts`, the font `FontCatalog::rune()` measures with. An app that wants others of Rune's features, such as its Metal backend, accessibility or SVG images, keeps such a `rune` line, at the same revision: two lines that name one revision are one `rune`, built with the features of both.
- **The four patches** are the `[patch.crates-io]` table of this repository's `Cargo.toml`, which is Rune's own (Rune's `crates/rune/README.md`, "Depending on Rune"). A dependency's `[patch]` does not reach the workspace that depends on it, so the app repeats them, from the same revision. Without `parley_engine`'s, Cargo cannot resolve `rune`: `package rune depends on parley_engine with feature line-dictionaries but parley_engine does not have that feature`. Without winit's, `rune` does not compile on macOS, which calls the input-method functions only Rune's winit has. wgpu-hal's lets Rune pick a Windows GPU without starting every adapter, and accesskit_windows' gives Windows screen readers headings and landmarks.

Then draw a diagram. This is `examples/consumer`, an app in a workspace of its own with the manifest above. Its tests draw this view headless and hold its manifest and this section's to the one rune-pele is built with. Its [`src/lib.rs`](examples/consumer/src/lib.rs):

```rust
use rune_pele::rune::{Cx, El, Tx, col, text, tok};
use rune_pele::{Diagram, Event, FontCatalog, Link, ViewOptions};

pub const SOURCE: &str = "flowchart LR
  A[Model writes] --> B[Chat streams] --> C[rune-pele draws] --> D[Reader follows]
  click D href \"https://example.org/guide\" \"Open the guide\"
";

pub struct App {
    pub diagram: Diagram,
    pub source: String,
    /// The last link pressed.
    pub followed: Option<Link>,
}

pub fn init() -> App {
    App { diagram: Diagram::new("diagram", FontCatalog::rune()), source: SOURCE.to_owned(), followed: None }
}

fn on_event(app: &mut App, event: Event, _: &mut Tx) {
    if let Event::Activate(link) = event {
        app.followed = Some(link);
    }
}

pub fn view(app: &App, cx: &mut Cx<App>) -> El {
    let t = tok::theme(cx.system_dark());
    let diagram = app
        .diagram
        .view_with(cx, &app.source, ViewOptions::fit(), on_event)
        .unwrap_or_else(|error| text(error.to_string()).color(t.text));
    col().w_full().h_full().fill(t.bg).padding(16.0).child(diagram)
}
```

And its [`src/main.rs`](examples/consumer/src/main.rs), which opens the window:

```rust
use rune_pele::rune::{self, AppSpec};
use rune_pele_consumer::{init, view};

fn main() {
    rune::run(AppSpec::new("rune-pele consumer", (720.0, 480.0), init, view));
}
```

Make one `Diagram` per diagram and keep it in the app's state. In the view, `diagram.view_with(cx, &source, ViewOptions::fit(), on_event)` draws the source as wide as its parent, and `on_event` hears a link pressed: the view opens nothing itself. `diagram.status()` says who drew the last view, `diagram.error()` why a source fails, and `diagram.engine()` which engine drew the drawing shown and what it can do. `rune_pele::capture` draws a view headless, for an app's own tests, as [`examples/consumer/tests/app.rs`](examples/consumer/tests/app.rs) does.

To see any Mermaid file drawn natively, clone this repository, run `mise install`, and run `mise exec -- cargo run -p rune-pele --example diagram -- tests/fixtures/rune/flowchart.mmd`. With `--stream` it types the file in a few characters a frame, as a model would.

## SVG without Rune

`pele-core` alone writes Pele's SVG, and needs neither Rune nor its patches:

```toml
[dependencies]
pele-core = { git = "https://github.com/osmelmora/rune-pele", tag = "v0.2.0" }
```

From pele-core's crate docs:

```rust
let rendered = pele_core::render("flowchart LR\n  A[Write] --> B[Draw]\n")?;
assert!(rendered.svg.starts_with("<svg"));
assert!(rendered.width > rendered.height);
```

`render` uses Pele's default options. `ParseOptions` and `LayoutOptions` are Pele's render options, which `parity_scene_with` lays a source out with and `svg::write` writes. A source Pele rejects is a `RenderError::Diagnostic`, with Pele's message and, for a syntax error, where it is. `cargo doc -p pele-core --open` in the app shows the rest.

## Options

- **Fonts.** rune-pele turns Rune's default features off, so the app chooses them, with its own `rune` line or with rune-pele's `embedded-fonts`. `FontCatalog::rune()` lays labels out in Inter, which Rune embeds with its `embedded-fonts` feature. Without it, and without fonts of the app's own, text falls back to the system's fonts: on macOS, labels are laid out and drawn in a system sans instead of Inter. An app that gives Rune its own fonts in `AppSpec::fonts` and `AppSpec::families` gives `FontCatalog::new` the same bytes and families, and can leave `embedded-fonts` off.
- **The fallback.** From `v0.2.0`, rune-pele's `fallback` feature, on by default, has Rune's rune-mermaid draw a source the native view does not. rune-mermaid draws no links or tooltips and ignores the zoom. An app that turns rune-pele's default features off, `default-features = false, features = ["embedded-fonts"]`, builds without rune-mermaid. Such a source is then an error, `ViewError::Geometry`, or `ViewError::NoFallback` for a type pele-core has not ported, as when rune-mermaid cannot draw it, and after a source that drew the view keeps that drawing. Every public item is in both builds, so the app's code compiles either way: `Engine::RuneMermaid` and `RenderStatus::RuneMermaid` are then never reported, and `Diagram::fallback()` is always `None`. `examples/consumer` built for release without it has one crate fewer (187 of 188) and a binary 1,133,040 bytes (3.8%) smaller. Its clean build was no faster in two runs each: 79 s and 84 s with it, 75 s and 95 s without.
- **While a source fails.** After a source that drew, a source that fails, as one still streaming in does, leaves that drawing shown, as Pele's `mount` does: by default dimmed, with the error over its bottom edge. `ViewOptions::fit().kept_look(KeptLook::Unchanged)` shows it as it was, for an app that shows the error itself or not at all. `diagram.status()` is then `RenderStatus::Kept`, and `diagram.error()` says why. The kept drawing follows the view's width and options as the source would. A source that fails with nothing drawn before it is an error, which the app shows.
- **Fitting and zoom.** `ViewOptions::fit()` fits the width the view has, and `.smallest_text(points)` sets the least size plain text is drawn at, 11 unless set. `ViewOptions::zoom(z)` draws `z` times the size, after any fit. `.palette(Palette::pele())` draws in Pele's own colours; without a palette the view follows the window's Rune theme, light or dark.
- **Icons.** A source can name icons, such as `logos:aws` for an architecture service or a treeview row. Pele draws them only for a page that gives it icons, so its SVG leaves such a box empty, and the view does too until the app gives it icons of its own. An `Icon` is SVG path data on a view box, filled or stroked, as an Iconify icon is. `IconPaint::Current` is the diagram's foreground, as `currentColor` is in Pele's SVG, and `IconPaint::Fixed` an icon's own colour. A name the function has no icon for is drawn as without icons. From rune-pele's crate docs:

  ```rust
  use rune_pele::{Icon, IconPaint, ViewOptions};

  fn icons(name: &str) -> Option<Icon> {
      let grid = Icon::new([0.0, 0.0, 24.0, 24.0]).ok()?;
      match name {
          "app:bolt" => grid.stroke("M13 2 4 14h7l-1 8 9-12h-7z", 2.0, IconPaint::Current).ok(),
          _ => None,
      }
  }
  let options = ViewOptions::fit().icons(icons);
  ```

- **The API.** From `v0.2.0`, an app relies on what `cargo doc` shows of the two crates. An item marked `#[doc(hidden)]` is public only for rune-pele or the workspace's tests, its doc says so, and any release may change it. Options are built with methods, as `ViewOptions::fit().zoom(0.75)` and `LayoutOptions::default().max_width(640.0)`. The types that will grow are `#[non_exhaustive]`, so a match on `ViewError`, `RenderStatus` or `Engine` has a wildcard arm. `rune_pele::pele_core` names the types rune-pele's own use, such as the `RenderError` in `ViewError::Core`, with no `pele-core` line of the app's own.
- **Rust.** Both crates build on Rust 1.95 and later, the `rust-version` each names.

## Versions and releases

`pele-core` and `rune-pele` share one version and one tag, `v<version>`, made on GitHub with a GitHub release. [`CHANGELOG.md`](CHANGELOG.md) lists each release's changes, and marks the breaking ones. Before 1.0, a compatible release moves the patch version and a breaking one the minor version: 0.2.0 becomes 0.2.1 or 0.3.0. An app moves to a release by changing its `tag`. A release that moves the Rune pin changes the revision of the four patches too, and an app moves them with it.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) says how to set up a checkout, run the checks and land a change.

## License

MIT. See [`LICENSE`](LICENSE). Code ported from Pele carries Pele's MIT notice in [`NOTICE`](NOTICE).
