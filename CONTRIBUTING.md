# Contributing to rune-pele

rune-pele is a port, so Pele 1.13.0 decides what is right: its syntax, its layout and its SVG. This guide says how to set up a checkout, what each check proves, and how a change lands. [`AGENTS.md`](AGENTS.md) holds the same rules in short, for coding agents.

## Prerequisites

- **mise.** [mise](https://mise.jdx.dev) installs every tool at the version `mise.toml` pins: Rust 1.100.0 with rustfmt and clippy, Node 26.13.1, Kani 0.68.0, hk 2.13.0, release-plz 0.4.3, cargo-semver-checks 0.51.0, actionlint 1.7.12, ShellCheck 0.11.0 and cargo-deny 0.22.1. `mise run msrv` also installs Rust 1.95, the first time it runs.
- **git and a network** for the first setup, which clones Pele from GitHub and installs the harness's npm packages, and for the first build, which fetches Rune from GitHub.
- **Everything through mise.** Run every tool as `mise exec -- …` or `mise run …`. The Node decides what the oracle prints, so a bare `node` can make a parity claim false, and the oracle refuses a Node other than the one `mise.toml` pins.

Where it works:

- Every measurement in this repository was made on macOS on Apple Silicon, and the tests run there locally. CI runs them on Linux arm64. They are not run on Windows.
- The view's tests draw headless, without a window, but they need a GPU adapter.
- The pixel tests draw Pele's SVG with resvg in Arial or a face with Arial's metrics: Arial on macOS or Windows, Liberation Sans on Linux, or the face `RUNE_PELE_RESVG_FONT` names. Without one they stop with `an Arial-compatible font, or RUNE_PELE_RESVG_FONT naming one`.
- On Linux, Rune needs the system libraries its README lists under "Linux".
- `mise run intel-trig` runs Node for x64 under Rosetta 2, so it needs a Mac with Apple Silicon.
- The tasks are POSIX shell and Node scripts.

## Setup

```sh
mise install        # the tools mise.toml pins
mise run setup      # Pele at the pin, the oracle bundle, and the git hooks
.agents/skills/verify-rune-pele/scripts/doctor.sh
```

`mise run setup` clones Pele into `vendor/pele` at the commit in `scripts/pele-pin.txt`, runs `npm ci` in `harness/`, bundles the oracle into `harness/dist/oracle.mjs`, and installs the git hooks from `hk.pkl`. It ends with `pele 1.13.0 at 3a2f075…, oracle at harness/dist/oracle.mjs`. It took 3.5 s on a fresh checkout here, and 6 s when run again.

The doctor is read-only. It checks that the Node the oracle runs and rustc are the pinned ones, that `vendor/pele` is at the pin with no edits, that the oracle bundle is built from the current harness and pin, and that `pele-cli` and the view's examples build. It ends with `doctor: ok`, or stops at the first `doctor: FAIL <reason>` with the fix.

## Everyday commands

Times are wall-clock times, measured on an Apple M3 Pro that other builds shared: on a fresh checkout ("cold"), and again with nothing changed ("warm").

| Command | What it checks | Time |
| --- | --- | --- |
| `mise run lint` | The checks that need no build: the licence copies, `cargo fmt --check`, actionlint over the workflows, and ShellCheck over every tracked `*.sh` at its default severity. | 1.7 s warm: ShellCheck 0.4 s, rustfmt 1.2 s. |
| `mise run deny` | cargo-deny holds every crate in `Cargo.lock` to `deny.toml`: a licence its list allows, crates.io or Rune's repository as the source, no `*` requirement, no vulnerability, and no unmaintained crate the workspace depends on itself. Duplicate versions only warn. CI's lint job runs it; `mise run check` does not. It needs a network, for the advisory database and the crates `cargo metadata` reads. | 1 s warm; 15 s with nothing in Cargo's home. |
| `mise run check` | Everything `mise run lint` checks, clippy with `-D warnings`, and every test: the unit tests, the differential tests against Pele through the oracle, and the view's headless tests in debug. Then rune-pele without its default features, clippy and tests; the doc gate, which fails on a public item with no doc comment; `mise run msrv`; and `mise run consumer`. It must pass before a commit. | About 13 min from a clean build, 3 to 4 min warm. `mise run ci`, which adds setup and parity, took 810 s from a clean build and 216 s warm on an M3 Pro. Test builds are optimised (`[profile.dev]` in `Cargo.toml`), which made the tests about 3.4 times faster. |
| `mise exec -- cargo run -q -p pele-cli -- parity` | `pele parity`: every source of Pele's documentation corpus through Pele and through the port. It prints a table per type and exits 1 unless every source is byte-identical. It reads `522 522 0 0 0`. | 13 s cold, 2 s warm. |
| `mise run rune-corpus` | The pixel comparison: every corpus source of the 32 types other than pie drawn natively, in a release build, beside Pele's SVG through resvg. `mise run check` does not run it. | 2 min 16 s cold, with the release build; 1 min 28 s warm. |
| `mise run prove` | The Kani proofs of the JavaScript number helpers. It passes when the last line reads `Complete - 9 successfully verified harnesses, 0 failures, 9 total.` | 4 min 34 s. |
| `mise run consumer` | `examples/consumer`, an app in a workspace of its own: fmt, clippy and its four tests. They draw its view headless at a phone's width and a desktop's, and hold its manifest and the README's to the workspace's. | 44 s cold, 4 s warm. |
| `mise run consumer-tag [tag [url]]` | An app under `out/consumer-tag/` that depends on rune-pele by git at a release tag, the newest `v*` tag unless given one, built and tested with only the pinned `cargo` and `rustc` on `PATH`: an app needs no `vendor/`, no Node and no oracle. Release tags are made on GitHub, so a checkout has them after `git fetch --tags`. | 1 min 44 s cold, 9 s warm. |
| `mise run msrv` | pele-core and rune-pele, with their default features, on Rust 1.95, the `rust-version` each names. | 28 s cold, with Rust 1.95 installed; 1 s warm. |

`pele render <file.mmd>` prints this port's SVG of a file, and `pele render <file.mmd> --engine pele` Pele's, through the oracle: `mise exec -- cargo run -q -p pele-cli -- render tests/fixtures/flow.mmd`. `cargo run -p rune-pele --example diagram -- <file.mmd>` opens a file in the native view, and `--snap <png>` draws it headless. The `verify-rune-pele` skill has every flag and exit code.

## Workspace layout

| Path | What it is |
| --- | --- |
| `crates/pele-core` | Rune-free core: the input boundary, the diagram catalog, text measurement, the Scene, the SVG writer and the ported types. `layout` and `graph` hold what the types that draw nodes and edges share. `js` holds the JavaScript number rules every Pele coordinate goes through. |
| `crates/pele-oracle` | Asks Pele, built from source, what it would output. Tests and the CLI use it. |
| `crates/pele-cli` | The `pele` binary: `pele render` and `pele parity`. |
| `crates/rune-pele` | The Rune adapter, and the only crate that depends on Rune. `Diagram::view` draws a Scene as native Rune elements for all 33 types, which `lower::NATIVE` lists, and hands a source whose Scene it cannot draw to Rune's own `rune-mermaid`. `ViewOptions::fit` fits a diagram to the width its view has, and `ViewOptions::zoom` draws it larger or smaller. While a source fails after one that drew, as one still streaming in does, it keeps showing that drawing, fitted as it would be drawn now, dimmed with the error over it as Pele's `mount` does, or unchanged with `KeptLook::Unchanged`. `examples/diagram.rs` opens any Mermaid file this way, fitted to its window, and with `--stream` types it in a few characters a frame. `examples/pie.rs` shows a palette switch. |
| `harness/` | `oracle.ts`, bundled by esbuild into `harness/dist/oracle.mjs` and run by the pinned Node. |
| `vendor/pele` | Pele's source at `scripts/pele-pin.txt`. Gitignored, and fetched by `mise run setup`. |
| `tests/fixtures` | Sources for the CLI recipes, and one folder per ported type with the sources harvested from Pele's tests by `scripts/harvest-<type>.mjs`. |
| `examples/consumer` | An app in a workspace of its own that depends on rune-pele, as the README's "Using rune-pele in your app" says. |
| `scripts/` | The tasks' scripts, the harvest and break scripts of each type, and `pele-pin.txt`. |
| `.agents/skills/` | The two project skills, `verify-rune-pele` and `prove-js-numerics`. `.claude/skills` links to them. |
| `docs/decisions.md` | The decision log, newest last: each decision, why, and the evidence. |
| `DIVERGENCES.md` | Every way the native view deliberately draws other than Pele's SVG in a browser, with the reason. |

## How the port is checked

Every claim that the port matches Pele comes from Pele itself, never from a hand-written expected output. Two skills hold the detail: [`verify-rune-pele`](.agents/skills/verify-rune-pele/SKILL.md) drives the CLI and the view and has a recipe for each feature in `features/`, and [`prove-js-numerics`](.agents/skills/prove-js-numerics/SKILL.md) holds the Kani proofs. Read them before claiming that a type, a number helper or a view feature works.

- **The oracle.** `harness/oracle.ts` imports Pele's TypeScript source from `vendor/pele`. esbuild bundles it into `harness/dist/oracle.mjs`, and the pinned Node runs it. `pele-oracle` asks it what Pele outputs for a source, and the tests and `pele parity` compare with that.
- **Parity.** `pele parity` renders all 522 sources of Pele's documentation corpus, `vendor/pele/tests/corpus/<type>-docs.json`, through Pele and through the port. A source is identical when the SVG is byte-equal and the width and height are equal. Each type also has differential tests, `<type>_parity` and, for most, `<type>_front`, over every source of that type in Pele's own tests, harvested into `tests/fixtures/<type>/harvest.json`. `mise run check` runs them.
- **The pixel comparison.** The view draws the same Scene natively. `mise run rune-corpus` draws the 520 corpus sources of the 32 types other than pie at 1× and 2×, draws Pele's SVG of each with resvg, and compares the pixels outside text. At 2× a drawing may have at most 5 pixels in 10,000 that no known difference explains, and no patch of such pixels over 12 pixels (3 square points). Flat areas are within 4 levels of resvg's at the 99.9th percentile. At 1×, where Rune draws slanted hairlines a little darker than resvg does, a patch may reach 30 pixels. The report is `out/verify/rune-graph/side-by-side.txt`, with an image of each drawing under `side-by-side/`. `mise run check` runs the other view tests in debug, and a debug build skips the ones that compare hundreds of drawings, such as the corpus and the zoomed fixtures of `fit_view`. The skill's `features/rune-view.md` runs each in release.
- **The Kani proofs.** `mise run prove` proves, for every double each names, that `Math.round`, Pele's `num()`, the rounding of `toExponential` and `toPrecision`, and the special values of V8's `sin`, `cos` and `log10` are what the port computes.
- **Lints.** Clippy and rustc hold rules the code must keep, and `-D warnings` makes each an error. `crates/pele-core/clippy.toml` denies pele-core the `f64` methods and libm functions that answer otherwise than V8, such as `f64::round`, `f64::sin`, `f64::max` and `libm::pow`, and each entry's reason names the `js` function to call instead. Only pele-core reads that file, so `mise run check` holds it; `mise run web-check` reads `examples/gallery/web/clippy.toml` instead. `[workspace.lints]` in `Cargo.toml` forbids unsafe code and warns on a public type without `Debug` and on a builder method without `#[must_use]`. A call that must stay takes an `#[expect]` with its reason where it is, never a crate-wide `allow`. Clippy cannot tell a number's `Display` or `{:.N}` from any other type's, so that half of the numbers rule is held by review and the parity tests only.
- **Breaks.** A break script edits the source to put one fault in, shows that the test that should catch it fails, and puts the source back. With names, it runs only those breaks. Run each under `mise exec --`; a bare run uses another Rust, which cannot build the crates. The scripts are `crates/rune-pele/break-lowering.sh` for the view and the API, `.agents/skills/verify-rune-pele/scripts/break-gates.sh` for the parity gate and the CLI, `.agents/skills/verify-rune-pele/scripts/break-walks.sh` for the Scene's keys, `.agents/skills/prove-js-numerics/scripts/break-numerics.sh` for the number helpers, `scripts/break-lints.sh` for the lints, ShellCheck and cargo-deny, and `scripts/break-<type>.mjs` for each type's differential tests. A new test gets a break in the matching script.

## How a change lands

- **Commit messages** follow [Conventional Commits](https://www.conventionalcommits.org): `feat:`, `fix:`, `docs:`, `test:`, `build:`, `refactor:`, `chore:`, `ci:`. release-plz reads them for the version bump and the changelog. hk's `commit-msg` hook, which `mise run setup` installs, rejects any other message, and its `pre-commit` hook runs `cargo fmt`, actionlint on the workflows and ShellCheck on the scripts a commit changes, and keeps each crate's `LICENSE` and `NOTICE` equal to the root's. Its ShellCheck step only checks: hk's builtin would apply ShellCheck's own fixes, which quote a variable a script splits on purpose.
- **A breaking change** is `feat!:`, `fix!:` or `refactor!:`, with a `BREAKING CHANGE:` footer that says what an app changes. cargo-semver-checks compares the public API with the last release, and makes any breaking change a breaking bump.
- **A Rune pin bump** is `build(rune): …`, because apps must move their Rune with it.
- **Releases.** Each push to `main` updates a release pull request, which bumps the version and writes `CHANGELOG.md`. Merging it tags the release, `v<version>`, and makes its GitHub release. `pele-core` and `rune-pele` share the version and the tag.
- **Decisions** go in [`docs/decisions.md`](docs/decisions.md), as a dated entry with the reason and the evidence.
- **A new divergence**, a deliberate way the native view draws other than Pele's SVG in a browser, gets a row in [`DIVERGENCES.md`](DIVERGENCES.md) with its reason. A difference that is not listed is a bug.

## Moving the Pele pin

No one procedure is written down. These steps are each in a script, a test or a skill:

1. Put the new commit in `scripts/pele-pin.txt` and run `mise run setup`, which checks it out in `vendor/pele` and rebuilds the oracle. The doctor fails until the bundle is rebuilt, since its stamp hashes the pin.
2. Follow the `prove-js-numerics` skill: diff Pele's `num()` in `src/svg/builder.ts` between the two pins, transcribe it again if it changed, and run `mise run prove` and the oracle tests.
3. Rerun the harvest scripts, `mise exec -- node scripts/harvest-<type>.mjs`, and `scripts/record-layout.mjs`, and commit what they write. Each writes the same file at the same pin, and `layout_replay` fails on a recording made at another pin.
4. Run `pele parity` on a fresh `--out` and `mise run check`. A type the new pin adds shows as not ported, and `pele render` exits 3 on it. A corpus with sources Pele rejects needs `--expect-pele-errors <n>`.
5. Run `mise run rune-corpus`, which holds the view to the new Pele's SVG, and update the Pele version wherever the docs name it.

A change of the Node in `mise.toml` also goes through the `prove-js-numerics` skill.

## Moving the Rune pin

No procedure is written down beyond this. The revision is in five files, which must name the same one: `crates/rune-pele/Cargo.toml` (`rune`, `rune-mermaid`, and `rune` again in the dev-dependencies), the workspace's `[patch.crates-io]` in `Cargo.toml`, `examples/consumer/Cargo.toml`, `examples/gallery/Cargo.toml`, and the manifest in `README.md`. `mise run consumer` fails until the first four agree, and each `Cargo.lock` updates on its next build. The patches are Rune's own, eight at `761555829`, from its README's "Depending on Rune" at that revision. The commit is `build(rune): …`.

## CI

GitHub Actions runs five workflows:

- **CI** (`.github/workflows/ci.yml`) runs on every pull request, every push to `main`, and when started by hand, in two jobs on Linux arm64 (`ubuntu-24.04-arm`). `Lint` runs `mise run lint` and `mise run deny`, the checks that need no build, with only Rust, actionlint, ShellCheck and cargo-deny installed, so a lint failure shows before the tests finish: 32 s on its first run. `Linux arm64` runs `mise run ci`: setup, everything `mise run check` runs, lint included, and `pele parity` held to 522 of 522 identical. On Linux the tests that draw use Mesa's lavapipe, a Vulkan GPU in software, and draw Pele's SVG in Liberation Sans, which has Arial's metrics. Eight pele-core tests are skipped on Linux, because Node gives different answers there; each names its reason. A newer push cancels the run it replaces. The test job takes about 33 minutes with the Rust cache warm. A change to documentation alone, every Markdown file and everything in `docs/` but `README.md`, whose patches and rev the consumer's test reads, skips both jobs: a first job, `Changes`, asks `scripts/code-changed.sh`, and a job its `if` skips reports success, so the required `Linux arm64` passes. Started by hand, or if `Changes` fails, both run.
- **Commit messages** (`commits.yml`) checks, on pull requests, that each commit follows Conventional Commits.
- **Pages** (`pages.yml`) builds the gallery for the web, `mise run gallery-web`, and publishes it on GitHub Pages on every push to `main` that changes more than documentation. A pull request that changes the crates, the gallery or its fixtures builds it without publishing.
- **Prove** (`prove.yml`) runs the Kani proofs, `mise run prove`, weekly and when started by hand.
- **Release-plz** (`release-plz.yml`) tags new versions and opens the release pull request on pushes to `main`, as the release GitHub App when its secrets are set.

A ruleset on `main` requires CI's `Linux arm64` job and the `conventional` commit check before a pull request merges, and blocks force pushes and deleting the branch. It does not require `Lint`, whose checks `Linux arm64` runs too, but for cargo-deny.

Each job runs the commands it names, which you run the same way after `mise run setup`: `mise run lint` and `mise run deny`, `mise run ci`, `mise run prove`, or `mise run actionlint` for the workflows alone. The hk pre-commit hook lints a workflow or a script you change. On Linux, install `mesa-vulkan-drivers`, `fonts-liberation`, `libxkbcommon-dev` and `libwayland-dev` first (Debian and Ubuntu names).

## Licence of contributions

rune-pele is MIT-licensed (`LICENSE`), and a contribution is made under the same licence. Code ported from Pele keeps Pele's MIT notice, which `NOTICE` carries, as each crate's copy of it does.
