//! The store the class parser fills, ported from Pele's `src/diagrams/class/db.ts`.

use super::{
    ClassInterface, ClassMember, ClassModel, ClassNode, ClassNote, ClassRelation, Direction, LinkTarget, MemberType,
    NamespaceNode, RelationEnd, StyleClass,
};
use crate::input::url::safe_url;
use crate::input::{JsText, Units, decode_entities, is_digit, split_unit};
use indexmap::{IndexMap, IndexSet};
use std::collections::HashMap;

/// Pele's `splitName`: a name and the generic type written after it, as in `List~T~`.
fn split_name(id: &[u16]) -> (&[u16], &[u16]) {
    match id.find_unit(126, 0) {
        Some(at) if at > 0 => {
            let end = id.find_unit(126, at + 1).unwrap_or(id.len());
            (&id[..at], &id[at + 1..end])
        }
        _ => (id, &[]),
    }
}

fn class_name(id: &[u16]) -> &[u16] {
    split_name(id).0
}

/// Pele's `lookupId`: Mermaid looks up ids that start with a digit under the prefix `classId-`,
/// so they never match a class of that name.
fn lookup_id(id: &[u16]) -> JsText {
    if id.at(0).is_some_and(is_digit) {
        let mut prefixed = JsText::from("classId-");
        prefixed.push_units(id);
        class_name(&prefixed).into()
    } else {
        class_name(id).into()
    }
}

pub(super) struct ClassDb {
    pub(super) model: ClassModel,
    namespace_stack: Vec<JsText>,
    /// The classes each class name was given to, by class id, in the order they were given it.
    with_css_class: HashMap<JsText, IndexSet<JsText>>,
}

impl ClassDb {
    pub(super) fn new() -> Self {
        ClassDb {
            model: ClassModel {
                direction: Direction::TB,
                classes: IndexMap::new(),
                relations: Vec::new(),
                notes: IndexMap::new(),
                namespaces: IndexMap::new(),
                interfaces: Vec::new(),
                style_classes: IndexMap::new(),
                title: None,
                acc_title: None,
                acc_descr: None,
            },
            namespace_stack: Vec::new(),
            with_css_class: HashMap::new(),
        }
    }

    pub(super) fn add_class(&mut self, id: &[u16]) {
        let (name, generic) = split_name(id);
        if self.model.classes.contains_key(name) {
            return;
        }
        let node = ClassNode {
            id: name.into(),
            generic: generic.into(),
            label: name.into(),
            css_classes: "default".into(),
            methods: Vec::new(),
            members: Vec::new(),
            annotations: Vec::new(),
            styles: Vec::new(),
            parent: None,
            link: None,
            link_target: None,
            have_callback: false,
            tooltip: None,
        };
        self.model.classes.insert(name.into(), node);
        self.tag(name, "default".encode_utf16().collect::<Vec<_>>().as_slice());
    }

    fn tag(&mut self, class: &[u16], css_class: &[u16]) {
        self.with_css_class.entry(css_class.into()).or_default().insert(class.into());
    }

    /// The class for a name that may carry a generic type, created if needed. Mermaid throws a
    /// TypeError where this creates, as for an annotation on a class not yet declared.
    fn ensure(&mut self, id: &[u16]) -> &mut ClassNode {
        self.add_class(id);
        self.model.classes.get_mut(class_name(id)).expect("added above")
    }

    pub(super) fn set_class_label(&mut self, id: &[u16], label: &[u16]) {
        self.ensure(id).label = label.into();
    }

    pub(super) fn add_relation(&mut self, mut relation: ClassRelation) {
        let (type1, type2) = (relation.type1, relation.type2);
        if type1 == RelationEnd::Lollipop && type2 == RelationEnd::None {
            self.add_class(&relation.id2.clone());
            relation.id1 = self.add_interface(&relation.id1, &relation.id2);
        } else if type2 == RelationEnd::Lollipop && type1 == RelationEnd::None {
            self.add_class(&relation.id1.clone());
            relation.id2 = self.add_interface(&relation.id2, &relation.id1);
        } else {
            self.add_class(&relation.id1.clone());
            self.add_class(&relation.id2.clone());
        }
        relation.id1 = class_name(&relation.id1).into();
        relation.id2 = class_name(&relation.id2).into();
        relation.relation_title1 = relation.relation_title1.trim_js().into();
        relation.relation_title2 = relation.relation_title2.trim_js().into();
        self.model.relations.push(relation);
    }

    fn add_interface(&mut self, label: &[u16], class_id: &[u16]) -> JsText {
        let id = JsText::from(format!("interface{}", self.model.interfaces.len()).as_str());
        self.model.interfaces.push(ClassInterface { id: id.clone(), label: label.into(), class_id: class_id.into() });
        id
    }

    pub(super) fn add_annotation(&mut self, class: &[u16], annotation: &[u16]) {
        self.ensure(class).annotations.push(annotation.into());
    }

    pub(super) fn add_member(&mut self, class: &[u16], member: &[u16]) {
        let node = self.ensure(class);
        let text = member.trim_js();
        if text.starts_at(0, "<<") && text.len() >= 2 && text[text.len() - 2..] == [62, 62] {
            // `substring(2, length - 2)`, whose bounds swap when they cross.
            let (from, to) = (2, text.len() - 2);
            node.annotations.push(text[from.min(to)..from.max(to)].into());
        } else if text.find_unit(41, 0).is_some_and(|at| at > 0) {
            let parsed = ClassMember::new(text, MemberType::Method);
            if parsed.member_type == MemberType::Method {
                node.methods.push(parsed);
            } else {
                node.members.push(parsed);
            }
        } else if !text.is_empty() {
            node.members.push(ClassMember::new(text, MemberType::Attribute));
        }
    }

    /// Takes the members as the grammar collects them, last to first.
    pub(super) fn add_members(&mut self, class: &[u16], mut members: Vec<&[u16]>) {
        members.reverse();
        for member in members {
            self.add_member(class, member);
        }
    }

    pub(super) fn add_note(&mut self, text: &[u16], class: Option<&[u16]>) -> JsText {
        let index = self.model.notes.len();
        let id = JsText::from(format!("note{index}").as_str());
        let note = ClassNote { id: id.clone(), class: class.map(JsText::from), text: text.into(), index, parent: None };
        self.model.notes.insert(id.clone(), note);
        id
    }

    pub(super) fn cleanup_label(label: &[u16]) -> JsText {
        let label = if label.at(0) == Some(58) { &label[1..] } else { label };
        label.trim_js().into()
    }

    pub(super) fn set_css_class(&mut self, ids: &[u16], class: &[u16]) {
        for id in split_unit(ids, 44) {
            let id = lookup_id(id);
            let Some(node) = self.model.classes.get_mut(&id) else {
                continue;
            };
            node.css_classes.push(32);
            node.css_classes.push_units(class);
            for word in split_unit(class, 32) {
                self.tag(&id, word);
            }
        }
    }

    pub(super) fn define_class(&mut self, ids: &[&[u16]], styles: &[JsText]) {
        for &id in ids {
            let style_class = self.model.style_classes.entry(id.into()).or_insert_with(|| StyleClass {
                id: id.into(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
            for style in styles {
                if style.has("color") {
                    style_class.text_styles.push(replace_first(style, "fill", "bgFill"));
                }
                style_class.styles.push(style.clone());
            }
            // Mermaid tests `cssClasses.includes(id)`, which also matches part of a longer name
            // (a classDef named `def` styles every class, through `default`). Whole names are
            // compared here.
            let Some(tagged) = self.with_css_class.get(id) else {
                continue;
            };
            let parts: Vec<&[u16]> = styles.iter().flat_map(|style| split_unit(style, 44)).collect();
            for class in tagged {
                let node = self.model.classes.get_mut(class).expect("a tagged class exists");
                node.styles.extend(parts.iter().map(|&part| JsText::from(part)));
            }
        }
    }

    pub(super) fn set_tooltip(&mut self, ids: &[u16], tooltip: &[u16]) {
        for id in split_unit(ids, 44) {
            if let Some(node) = self.model.classes.get_mut(class_name(id)) {
                node.tooltip = Some(tooltip.into());
            }
        }
    }

    pub(super) fn set_link(&mut self, ids: &[u16], link: &[u16], target: Option<&[u16]>) {
        // Entity codes are resolved first, so that what is checked is what a browser would follow.
        let decoded = decode_entities(link);
        let url = decoded.trim_js();
        for id in split_unit(ids, 44) {
            if let Some(node) = self.model.classes.get_mut(&lookup_id(id)) {
                node.link = (!url.is_empty()).then(|| safe_url(url));
                node.link_target = Some(target.map_or(LinkTarget::Blank, |target| {
                    LinkTarget::named(target).expect("a LINK_TARGET token names a target")
                }));
            }
        }
        self.set_css_class(ids, &"clickable".encode_utf16().collect::<Vec<_>>());
    }

    /// Callbacks are never executed; the class is only marked clickable, as in Mermaid's strict
    /// mode.
    pub(super) fn set_click_event(&mut self, ids: &[u16]) {
        for id in split_unit(ids, 44) {
            if let Some(node) = self.model.classes.get_mut(class_name(id)) {
                node.have_callback = true;
            }
        }
        self.set_css_class(ids, &"clickable".encode_utf16().collect::<Vec<_>>());
    }

    pub(super) fn set_direction(&mut self, direction: Direction) {
        self.model.direction = direction;
    }

    pub(super) fn set_acc_title(&mut self, text: &[u16]) {
        self.model.acc_title = Some(text.trim_start_js().into());
    }

    /// `text.replace(/\n\s+/g, '\n')`
    pub(super) fn set_acc_description(&mut self, text: &[u16]) {
        let mut out = JsText::default();
        let mut at = 0;
        while at < text.len() {
            let c = text[at];
            out.push(c);
            at += 1;
            if c == 10 {
                let end = text.run(at, crate::input::is_space);
                if end > at {
                    // `\s+` is greedy, so the run ends at the last whitespace; a line feed in it
                    // is swallowed with the rest.
                    at = end;
                }
            }
        }
        self.model.acc_descr = Some(out);
    }

    pub(super) fn add_namespace(&mut self, id: &[u16], label: Option<&[u16]>) -> JsText {
        let prefix = self.namespace_stack.last().cloned();
        let qualified: JsText = match &prefix {
            Some(prefix) if !prefix.is_empty() => {
                let mut qualified = prefix.clone();
                qualified.push(46);
                qualified.push_units(id);
                qualified
            }
            _ => id.into(),
        };
        self.namespace_stack.push(qualified.clone());
        let label = label.filter(|label| !label.is_empty());

        if let Some(existing) = self.model.namespaces.get_mut(&qualified) {
            existing.explicit = true;
            if let Some(label) = label {
                existing.label = label.into();
            }
            return qualified;
        }

        // The enclosing namespace and its ancestors exist already, so only the parts of `id`
        // are walked.
        let prefix = prefix.filter(|prefix| !prefix.is_empty());
        let mut parent_id = prefix.clone();
        let mut from = prefix.as_ref().map_or(0, |prefix| prefix.len() + 1);
        loop {
            let dot = qualified.find_unit(46, from);
            let leaf = dot.is_none();
            let current: JsText = match dot {
                Some(dot) => qualified[..dot].into(),
                None => qualified.clone(),
            };
            if !self.model.namespaces.contains_key(&current) {
                let part: JsText = qualified[from..dot.unwrap_or(qualified.len())].into();
                let node = NamespaceNode {
                    id: current.clone(),
                    label: if leaf { label.map_or(part.clone(), JsText::from) } else { part },
                    classes: IndexSet::new(),
                    notes: IndexSet::new(),
                    children: IndexSet::new(),
                    parent: parent_id.clone(),
                    explicit: leaf,
                };
                self.model.namespaces.insert(current.clone(), node);
            } else if leaf {
                self.model.namespaces.get_mut(&current).expect("present").explicit = true;
            }
            if let Some(parent) = parent_id.as_ref().and_then(|parent| self.model.namespaces.get_mut(parent)) {
                parent.children.insert(current.clone());
                let parent = parent_id.clone();
                let node = self.model.namespaces.get_mut(&current).expect("present");
                if node.parent.is_none() {
                    node.parent = parent;
                }
            }
            match dot {
                None => return qualified,
                Some(dot) => {
                    parent_id = Some(current);
                    from = dot + 1;
                }
            }
        }
    }

    pub(super) fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
    }

    pub(super) fn add_classes_to_namespace(&mut self, id: &[u16], classes: &[JsText], notes: &[JsText]) {
        let Some(namespace) = self.model.namespaces.get_mut(id) else {
            return;
        };
        for name in classes {
            let name = class_name(name);
            let Some(node) = self.model.classes.get_mut(name) else {
                continue;
            };
            node.parent = Some(id.into());
            namespace.classes.insert(name.into());
        }
        for name in notes {
            let Some(note) = self.model.notes.get_mut(&**name) else {
                continue;
            };
            note.parent = Some(id.into());
            namespace.notes.insert(name.clone());
        }
    }

    pub(super) fn set_css_style(&mut self, id: &[u16], styles: &[JsText]) {
        let Some(node) = self.model.classes.get_mut(id) else {
            return;
        };
        for style in styles {
            node.styles.extend(split_unit(style, 44).into_iter().map(JsText::from));
        }
    }
}

/// `text.replace(from, to)`: the first match only.
fn replace_first(text: &[u16], from: &str, to: &str) -> JsText {
    match text.find_str(from, 0) {
        Some(at) => {
            let mut out = JsText::from(&text[..at]);
            out.push_str(to);
            out.push_units(&text[at + from.encode_utf16().count()..]);
            out
        }
        None => text.into(),
    }
}
