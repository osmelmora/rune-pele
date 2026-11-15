//! Cynefin diagrams, ported from Pele's `src/diagrams/cynefin`. The parser reads the domain
//! blocks and transitions, `boundaries` draws Mermaid's seeded wavy lines between the domains,
//! and `render` places the five domains, their items and the transitions between them.

mod boundaries;
pub(crate) mod json;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsText, Units, is_space};

pub(crate) use render::Cynefin;

/// One of the five domains of the framework.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Domain {
    Complex,
    Complicated,
    Clear,
    Chaotic,
    Confusion,
}

impl Domain {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Domain::Complex => "complex",
            Domain::Complicated => "complicated",
            Domain::Clear => "clear",
            Domain::Chaotic => "chaotic",
            Domain::Confusion => "confusion",
        }
    }
}

/// A move from one domain to another, with an optional label.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Transition {
    pub(crate) from: Domain,
    pub(crate) to: Domain,
    pub(crate) label: Option<JsText>,
}

/// Pele's `CynefinModel`. `domains` holds the blocks the source declares, in the order each
/// domain was first declared, each with the items of its last block.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CynefinModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) domains: Vec<(Domain, Vec<JsText>)>,
    pub(crate) transitions: Vec<Transition>,
}

impl CynefinModel {
    pub(crate) fn items(&self, domain: Domain) -> &[JsText] {
        self.domains.iter().find(|(name, _)| *name == domain).map_or(&[], |(_, items)| items)
    }
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

/// Pele's `buildCynefin`: a later block for a domain replaces an earlier one, a transition from
/// a domain to itself is dropped, and an empty label is none.
fn build(ast: parser::CynefinAst, front_title: Option<&JsText>) -> CynefinModel {
    let mut domains: Vec<(Domain, Vec<JsText>)> = Vec::new();
    for (domain, items) in ast.domains {
        let items = items.into_iter().map(JsText::from).collect();
        match domains.iter_mut().find(|(name, _)| *name == domain) {
            Some(block) => block.1 = items,
            None => domains.push((domain, items)),
        }
    }
    let transitions = ast
        .transitions
        .into_iter()
        .filter(|(from, to, _)| from != to)
        .map(|(from, to, label)| Transition {
            from,
            to,
            label: label.filter(|text| !text.is_empty()).map(JsText::from),
        })
        .collect();
    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    CynefinModel {
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        acc_title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
        acc_descr: filled(ast.acc_descr).map(|text| unindent(&text).into()),
        domains,
        transitions,
    }
}

/// Pele's cynefin `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<CynefinModel, RenderError> {
    let ast = parser::parse(cx.text).map_err(|error| cx.syntax(error))?;
    Ok(build(ast, cx.title))
}
