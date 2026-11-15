//! The use case parser, ported from Pele's `src/diagrams/usecase/parser.ts`.
//!
//! Mermaid parses use case diagrams with a Chevrotain parser, which decides each choice by
//! looking at most three tokens ahead and reports the first token that fits nowhere. This parser
//! makes the same decisions from the same lookahead, so it accepts the same text and names the
//! same token in its errors. It collects what it reads as drafts, which `db` resolves.

use super::db::{
    ClassAssignment, DraftBoundary, DraftElement, DraftEndpoint, DraftJson, DraftLabel, DraftProperty,
    DraftRelationship, Drafts, ElementKind, MetaValue, Named,
};
use super::json::parse_ordered_json;
use super::lexer::{LexFault, T, TOKEN_COUNT, Tokens, tokenize};
use super::{Arrow, Direction, LabelType, Positions, RelationshipType, Span, UseCaseShape};
use crate::input::{JsText, Units, is_word};
use crate::{ErrorCode, RenderError};
use std::sync::LazyLock;

const LINE: u8 = 1;
const RELATION: u8 = 2;
const STYLE_PART: u8 = 4;
const FOLLOWS_NAME: u8 = 8;
const BOUNDARY_LINE: u8 = 16;
const CLOSES_LABEL: u8 = 32;
const METADATA_FIRST: u8 = 64;

const ARROWS: &str = "DEPENDENCY_ARROW|GENERALIZATION|FORWARD_SOLID|BACKWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|BACKWARD_CIRCLE|FORWARD_CROSS|BACKWARD_CROSS";
const NAMES: &str = "IDENTIFIER|PLAIN_STRING|MARKDOWN_STRING";
const LABELS: &str = "PLAIN_STRING|MARKDOWN_STRING|LABEL_TEXT";
const STATEMENTS: &str = "ACC_TITLE_LINE|ACC_DESCR_LINE|ACC_DESCR_BLOCK|DIRECTION|ACTOR|SYSTEM_BOUNDARY|NOTE|JSON_DECLARATION_START|CLASS_DEF|CLASS|STYLE";
const STYLE_PARTS: &str = "WORD|PLAIN_STRING|NUMBER|HASH_COLOR|CSS_IDENTIFIER|CSS_ESCAPED_COMMA|DASH|DOT|PERCENT|CSS_PUNCTUATION|COLON|LPAREN|RPAREN|LBRACKET|RBRACKET|LBRACE|RBRACE|AT|MARKERLESS_SOLID";
const FOLLOWS: &str = "STEREOTYPE_START|CLASS_SEPARATOR|IDENTIFIER";
const FORBIDDEN: [&str; 5] = ["allowmixing", "newpage", "package", "rectangle", "skinparam"];

/// The sets each token type is in, as bits.
static FLAGS: LazyLock<[u8; TOKEN_COUNT]> = LazyLock::new(|| {
    let mut flags = [0u8; TOKEN_COUNT];
    let mut flag = |names: &str, bits: u8| {
        for name in names.split('|') {
            if name == "WORD" {
                for kind in super::lexer::ALL.iter().filter(|kind| kind.is_word()) {
                    flags[*kind as usize] |= bits;
                }
            } else {
                flags[T::named(name).expect("a token name") as usize] |= bits;
            }
        }
    };
    flag(&format!("NEWLINE|COMMENT|{STATEMENTS}|{NAMES}"), LINE);
    flag(&format!("IDENTIFIER|{ARROWS}"), RELATION);
    flag(STYLE_PARTS, STYLE_PART);
    flag(&format!("{FOLLOWS}|{ARROWS}|NEWLINE|EOF"), FOLLOWS_NAME);
    flag(&format!("NEWLINE|COMMENT|ACTOR|{NAMES}"), BOUNDARY_LINE);
    flag("FORWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|FORWARD_CROSS", CLOSES_LABEL);
    flag("NEWLINE|IDENTIFIER|PLAIN_STRING|RBRACE", METADATA_FIRST);
    flags
});

/// A name in a statement, with what the statement says about it.
struct Item {
    named: Named,
    shape: Option<UseCaseShape>,
    metadata: Option<Vec<DraftProperty>>,
    stereotype: Option<JsText>,
    classes: Vec<JsText>,
    /// Whether this mention says something about the element, and so declares it.
    explicit: bool,
}

/// What follows the first name of a relationship.
struct Tail {
    explicit_id: Option<(JsText, Span)>,
    kind: RelationshipType,
    arrow: Arrow,
    label: Option<DraftLabel>,
    minlen: usize,
    target: Item,
    span: Span,
}

struct Parser<'a> {
    src: &'a [u16],
    tokens: Tokens,
    positions: &'a Positions<'a>,
    p: usize,
    drafts: Drafts,
    /// The boundary whose lines are being read, by its place among the boundary drafts.
    parent: Option<usize>,
    /// Each `json` draft with the token of its literal.
    pending_json: Vec<(usize, usize)>,
}

type Parsed<T> = Result<T, RenderError>;

impl Parser<'_> {
    fn kind(&self, k: usize) -> T {
        self.tokens.types.get(k).copied().unwrap_or(T::Eof)
    }

    fn here(&self) -> T {
        self.kind(self.p)
    }

    fn flags(&self, k: usize) -> u8 {
        FLAGS[self.kind(k) as usize]
    }

    fn span(&self, k: usize) -> Span {
        [self.tokens.starts[k], self.tokens.ends[k]]
    }

    fn image(&self, k: usize) -> &[u16] {
        &self.src[self.tokens.starts[k]..self.tokens.ends[k]]
    }

    fn shown(&self, k: usize) -> String {
        String::from_utf16_lossy(self.image(k))
    }

    fn fail(&self, detail: &str) -> RenderError {
        let message =
            format!("Error parsing usecase diagram: {detail} at {}", self.positions.location_text(self.span(self.p)));
        self.positions.error(ErrorCode::Syntax, message, self.tokens.starts[self.p])
    }

    fn mismatch(&self, name: &str) -> RenderError {
        self.fail(&format!("Expecting token of type --> {name} <-- but found --> '{}' <--", self.shown(self.p)))
    }

    fn expect(&mut self, kind: T) -> Parsed<usize> {
        if self.here() != kind {
            return Err(self.mismatch(kind.name()));
        }
        self.p += 1;
        Ok(self.p - 1)
    }

    /// Chevrotain lists every token sequence that would have been accepted.
    fn no_alternative(&self, expected: &str) -> RenderError {
        let list: Vec<String> =
            expected.split('|').enumerate().map(|(i, sequence)| format!("  {}. [{sequence}]", i + 1)).collect();
        self.fail(&format!(
            "Expecting: one of these possible Token sequences:\n{}\nbut found: '{}'",
            list.join("\n"),
            self.shown(self.p)
        ))
    }

    fn line_end(&mut self) -> Parsed<()> {
        match self.here() {
            T::Newline => {
                self.p += 1;
                Ok(())
            }
            T::Eof => Ok(()),
            _ => Err(self.no_alternative("NEWLINE|EOF")),
        }
    }

    /// The text of a name or string token, without its quotes.
    fn content(&self, k: usize) -> Span {
        let trim = match self.kind(k) {
            T::MarkdownString => 2,
            T::PlainString => 1,
            _ => 0,
        };
        [self.tokens.starts[k] + trim, self.tokens.ends[k] - trim]
    }

    fn text(&self, span: Span) -> JsText {
        self.src[span[0]..span[1]].into()
    }

    fn token_label(&self, k: usize) -> DraftLabel {
        let markdown = self.kind(k) == T::MarkdownString;
        DraftLabel {
            text: self.text(self.content(k)),
            kind: if markdown { LabelType::Markdown } else { LabelType::Text },
        }
    }

    /// An unquoted label runs to the next delimiter and is taken from the source as written.
    fn label(&mut self) -> Parsed<DraftLabel> {
        let kind = self.here();
        if kind == T::PlainString || kind == T::MarkdownString {
            self.p += 1;
            return Ok(self.token_label(self.p - 1));
        }
        if !kind.is_label_text() {
            return Err(self.no_alternative(LABELS));
        }
        let first = self.p;
        while self.here().is_label_text() {
            self.p += 1;
        }
        let text = self.text([self.tokens.starts[first], self.tokens.ends[self.p - 1]]);
        Ok(DraftLabel { text, kind: LabelType::Text })
    }

    fn property(&mut self) -> Parsed<DraftProperty> {
        let key = self.p;
        if !is_key(self.here()) {
            return Err(self.no_alternative("IDENTIFIER|PLAIN_STRING"));
        }
        self.p += 1;
        self.expect(T::Colon)?;
        let value = self.p;
        let kind = self.here();
        if !is_key(kind) && kind != T::True && kind != T::False {
            return Err(self.no_alternative("IDENTIFIER|PLAIN_STRING|TRUE|FALSE"));
        }
        self.p += 1;
        let key_span = self.content(key);
        Ok(DraftProperty {
            key: self.text(key_span),
            value: match kind {
                T::True => MetaValue::Bool(true),
                T::False => MetaValue::Bool(false),
                _ => MetaValue::Text(self.text(self.content(value))),
            },
            key_span,
        })
    }

    /// Whether another property follows. A comma or line breaks before the closing brace do not
    /// count, but Chevrotain looks only three tokens ahead, so a comma and two line breaks, or
    /// three line breaks, are taken for a separator and the brace after them is then an error.
    fn separator_ahead(&self) -> bool {
        let (a, b, c) = (self.here(), self.kind(self.p + 1), self.kind(self.p + 2));
        if a == T::Comma {
            return is_key(b) || (b == T::Newline && (c == T::Newline || is_key(c)));
        }
        if a != T::Newline {
            return false;
        }
        b == T::Comma || is_key(b) || (b == T::Newline && (c == T::Newline || c == T::Comma || is_key(c)))
    }

    fn skip_newlines(&mut self) {
        while self.here() == T::Newline {
            self.p += 1;
        }
    }

    fn metadata(&mut self) -> Parsed<Vec<DraftProperty>> {
        self.expect(T::MetadataStart)?;
        let mut properties = Vec::new();
        self.skip_newlines();
        if is_key(self.here()) {
            properties.push(self.property()?);
            while self.separator_ahead() {
                if self.here() == T::Comma {
                    self.p += 1;
                } else {
                    self.skip_newlines();
                    if self.here() == T::Comma {
                        self.p += 1;
                    }
                }
                self.skip_newlines();
                properties.push(self.property()?);
            }
            if self.here() == T::Comma {
                self.p += 1;
            }
            self.skip_newlines();
        }
        self.expect(T::RBrace)?;
        Ok(properties)
    }

    fn class_suffix(&mut self, classes: &mut Vec<JsText>) -> Parsed<()> {
        loop {
            self.p += 1;
            let id = self.expect(T::Identifier)?;
            classes.push(self.image(id).into());
            if self.here() != T::Comma {
                return Ok(());
            }
        }
    }

    /// An id with an optional label in parentheses or brackets, or a string that stands for both.
    fn name(&mut self, brackets: bool) -> Parsed<Item> {
        let k = self.p;
        let kind = self.here();
        let span = self.content(k);
        let (id, label, shape);
        if kind == T::Identifier {
            self.p += 1;
            id = JsText::from(self.image(k));
            let open = self.here();
            if open == T::LParen || (brackets && open == T::LBracket) {
                self.p += 1;
                label = self.label()?;
                self.expect(if open == T::LParen { T::RParen } else { T::RBracket })?;
                shape = Some(if open == T::LParen { UseCaseShape::Ellipse } else { UseCaseShape::Rect });
            } else {
                label = self.token_label(k);
                shape = None;
            }
        } else if kind == T::PlainString || kind == T::MarkdownString {
            self.p += 1;
            label = self.token_label(k);
            id = generated_id(&label.text);
            shape = None;
        } else {
            return Err(self.no_alternative(NAMES));
        }
        Ok(Item {
            named: Named { id, label, span, generated: kind != T::Identifier },
            shape,
            metadata: None,
            stereotype: None,
            classes: Vec::new(),
            explicit: false,
        })
    }

    fn element(&mut self, actor: bool) -> Parsed<Item> {
        let mut item = self.name(!actor)?;
        if actor {
            item.shape = None;
        }
        if self.here() == T::MetadataStart {
            item.metadata = Some(self.metadata()?);
        }
        if self.here() == T::StereotypeStart {
            self.p += 1;
            let text = self.expect(T::StereotypeText)?;
            self.expect(T::StereotypeEnd)?;
            item.stereotype = Some(self.image(text).trim_js().into());
        }
        if self.here() == T::ClassSeparator {
            self.class_suffix(&mut item.classes)?;
        }
        item.explicit = item.shape.is_some() || item.metadata.is_some() || item.stereotype.is_some();
        Ok(item)
    }

    fn draft(&self, item: &Item, kind: ElementKind) -> DraftElement {
        DraftElement {
            named: item.named.clone(),
            kind,
            parent: self.parent,
            shape: item.shape,
            metadata: item.metadata.clone(),
            stereotype: item.stereotype.clone(),
            classes: item.classes.clone(),
        }
    }

    /// Whether a label and then one of the operators that can close it come next.
    fn labeled_right(&self, closers: u8) -> bool {
        let labels = |kind: T| (T::Usecase..=T::MarkdownString).contains(&kind);
        if !labels(self.here()) {
            return false;
        }
        let mut k = self.p + 1;
        loop {
            let kind = self.kind(k);
            if kind == T::MarkerlessSolid || self.flags(k) & closers != 0 {
                return true;
            }
            if !labels(kind) {
                return false;
            }
            k += 1;
        }
    }

    /// Each dash beyond the second asks for one more rank between the two ends.
    fn minimum_length(&self, k: usize) -> usize {
        let [start, end] = self.span(k);
        let dashes = end - start - usize::from(self.kind(k) != T::MarkerlessSolid);
        dashes.saturating_sub(1).max(1)
    }

    fn relation_tail(&mut self) -> Parsed<Tail> {
        let first = self.p;
        let mut explicit_id = None;
        if self.here() == T::Identifier && self.kind(self.p + 1) == T::At {
            explicit_id = Some((JsText::from(self.image(self.p)), self.span(self.p)));
            self.p += 2;
        }
        let (mut kind, mut arrow, mut label, mut minlen) = (RelationshipType::Association, Arrow::Forward, None, 1);
        let k = self.p;
        let length = self.tokens.ends[k] - self.tokens.starts[k];
        self.p += 1;
        match self.kind(k) {
            T::DependencyArrow => {
                self.expect(T::Colon)?;
                kind = match self.here() {
                    T::Include => RelationshipType::Include,
                    T::Extend => RelationshipType::Extend,
                    _ => return Err(self.no_alternative("INCLUDE|EXTEND")),
                };
                label = Some(DraftLabel { text: kind.name().into(), kind: LabelType::Text });
                self.p += 1;
            }
            T::Generalization => kind = RelationshipType::Generalization,
            T::ForwardSolid => minlen = self.minimum_length(k),
            T::BackwardSolid => {
                arrow = Arrow::Backward;
                // Only the shortest operator can open a label: `A <-- label -- B`.
                if length == 3 && self.labeled_right(0) {
                    label = Some(self.label()?);
                    let close = self.expect(T::MarkerlessSolid)?;
                    minlen = self.minimum_length(close);
                } else {
                    minlen = self.minimum_length(k);
                }
            }
            T::MarkerlessSolid => {
                arrow = Arrow::Plain;
                if length == 2 && self.labeled_right(CLOSES_LABEL) {
                    label = Some(self.label()?);
                    arrow = match self.here() {
                        T::ForwardSolid => Arrow::Forward,
                        T::ForwardCircle => Arrow::Circle,
                        T::ForwardCross => Arrow::Cross,
                        T::MarkerlessSolid => Arrow::Plain,
                        _ => {
                            return Err(
                                self.no_alternative("FORWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|FORWARD_CROSS")
                            );
                        }
                    };
                    if arrow == Arrow::Forward || arrow == Arrow::Plain {
                        minlen = self.minimum_length(self.p);
                    }
                    self.p += 1;
                } else {
                    minlen = self.minimum_length(k);
                }
            }
            T::ForwardCircle => arrow = Arrow::Circle,
            T::ForwardCross => arrow = Arrow::Cross,
            found @ (T::BackwardCircle | T::BackwardCross) => {
                arrow = if found == T::BackwardCircle { Arrow::CircleReversed } else { Arrow::CrossReversed };
                if self.labeled_right(0) {
                    label = Some(self.label()?);
                    self.expect(T::MarkerlessSolid)?;
                }
            }
            _ => {
                self.p -= 1;
                return Err(self.no_alternative(ARROWS));
            }
        }
        let target = self.element(false)?;
        let span = [self.tokens.starts[first], self.tokens.ends[self.p - 1]];
        Ok(Tail { explicit_id, kind, arrow, label, minlen, target, span })
    }

    fn relationship(&mut self, source: &Item, declaration: bool, tail: Tail) {
        let target = &tail.target;
        if target.explicit {
            let draft = self.draft(target, ElementKind::Usecase);
            self.drafts.elements.push(draft);
        }
        let endpoint = |item: &Item, declares: bool| DraftEndpoint {
            named: item.named.clone(),
            declares,
            classed: !item.classes.is_empty(),
        };
        self.drafts.relationships.push(DraftRelationship {
            source: endpoint(source, declaration),
            target: endpoint(target, target.explicit),
            span: tail.span,
            id: tail.explicit_id,
            kind: tail.kind,
            arrow: tail.arrow,
            label: tail.label,
            minlen: tail.minlen,
        });
    }

    /// `actor A, B` declares several actors; at the top level `actor A --> B` declares one and
    /// relates it.
    fn actors(&mut self, top: bool) -> Parsed<()> {
        self.p += 1;
        let mut items = vec![self.element(true)?];
        let mut tail = None;
        if self.here() == T::Comma {
            while self.here() == T::Comma {
                self.p += 1;
                items.push(self.element(true)?);
            }
        } else if top && self.flags(self.p) & RELATION != 0 {
            tail = Some(self.relation_tail()?);
        }
        self.line_end()?;
        for item in &items {
            let draft = self.draft(item, ElementKind::Actor);
            self.drafts.elements.push(draft);
        }
        if let Some(tail) = tail {
            self.relationship(&items[0], true, tail);
        }
        Ok(())
    }

    fn entity(&mut self, top: bool) -> Parsed<()> {
        let mut source = self.element(false)?;
        let tail = if top && self.flags(self.p) & RELATION != 0 { Some(self.relation_tail()?) } else { None };
        self.line_end()?;
        if tail.is_none() {
            source.explicit = true;
            source.shape.get_or_insert(UseCaseShape::Ellipse);
        }
        if source.explicit {
            let draft = self.draft(&source, ElementKind::Usecase);
            self.drafts.elements.push(draft);
        }
        if let Some(tail) = tail {
            self.relationship(&source, source.explicit, tail);
        }
        Ok(())
    }

    fn boundary(&mut self) -> Parsed<()> {
        self.p += 1;
        let mut named = self.name(true)?;
        let inline = if self.here() == T::MetadataStart { Some(self.metadata()?) } else { None };
        if self.here() == T::ClassSeparator {
            self.class_suffix(&mut named.classes)?;
        }
        self.line_end()?;
        let (id, span) = (named.named.id.clone(), named.named.span);
        self.drafts.boundaries.push(DraftBoundary { named: named.named, classes: named.classes });
        self.parent = Some(self.drafts.boundaries.len() - 1);
        while self.flags(self.p) & BOUNDARY_LINE != 0 {
            self.line(false)?;
        }
        self.parent = None;
        self.expect(T::End)?;
        self.line_end()?;
        if let Some(properties) = inline {
            self.drafts.metadata.push((id, span, properties));
        }
        Ok(())
    }

    fn style(&mut self) -> Parsed<JsText> {
        let first = self.p;
        let kind = self.here();
        if kind.is_word() || kind == T::CssIdentifier {
            self.p += 1;
        } else if kind == T::MarkerlessSolid {
            self.p += 1;
            if !self.here().is_word() {
                return Err(self.mismatch("WORD"));
            }
            self.p += 1;
        } else {
            return Err(self.no_alternative("WORD|CSS_IDENTIFIER|MARKERLESS_SOLID"));
        }
        self.expect(T::Colon)?;
        if self.flags(self.p) & STYLE_PART == 0 {
            return Err(self.fail(&format!(
                "Expecting: expecting at least one iteration which starts with one of these possible Token sequences::\n  <[{}]>\nbut found: '{}'",
                STYLE_PARTS.replace('|', "] ,["),
                self.shown(self.p)
            )));
        }
        while self.flags(self.p) & STYLE_PART != 0 {
            self.p += 1;
        }
        // The value is taken from the source, so the spaces inside `1px solid red` survive.
        let written = &self.src[self.tokens.starts[first]..self.tokens.ends[self.p - 1]];
        let mut out = JsText::default();
        let mut at = 0;
        while at < written.len() {
            if written.starts_at(at, "\\,") {
                at += 1;
            }
            out.push(written[at]);
            at += 1;
        }
        Ok(out)
    }

    fn styles(&mut self) -> Parsed<Vec<JsText>> {
        let mut out = vec![self.style()?];
        while self.here() == T::Comma {
            self.p += 1;
            out.push(self.style()?);
        }
        Ok(out)
    }

    fn identifiers(&mut self) -> Parsed<Vec<usize>> {
        let mut out = vec![self.expect(T::Identifier)?];
        while self.here() == T::Comma {
            self.p += 1;
            out.push(self.expect(T::Identifier)?);
        }
        Ok(out)
    }

    fn statement_sequences() -> String {
        let names: Vec<&str> = NAMES.split('|').collect();
        let metadata: Vec<String> = names
            .iter()
            .flat_map(|name| {
                ["NEWLINE", "IDENTIFIER", "PLAIN_STRING", "RBRACE"]
                    .map(|next| format!("{name}, METADATA_START, {next}"))
            })
            .collect();
        let follows = format!("{FOLLOWS}|{ARROWS}|NEWLINE|EOF");
        let follows: Vec<&str> = follows.split('|').collect();
        let entities: Vec<String> = names
            .iter()
            .enumerate()
            .flat_map(|(i, name)| {
                let first: &[&str] = if i == 0 { &["LPAREN", "LBRACKET"] } else { &[] };
                first.iter().chain(&follows).map(|next| format!("{name}, {next}")).collect::<Vec<_>>()
            })
            .collect();
        let mut all = vec![STATEMENTS.to_owned()];
        all.extend(metadata.iter().cloned());
        all.extend(entities);
        all.extend(metadata);
        all.join("|")
    }

    fn top_statement(&mut self) -> Parsed<()> {
        let k = self.p;
        let kind = self.here();
        match kind {
            T::AccTitleLine | T::AccDescrLine | T::AccDescrBlock => {
                self.p += 1;
                let text = self.image(k);
                let after = |unit: u8| text.find_unit(u16::from(unit), 0).map_or(0, |at| at + 1);
                if kind == T::AccDescrBlock {
                    let close = text.iter().rposition(|&c| c == u16::from(b'}')).unwrap_or(text.len());
                    self.drafts.acc_description = text[after(b'{')..close].trim_js().into();
                } else if kind == T::AccDescrLine {
                    self.drafts.acc_description = text[after(b':')..].trim_js().into();
                } else {
                    self.drafts.acc_title = text[after(b':')..].trim_js().into();
                }
            }
            T::Direction => {
                self.p += 1;
                self.drafts.direction = Some(match self.here() {
                    T::Td | T::Tb => Direction::TB,
                    T::Bt => Direction::BT,
                    T::Lr => Direction::LR,
                    T::Rl => Direction::RL,
                    _ => return Err(self.no_alternative("TD|TB|BT|LR|RL")),
                });
                self.p += 1;
            }
            T::Actor => return self.actors(true),
            T::SystemBoundary => return self.boundary(),
            T::Note => {
                self.p += 1;
                self.expect(T::For)?;
                let target = self.expect(T::Identifier)?;
                let label = self.label()?;
                self.drafts.notes.push((self.image(target).into(), self.span(target), label));
            }
            T::JsonDeclarationStart => {
                self.p += 1;
                let literal = self.expect(T::JsonObjectLiteral)?;
                let mut classes = Vec::new();
                if self.here() == T::ClassSeparator {
                    self.class_suffix(&mut classes)?;
                }
                let from = self.src.run(self.tokens.starts[k] + 4, |c| c == 32 || c == 9);
                let to = self.src.run(from, is_word);
                self.drafts.jsons.push(DraftJson { id: self.text([from, to]), value: None, span: [from, to], classes });
                self.pending_json.push((self.drafts.jsons.len() - 1, literal));
            }
            T::ClassDef => {
                self.p += 1;
                let ids = self.identifiers()?;
                let ids = ids.into_iter().map(|id| self.image(id).into()).collect();
                let styles = self.styles()?;
                self.drafts.class_defs.push((ids, styles));
            }
            T::Class => {
                self.p += 1;
                let targets = self.identifiers()?;
                let classes = self.identifiers()?;
                let assignment = ClassAssignment {
                    targets: targets.into_iter().map(|id| (self.image(id).into(), self.span(id))).collect(),
                    classes: classes.into_iter().map(|id| self.image(id).into()).collect(),
                };
                self.drafts.classes.push(assignment);
            }
            T::Style => {
                self.p += 1;
                let target = self.expect(T::Identifier)?;
                let styles = self.styles()?;
                self.drafts.styles.push((self.image(target).into(), self.span(target), styles));
            }
            _ => {
                let next = self.kind(self.p + 1);
                if next != T::MetadataStart {
                    let follows = self.flags(self.p + 1) & FOLLOWS_NAME != 0
                        || (kind == T::Identifier && (next == T::LParen || next == T::LBracket));
                    // Mermaid turns away the PlantUML statements people are most likely to try.
                    let lower: String = self.shown(k).to_lowercase();
                    if !follows || (kind == T::Identifier && FORBIDDEN.contains(&lower.as_str())) {
                        return Err(self.no_alternative(&Self::statement_sequences()));
                    }
                    return self.entity(true);
                }
                if self.flags(self.p + 2) & METADATA_FIRST == 0 {
                    return Err(self.no_alternative(&Self::statement_sequences()));
                }
                self.p += 1;
                let span = self.content(k);
                let written = self.text(span);
                let target = if kind == T::Identifier { self.image(k).into() } else { generated_id(&written) };
                let assigned = self.metadata()?;
                self.drafts.metadata.push((target, span, assigned));
            }
        }
        self.line_end()
    }

    fn line(&mut self, top: bool) -> Parsed<()> {
        match self.here() {
            T::Newline => {
                self.p += 1;
                Ok(())
            }
            T::Comment => {
                self.p += 1;
                self.line_end()
            }
            _ if top => self.top_statement(),
            T::Actor => self.actors(false),
            _ => self.entity(false),
        }
    }
}

fn is_key(kind: T) -> bool {
    kind == T::Identifier || kind == T::PlainString
}

/// `label.replace(/\W/g, '_')`
fn generated_id(label: &[u16]) -> JsText {
    label.iter().map(|&c| if is_word(c) { c } else { u16::from(b'_') }).collect::<Vec<u16>>().into()
}

/// Mermaid reports the first character no token starts with, and how many characters its lexer
/// skipped before one did.
fn lex_error(src: &[u16], fault: &LexFault, positions: &Positions<'_>) -> RenderError {
    let (line, column) = positions.line_column(fault.offset);
    let (from, to) = (positions.written(fault.offset), positions.written(fault.offset + fault.length));
    let found = String::from_utf16_lossy(&src[fault.offset..(fault.offset + 1).min(src.len())]);
    let message = format!(
        "Error lexing usecase diagram: unexpected character: ->{found}<- at offset: {from}, skipped {} characters. at line {line}, column {column} [{from},{to})",
        to.wrapping_sub(from) as isize
    );
    positions.error_at(ErrorCode::Syntax, message, line, column)
}

/// Pele's `parseUsecase`, up to `resolveUsecase`: everything the source says, in source order.
pub(super) fn parse(src: &[u16], positions: &Positions<'_>) -> Result<Drafts, RenderError> {
    let tokens = tokenize(src).map_err(|fault| lex_error(src, &fault, positions))?;
    let mut parser =
        Parser { src, tokens, positions, p: 0, drafts: Drafts::default(), parent: None, pending_json: Vec::new() };
    parser.expect(T::Usecase)?;
    parser.line_end()?;
    while parser.flags(parser.p) & LINE != 0 {
        parser.line(true)?;
    }
    if parser.here() != T::Eof {
        return Err(parser.fail(&format!("Redundant input, expecting EOF but found: {}", parser.shown(parser.p))));
    }
    // The whole text has to be well formed before the JSON in it is looked at, as in Mermaid.
    for (draft, literal) in std::mem::take(&mut parser.pending_json) {
        let start = parser.tokens.starts[literal];
        let value = parse_ordered_json(parser.image(literal), || positions.line_column(start)).map_err(|fault| {
            let message = String::from_utf16_lossy(&fault.worded());
            positions.error_at(ErrorCode::Syntax, message, fault.line as usize, fault.column as usize)
        })?;
        parser.drafts.jsons[draft].value = Some(value);
    }
    Ok(parser.drafts)
}
