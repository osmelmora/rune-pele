//! Treemap diagrams, ported from Pele's `src/diagrams/treemap`. The parser reads rows of sections,
//! leaves and class definitions, the model nests each row under the nearest section above it that
//! is indented less, `layout` squarifies the tree into boxes, `format` writes the values, and
//! `render` draws the boxes.

mod format;
pub(crate) mod json;
mod layout;
mod parser;
pub mod probe;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::{JsText, Units};
use indexmap::IndexMap;
use parser::{Row, TreemapAst};

pub(crate) use render::Treemap;

/// What a node of the tree is: a section with its rows, which may be none, or a leaf with the
/// value it is written with, NaN where that is no number.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    Section(Vec<TreemapNode>),
    Leaf(f64),
}

/// Pele's `TreemapNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreemapNode {
    pub(crate) name: JsText,
    pub(crate) content: Content,
    pub(crate) class_selector: Option<JsText>,
    /// The declarations of the node's class, where it names one that has any.
    pub(crate) css_compiled_styles: Option<Vec<JsText>>,
}

/// Pele's `TreemapModel`. `classes` keeps each class in the order it was first defined.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreemapModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) roots: Vec<TreemapNode>,
    pub(crate) classes: IndexMap<JsText, Vec<JsText>>,
}

/// `text.replace(/\\,/g, '§§§').replace(/,/g, ';').replace(/§§§/g, ',').split(';')`: an escaped
/// comma stays in its declaration, and the other commas separate declarations.
fn declarations(text: &[u16]) -> Vec<JsText> {
    const MARK: u16 = 0xa7;
    let mut marked = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if text[at] == 92 && text.at(at + 1) == Some(44) {
            marked.extend([MARK; 3]);
            at += 2;
        } else {
            marked.push(if text[at] == 44 { 59 } else { text[at] });
            at += 1;
        }
    }
    let mut unmarked = Vec::with_capacity(marked.len());
    let mut at = 0;
    while at < marked.len() {
        if marked[at..].starts_with(&[MARK; 3]) {
            unmarked.push(44);
            at += 3;
        } else {
            unmarked.push(marked[at]);
            at += 1;
        }
    }
    unmarked.split(|&c| c == 59).map(JsText::from).collect()
}

/// `text.replace(/\n\s+/g, '\n')`
fn unindent(text: &[u16]) -> JsText {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        let spaces = text.run(at + 1, crate::input::is_space);
        at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
    }
    out.into()
}

/// Pele's `buildHierarchy`: a row belongs to the nearest section above it that is indented less.
/// The tree is built from paths of child positions, so it holds no references while it grows.
fn build_hierarchy(items: Vec<(usize, TreemapNode)>) -> Vec<TreemapNode> {
    let mut roots: Vec<TreemapNode> = Vec::new();
    // The open sections, outermost first: where each sits among its parent's rows, and its level.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (level, node) in items {
        while stack.last().is_some_and(|&(_, open)| open >= level) {
            stack.pop();
        }
        let section = matches!(node.content, Content::Section(_));
        let mut rows = &mut roots;
        for &(place, _) in &stack {
            match &mut rows[place].content {
                Content::Section(children) => rows = children,
                Content::Leaf(_) => unreachable!("only sections are open"),
            }
        }
        rows.push(node);
        if section {
            stack.push((rows.len() - 1, level));
        }
    }
    roots
}

/// Pele's `buildTreemap`.
fn build(ast: TreemapAst, front_title: Option<&JsText>) -> TreemapModel {
    let mut classes: IndexMap<JsText, Vec<JsText>> = IndexMap::new();
    for row in &ast.rows {
        if let Row::ClassDef { name, style_text } = row {
            let styles = classes.entry(name.as_slice().into()).or_default();
            styles.extend(declarations(style_text.as_deref().unwrap_or_default()));
        }
    }
    let mut items = Vec::new();
    for row in ast.rows {
        let (indent, name, content, selector) = match row {
            Row::ClassDef { .. } => continue,
            Row::Section { indent, name, class_selector } => {
                (indent, name, Content::Section(Vec::new()), class_selector)
            }
            Row::Leaf { indent, name, value, class_selector } => (indent, name, Content::Leaf(value), class_selector),
        };
        let class_selector: Option<JsText> = selector.map(JsText::from);
        let css_compiled_styles = class_selector
            .as_ref()
            .and_then(|selector| classes.get(selector))
            .filter(|styles| !styles.is_empty())
            .cloned();
        items.push((
            indent.unwrap_or(0),
            TreemapNode { name: name.into(), content, class_selector, css_compiled_styles },
        ));
    }
    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    TreemapModel {
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        acc_title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
        acc_descr: filled(ast.acc_descr).map(|text| unindent(&text)),
        roots: build_hierarchy(items),
        classes,
    }
}

/// Pele's treemap `parse`: the model a source describes, or the first error in it.
pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<TreemapModel, RenderError> {
    let ast = parser::parse(cx.text).map_err(|error| cx.syntax(error))?;
    Ok(build(ast, cx.title))
}
