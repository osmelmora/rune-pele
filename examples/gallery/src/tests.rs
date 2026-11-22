use std::collections::BTreeSet;

use rune::{Env, Mods, Runtime};
use rune_pele::pele_core::{DiagramKind, ParseOptions, prepare};

use super::*;

const WINDOW: (f32, f32) = (1280.0, 820.0);

fn gallery() -> Runtime<Home> {
    App::new(spec()).playground(playground()).headless_runtime(WINDOW, Env::new(false, 2.0))
}

fn fixture_shown(rt: &Runtime<Home>) -> &Shown<&'static Fixture> {
    rt.playground_state().expect("a fixture's story is open")
}

fn editor(rt: &Runtime<Home>) -> &Shown<TextEdit> {
    rt.playground_state().expect("the editor is open")
}

fn shown_word(rt: &Runtime<Home>) -> Option<String> {
    rt.playground_element("status-word").and_then(|element| element.text)
}

fn press(rt: &mut Runtime<Home>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    rt.frame(1.0 / 60.0);
}

#[test]
fn every_file_of_the_rune_fixtures_has_a_story() {
    let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/rune");
    let files: BTreeSet<String> = std::fs::read_dir(folder)
        .expect("the fixtures folder")
        .map(|entry| format!("rune/{}", entry.expect("an entry").file_name().to_string_lossy()))
        .filter(|file| file.ends_with(".mmd"))
        .collect();
    let shown: BTreeSet<String> =
        FIXTURES.iter().map(|fixture| fixture.file.to_owned()).filter(|file| file.starts_with("rune/")).collect();
    assert_eq!(shown, files);
}

#[test]
fn every_type_of_pele_has_a_story() {
    let kinds: BTreeSet<&str> = FIXTURES
        .iter()
        .map(|fixture| prepare(fixture.source, ParseOptions::default()).expect(fixture.file).kind().name())
        .collect();
    let all: BTreeSet<&str> = DiagramKind::ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(kinds, all);
}

#[test]
fn every_story_draws_natively_but_the_one_that_falls_back() {
    let mut rt = gallery();
    for fixture in &FIXTURES {
        rt.open_playground(fixture.path, None);
        let falls_back = fixture.file == "rune/unlowered.mmd";
        let (status, word) =
            if falls_back { (RenderStatus::RuneMermaid, "Fallback") } else { (RenderStatus::Pele, "Drawn") };
        assert_eq!(fixture_shown(&rt).diagram.status(), status, "{}", fixture.path);
        assert_eq!(shown_word(&rt).as_deref(), Some(word), "{}", fixture.path);
    }
}

#[test]
fn the_editor_draws_what_is_typed_and_keeps_the_last_drawing_while_it_fails() {
    let mut rt = gallery();
    rt.open_playground(EDITOR, None);
    assert_eq!(editor(&rt).diagram.status(), RenderStatus::Pele, "the starter draws");

    let field = rt.playground_element("source").expect("the field").rect;
    let (x, y) = (field[0] + field[2] / 2.0, field[1] + field[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    press(&mut rt, Key::Command('a'), Mods::default());
    rt.paste("flowchart LR\n    A[Typed] --> B[Drawn]\n    B -->");
    rt.frame(1.0 / 60.0);
    assert_eq!(editor(&rt).diagram.status(), RenderStatus::Kept, "a source that fails keeps the starter");
    assert!(editor(&rt).diagram.error().is_some(), "and says why it fails");
    assert_eq!(shown_word(&rt).as_deref(), Some("Kept"));

    press(&mut rt, Key::Backspace, Mods::default());
    press(&mut rt, Key::Backspace, Mods::default());
    press(&mut rt, Key::Backspace, Mods::default());
    press(&mut rt, Key::Backspace, Mods::default());
    assert_eq!(editor(&rt).source.text, "flowchart LR\n    A[Typed] --> B[Drawn]\n    B");
    assert_eq!(editor(&rt).diagram.status(), RenderStatus::Pele, "the typed source draws once it parses");
    assert_eq!(shown_word(&rt).as_deref(), Some("Drawn"));
}

#[test]
fn the_editors_b_breaks_the_source_and_s_streams_one_in() {
    let mut rt = gallery();
    rt.open_playground(EDITOR, None);
    rt.playground_trigger('b');
    assert_eq!(editor(&rt).diagram.status(), RenderStatus::Kept);

    rt.playground_trigger('s');
    let mut kept = 0;
    while editor(&rt).source.text.len() < STREAMED.len() {
        rt.frame(CHUNK_EVERY);
        kept += usize::from(editor(&rt).diagram.status() == RenderStatus::Kept);
    }
    rt.frame(1.0 / 60.0);
    assert_eq!(editor(&rt).source.text, STREAMED);
    assert_eq!(editor(&rt).diagram.status(), RenderStatus::Pele, "the whole diagram draws");
    assert!(kept > 0, "a part that does not parse keeps the last that did");
}

#[test]
fn the_side_columns_keys_step_the_zoom_and_the_theme() {
    let mut rt = gallery();
    rt.open_playground("Graphs/Flowchart", None);
    let before = fixture_shown(&rt).diagram.zoom().expect("drawn natively");
    // → takes the keyboard to the side column, and → again steps the zoom up.
    press(&mut rt, Key::Right, Mods::default());
    press(&mut rt, Key::Right, Mods::default());
    assert_eq!(fixture_shown(&rt).look.zoom(), 1.25);
    let after = fixture_shown(&rt).diagram.zoom().expect("drawn natively");
    assert!((after / before - 1.25).abs() < 1e-4, "drawn 1.25 times as large: {before} then {after}");

    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Right, Mods::default());
    assert_eq!(fixture_shown(&rt).look.appearance, Appearance::Light);
    press(&mut rt, Key::Right, Mods::default());
    press(&mut rt, Key::Right, Mods::default());
    assert_eq!(fixture_shown(&rt).look.appearance, Appearance::Dark, "the last one stays");
}
