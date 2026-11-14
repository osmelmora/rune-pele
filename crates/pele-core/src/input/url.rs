//! Pele's URL policy for a URL a diagram names (`src/util/url.ts`): what Mermaid's sanitizer
//! refuses becomes `about:blank`. An `http:` or `https:` URL is normalized by the WHATWG URL
//! parser, as `new URL` does.

use super::{JsText, Units, decode_entities, hex_digit, is_line_end, is_word, string_to_number, to_lowercase};

const BLANK: &str = "about:blank";

const fn unit(c: u8) -> u16 {
    c as u16
}

fn is_high(c: u16) -> bool {
    (0xd800..0xdc00).contains(&c)
}

fn is_low(c: u16) -> bool {
    (0xdc00..0xe000).contains(&c)
}

/// `[\u0000-\u001F\u007F-\u009F -‍﻿\uD800-\uDFFF￾￿]` under the `u` flag:
/// a lone surrogate matches, a surrogate pair does not.
fn strip_controls(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let c = text[at];
        if is_high(c) && text.at(at + 1).is_some_and(is_low) {
            out.extend_from_slice(&text[at..at + 2]);
            at += 2;
            continue;
        }
        let control =
            matches!(c, 0..=0x1f | 0x7f..=0x9f | 0x2000..=0x200d | 0xfeff | 0xd800..=0xdfff | 0xfffe | 0xffff);
        if !control {
            out.push(c);
        }
        at += 1;
    }
    out
}

/// `ToUint16`, as `String.fromCharCode` applies it.
fn to_uint16(value: f64) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    let wrapped = value.trunc() % 65536.0;
    (if wrapped < 0.0 { wrapped + 65536.0 } else { wrapped }) as u16
}

/// Where `&#(\w+)(^\w|;)?` matches at `at`, with the number its word names.
fn html_entity(text: &[u16], at: usize) -> Option<(usize, f64)> {
    if !text.starts_at(at, "&#") {
        return None;
    }
    let end = text.run(at + 2, is_word);
    if end == at + 2 {
        return None;
    }
    let number = string_to_number(&text[at + 2..end]);
    Some((end + usize::from(text.at(end) == Some(unit(b';'))), number))
}

/// The length of `&(newline|tab);` at `at`, ignoring ASCII case.
fn ctrl_entity(text: &[u16], at: usize) -> Option<usize> {
    ["&newline;", "&tab;"].into_iter().map(str::len).find(|&length| {
        let word = if length == 9 { "&newline;" } else { "&tab;" };
        text.get(at..at + length).is_some_and(|found| {
            found.iter().zip(word.bytes()).all(|(&c, b)| u8::try_from(c).is_ok_and(|c| c.eq_ignore_ascii_case(&b)))
        })
    })
}

/// The length of `(\\|%5[cC])((%(6[eE]|72|74))|[nrt])` at `at`.
fn ws_escape(text: &[u16], at: usize) -> Option<usize> {
    let after = if text.at(at) == Some(unit(b'\\')) {
        at + 1
    } else if text.starts_at(at, "%5c") || text.starts_at(at, "%5C") {
        at + 3
    } else {
        return None;
    };
    if matches!(text.at(after), Some(110 | 114 | 116)) {
        return Some(after + 1 - at);
    }
    ["%6e", "%6E", "%72", "%74"].iter().any(|code| text.starts_at(after, code)).then_some(after + 3 - at)
}

/// Replaces each match of a scanner, where `with` gives what takes its place.
fn replace(text: &[u16], matched: impl Fn(&[u16], usize) -> Option<(usize, Option<u16>)>) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        match matched(text, at) {
            Some((end, with)) => {
                out.extend(with);
                at = end;
            }
            None => {
                out.push(text[at]);
                at += 1;
            }
        }
    }
    out
}

/// `decodeURIComponent`, or `None` where it throws a `URIError`.
fn decode_uri_component(text: &[u16]) -> Option<Vec<u16>> {
    let byte = |at: usize| -> Option<u8> {
        let high = hex_digit(*text.get(at + 1)?)?;
        let low = hex_digit(*text.get(at + 2)?)?;
        (text[at] == unit(b'%')).then_some((high * 16 + low) as u8)
    };
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if text[at] != unit(b'%') {
            out.push(text[at]);
            at += 1;
            continue;
        }
        let first = byte(at)?;
        let count = first.leading_ones() as usize;
        if count == 0 {
            out.push(u16::from(first));
            at += 3;
            continue;
        }
        if count == 1 || count > 4 {
            return None;
        }
        let mut octets = vec![first];
        for index in 1..count {
            octets.push(byte(at + 3 * index)?);
        }
        let decoded = std::str::from_utf8(&octets).ok()?;
        out.extend(decoded.encode_utf16());
        at += 3 * count;
    }
    Some(out)
}

/// Pele's `unwrap`: the controls, entity codes and escapes that could hide a scheme removed, and
/// with `percent`, percent escapes too. `None` for a URL wrapped too many times to be a real one.
fn unwrap(url: &[u16], percent: bool) -> Option<Vec<u16>> {
    let decode = |text: Vec<u16>| decode_uri_component(&text).unwrap_or(text);
    let trimmed = url.trim_js().to_vec();
    let mut out = if percent { decode(trimmed) } else { trimmed };
    for _ in 0..32 {
        out = strip_controls(&out);
        out = replace(&out, |text, at| html_entity(text, at).map(|(end, number)| (end, Some(to_uint16(number)))));
        out = replace(&out, |text, at| ctrl_entity(text, at).map(|length| (at + length, None)));
        out = strip_controls(&out);
        out = replace(&out, |text, at| ws_escape(text, at).map(|length| (at + length, None)));
        out = out.trim_js().to_vec();
        if percent {
            out = decode(out);
        }
        let more = strip_controls(&out).len() != out.len()
            || (0..out.len()).any(|at| {
                html_entity(&out, at).is_some() || ctrl_entity(&out, at).is_some() || ws_escape(&out, at).is_some()
            });
        if !more {
            return Some(out);
        }
    }
    None
}

/// The first match of `/^.+(:|&colon;)/gim`: from the first line start where one exists, the
/// longest run of the line that ends in `:` or `&colon;`.
fn scheme(text: &[u16]) -> Option<&[u16]> {
    let starts = std::iter::once(0).chain((0..text.len()).filter(|&at| is_line_end(text[at])).map(|at| at + 1));
    starts.into_iter().find_map(|start| {
        let line_end = text.run(start, |c| !is_line_end(c));
        (1..=line_end.saturating_sub(start)).rev().find_map(|length| {
            let at = start + length;
            if text.at(at) == Some(unit(b':')) {
                return Some(&text[start..=at]);
            }
            let colon = text.get(at..at + 7).is_some_and(|found| {
                found.iter().zip(b"&colon;").all(|(&c, b)| u8::try_from(c).is_ok_and(|c| c.eq_ignore_ascii_case(b)))
            });
            colon.then(|| &text[start..at + 7])
        })
    })
}

/// `/^([^\w]*)(javascript|data|vbscript)/im.test(protocol)`.
fn invalid_protocol(protocol: &[u16]) -> bool {
    let starts = std::iter::once(0).chain((0..protocol.len()).filter(|&at| is_line_end(protocol[at])).map(|at| at + 1));
    starts.into_iter().any(|start| {
        let word = protocol.run(start, |c| !is_word(c));
        ["javascript", "data", "vbscript"].iter().any(|name| {
            protocol.get(word..word + name.len()).is_some_and(|found| {
                found.iter().zip(name.bytes()).all(|(&c, b)| u8::try_from(c).is_ok_and(|c| c.eq_ignore_ascii_case(&b)))
            })
        })
    })
}

/// Pele's `sanitizeUrl`: refuses what `@braintree/sanitize-url` refuses, but keeps percent
/// escapes.
fn sanitize_url(url: &[u16]) -> JsText {
    if url.is_empty() {
        return BLANK.into();
    }
    let (Some(decoded), Some(kept)) = (unwrap(url, true), unwrap(url, false)) else {
        return BLANK.into();
    };
    if decoded.is_empty() || kept.is_empty() {
        return BLANK.into();
    }
    if decoded[0] == unit(b'.') || decoded[0] == unit(b'/') {
        return kept.into();
    }
    let Some(scheme) = scheme(decoded.trim_start_js()) else {
        return kept.into();
    };
    let protocol = to_lowercase(scheme);
    let protocol = protocol.trim_js();
    if invalid_protocol(protocol) {
        return BLANK.into();
    }
    let normalized: Vec<u16> =
        kept.trim_start_js().iter().map(|&c| if c == unit(b'\\') { unit(b'/') } else { c }).collect();
    if protocol.is_str("mailto:") || protocol.has("://") {
        return normalized.into();
    }
    if protocol.is_str("http:") || protocol.is_str("https:") {
        // `new URL` reads a USVString, so a lone surrogate is U+FFFD to it.
        return match ::url::Url::parse(&String::from_utf16_lossy(&normalized)) {
            Ok(parsed) => parsed.as_str().into(),
            Err(_) => BLANK.into(),
        };
    }
    normalized.into()
}

/// Pele's `safeUrl`: a URL as written in a diagram, with entity codes read first so that they
/// cannot hide a scheme.
pub(crate) fn safe_url(url: &[u16]) -> JsText {
    sanitize_url(&decode_entities(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ours(url: &str) -> String {
        safe_url(&url.encode_utf16().collect::<Vec<_>>()).to_display()
    }

    fn urls() -> Vec<String> {
        let mut out: Vec<String> = [
            "",
            " ",
            "https://example.com",
            "https://Example.COM/a/../b?q=1#f",
            "HTTP://EXAMPLE.com:80/x",
            "http:example.com",
            "https://example.com:8080/path",
            "http://user:pass@example.com/",
            "http://[::A]:8080/",
            "http://192.168.0.1/",
            "http://0x7f.1/",
            "https://bücher.example/ü?ö#ä",
            "https://example.com/a b",
            "https://exa mple.com",
            "http://",
            "https:",
            "https:\\\\example.com\\a\\b",
            "mailto:someone@example.com",
            "MAILTO:A@B",
            "tel:+123",
            "ftp://example.com",
            "file:///etc/passwd",
            "//network/share",
            "/relative/path",
            "./here",
            "../up",
            "relative/path",
            "no-scheme",
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "  javascript:alert(1)",
            "\u{1}javascript:alert(1)",
            "java\nscript:alert(1)",
            "java\tscript:alert(1)",
            "javascript&colon;alert(1)",
            "javascript&COLON;alert(1)",
            "&#106;avascript:alert(1)",
            "&#x6A;avascript:alert(1)",
            "&#0x6A;avascript:alert(1)",
            "&#106avascript:alert(1)",
            "&#65601;x:y",
            "&#1e3;x",
            "&#Infinity;x",
            "&#0b1101010;avascript:x",
            "&#0o152;avascript:x",
            "&#0x10000000000000000006A;avascript:x",
            "&#99999999999999999999;x",
            "&#abc;x",
            "jav&#x09;ascript:alert(1)",
            "jav&NewLine;ascript:alert(1)",
            "jav&Tab;ascript:alert(1)",
            "%6A%61%76%61%73%63%72%69%70%74%3Aalert(1)",
            "%6a%61vascript:alert(1)",
            "%E0%A4%A",
            "%C0%80",
            "%ED%A0%80",
            "%F4%90%80%80",
            "%FF",
            "%",
            "%%",
            "javascript\\nalert",
            "java%5cnscript:x",
            "x\\%6E\\%72\\%74",
            "data:text/html,<script>",
            "vbscript:msgbox",
            "  data:x",
            "-data:x",
            "a:b:c",
            "x\u{2028}javascript:y",
            "x\u{2028}https:y",
            "\u{2028}http:y",
            "https://example.com/\u{200b}x",
            "https://example.com/\u{feff}",
            "https://example.com/#35;",
            "https://example.com/?a=#quot;b#quot;",
            "&amp;&lt;https://example.com",
            "http:/\\/x",
            "http://example.com/%zz",
            "http://exa%41mple.com/",
            "http://xn--bcher-kva.example/",
            "https://example.com/",
            "https://ex\u{0130}ample.com/",
            "ＨＴＴＰＳ://example.com",
            "\u{212a}ttp:x",
            "data\u{ff1a}x",
            "  https://example.com  ",
            "\thttps://example.com",
            "https://example.com/\u{1f600}",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        out.push(format!("{}x", "%25".repeat(40)));
        out.push(format!("https://example.com/{}", "a".repeat(3000)));
        out.push(format!("{}javascript:x", "&#38;#38;".repeat(10)));
        out
    }

    #[test]
    fn safe_url_matches_pele() {
        let urls = urls();
        let requests: Vec<String> = urls.iter().map(|url| format!("safe_url\t{}", crate::json_string(url))).collect();
        let theirs = pele_oracle::ask(&requests);
        let mismatches: Vec<String> = urls
            .iter()
            .zip(&theirs)
            .filter_map(|(url, theirs)| {
                let theirs: String = serde_json::from_str(theirs).expect("the oracle answers a JSON string");
                let ours = ours(url);
                (ours != theirs).then(|| format!("{url:?}: ours {ours:?}, pele {theirs:?}"))
            })
            .collect();
        assert!(
            mismatches.is_empty(),
            "{} of {} URLs differ:\n{}",
            mismatches.len(),
            urls.len(),
            mismatches.join("\n")
        );
    }
}
