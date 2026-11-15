//! Helpers for the hand-written lexers of the types Mermaid defines with Jison grammars, ported
//! from Pele's `src/diagrams/common/scan.ts` (journey and timeline) and `caseless.ts` (quadrant
//! and xychart). Positions are UTF-16 code units, as `charCodeAt` reads them.

use crate::input::{Units, is_space};

/// Pele's `lineEnd`: where the line holding `from` ends, at its line feed or the end of the text.
pub(crate) fn line_end(src: &[u16], from: usize) -> usize {
    src.find_unit(10, from).unwrap_or(src.len())
}

/// Pele's `keywordAt`: whether `word`, lowercase ASCII letters, is at `at` in any case. It folds by
/// setting bit 5 of each code unit, as Pele does, so a word that held anything but letters would
/// match more than its own case: a `-` matches a carriage return, and a space the end of the text.
pub(crate) fn keyword_at(src: &[u16], at: usize, word: &str) -> bool {
    // Past the end `charCodeAt` is NaN, and `NaN | 32` is 32.
    word.encode_utf16().enumerate().all(|(i, w)| src.at(at + i).map_or(32, |c| c | 32) == w)
}

/// Pele's `wordAt` (`caseless.ts`): whether `word`, written in lower case, is at `at`. Only ASCII
/// letters match in either case, as with the `i` flag on a regular expression without `u`.
pub(crate) fn word_at(src: &[u16], at: usize, word: &str) -> bool {
    word.encode_utf16().enumerate().all(|(k, w)| {
        let c = src.at(at + k);
        c == Some(w) || ((97..=122).contains(&w) && c == Some(w - 32))
    })
}

/// Pele's `skipTrivia`: skips comments and whitespace from `from`, and stops at a line break that
/// starts a token, which is a NEWLINE. Jison compiles `\%%` to `%`, so a single `%` starts a
/// comment unless a `{` follows it. A run of whitespace that starts with a space goes on through
/// any line breaks in it.
pub(crate) fn skip_trivia(src: &[u16], from: usize) -> usize {
    let mut p = from;
    while let Some(c) = src.at(p) {
        if c == 37 && src.at(p + 1) != Some(123) {
            p = line_end(src, p);
        } else if c == 10 {
            return p;
        } else if is_space(c) {
            p = src.run(p + 1, is_space);
        } else if c == 35 {
            p = line_end(src, p);
        } else {
            return p;
        }
    }
    p
}

/// The tokens of an accessibility statement. Pele numbers them consecutively from a type's own
/// first one, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccPart {
    /// `accTitle` up to its value, colon and spaces included.
    Title,
    TitleValue,
    /// `accDescr` up to its value, colon and spaces included.
    Descr,
    DescrValue,
    /// The text of a braced `accDescr { … }`.
    DescrMultilineValue,
}

impl AccPart {
    /// The place of the part in Pele's numbering, counted from the first.
    pub(crate) fn offset(self) -> usize {
        self as usize
    }
}

/// Pele's `accessibility`: lexes an `accTitle` or `accDescr` statement at `at`, handing each token
/// to `emit` with its start and end, and returns where the statement ends, or `at` when there is
/// none. A braced description that never closes gives the rest of the text, then an empty token
/// at its end.
pub(crate) fn accessibility(src: &[u16], at: usize, mut emit: impl FnMut(AccPart, usize, usize)) -> usize {
    let descr = keyword_at(src, at, "accdescr");
    if !descr && !keyword_at(src, at, "acctitle") {
        return at;
    }
    let n = src.len();
    let mut q = src.run(at + 8, is_space);
    let c = src.at(q);
    if c == Some(58) {
        q = src.run(q + 1, is_space);
        let end = line_end(src, q);
        emit(if descr { AccPart::Descr } else { AccPart::Title }, at, q);
        emit(if descr { AccPart::DescrValue } else { AccPart::TitleValue }, q, end);
        return end;
    }
    if !descr || c != Some(123) {
        return at;
    }
    q = src.run(q + 1, is_space);
    while src.at(q) != Some(125) {
        let end = src.find_unit(125, q).unwrap_or(n);
        emit(AccPart::DescrMultilineValue, q, end);
        if q == n {
            return n;
        }
        q = end;
    }
    q + 1
}
