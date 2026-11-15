//! Swimlane diagrams, ported from Pele's `src/diagrams/swimlane`. A swimlane diagram is a
//! flowchart whose top-level subgraphs are lanes: the same syntax and model, laid out by
//! `layout::lane_layout`, with each lane drawn as a band the length of the diagram.

use super::flowchart::{self, FlowGraph, FlowVariant, FlowchartModel, GraphNode, LabelType, ShapeName};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::JsText;
use crate::layout::lanes::LANE_PAD;
use crate::layout::{CNode, Dir, lane_layout};
use crate::measure::Label;
use crate::scene::{
    AuthorClasses, Facet, LabelLook, LayerSpec, Number, Paint, PartId, Point, ResolvedStyle, Role, Scene, SceneBuilder,
    ink_on,
};
use crate::{Diagnostic, DiagramKind, RenderError};

const DEFAULT_LANE: &str = "__swimlane_default__";

/// Nodes outside every lane share one unnamed lane, as in Mermaid. Lanes all run the same way, so
/// a `direction` inside a subgraph has no effect.
fn prepare(graph: &mut FlowGraph) {
    let mut loose = false;
    for node in &mut graph.nodes {
        if node.is_group {
            node.dir = None;
        } else if node.parent_id.is_none() {
            loose = true;
        }
    }
    if !loose {
        return;
    }
    for node in &mut graph.nodes {
        if !node.is_group && node.parent_id.is_none() {
            node.parent_id = Some(DEFAULT_LANE.into());
        }
    }
    graph.nodes.push(GraphNode {
        id: DEFAULT_LANE.into(),
        label: JsText::default(),
        label_type: LabelType::Text,
        shape: ShapeName::RECT,
        is_group: true,
        parent_id: None,
        css_styles: Vec::new(),
        css_compiled_styles: Vec::new(),
        css_classes: JsText::default(),
        dir: None,
        link: None,
        link_target: None,
        tooltip: None,
        icon: None,
        pos: None,
        img: None,
        asset_width: None,
        asset_height: None,
        constraint: None,
        color_index: None,
        metadata: None,
    });
}

fn sideways(dir: Dir) -> bool {
    matches!(dir, Dir::LR | Dir::RL)
}

/// A lane: an outline the length of the diagram, with its title in a band at the start.
fn draw_lane(
    out: &mut SceneBuilder,
    part: PartId,
    c: &CNode,
    label: &Label,
    style: &ResolvedStyle,
    classes: &AuthorClasses,
    dir: Dir,
) {
    let left = c.x - c.w / 2.0;
    let top = c.y - c.h / 2.0;
    let sideways = sideways(dir);
    let at =
        if sideways { Point { x: left + c.pad_top / 2.0, y: c.y } } else { Point { x: c.x, y: top + c.pad_top / 2.0 } };
    let ink = ink_on(style.shape.as_ref(), style.text.as_ref()).unwrap_or(Role::Muted);
    let look = || LabelLook::class("pele-cluster-label").fill(Some(Paint::Role(ink))).style(style.text.as_ref());
    let origin = [Number::num(left), Number::num(top)];
    let group = LayerSpec::classes("pele-cluster pele-lane", classes).part(part, Facet::Shape).data_id();
    out.layer(group, |out| {
        let outline = out.element(part, Facet::Shape).rect(origin, [Number::num(c.w), Number::num(c.h)], None);
        outline.fill(Paint::None).stroke(Paint::Role(Role::Border)).finish();
        let band = if sideways { [c.pad_top, c.h] } else { [c.w, c.pad_top] };
        let band = out.element(part, Facet::Shape).rect(origin, band.map(Number::num), None);
        band.fill(Paint::Role(Role::SurfaceAlt)).stroke(Paint::Role(Role::Border)).style(style.shape.clone()).finish();
        if sideways {
            out.layer(LayerSpec::group().rotate(-90.0, at), |out| out.label(part, Facet::Title, label, at, look()));
        } else {
            out.label(part, Facet::Title, label, at, look());
        }
    });
}

/// The band of a lane that takes its fill, as left, top, right and bottom.
fn lane_band(c: &CNode, dir: Dir) -> [f64; 4] {
    let left = c.x - c.w / 2.0;
    let top = c.y - c.h / 2.0;
    if sideways(dir) { [left, top, left + c.pad_top, top + c.h] } else { [left, top, left + c.w, top + c.pad_top] }
}

const LANES: FlowVariant = FlowVariant {
    section: "swimlane",
    prepare,
    layout: |nodes, edges, dir, options, tight| {
        lane_layout(nodes, edges, dir, options, if tight { 12.0 } else { LANE_PAD })
    },
    draw_group: draw_lane,
    filled_box: lane_band,
};

pub(crate) struct Swimlane;

impl Diagram for Swimlane {
    type Model = FlowchartModel;

    /// The flowchart parser, whose errors name the flowchart as Pele's do.
    fn parse(cx: &ParseContext<'_>) -> Result<FlowchartModel, RenderError> {
        flowchart::parse(cx).map_err(|error| match error {
            RenderError::Diagnostic(diagnostic) => RenderError::Diagnostic(Diagnostic {
                kind: diagnostic.kind.or(Some(DiagramKind::Flowchart)),
                ..diagnostic
            }),
            other => other,
        })
    }

    fn scene(model: &FlowchartModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        flowchart::render_flowchart(model, cx, Some(&LANES))
    }
}
