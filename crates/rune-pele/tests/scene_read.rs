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

#[test]
fn a_gradient_is_on_the_shape_it_fills_from_one_paint_to_the_other() {
    use pele_core::scene::{Draw, LinearGradient, Paint, Point, SeriesSlot};
    use pele_core::{LayoutOptions, ParseOptions};
    let source = include_str!("../../../tests/fixtures/rune/sankey.mmd");
    let links = |options: LayoutOptions| -> Vec<Draw> {
        let scene = pele_core::parity_scene_with(source, ParseOptions::default(), &options).expect("a sankey");
        let draws = scene.scene().paint().filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => Some(draw),
            _ => None,
        });
        draws.filter(|draw| draw.key.to_string().starts_with("edge:")).collect()
    };
    // Pele runs a link's fill along a gradient only where the drawing has an id prefix to name
    // the gradient by. Without one a link is in its source's colour.
    let plain = links(LayoutOptions::default());
    assert_eq!(plain.len(), 6);
    assert!(plain.iter().all(|link| link.style.gradient.is_none()));

    let graded = links(LayoutOptions { id_prefix: Some("view".to_owned()), ..LayoutOptions::default() });
    let gas = graded.iter().find(|link| link.key.to_string() == "edge:Gas~Power~link/shape").expect("the link");
    let Geometry::Path(commands) = &gas.geometry else { panic!("a link is a path") };
    let xs = commands.iter().filter_map(|command| match command {
        pele_core::scene::PathCommand::Move(p) | pele_core::scene::PathCommand::Line(p) => Some(p.x),
        _ => None,
    });
    let xs: Vec<f64> = xs.collect();
    // Nodes are numbered as the source first names them: Coal, Power, Gas. The gradient runs
    // from Gas's colour where the link leaves it to Power's where the link arrives, level.
    let at = |x: f64| Point { x, y: 0.0 };
    let stops = [Paint::Series(SeriesSlot::new(2)), Paint::Series(SeriesSlot::new(1))];
    assert_eq!(gas.style.gradient, Some(LinearGradient { from: at(xs[0]), to: at(xs[1]), stops }));
    assert_eq!(gas.style.fill, stops[0], "the fill is what is drawn where the gradient cannot be");
    assert!(xs[0] < xs[1]);
    assert!(graded.iter().all(|link| link.style.gradient.is_some()), "every link has its own");
}
