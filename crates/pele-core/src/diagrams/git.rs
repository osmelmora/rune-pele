//! Git graphs (`gitGraph`), ported from Pele's `src/diagrams/git`. The parser reads Mermaid's
//! Langium grammar into statements, `db` plays them as Mermaid's database does into commits on
//! branches, and `render` draws the commits along their lanes as a Scene.

mod db;
pub(crate) mod json;
mod parser;
mod render;

use crate::RenderError;
use crate::catalog::ParseContext;
use crate::input::JsText;
use crate::scene::Accessible;
use indexmap::IndexMap;

pub(crate) use render::Git;

/// Which way the history runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    LeftRight,
    TopBottom,
    BottomTop,
}

impl Direction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Direction::LeftRight => "LR",
            Direction::TopBottom => "TB",
            Direction::BottomTop => "BT",
        }
    }
}

/// What a commit is. Pele's `commitType` numbers, which pick a symbol and a class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CommitType {
    Normal,
    Reverse,
    Highlight,
    Merge,
    CherryPick,
}

impl CommitType {
    /// Pele's number for the type.
    pub(crate) fn code(self) -> u8 {
        self as u8
    }

    /// The suffix of the commit's `pele-commit-*` class.
    pub(crate) fn class(self) -> &'static str {
        match self {
            CommitType::Normal => "normal",
            CommitType::Reverse => "reverse",
            CommitType::Highlight => "highlight",
            CommitType::Merge => "merge",
            CommitType::CherryPick => "cherry-pick",
        }
    }
}

/// Pele's `GitCommit`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Commit {
    pub(crate) id: JsText,
    pub(crate) message: JsText,
    pub(crate) seq: usize,
    pub(crate) kind: CommitType,
    pub(crate) tags: Vec<JsText>,
    pub(crate) parents: Vec<JsText>,
    pub(crate) branch: JsText,
    /// The type a merge is drawn as, where its statement names one.
    pub(crate) custom_type: Option<CommitType>,
    /// Whether a merge's id is the author's. Only merges say.
    pub(crate) custom_id: Option<bool>,
}

/// Pele's `GitModel`.
#[derive(Clone, Debug)]
pub(crate) struct GitModel {
    pub(crate) title: Option<JsText>,
    pub(crate) accessible: Accessible,
    pub(crate) direction: Direction,
    /// Keyed by commit id, in the order the ids first appeared. A later commit with an id
    /// already taken replaces the earlier one in its place.
    pub(crate) commits: IndexMap<JsText, Commit>,
    /// The id of each branch's latest commit, or `None` while the branch has none.
    pub(crate) branches: IndexMap<JsText, Option<JsText>>,
    /// Branch names in drawing order: by `order`, then by creation.
    pub(crate) lanes: Vec<JsText>,
    pub(crate) current_branch: JsText,
    /// The id of the commit the current branch is at.
    pub(crate) head: Option<JsText>,
    pub(crate) warnings: Vec<JsText>,
}

pub(crate) fn parse(cx: &ParseContext<'_>) -> Result<GitModel, RenderError> {
    let ast = parser::parse_git(cx.text).map_err(|error| cx.syntax(error))?;
    db::build_git(ast, cx.config, cx.title).map_err(|message| cx.semantic(message))
}
