//! Pele's `RequirementDb` (`src/diagrams/requirement/db.ts`): what each statement does to the model.

use super::{ClassDef, Element, Relation, Relationship, Requirement, RequirementModel, Styling};
use crate::input::{JsText, Units, is_space};
use crate::layout::Dir;
use crate::{Diagnostic, ErrorCode, RenderError};
use indexmap::IndexMap;

// Applying a class copies its declarations onto each node, so a short text can ask for hundreds
// of millions of copies. Past this many the diagram is refused.
const MAX_STYLE_COPIES: usize = 1_000_000;

/// The fields of one requirement's body. Of two that repeat, the first wins.
#[derive(Default)]
pub(super) struct RequirementFields {
    pub(super) id: Option<JsText>,
    pub(super) text: Option<JsText>,
    pub(super) risk: Option<super::Risk>,
    pub(super) verify_method: Option<super::VerifyMethod>,
}

/// The fields of one element's body. Of two that repeat, the first wins.
#[derive(Default)]
pub(super) struct ElementFields {
    pub(super) kind: Option<JsText>,
    pub(super) doc_ref: Option<JsText>,
}

pub(super) struct RequirementDb {
    pub(super) model: RequirementModel,
    style_copies: usize,
}

/// `s.split(',')`
fn split_commas(text: &[u16]) -> impl Iterator<Item = JsText> + '_ {
    text.split(|&c| c == u16::from(b',')).map(JsText::from)
}

impl RequirementDb {
    pub(super) fn new() -> Self {
        let model = RequirementModel {
            direction: Dir::TB,
            requirements: IndexMap::new(),
            elements: IndexMap::new(),
            relations: Vec::new(),
            classes: IndexMap::new(),
            title: None,
            acc_title: None,
            acc_descr: None,
        };
        RequirementDb { model, style_copies: 0 }
    }

    /// Counts the copies, and refuses the diagram past the limit before making them.
    fn count(&mut self, copies: usize) -> Result<(), RenderError> {
        self.style_copies += copies;
        if self.style_copies > MAX_STYLE_COPIES {
            let message = "Requirement diagram applies too many style declarations.".to_owned();
            return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, message)));
        }
        Ok(())
    }

    pub(super) fn set_direction(&mut self, dir: Dir) {
        self.model.direction = dir;
    }

    pub(super) fn add_requirement(&mut self, name: &[u16], kind: super::RequirementType, fields: RequirementFields) {
        self.model.requirements.entry(name.into()).or_insert_with(|| Requirement {
            name: name.into(),
            kind,
            requirement_id: fields.id.unwrap_or_default(),
            text: fields.text.unwrap_or_default(),
            risk: fields.risk,
            verify_method: fields.verify_method,
            styling: Styling::default(),
        });
    }

    pub(super) fn add_element(&mut self, name: &[u16], fields: ElementFields) {
        self.model.elements.entry(name.into()).or_insert_with(|| Element {
            name: name.into(),
            kind: fields.kind.unwrap_or_default(),
            doc_ref: fields.doc_ref.unwrap_or_default(),
            styling: Styling::default(),
        });
    }

    pub(super) fn add_relationship(&mut self, kind: Relationship, src: &[u16], dst: &[u16]) {
        self.model.relations.push(Relation { kind, src: src.into(), dst: dst.into() });
    }

    /// A requirement of the name, or else an element of it.
    fn node(&mut self, id: &[u16]) -> Option<&mut Styling> {
        let model = &mut self.model;
        match model.requirements.get_mut(id) {
            Some(requirement) => Some(&mut requirement.styling),
            None => model.elements.get_mut(id).map(|element| &mut element.styling),
        }
    }

    pub(super) fn set_css_style(&mut self, ids: &[JsText], styles: &[JsText]) -> Result<(), RenderError> {
        for id in ids {
            // Mermaid stops at the first unknown id, leaving the rest of the list unstyled.
            if self.node(id).is_none() {
                return Ok(());
            }
            for style in styles {
                let parts: Vec<JsText> = split_commas(style).collect();
                self.count(parts.len())?;
                self.node(id).expect("found above").css_styles.extend(parts);
            }
        }
        Ok(())
    }

    pub(super) fn set_class(&mut self, ids: &[JsText], names: &[JsText]) -> Result<(), RenderError> {
        for id in ids {
            if self.node(id).is_none() {
                continue;
            }
            for name in names {
                self.node(id).expect("found above").classes.push(name.clone());
                let styles = self.model.classes.get(&**name).map(|class| class.styles.clone());
                if let Some(styles) = styles {
                    self.count(styles.len())?;
                    self.node(id).expect("found above").css_styles.extend(styles);
                }
            }
        }
        Ok(())
    }

    pub(super) fn define_class(&mut self, ids: &[JsText], style: &[JsText]) -> Result<(), RenderError> {
        for id in ids {
            let class = self.model.classes.entry(id.clone()).or_insert_with(|| ClassDef {
                id: id.clone(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
            for s in style {
                if s.has("color") {
                    class.text_styles.push(replace_first(s, "fill", "bgFill"));
                }
                class.styles.push(s.clone());
            }
            let split: Vec<JsText> = style.iter().flat_map(|s| split_commas(s)).collect();
            let has = |styling: &Styling| styling.classes.iter().any(|name| **name == **id);
            let requirements = self.model.requirements.values().filter(|r| has(&r.styling)).count();
            let elements = self.model.elements.values().filter(|e| has(&e.styling)).count();
            for _ in 0..requirements + elements {
                self.count(split.len())?;
            }
            for styling in self
                .model
                .requirements
                .values_mut()
                .map(|r| &mut r.styling)
                .chain(self.model.elements.values_mut().map(|e| &mut e.styling))
            {
                if has(styling) {
                    styling.css_styles.extend(split.iter().cloned());
                }
            }
        }
        Ok(())
    }

    /// `text.replace(/^\s+/g, '')`
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
            if c == 10 && text.at(at).is_some_and(is_space) {
                at = text.run(at, is_space);
            }
        }
        self.model.acc_descr = Some(out);
    }
}

/// `text.replace(from, to)` for a string `from`: the first match only.
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
