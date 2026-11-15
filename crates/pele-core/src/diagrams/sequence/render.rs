//! Drawing a sequence diagram, ported from Pele's `src/diagrams/sequence/render.ts`.

use super::{Entry, LineType, SeqMessage};
use crate::js;

/// The number each message line carries, whether or not it is shown.
pub(crate) struct Numbering {
    /// NaN for every entry that is no message line.
    pub(crate) index: Vec<f64>,
    pub(crate) shown: Vec<bool>,
    /// Whether numbers are on after the last statement.
    pub(crate) visible: bool,
}

/// The head a message line ends in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    None,
    Arrow,
    Cross,
    Open,
    HalfSolid,
    HalfStick,
}

/// How a message line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineStyle {
    dotted: bool,
    head: Head,
    /// Where the heads are: at the receiver, at the sender only (the reverse arrows), or at both.
    at_start: bool,
    at_end: bool,
    /// Whether a half head sits under the line.
    bottom: bool,
}

/// Pele's `STYLES`: the style of an entry that is a message line.
fn line_style(kind: LineType) -> Option<LineStyle> {
    use LineType as L;
    let style = |dotted, head| LineStyle { dotted, head, at_start: false, at_end: true, bottom: false };
    let both = |dotted| LineStyle { at_start: true, ..style(dotted, Head::Arrow) };
    let reverse = |style: LineStyle| LineStyle { at_start: true, at_end: false, ..style };
    let under = |style: LineStyle| LineStyle { bottom: true, ..style };
    Some(match kind {
        L::Solid => style(false, Head::Arrow),
        L::Dotted => style(true, Head::Arrow),
        L::SolidCross => style(false, Head::Cross),
        L::DottedCross => style(true, Head::Cross),
        L::SolidOpen => style(false, Head::None),
        L::DottedOpen => style(true, Head::None),
        L::SolidPoint => style(false, Head::Open),
        L::DottedPoint => style(true, Head::Open),
        L::BidirectionalSolid => both(false),
        L::BidirectionalDotted => both(true),
        L::SolidTop => style(false, Head::HalfSolid),
        L::SolidBottom => under(style(false, Head::HalfSolid)),
        L::StickTop => style(false, Head::HalfStick),
        L::StickBottom => under(style(false, Head::HalfStick)),
        L::SolidArrowTopReverse => reverse(style(false, Head::HalfSolid)),
        L::SolidArrowBottomReverse => under(reverse(style(false, Head::HalfSolid))),
        L::StickArrowTopReverse => reverse(style(false, Head::HalfStick)),
        L::StickArrowBottomReverse => under(reverse(style(false, Head::HalfStick))),
        L::SolidTopDotted => style(true, Head::HalfSolid),
        L::SolidBottomDotted => under(style(true, Head::HalfSolid)),
        L::StickTopDotted => style(true, Head::HalfStick),
        L::StickBottomDotted => under(style(true, Head::HalfStick)),
        L::SolidArrowTopReverseDotted => reverse(style(true, Head::HalfSolid)),
        L::SolidArrowBottomReverseDotted => under(reverse(style(true, Head::HalfSolid))),
        L::StickArrowTopReverseDotted => reverse(style(true, Head::HalfStick)),
        L::StickArrowBottomReverseDotted => under(reverse(style(true, Head::HalfStick))),
        _ => return None,
    })
}

pub(crate) fn is_line_message(kind: LineType) -> bool {
    line_style(kind).is_some()
}

/// Pele's `numberMessages`: counts message lines the way Mermaid's renderer does while it draws.
pub(crate) fn number_messages(messages: &[SeqMessage], always: bool) -> Numbering {
    let mut index = Vec::with_capacity(messages.len());
    let mut shown = Vec::with_capacity(messages.len());
    let (mut current, mut step, mut visible) = (1.0, 1.0, false);
    // `a || b` for numbers: zero and NaN give way.
    let or = |given: Option<f64>, kept: f64| given.filter(|&given| given != 0.0 && !given.is_nan()).unwrap_or(kept);
    for message in messages {
        index.push(f64::NAN);
        shown.push(false);
        if let Entry::Numbering(spec) = &message.entry {
            current = or(spec.start, current);
            step = or(spec.step, step);
            visible = spec.visible;
        } else if is_line_message(message.kind()) {
            *index.last_mut().expect("pushed above") = current;
            *shown.last_mut().expect("pushed above") = visible || always;
            current = js::round((current + step) * 100.0) / 100.0;
        }
    }
    Numbering { index, shown, visible }
}
