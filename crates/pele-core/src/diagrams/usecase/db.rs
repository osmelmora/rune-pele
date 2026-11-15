//! What the parser reads, and how it becomes a model, ported from Pele's
//! `src/diagrams/usecase/db.ts`. `resolve` runs once the whole diagram is read: it decides which
//! ids are actors, use cases, boundaries, JSON nodes or edges, in what order they appear, and
//! whether the statements about them agree. Its messages are Mermaid's `UsecaseModelBuilder`'s.

use super::json::Json;
use super::{
    Actor, ActorType, Animation, Arrow, BoundaryType, ClassDef, Direction, JsonNode, LabelType, Note, Positions,
    Relationship, RelationshipType, Span, SystemBoundary, UseCase, UseCaseShape, UsecaseModel,
};
use crate::input::{JsText, Units, is_space};
use crate::{ErrorCode, RenderError};
use indexmap::IndexMap;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(super) struct DraftLabel {
    pub(super) text: JsText,
    pub(super) kind: LabelType,
}

/// What a metadata property is set to: a word or a string, or `true` or `false`.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum MetaValue {
    Text(JsText),
    Bool(bool),
}

impl MetaValue {
    fn is(&self, text: &str) -> bool {
        matches!(self, MetaValue::Text(value) if value.is_str(text))
    }
}

/// `key_span` is where an error about the property points.
#[derive(Clone, Debug)]
pub(super) struct DraftProperty {
    pub(super) key: JsText,
    pub(super) value: MetaValue,
    pub(super) key_span: Span,
}

/// What every mention of an id carries: where it is, and whether the id was made from a label.
#[derive(Clone, Debug)]
pub(super) struct Named {
    pub(super) id: JsText,
    pub(super) label: DraftLabel,
    pub(super) span: Span,
    pub(super) generated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ElementKind {
    Actor,
    Usecase,
}

/// What an id stands for in the whole diagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SymbolKind {
    Actor,
    Usecase,
    Boundary,
    Json,
    Edge,
}

impl SymbolKind {
    fn name(self) -> &'static str {
        match self {
            SymbolKind::Actor => "actor",
            SymbolKind::Usecase => "usecase",
            SymbolKind::Boundary => "boundary",
            SymbolKind::Json => "json",
            SymbolKind::Edge => "edge",
        }
    }
}

impl From<ElementKind> for SymbolKind {
    fn from(kind: ElementKind) -> Self {
        match kind {
            ElementKind::Actor => SymbolKind::Actor,
            ElementKind::Usecase => SymbolKind::Usecase,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct DraftElement {
    pub(super) named: Named,
    pub(super) kind: ElementKind,
    /// The boundary the element is written in, by its place among the boundary drafts.
    pub(super) parent: Option<usize>,
    pub(super) shape: Option<UseCaseShape>,
    pub(super) metadata: Option<Vec<DraftProperty>>,
    pub(super) stereotype: Option<JsText>,
    pub(super) classes: Vec<JsText>,
}

#[derive(Clone, Debug)]
pub(super) struct DraftBoundary {
    pub(super) named: Named,
    pub(super) classes: Vec<JsText>,
}

#[derive(Debug)]
pub(super) struct DraftJson {
    pub(super) id: JsText,
    /// Set once the whole text has parsed.
    pub(super) value: Option<Json>,
    pub(super) span: Span,
    pub(super) classes: Vec<JsText>,
}

#[derive(Clone, Debug)]
pub(super) struct DraftEndpoint {
    pub(super) named: Named,
    /// Whether this mention declares the element, and whether it carries a `:::` suffix.
    pub(super) declares: bool,
    pub(super) classed: bool,
}

#[derive(Debug)]
pub(super) struct DraftRelationship {
    pub(super) source: DraftEndpoint,
    pub(super) target: DraftEndpoint,
    pub(super) span: Span,
    pub(super) id: Option<(JsText, Span)>,
    pub(super) kind: RelationshipType,
    pub(super) arrow: Arrow,
    pub(super) label: Option<DraftLabel>,
    pub(super) minlen: usize,
}

/// A `class` statement: the ids it names with where each is written, and the classes it gives them.
#[derive(Debug)]
pub(super) struct ClassAssignment {
    pub(super) targets: Vec<(JsText, Span)>,
    pub(super) classes: Vec<JsText>,
}

/// Everything the parser read, in source order, before any of it is resolved.
#[derive(Debug, Default)]
pub(super) struct Drafts {
    pub(super) elements: Vec<DraftElement>,
    pub(super) boundaries: Vec<DraftBoundary>,
    pub(super) jsons: Vec<DraftJson>,
    pub(super) relationships: Vec<DraftRelationship>,
    /// The id a note is for, where that id is written, and the note's text.
    pub(super) notes: Vec<(JsText, Span, DraftLabel)>,
    /// The id a metadata block is for, where that id is written, and the block's properties.
    pub(super) metadata: Vec<(JsText, Span, Vec<DraftProperty>)>,
    pub(super) classes: Vec<ClassAssignment>,
    pub(super) styles: Vec<(JsText, Span, Vec<JsText>)>,
    /// The names a `classDef` defines, and its declarations.
    pub(super) class_defs: Vec<(Vec<JsText>, Vec<JsText>)>,
    pub(super) direction: Option<Direction>,
    pub(super) acc_title: JsText,
    pub(super) acc_description: JsText,
}

/// Where an id was first given its meaning.
struct Origin {
    kind: SymbolKind,
    span: Span,
    generated: bool,
    /// The label a generated id was derived from.
    label: Option<JsText>,
}

/// An actor or a use case while the statements about it are gathered.
struct ElementState {
    named: Named,
    kind: ElementKind,
    /// A boundary draft. One whose title makes an empty id is no parent, but it still takes the
    /// place of one.
    parent: Option<usize>,
    shape: Option<UseCaseShape>,
    stereotype: Option<JsText>,
    stereotype_span: Option<Span>,
    classes: Vec<JsText>,
    actor_type: Option<ActorType>,
    icon: Option<JsText>,
    business: Option<bool>,
}

struct BoundaryState {
    named: Named,
    classes: Vec<JsText>,
    kind: Option<BoundaryType>,
    members: Vec<JsText>,
}

fn push_unique(target: &mut Vec<JsText>, values: &[JsText]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn shown(text: &[u16]) -> String {
    String::from_utf16_lossy(text)
}

/// Which kind of element the keys of a metadata block could only belong to, for the error message.
fn infer_kind(properties: &[DraftProperty]) -> Option<&'static str> {
    let mut possible = vec!["actor", "usecase", "boundary", "edge"];
    for DraftProperty { key, value, .. } in properties {
        if key.is_str("icon") {
            possible = vec!["actor"];
        } else if key.is_str("animate") || key.is_str("animation") {
            possible = vec!["edge"];
        } else if key.is_str("type") {
            possible.clear();
            if value.is("rect") || value.is("package") {
                possible.push("boundary");
            } else if value.is("normal") || value.is("hollow") || value.is("awesome") {
                possible.push("actor");
            }
        } else if key.is_str("business") {
            possible.retain(|&kind| kind != "boundary" && kind != "edge");
        } else {
            return None;
        }
    }
    match possible.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

type Checked<T = ()> = Result<T, RenderError>;

/// Pele's messages for a diagram whose statements do not agree.
#[derive(Clone, Copy)]
struct Faults<'a> {
    positions: &'a Positions<'a>,
}

impl Faults<'_> {
    fn error(self, message: String, span: Span) -> RenderError {
        self.positions.error(ErrorCode::Semantic, message, span[0])
    }

    fn fail(self, message: String, span: Span) -> RenderError {
        self.error(format!("{message} at {}", self.positions.location_text(span)), span)
    }

    /// A statement that disagrees with an earlier one, which the message also points to. A
    /// label is shown for an id that was made from it.
    fn conflict(
        self,
        message: String,
        current: (Span, Option<&JsText>),
        previous: (Span, Option<&JsText>),
    ) -> RenderError {
        let place = |(span, label): (Span, Option<&JsText>)| {
            let label = label.map_or(String::new(), |label| format!(" (label \"{}\")", shown(label)));
            format!("{}{label}", self.positions.location_text(span))
        };
        self.error(format!("{message} at {}; previous declaration at {}", place(current), place(previous)), current.0)
    }

    fn invalid(self, id: &[u16], kind: &str, property: &DraftProperty) -> RenderError {
        let message = format!("Metadata property '{}' is invalid for {kind} '{}'", shown(&property.key), shown(id));
        self.fail(message, property.key_span)
    }

    /// A metadata statement replaces what a declaration said; two declarations have to agree.
    fn element_metadata(self, state: &mut ElementState, properties: &[DraftProperty], replace: bool) -> Checked {
        for property in properties {
            let DraftProperty { key, value, key_span } = property;
            let actor = state.kind == ElementKind::Actor;
            let (id, span) = (shown(&state.named.id), state.named.span);
            let clash = |differs: bool| {
                if replace || !differs {
                    return Ok(());
                }
                let what = if actor { "Actor" } else { "Use case" };
                let message = format!("{what} '{id}' has conflicting {} metadata", shown(key));
                Err(self.conflict(message, (*key_span, None), (span, None)))
            };
            let actor_type =
                [("normal", ActorType::Normal), ("hollow", ActorType::Hollow), ("awesome", ActorType::Awesome)]
                    .into_iter()
                    .find(|(name, _)| value.is(name))
                    .map(|(_, kind)| kind);
            match (value, actor_type) {
                (_, Some(kind)) if actor && key.is_str("type") => {
                    clash(state.actor_type.is_some_and(|previous| previous != kind))?;
                    state.actor_type = Some(kind);
                }
                (MetaValue::Text(icon), _) if actor && key.is_str("icon") => {
                    clash(state.icon.as_ref().is_some_and(|previous| previous != icon))?;
                    state.icon = Some(icon.clone());
                }
                (MetaValue::Bool(business), _) if key.is_str("business") => {
                    clash(state.business.is_some_and(|previous| previous != *business))?;
                    state.business = Some(*business);
                }
                _ => return Err(self.invalid(&state.named.id, if actor { "actor" } else { "usecase" }, property)),
            }
        }
        Ok(())
    }
}

/// The id of the boundary an element is in, where that boundary has an id.
fn parent_id(boundaries: &[DraftBoundary], parent: Option<usize>) -> Option<&JsText> {
    parent.map(|at| &boundaries[at].named.id).filter(|id| !id.is_empty())
}

struct Resolver<'a> {
    faults: Faults<'a>,
    boundary_drafts: &'a [DraftBoundary],
    model: UsecaseModel,
    symbols: HashMap<JsText, Origin>,
    /// In the order the elements are first declared, then first related.
    elements: IndexMap<JsText, ElementState>,
    boundaries: IndexMap<JsText, BoundaryState>,
    /// Each relationship id with the place of the last relationship that has it.
    edges: HashMap<JsText, usize>,
    /// The offset at which each id is first mentioned, which decides the order of the model.
    first: HashMap<JsText, usize>,
}

impl Resolver<'_> {
    fn mention(&mut self, id: &JsText, offset: usize) {
        let first = self.first.entry(id.clone()).or_insert(offset);
        *first = (*first).min(offset);
    }

    fn rank(&self, id: &JsText) -> usize {
        self.first.get(id).copied().unwrap_or(0)
    }

    fn register(&mut self, named: &Named, kind: SymbolKind) {
        let label = Some(named.label.text.clone());
        self.symbols.insert(named.id.clone(), Origin { kind, span: named.span, generated: named.generated, label });
    }

    /// An id means one thing in the whole diagram, and an id made from a label may be declared once.
    fn claim(&self, draft: &Named, kind: SymbolKind) -> Checked {
        let Some(origin) = self.symbols.get(&draft.id) else {
            return Ok(());
        };
        let current = (draft.span, draft.generated.then_some(&draft.label.text));
        let previous = (origin.span, origin.label.as_ref().filter(|_| origin.generated));
        let id = shown(&draft.id);
        if origin.kind != kind {
            let message = format!("ID '{id}' is declared as both {} and {}", origin.kind.name(), kind.name());
            return Err(self.faults.conflict(message, current, previous));
        }
        if origin.generated || draft.generated {
            let message = format!("Generated ID '{id}' collides with another declaration");
            return Err(self.faults.conflict(message, current, previous));
        }
        Ok(())
    }

    fn unique(&mut self, id: &JsText, kind: SymbolKind, span: Span) -> Checked {
        if let Some(previous) = self.symbols.get(id) {
            let message =
                format!("ID '{}' is declared more than once ({} and {})", shown(id), previous.kind.name(), kind.name());
            let label = previous.label.as_ref().filter(|_| previous.generated);
            return Err(self.faults.conflict(message, (span, None), (previous.span, label)));
        }
        self.symbols.insert(id.clone(), Origin { kind, span, generated: false, label: None });
        Ok(())
    }

    fn publish(&mut self, id: &JsText) {
        let state = &self.elements[id];
        let label = state.named.label.text.clone();
        let label_type = state.named.label.kind;
        let business = state.business.unwrap_or(false);
        let classes = state.classes.clone();
        let stereotype = state.stereotype.clone().filter(|stereotype| !stereotype.is_empty());
        let parent_id = parent_id(self.boundary_drafts, state.parent).cloned();
        let id = id.clone();
        if state.kind == ElementKind::Actor {
            let icon = state.icon.clone().filter(|icon| !icon.is_empty());
            let kind = if icon.is_some() { ActorType::Icon } else { state.actor_type.unwrap_or(ActorType::Normal) };
            self.model.use_cases.shift_remove(&id);
            let styles = self.model.actors.get(&id).map(|actor| actor.styles.clone()).unwrap_or_default();
            let actor = Actor {
                id: id.clone(),
                label,
                label_type,
                kind,
                icon,
                business,
                stereotype,
                parent_id,
                classes,
                styles,
            };
            self.model.actors.insert(id, actor);
        } else {
            let shape = state.shape.unwrap_or(UseCaseShape::Ellipse);
            self.model.actors.shift_remove(&id);
            let styles = self.model.use_cases.get(&id).map(|use_case| use_case.styles.clone()).unwrap_or_default();
            let use_case =
                UseCase { id: id.clone(), label, label_type, shape, business, stereotype, parent_id, classes, styles };
            self.model.use_cases.insert(id, use_case);
        }
    }

    fn declare_element(&mut self, draft: &DraftElement) -> Checked {
        let named = &draft.named;
        self.claim(named, draft.kind.into())?;
        let (faults, boundaries) = (self.faults, self.boundary_drafts);
        let parent = draft.parent.filter(|&at| !boundaries[at].named.id.is_empty());
        let metadata = draft.metadata.as_deref().unwrap_or_default();
        let stereotype_span = draft.stereotype.as_ref().map(|_| named.span);
        let Some(existing) = self.elements.get_mut(&named.id) else {
            let mut state = ElementState {
                named: named.clone(),
                kind: draft.kind,
                parent,
                shape: draft.shape,
                stereotype: draft.stereotype.clone(),
                stereotype_span,
                classes: draft.classes.clone(),
                actor_type: None,
                icon: None,
                business: None,
            };
            faults.element_metadata(&mut state, metadata, false)?;
            self.elements.insert(named.id.clone(), state);
            self.register(named, draft.kind.into());
            return Ok(());
        };
        let id = shown(&named.id);
        let was = &existing.named;
        let clash = |message: String, current: Span, previous: Span| {
            Err(faults.conflict(message, (current, None), (previous, None)))
        };
        if was.label.text != named.label.text || was.label.kind != named.label.kind {
            return clash(format!("ID '{id}' has conflicting labels"), named.span, was.span);
        }
        if let (Some(shape), Some(previous)) = (draft.shape, existing.shape)
            && shape != previous
        {
            return clash(format!("Use case '{id}' has conflicting shapes"), named.span, was.span);
        }
        if let (Some(parent), Some(previous)) = (parent, existing.parent)
            && parent_id(boundaries, Some(previous)).is_some_and(|previous| *previous != boundaries[parent].named.id)
        {
            let message = format!("Element '{id}' belongs to more than one system boundary");
            return clash(message, boundaries[parent].named.span, boundaries[previous].named.span);
        }
        if let (Some(stereotype), Some(previous)) = (&draft.stereotype, &existing.stereotype)
            && stereotype != previous
        {
            let message = format!("Element '{id}' has conflicting stereotypes");
            return clash(message, named.span, existing.stereotype_span.unwrap_or(was.span));
        }
        existing.shape = existing.shape.or(draft.shape);
        existing.parent = existing.parent.or(draft.parent);
        if existing.stereotype.is_none() {
            existing.stereotype = draft.stereotype.clone();
        }
        existing.stereotype_span = existing.stereotype_span.or(stereotype_span);
        push_unique(&mut existing.classes, &draft.classes);
        faults.element_metadata(existing, metadata, false)
    }

    fn declare_boundary(&mut self, draft: &DraftBoundary) -> Checked {
        let named = &draft.named;
        self.claim(named, SymbolKind::Boundary)?;
        if let Some(existing) = self.boundaries.get_mut(&named.id) {
            let was = &existing.named;
            if was.label.text != named.label.text || was.label.kind != named.label.kind {
                let message = format!("Boundary '{}' has conflicting titles", shown(&named.id));
                return Err(self.faults.conflict(message, (named.span, None), (was.span, None)));
            }
            push_unique(&mut existing.classes, &draft.classes);
            return Ok(());
        }
        let state =
            BoundaryState { named: named.clone(), classes: draft.classes.clone(), kind: None, members: Vec::new() };
        self.boundaries.insert(named.id.clone(), state);
        self.register(named, SymbolKind::Boundary);
        Ok(())
    }

    /// An end of a relationship that nothing declares becomes a use case.
    fn endpoint(&mut self, end: &DraftEndpoint) -> Checked<SymbolKind> {
        let named = &end.named;
        if end.classed && !end.declares {
            let message = format!("Relationship endpoint '{}' uses ::: without declaring the node", shown(&named.id));
            return Err(self.faults.fail(message, named.span));
        }
        if let Some(origin) = self.symbols.get(&named.id) {
            return Ok(origin.kind);
        }
        let state = ElementState {
            named: named.clone(),
            kind: ElementKind::Usecase,
            parent: None,
            shape: Some(UseCaseShape::Ellipse),
            stereotype: None,
            stereotype_span: None,
            classes: Vec::new(),
            actor_type: None,
            icon: None,
            business: None,
        };
        self.elements.insert(named.id.clone(), state);
        self.register(named, SymbolKind::Usecase);
        self.mention(&named.id, named.span[0]);
        Ok(SymbolKind::Usecase)
    }

    /// Pele's `stylable`: the classes and the styles of what `id` names.
    fn stylable(&mut self, id: &JsText, span: Span) -> Checked<(&mut Vec<JsText>, &mut Vec<JsText>)> {
        let kind = self.symbols.get(id).map(|origin| origin.kind);
        let model = &mut self.model;
        let target = match kind {
            Some(SymbolKind::Actor) => model.actors.get_mut(id).map(|actor| (&mut actor.classes, &mut actor.styles)),
            Some(SymbolKind::Usecase) => model.use_cases.get_mut(id).map(|item| (&mut item.classes, &mut item.styles)),
            Some(SymbolKind::Boundary) => {
                model.system_boundaries.get_mut(id).map(|boundary| (&mut boundary.classes, &mut boundary.styles))
            }
            Some(SymbolKind::Json) => model.json_nodes.get_mut(id).map(|json| (&mut json.classes, &mut json.styles)),
            Some(SymbolKind::Edge) | None => {
                let at = self.edges.get(id).copied();
                at.map(|at| &mut model.relationships[at]).map(|edge| (&mut edge.classes, &mut edge.styles))
            }
        };
        let message = format!("Class/style target '{}' is unresolved or anonymous", shown(id));
        target.ok_or_else(|| self.faults.fail(message, span))
    }

    /// The ids of the elements in the order of their first mention.
    fn elements_in_order(&self) -> Vec<JsText> {
        let mut ids: Vec<JsText> = self.elements.keys().cloned().collect();
        ids.sort_by_key(|id| self.rank(id));
        ids
    }
}

/// What is declared at one place in the source.
enum Declaration {
    Element(usize),
    Boundary(usize),
    Json(usize),
    Edge(usize),
}

/// `text.replace(/\n\s+/g, '\n')`
fn dedent(text: &[u16]) -> JsText {
    let mut out = JsText::default();
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        at += 1;
        if text[at - 1] == 10 && text.at(at).is_some_and(is_space) {
            at = text.run(at, is_space);
        }
    }
    out
}

/// Pele's `resolveUsecase`.
pub(super) fn resolve(
    mut drafts: Drafts,
    title: Option<JsText>,
    positions: &Positions<'_>,
) -> Result<UsecaseModel, RenderError> {
    let model = UsecaseModel {
        actors: IndexMap::new(),
        use_cases: IndexMap::new(),
        system_boundaries: IndexMap::new(),
        relationships: Vec::new(),
        notes: IndexMap::new(),
        json_nodes: IndexMap::new(),
        class_defs: IndexMap::new(),
        direction: drafts.direction.unwrap_or(Direction::LR),
        title,
        acc_title: std::mem::take(&mut drafts.acc_title),
        acc_descr: dedent(&drafts.acc_description),
    };
    let json_values: Vec<Option<Json>> = drafts.jsons.iter_mut().map(|draft| draft.value.take()).collect();
    let drafts = &drafts;
    let mut r = Resolver {
        faults: Faults { positions },
        boundary_drafts: &drafts.boundaries,
        model,
        symbols: HashMap::new(),
        elements: IndexMap::new(),
        boundaries: IndexMap::new(),
        edges: HashMap::new(),
        first: HashMap::new(),
    };

    let mut declarations: Vec<(usize, Declaration)> = Vec::new();
    for DraftRelationship { source, target, .. } in &drafts.relationships {
        r.mention(&source.named.id, source.named.span[0]);
        r.mention(&target.named.id, target.named.span[0]);
    }
    for (at, draft) in drafts.elements.iter().enumerate() {
        r.mention(&draft.named.id, draft.named.span[0]);
        declarations.push((draft.named.span[0], Declaration::Element(at)));
    }
    for (at, draft) in drafts.boundaries.iter().enumerate() {
        r.mention(&draft.named.id, draft.named.span[0]);
        declarations.push((draft.named.span[0], Declaration::Boundary(at)));
    }
    for (at, draft) in drafts.jsons.iter().enumerate() {
        r.mention(&draft.id, draft.span[0]);
        declarations.push((draft.span[0], Declaration::Json(at)));
    }
    for (at, draft) in drafts.relationships.iter().enumerate() {
        if let Some((_, span)) = &draft.id {
            declarations.push((span[0], Declaration::Edge(at)));
        }
    }
    declarations.sort_by_key(|(offset, _)| *offset);
    for (_, declaration) in &declarations {
        match *declaration {
            Declaration::Element(at) => r.declare_element(&drafts.elements[at])?,
            Declaration::Boundary(at) => r.declare_boundary(&drafts.boundaries[at])?,
            Declaration::Json(at) => r.unique(&drafts.jsons[at].id, SymbolKind::Json, drafts.jsons[at].span)?,
            Declaration::Edge(at) => {
                let (id, span) = drafts.relationships[at].id.as_ref().expect("the edge has an id");
                r.unique(id, SymbolKind::Edge, *span)?;
            }
        }
    }

    for id in r.elements_in_order() {
        r.publish(&id);
    }
    let mut boundary_ids: Vec<JsText> = r.boundaries.keys().cloned().collect();
    boundary_ids.sort_by_key(|id| r.rank(id));
    for id in boundary_ids {
        let state = &r.boundaries[&id];
        let boundary = SystemBoundary {
            id: id.clone(),
            label: state.named.label.text.clone(),
            label_type: state.named.label.kind,
            kind: BoundaryType::Rect,
            members: Vec::new(),
            classes: Vec::new(),
            styles: Vec::new(),
        };
        r.model.system_boundaries.insert(id, boundary);
    }
    let mut jsons: Vec<(&DraftJson, Option<Json>)> = drafts.jsons.iter().zip(json_values).collect();
    jsons.sort_by_key(|(draft, _)| r.rank(&draft.id));
    for (draft, value) in jsons {
        let value = value.expect("every json draft is parsed before it is resolved");
        let node = JsonNode { id: draft.id.clone(), value, classes: draft.classes.clone(), styles: Vec::new() };
        r.model.json_nodes.insert(draft.id.clone(), node);
    }

    let mut anonymous = 0;
    for draft in &drafts.relationships {
        let DraftRelationship { source, target, kind, .. } = draft;
        let source_kind = r.endpoint(source)?;
        let target_kind = r.endpoint(target)?;
        let related = |kind| matches!(kind, SymbolKind::Actor | SymbolKind::Usecase | SymbolKind::Json);
        for (end, end_kind, role) in [(source, source_kind, "source"), (target, target_kind, "target")] {
            if !related(end_kind) {
                let message = format!("Relationship {role} '{}' cannot be {}", shown(&end.named.id), end_kind.name());
                let previous = r.symbols[&end.named.id].span;
                return Err(r.faults.conflict(message, (end.named.span, None), (previous, None)));
            }
        }
        let use_cases = source_kind == SymbolKind::Usecase && target_kind == SymbolKind::Usecase;
        if matches!(kind, RelationshipType::Include | RelationshipType::Extend) && !use_cases {
            let message = format!("{} relationship requires use-case endpoints", kind.name());
            return Err(r.faults.fail(message, draft.span));
        }
        if *kind == RelationshipType::Generalization && (source_kind == SymbolKind::Json || source_kind != target_kind)
        {
            let message = "Generalization requires actor-to-actor or use-case-to-use-case endpoints".to_owned();
            return Err(r.faults.fail(message, draft.span));
        }
        let json = source_kind == SymbolKind::Json || target_kind == SymbolKind::Json;
        if *kind == RelationshipType::Association && json && draft.arrow.number() > 2 {
            let message = format!(
                "JSON relationship '{}' to '{}' permits only point, reversed-point, or markerless solid association",
                shown(&source.named.id),
                shown(&target.named.id)
            );
            return Err(r.faults.fail(message, draft.span));
        }
        let id = match &draft.id {
            Some((id, _)) => id.clone(),
            None => {
                anonymous += 1;
                JsText::from(format!("edge-{}", anonymous - 1).as_str())
            }
        };
        let relationship = Relationship {
            id: id.clone(),
            explicit_id: draft.id.is_some(),
            source: source.named.id.clone(),
            target: target.named.id.clone(),
            kind: *kind,
            arrow: draft.arrow,
            label: draft.label.as_ref().map(|label| label.text.clone()),
            label_type: draft.label.as_ref().map(|label| label.kind),
            minlen: draft.minlen,
            classes: Vec::new(),
            styles: Vec::new(),
            animate: false,
            animation: None,
        };
        r.model.relationships.push(relationship);
        r.edges.insert(id, r.model.relationships.len() - 1);
    }

    // The relationships may have added use cases; put all elements back in the order of first mention.
    let mut declared_actors = std::mem::take(&mut r.model.actors);
    let mut declared_use_cases = std::mem::take(&mut r.model.use_cases);
    for id in r.elements_in_order() {
        if let Some(actor) = declared_actors.swap_remove(&id) {
            r.model.actors.insert(id, actor);
        } else if let Some(use_case) = declared_use_cases.swap_remove(&id) {
            r.model.use_cases.insert(id, use_case);
        } else {
            r.publish(&id);
        }
    }

    for (target, span, properties) in &drafts.metadata {
        let Some(origin) = r.symbols.get(target) else {
            let implied = infer_kind(properties).map_or(String::new(), |kind| format!(" (metadata implies {kind})"));
            return Err(r.faults.fail(format!("Metadata target '{}' is unresolved{implied}", shown(target)), *span));
        };
        let kind = origin.kind;
        if matches!(kind, SymbolKind::Actor | SymbolKind::Usecase) {
            let state = r.elements.get_mut(target).expect("an element's id names an element");
            r.faults.element_metadata(state, properties, true)?;
            continue;
        }
        for property in properties {
            let DraftProperty { key, value, .. } = property;
            let boundary_type = [("rect", BoundaryType::Rect), ("package", BoundaryType::Package)]
                .into_iter()
                .find(|(name, _)| value.is(name))
                .map(|(_, kind)| kind);
            let animation = [("fast", Animation::Fast), ("slow", Animation::Slow)]
                .into_iter()
                .find(|(name, _)| value.is(name))
                .map(|(_, kind)| kind);
            let edge = r.edges.get(target).copied().filter(|_| kind == SymbolKind::Edge);
            let boundary = r.boundaries.get_mut(target).filter(|_| kind == SymbolKind::Boundary);
            match (boundary, edge, value) {
                (Some(boundary), _, _) if key.is_str("type") && boundary_type.is_some() => {
                    boundary.kind = boundary_type;
                }
                (_, Some(edge), MetaValue::Bool(animate)) if key.is_str("animate") => {
                    r.model.relationships[edge].animate = *animate;
                }
                (_, Some(edge), _) if key.is_str("animation") && animation.is_some() => {
                    r.model.relationships[edge].animation = animation;
                    r.model.relationships[edge].animate = true;
                }
                _ => return Err(r.faults.invalid(target, kind.name(), property)),
            }
        }
    }
    for relationship in &mut r.model.relationships {
        if relationship.animation.is_some() {
            relationship.animate = true;
        }
    }

    let declared: Vec<JsText> = r.elements.keys().cloned().collect();
    for id in declared {
        let state = &r.elements[&id];
        let (name, span) = (shown(&id), state.named.span);
        let icon = state.icon.as_ref().is_some_and(|icon| !icon.is_empty());
        let business = state.business == Some(true);
        if state.kind == ElementKind::Actor {
            if let Some(kind) = state.actor_type.filter(|&kind| icon && kind != ActorType::Normal) {
                let message = format!("Actor '{name}' cannot combine icon with type '{}'", kind.name());
                return Err(r.faults.fail(message, span));
            }
            if business && (icon || state.actor_type == Some(ActorType::Awesome)) {
                let message = format!("Business actor '{name}' must use normal or hollow geometry");
                return Err(r.faults.fail(message, span));
            }
        } else if state.shape == Some(UseCaseShape::Rect) && business {
            let message = format!("Rectangular use case '{name}' cannot be a business use case");
            return Err(r.faults.fail(message, span));
        }
        r.publish(&id);
    }

    let mut members: Vec<&DraftElement> = drafts.elements.iter().collect();
    members.sort_by_key(|draft| draft.named.span[0]);
    for draft in members {
        if let Some(parent) = parent_id(&drafts.boundaries, draft.parent) {
            let members = &mut r.boundaries.get_mut(parent).expect("a boundary that has members is declared").members;
            if !members.contains(&draft.named.id) {
                members.push(draft.named.id.clone());
            }
        }
    }
    for (id, state) in std::mem::take(&mut r.boundaries) {
        let boundary = r.model.system_boundaries.get_mut(&id).expect("every boundary is published");
        boundary.kind = state.kind.unwrap_or(BoundaryType::Rect);
        boundary.members = state.members;
        boundary.classes = state.classes;
    }

    for (count, (target, span, label)) in drafts.notes.iter().enumerate() {
        let Some(origin) = r.symbols.get(target) else {
            return Err(r.faults.fail(format!("Note target '{}' is unresolved", shown(target)), *span));
        };
        if !matches!(origin.kind, SymbolKind::Actor | SymbolKind::Usecase) {
            let message =
                format!("Note target '{}' must be an actor or use case, not {}", shown(target), origin.kind.name());
            return Err(r.faults.conflict(message, (*span, None), (origin.span, None)));
        }
        let id = JsText::from(format!("note-{count}").as_str());
        let note = Note { id: id.clone(), target: target.clone(), label: label.text.clone(), label_type: label.kind };
        r.model.notes.insert(id, note);
    }

    for (ids, styles) in &drafts.class_defs {
        for id in ids {
            r.model.class_defs.insert(id.clone(), ClassDef { id: id.clone(), styles: styles.clone() });
        }
    }
    for ClassAssignment { targets, classes } in &drafts.classes {
        for (target, span) in targets {
            push_unique(r.stylable(target, *span)?.0, classes);
        }
    }
    for (target, span, styles) in &drafts.styles {
        r.stylable(target, *span)?.1.extend(styles.iter().cloned());
    }

    Ok(r.model)
}
