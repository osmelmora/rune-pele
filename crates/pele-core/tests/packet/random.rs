//! Seeded random packet diagrams for `packet_front.rs` and `packet_parity.rs`: fields written by
//! their bits, by a range or by a width, labels short and long, fields that fill a row or run over
//! several, each setting of the `packet` config, and a few fields that break Pele's rules.

use serde_json::{Value, json};

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

const LABELS: &[&str] = &[
    "Source Port",
    "Destination Port",
    "Sequence Number",
    "URG",
    "x",
    "",
    "Data offset",
    "Payload (variable length)",
    "A field label that is much too long for a narrow field",
    "#35; & <b>x</b>",
    "two\\nlines",
    "\u{4e2d}\u{6587}",
    "  padded  ",
    "Window",
];

/// One random packet diagram.
pub fn random_packet(rng: &mut Rng) -> String {
    let mut config = Vec::new();
    let settings: &[(&str, &[&str])] = &[
        ("bitsPerRow", &["8", "16", "20", "64", "1", "4096", "-4", "7.5"]),
        ("bitWidth", &["10", "18", "50", "\"wide\""]),
        ("rowHeight", &["20", "80", "0"]),
        ("paddingX", &["0", "20", "9999"]),
        ("paddingY", &["0", "30"]),
        ("showBits", &["false", "true"]),
        ("bitOrder", &["descending", "ascending"]),
    ];
    for (key, values) in settings {
        if rng.chance(14) {
            config.push(format!("    {key}: {}", rng.pick(values)));
        }
    }
    let mut out = String::new();
    if !config.is_empty() || rng.chance(8) {
        out.push_str("---\n");
        if rng.chance(25) {
            out.push_str("title: From front matter\n");
        }
        if !config.is_empty() {
            out.push_str("config:\n  packet:\n");
            for line in &config {
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str("---\n");
    }
    out.push_str(rng.pick(&["packet", "packet-beta"]));
    out.push('\n');
    if rng.chance(20) {
        out.push_str(rng.pick(&[
            "title TCP Packet\n",
            "title A title far wider than the packet under it\n",
            "title #35; t\n",
        ]));
    }
    if rng.chance(8) {
        out.push_str("accTitle: Fields\naccDescr: Bits of a header\n");
    }
    let mut next = 0u64;
    for _ in 0..rng.below(12) {
        let width = *rng.pick(&[1, 1, 2, 4, 8, 8, 16, 16, 24, 32, 40, 64, 100]);
        let label = rng.pick(LABELS);
        let line = match rng.below(150) {
            0 => format!("{}-{}: \"{label}\"", next + 2, next + 1 + width),
            1 => format!("{}-{}: \"{label}\"", next + width, next),
            2 => format!("+0: \"{label}\""),
            3..=50 => format!("+{width}: \"{label}\""),
            51..=80 if width == 1 => format!("{next}: \"{label}\""),
            _ if width == 1 => format!("{next}: '{label}'"),
            _ => format!("{next}-{}: \"{label}\"", next + width - 1),
        };
        out.push_str(&line);
        out.push('\n');
        next += width;
    }
    out
}

/// Render options a host may give: a width, which may draw the rows shorter, and other sizes.
pub fn random_options(rng: &mut Rng) -> Value {
    let mut options = serde_json::Map::new();
    if rng.chance(45) {
        options.insert("maxWidth".to_owned(), json!(*rng.pick(&[120.0, 200.0, 320.0, 480.0, 639.0, 900.0])));
    }
    if rng.chance(10) {
        options.insert("fontSize".to_owned(), json!(*rng.pick(&[12.0, 13.5, 20.0])));
    }
    if rng.chance(10) {
        options.insert("padding".to_owned(), json!(*rng.pick(&[0.0, 20.5])));
    }
    if rng.chance(5) {
        options.insert("autoDirection".to_owned(), json!(false));
    }
    Value::Object(options)
}
