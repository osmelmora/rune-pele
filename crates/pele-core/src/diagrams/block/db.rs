//! The block diagram db, ported from Pele's `src/diagrams/block/db.ts`: counted ids, classes and
//! styles, and `setHierarchy`, which builds the statements into the tree of blocks.

use super::parser::{Item, Parsed};
use super::{BlockClassDef, BlockEdge, BlockModel, BlockNode, Kind};
use crate::input::{JsText, Units};
use crate::{Diagnostic, ErrorCode, RenderError};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

const MAX_SPACES: f64 = 100_000.0;

/// A statement list being built into the children of `parent`. A block that repeats an id
/// already in the tree has no place of its own, so its children go nowhere.
struct Frame {
    list: std::vec::IntoIter<Item>,
    parent: Option<usize>,
    children: Vec<usize>,
}

pub(super) struct BlockDb {
    model: BlockModel,
    /// Ids that a class or style statement named before any block had them.
    pending: HashSet<JsText>,
    edge_count: HashMap<JsText, u32>,
    ids: u32,
    colors: u32,
    spaces: f64,
}

fn split(text: &[u16], separator: u16) -> impl Iterator<Item = &[u16]> {
    text.split(move |&c| c == separator)
}

/// `replace(from, to)` of a string: the first `from` only.
fn replace_first(text: &[u16], from: &str, to: &str) -> JsText {
    match text.find_str(from, 0) {
        Some(at) => {
            let mut out = JsText::from(&text[..at]);
            out.push_str(to);
            out.push_units(&text[at + from.len()..]);
            out
        }
        None => text.into(),
    }
}

impl BlockDb {
    pub(super) fn new() -> Self {
        let root = BlockNode {
            id: "root".into(),
            kind: Kind::Composite,
            label: None,
            children: Vec::new(),
            columns: Some(-1.0),
            width_in_columns: None,
            width: None,
            directions: None,
            classes: None,
            styles: None,
            color_index: None,
        };
        BlockDb {
            model: BlockModel {
                blocks: vec![root],
                edges: Vec::new(),
                classes: IndexMap::new(),
                by_id: IndexMap::new(),
                title: None,
            },
            pending: HashSet::new(),
            edge_count: HashMap::new(),
            ids: 0,
            colors: 0,
            spaces: 0.0,
        }
    }

    /// Mermaid's ids are random. These are counted, so the same text always gives the same output.
    pub(super) fn generate_id(&mut self) -> JsText {
        self.ids += 1;
        format!("id-{}", self.ids).as_str().into()
    }

    fn add_style_class(&mut self, id: JsText, attributes: &[u16]) {
        let found = self.model.classes.entry(id.clone()).or_insert_with(|| BlockClassDef {
            id,
            styles: Vec::new(),
            text_styles: Vec::new(),
        });
        for attribute in split(attributes, 44) {
            let fixed: JsText = match attribute.find_unit(59, 0) {
                None => attribute.trim_js().into(),
                Some(semi) => {
                    let mut joined = JsText::from(&attribute[..semi]);
                    joined.push_units(&attribute[semi + 1..]);
                    joined.trim_js().into()
                }
            };
            if attribute.has("color") {
                found.text_styles.push(replace_first(&replace_first(&fixed, "fill", "bgFill"), "color", "fill"));
            }
            found.styles.push(fixed);
        }
    }

    /// Pele's `named`: the block of an id, or a placeholder that waits for the block.
    fn named(&mut self, id: &[u16]) -> &mut BlockNode {
        let at = match self.model.by_id.get(id) {
            Some(&at) => at,
            None => {
                let at = self.model.blocks.len();
                self.model.blocks.push(BlockNode {
                    id: id.into(),
                    kind: Kind::Na,
                    label: None,
                    children: Vec::new(),
                    columns: None,
                    width_in_columns: None,
                    width: None,
                    directions: None,
                    classes: None,
                    styles: None,
                    color_index: None,
                });
                self.model.by_id.insert(id.into(), at);
                self.pending.insert(id.into());
                at
            }
        };
        &mut self.model.blocks[at]
    }

    /// Mermaid looks each id up before trimming it. Here it is trimmed first, as in Pele.
    fn set_css_class(&mut self, ids: &[u16], class: &JsText) {
        for id in split(ids, 44) {
            self.named(id.trim_js()).classes.get_or_insert_with(Vec::new).push(class.clone());
        }
    }

    fn add_style_to_node(&mut self, id: &[u16], styles: &[u16]) {
        self.named(id).styles = Some(split(styles, 44).map(JsText::from).collect());
    }

    fn edge(&mut self, mut edge: BlockEdge) {
        let count = self.edge_count.entry(edge.id.clone()).or_insert(0);
        *count += 1;
        let mut id = JsText::from(count.to_string().as_str());
        id.push(45);
        id.push_units(&edge.id);
        edge.id = id;
        self.model.edges.push(edge);
    }

    /// Pele's `setHierarchy`.
    pub(super) fn set_hierarchy(&mut self, statements: Vec<Item>) -> Result<(), RenderError> {
        let mut stack = vec![Frame { list: statements.into_iter(), parent: Some(0), children: Vec::new() }];
        while let Some(frame) = stack.last_mut() {
            let Some(item) = frame.list.next() else {
                let frame = stack.pop().expect("a frame is open");
                if let Some(parent) = frame.parent {
                    self.model.blocks[parent].children = frame.children;
                }
                continue;
            };
            let parent = frame.parent;
            let parsed = match item {
                Item::ClassDef { id, css } => {
                    self.add_style_class(id, &css);
                    continue;
                }
                Item::ApplyClass { ids, class } => {
                    self.set_css_class(&ids, &class);
                    continue;
                }
                Item::ApplyStyles { id, styles } => {
                    if !styles.is_empty() {
                        self.add_style_to_node(&id, &styles);
                    }
                    continue;
                }
                Item::Columns(columns) => {
                    if let Some(parent) = parent {
                        self.model.blocks[parent].columns = Some(columns);
                    }
                    continue;
                }
                Item::Edge(edge) => {
                    self.edge(edge);
                    continue;
                }
                Item::Block(parsed) => parsed,
            };
            let placed = self.place(parsed)?;
            let frame = stack.last_mut().expect("a frame is open");
            frame.children.extend(placed.children);
            if !placed.inner.is_empty() {
                stack.push(Frame { list: placed.inner.into_iter(), parent: placed.at, children: Vec::new() });
            }
        }
        Ok(())
    }

    /// One block of a statement list: its id and label, its place in `byId`, and the blocks it
    /// adds to the list's children.
    fn place(&mut self, parsed: Parsed) -> Result<Placed, RenderError> {
        let Parsed { id, kind, label, width_in_columns, width, directions, children: inner } = parsed;
        // A composite whose header was a link statement has no id of its own.
        let id = id.unwrap_or_else(|| self.generate_id());
        let label = match label.filter(|label| !label.is_empty()) {
            Some(label) => label,
            None if kind == Kind::Composite => JsText::default(),
            None => id.clone(),
        };
        let known = self.model.by_id.get(&id).copied();
        let fresh = known.is_none() || self.pending.remove(&id);
        let mut block = BlockNode {
            id: id.clone(),
            kind,
            label: Some(label),
            children: Vec::new(),
            columns: None,
            width_in_columns,
            width,
            directions,
            classes: None,
            styles: None,
            color_index: None,
        };
        let mut at = None;
        match known {
            Some(known) if !fresh => {
                let known = &mut self.model.blocks[known];
                if kind != Kind::Na {
                    known.kind = kind;
                }
                if block.label.as_ref() != Some(&id) {
                    known.label = block.label.clone();
                }
            }
            _ => {
                if let Some(known) = known {
                    let known = &self.model.blocks[known];
                    block.classes = known.classes.clone();
                    block.styles = known.styles.clone();
                }
                if kind == Kind::Composite {
                    block.color_index = Some(f64::from(self.colors));
                    self.colors += 1;
                }
                let index = self.model.blocks.len();
                self.model.blocks.push(block.clone());
                self.model.by_id.insert(id.clone(), index);
                at = Some(index);
            }
        }
        let mut children = Vec::new();
        if kind == Kind::Space {
            let width = width.unwrap_or(1.0);
            // Pele's `!(width <= …)`, which a count too large to parse also fails.
            if width.partial_cmp(&(MAX_SPACES - self.spaces)).is_none_or(|order| order.is_gt()) {
                let message = format!("A block diagram can hold at most {} space blocks.", MAX_SPACES as u32);
                return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, message)));
            }
            self.spaces += width.max(0.0);
            let mut j = 0.0;
            while j < width {
                let mut copy = BlockNode { children: Vec::new(), ..block.clone() };
                copy.id.push(45);
                copy.id.push_str(&crate::js::to_string(j));
                let index = self.model.blocks.len();
                self.model.by_id.insert(copy.id.clone(), index);
                self.model.blocks.push(copy);
                children.push(index);
                j += 1.0;
            }
        } else if let Some(at) = at {
            children.push(at);
        }
        Ok(Placed { at, children, inner })
    }

    pub(super) fn finish(self) -> BlockModel {
        self.model
    }
}

struct Placed {
    /// Where the block went in the arena, unless its id was already taken.
    at: Option<usize>,
    children: Vec<usize>,
    inner: Vec<Item>,
}
