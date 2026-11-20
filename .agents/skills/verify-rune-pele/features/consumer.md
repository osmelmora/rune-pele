# Using rune-pele from another project

An app outside this workspace depends on `rune-pele` by path, and on Rune at the revision rune-pele pins with Rune's four patches, as the README's "Using rune-pele in your app" says. A dependency's `[patch]` does not reach the workspace that depends on it, so without the patches the app does not resolve. `examples/consumer` is such an app, in a workspace of its own: it draws a flowchart with `Diagram::view_with` and `ViewOptions::fit()`, and names Rune only as `rune_pele::rune`, with rune-pele's `embedded-fonts`. An app can turn rune-pele's `fallback` feature off and build without rune-mermaid, and `mise run consumer-tag` builds an app on a release tag, by git.

## Sub-features

- `consumer-build` resolves and builds the app on its own, with its own lock and patches, into the workspace's target directory, and lints it.
- `consumer-view` draws the app's view headless 390 and 1200 points wide: natively, inside its padding, with a link that tells the app.
- `consumer-manifest` holds the app's patches and the README's to the workspace's, and every Rune revision they name to rune-pele's.
- `consumer-unpatched` shows the same app without the patches does not resolve.
- `consumer-reexport` holds the Rune Cargo resolves for the app, which has no `rune` line, to the features it turns on through rune-pele: `embedded-fonts` and no others.
- `consumer-fallback` draws, on rune-pele's default features, a source the native view does not draw with rune-mermaid; without them, such a source is an error, or keeps the drawing before it.
- `consumer-tag` builds an app on a release tag, by git, with only the pinned Rust's `cargo` and `rustc` on PATH, and draws one diagram natively.
- `consumer-api` is the API an app relies on: the documented items of both crates, each with a doc comment, options an app builds with methods, types that can grow without breaking an app, `rune_pele::pele_core` for the types in rune-pele's signatures, and the oldest Rust each crate builds on, its `rust-version`. Anything an app should not call is `#[doc(hidden)]` with a line saying so.
- `consumer-semver` lists what cargo-semver-checks, which release-plz runs, finds broken against the last release.

## How to get to it (user POV)

- A developer copies the README's manifest into their app, makes a `Diagram` with `Diagram::new(id, FontCatalog::rune())`, and calls `diagram.view_with(cx, &source, ViewOptions::fit(), on_event)` in the view.
- `mise exec -- cargo run --manifest-path examples/consumer/Cargo.toml --target-dir target` opens the app's window.
- `mise run consumer` builds, lints and tests the app. `mise run check` runs it last.
- `mise run consumer-tag [tag [url]]` writes, builds and tests an app on the newest `v*` tag, or the tag given, fetched from this repository as a `file://` URL, or the URL given. `RUNE_PELE_TAG` and `RUNE_PELE_URL` set them too.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.
- A GPU adapter is available. The app's test draws headless without a window.

- **Build and test the app.** Run `$C $R consumer-build mise run consumer`. Exit 0. Stdout has `running 4 tests` and `test result: ok. 4 passed`: `the_app_draws_its_diagram_natively_within_a_phone_and_a_desktop`, `on_rune_pele_s_default_features_rune_mermaid_draws_what_the_native_view_does_not`, `the_app_pins_and_patches_rune_as_rune_pele_does_and_the_readme_says` and `the_app_s_rune_has_the_features_it_turns_on_through_rune_pele_and_no_others`.
- **Without the patches.** Copy the app aside without its patch table, and resolve it:

  ```sh
  mkdir -p out/verify/$R/unpatched && cp -R examples/consumer/src out/verify/$R/unpatched/
  sed -e '/^\[patch.crates-io\]/,$d' -e "s|\.\./\.\./crates/rune-pele|$PWD/crates/rune-pele|" \
    examples/consumer/Cargo.toml > out/verify/$R/unpatched/Cargo.toml
  $C $R consumer-unpatched cargo check --offline --manifest-path out/verify/$R/unpatched/Cargo.toml --target-dir target
  ```

  Exit 101. Stderr starts ``error: failed to select a version for `parley_engine`.`` and has the line `` package `rune` depends on `parley_engine` with feature `line-dictionaries` but `parley_engine` does not have that feature. ``
- **Without winit's patch.** Write the manifest again with `-e '/^winit = /d'` in place of the first `-e`, so that only winit's patch is gone, and run `$C $R consumer-unpatched-winit cargo check --offline --manifest-path out/verify/$R/unpatched/Cargo.toml --target-dir out/verify/$R/unpatched/target`. On macOS, exit 101 after Rune's dependencies are checked, about a minute cold, and stderr has ``cannot find function `take_ime_clauses` in module `winit::platform::macos` ``.

- **Without the fallback.** Run `$C $R consumer-no-fallback cargo test -q -p rune-pele --no-default-features --test without_fallback`. Exit 0 and `2 passed`: a flowchart with `style B text-decoration:line-through` is `ViewError::Geometry` with nothing drawn before it, and after a flowchart that drew is `RenderStatus::Kept` with that error. `cargo tree -p rune-pele --no-default-features -e normal | grep -c rune-mermaid` prints `0`. `mise run check` lints and tests rune-pele so too.
- **On a release tag.** Run `$C $R consumer-tag mise run consumer-tag`. Exit 0. Stdout has `rune-pele v0.1.0 at 4647b0f, from file://<repository>/.git`, `the tag's tree has no vendor/ and no oracle`, a `PATH=` line naming only `out/consumer-tag/bin` and the system's directories, `node, npm and mise are not on PATH`, and `test tests::an_app_on_the_tag_draws_a_diagram_natively ... ok`.
- **Break how an app uses rune-pele.** Run `$C $R consumer-break crates/rune-pele/break-lowering.sh rune-not-re-exported fonts-not-forwarded fallback-off-by-default fallback-ungated without-fallback-test-run-with-it`. Exit 0 and five lines `break <name>: <test> fails, as it should:`: `rune-not-re-exported` fails `rune_reexport` to compile, `fonts-not-forwarded` and `fallback-off-by-default` fail the app's tests, `fallback-ungated` fails `without_fallback` without default features, and `without-fallback-test-run-with-it` fails it with them. `git status --short crates/rune-pele Cargo.lock examples/consumer` prints nothing afterwards.

- **The API an app relies on.** Run `$C $R consumer-api-options cargo test -q -p pele-core --test options` and `$C $R consumer-api-reexport cargo test -q -p rune-pele --test api`. Exit 0, `2 passed` each: every builder of `LayoutOptions`, `ParseOptions`, `Limits` and `ResourcePolicy` draws as Pele draws the source with the same render option, and changes the drawing; a syntax error's line and column and a Scene's size are read through `rune_pele::pele_core`. Run `$C $R consumer-api-grows cargo test -q -p pele-core -p rune-pele --doc grows`. Exit 0, `14 passed` and `12 passed`, every one `compile fail`: each `#[non_exhaustive]` type refuses a struct literal or a match with no wildcard.
- **The doc gate.** `mise run check` runs `RUSTDOCFLAGS="-D warnings -D missing_docs" cargo doc -q --no-deps -p pele-core -p rune-pele`, so a public item with no doc comment fails it, with or without the crates' `#![warn(missing_docs)]`. `missing_docs` skips whatever lies under a hidden module, also an item re-exported elsewhere, so a module that holds anything an app uses is never hidden whole: its other items are hidden one by one.
- **The oldest Rust.** Run `$C $R consumer-msrv mise run msrv`. Exit 0, with `pele-core on Rust 1.95` and `rune-pele on Rust 1.95`: each crate, with its default features, checks on the Rust its `rust-version` names, and Cargo refuses a dependency that asks for a newer one. `mise run check` runs it.
- **Against the last release.** Run `$C $R consumer-semver cargo semver-checks --baseline-rev v0.1.0 -p pele-core -p rune-pele`. It lists each break it finds, and every one must be meant: a `feat!:` or `refactor!:` commit since the tag whose `BREAKING CHANGE:` footer says what an app changes. A clean tree reads only hidden or private items as gone, never a documented one by accident.
- **Break the API.** Run `$C $R consumer-api-break crates/rune-pele/break-lowering.sh layout-builder-sets-nothing resource-builder-sets-nothing limits-builder-sets-nothing turn-builder-sets-nothing layout-options-open-to-literals render-error-open-to-matches view-error-open-to-matches view-stats-open-to-literals doc-missing doc-missing-without-the-crate-lint doc-missing-on-a-root-re-export pele-core-not-re-exported msrv-below-what-builds`. Exit 0 and thirteen lines `break <name>: <command> fails, as it should:`. The two `*-open-to-*` breaks of each crate fail one `grows` doctest with `Test compiled successfully, but it's marked compile_fail`.

## Gotchas

- The app builds into the workspace's `target/`, with Rune's features as the app sets them. The first run builds `rune`, `rune-mermaid`, `pele-core` and `rune-pele` once more for those features, about a minute; a warm run builds nothing new.
- Cargo reads `.cargo/config.toml` from the directory it runs in, not from the manifest's, so the task passes `--target-dir target`. A `cargo` run inside `examples/consumer` without it builds into `examples/consumer/target`, which that directory's `.gitignore` leaves out.
- A Rune bump changes the revision in four places: `crates/rune-pele/Cargo.toml`, the workspace's `[patch.crates-io]`, the app's manifest and the README. `consumer-manifest` fails until they agree, and the app's `Cargo.lock` then updates on its next build.
- wgpu-hal's and accesskit_windows' patches change Windows only, which this recipe does not reach. Rune falls back without wgpu-hal's (`crates/rune/src/gpu/pick.rs`).
- Without `embedded-fonts`, Rune at this revision draws text in the system's fonts on macOS, so no pixel test tells the feature apart: `consumer-reexport` reads the features Cargo resolves instead.
- Release tags are made on GitHub, and a checkout has them only after `git fetch --tags`. Without one, `mise run consumer-tag` stops with `no v* tag`. Its clone of an annotated tag prints git's own `warning: refs/tags/<tag> ... is not a commit!`, which is harmless.
- `mise run consumer-tag` builds into `out/consumer-tag/target`, about 50 s the first time and a few seconds after. It starts the app's `Cargo.lock` from the tag's, so the app builds the crates the tag was tested with, and Cargo fetches nothing but the tag when they are cached.
- `mise run msrv` installs Rust 1.95 through mise the first time, about 9 s, and checks into `target/msrv`: 26 s cold, under a second warm. `--ignore-rust-version` stays out of it, so a dependency that asks for a newer Rust fails it too.
- `#[doc(hidden)]` items stay callable from an app; only the docs and cargo-semver-checks leave them out. A hidden item's doc says who it is public for, so a reader of the source knows it is no promise.
