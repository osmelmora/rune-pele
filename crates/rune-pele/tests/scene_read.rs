//! What the Rune view reads of a Scene beyond its drawings: the text declarations of a style, the
//! declarations it cannot draw, and the tooltip of a group.

use pele_core::scene::{BaselineRun, Geometry, Group, LineCap, PaintVisit, Scene};
use pele_core::{FontFamily, FontWeight};

fn scene(source: &str) -> Scene {
    pele_core::parity_scene(source).unwrap_or_else(|error| panic!("{source:?}: {error}")).scene().clone()
}

fn run(scene: &Scene, text: &str) -> BaselineRun {
    let runs = scene.paint().filter_map(|visit| match visit {
        PaintVisit::Draw(draw) => match draw.geometry {
            Geometry::Text(run) => Some(run),
            _ => None,
        },
        _ => None,
    });
    runs.into_iter().find(|run| run.text == text).unwrap_or_else(|| panic!("no run {text:?}"))
}

fn groups(scene: &Scene) -> Vec<Group> {
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Begin(group) => Some(group),
            _ => None,
        })
        .collect()
}

#[test]
fn a_text_declaration_in_a_style_is_what_the_run_is_set_in() {
    let styled = scene(
        "flowchart LR\n    A[Marked] --> B[Plain] --> C[Code]\n    \
         style A font-weight:bold,font-style:italic,text-decoration:underline\n    style C font-family:monospace",
    );
    let marked = run(&styled, "Marked");
    assert_eq!((marked.font.weight, marked.font.italic, marked.underline), (FontWeight::Bold, true, true));
    let plain = run(&styled, "Plain");
    assert_eq!((plain.font.weight, plain.font.italic, plain.underline), (FontWeight::Normal, false, false));
    assert_eq!(plain.font.family, FontFamily::Sans);
    assert_eq!(run(&styled, "Code").font.family, FontFamily::Mono);
    assert_eq!(styled.unread_styles(), Vec::<String>::new(), "every declaration here is drawn");

    // A declaration wins over what the text element says itself: an entity's style reaches
    // the cells of its table, which are plain without it.
    let table = "erDiagram\n    CUSTOMER {\n        string name\n    }";
    assert_eq!(run(&scene(table), "name").font.weight, FontWeight::Normal);
    let bold = scene(&format!("{table}\n    style CUSTOMER font-weight:bold"));
    assert_eq!(run(&bold, "name").font.weight, FontWeight::Bold);
}

#[test]
fn a_line_cap_in_a_style_is_the_strokes_cap() {
    let capped = scene("flowchart LR\n    A --> B\n    style A stroke-linecap:round");
    let caps: Vec<LineCap> = capped
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) if draw.key.to_string() == "node:A/shape" => {
                draw.style.stroke.map(|stroke| stroke.cap)
            }
            _ => None,
        })
        .collect();
    assert_eq!(caps, [LineCap::Round]);
    assert_eq!(capped.unread_styles(), Vec::<String>::new());
}

#[test]
fn a_declaration_only_a_page_can_apply_is_listed_as_unread() {
    let unread = |style: &str| scene(&format!("flowchart LR\n    A --> B\n    style A {style}")).unread_styles();
    assert_eq!(
        unread("fill:#036,stroke:blue,stroke-width:2px,opacity:0.5,stroke-dasharray:3 4,font-size:20px"),
        Vec::<String>::new()
    );
    assert_eq!(unread("fill:inherit"), ["fill:inherit"]);
    assert_eq!(unread("stroke:currentcolor"), ["stroke:currentcolor"]);
    assert_eq!(unread("font-family:Georgia"), ["font-family:Georgia"]);
    assert_eq!(unread("text-decoration:line-through"), ["text-decoration:line-through"]);
    assert_eq!(unread("stroke-dashoffset:4"), ["stroke-dashoffset:4"]);
    assert_eq!(unread("stroke-dashoffset:0"), Vec::<String>::new(), "an offset of nothing changes nothing");
    assert_eq!(unread("stroke-linejoin:bevel"), ["stroke-linejoin:bevel"]);
    assert_eq!(unread("font-size:1.5em"), ["font-size:1.5em"]);
    assert_eq!(unread("letter-spacing:2px"), ["letter-spacing:2px"]);
}

#[test]
fn a_tooltip_is_on_the_group_pele_writes_it_in() {
    let linked =
        scene("flowchart LR\n    A[Home] --> B[Away]\n    click A \"https://example.com/a\" \"Go home &amp; rest\"");
    let with_tips: Vec<Group> = groups(&linked).into_iter().filter(|group| group.tooltip.is_some()).collect();
    let [group] = &with_tips[..] else { panic!("one group with a tooltip: {with_tips:?}") };
    assert_eq!(group.tooltip.as_deref(), Some("Go home & rest"));
    assert_eq!(group.link.as_ref().map(|link| link.href.as_str()), Some("https://example.com/a"), "the link holds it");

    // A tooltip with no link is on the node's own group.
    let tipped =
        scene("stateDiagram-v2\n    Idle --> Busy\n    click Idle \"javascript:void(0)\" \"Waiting for work\"");
    let tips: Vec<(Option<String>, bool)> = groups(&tipped)
        .into_iter()
        .filter(|group| group.tooltip.is_some())
        .map(|group| (group.tooltip, group.link.is_some()))
        .collect();
    assert_eq!(tips, [(Some("Waiting for work".to_owned()), false)], "the address is not one a link may have");
}
