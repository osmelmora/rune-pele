//! Small helpers that several of Pele's diagram types share.
//!
//! Each one is ported before the types that use it, so parallel ports reuse one copy. Helpers
//! that belong to a module of their own live there, and [`probe`] reaches them for the tests.

use std::sync::OnceLock;

pub mod probe;

/// Pele's `parseGenericTypes` (`src/diagrams/common/generics.ts`): turns `List~int~` into
/// `List<int>`, with the quirks of Mermaid's parser around commas. Class and ER use it.
pub fn parse_generic_types(input: &str) -> String {
    if !input.contains('~') {
        return input.to_owned();
    }
    // `input.split(/(,)/)`: the pieces between commas with each comma kept as its own piece.
    let mut sets: Vec<&str> = Vec::new();
    let mut rest = input;
    while let Some(at) = rest.find(',') {
        sets.push(&rest[..at]);
        sets.push(",");
        rest = &rest[at + 1..];
    }
    sets.push(rest);
    let mut output: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sets.len() {
        let mut set = sets[i].to_owned();
        if sets[i] == "," && i > 0 && i + 1 < sets.len() && tildes(sets[i - 1]) == 1 && tildes(sets[i + 1]) == 1 {
            set = format!("{},{}", sets[i - 1], sets[i + 1]);
            i += 1;
            output.pop();
        }
        output.push(pair_tildes(&set));
        i += 1;
    }
    output.concat()
}

fn tildes(text: &str) -> usize {
    text.bytes().filter(|&b| b == b'~').count()
}

/// Pairs tildes from the outside in: the first with the last, the second with the one before it.
/// With an odd count and a leading tilde, the leading one stays a tilde.
fn pair_tildes(text: &str) -> String {
    let count = tildes(text);
    if count <= 1 {
        return text.to_owned();
    }
    let lead = !count.is_multiple_of(2) && text.starts_with('~');
    let mut bytes = text.as_bytes().to_vec();
    let at: Vec<usize> =
        bytes.iter().enumerate().skip(usize::from(lead)).filter(|&(_, &b)| b == b'~').map(|(i, _)| i).collect();
    let (mut lo, mut hi) = (0, at.len() - 1);
    while lo < hi {
        bytes[at[lo]] = b'<';
        bytes[at[hi]] = b'>';
        lo += 1;
        hi -= 1;
    }
    String::from_utf8(bytes).expect("only ASCII tildes were replaced")
}

/// Pele's packed table of letter ranges (`src/util/unicode.ts`), as base-36 gaps and lengths.
const PACKED: &str = "4q 0 b 0 5 0 6 m 2 u 2 cp 5 b f 4 8 0 2 0 3m 4 2 1 3 3 9 0 2 2 2 0 2 j 2 2a 2 3u 9 4d a 11 3 0 8 12 21 q 6 2 1a 16 10 1 2 2q 2 0 g 1 8 1 b 2 3 0 h 0 2 t u 2g c 0 p w a 1 5 0 6 l 5 0 a 0 4 0 o o 20 0 2 a 2g 1h 4 0 j 0 8 9 g 6 2 6 6 7 3 1 3 l 2 6 2 0 4 3 4 0 h 0 e 1 2 2 f 1 k 5 5 1 3 l 2 6 2 1 2 1 2 1 w 3 2 0 k 2 h 8 2 2 2 l 2 6 2 1 2 4 4 0 j 0 g 1 10 7 3 1 3 l 2 6 2 1 2 4 4 0 v 1 2 2 g 0 i 0 2 5 4 2 2 3 4 1 2 0 2 1 4 1 4 2 4 b n 0 1h 7 2 2 2 m 2 9 2 4 4 0 r 1 7 1 10 7 2 2 2 m 2 9 2 4 4 0 x 0 2 1 g 1 j 7 2 2 2 14 3 0 h 0 i 1 p 5 6 h 4 n 2 8 2 0 3 6 1n 1b 2 1 d 6 1n 1 2 0 3 1 2 0 3 0 7 3 2 6 2 2 2 0 2 0 3 1 2 3 2 1 a 0 3 4 2 0 m 3 x 0 1s 7 2 z s 4 38 16 l 0 h 5 5 3 4 0 4 1 8 2 5 c d 0 i 11 2 0 6 0 3 16 2 98 2 3 3 6 2 0 2 3 3 14 2 3 3 w 2 3 3 6 2 0 2 3 3 e 2 1k 2 3 3 1u 12 f h 2c d h7 3 g 2 p 6 22 m c 2 3 f h f h f c 2 2 g 1f 10 0 5 0 1w 2f 9 14 2 0 6 1x b s 1g t 3 4 c 17 m 6 1l m a 1g 2b 0 2m 1a i 6 1k t e 1 b 17 r z 16 2 b z 30 3 2 3 4 1 a 5b 1t 7p 3 5 3 11 3 5 3 7 2 0 2 0 2 0 2 u 3 1g 2 6 2 0 4 2 2 6 4 3 3 5 5 c 6 2 2 6 39 0 e 0 h c 2u 0 5 0 3 9 2 0 4 4 7 0 2 0 2 0 2 3 2 a 3 3 6 4 5 0 1h 1 22k 1a 2 1a 2 3o 7 3 4 1 d 11 2 0 6 0 3 1j 8 0 h m a 6 2 6 2 6 2 6 2 6 2 6 2 6 2 6 29 0 d2 1 17 4 6 1 5 2d 7 2 2 2h 2 3 6 14 4 2l i q 1i f e9 52t 23 g5o 1g wc 1w 19 3 7g 4 f b 1 l 1a h o 9 1x 1e 8 3 2u 3 3 2 3 d a 26 9 2 2 2 3 2 m u 1f f 1d 1r 5 4 0 f r b m q s 8 1a t 0 1d 14 o 2 2 7 l m 4 0 6 1b 2 0 4 1 3 4 3 0 2 0 p 2 3 a 8 2 d 5 3 5 3 5 a 6 2 6 42 y u 8mb d m 5 1c 6it a5 3 2x 13 6 d 4 6 0 2 9 2 c 2 4 2 0 2 1 2 1 2 2z y a2 j 1r 3 1h 15 b 39 4 2 3q 11 p 7 p c 2g 4 5 3 5 3 5 3 2";

/// Pele's `isUnicodeLetter`: whether a UTF-16 code unit is a letter in Mermaid's lexers.
/// Class and agentflow use it. It takes a code unit, as `charCodeAt` gives, not a scalar value.
pub fn is_unicode_letter(code: u16) -> bool {
    if !(0xaa..=0xffdc).contains(&code) {
        return false;
    }
    static TABLE: OnceLock<Vec<(u16, u16)>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let parts: Vec<u32> =
            PACKED.split(' ').map(|part| u32::from_str_radix(part, 36).expect("base-36 table entry")).collect();
        let mut prev = 0u32;
        parts
            .chunks(2)
            .map(|pair| {
                let start = prev + pair[0];
                prev = start + pair[1];
                // Pele stores both ends in a Uint16Array, which keeps the low 16 bits.
                (start as u16, prev as u16)
            })
            .collect()
    });
    let (mut lo, mut hi) = (0isize, table.len() as isize - 1);
    while lo <= hi {
        let mid = (lo + hi) >> 1;
        let (start, end) = table[mid as usize];
        if code < start {
            hi = mid - 1;
        } else if code > end {
            lo = mid + 1;
        } else {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generics_pair_from_the_outside_in() {
        assert_eq!(parse_generic_types("List~int~"), "List<int>");
        assert_eq!(parse_generic_types("Map~K, V~"), "Map<K, V>");
        assert_eq!(parse_generic_types("~a~b~"), "~a<b>");
        assert_eq!(parse_generic_types("plain"), "plain");
    }

    #[test]
    fn letters_follow_the_table() {
        assert!(!is_unicode_letter(u16::from(b'a')));
        assert!(is_unicode_letter(0xaa));
        assert!(is_unicode_letter(0xe9));
        assert!(!is_unicode_letter(0xd7));
    }
}
