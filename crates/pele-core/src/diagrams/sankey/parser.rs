//! Pele's sankey parser (`src/diagrams/sankey/parser.ts`): the rows of CSV that Mermaid's
//! `sankey.jison` accepts, read after the text is trimmed as Mermaid's `prepareTextForParsing`
//! trims it, with errors placed in the text before that trimming.

use super::lexer::{T, tokenize};
use super::{Link, SankeyModel};
use crate::diagrams::tokens::SyntaxError;
use crate::input::{Edit, JsText, Steps, Units, decimal, decode_entities, input_offset, is_digit};
use std::collections::HashMap;

/// Sankey's own blank: a space, but not a line break.
fn is_blank(c: u16) -> bool {
    if c == 10 || c == 13 {
        return false;
    }
    if c < 128 {
        return c == 32 || c == 9 || c == 11 || c == 12;
    }
    matches!(c, 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

/// Pele's `trimRows`: the blanks at the edges of each row outside quoted fields go, but a blank
/// first field stays, as Mermaid accepts it as an empty name.
fn trim_rows(text: &[u16], edits: &mut Vec<Edit>) -> Vec<u16> {
    let mut end = text.len();
    while end > 0 && is_blank(text[end - 1]) {
        end -= 1;
    }
    let mut out = Vec::with_capacity(end);
    let mut kept = 0;
    let mut quoted = false;
    let mut i = 0;
    while i < end {
        let c = text[i];
        if quoted {
            if c == 34 {
                if text.at(i + 1) == Some(34) {
                    i += 1;
                } else {
                    quoted = false;
                }
            }
            i += 1;
        } else if c == 34 {
            quoted = true;
            i += 1;
        } else if is_blank(c) {
            let j = text.run(i + 1, is_blank);
            let before = i.checked_sub(1).map(|k| text[k]);
            let after = text.at(j);
            let line_end = |c: Option<u16>| c == Some(10) || c == Some(13);
            if i == 0 || line_end(after) || (line_end(before) && after != Some(44)) {
                out.extend_from_slice(&text[kept..i]);
                edits.push((i, j, 0));
                kept = j;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    if end < text.len() {
        edits.push((end, text.len(), 0));
    }
    if kept < end {
        out.extend_from_slice(&text[kept..end]);
    }
    out
}

/// Pele's `prepareTextForParsing`: Mermaid's, which collapses line breaks and trims the text,
/// and also trims row edges outside quoted fields. With `steps`, records where each kept unit was.
pub(super) fn prepare_text_for_parsing(text: &[u16], steps: Option<&mut Steps>) -> Vec<u16> {
    let mut edits = Vec::new();
    let rows = trim_rows(text, &mut edits);
    // `rows.replace(/[\n\r]+/g, '\n')`, and where each run of line breaks was.
    let mut collapsed = Vec::with_capacity(rows.len());
    let mut breaks = Vec::new();
    let mut at = 0;
    while at < rows.len() {
        let end = rows.run(at, |c| c == 10 || c == 13);
        if end > at {
            breaks.push((at, end, 1));
            collapsed.push(10);
            at = end;
        } else {
            collapsed.push(rows[at]);
            at += 1;
        }
    }
    let trimmed = collapsed.trim_js().to_vec();
    if let Some(steps) = steps {
        let lead = collapsed.len() - collapsed.trim_start_js().len();
        steps.push(edits);
        steps.push(breaks);
        steps.push(vec![(0, lead, 0), (lead + trimmed.len(), collapsed.len(), 0)]);
    }
    trimmed
}

/// `parseFloat`: the longest prefix of the text, past its leading white space, that reads as a
/// decimal number, or NaN where none does.
fn parse_float(text: &[u16]) -> f64 {
    let text = text.trim_start_js();
    let digits = usize::from(matches!(text.at(0), Some(43 | 45)));
    if text[digits..].starts_at(0, "Infinity") {
        return if text.at(0) == Some(45) { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let whole = text.run(digits, is_digit);
    let mut end = whole;
    let mut any = whole > digits;
    if text.at(end) == Some(46) {
        let fraction = text.run(end + 1, is_digit);
        if any || fraction > end + 1 {
            end = fraction;
            any = true;
        }
    }
    if !any {
        return f64::NAN;
    }
    if matches!(text.at(end), Some(101 | 69)) {
        let sign = end + 1 + usize::from(matches!(text.at(end + 1), Some(43 | 45)));
        let exponent = text.run(sign, is_digit);
        if exponent > sign {
            end = exponent;
        }
    }
    decimal(&text[..end])
}

/// `/[\x00-\x08\x0b\x0c\x0e-\x1f\ud800-\udfff￾￿]/u`
fn has_invalid_xml(text: &[u16]) -> bool {
    char::decode_utf16(text.iter().copied()).any(|item| match item {
        Ok(c) => matches!(c, '\0'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'),
        Err(_) => true,
    })
}

/// Pele's `parseSankey`, on the trimmed text `src`. An error is placed in `source`, the text
/// before trimming, as an offset into it.
pub(super) fn parse_sankey(src: &[u16], source: &[u16], title: Option<JsText>) -> Result<SankeyModel, SyntaxError> {
    let tokens = tokenize(src);
    let types = &tokens.types;
    let text = |i: usize| &src[tokens.spans[i].0..tokens.spans[i].1];
    let start = |i: usize| tokens.spans[i].0;
    let mut i = 0;

    let error = |at: usize, detail: String| {
        let mut steps = Steps::new();
        if source != src {
            prepare_text_for_parsing(source, Some(&mut steps));
        }
        // Map EOF before the trimmed trailing blanks.
        let offset =
            if at > 0 && at >= src.len() { input_offset(&steps, src.len() - 1) + 1 } else { input_offset(&steps, at) };
        SyntaxError { at: offset, detail, lexical: false }
    };
    let fail = |i: usize, expected: &str| {
        let got = match types[i] {
            T::Stuck => i.checked_sub(1).map_or("undefined", |previous| types[previous].name()),
            kind => kind.name(),
        };
        error(start(i), format!("Expecting {expected}, got '{got}'"))
    };
    let expect = |i: &mut usize, kind: T, also: &str| {
        if types[*i] != kind {
            return Err(fail(*i, &format!("'{}'{also}", kind.name())));
        }
        *i += 1;
        Ok(())
    };
    let field = |i: &mut usize| -> Result<&[u16], SyntaxError> {
        if types[*i] == T::NonEscapedText {
            *i += 1;
            return Ok(text(*i - 1));
        }
        if types[*i] != T::Dquote {
            return Err(fail(*i, "'DQUOTE', 'NON_ESCAPED_TEXT'"));
        }
        *i += 1;
        if types[*i] != T::EscapedText {
            return Err(fail(*i, "'ESCAPED_TEXT'"));
        }
        let value = text(*i);
        *i += 1;
        expect(i, T::Dquote, "")?;
        Ok(value)
    };
    let name = |i: &mut usize| -> Result<JsText, SyntaxError> {
        let at = start(*i);
        let raw = field(i)?.trim_js();
        // `.replaceAll('""', '"')`
        let mut name = Vec::with_capacity(raw.len());
        let mut k = 0;
        while k < raw.len() {
            name.push(raw[k]);
            k += if raw[k] == 34 && raw.at(k + 1) == Some(34) { 2 } else { 1 };
        }
        // Entity codes can encode characters excluded by the lexer.
        if has_invalid_xml(&decode_entities(&name)) {
            return Err(error(at, "Invalid XML character in node name".to_owned()));
        }
        Ok(name.into())
    };

    let mut model = SankeyModel { nodes: Vec::new(), links: Vec::new(), title };
    let mut lookup: HashMap<JsText, usize> = HashMap::new();
    let mut node = |model: &mut SankeyModel, id: JsText| {
        *lookup.entry(id.clone()).or_insert_with(|| {
            model.nodes.push(id);
            model.nodes.len() - 1
        })
    };
    expect(&mut i, T::Sankey, "")?;
    expect(&mut i, T::Newline, "")?;
    loop {
        let from = name(&mut i)?;
        expect(&mut i, T::Comma, ", 'NEWLINE', 'EOF'")?;
        let to = name(&mut i)?;
        expect(&mut i, T::Comma, ", 'NEWLINE', 'EOF'")?;
        let at = start(i);
        // Keep Mermaid's numeric prefixes, but reject non-finite values.
        let value = parse_float(field(&mut i)?);
        if !value.is_finite() {
            return Err(error(at, "Expecting a number".to_owned()));
        }
        let source = node(&mut model, from);
        let target = node(&mut model, to);
        model.links.push(Link { source, target, value });
        if types[i] != T::Newline {
            break;
        }
        i += 1;
    }
    if types[i] == T::Eof {
        i += 1;
    }
    if types[i] != T::End {
        return Err(fail(i, "'NEWLINE', 'EOF'"));
    }
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn parse_float_reads_the_longest_number_first() {
        for (text, expected) in [
            ("1", 1.0),
            ("  -1.5e3xyz", -1500.0),
            ("1.", 1.0),
            (".5", 0.5),
            ("1e", 1.0),
            ("1e+", 1.0),
            ("+Infinityx", f64::INFINITY),
            ("10%", 10.0),
            ("\u{a0}7", 7.0),
        ] {
            assert_eq!(parse_float(&units(text)), expected, "{text:?}");
        }
        for text in ["", ".", "x1", "-", "e5", "infinity"] {
            assert!(parse_float(&units(text)).is_nan(), "{text:?}");
        }
    }
}
