//! Pele's `buildWardley` and `WardleyBuilder` (`model.ts`, `builder.ts`): the syntax tree as the
//! map's nodes, links and marks, with coordinates as percentages.

use super::parser::{LabelAst, PlacedAst, WardleyAst};
use crate::input::{JsText, Units, is_space};
use crate::js;
use crate::scene::Accessible;
use indexmap::IndexMap;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Strategy {
    Build,
    Buy,
    Outsource,
    Market,
}

impl Strategy {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Strategy::Build => "build",
            Strategy::Buy => "buy",
            Strategy::Outsource => "outsource",
            Strategy::Market => "market",
        }
    }

    fn of(text: &[u16]) -> Strategy {
        [Strategy::Build, Strategy::Buy, Strategy::Outsource, Strategy::Market]
            .into_iter()
            .find(|strategy| text.is_str(strategy.name()))
            .expect("a STRATEGY token names a strategy")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Forward,
    Backward,
    Bidirectional,
}

impl Flow {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Flow::Forward => "forward",
            Flow::Backward => "backward",
            Flow::Bidirectional => "bidirectional",
        }
    }
}

/// Pele's `className` of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeClass {
    Anchor,
    Component,
    PipelineComponent,
}

impl NodeClass {
    pub(crate) fn name(self) -> &'static str {
        match self {
            NodeClass::Anchor => "anchor",
            NodeClass::Component => "component",
            NodeClass::PipelineComponent => "pipeline-component",
        }
    }
}

/// A node, at `x` along evolution and `y` up visibility, both percentages.
#[derive(Clone, Debug)]
pub(crate) struct WardleyNode {
    pub(crate) id: JsText,
    pub(crate) label: JsText,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) class: NodeClass,
    pub(crate) label_offset_x: Option<f64>,
    pub(crate) label_offset_y: Option<f64>,
    pub(crate) in_pipeline: bool,
    pub(crate) is_pipeline_parent: bool,
    pub(crate) inertia: Option<bool>,
    pub(crate) strategy: Option<Strategy>,
}

pub(crate) struct WardleyLink {
    pub(crate) source: JsText,
    pub(crate) target: JsText,
    pub(crate) dashed: bool,
    pub(crate) label: Option<JsText>,
    pub(crate) flow: Option<Flow>,
}

pub(crate) struct Trend {
    pub(crate) node_id: JsText,
    pub(crate) target_x: f64,
    pub(crate) target_y: f64,
}

pub(crate) struct Pipeline {
    pub(crate) node_id: JsText,
    pub(crate) component_ids: Vec<JsText>,
}

pub(crate) struct Annotation {
    pub(crate) number: f64,
    pub(crate) at: (f64, f64),
    pub(crate) text: JsText,
}

/// A note, an accelerator or a deaccelerator: text at a place.
pub(crate) struct Mark {
    pub(crate) text: JsText,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

pub(crate) struct WardleyModel {
    pub(crate) title: Option<JsText>,
    pub(crate) accessible: Accessible,
    pub(crate) nodes: Vec<WardleyNode>,
    pub(crate) links: Vec<WardleyLink>,
    pub(crate) trends: Vec<Trend>,
    pub(crate) pipelines: Vec<Pipeline>,
    pub(crate) annotations: Vec<Annotation>,
    pub(crate) notes: Vec<Mark>,
    pub(crate) accelerators: Vec<Mark>,
    pub(crate) deaccelerators: Vec<Mark>,
    pub(crate) annotations_box: Option<(f64, f64)>,
    /// The stages of evolution and the right edges of those that give one.
    pub(crate) axes: Option<(Vec<JsText>, Vec<f64>)>,
    pub(crate) size: Option<(f64, f64)>,
}

/// What an `addNode` call gives besides the id, label and place. A field a statement does not
/// give keeps the value the node has.
enum Given {
    Anchor,
    Component { offsets: [Option<f64>; 2], inertia: bool, strategy: Option<Strategy> },
    PipelineComponent { offsets: [Option<f64>; 2] },
}

#[derive(Default)]
struct Builder {
    nodes: IndexMap<JsText, WardleyNode>,
    /// Of each label, the first node that has it, until a node is added.
    labels: Option<HashMap<JsText, JsText>>,
    trends: IndexMap<JsText, Trend>,
    pipelines: IndexMap<JsText, Pipeline>,
}

impl Builder {
    fn add_node(&mut self, id: &[u16], label: &[u16], at: (f64, f64), given: Given) {
        let fresh = || WardleyNode {
            id: id.into(),
            label: label.into(),
            x: 0.0,
            y: 0.0,
            class: NodeClass::Anchor,
            label_offset_x: None,
            label_offset_y: None,
            in_pipeline: false,
            is_pipeline_parent: false,
            inertia: None,
            strategy: None,
        };
        let mut node = self.nodes.get(id).cloned().unwrap_or_else(fresh);
        node.label = label.into();
        (node.x, node.y) = at;
        match given {
            Given::Anchor => node.class = NodeClass::Anchor,
            Given::Component { offsets, inertia, strategy } => {
                node.class = NodeClass::Component;
                node.label_offset_x = offsets[0].or(node.label_offset_x);
                node.label_offset_y = offsets[1].or(node.label_offset_y);
                node.inertia = Some(inertia);
                node.strategy = strategy;
            }
            Given::PipelineComponent { offsets } => {
                node.class = NodeClass::PipelineComponent;
                node.label_offset_x = offsets[0].or(node.label_offset_x);
                node.label_offset_y = offsets[1].or(node.label_offset_y);
                node.inertia = None;
                node.strategy = None;
            }
        }
        self.nodes.insert(id.into(), node);
        self.labels = None;
    }

    /// A name is a node id, or else the label of the first node that has it.
    fn resolve(&mut self, name: &[u16]) -> JsText {
        if self.nodes.contains_key(name) {
            return name.into();
        }
        let nodes = &self.nodes;
        let labels = self.labels.get_or_insert_with(|| {
            let mut labels = HashMap::new();
            for (id, node) in nodes {
                labels.entry(node.label.clone()).or_insert_with(|| id.clone());
            }
            labels
        });
        labels.get(name).cloned().unwrap_or_else(|| name.into())
    }
}

fn received(context: &str, value: f64) -> String {
    format!("{context} must be between 0-1 (decimal) or 0-100 (percentage). Received: {}", js::to_string(value))
}

/// Values up to 1 are fractions; larger ones are already percentages.
fn to_percent(value: f64, context: impl FnOnce() -> String) -> Result<f64, String> {
    let normalized = if value <= 1.0 { value * 100.0 } else { value };
    if normalized < 0.0 || normalized > 100.0 {
        return Err(received(&context(), value));
    }
    Ok(normalized)
}

/// Evolution along x, from the second number; visibility up y, from the first.
fn to_coordinates(placed: &PlacedAst, context: &str) -> Result<(f64, f64), String> {
    let x = to_percent(placed.second, || format!("{context} evolution"))?;
    let y = to_percent(placed.first, || format!("{context} visibility"))?;
    Ok((x, y))
}

fn shown(text: &[u16]) -> String {
    String::from_utf16_lossy(text)
}

fn flow_from_port(port: Option<&[u16]>) -> Option<Flow> {
    let port = port?;
    if port.is_str("+<>") {
        Some(Flow::Bidirectional)
    } else if port.is_str("+<") {
        Some(Flow::Backward)
    } else if port.is_str("+>") {
        Some(Flow::Forward)
    } else {
        None
    }
}

/// Pele's `flowFromArrow`: the flow and the label of a `+'label'>` arrow.
fn flow_from_arrow(arrow: Option<&[u16]>) -> (Option<Flow>, Option<JsText>) {
    let Some(arrow) = arrow.filter(|arrow| arrow.at(0) == Some(u16::from(b'+'))) else {
        return (None, None);
    };
    let quote = u16::from(b'\'');
    let end = arrow.find_unit(quote, 2);
    let label = match end {
        Some(end) if arrow.at(1) == Some(quote) => Some(JsText::from(&arrow[2..end])),
        _ => None,
    };
    let flow = if arrow.has("<>") {
        Some(Flow::Bidirectional)
    } else if arrow.has("<") {
        Some(Flow::Backward)
    } else if arrow.has(">") {
        Some(Flow::Forward)
    } else {
        None
    };
    (flow, label)
}

fn offsets(label: Option<&LabelAst>) -> [Option<f64>; 2] {
    let Some(label) = label else { return [None, None] };
    let signed = |negative: bool, value: f64| if negative { -1.0 } else { 1.0 } * value;
    [Some(signed(label.neg_x, label.offset_x)), Some(signed(label.neg_y, label.offset_y))]
}

fn text(units: &[u16]) -> JsText {
    units.into()
}

/// `text.replace(/\n\s+/g, '\n')`
fn unindent(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        let spaces = text.run(at + 1, is_space);
        at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
    }
    out
}

/// Pele's `buildWardley`: Mermaid's `populateDb` in Mermaid's order, then the builder's result.
/// The error is the message of the semantic error Pele throws.
pub(crate) fn build_wardley(ast: WardleyAst, front_title: Option<&JsText>) -> Result<WardleyModel, String> {
    let mut db = Builder::default();
    let axes = ast.evolution.as_ref().map(|stages| {
        let names = stages
            .iter()
            .map(|stage| match stage.second_name.as_ref().filter(|second| !second.is_empty()) {
                Some(second) => {
                    let mut name = JsText::from(stage.name.trim_js());
                    name.push_str(" / ");
                    name.push_units(second.trim_js());
                    name
                }
                None => text(stage.name.trim_js()),
            })
            .collect();
        (names, stages.iter().filter_map(|stage| stage.boundary).collect())
    });

    for anchor in &ast.anchors {
        let at = to_coordinates(anchor, &format!("Anchor \"{}\"", shown(&anchor.name)))?;
        db.add_node(&anchor.name, &anchor.name, at, Given::Anchor);
    }
    for component in &ast.components {
        let placed = PlacedAst { name: Vec::new(), first: component.visibility, second: component.evolution };
        let at = to_coordinates(&placed, &format!("Component \"{}\"", shown(&component.name)))?;
        let given = Given::Component {
            offsets: offsets(component.label.as_ref()),
            inertia: component.inertia,
            strategy: component.strategy.as_deref().map(Strategy::of),
        };
        db.add_node(&component.name, &component.name, at, given);
    }
    let mut notes = Vec::new();
    for note in &ast.notes {
        let (x, y) = to_coordinates(note, &format!("Note \"{}\"", shown(&note.name)))?;
        notes.push(Mark { text: text(&note.name), x, y });
    }
    for pipeline in &ast.pipelines {
        let Some(parent) = db.nodes.get_mut(pipeline.parent.as_slice()) else {
            return Err(format!(
                "Pipeline \"{}\" must reference an existing component with coordinates.",
                shown(&pipeline.parent)
            ));
        };
        let y = parent.y;
        parent.is_pipeline_parent = true;
        let parent_id = text(&pipeline.parent);
        db.pipelines.insert(parent_id.clone(), Pipeline { node_id: parent_id.clone(), component_ids: Vec::new() });
        for component in &pipeline.components {
            let mut id = parent_id.clone();
            id.push_str("_");
            id.push_units(&component.name);
            let x = to_percent(component.evolution, || {
                format!("Pipeline component \"{}\" evolution", shown(&component.name))
            })?;
            let given = Given::PipelineComponent { offsets: offsets(component.label.as_ref()) };
            db.add_node(&id, &component.name, (x, y), given);
            db.pipelines.get_mut(&parent_id).expect("the pipeline started above").component_ids.push(id.clone());
            db.nodes.get_mut(&id).expect("the node added above").in_pipeline = true;
        }
    }
    let mut links = Vec::new();
    for link in &ast.links {
        let arrow = link.arrow.as_deref();
        let dashed = arrow.is_some_and(|arrow| arrow.has("-.->") || arrow.has(".-."));
        let (arrow_flow, arrow_label) = flow_from_arrow(arrow);
        let flow = flow_from_port(link.from_port.as_deref()).or(flow_from_port(link.to_port.as_deref())).or(arrow_flow);
        let source = db.resolve(&link.from);
        let target = db.resolve(&link.to);
        let label = arrow_label.or_else(|| link.link_label.as_deref().map(text));
        links.push(WardleyLink { source, target, dashed, label, flow });
    }
    for (component, target) in &ast.evolves {
        if let Some(node) = db.nodes.get(component.as_slice()) {
            let target_y = node.y;
            let target_x = to_percent(*target, || format!("Evolve target for \"{}\"", shown(component)))?;
            db.trends.insert(text(component), Trend { node_id: text(component), target_x, target_y });
        }
    }
    let annotations_box = match ast.annotations.first() {
        Some(&(first, second)) => {
            Some(to_coordinates(&PlacedAst { name: Vec::new(), first, second }, "Annotations box")?)
        }
        None => None,
    };
    let mut annotations = Vec::new();
    for annotation in &ast.annotation {
        let placed = PlacedAst { name: Vec::new(), first: annotation.x, second: annotation.y };
        let at = to_coordinates(&placed, &format!("Annotation {}", js::to_string(annotation.number)))?;
        annotations.push(Annotation { number: annotation.number, at, text: text(&annotation.text) });
    }
    let forces = |list: &[PlacedAst], what: &str| -> Result<Vec<Mark>, String> {
        list.iter()
            .map(|force| {
                let (x, y) = to_coordinates(force, &format!("{what} \"{}\"", shown(&force.name)))?;
                Ok(Mark { text: text(&force.name), x, y })
            })
            .collect()
    };
    let accelerators = forces(&ast.accelerators, "Accelerator")?;
    let deaccelerators = forces(&ast.deaccelerators, "Deaccelerator")?;

    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    Ok(WardleyModel {
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        accessible: Accessible {
            title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
            description: filled(ast.acc_descr).map(|text| unindent(&text).into()),
        },
        nodes: db.nodes.into_values().collect(),
        links,
        trends: db.trends.into_values().collect(),
        pipelines: db.pipelines.into_values().collect(),
        annotations,
        notes,
        accelerators,
        deaccelerators,
        annotations_box,
        axes,
        size: ast.size,
    })
}
