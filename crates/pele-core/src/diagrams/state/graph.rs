//! Mermaid's `dataFetcher` (Pele's `src/diagrams/state/graph.ts`): walks the documents and
//! resolves them into the nodes and edges that are drawn.

use super::{ClassDef, DbState, Description, Direction, ROOT, StateStmt, StateType, Stmt, direction_of};
use crate::input::{JsText, Units, split_unit};
use indexmap::IndexMap;
use std::collections::HashMap;

/// What a node is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Rect,
    RectWithTitle,
    StateStart,
    StateEnd,
    Choice,
    Fork,
    Join,
    Divider,
    RoundedWithTitle,
    Note,
    NoteGroup,
}

impl Shape {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Shape::Rect => "rect",
            Shape::RectWithTitle => "rectWithTitle",
            Shape::StateStart => "stateStart",
            Shape::StateEnd => "stateEnd",
            Shape::Choice => "choice",
            Shape::Fork => "fork",
            Shape::Join => "join",
            Shape::Divider => "divider",
            Shape::RoundedWithTitle => "roundedWithTitle",
            Shape::Note => "note",
            Shape::NoteGroup => "noteGroup",
        }
    }

    /// The shape a statement of `kind` starts as, where its kind names one. A note's statement
    /// has no kind, and so no shape.
    fn of(kind: Option<StateType>) -> Option<Shape> {
        match kind? {
            StateType::Default => Some(Shape::Rect),
            StateType::Fork => Some(Shape::Fork),
            StateType::Join => Some(Shape::Join),
            StateType::Choice => Some(Shape::Choice),
            StateType::Divider => Some(Shape::Divider),
        }
    }
}

/// Pele's `StateNode`. Labels are markdown throughout, as in Mermaid.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphNode {
    pub(crate) id: JsText,
    pub(crate) shape: Option<Shape>,
    pub(crate) label: Option<JsText>,
    pub(crate) description: Vec<JsText>,
    pub(crate) css_classes: JsText,
    pub(crate) css_compiled_styles: Vec<JsText>,
    pub(crate) css_styles: Vec<JsText>,
    /// `group`, `node`, or nothing for a state that is not a composite.
    pub(crate) kind: Option<&'static str>,
    pub(crate) is_group: bool,
    pub(crate) parent_id: Option<JsText>,
    /// The direction inside a composite: its last `direction` statement, or TB.
    pub(crate) dir: Option<Direction>,
    pub(crate) position: Option<JsText>,
    pub(crate) note_for: Option<JsText>,
    pub(crate) color_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphEdge {
    pub(crate) id: JsText,
    pub(crate) start: JsText,
    pub(crate) end: JsText,
    pub(crate) label: JsText,
    pub(crate) classes: &'static str,
    /// Whether the edge joins a note to its state.
    pub(crate) dashed: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct StateGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

struct Draft {
    shape: Option<Shape>,
    description: Description,
    css_classes: JsText,
    css_styles: Vec<JsText>,
    group: bool,
    dir: Option<Direction>,
    color_index: Option<usize>,
}

/// A node as `insert` is given it. A note's nodes carry no colour slot, so inserting one over a
/// node with the same id keeps that node's.
struct Insert {
    node: GraphNode,
    has_color_index: bool,
}

struct Builder<'a> {
    states: &'a IndexMap<JsText, DbState>,
    classes: &'a IndexMap<JsText, ClassDef>,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    by_id: HashMap<JsText, usize>,
    drafts: HashMap<JsText, Draft>,
    slots: HashMap<JsText, Option<usize>>,
    counter: usize,
    next_slot: usize,
}

fn text(value: &str) -> JsText {
    value.into()
}

impl Builder<'_> {
    fn insert(&mut self, insert: Insert) {
        let Insert { mut node, has_color_index } = insert;
        // The two literal ids are what DOMPurify leaves of `<<fork>>` and `<<choice>>` in
        // Mermaid; kept for parity.
        if node.id.is_empty() || node.id.is_str("</join></fork>") || node.id.is_str("</choice>") {
            return;
        }
        if !node.css_classes.is_empty() {
            // One copy, however many classes: a state named in a class statement many times has
            // a long list.
            let mut compiled: Option<Vec<JsText>> = None;
            for name in split_unit(&node.css_classes, 32) {
                let Some(def) = self.classes.get(name) else {
                    continue;
                };
                compiled.get_or_insert_with(|| node.css_compiled_styles.clone()).extend(def.styles.iter().cloned());
            }
            if let Some(compiled) = compiled {
                node.css_compiled_styles = compiled;
            }
        }
        match self.by_id.get(&node.id) {
            Some(&at) => {
                let existing = &mut self.nodes[at];
                let parent_id = node.parent_id.take().or(existing.parent_id.take());
                let color_index = if has_color_index { node.color_index } else { existing.color_index };
                let position = node.position.take().or(existing.position.take());
                let note_for = node.note_for.take().or(existing.note_for.take());
                *existing = GraphNode { parent_id, color_index, position, note_for, ..node };
            }
            None => {
                self.by_id.insert(node.id.clone(), self.nodes.len());
                self.nodes.push(node);
            }
        }
    }

    fn visit(&mut self, parent: Option<&StateStmt>, item: &StateStmt, alt: bool, docs: &[Vec<Stmt>]) {
        let id = &item.id;
        if id.is_str("root") {
            return;
        }
        let known = self.states.get(id);
        let class_str = known.map(|known| known.classes.join(&[32][..])).unwrap_or_default();
        let styles = known.map(|known| known.styles.clone()).unwrap_or_default();

        let draft = self.drafts.entry(id.clone()).or_insert_with(|| {
            let mut shape = Some(Shape::Rect);
            match item.start {
                Some(true) => shape = Some(Shape::StateStart),
                Some(false) => shape = Some(Shape::StateEnd),
                None => {}
            }
            if item.kind != Some(StateType::Default) {
                shape = Shape::of(item.kind);
            }
            let mut css_classes = JsText::from(class_str.as_slice());
            css_classes.push_str(" statediagram-state");
            Draft {
                shape,
                description: Description::Text(id.clone()),
                css_classes,
                css_styles: styles.clone(),
                group: false,
                dir: None,
                color_index: None,
            }
        });

        if let Some(description) = item.description.as_ref().filter(|description| description.truthy()) {
            let given = |description: &Description| match description {
                Description::Text(text) => vec![text.clone()],
                Description::Lines(lines) => lines.clone(),
            };
            match &draft.description {
                Description::Lines(lines) => {
                    draft.shape = Some(Shape::RectWithTitle);
                    let mut lines = lines.clone();
                    lines.extend(given(description));
                    draft.description = Description::Lines(lines);
                }
                Description::Text(own) if !own.is_empty() => {
                    draft.shape = Some(Shape::RectWithTitle);
                    let mut lines = if own == id { Vec::new() } else { vec![own.clone()] };
                    lines.extend(given(description));
                    draft.description = Description::Lines(lines);
                }
                Description::Text(_) => {
                    draft.shape = Some(Shape::Rect);
                    draft.description = description.clone();
                }
            }
        }
        if draft.description.length() == 1 && draft.shape == Some(Shape::RectWithTitle) {
            draft.shape = Some(if draft.group { Shape::RoundedWithTitle } else { Shape::Rect });
        }

        if let Some(doc) = item.doc.filter(|_| !draft.group) {
            draft.group = true;
            draft.dir = Some(direction_of(&docs[doc.0]));
            draft.shape =
                Some(if item.kind == Some(StateType::Divider) { Shape::Divider } else { Shape::RoundedWithTitle });
            // A container takes the next palette slot unless it is a region, which shares its
            // composite's.
            let shared =
                parent.filter(|_| draft.shape == Some(Shape::Divider)).and_then(|parent| self.slots.get(&parent.id));
            draft.color_index = match shared {
                Some(&slot) => slot,
                None => {
                    let slot = self.next_slot;
                    self.next_slot += 1;
                    let plain = class_str.trim_js().is_empty() && styles.is_empty();
                    plain.then_some(slot)
                }
            };
            self.slots.insert(id.clone(), draft.color_index);
            draft.css_classes.push_str(" statediagram-cluster ");
            if alt {
                draft.css_classes.push_str("statediagram-cluster-alt");
            }
        }

        let (label, description) = match &draft.description {
            Description::Lines(lines) => (lines.first().cloned(), lines.iter().skip(1).cloned().collect()),
            Description::Text(text) => (Some(text.clone()), Vec::new()),
        };
        let mut node = GraphNode {
            id: id.clone(),
            shape: draft.shape,
            label,
            description,
            css_classes: draft.css_classes.clone(),
            css_compiled_styles: Vec::new(),
            css_styles: draft.css_styles.clone(),
            kind: draft.group.then_some("group"),
            is_group: draft.group,
            parent_id: None,
            dir: draft.dir,
            position: None,
            note_for: None,
            color_index: draft.color_index,
        };
        if node.shape == Some(Shape::Divider) {
            node.label = Some(JsText::default());
        }
        if let Some(parent) = parent.filter(|parent| !parent.id.is_str("root")) {
            node.parent_id = Some(parent.id.clone());
        }

        let Some(note) = &item.note else {
            self.insert(Insert { node, has_color_index: true });
            return;
        };
        // Mermaid puts each note in a group of its own, named after the state. Pele keeps that
        // group in the model and lays the note out beside its state instead.
        let aside = |aside_id: JsText, shape: Shape, css_classes: JsText, is_group: bool| GraphNode {
            id: aside_id,
            shape: Some(shape),
            label: Some(note.text.clone()),
            description: Vec::new(),
            css_classes,
            css_compiled_styles: Vec::new(),
            css_styles: Vec::new(),
            kind: Some(if is_group { "group" } else { "node" }),
            is_group,
            parent_id: None,
            dir: None,
            position: Some(note.position.clone()),
            note_for: None,
            color_index: None,
        };
        let mut note_id = id.clone();
        note_id.push_str(&format!("----note-{}", self.counter));
        self.counter += 1;
        let mut group_id = id.clone();
        group_id.push_str("----parent");
        let mut note_node = aside(note_id.clone(), Shape::Note, text("statediagram-note"), false);
        note_node.parent_id = Some(group_id.clone());
        note_node.note_for = Some(id.clone());
        let group = aside(group_id, Shape::NoteGroup, draft.css_classes.clone(), true);
        let left = note.position.is_str("left of");
        self.insert(Insert { node: group, has_color_index: false });
        self.insert(Insert { node: note_node, has_color_index: false });
        self.insert(Insert { node, has_color_index: true });
        let (from, to) = if left { (note_id, id.clone()) } else { (id.clone(), note_id) };
        let mut edge_id = from.clone();
        edge_id.push(45);
        edge_id.push_units(&to);
        self.edges.push(GraphEdge {
            id: edge_id,
            start: from,
            end: to,
            label: JsText::default(),
            classes: "transition note-edge",
            dashed: true,
        });
    }
}

/// A document being walked: the composite it belongs to, the next statement, and whether its
/// composites are drawn in the alternate shade.
struct Frame<'a> {
    item: Option<&'a StateStmt>,
    doc: &'a [Stmt],
    at: usize,
    alt: bool,
}

/// Pele's `buildStateGraph`. A composite given a description is the error it answers.
pub(super) fn build_state_graph(
    docs: &[Vec<Stmt>],
    states: &IndexMap<JsText, DbState>,
    classes: &IndexMap<JsText, ClassDef>,
) -> Result<StateGraph, String> {
    let mut builder = Builder {
        states,
        classes,
        nodes: Vec::new(),
        edges: Vec::new(),
        by_id: HashMap::new(),
        drafts: HashMap::new(),
        slots: HashMap::new(),
        counter: 0,
        next_slot: 0,
    };
    let mut stack = vec![Frame { item: None, doc: &docs[ROOT.0], at: 0, alt: true }];
    while let Some(frame) = stack.last_mut() {
        let doc = frame.doc;
        let Some(item) = doc.get(frame.at) else {
            stack.pop();
            continue;
        };
        frame.at += 1;
        let (parent, alt) = (frame.item, frame.alt);
        match item {
            Stmt::State(state) => {
                builder.visit(parent, state, alt, docs);
                if let Some(doc) = state.doc {
                    stack.push(Frame { item: Some(state), doc: &docs[doc.0], at: 0, alt: !alt });
                }
            }
            Stmt::Relation { state1, state2, description } => {
                builder.visit(parent, state1, alt, docs);
                builder.visit(parent, state2, alt, docs);
                let id = format!("edge{}", builder.counter);
                builder.counter += 1;
                builder.edges.push(GraphEdge {
                    id: id.as_str().into(),
                    start: state1.id.clone(),
                    end: state2.id.clone(),
                    label: description.clone().unwrap_or_default(),
                    classes: "transition",
                    dashed: false,
                });
            }
            _ => {}
        }
    }

    if let Some(node) = builder.nodes.iter().find(|node| node.is_group && !node.description.is_empty()) {
        return Err(format!(
            "Group nodes can only have label. Remove the additional description for node [{}]",
            node.id.to_display()
        ));
    }
    Ok(StateGraph { nodes: builder.nodes, edges: builder.edges })
}
