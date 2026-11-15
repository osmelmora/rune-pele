//! Mermaid's `StateDB` in its version 2 form (Pele's `src/diagrams/state/db.ts`), which is what
//! Mermaid uses for both `stateDiagram` and `stateDiagram-v2`.

use super::graph::build_state_graph;
use super::{
    ClassDef, DbState, Description, DocId, ROOT, Relation, StateLink, StateModel, StateNote, StateStmt, StateType, Stmt,
};
use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsText, Units, is_space, split_unit};
use indexmap::IndexMap;

/// What the parser builds the statement tree into, and the calls Mermaid's grammar makes on its
/// `yy` object.
pub(super) struct StateDb {
    pub(super) docs: Vec<Vec<Stmt>>,
    pub(super) title: Option<JsText>,
    acc_title: Option<JsText>,
    acc_descr: Option<JsText>,
    divider_count: usize,
}

/// `text.startsWith(':') ? text.slice(1).trim() : text.trim()`
pub(super) fn trim_colon(text: &[u16]) -> JsText {
    let text = if text.first() == Some(&58) { &text[1..] } else { text };
    text.trim_js().into()
}

/// `text.replace(/\n\s+/g, '\n')`
fn close_indents(text: &[u16]) -> JsText {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        if text[at] == 10 && text.get(at + 1).is_some_and(|&c| is_space(c)) {
            at = text.run(at + 1, is_space);
        } else {
            at += 1;
        }
    }
    out.into()
}

/// A statement the translation pass visits: the root, a statement of a document, or one side of
/// a relation.
#[derive(Clone, Copy)]
enum Target {
    Root,
    Item(DocId, usize),
    Side(DocId, usize, bool),
}

impl StateDb {
    pub(super) fn new() -> Self {
        StateDb { docs: vec![Vec::new()], title: None, acc_title: None, acc_descr: None, divider_count: 0 }
    }

    /// A new, empty document.
    pub(super) fn doc(&mut self) -> DocId {
        self.docs.push(Vec::new());
        DocId(self.docs.len() - 1)
    }

    pub(super) fn divider_id(&mut self) -> JsText {
        self.divider_count += 1;
        format!("divider-id-{}", self.divider_count).as_str().into()
    }

    pub(super) fn set_acc_title(&mut self, text: &[u16]) {
        self.acc_title = Some(text.trim_start_js().into());
    }

    pub(super) fn set_acc_description(&mut self, text: &[u16]) {
        self.acc_descr = Some(close_indents(text));
    }

    fn state_at(&mut self, target: Target) -> Option<&mut StateStmt> {
        match target {
            Target::Root => None,
            Target::Item(doc, at) => match &mut self.docs[doc.0][at] {
                Stmt::State(state) => Some(state),
                _ => None,
            },
            Target::Side(doc, at, second) => match &mut self.docs[doc.0][at] {
                Stmt::Relation { state1, state2, .. } => Some(if second { state2 } else { state1 }),
                _ => None,
            },
        }
    }

    /// Mermaid's `docTranslator`: gives each `[*]` an id scoped to its parent, and turns a
    /// document split by `--` into one region per part.
    fn translate(&mut self) {
        let mut pending: Vec<(JsText, Target, bool)> = vec![("root".into(), Target::Root, true)];
        while let Some((parent_id, target, first)) = pending.pop() {
            if let Target::Item(doc, at) = target {
                match &self.docs[doc.0][at] {
                    Stmt::Relation { .. } => {
                        pending.push((parent_id.clone(), Target::Side(doc, at, true), false));
                        pending.push((parent_id, Target::Side(doc, at, false), true));
                        continue;
                    }
                    Stmt::State(_) => {}
                    _ => continue,
                }
            }
            let (id, doc) = match self.state_at(target) {
                Some(state) => {
                    if state.id.is_str("[*]") {
                        let mut id = parent_id;
                        id.push_str(if first { "_start" } else { "_end" });
                        state.id = id;
                        state.start = Some(first);
                    } else {
                        state.id = state.id.trim_js().into();
                    }
                    (state.id.clone(), state.doc)
                }
                None => ("root".into(), Some(ROOT)),
            };
            let Some(doc) = doc else {
                continue;
            };

            let items = std::mem::take(&mut self.docs[doc.0]);
            let mut regions: Vec<Stmt> = Vec::new();
            let mut current: Vec<Stmt> = Vec::new();
            for item in items {
                match item {
                    Stmt::State(state) if state.kind == Some(StateType::Divider) && state.doc.is_none() => {
                        let region = self.doc();
                        self.docs[region.0] = std::mem::take(&mut current);
                        regions.push(Stmt::State(StateStmt { doc: Some(region), ..state }));
                    }
                    other => current.push(other),
                }
            }
            // Mermaid names the region after the last divider with a random id; here it
            // continues the divider numbering. Mermaid also leaves a document that ends with a
            // divider ungrouped and then fails to draw it; here the regions before that divider
            // are kept.
            if regions.is_empty() {
                self.docs[doc.0] = current;
            } else {
                if !current.is_empty() {
                    let region = self.doc();
                    self.docs[region.0] = current;
                    let id = self.divider_id();
                    regions.push(Stmt::State(StateStmt {
                        doc: Some(region),
                        ..StateStmt::new(id, Some(StateType::Divider))
                    }));
                }
                self.docs[doc.0] = regions;
            }
            for at in (0..self.docs[doc.0].len()).rev() {
                pending.push((id.clone(), Target::Item(doc, at), true));
            }
        }
    }

    /// Pele's `setRootDoc`: translates the tree, then reads the root document's statements into
    /// states, relations and classes and builds the graph. The statements inside composites are
    /// read only by the graph.
    pub(super) fn finish(mut self, cx: &ParseContext<'_>) -> Result<StateModel, RenderError> {
        self.translate();
        let mut out = Extract::default();
        for item in &self.docs[ROOT.0] {
            match item {
                Stmt::Text(_) | Stmt::Dir(_) => {}
                Stmt::State(state) => out.add_state(
                    state.id.trim_js(),
                    state.kind,
                    state.doc,
                    state.description.as_ref(),
                    state.note.as_ref(),
                    None,
                ),
                Stmt::Relation { state1, state2, description } => {
                    for item in [state1, state2] {
                        let classes = item.classes.as_deref();
                        out.add_state(
                            &item.id,
                            item.kind,
                            item.doc,
                            item.description.as_ref(),
                            item.note.as_ref(),
                            classes,
                        );
                    }
                    out.relations.push(Relation {
                        id1: state1.id.trim_js().into(),
                        id2: state2.id.trim_js().into(),
                        title: description.clone().unwrap_or_default(),
                    });
                }
                Stmt::ClassDef { id, classes } => out.add_style_class(id.trim_js(), classes),
                Stmt::Style { id, style_class } => {
                    let styles: Vec<JsText> = split_unit(style_class, 44)
                        .into_iter()
                        .map(|style| {
                            let kept: Vec<u16> = style.iter().copied().filter(|&c| c != 59).collect();
                            kept.trim_js().into()
                        })
                        .collect();
                    for key in out.named(id.trim_js()) {
                        out.states[&key].styles = styles.clone();
                    }
                }
                Stmt::ApplyClass { id, style_class } => out.set_css_class(id.trim_js(), style_class),
                Stmt::Click { id, url, tooltip } => {
                    out.links.insert(id.id.clone(), StateLink { url: url.clone(), tooltip: tooltip.clone() });
                }
            }
        }
        let graph = build_state_graph(&self.docs, &out.states, &out.classes).map_err(|message| cx.semantic(message))?;
        Ok(StateModel {
            docs: self.docs,
            states: out.states,
            relations: out.relations,
            classes: out.classes,
            links: out.links,
            graph,
            title: self.title,
            acc_title: self.acc_title,
            acc_descr: self.acc_descr,
        })
    }
}

/// What the db reads from the root document.
#[derive(Default)]
struct Extract {
    states: IndexMap<JsText, DbState>,
    relations: Vec<Relation>,
    classes: IndexMap<JsText, ClassDef>,
    links: IndexMap<JsText, StateLink>,
}

impl Extract {
    /// Pele's `addState`. An absent `kind` is the default.
    fn add_state(
        &mut self,
        id: &[u16],
        kind: Option<StateType>,
        doc: Option<DocId>,
        description: Option<&Description>,
        note: Option<&StateNote>,
        classes: Option<&[JsText]>,
    ) {
        let id: JsText = id.trim_js().into();
        let kind = kind.unwrap_or(StateType::Default);
        match self.states.get_mut(&id) {
            None => {
                let state = DbState {
                    id: id.clone(),
                    descriptions: Vec::new(),
                    kind,
                    doc,
                    note: note.cloned(),
                    classes: Vec::new(),
                    styles: Vec::new(),
                    text_styles: Vec::new(),
                };
                self.states.insert(id.clone(), state);
            }
            Some(state) => {
                if state.doc.is_none() {
                    state.doc = doc;
                }
            }
        }
        if let Some(description) = description.filter(|description| description.truthy()) {
            let texts: Vec<&JsText> = match description {
                Description::Text(text) => vec![text],
                Description::Lines(lines) => lines.iter().collect(),
            };
            for text in texts {
                self.add_description(&id, text.trim_js());
            }
        }
        if let Some(note) = note {
            self.states[&id].note = Some(note.clone());
        }
        if let Some(classes) = classes {
            for name in classes {
                self.set_css_class(&id, name.trim_js());
            }
        }
    }

    fn add_description(&mut self, id: &[u16], description: &[u16]) {
        if let Some(state) = self.states.get_mut(id) {
            let text = if description.first() == Some(&58) { description[1..].trim_js() } else { description };
            state.descriptions.push(text.into());
        }
    }

    /// Pele's `addStyleClass`.
    fn add_style_class(&mut self, id: &[u16], attributes: &[u16]) {
        let found = self.classes.entry(id.into()).or_insert_with(|| ClassDef {
            id: id.into(),
            styles: Vec::new(),
            text_styles: Vec::new(),
        });
        if attributes.is_empty() {
            return;
        }
        for attribute in split_unit(attributes, 44) {
            let fixed: JsText = replace_first(attribute, ";", "").trim_js().into();
            if attribute.has("color") {
                found.text_styles.push(replace_first(&replace_first(&fixed, "fill", "bgFill"), "color", "fill").into());
            }
            found.styles.push(fixed);
        }
    }

    fn set_css_class(&mut self, ids: &[u16], name: &[u16]) {
        for key in self.named(ids) {
            self.states[&key].classes.push(name.into());
        }
    }

    /// The states a comma-separated list of ids names, created where they do not exist yet. As
    /// in Mermaid, an id is looked up as written and trimmed only when it has to be created.
    fn named(&mut self, ids: &[u16]) -> Vec<JsText> {
        let mut out = Vec::new();
        for id in split_unit(ids, 44) {
            if self.states.contains_key(id) {
                out.push(id.into());
                continue;
            }
            self.add_state(id, None, None, None, None, None);
            let trimmed = id.trim_js();
            if self.states.contains_key(trimmed) {
                out.push(trimmed.into());
            }
        }
        out
    }
}

/// `text.replace(from, to)` for a text pattern: the first occurrence only.
fn replace_first(text: &[u16], from: &str, to: &str) -> Vec<u16> {
    match text.find_str(from, 0) {
        Some(at) => {
            let mut out = text[..at].to_vec();
            out.extend(to.encode_utf16());
            out.extend_from_slice(&text[at + from.encode_utf16().count()..]);
            out
        }
        None => text.to_vec(),
    }
}
