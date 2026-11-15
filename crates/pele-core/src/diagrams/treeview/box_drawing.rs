//! Mermaid's box-drawing preprocessor, ported from Pele's `treeview/boxDrawing.ts`: turns `├──`,
//! `└──`, `│` and their heavy forms into the indentation the grammar reads, and remembers where
//! each rewritten line came from.

use crate::input::{Units, is_space};
use crate::js;
use std::collections::HashMap;

/// `[─━│┃└┗├┣]`
fn is_box(c: u16) -> bool {
    matches!(c, 0x2500 | 0x2501 | 0x2502 | 0x2503 | 0x2514 | 0x2517 | 0x251c | 0x2523)
}

/// `[└┗├┣]`
fn is_branch(c: u16) -> bool {
    matches!(c, 0x2514 | 0x2517 | 0x251c | 0x2523)
}

/// `[─━]`
fn is_dash(c: u16) -> bool {
    matches!(c, 0x2500 | 0x2501)
}

/// `^[\s│┃]+$`
fn decoration_only(line: &[u16]) -> bool {
    !line.is_empty() && line.iter().all(|&c| is_space(c) || c == 0x2502 || c == 0x2503)
}

/// `^[\s─━│┃└┗├┣]+$`
fn box_only(line: &[u16]) -> bool {
    !line.is_empty() && line.iter().all(|&c| is_space(c) || is_box(c))
}

/// `^\s*(title[\t ]|accTitle[\t ]*:|accDescr[\t ]*[:{])`
fn metadata_line(line: &[u16]) -> bool {
    let at = line.run(0, is_space);
    let blank = |c: u16| c == 9 || c == 32;
    if line.starts_at(at, "title") {
        return line.at(at + 5).is_some_and(blank);
    }
    if line.starts_at(at, "accTitle") {
        return line.at(line.run(at + 8, blank)) == Some(u16::from(b':'));
    }
    if line.starts_at(at, "accDescr") {
        return line.at(line.run(at + 8, blank)).is_some_and(|c| c == u16::from(b':') || c == u16::from(b'{'));
    }
    false
}

/// `^\s*%%`
fn comment_line(line: &[u16]) -> bool {
    line.starts_at(line.run(0, is_space), "%%")
}

fn passes(line: &[u16]) -> bool {
    line.trim_js().is_empty() || comment_line(line) || metadata_line(line)
}

fn expand_tabs(line: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(line.len());
    for &c in line {
        if c == 9 {
            out.extend_from_slice(&[32; 4]);
        } else {
            out.push(c);
        }
    }
    out
}

/// Where a column of a rewritten line was in the line as written: the written line, where its
/// content starts in the line with its tabs expanded, and how much indentation the rewrite gave.
#[derive(Clone)]
pub(super) struct Columns {
    line: Vec<u16>,
    content: usize,
    indent: usize,
}

impl Columns {
    /// Pele's column map: an index of the rewritten line as an index of the written one, with
    /// the four-space tab expansion undone.
    pub(super) fn written(&self, at: usize) -> usize {
        let target = self.content + at.saturating_sub(self.indent);
        let (mut i, mut n) = (0, 0);
        while i < self.line.len() && n < target {
            n += if self.line[i] == 9 { 4 } else { 1 };
            i += 1;
        }
        i
    }
}

pub(super) struct Rewritten {
    pub(super) text: Vec<u16>,
    /// Rewritten line number to written line number, both from 1.
    pub(super) lines: HashMap<usize, usize>,
    pub(super) columns: HashMap<usize, Columns>,
}

/// Why the box drawing cannot be read: the line, by its index, and what is wrong with it.
pub(super) struct BoxError {
    pub(super) index: usize,
    pub(super) message: &'static str,
}

/// Pele's `preprocessBoxDrawing`. `None` is text left as it is.
pub(super) fn preprocess_box_drawing(input: &[u16]) -> Result<Option<Rewritten>, BoxError> {
    if !input.iter().any(|&c| is_box(c)) {
        return Ok(None);
    }
    let lines: Vec<&[u16]> = input.split(|&c| c == 10).collect();
    let Some(keyword) = lines.iter().position(|line| line.trim_js().is_str("treeView-beta")) else {
        return Ok(None);
    };
    let content: Vec<Vec<u16>> = lines[keyword + 1..]
        .iter()
        .filter(|line| !passes(line) && !decoration_only(line))
        .map(|line| expand_tabs(line))
        .collect();
    if !content.iter().any(|line| line.iter().any(|&c| is_box(c))) {
        return Ok(None);
    }
    let segment =
        content.iter().find_map(|line| line.iter().position(|&c| is_branch(c)).filter(|&at| at > 0)).unwrap_or(4);

    let mut out: Vec<Vec<u16>> = Vec::new();
    let mut rewritten = Rewritten { text: Vec::new(), lines: HashMap::new(), columns: HashMap::new() };
    let mut keep = |text: Vec<u16>, index: usize, columns: Option<Columns>, rewritten: &mut Rewritten| {
        out.push(text);
        rewritten.lines.insert(out.len(), index + 1);
        if let Some(columns) = columns {
            rewritten.columns.insert(out.len(), columns);
        }
    };
    for (index, line) in lines.iter().enumerate().take(keyword + 1) {
        keep(line.to_vec(), index, None, &mut rewritten);
    }
    for (index, &line) in lines.iter().enumerate().skip(keyword + 1) {
        if passes(line) {
            keep(line.to_vec(), index, None, &mut rewritten);
            continue;
        }
        if decoration_only(line) {
            continue;
        }
        let normalized = expand_tabs(line);
        if let Some(branch) = normalized.iter().position(|&c| is_branch(c)) {
            let depth = js::round(branch as f64 / segment as f64) as usize + 1;
            let mut pos = branch + 1;
            while pos < normalized.len() && is_dash(normalized[pos]) {
                pos += 1;
            }
            while pos < normalized.len() && normalized[pos] == 32 {
                pos += 1;
            }
            let content = normalized[pos..].trim_end_js();
            if content.is_empty() {
                let message = "Empty node — expected a filename or directory name after the box-drawing prefix";
                return Err(BoxError { index, message });
            }
            let indent = 4 * depth;
            let mut text = vec![32; indent];
            text.extend_from_slice(content);
            let columns = Columns { line: line.to_vec(), content: pos, indent };
            keep(text, index, Some(columns), &mut rewritten);
        } else if box_only(&normalized) {
            continue;
        } else if normalized.iter().any(|&c| is_box(c)) {
            // A name that happens to contain a box character, such as "Section ─ A.txt", is a root item.
            keep(line.to_vec(), index, None, &mut rewritten);
        } else if normalized.first().is_some_and(|&c| is_space(c)) {
            let message = "Unexpected indentation without box-drawing characters. In box-drawing format, use ├── or └── prefixes for indented nodes.";
            return Err(BoxError { index, message });
        } else {
            keep(line.to_vec(), index, None, &mut rewritten);
        }
    }
    rewritten.text = out.join(&10);
    Ok(Some(rewritten))
}

/// Pele's `lineStart`: where line `line`, counted from 1, starts.
pub(super) fn line_start(text: &[u16], line: usize) -> usize {
    let mut at = 0;
    for _ in 1..line {
        at = text.find_unit(10, at).map_or(0, |feed| feed + 1);
    }
    at
}
