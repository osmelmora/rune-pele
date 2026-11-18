//! Pele's `buildGit` (`src/diagrams/git/db.ts`): the statements played into commits on branches
//! as Mermaid's git database plays them, with its error messages.

use super::parser::{GitAst, Statement};
use super::{Commit, CommitType, Direction, GitModel};
use crate::input::{JsObject, JsText, JsValue, Units, is_space};
use crate::layout::num::by_difference;
use crate::scene::Accessible;
use indexmap::IndexMap;

/// The ids Pele makes up: the sequence number, then seven hex digits derived from it where
/// Mermaid's are random, so the same text always gives the same ids.
fn next_id(seq: usize, commits: &IndexMap<JsText, Commit>) -> JsText {
    let mut salt = (seq + 1) as f64;
    loop {
        let hash = (salt as u64 as u32).wrapping_mul(0x9e37_79b1);
        let id = JsText::from(format!("{seq}-{}", &format!("{hash:08x}")[..7]).as_str());
        if !commits.contains_key(&id) {
            return id;
        }
        salt += f64::from(0x9e37_79b1_u32);
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

/// The database Mermaid's parser fills, under its names.
struct Db {
    commits: IndexMap<JsText, Commit>,
    branches: IndexMap<JsText, Option<JsText>>,
    orders: Vec<(JsText, f64)>,
    warnings: Vec<JsText>,
    head: Option<JsText>,
    current: JsText,
    direction: Direction,
    seq: usize,
}

impl Db {
    fn add(&mut self, commit: Commit) {
        self.head = Some(commit.id.clone());
        self.branches.insert(self.current.clone(), Some(commit.id.clone()));
        self.commits.insert(commit.id.clone(), commit);
    }

    fn head(&self) -> Option<&Commit> {
        self.head.as_ref().and_then(|id| self.commits.get(id))
    }

    fn checkout(&mut self, branch: &JsText) -> Result<(), String> {
        let Some(id) = self.branches.get(branch) else {
            return Err(format!(
                "Trying to checkout branch which is not yet created. (Help try using \"branch {}\")",
                branch.to_display()
            ));
        };
        self.head = id.clone().filter(|id| self.commits.contains_key(id));
        self.current = branch.clone();
        Ok(())
    }

    fn commit(&mut self, id: Option<Vec<u16>>, message: Vec<u16>, kind: CommitType, tags: Vec<Vec<u16>>) {
        let (id, own_id) = match id {
            Some(id) if !id.is_empty() => (JsText::from(id), true),
            _ => (next_id(self.seq, &self.commits), false),
        };
        let commit = Commit {
            id,
            message: message.into(),
            seq: self.seq,
            kind,
            tags: tags.into_iter().map(JsText::from).collect(),
            parents: self.head().map(|head| vec![head.id.clone()]).unwrap_or_default(),
            branch: self.current.clone(),
            custom_type: None,
            own_id,
        };
        self.seq += 1;
        if self.commits.contains_key(&commit.id) {
            let mut warning = JsText::from("Commit ID ");
            warning.push_units(&commit.id);
            warning.push_str(" already exists");
            self.warnings.push(warning);
        }
        self.add(commit);
    }

    fn branch(&mut self, name: JsText, order: f64) -> Result<(), String> {
        if self.branches.contains_key(&name) {
            return Err(format!(
                "Trying to create an existing branch. (Help: Either use a new name if you want create a new branch or try using \"checkout {}\")",
                name.to_display()
            ));
        }
        self.branches.insert(name.clone(), self.head().map(|head| head.id.clone()));
        self.orders.push((name.clone(), order));
        self.checkout(&name)
    }

    fn merge(
        &mut self,
        other: JsText,
        custom_id: JsText,
        kind: Option<CommitType>,
        tags: Vec<Vec<u16>>,
    ) -> Result<(), String> {
        let current = self.current.clone();
        let current_head = self.branches.get(&current).cloned().flatten();
        let other_head = self.branches.get(&other).cloned().flatten();
        let current_commit = current_head.as_ref().and_then(|id| self.commits.get(id));
        let other_commit = other_head.as_ref().and_then(|id| self.commits.get(id));
        let (shown_other, shown_current) = (other.to_display(), current.to_display());
        if let (Some(current_commit), Some(_)) = (current_commit, other_commit)
            && current_commit.branch == other
        {
            return Err(format!("Cannot merge branch '{shown_other}' into itself."));
        }
        if current == other {
            return Err("Incorrect usage of \"merge\". Cannot merge a branch to itself".to_owned());
        }
        let Some(current_commit) = current_commit else {
            return Err(format!("Incorrect usage of \"merge\". Current branch ({shown_current})has no commits"));
        };
        if !self.branches.contains_key(&other) {
            return Err(format!("Incorrect usage of \"merge\". Branch to be merged ({shown_other}) does not exist"));
        }
        let Some(other_commit) = other_commit else {
            return Err(format!("Incorrect usage of \"merge\". Branch to be merged ({shown_other}) has no commits"));
        };
        if current_commit.id == other_commit.id {
            return Err("Incorrect usage of \"merge\". Both branches have same head".to_owned());
        }
        if !custom_id.is_empty() && self.commits.contains_key(&custom_id) {
            return Err(format!(
                "Incorrect usage of \"merge\". Commit with id:{} already exists, use different custom id",
                custom_id.to_display()
            ));
        }
        let parents = match self.head() {
            Some(head) => vec![head.id.clone(), other_head.clone().unwrap_or_default()],
            None => Vec::new(),
        };
        let own_id = !custom_id.is_empty();
        let commit = Commit {
            id: if own_id { custom_id } else { next_id(self.seq, &self.commits) },
            message: merged_message(&other, &current),
            seq: self.seq,
            kind: CommitType::Merge,
            tags: tags.into_iter().map(JsText::from).collect(),
            parents,
            branch: current,
            custom_type: kind,
            own_id,
        };
        self.seq += 1;
        self.add(commit);
        Ok(())
    }

    fn cherry_pick(
        &mut self,
        source_id: Option<Vec<u16>>,
        tags: Option<Vec<Vec<u16>>>,
        parent: Option<Vec<u16>>,
    ) -> Result<(), String> {
        let source = source_id.filter(|id| !id.is_empty()).and_then(|id| self.commits.get(&JsText::from(id)));
        let Some(source) = source else {
            return Err("Incorrect usage of \"cherryPick\". Source commit id should exist and provided".to_owned());
        };
        let parent = parent.filter(|parent| !parent.is_empty()).map(JsText::from);
        if let Some(parent) = &parent
            && !source.parents.contains(parent)
        {
            return Err(
                "Invalid operation: The specified parent commit is not an immediate parent of the cherry-picked commit."
                    .to_owned(),
            );
        }
        if source.kind == CommitType::Merge && parent.is_none() {
            return Err("Incorrect usage of cherry-pick: If the source commit is a merge commit, an immediate parent commit must be specified.".to_owned());
        }
        if source.branch == self.current {
            return Err("Incorrect usage of \"cherryPick\". Source commit is already on current branch".to_owned());
        }
        let current_head = self.branches.get(&self.current).cloned().flatten();
        if !current_head.is_some_and(|id| self.commits.contains_key(&id)) {
            return Err(format!(
                "Incorrect usage of \"cherry-pick\". Current branch ({})has no commits",
                self.current.to_display()
            ));
        }
        let tags: Vec<JsText> = match tags {
            Some(tags) => tags.into_iter().filter(|tag| !tag.is_empty()).map(JsText::from).collect(),
            None => {
                let mut tag = JsText::from("cherry-pick:");
                tag.push_units(&source.id);
                if source.kind == CommitType::Merge {
                    tag.push_str("|parent:");
                    // A merge always has a parent here; `undefined` would be written otherwise.
                    tag.push_units(parent.as_deref().unwrap_or_default());
                }
                vec![tag]
            }
        };
        let mut message = JsText::from("cherry-picked ");
        message.push_units(&source.message);
        message.push_str(" into ");
        message.push_units(&self.current);
        let source_id = source.id.clone();
        let parents = match self.head() {
            Some(head) => vec![head.id.clone(), source_id],
            None => Vec::new(),
        };
        let commit = Commit {
            id: next_id(self.seq, &self.commits),
            message,
            seq: self.seq,
            kind: CommitType::CherryPick,
            tags,
            parents,
            branch: self.current.clone(),
            custom_type: None,
            own_id: false,
        };
        self.seq += 1;
        self.add(commit);
        Ok(())
    }
}

/// `merged branch ${other} into ${current}`, with the names as written.
fn merged_message(other: &JsText, current: &JsText) -> JsText {
    let mut message = JsText::from("merged branch ");
    message.push_units(other);
    message.push_str(" into ");
    message.push_units(current);
    message
}

/// The value at `gitGraph.<key>` of the config.
fn option<'a>(config: &'a JsObject, key: &str) -> Option<&'a JsValue> {
    config.get("gitGraph")?.as_record()?.get(key)
}

pub(super) fn build_git(ast: GitAst, config: &JsObject, front_title: Option<&JsText>) -> Result<GitModel, String> {
    let main = match option(config, "mainBranchName") {
        Some(JsValue::String(name)) => name.clone(),
        _ => JsText::from("main"),
    };
    let main_order = match option(config, "mainBranchOrder") {
        Some(JsValue::Number(order)) => *order,
        _ => 0.0,
    };
    let mut db = Db {
        commits: IndexMap::new(),
        branches: IndexMap::from([(main.clone(), None)]),
        orders: vec![(main.clone(), main_order)],
        warnings: Vec::new(),
        head: None,
        current: main,
        direction: Direction::LeftRight,
        seq: 0,
    };
    if let Some(direction) = ast.direction {
        db.direction = direction;
    }
    for statement in ast.statements {
        match statement {
            Statement::Commit { id, message, tags, kind } => {
                db.commit(id, message.unwrap_or_default(), kind.unwrap_or(CommitType::Normal), tags);
            }
            Statement::Branch { name, order } => db.branch(name.into(), order.unwrap_or(0.0))?,
            Statement::Merge { branch, id, tags, kind } => {
                db.merge(branch.into(), id.unwrap_or_default().into(), kind, tags)?;
            }
            Statement::Checkout { branch } => db.checkout(&branch.into())?,
            Statement::CherryPick { id, tags, parent } => {
                let tags = if tags.is_empty() { None } else { Some(tags) };
                db.cherry_pick(id, tags, parent)?;
            }
        }
    }

    // A branch made without an order sorts by creation among those of order zero, as in Mermaid.
    let mut lanes = db.orders;
    lanes.sort_by(|a, b| by_difference(a.1 - b.1));

    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    Ok(GitModel {
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        accessible: Accessible {
            title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
            description: filled(ast.acc_descr).map(|text| unindent(&text).into()),
        },
        direction: db.direction,
        commits: db.commits,
        branches: db.branches,
        lanes: lanes.into_iter().map(|(name, _)| name).collect(),
        current_branch: db.current,
        head: db.head,
        warnings: db.warnings,
    })
}
