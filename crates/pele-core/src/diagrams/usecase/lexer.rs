//! The tokens of a use case diagram, ported from Pele's `src/diagrams/usecase/lexer.ts` and
//! `tokens.ts`. They are the tokens of Mermaid's Chevrotain lexer: the first pattern in its list
//! that matches wins, a keyword gives way to a longer identifier, and `json id@` and `<<` switch
//! to modes that read a JSON object and a stereotype.

use crate::input::{Units, hex_digit, is_blank, is_digit};

/// Mermaid's token types, in an order that makes its token categories ranges: a word is
/// `Usecase` to `Identifier`, and label text is `Usecase` to `LabelSymbol`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub(super) enum T {
    Eof,
    Usecase,
    Actor,
    SystemBoundary,
    End,
    Direction,
    Td,
    Tb,
    Bt,
    Lr,
    Rl,
    Note,
    For,
    Json,
    ClassDef,
    Class,
    Style,
    Include,
    Extend,
    True,
    False,
    Identifier,
    Number,
    At,
    Comma,
    Colon,
    HashColor,
    CssIdentifier,
    CssEscapedComma,
    Dash,
    Dot,
    Percent,
    CssPunctuation,
    LabelPunctuation,
    LabelSymbol,
    PlainString,
    MarkdownString,
    UnclosedMarkdownString,
    Comment,
    Newline,
    AccDescrBlock,
    AccTitleLine,
    AccDescrLine,
    JsonDeclarationStart,
    JsonObjectLiteral,
    UnclosedJsonObjectLiteral,
    Generalization,
    DependencyArrow,
    StereotypeStart,
    StereotypeEnd,
    StereotypeText,
    UnclosedStereotypeText,
    ClassSeparator,
    ForwardSolid,
    BackwardSolid,
    ForwardCircle,
    BackwardCircle,
    ForwardCross,
    BackwardCross,
    MarkerlessSolid,
    MetadataStart,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
}

pub(super) const TOKEN_COUNT: usize = T::RParen as usize + 1;

const TOKEN_NAMES: &str = "EOF USECASE ACTOR SYSTEM_BOUNDARY END DIRECTION TD TB BT LR RL NOTE FOR JSON CLASS_DEF CLASS STYLE INCLUDE EXTEND TRUE FALSE IDENTIFIER NUMBER AT COMMA COLON HASH_COLOR CSS_IDENTIFIER CSS_ESCAPED_COMMA DASH DOT PERCENT CSS_PUNCTUATION LABEL_PUNCTUATION LABEL_SYMBOL PLAIN_STRING MARKDOWN_STRING UNCLOSED_MARKDOWN_STRING COMMENT NEWLINE ACC_DESCR_BLOCK ACC_TITLE_LINE ACC_DESCR_LINE JSON_DECLARATION_START JSON_OBJECT_LITERAL UNCLOSED_JSON_OBJECT_LITERAL GENERALIZATION DEPENDENCY_ARROW STEREOTYPE_START STEREOTYPE_END STEREOTYPE_TEXT UNCLOSED_STEREOTYPE_TEXT CLASS_SEPARATOR FORWARD_SOLID BACKWARD_SOLID FORWARD_CIRCLE BACKWARD_CIRCLE FORWARD_CROSS BACKWARD_CROSS MARKERLESS_SOLID METADATA_START LBRACE RBRACE LBRACKET RBRACKET LPAREN RPAREN";

impl T {
    /// Mermaid's name for the token type, as its messages spell it.
    pub(super) fn name(self) -> &'static str {
        TOKEN_NAMES.split(' ').nth(self as usize).expect("every token type has a name")
    }

    pub(super) fn named(name: &str) -> Option<T> {
        let index = TOKEN_NAMES.split(' ').position(|known| known == name)?;
        ALL.get(index).copied()
    }

    /// Mermaid's `WORD` category: every keyword and `IDENTIFIER`.
    pub(super) fn is_word(self) -> bool {
        (T::Usecase..=T::Identifier).contains(&self)
    }

    /// Mermaid's `LABEL_TEXT` category.
    pub(super) fn is_label_text(self) -> bool {
        (T::Usecase..=T::LabelSymbol).contains(&self)
    }
}

pub(super) const ALL: [T; TOKEN_COUNT] = {
    use T::*;
    [
        Eof,
        Usecase,
        Actor,
        SystemBoundary,
        End,
        Direction,
        Td,
        Tb,
        Bt,
        Lr,
        Rl,
        Note,
        For,
        Json,
        ClassDef,
        Class,
        Style,
        Include,
        Extend,
        True,
        False,
        Identifier,
        Number,
        At,
        Comma,
        Colon,
        HashColor,
        CssIdentifier,
        CssEscapedComma,
        Dash,
        Dot,
        Percent,
        CssPunctuation,
        LabelPunctuation,
        LabelSymbol,
        PlainString,
        MarkdownString,
        UnclosedMarkdownString,
        Comment,
        Newline,
        AccDescrBlock,
        AccTitleLine,
        AccDescrLine,
        JsonDeclarationStart,
        JsonObjectLiteral,
        UnclosedJsonObjectLiteral,
        Generalization,
        DependencyArrow,
        StereotypeStart,
        StereotypeEnd,
        StereotypeText,
        UnclosedStereotypeText,
        ClassSeparator,
        ForwardSolid,
        BackwardSolid,
        ForwardCircle,
        BackwardCircle,
        ForwardCross,
        BackwardCross,
        MarkerlessSolid,
        MetadataStart,
        LBrace,
        RBrace,
        LBracket,
        RBracket,
        LParen,
        RParen,
    ]
};

/// The tokens of a text, followed by three `Eof` tokens so that a parser can look ahead.
pub(super) struct Tokens {
    pub(super) types: Vec<T>,
    pub(super) starts: Vec<usize>,
    pub(super) ends: Vec<usize>,
}

impl Tokens {
    fn push(&mut self, kind: T, start: usize, end: usize) {
        self.types.push(kind);
        self.starts.push(start);
        self.ends.push(end);
    }
}

/// The first character no token starts with, and how many characters Mermaid's lexer skipped
/// before one did.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LexFault {
    pub(super) offset: usize,
    pub(super) length: usize,
}

/// A character of `\w`.
fn is_word(c: Option<u16>) -> bool {
    c.is_some_and(crate::input::is_word)
}

fn is_letter(c: Option<u16>) -> bool {
    c.is_some_and(|c| u8::try_from(c).is_ok_and(|c| c.is_ascii_alphabetic()))
}

fn digit(c: Option<u16>) -> bool {
    c.is_some_and(is_digit)
}

fn blank(c: Option<u16>) -> bool {
    c.is_some_and(is_blank)
}

fn is(c: Option<u16>, byte: u8) -> bool {
    c == Some(u16::from(byte))
}

/// The characters Mermaid's `CSS_PUNCTUATION` holds besides `#`.
fn is_css_punctuation(c: u16) -> bool {
    u8::try_from(c).is_ok_and(|c| b"!#$&*+/=?^|~".contains(&c))
}

/// Mermaid's `INCLUDE` and `EXTEND` are the only keywords matched without regard to case.
fn starts_with_no_case(src: &[u16], at: usize, word: &str) -> bool {
    word.bytes().enumerate().all(|(k, byte)| src.at(at + k).is_some_and(|c| c | 32 == u16::from(byte)))
}

/// The keyword that is a prefix of the text at `at`, tried in the order of Mermaid's token list.
fn keyword_at(src: &[u16], at: usize, c: u16) -> Option<(T, usize)> {
    let starts = |word: &'static str, kind: T| src.starts_at(at, word).then_some((kind, word.len()));
    let no_case = |word: &'static str, kind: T| starts_with_no_case(src, at, word).then_some((kind, word.len()));
    match u8::try_from(c).ok()? {
        b'a' => starts("actor", T::Actor),
        b's' => starts("systemBoundary", T::SystemBoundary).or_else(|| starts("style", T::Style)),
        b'e' => starts("end", T::End).or_else(|| no_case("extend", T::Extend)),
        b'E' => no_case("extend", T::Extend),
        b'd' => starts("direction", T::Direction),
        b'T' => starts("TD", T::Td).or_else(|| starts("TB", T::Tb)),
        b'B' => starts("BT", T::Bt),
        b'L' => starts("LR", T::Lr),
        b'R' => starts("RL", T::Rl),
        b'n' => starts("note", T::Note),
        b'f' => starts("for", T::For).or_else(|| starts("false", T::False)),
        b'j' => starts("json", T::Json),
        b'c' => starts("classDef", T::ClassDef).or_else(|| starts("class", T::Class)),
        b'i' | b'I' => no_case("include", T::Include),
        b't' => starts("true", T::True),
        b'u' => starts("usecase-beta", T::Usecase),
        _ => None,
    }
}

fn is_line_break(c: u16) -> bool {
    c == 10 || c == 13
}

/// The end of a quoted string that opens at `at`, or nothing when the line ends first.
fn string_end(src: &[u16], at: usize, quote: u16) -> Option<usize> {
    for (k, &c) in src.iter().enumerate().skip(at + 1) {
        if c == quote {
            return Some(k + 1);
        }
        if is_line_break(c) {
            return None;
        }
    }
    None
}

fn starts_token(src: &[u16], at: usize) -> bool {
    let c = src[at];
    if c != 34 && c != 39 {
        return true;
    }
    (c == 34 && is(src.at(at + 1), b'`')) || string_end(src, at, c).is_some()
}

/// Pele's `tokenize`.
pub(super) fn tokenize(src: &[u16]) -> Result<Tokens, LexFault> {
    let mut tokens = Tokens { types: Vec::new(), starts: Vec::new(), ends: Vec::new() };
    let n = src.len();
    let at = |k: usize| src.at(k);
    let mut i = 0;
    // Whether only blanks came before on this line, where comments and accessibility lines may start.
    let mut line_start = true;
    let mut no_brace = false;

    while i < n {
        let c = src[i];
        if c == 32 || c == 9 {
            i += 1;
            continue;
        }
        let kind: T;
        let mut end = i + 1;
        let mut fresh = false;

        if is_word(Some(c)) {
            let mut found = None;
            if c == 97 && line_start && src.starts_at(i, "acc") {
                let descr = src.starts_at(i + 3, "Descr");
                if descr || src.starts_at(i + 3, "Title") {
                    let mut j = i + 8;
                    while blank(at(j)) {
                        j += 1;
                    }
                    if is(at(j), b':') {
                        found = Some(if descr { T::AccDescrLine } else { T::AccTitleLine });
                        end = j + 1;
                        while end < n && !is_line_break(src[end]) {
                            end += 1;
                        }
                    } else if is(at(j), b'{') && descr && !no_brace {
                        match src.find_unit(u16::from(b'}'), j + 1) {
                            Some(close) => {
                                found = Some(T::AccDescrBlock);
                                end = close + 1;
                            }
                            None => no_brace = true,
                        }
                    }
                }
            } else if c == 106 && src.starts_at(i, "json") && blank(at(i + 4)) {
                let mut j = i + 5;
                while blank(at(j)) {
                    j += 1;
                }
                let id = j;
                while is_word(at(j)) {
                    j += 1;
                }
                if j > id {
                    while blank(at(j)) {
                        j += 1;
                    }
                    if is(at(j), b'@') {
                        j += 1;
                        while blank(at(j)) {
                            j += 1;
                        }
                        if is(at(j), b'{') {
                            tokens.push(T::JsonDeclarationStart, i, j);
                            let mut depth = 0usize;
                            let mut quoted = false;
                            let mut escaped = false;
                            let mut k = j;
                            while k < n {
                                let ch = src[k];
                                if quoted {
                                    if escaped {
                                        escaped = false;
                                    } else if ch == 92 {
                                        escaped = true;
                                    } else if ch == 34 {
                                        quoted = false;
                                    }
                                } else if ch == 34 {
                                    quoted = true;
                                } else if ch == 123 {
                                    depth += 1;
                                } else if ch == 125 {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                k += 1;
                            }
                            let closed = k < n;
                            i = if closed { k + 1 } else { n };
                            tokens.push(if closed { T::JsonObjectLiteral } else { T::UnclosedJsonObjectLiteral }, j, i);
                            line_start = false;
                            continue;
                        }
                    }
                }
            }
            kind = match found {
                Some(found) => found,
                None => {
                    while end < n && is_word(at(end)) {
                        end += 1;
                    }
                    if let Some((keyword, length)) = keyword_at(src, i, c) {
                        // A keyword that is only the start of a longer word is an identifier.
                        if keyword == T::Usecase || end - i == length {
                            end = i + length;
                            keyword
                        } else {
                            T::Identifier
                        }
                    } else if (c == 111 || c == 120) && is(at(i + 1), b'-') && is(at(i + 2), b'-') {
                        end = i + 3;
                        if c == 111 { T::BackwardCircle } else { T::BackwardCross }
                    } else if is_digit(c) {
                        // `1mg` is an identifier, and `1.5px` is a number because the identifier stops at the dot.
                        let mut digits = i + 1;
                        while digit(at(digits)) {
                            digits += 1;
                        }
                        if is(at(digits), b'.') && digit(at(digits + 1)) {
                            end = digits + 2;
                            while digit(at(end)) {
                                end += 1;
                            }
                            while is_letter(at(end)) {
                                end += 1;
                            }
                            T::Number
                        } else {
                            T::Identifier
                        }
                    } else {
                        let mut kind = T::Identifier;
                        while is(at(end), b'-') && is_word(at(end + 1)) {
                            kind = T::CssIdentifier;
                            end += 2;
                            while is_word(at(end)) {
                                end += 1;
                            }
                        }
                        kind
                    }
                }
            };
        } else if c == 10 {
            kind = T::Newline;
            fresh = true;
        } else if c == 13 {
            kind = T::Newline;
            fresh = true;
            if is(at(end), b'\n') {
                end += 1;
            }
        } else if c == 34 || c == 39 {
            if c == 34 && is(at(i + 1), b'`') {
                match src.find_str("`\"", i + 2) {
                    Some(close) => {
                        kind = T::MarkdownString;
                        end = close + 2;
                    }
                    None => {
                        kind = T::UnclosedMarkdownString;
                        end = n;
                    }
                }
            } else {
                kind = T::PlainString;
                match string_end(src, i, c) {
                    Some(close) => end = close,
                    None => {
                        let mut resume = i + 1;
                        while resume < n && !starts_token(src, resume) {
                            resume += 1;
                        }
                        return Err(LexFault { offset: i, length: resume - i });
                    }
                }
            }
        } else if c == 37 {
            if line_start && is(at(end), b'%') {
                kind = T::Comment;
                while end < n && !is_line_break(src[end]) {
                    end += 1;
                }
            } else {
                kind = T::Percent;
            }
        } else if c == 45 {
            if !is(at(end), b'-') {
                kind = T::Dash;
            } else {
                while is(at(end), b'-') {
                    end += 1;
                }
                let next = at(end);
                let two = end - i == 2;
                if two && is(next, b'|') && is(at(end + 1), b'>') {
                    kind = T::Generalization;
                    end += 2;
                } else if is(next, b'>') {
                    kind = T::ForwardSolid;
                    end += 1;
                } else if two && (is(next, b'o') || is(next, b'x')) {
                    kind = if is(next, b'o') { T::ForwardCircle } else { T::ForwardCross };
                    end += 1;
                } else {
                    kind = T::MarkerlessSolid;
                }
            }
        } else if c == 46 {
            let next = at(end);
            if is(next, b'.') && is(at(end + 1), b'>') {
                kind = T::DependencyArrow;
                end += 2;
            } else if digit(next) {
                kind = T::Number;
                while digit(at(end)) {
                    end += 1;
                }
                while is_letter(at(end)) {
                    end += 1;
                }
            } else {
                kind = T::Dot;
            }
        } else if c == 60 {
            if is(at(end), b'<') {
                tokens.push(T::StereotypeStart, i, i + 2);
                i += 2;
                if i >= n {
                    break;
                }
                if is(at(i), b'>') && is(at(i + 1), b'>') {
                    kind = T::StereotypeEnd;
                    end = i + 2;
                } else {
                    let mut close = i;
                    let mut found = false;
                    while close < n {
                        let ch = src[close];
                        if is_line_break(ch) {
                            break;
                        }
                        if ch == 62 && is(at(close + 1), b'>') {
                            found = true;
                            break;
                        }
                        close += 1;
                    }
                    if found && !src[i..close].trim_js().is_empty() {
                        tokens.push(T::StereotypeText, i, close);
                        kind = T::StereotypeEnd;
                        i = close;
                        end = close + 2;
                    } else {
                        while close < n && !is_line_break(src[close]) {
                            close += 1;
                        }
                        if close == i {
                            while close < n && is_line_break(src[close]) {
                                close += 1;
                            }
                            return Err(LexFault { offset: i, length: close - i });
                        }
                        kind = T::UnclosedStereotypeText;
                        end = close;
                    }
                }
            } else if is(at(end), b'-') && is(at(end + 1), b'-') {
                kind = T::BackwardSolid;
                end += 2;
                while is(at(end), b'-') {
                    end += 1;
                }
            } else {
                kind = T::LabelPunctuation;
            }
        } else if c == 58 {
            if is(at(end), b':') && is(at(end + 1), b':') {
                kind = T::ClassSeparator;
                end += 2;
            } else {
                kind = T::Colon;
            }
        } else if c == 64 {
            if is(at(end), b'{') {
                kind = T::MetadataStart;
                end += 1;
            } else {
                kind = T::At;
            }
        } else if c == 123 {
            kind = T::LBrace;
        } else if c == 125 {
            kind = T::RBrace;
        } else if c == 91 {
            kind = T::LBracket;
        } else if c == 93 {
            kind = T::RBracket;
        } else if c == 40 {
            kind = T::LParen;
        } else if c == 41 {
            kind = T::RParen;
        } else if c == 44 {
            kind = T::Comma;
        } else if c == 35 {
            while at(end).is_some_and(|c| hex_digit(c).is_some()) {
                end += 1;
            }
            kind = if end > i + 1 { T::HashColor } else { T::CssPunctuation };
        } else if c == 92 {
            if is(at(end), b',') {
                kind = T::CssEscapedComma;
                end += 1;
            } else {
                kind = T::LabelPunctuation;
            }
        } else if c < 127 && c > 32 {
            kind = if is_css_punctuation(c) { T::CssPunctuation } else { T::LabelPunctuation };
        } else {
            kind = T::LabelSymbol;
            while end < n {
                let ch = src[end];
                if (32..127).contains(&ch) || ch == 9 || ch == 10 || ch == 13 {
                    break;
                }
                end += 1;
            }
        }

        tokens.push(kind, i, end);
        i = end;
        line_start = fresh;
    }

    for _ in 0..3 {
        tokens.push(T::Eof, n, n);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_type_has_its_name_and_place() {
        assert_eq!(TOKEN_NAMES.split(' ').count(), TOKEN_COUNT);
        for (index, kind) in ALL.iter().enumerate() {
            assert_eq!(*kind as usize, index);
            assert_eq!(T::named(kind.name()), Some(*kind));
        }
        assert_eq!(T::SystemBoundary.name(), "SYSTEM_BOUNDARY");
        assert_eq!(T::RParen.name(), "RPAREN");
    }
}
