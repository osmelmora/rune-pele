//! The colours a source sets in its own theme config, through rune-pele's public API: they win
//! in Pele's own palette, as in Pele, and an app's palette wins over them. Also how the
//! side-by-side reads those colours from Pele's SVG, and a git commit's tooltip.

mod common;

use common::{as_a_page_reads, rasterise};
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels, Window};
use rune_pele::{Diagram, FontCatalog, Palette, RenderStatus, ViewOptions};

const SIZE: (f32, f32) = (480.0, 280.0);

/// A git graph whose theme sets its first two branches' colours, as git 28 of Pele's corpus does,
/// and one of whose commits has a message.
const THEMED_GIT: &str = "---\nconfig:\n  themeVariables:\n    git0: '#ff0000'\n    git1: '#00ff00'\n---\n\
gitGraph\n    commit id: \"init\" msg: \"Start the project\"\n    branch develop\n    commit id: \"api\"\n    \
checkout main\n    commit id: \"docs\"\n";

struct Viewer {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let drawn = s.diagram.view(cx, &s.source, s.options.clone()).expect("the source draws");
    el().size(SIZE.0, SIZE.1).fill(Color::WHITE).child(drawn)
}

fn window<'a>(capturer: &'a Capturer, source: &str, options: ViewOptions) -> Window<'a, Viewer> {
    let viewer = Viewer { diagram: Diagram::new("diagram", FontCatalog::rune()), source: source.to_owned(), options };
    capturer.window(viewer, view, SIZE, Env::new(false, 1.0))
}

/// The element keyed `key`, as `[x, y, width, height]` in points.
fn rect_of(window: &Window<'_, Viewer>, key: &str) -> [f32; 4] {
    let elements = window.runtime().elements();
    elements.into_iter().find(|element| element.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect
}

fn srgb8(r: f32, g: f32, b: f32) -> Color {
    let [r, g, b] = [r, g, b].map(|c| rune::color::srgb_decode(c / 255.0));
    Color { r, g, b, a: 1.0 }
}

/// Whether `pixel` is `expected` to within a level in each channel, as two roundings of one
/// colour can differ.
fn is(pixel: [u8; 4], expected: [u8; 3]) -> bool {
    (0..3).all(|c| pixel[c].abs_diff(expected[c]) <= 1)
}

#[test]
fn a_page_reads_a_variable_the_root_declares_and_the_fallback_of_any_other() {
    // Pele's spelling: private aliases and a source's series colours on the root, each shape's
    // colour a variable with Pele's own colour as its fallback.
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 30 10\" width=\"30\" height=\"10\" \
               style=\"--_bg:var(--pele-bg,#fff);font:16px sans-serif;--pele-series-1:#ff0000\">\
               <rect width=\"10\" height=\"10\" fill=\"var(--pele-series-1,#4c78a8)\"/>\
               <rect x=\"10\" width=\"10\" height=\"10\" fill=\"var(--pele-series-2,#f58518)\"/>\
               <rect x=\"20\" width=\"10\" height=\"10\" fill=\"#000\"/>\
               <rect x=\"20\" width=\"10\" height=\"10\" fill=\"var(--_bg)\"/></svg>";
    let page = rasterise(&as_a_page_reads(svg), 1.0);
    assert_eq!(page.at(5, 5), [255, 0, 0, 255], "a series the root declares is in the declared colour");
    assert_eq!(page.at(15, 5), [0xf5, 0x85, 0x18, 255], "a series the root leaves alone is in its fallback");
    assert_eq!(page.at(25, 5), [255, 255, 255, 255], "a private alias reads through to its variable's fallback");
}

#[test]
fn a_source_s_theme_colours_win_in_pele_s_palette_and_an_app_s_palette_wins_over_them() {
    let capturer = Capturer::new();
    let mut window = window(&capturer, THEMED_GIT, ViewOptions::natural().palette(Palette::pele()));
    let main_label = |window: &mut Window<'_, Viewer>| -> [u8; 4] {
        let pixels: Pixels = window.capture();
        assert_eq!(window.state().diagram.status(), RenderStatus::Pele, "git is drawn natively");
        // Inside the main branch's label, left of its text, half way down.
        let [x, y, _, height] = rect_of(window, "group:main/label");
        pixels.at((x + 3.0) as u32, (y + height / 2.0) as u32)
    };

    let pele = main_label(&mut window);
    assert!(is(pele, [255, 0, 0]), "Pele's palette: the source's git0 wins, {pele:?}");

    let app_green = [0x00, 0xaa, 0x55];
    let app = Palette::pele().with_series(0, srgb8(0.0, 170.0, 85.0));
    window.update(|viewer| viewer.options = viewer.options.clone().palette(app));
    let app_palette = main_label(&mut window);
    assert!(is(app_palette, app_green), "an app's palette: its own first series wins, {app_palette:?}");

    window.update(|viewer| viewer.options = ViewOptions::natural());
    let themed = main_label(&mut window);
    assert!(is(themed, [0x4c, 0x78, 0xa8]), "the window's theme: its first series, Pele's blue, {themed:?}");

    let stats = window.state().diagram.stats();
    assert_eq!((stats.layouts, stats.paints), (1, 3), "each palette repaints, and nothing is laid out again");
    println!("main label in Pele's palette {pele:?}, an app's palette {app_palette:?}, the window's theme {themed:?}");
}

#[test]
fn a_git_commit_with_a_message_shows_it_where_the_pointer_rests() {
    let capturer = Capturer::new();
    let mut window = window(&capturer, THEMED_GIT, ViewOptions::natural().palette(Palette::pele()));
    window.capture();
    let elements = window.runtime().elements();
    // A shape that holds a tooltip is found in a row of boxes along it, each carrying the text.
    let helps: std::collections::BTreeSet<&str> =
        elements.iter().filter_map(|element| element.help.as_deref()).collect();
    assert_eq!(helps, ["Start the project"].into(), "the one commit with a message carries it");
    let commit = elements.iter().find(|element| element.help.is_some()).expect("found above").rect;
    let docs = rect_of(&window, "node:docs/shape");

    window.rest(commit[0] + commit[2] / 2.0, commit[1] + commit[3] / 2.0, 2.0);
    let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
    window.rest(docs[0] + docs[2] / 2.0, docs[1] + docs[3] / 2.0, 2.0);
    let elsewhere = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
    println!("tooltip on init: {shown:?}; on docs, which has no message: {elsewhere:?}");
    assert_eq!(shown.as_deref(), Some("Start the project"));
    assert_eq!(elsewhere, None);
}
