//! The last drawing that drew, kept while a later source fails: through every character prefix
//! of every fixture, in both looks, after a fallback to rune-mermaid, through a palette or
//! dark-mode change, and until a source draws again.

// What only the tests that need rune-mermaid use is unused without `fallback`.
#![cfg_attr(not(feature = "fallback"), allow(dead_code, unused_imports))]

mod common;

use std::cell::RefCell;
use std::fmt::Write as _;

use common::out_dir_for;
use pele_core::{LayoutOptions, ParseOptions, RenderError};
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels, Window};
use rune_pele::{Diagram, Fallback, FontCatalog, KeptLook, Palette, RenderStatus, ViewError, ViewOptions};

const RUN: &str = "rune-keep";
/// Every fixture: one of each type that `NATIVE` lists but pie, then one that runs across, then
/// one that rune-mermaid draws.
const FIXTURES: [&str; 34] = [
    "flowchart",
    "class",
    "state",
    "er",
    "requirement",
    "usecase",
    "swimlane",
    "agentflow",
    "architecture",
    "treeview",
    "git",
    "radar",
    "timeline",
    "xychart",
    "quadrant",
    "sequence",
    "gantt",
    "block",
    "c4",
    "railroad",
    "info",
    "cynefin",
    "wardley",
    "venn",
    "packet",
    "eventmodeling",
    "treemap",
    "mindmap",
    "journey",
    "kanban",
    "ishikawa",
    "sankey",
    "across",
    "unlowered",
];
const WIDTH: f32 = 800.0;
const VIEWPORT: (f32, f32) = (WIDTH, 1000.0);

/// Pele's `mark`: the opacity of a drawing whose text since then cannot be drawn.
const PELE_OPACITY: f32 = 0.4;
/// Pele's `OVER_STYLE` at the view's default font size of 16: the error at 0.8 of it, in a line
/// 1.3 of its own size high.
const ERROR_LINE: f32 = 16.0 * 0.8 * 1.3;
/// Pele's error colour, `--pele-error`'s fallback, which is also the critical colour of
/// `Palette::pele()`.
const PELE_RED: [u8; 3] = [0xe4, 0x57, 0x56];

struct Viewer {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
    /// What the last view gave back, when it was an error.
    error: RefCell<Option<ViewError>>,
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let root = el().size(VIEWPORT.0, VIEWPORT.1).fill(Color::WHITE);
    match s.diagram.view(cx, &s.source, s.options.clone()) {
        Ok(drawn) => {
            s.error.replace(None);
            root.child(drawn)
        }
        Err(error) => {
            s.error.replace(Some(error));
            root
        }
    }
}

fn window<'a>(capturer: &'a Capturer, source: &str, options: ViewOptions, scale: f32) -> Window<'a, Viewer> {
    let viewer = Viewer {
        diagram: Diagram::new("diagram", FontCatalog::rune()),
        source: source.to_owned(),
        options,
        error: RefCell::new(None),
    };
    capturer.window(viewer, view, VIEWPORT, Env::new(false, scale))
}

fn fixture(kind: &str) -> String {
    let path = pele_oracle::root().join(format!("tests/fixtures/rune/{kind}.mmd"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn options() -> ViewOptions {
    ViewOptions::new(WIDTH).palette(Palette::pele())
}

/// pele-core's own error for `source` laid out as `options()` lays it out, without the view.
fn pele_core_error(source: &str) -> Option<RenderError> {
    let layout = LayoutOptions::default().max_width(f64::from(WIDTH)).id_prefix("pele");
    pele_core::parity_scene_with(source, ParseOptions::default(), &layout).err()
}

/// Pele's `summary` in `mount.ts`: a message's first and last lines that are not blank.
fn pele_summary(message: &str) -> String {
    let lines: Vec<&str> = message.lines().filter(|line| !line.trim().is_empty()).collect();
    if lines.len() > 1 { format!("{} {}", lines[0], lines[lines.len() - 1]) } else { lines.concat() }
}

fn write_report(name: &str, text: &str) {
    std::fs::create_dir_all(out_dir_for(RUN)).unwrap();
    std::fs::write(out_dir_for(RUN).join(name), text).unwrap();
}

fn element<'a>(elements: &'a [rune::runtime::ElementInfo], key: &str) -> Option<&'a rune::runtime::ElementInfo> {
    elements.iter().find(|element| element.key.as_deref() == Some(key))
}

/// The flowchart fixture with a last line that does not parse.
fn broken(source: &str) -> String {
    format!("{}\n    Z -->", source.trim_end())
}

#[cfg(feature = "fallback")]
#[test]
fn every_prefix_of_each_fixture_after_its_first_drawing_draws_one() {
    let listed: Vec<String> = FIXTURES.iter().map(|name| format!("{name}.mmd")).collect();
    let mut on_disk: Vec<String> = std::fs::read_dir(pele_oracle::root().join("tests/fixtures/rune"))
        .expect("the fixtures")
        .map(|entry| entry.expect("a fixture").file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".mmd"))
        .collect();
    on_disk.sort();
    let mut sorted = listed.clone();
    sorted.sort();
    assert_eq!(sorted, on_disk, "every fixture is streamed");
    let capturer = Capturer::new();
    let mut report = String::from(
        "Every character prefix of each fixture through one Diagram, as a source streams in, with KeptLook::Unchanged.\n\
         kept: frames that showed the last drawing that drew. run: the most kept frames in a row. pele-core: kept\n\
         frames whose error is pele-core's own for the prefix. lowering: kept frames whose source pele-core lays out\n\
         and the view neither lowers nor hands to rune-mermaid. measured: kept frames that shaped text, all while laying out\n\
         a source that parses and then fails. rune-mermaid: prefixes drawn by rune-mermaid, each listed below with why.\n\n\
         fixture       prefixes before drawn rune-mermaid kept  run pele-core lowering measured\n",
    );
    let (mut all_kept, mut longest) = (0, (0, ""));
    let mut by_mermaid_why = String::new();
    for kind in FIXTURES {
        let source = fixture(kind);
        let mut window = window(&capturer, "", options().kept_look(KeptLook::Unchanged), 1.0);
        let (mut before, mut drawn, mut by_mermaid, mut kept, mut run, mut most) = (0, 0, 0, 0, 0, 0);
        let (mut pele_core_errors, mut unlowered, mut measured) = (0, 0, 0);
        let mut last_good: Option<Pixels> = None;
        for (at, character) in source.char_indices() {
            let prefix = &source[..at + character.len_utf8()];
            let stats = window.state().diagram.stats();
            window.update(|viewer| viewer.source = prefix.to_owned());
            let pixels = window.capture();
            let diagram = &window.state().diagram;
            let returned = window.state().error.borrow().clone();
            match diagram.status() {
                RenderStatus::Pele | RenderStatus::RuneMermaid => {
                    assert_eq!((returned, diagram.error()), (None, None), "{kind} {prefix:?}: a source that draws");
                    if diagram.status() == RenderStatus::RuneMermaid {
                        by_mermaid += 1;
                        let line = prefix.lines().last().unwrap_or_default().trim_start();
                        writeln!(by_mermaid_why, "{kind} {line:?}: {:?}", diagram.fallback()).unwrap();
                    }
                    drawn += 1;
                    run = 0;
                    last_good = Some(pixels);
                }
                RenderStatus::Kept => {
                    let last =
                        last_good.as_ref().unwrap_or_else(|| panic!("{kind} {prefix:?}: kept with nothing drawn"));
                    assert_eq!(returned, None, "{kind} {prefix:?}: a kept frame is a drawing, not an error");
                    assert!(
                        pixels.rgba == last.rgba,
                        "{kind} {prefix:?}: the kept frame shows the last drawing that drew"
                    );
                    let now = diagram.stats();
                    assert_eq!(
                        (now.layouts, now.paints),
                        (stats.layouts, stats.paints),
                        "{kind} {prefix:?}: kept, laid out or painted"
                    );
                    match diagram.error().expect("a kept frame says why") {
                        ViewError::Core(error) => {
                            assert_eq!(
                                Some(error),
                                pele_core_error(prefix),
                                "{kind} {prefix:?}: pele-core's own error"
                            );
                            pele_core_errors += 1;
                        }
                        other => {
                            assert!(
                                matches!(other, ViewError::Geometry(_)) && pele_core_error(prefix).is_none(),
                                "{kind} {prefix:?}: {other:?}, which pele-core draws"
                            );
                            unlowered += 1;
                        }
                    }
                    if now.measurements != stats.measurements {
                        assert!(
                            pele_core::prepare(prefix, ParseOptions::default()).is_ok(),
                            "{kind} {prefix:?}: only laying out a source that parses measures"
                        );
                        measured += 1;
                    }
                    kept += 1;
                    run += 1;
                    most = most.max(run);
                }
                RenderStatus::Unrendered => {
                    assert!(last_good.is_none(), "{kind} {prefix:?}: nothing shown after a drawing");
                    let error = returned.unwrap_or_else(|| panic!("{kind} {prefix:?}: unrendered, yet no error"));
                    assert_eq!(diagram.error(), Some(error.clone()), "{kind} {prefix:?}: the error the view gave back");
                    if let ViewError::Core(error) = error {
                        assert_eq!(Some(error), pele_core_error(prefix), "{kind} {prefix:?}: pele-core's own error");
                    }
                    before += 1;
                }
                other => panic!("{kind} {prefix:?}: a status this test does not know: {other:?}"),
            }
        }
        let whole_native = window.state().diagram.status() == RenderStatus::Pele;
        let prefixes = source.chars().count();
        assert_eq!(drawn + kept + before, prefixes, "{kind}: every prefix is counted once");
        assert!(drawn > 0, "{kind}: some prefix draws");
        assert!(
            !whole_native || by_mermaid == 0,
            "{kind}: drawn natively whole, yet {by_mermaid} prefixes drawn by rune-mermaid:\n{by_mermaid_why}"
        );
        writeln!(
            report,
            "{kind:<13} {prefixes:>8} {before:>6} {drawn:>5} {by_mermaid:>12} {kept:>4} {most:>4} {pele_core_errors:>9} {unlowered:>8} {measured:>8}"
        )
        .unwrap();
        all_kept += kept;
        if most > longest.0 {
            longest = (most, kind);
        }
    }
    writeln!(report, "\n{all_kept} kept frames; the longest run is {} frames, in {}.", longest.0, longest.1).unwrap();
    writeln!(report, "\nPrefixes drawn by rune-mermaid, by their last line:\n{by_mermaid_why}").unwrap();
    print!("{report}");
    write_report("prefixes.txt", &report);
}

/// `ours` against `last` dimmed to `opacity` over white, per channel, outside `skip` (device
/// pixels as `[left, top, right, bottom]`): how many pixels are further apart than `levels`.
fn off_dimmed(ours: &Pixels, last: &Pixels, opacity: f32, skip: [u32; 4], levels: u8) -> (usize, usize) {
    let (mut compared, mut off) = (0, 0);
    for y in 0..ours.height {
        for x in 0..ours.width {
            if (skip[0]..skip[2]).contains(&x) && (skip[1]..skip[3]).contains(&y) {
                continue;
            }
            compared += 1;
            let (a, b) = (ours.at(x, y), last.at(x, y));
            let dimmed = |c: u8| (opacity * f32::from(c) + (1.0 - opacity) * 255.0).round() as u8;
            if (0..3).any(|c| a[c].abs_diff(dimmed(b[c])) > levels) {
                off += 1;
            }
        }
    }
    (compared, off)
}

#[test]
fn the_dimmed_look_fades_the_kept_drawing_as_one_layer_with_the_error_over_its_bottom_edge() {
    let capturer = Capturer::new();
    let scale = 2.0;
    let source = fixture("flowchart");
    let mut window = window(&capturer, &source, options(), scale);
    let last = window.capture();
    assert_eq!(window.state().diagram.status(), RenderStatus::Pele);
    let elements = window.runtime().elements();
    let drawing = element(&elements, "diagram").expect("the drawing").rect;

    let failing = broken(&source);
    window.update(|viewer| viewer.source = failing.clone());
    let kept = window.capture();
    common::save_in(RUN, &common::side_by_side(&last, &kept, 16), "dimmed@2x.png");
    let diagram = &window.state().diagram;
    assert_eq!(diagram.status(), RenderStatus::Kept);
    let error = diagram.error().expect("the source fails");
    let ViewError::Core(core) = &error else { panic!("{error:?}") };
    assert_eq!(Some(core.clone()), pele_core_error(&failing), "pele-core's own error");
    let message = error.to_string();

    let elements = window.runtime().elements();
    let line = element(&elements, "error").expect("the error over the drawing");
    assert_eq!(line.text.as_deref(), Some(pele_summary(&message).as_str()), "Pele's summary of the message");
    assert_eq!(line.help.as_deref(), Some(message.as_str()), "the whole message as the tooltip, as Pele's title");
    let [x, y, w, h] = line.rect;
    let bottom = drawing[1] + drawing[3];
    assert!((h - ERROR_LINE).abs() < 0.01, "the error's line is {h} high, for {ERROR_LINE}");
    assert!((y + h - bottom).abs() < 0.01, "the error's line ends at {}, the drawing at {bottom}", y + h);
    assert!((x - drawing[0]).abs() < 0.01 && w >= drawing[2] - 0.01, "the error spans the drawing: {:?}", line.rect);

    let device = |points: f32| (points * scale).round() as u32;
    let skip = [device(x), device(y), device(x + w), device(y + h)];
    let (compared, off) = off_dimmed(&kept, &last, PELE_OPACITY, skip, 2);
    let red = (skip[1]..skip[3])
        .flat_map(|py| (skip[0]..skip[2]).map(move |px| (px, py)))
        .filter(|&(px, py)| {
            let pixel = kept.at(px, py);
            (0..3).all(|c| pixel[c].abs_diff(PELE_RED[c]) <= 24)
        })
        .count();
    let report = format!(
        "last drawing at {drawing:?}, error line at {:?}\n{off} of {compared} px outside the error line are more than 2 levels from the last drawing at opacity {PELE_OPACITY} over white\n{red} px of the error line are within 24 levels of Pele's red {PELE_RED:?}\ntext {:?}\ntooltip {message:?}\n",
        line.rect, line.text
    );
    print!("{report}");
    write_report("dimmed.txt", &report);
    assert_eq!(off, 0, "the kept drawing is the last one faded as one layer to Pele's opacity");
    assert!(red > 100, "the error is drawn in Pele's red: {red} px");
}

#[test]
fn the_unchanged_look_shows_the_kept_drawing_as_it_was() {
    let capturer = Capturer::new();
    let source = fixture("flowchart");
    let mut window = window(&capturer, &source, options().kept_look(KeptLook::Unchanged), 2.0);
    let last = window.capture();
    let paths = |window: &Window<'_, Viewer>| -> Vec<String> {
        window.runtime().elements().into_iter().map(|element| element.path).collect()
    };
    let drawn = paths(&window);
    window.update(|viewer| viewer.source = broken(&source));
    let kept = window.capture();
    let diagram = &window.state().diagram;
    assert_eq!(diagram.status(), RenderStatus::Kept);
    assert!(matches!(diagram.error(), Some(ViewError::Core(_))), "{:?}", diagram.error());
    assert!(kept.rgba == last.rgba, "the kept frame is the last drawing, pixel for pixel");
    assert_eq!(paths(&window), drawn, "the same elements, under the same key paths");
}

#[cfg(feature = "fallback")]
#[test]
fn a_source_that_falls_back_to_rune_mermaid_and_then_fails_keeps_that_drawing() {
    let capturer = Capturer::new();
    let source = fixture("unlowered");
    let mut window = window(&capturer, &source, options().kept_look(KeptLook::Unchanged), 2.0);
    let last = window.capture();
    let why = window.state().diagram.fallback();
    assert_eq!(window.state().diagram.status(), RenderStatus::RuneMermaid);
    assert!(matches!(why, Some(Fallback::NotLowered(_))), "{why:?}");
    window.update(|viewer| viewer.source = broken(&source));
    let kept = window.capture();
    common::save_in(RUN, &common::side_by_side(&last, &kept, 16), "rune-mermaid-kept@2x.png");
    let diagram = &window.state().diagram;
    assert_eq!(diagram.status(), RenderStatus::Kept);
    assert_eq!(diagram.fallback(), why, "the kept drawing is still rune-mermaid's, for the same reason");
    assert!(matches!(diagram.error(), Some(ViewError::Core(_))), "{:?}", diagram.error());
    assert!(kept.rgba == last.rgba, "the kept frame is rune-mermaid's drawing, pixel for pixel");
}

fn changed(a: &Pixels, b: &Pixels) -> usize {
    a.rgba.chunks(4).zip(b.rgba.chunks(4)).filter(|(p, q)| p != q).count()
}

fn inked(pixels: &Pixels) -> usize {
    pixels.rgba.chunks(4).filter(|px| px[..3] != [255, 255, 255]).count()
}

#[cfg(feature = "fallback")]
#[test]
fn a_palette_or_dark_mode_change_repaints_the_kept_drawing_without_laying_out() {
    let capturer = Capturer::new();
    let source = fixture("flowchart");
    let mut report = String::new();
    // In the window's theme, so that dark mode recolours it too.
    let mut window = window(&capturer, &source, ViewOptions::new(WIDTH), 2.0);
    window.capture();
    window.update(|viewer| viewer.source = broken(&source));
    let light = window.capture();
    let before = window.state().diagram.stats();
    assert_eq!(window.state().diagram.status(), RenderStatus::Kept);

    window.set_dark(true);
    let dark = window.capture();
    let after_dark = window.state().diagram.stats();
    window.set_dark(false);
    window.update(|viewer| viewer.options = viewer.options.clone().palette(Palette::pele()));
    let pele = window.capture();
    let after_palette = window.state().diagram.stats();
    assert_eq!(window.state().diagram.status(), RenderStatus::Kept, "still kept");
    writeln!(report, "kept, light: {before:?}").unwrap();
    writeln!(report, "kept, dark: {after_dark:?}, {} px changed of {} drawn", changed(&light, &dark), inked(&light))
        .unwrap();
    writeln!(report, "kept, Pele's palette: {after_palette:?}, {} px changed", changed(&light, &pele)).unwrap();
    for (what, after) in [("dark mode", after_dark), ("a palette", after_palette)] {
        assert_eq!((after.layouts, after.measurements), (before.layouts, before.measurements), "{what}: no layout");
    }
    assert_eq!(after_dark.paints, before.paints + 1, "dark mode paints once more");
    assert_eq!(after_palette.paints, after_dark.paints + 1, "a palette paints once more");
    assert!(2 * changed(&light, &dark) > inked(&light), "dark mode recolours the drawing");
    assert!(changed(&light, &pele) > 0, "the palette recolours the drawing");

    // rune-mermaid has no colours to change without drawing again.
    let mermaid = fixture("unlowered");
    let mut by_mermaid = self::window(&capturer, &mermaid, options().kept_look(KeptLook::Unchanged), 2.0);
    by_mermaid.capture();
    by_mermaid.update(|viewer| viewer.source = broken(&mermaid));
    let light = by_mermaid.capture();
    // The source is laid out once, on its first frame, before its lowering refuses it.
    let laid_out = by_mermaid.state().diagram.stats().layouts;
    by_mermaid.set_dark(true);
    let dark = by_mermaid.capture();
    assert_eq!(by_mermaid.state().diagram.status(), RenderStatus::Kept);
    assert_eq!(
        by_mermaid.state().diagram.stats().layouts,
        laid_out,
        "rune-pele lays out nothing for rune-mermaid's drawing in dark mode"
    );
    writeln!(
        report,
        "kept rune-mermaid drawing, dark: {} px changed of {} drawn",
        changed(&light, &dark),
        inked(&light)
    )
    .unwrap();
    assert!(2 * changed(&light, &dark) > inked(&light), "dark mode recolours rune-mermaid's drawing");
    print!("{report}");
    write_report("theme-change.txt", &report);
}

#[test]
fn with_no_drawing_yet_or_with_options_refused_the_view_is_an_error() {
    let capturer = Capturer::new();
    let source = fixture("flowchart");
    let failing = broken(&source);
    let mut window = window(&capturer, &failing, options(), 1.0);
    window.capture();
    let diagram = &window.state().diagram;
    let returned = window.state().error.borrow().clone();
    assert!(matches!(returned, Some(ViewError::Core(_))), "{returned:?}");
    assert_eq!((diagram.status(), diagram.error()), (RenderStatus::Unrendered, returned));

    window.update(|viewer| viewer.source = source.clone());
    window.capture();
    assert_eq!(window.state().diagram.status(), RenderStatus::Pele);
    window.update(|viewer| viewer.options = viewer.options.clone().font_size(-1.0));
    window.capture();
    let diagram = &window.state().diagram;
    let returned = window.state().error.borrow().clone();
    assert_eq!(returned, Some(ViewError::Option("font size")), "a refused option stays an error");
    assert_eq!((diagram.status(), diagram.error()), (RenderStatus::Unrendered, returned));
    assert_eq!((diagram.scene().is_none(), diagram.fallback()), (true, None), "nothing is shown");

    window.update(|viewer| viewer.source = failing.clone());
    window.capture();
    let diagram = &window.state().diagram;
    assert!(matches!(window.state().error.borrow().clone(), Some(ViewError::Option(_))), "still the option's error");
    assert_eq!(diagram.status(), RenderStatus::Unrendered);
    window.update(|viewer| viewer.options = options());
    window.capture();
    let diagram = &window.state().diagram;
    assert_eq!(*window.state().error.borrow(), None, "with good options, the failing source keeps the drawing");
    assert_eq!(diagram.status(), RenderStatus::Kept);
    assert!(diagram.scene().is_some(), "the kept drawing's Scene");
}

#[test]
fn a_source_that_draws_replaces_the_kept_drawing_at_once() {
    let capturer = Capturer::new();
    let source = fixture("flowchart");
    let first = "flowchart TD\n    A[Checkout] --> B{Cached?}";
    let keys = |window: &Window<'_, Viewer>| -> Vec<String> {
        let mut keys: Vec<String> = window.runtime().elements().into_iter().filter_map(|element| element.key).collect();
        keys.sort();
        keys
    };
    let mut fresh = window(&capturer, &source, options(), 2.0);
    let whole = fresh.capture();
    for look in [KeptLook::DimmedWithError, KeptLook::Unchanged] {
        let mut window = window(&capturer, first, options().kept_look(look), 2.0);
        window.capture();
        assert_eq!(window.state().diagram.status(), RenderStatus::Pele, "{look:?}: the first lines draw");
        window.update(|viewer| viewer.source = broken(first));
        window.capture();
        assert_eq!(window.state().diagram.status(), RenderStatus::Kept, "{look:?}");
        window.update(|viewer| viewer.source = source.clone());
        let next = window.capture();
        let diagram = &window.state().diagram;
        assert_eq!((diagram.status(), diagram.error()), (RenderStatus::Pele, None), "{look:?}: the new source drew");
        assert!(next.rgba == whole.rgba, "{look:?}: the new drawing, as a view that kept nothing draws it");
        assert_eq!(keys(&window), keys(&fresh), "{look:?}: the new drawing's keys, and no error over it");
    }
}
