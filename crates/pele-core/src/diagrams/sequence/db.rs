//! The sequence model builder, ported from Pele's `src/diagrams/sequence/db.ts`: it applies the
//! parser's statements in order and keeps what Mermaid's `SequenceDB` keeps.

use super::colors::is_css_color;
use super::parser::{Draw, Menu, Named, SeqBoxData, Statement};
use super::{
    Central, Entry, LineType, NoteActors, Placement, SeqActor, SeqBox, SeqMessage, SeqNote, SeqNumbering, SeqText,
    SequenceModel,
};
use crate::input::{JsText, JsValue, JsonError, Units, is_line_end, is_space, is_word, parse_json, parse_yaml};
use indexmap::IndexMap;
use std::collections::HashMap;

/// Why a statement cannot be applied.
pub(super) enum DbError {
    /// The statements contradict each other.
    Semantic(String),
    /// A participant's config is not YAML.
    Config(String),
    /// A menu's JSON nests deeper than this port reads.
    TooDeep,
}

/// `/^:?(?:no)?wrap:/`: the length of a wrap prefix and what it asks for.
fn wrap_prefix(text: &[u16]) -> Option<(usize, bool)> {
    let at = usize::from(text.at(0) == Some(u16::from(b':')));
    if text.starts_at(at, "wrap:") {
        Some((at + 5, true))
    } else if text.starts_at(at, "nowrap:") {
        Some((at + 7, false))
    } else {
        None
    }
}

/// Pele's `extractWrap`, which is also its `parseMessage`.
pub(super) fn extract_wrap(raw: &[u16]) -> SeqText {
    let text = raw.trim_js();
    match wrap_prefix(text) {
        Some((length, wrap)) => SeqText { text: text[length..].trim_js().into(), wrap: Some(wrap) },
        None => SeqText { text: text.into(), wrap: None },
    }
}

fn has_line_terminator(text: &[u16]) -> bool {
    text.iter().any(|&c| c != 10 && is_line_end(c))
}

/// Splits a box line into a leading colour and the rest, as Mermaid's
/// `/^((?:rgba?|hsla?)\s*\(.*\)|\w*)(.*)$/` does. `.` stops at a line terminator, so a line that
/// holds one outside the gap before the parenthesis does not match at all.
fn split_box(text: &[u16]) -> Option<(&[u16], &[u16])> {
    // /^(?:rgba?|hsla?)\s*\(/
    let function = ["rgba", "rgb", "hsla", "hsl"].iter().find_map(|name| {
        let open = text.starts_at(0, name).then(|| text.run(name.len(), is_space))?;
        (text.at(open) == Some(u16::from(b'('))).then_some(open + 1)
    });
    if let Some(after) = function
        && let Some(close) = text.iter().rposition(|&c| c == u16::from(b')')).filter(|&close| close >= after)
    {
        if has_line_terminator(&text[after..]) {
            return None;
        }
        return Some((&text[..=close], &text[close + 1..]));
    }
    if has_line_terminator(text) {
        return None;
    }
    let word = text.run(0, is_word);
    Some((&text[..word], &text[word..]))
}

/// Pele's `parseBoxData`.
pub(super) fn parse_box_data(text: &[u16]) -> SeqBoxData {
    let found = split_box(text);
    let mut color: JsText = match found {
        Some((color, _)) if !color.is_empty() => color.trim_js().into(),
        _ => "transparent".into(),
    };
    let mut title: Option<&[u16]> = match found {
        Some((_, rest)) if !rest.is_empty() => Some(rest.trim_js()),
        _ => None,
    };
    if !is_css_color(&color) {
        color = "transparent".into();
        title = Some(text.trim_js());
    }
    let Some(title) = title else {
        return SeqBoxData { text: None, color, wrap: None };
    };
    let SeqText { text, wrap } = extract_wrap(title);
    SeqBoxData { text: Some(text).filter(|text| !text.is_empty()), color, wrap }
}

/// `slice(start, end)`, where a negative index counts back from the end.
fn slice(text: &[u16], start: isize, end: isize) -> &[u16] {
    let length = text.len() as isize;
    let clamp = |index: isize| if index < 0 { (length + index).max(0) } else { index.min(length) } as usize;
    let (start, end) = (clamp(start), clamp(end));
    &text[start.min(end)..end]
}

/// Pele's `entries`: what a menu's parsed JSON sets, key by key.
fn entries(value: JsValue) -> Vec<(JsText, JsValue)> {
    let index = |i: usize| JsText::from(i.to_string().as_str());
    match value {
        JsValue::String(text) => char::decode_utf16(text.iter().copied())
            .enumerate()
            .map(|(i, c)| {
                let mut one = JsText::default();
                match c {
                    Ok(c) => one.push_code_point(u32::from(c)),
                    Err(lone) => one.push(lone.unpaired_surrogate()),
                }
                (index(i), JsValue::String(one))
            })
            .collect(),
        JsValue::Array(items) => items.into_iter().enumerate().map(|(i, item)| (index(i), item)).collect(),
        JsValue::Object(object) => object.iter().map(|(key, value)| (key.clone(), value.clone())).collect(),
        JsValue::Null | JsValue::Bool(_) | JsValue::Number(_) => Vec::new(),
    }
}

/// What a participant is declared with: the label and kind a statement or its config gives.
struct Description {
    text: JsText,
    wrap: Option<bool>,
}

pub(super) struct SeqDb {
    pub(super) model: SequenceModel,
    prev_actor: Option<JsText>,
    current_box: Option<usize>,
    last_created: Option<JsText>,
    last_destroyed: Option<JsText>,
    /// The top-level `wrap` setting, which the `%%{wrap}%%` directive also sets.
    wrap_enabled: Option<bool>,
    /// `sequence.wrap`.
    sequence_wrap: bool,
    /// How many activations each participant has open.
    active: HashMap<JsText, usize>,
}

impl SeqDb {
    pub(super) fn new(wrap: Option<bool>, sequence_wrap: bool) -> Self {
        SeqDb {
            model: SequenceModel::default(),
            prev_actor: None,
            current_box: None,
            last_created: None,
            last_destroyed: None,
            wrap_enabled: wrap,
            sequence_wrap,
            active: HashMap::new(),
        }
    }

    fn auto_wrap(&self) -> bool {
        self.wrap_enabled.unwrap_or(self.sequence_wrap)
    }

    pub(super) fn set_acc_title(&mut self, text: &[u16]) {
        self.model.acc_title = Some(text.trim_start_js().into());
    }

    /// `text.replace(/\n\s+/g, '\n')`
    pub(super) fn set_acc_description(&mut self, text: &[u16]) {
        let mut out = JsText::default();
        let mut at = 0;
        while at < text.len() {
            out.push(text[at]);
            at += 1;
            if text[at - 1] == 10 {
                let end = text.run(at, is_space);
                if end == at {
                    continue;
                }
                at = end;
            }
        }
        self.model.acc_descr = Some(out);
    }

    fn fail<V>(message: String) -> Result<V, DbError> {
        Err(DbError::Semantic(message))
    }

    fn add_actor(&mut self, named: Named) -> Result<(), DbError> {
        let Named { actor: id, draw, description, config } = named;
        let mut kind: Option<JsText> = draw.map(|draw| match draw {
            Draw::Participant => "participant".into(),
            Draw::Actor => "actor".into(),
        });
        let mut description = description.map(|SeqText { text, wrap }| Description { text, wrap });

        let mut doc = None;
        if let Some(metadata) = &config {
            let mut source = JsText::default();
            if metadata.contains(&10) {
                source.push_units(metadata);
                source.push(10);
            } else {
                source.push_str("{\n");
                source.push_units(metadata);
                source.push_str("\n}");
            }
            match parse_yaml(&source) {
                Ok(JsValue::Object(object)) => doc = Some(object),
                Ok(_) => {}
                Err(message) => {
                    let message = format!("Config of participant \"{}\" is not valid YAML. {message}", id.to_display());
                    return Err(DbError::Config(message));
                }
            }
        }
        let own = |name: &str| doc.as_ref().and_then(|doc| doc.get(name));
        if let Some(given) = own("type").filter(|given| !matches!(given, JsValue::Null)) {
            kind = Some(given.to_js_string());
        }
        if let Some(alias) = own("alias").filter(|alias| alias.truthy())
            && description.as_ref().is_none_or(|description| description.text == id)
        {
            let wrap = description.and_then(|description| description.wrap);
            description = Some(Description { text: alias.to_js_string(), wrap });
        }

        let mut assigned = self.current_box;
        if let Some(old) = self.model.actors.get_mut(&id) {
            if let (Some(current), Some(theirs)) = (self.current_box, old.in_box)
                && current != theirs
            {
                let name = |index: usize| match &self.model.boxes[index].name {
                    Some(name) => name.to_display(),
                    None => "undefined".to_owned(),
                };
                return Self::fail(format!(
                    "A same participant should only be defined in one Box: {} can't be in '{}' and in '{}' at the same time.",
                    old.name.to_display(),
                    name(theirs),
                    name(current)
                ));
            }
            assigned = old.in_box.or(self.current_box);
            old.in_box = assigned;
            if description.is_none() {
                return Ok(());
            }
        }
        // A participant that is only mentioned in a message has no kind and takes its name as its label.
        let description = match description {
            Some(description) if kind.is_some() => description,
            _ => Description { text: id.clone(), wrap: None },
        };

        let actor = SeqActor {
            in_box: assigned,
            name: id.clone(),
            description: description.text,
            wrap: description.wrap.unwrap_or(self.auto_wrap()),
            prev_actor: self.prev_actor.clone(),
            next_actor: None,
            links: IndexMap::new(),
            properties: IndexMap::new(),
            details: None,
            kind: kind.unwrap_or_else(|| "participant".into()),
        };
        self.model.actors.insert(id.clone(), actor);
        if let Some(prev) = self.prev_actor.as_ref().filter(|prev| !prev.is_empty())
            && let Some(prev) = self.model.actors.get_mut(prev)
        {
            prev.next_actor = Some(id.clone());
        }
        if let Some(current) = self.current_box {
            self.model.boxes[current].actor_keys.push(id.clone());
        }
        self.prev_actor = Some(id);
        Ok(())
    }

    fn add_signal(
        &mut self,
        from: Option<JsText>,
        to: Option<JsText>,
        message: Option<SeqText>,
        kind: LineType,
        activate: bool,
        central: Option<Central>,
    ) -> Result<(), DbError> {
        // Mermaid recounts every earlier message here. A running count per participant says the same.
        let named = from.as_ref().filter(|from| !from.is_empty());
        if kind == LineType::ActiveEnd {
            let count = named.and_then(|from| self.active.get(from)).copied().unwrap_or(0);
            if count < 1 {
                let from = from.as_ref().map_or("undefined".to_owned(), |from| from.to_display());
                return Self::fail(format!("Trying to inactivate an inactive participant ({from})"));
            }
            self.active.insert(from.clone().expect("an active participant"), count - 1);
        } else if kind == LineType::ActiveStart
            && let Some(from) = named
        {
            *self.active.entry(from.clone()).or_insert(0) += 1;
        }
        let wrap = message.as_ref().and_then(|message| message.wrap).unwrap_or(self.auto_wrap());
        self.model.messages.push(SeqMessage {
            from,
            to,
            text: message.map(|message| message.text).unwrap_or_default(),
            wrap,
            entry: Entry::Signal { kind, activate, central },
        });
        Ok(())
    }

    fn add_note(&mut self, actor: NoteActors, placement: Placement, text: SeqText) {
        let wrap = text.wrap.unwrap_or(self.auto_wrap());
        let (from, to) = match &actor {
            NoteActors::One(actor) => (actor.clone(), actor.clone()),
            NoteActors::Span(from, to) => (from.clone(), to.clone()),
        };
        self.model.notes.push(SeqNote { actor, placement, message: text.text.clone(), wrap });
        self.model.messages.push(SeqMessage {
            from: Some(from),
            to: Some(to),
            text: text.text,
            wrap,
            entry: Entry::Note { placement },
        });
    }

    /// Mermaid logs and ignores menu text that is not valid JSON.
    fn menu_entries(text: &[u16]) -> Result<Vec<(JsText, JsValue)>, DbError> {
        match parse_json(text) {
            Ok(value) => Ok(entries(value)),
            Err(JsonError::Invalid) => Ok(Vec::new()),
            Err(JsonError::TooDeep) => Err(DbError::TooDeep),
        }
    }

    fn add_menu(&mut self, menu: Menu, id: &JsText, text: SeqText) -> Result<(), DbError> {
        let Some(actor) = self.model.actors.get_mut(id) else {
            return Ok(());
        };
        let text = text.text;
        match menu {
            Menu::Links => actor.links.extend(Self::menu_entries(&text)?),
            Menu::Properties => actor.properties.extend(Self::menu_entries(&text)?),
            Menu::Link => {
                let sep = text.find_unit(u16::from(b'@'), 0).map_or(-1, |at| at as isize);
                let label = slice(&text, 0, sep - 1).trim_js();
                let target = slice(&text, sep + 1, text.len() as isize).trim_js();
                actor.links.insert(label.into(), JsValue::String(target.into()));
            }
            // Mermaid reads the properties and links from a page element with this id. Pele keeps the id.
            Menu::Details => actor.details = Some(text),
        }
        Ok(())
    }

    /// Pele's `applyOne`.
    pub(super) fn apply(&mut self, statement: Statement) -> Result<(), DbError> {
        match statement {
            Statement::Numbering { start, step, visible } => self.model.messages.push(SeqMessage {
                from: None,
                to: None,
                text: JsText::default(),
                wrap: false,
                entry: Entry::Numbering(SeqNumbering { start, step, visible }),
            }),
            Statement::AddParticipant(named) => self.add_actor(named)?,
            Statement::CreateParticipant(named) => {
                if self.model.actors.contains_key(&named.actor) {
                    return Self::fail(
                        "It is not possible to have actors with the same id, even if one is destroyed before the next is created. Use 'AS' aliases to simulate the behavior"
                            .to_owned(),
                    );
                }
                let id = named.actor.clone();
                self.last_created = Some(id.clone());
                self.add_actor(named)?;
                self.model.created_actors.insert(id, self.model.messages.len());
            }
            Statement::DestroyParticipant(id) => {
                self.last_destroyed = Some(id.clone());
                self.model.destroyed_actors.insert(id, self.model.messages.len());
            }
            Statement::Mark { actor, kind } => self.add_signal(Some(actor), None, None, kind, false, None)?,
            Statement::AddNote { actor, placement, text } => self.add_note(actor, placement, text),
            Statement::Menu { menu, actor, text } => self.add_menu(menu, &actor, text)?,
            Statement::AddMessage { from, to, kind, msg, activate, central } => {
                let pending = |last: &Option<JsText>| last.clone().filter(|last| !last.is_empty());
                if let Some(created) = pending(&self.last_created) {
                    if to != created {
                        return Self::fail(format!(
                            "The created participant {} does not have an associated creating message after its declaration. Please check the sequence diagram.",
                            created.to_display()
                        ));
                    }
                    self.last_created = None;
                } else if let Some(destroyed) = pending(&self.last_destroyed) {
                    if to != destroyed && from != destroyed {
                        return Self::fail(format!(
                            "The destroyed participant {} does not have an associated destroying message after its declaration. Please check the sequence diagram.",
                            destroyed.to_display()
                        ));
                    }
                    self.last_destroyed = None;
                }
                self.add_signal(Some(from), Some(to), Some(msg), kind, activate, central)?;
            }
            Statement::BoxStart(data) => {
                let wrap = data.wrap.unwrap_or(self.auto_wrap());
                self.current_box = Some(self.model.boxes.len());
                self.model.boxes.push(SeqBox { name: data.text, wrap, fill: data.color, actor_keys: Vec::new() });
            }
            Statement::BoxEnd => self.current_box = None,
            Statement::Block { text, kind } => self.add_signal(None, None, Some(text), kind, false, None)?,
            Statement::BlockEnd(kind) => self.add_signal(None, None, None, kind, false, None)?,
        }
        Ok(())
    }
}
