//! Differential tests for the helpers several diagram types share: every answer must equal
//! Pele 1.13.0's in the pinned Node.

use pele_core::common::{is_unicode_letter, parse_generic_types};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Short strings over the characters `parseGenericTypes` reacts to, plus a few it must carry
/// through untouched, including a character outside the BMP.
fn generic_inputs() -> Vec<String> {
    let pieces = ["~", "~", "~", ",", ",", " ", "a", "List", "int", "<", ">", "é", "😀", "~~", ",~"];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut inputs: Vec<String> = ["", "~", ",", "~,~", "List~int~", "Map~K, V~", "~a~b~", "a~b,c~d", "~,~,~", "x~~"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for _ in 0..20_000 {
        let len = rng.below(12);
        inputs.push((0..len).map(|_| pieces[rng.below(pieces.len())]).collect());
    }
    inputs
}

#[test]
fn parse_generic_types_matches_pele() {
    let inputs = generic_inputs();
    let requests: Vec<String> =
        inputs.iter().map(|s| format!("generics\t{}", serde_json::to_string(s).expect("a string encodes"))).collect();
    let answers = pele_oracle::ask(&requests);
    let mut differences = Vec::new();
    for (input, answer) in inputs.iter().zip(&answers) {
        let theirs: String = serde_json::from_str(answer).expect("a JSON string");
        let ours = parse_generic_types(input);
        if ours != theirs {
            differences.push(format!("{input:?}: ours {ours:?}, Pele {theirs:?}"));
        }
    }
    assert!(differences.is_empty(), "{} of {} differ:\n{}", differences.len(), inputs.len(), differences.join("\n"));
}

#[test]
fn is_unicode_letter_matches_pele_for_every_code_unit() {
    let answer = pele_oracle::ask(&["unicode_letters".to_owned()]).remove(0);
    let theirs: Vec<u16> = serde_json::from_str(&answer).expect("a JSON array of code units");
    let ours: Vec<u16> = (0..=u16::MAX).filter(|&code| is_unicode_letter(code)).collect();
    assert!(theirs.len() > 40_000, "Pele accepted only {} code units; the op is wrong", theirs.len());
    assert_eq!(ours, theirs);
}
