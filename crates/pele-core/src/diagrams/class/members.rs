//! A class's attributes and methods, ported from Pele's `src/diagrams/class/members.ts`.

use crate::common::parse_generic_types;
use crate::input::{JsText, Units, is_line_end};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemberType {
    Method,
    Attribute,
}

impl MemberType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            MemberType::Method => "method",
            MemberType::Attribute => "attribute",
        }
    }
}

/// The mark before a member's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Visibility {
    None,
    Protected,
    Public,
    Package,
    Private,
}

impl Visibility {
    fn of(c: Option<u16>) -> Visibility {
        match c.and_then(|c| u8::try_from(c).ok()) {
            Some(b'#') => Visibility::Protected,
            Some(b'+') => Visibility::Public,
            Some(b'~') => Visibility::Package,
            Some(b'-') => Visibility::Private,
            _ => Visibility::None,
        }
    }

    pub(crate) fn mark(self) -> &'static str {
        match self {
            Visibility::None => "",
            Visibility::Protected => "#",
            Visibility::Public => "+",
            Visibility::Package => "~",
            Visibility::Private => "-",
        }
    }
}

/// The mark after a member: `$` for a static one, `*` for an abstract one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Classifier {
    None,
    Static,
    Abstract,
}

impl Classifier {
    fn of(c: Option<u16>) -> Classifier {
        match c {
            Some(36) => Classifier::Static,
            Some(42) => Classifier::Abstract,
            _ => Classifier::None,
        }
    }

    pub(crate) fn mark(self) -> &'static str {
        match self {
            Classifier::None => "",
            Classifier::Static => "$",
            Classifier::Abstract => "*",
        }
    }
}

/// Pele's `ClassMember`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassMember {
    pub(crate) id: JsText,
    pub(crate) member_type: MemberType,
    pub(crate) visibility: Visibility,
    pub(crate) classifier: Classifier,
    pub(crate) parameters: JsText,
    pub(crate) return_type: JsText,
}

impl ClassMember {
    pub(crate) fn new(input: &[u16], member_type: MemberType) -> ClassMember {
        let mut member = ClassMember {
            id: JsText::default(),
            member_type,
            visibility: Visibility::None,
            classifier: Classifier::None,
            parameters: JsText::default(),
            return_type: JsText::default(),
        };
        // Mermaid throws a TypeError for a method with no opening parenthesis, such as `run)`.
        // It is kept here as an attribute, so it is shown as written.
        if member_type != MemberType::Method || !member.parse_method(input) {
            member.parse_attribute(input);
        }
        let trimmed = member.id.trim_js();
        member.id = if member.id.at(0) == Some(32) {
            let mut id = JsText::from(" ");
            id.push_units(trimmed);
            id
        } else {
            trimmed.into()
        };
        member
    }

    /// The same split as Mermaid's `/([#+~-])?(.+)\((.*)\)([\s$*])?(.*)([$*])?/`, found in one
    /// pass: the first line with an opening parenthesis after its first character and a
    /// closing one later.
    fn parse_method(&mut self, input: &[u16]) -> bool {
        let n = input.len();
        let mut start = 0;
        while start < n {
            let end = input.run(start, |c| !is_line_end(c));
            let mut close = end as isize - 1;
            while close > start as isize && input[close as usize] != 41 {
                close -= 1;
            }
            let mut open = close - 1;
            while open > start as isize && input[open as usize] != 40 {
                open -= 1;
            }
            if open > start as isize {
                let (open, close) = (open as usize, close as usize);
                let first = input[start];
                let visibility = Visibility::of(Some(first));
                let visible = visibility != Visibility::None && open > start + 1;
                if visible {
                    self.visibility = visibility;
                }
                self.id = input[if visible { start + 1 } else { start }..open].into();
                self.parameters = input[open + 1..close].trim_js().into();
                let mut rest = close + 1;
                let mut classifier = Classifier::None;
                if rest < n {
                    let next = input[rest];
                    let blank = std::slice::from_ref(&next).trim_js().is_empty();
                    if next == 36 || next == 42 || blank {
                        classifier = if blank { Classifier::None } else { Classifier::of(Some(next)) };
                        rest += 1;
                    }
                }
                let stop = input.run(rest, |c| !is_line_end(c));
                self.return_type = input[rest..stop].trim_js().into();
                if classifier == Classifier::None {
                    let last = Classifier::of(self.return_type.last().copied());
                    if last != Classifier::None {
                        classifier = last;
                        let length = self.return_type.len() - 1;
                        self.return_type.truncate(length);
                    }
                }
                self.classifier = classifier;
                return true;
            }
            start = end + 1;
        }
        self.member_type = MemberType::Attribute;
        false
    }

    fn parse_attribute(&mut self, input: &[u16]) {
        self.visibility = Visibility::of(input.first().copied());
        self.classifier = Classifier::of(input.last().copied());
        let from = usize::from(self.visibility != Visibility::None);
        let to = input.len() - usize::from(self.classifier != Classifier::None);
        // `substring` swaps its bounds when they cross, as for the lone `+` of `+`.
        self.id = input[from.min(to)..from.max(to)].into();
    }

    /// Pele's `getDisplayDetails().displayText`: what a member is drawn as.
    pub(crate) fn display_text(&self) -> JsText {
        let generics = |text: &[u16]| JsText::from(parse_generic_types(&String::from_utf16_lossy(text)).as_str());
        let mut text = JsText::from(self.visibility.mark());
        text.push_units(&generics(&self.id));
        if self.member_type == MemberType::Method {
            text.push(40);
            text.push_units(&generics(self.parameters.trim_js()));
            text.push(41);
            if !self.return_type.is_empty() {
                text.push_str(" : ");
                text.push_units(&generics(&self.return_type));
            }
        }
        text.trim_js().into()
    }
}
