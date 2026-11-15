//! Requirement diagrams, ported from Pele's `src/diagrams/requirement`. The lexer and the parser
//! fill Pele's `RequirementDb`.

mod db;
pub(crate) mod json;
mod lexer;
mod parser;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;
use crate::layout::Dir;
use indexmap::IndexMap;

/// Pele's `RequirementType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequirementType {
    Requirement,
    Functional,
    Interface,
    Performance,
    Physical,
    DesignConstraint,
}

impl RequirementType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RequirementType::Requirement => "Requirement",
            RequirementType::Functional => "Functional Requirement",
            RequirementType::Interface => "Interface Requirement",
            RequirementType::Performance => "Performance Requirement",
            RequirementType::Physical => "Physical Requirement",
            RequirementType::DesignConstraint => "Design Constraint",
        }
    }
}

/// Pele's `RiskLevel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Risk {
    Low,
    Medium,
    High,
}

impl Risk {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Risk::Low => "Low",
            Risk::Medium => "Medium",
            Risk::High => "High",
        }
    }
}

/// Pele's `VerifyType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VerifyMethod {
    Analysis,
    Demonstration,
    Inspection,
    Test,
}

impl VerifyMethod {
    pub(crate) fn name(self) -> &'static str {
        match self {
            VerifyMethod::Analysis => "Analysis",
            VerifyMethod::Demonstration => "Demonstration",
            VerifyMethod::Inspection => "Inspection",
            VerifyMethod::Test => "Test",
        }
    }
}

/// Pele's `Relationships`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Relationship {
    Contains,
    Copies,
    Derives,
    Satisfies,
    Verifies,
    Refines,
    Traces,
}

impl Relationship {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Relationship::Contains => "contains",
            Relationship::Copies => "copies",
            Relationship::Derives => "derives",
            Relationship::Satisfies => "satisfies",
            Relationship::Verifies => "verifies",
            Relationship::Refines => "refines",
            Relationship::Traces => "traces",
        }
    }
}

/// The styles and classes a source gives a requirement or an element. Every list of classes
/// starts with `default`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Styling {
    pub(crate) css_styles: Vec<JsText>,
    pub(crate) classes: Vec<JsText>,
}

impl Default for Styling {
    fn default() -> Self {
        Styling { css_styles: Vec::new(), classes: vec![JsText::from("default")] }
    }
}

impl Styling {
    /// The classes as one string, which Pele's graph calls `cssClasses`.
    pub(crate) fn css_classes(&self) -> JsText {
        let mut out = JsText::default();
        for (index, class) in self.classes.iter().enumerate() {
            if index > 0 {
                out.push(32);
            }
            out.push_units(class);
        }
        out
    }
}

/// Pele's `Requirement`. A field the body leaves out is empty or `None`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Requirement {
    pub(crate) name: JsText,
    pub(crate) kind: RequirementType,
    pub(crate) requirement_id: JsText,
    pub(crate) text: JsText,
    pub(crate) risk: Option<Risk>,
    pub(crate) verify_method: Option<VerifyMethod>,
    pub(crate) styling: Styling,
}

/// Pele's `RequirementElement`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Element {
    pub(crate) name: JsText,
    pub(crate) kind: JsText,
    pub(crate) doc_ref: JsText,
    pub(crate) styling: Styling,
}

/// Pele's `Relation`: `src` relates to `dst`, which need not exist.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Relation {
    pub(crate) kind: Relationship,
    pub(crate) src: JsText,
    pub(crate) dst: JsText,
}

impl Relation {
    /// The id Pele's graph gives the relation at `index`: its two ends and its place in the source.
    pub(crate) fn id(&self, index: usize) -> JsText {
        let mut id = self.src.clone();
        id.push(u16::from(b'-'));
        id.push_units(&self.dst);
        id.push_str(&format!("-{index}"));
        id
    }
}

/// Pele's `RequirementClass`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassDef {
    pub(crate) id: JsText,
    pub(crate) styles: Vec<JsText>,
    pub(crate) text_styles: Vec<JsText>,
}

/// Pele's `RequirementModel`, as its `RequirementDb` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RequirementModel {
    pub(crate) direction: Dir,
    pub(crate) requirements: IndexMap<JsText, Requirement>,
    pub(crate) elements: IndexMap<JsText, Element>,
    pub(crate) relations: Vec<Relation>,
    pub(crate) classes: IndexMap<JsText, ClassDef>,
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
}

/// The name Pele gives a direction.
pub(crate) fn direction_name(dir: Dir) -> &'static str {
    match dir {
        Dir::TB => "TB",
        Dir::BT => "BT",
        Dir::RL => "RL",
        Dir::LR => "LR",
    }
}

/// Pele's requirement `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<RequirementModel, RenderError> {
    let mut db = db::RequirementDb::new();
    db.model.title = cx.title.filter(|title| !title.is_empty()).cloned();
    parser::parse(cx, &mut db)?;
    Ok(db.model)
}
