//! The tree Pele builds from the parsed entries, ported from `treeview/model.ts`: each entry goes
//! under the nearest one before it that is indented less.

use super::box_drawing::{BoxError, Rewritten, line_start, preprocess_box_drawing};
use super::parser::{TreeViewAst, parse_tree_view};
use crate::catalog::ParseContext;
use crate::diagrams::railroad::close_indents;
use crate::diagrams::tokens::SyntaxError;
use crate::input::{JsText, Units};
use crate::{Diagnostic, ErrorCode, Location, RenderError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeType {
    File,
    Directory,
}

pub(crate) struct TreeNode {
    pub(crate) id: usize,
    /// The indentation as written, or -1 for the root.
    pub(crate) level: i64,
    pub(crate) name: JsText,
    pub(crate) node_type: NodeType,
    /// An icon name as written, or `none` to hide the icon.
    pub(crate) icon: Option<JsText>,
    pub(crate) class: Option<JsText>,
    pub(crate) description: Option<JsText>,
    /// Places in [`TreeViewModel::nodes`].
    pub(crate) children: Vec<usize>,
}

/// Pele's `TreeViewModel`. The nodes are in the order they were added, the root first: a
/// directory named `/` at level -1 that holds the top-level entries.
pub(crate) struct TreeViewModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) nodes: Vec<TreeNode>,
}

/// Pele's `TreeViewDb` with `populate`: nodes arrive in source order with their indentation.
fn populate(ast: TreeViewAst) -> Vec<TreeNode> {
    let root = TreeNode {
        id: 0,
        level: -1,
        name: "/".into(),
        node_type: NodeType::Directory,
        icon: None,
        class: None,
        description: None,
        children: Vec::new(),
    };
    let mut nodes = vec![root];
    let mut stack = vec![0];
    for node in ast.nodes {
        let level = node.indent.unwrap_or(0) as i64;
        let directory = node.name.last() == Some(&u16::from(b'/'));
        let name = if directory { &node.name[..node.name.len() - 1] } else { &node.name[..] };
        let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty()).map(JsText::from);
        while level <= nodes[*stack.last().expect("the root stays")].level {
            stack.pop();
        }
        let id = nodes.len();
        nodes.push(TreeNode {
            id,
            level,
            name: name.into(),
            node_type: if directory { NodeType::Directory } else { NodeType::File },
            icon: node.icon.map(|icon| if icon.is_empty() { "none".into() } else { icon.into() }),
            class: filled(node.class),
            description: filled(node.description),
            children: Vec::new(),
        });
        let parent = *stack.last().expect("the root stays");
        nodes[parent].children.push(id);
        stack.push(id);
    }
    nodes
}

/// Pele's `buildTreeView`. `title` is what front matter sets.
fn build_tree_view(ast: TreeViewAst, title: Option<&JsText>) -> TreeViewModel {
    let filled = |text: &Option<Vec<u16>>| text.as_ref().filter(|text| !text.is_empty()).cloned();
    TreeViewModel {
        title: filled(&ast.title).map(JsText::from).or_else(|| title.cloned()),
        acc_title: filled(&ast.acc_title).map(|text| text.trim_start_js().into()),
        acc_descr: filled(&ast.acc_descr).map(|text| close_indents(&text)),
        nodes: populate(ast),
    }
}

/// Pele's `locate` on text that is not the text being parsed, so that no position is mapped.
fn locate_in(src: &[u16], offset: usize) -> Location {
    let end = src.trim_end_js().len();
    let offset = if end > 0 && offset >= end { end } else { offset };
    let line_start = src[..offset.min(src.len())].iter().rposition(|&c| c == 10).map_or(0, |at| at + 1);
    let line_end = src.find_unit(10, offset).unwrap_or(src.len());
    let column = offset - line_start + 1;
    let text = String::from_utf16_lossy(&src[line_start..line_end.max(line_start)]);
    Location {
        line: 1 + src[..line_start].iter().filter(|&&c| c == 10).count(),
        column_utf16: column,
        snippet: format!("{text}\n{}^", "-".repeat(column - 1)),
    }
}

fn syntax_message(error: &SyntaxError, at: &Location) -> String {
    if error.lexical {
        format!("Lexical error on line {}. Unrecognized text.\n{}", at.line, at.snippet)
    } else {
        format!("Parse error on line {}:\n{}\n{}", at.line, at.snippet, error.detail)
    }
}

fn located(message: String, location: Location) -> RenderError {
    RenderError::Diagnostic(Diagnostic { location: Some(location), ..Diagnostic::new(ErrorCode::Syntax, message) })
}

/// Pele's `readTreeView` for a syntax error in rewritten box drawing: the message and position
/// name the line as written.
fn remapped(cx: &ParseContext<'_>, rewritten: &Rewritten, error: &SyntaxError) -> RenderError {
    let at = locate_in(&rewritten.text, error.at);
    let message = syntax_message(error, &at);
    let Some(&line) = rewritten.lines.get(&at.line) else {
        return located(message, at);
    };
    let column =
        rewritten.columns.get(&at.line).map_or(at.column_utf16 - 1, |columns| columns.written(at.column_utf16 - 1));
    let written = cx.origin.locate(cx.text, line_start(cx.text, line) + column);
    let head = message.find('\n').unwrap_or(message.len());
    let (first, rest) = message.split_at(head);
    let first = first.replacen(&format!("line {}", at.line), &format!("line {}", written.line), 1);
    let rest = rest.replacen(&at.snippet, &written.snippet, 1);
    located(format!("{first}{rest}"), written)
}

/// Pele's `readTreeView` and `buildTreeView`: the source, as box drawing or as indentation, to
/// a tree. Errors name lines of the text as written.
pub(crate) fn read(cx: &ParseContext<'_>) -> Result<TreeViewModel, RenderError> {
    let rewritten = preprocess_box_drawing(cx.text).map_err(|BoxError { index, message }| {
        let line = cx.origin.locate(cx.text, line_start(cx.text, index + 1)).line;
        let location = Location { line, column_utf16: 0, snippet: String::new() };
        located(format!("Line {line}: {message}"), location)
    })?;
    let ast = match &rewritten {
        None => parse_tree_view(cx.text).map_err(|error| cx.syntax(error))?,
        Some(rewritten) if rewritten.text == cx.text => parse_tree_view(cx.text).map_err(|error| cx.syntax(error))?,
        Some(rewritten) => parse_tree_view(&rewritten.text).map_err(|error| remapped(cx, rewritten, &error))?,
    };
    Ok(build_tree_view(ast, cx.title))
}
