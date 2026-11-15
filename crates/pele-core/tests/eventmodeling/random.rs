//! Seeded random event models for `eventmodeling_front.rs` and `eventmodeling_parity.rs`: every
//! frame keyword and entity type, namespaces, flows named forwards, backwards, to themselves and
//! to nothing, data inline and in blocks, notes and specifications, titles in every place the
//! grammar takes them, comments, and now and then a fragment that the grammar rejects.

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

const KINDS: [&str; 9] = ["ui", "pcr", "processor", "rmo", "readmodel", "cmd", "command", "evt", "event"];
const NAMES: [&str; 14] = [
    "Cart",
    "AddItem",
    "ItemAdded",
    "CartView",
    "Checkout",
    "PaymentAccepted",
    "x",
    "_hidden",
    "titles",
    "eventx",
    "OrderShipmentNotificationProcessorForWarehouses",
    "A1",
    "CustomerAccountManagementReadModelProjection",
    "W",
];
const NAMESPACES: [&str; 5] = ["Cart", "Billing", "Shipping", "Customer_Account_Management_Context", "N"];
const DATA_TYPES: [&str; 8] = ["json", "jsobj", "figma", "salt", "uri", "md", "html", "text"];
const WORDS: [&str; 16] = [
    "productId: 7",
    "\"name\": \"Widget\"",
    "a",
    "total: 12.50",
    "<b>bold</b>",
    "#35; and #quot;",
    "x & y",
    "😀 emoji",
    "nested: { deep: true }",
    "a_rather_long_value_that_runs_past_the_forty_eight_column_limit_of_data",
    "  indented",
    "tab\there",
    "%% kept",
    "é ü 中文",
    "",
    "a very long line of note text that keeps going well past the eighty column limit of a note card",
];
const FRAGMENTS: [&str; 22] = [
    "tf", "->>", "[[", "]]", "`", ".", "{", "}", "\"", "'", "given", "then", "when", "note 01", "gwt", "1234", "title",
    "accDescr", "/*", "é", ":", "data",
];

fn name(rng: &mut Rng) -> String {
    let base = rng.pick(&NAMES).to_string();
    match rng.below(10) {
        0..=2 => format!("{}.{base}", rng.pick(&NAMESPACES)),
        3 => format!("a.{base}.c"),
        _ => base,
    }
}

fn text(rng: &mut Rng, lines: usize) -> Vec<String> {
    (0..lines).map(|_| rng.pick(&WORDS).to_string()).collect()
}

fn block(rng: &mut Rng) -> String {
    let indent = *rng.pick(&["", " ", "  ", "\t", "    "]);
    let count = 1 + rng.below(4);
    let lines = text(rng, count);
    let mut out = String::from("{");
    if rng.chance(10) {
        out.push_str("  ");
    }
    out.push('\n');
    for (i, line) in lines.iter().enumerate() {
        let extra = if i % 2 == 1 && rng.chance(30) { "  " } else { "" };
        out.push_str(&format!("{indent}{extra}{line}\n"));
    }
    if rng.chance(10) {
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

fn inline(rng: &mut Rng) -> String {
    let words = text(rng, 1).remove(0);
    let typed = if rng.chance(25) { format!("`{}`", rng.pick(&DATA_TYPES)) } else { String::new() };
    match rng.below(5) {
        0 => format!("{typed}\"{}\"", words.replace('"', "'")),
        1 => format!("{typed}'{}'", words.replace('\'', "\"")),
        2 => format!("{typed}{{ {words} }} {{ more }}"),
        _ => format!("{typed}{{ {words} }}"),
    }
}

fn frame_id(rng: &mut Rng, index: usize) -> String {
    match rng.below(12) {
        0 => format!("{}", index + 1),
        1 => format!("{:03}", index + 1),
        _ => format!("{:02}", index + 1),
    }
}

/// A random event model, mostly one Pele draws.
pub fn source(rng: &mut Rng) -> String {
    let mut out = String::new();
    if rng.chance(8) {
        out.push_str(&format!("---\ntitle: {}\n---\n", rng.pick(&["Front", "Front <b>matter</b>", "A title"])));
    }
    out.push_str("eventmodeling");
    match rng.below(10) {
        0 => out
            .push_str(&format!(" title {}", rng.pick(&["Checkout", "A <i>shop</i>", "Order flow with a long title"]))),
        1 => out.push_str(" accTitle: Accessible"),
        2 => out.push_str(" accDescr {\n  described\n   on two lines\n}"),
        3 => out.push_str("%%c"),
        _ => {}
    }
    out.push('\n');
    let frames = rng.below(13);
    let mut ids: Vec<String> = Vec::new();
    let mut data_names: Vec<String> = Vec::new();
    for index in 0..frames {
        if rng.chance(8) {
            out.push_str(rng.pick(&["%% a comment\n", "// a comment\n", "/* block\n comment */\n", "\n"]));
        }
        let keyword = rng.pick(&["tf", "tf", "tf", "timeframe", "rf", "resetframe"]);
        let id = if !ids.is_empty() && rng.chance(1) { rng.pick(&ids).clone() } else { frame_id(rng, index) };
        let mut line = format!("{keyword} {id} {} {}", rng.pick(&KINDS), name(rng));
        if rng.chance(25) {
            for _ in 0..1 + rng.below(3) {
                let source = if !ids.is_empty() && rng.chance(80) {
                    rng.pick(&ids).clone()
                } else if rng.chance(50) {
                    id.clone()
                } else {
                    format!("{:02}", 1 + rng.below(20))
                };
                line.push_str(&format!(" ->> {source}"));
            }
        }
        if rng.chance(15) {
            let reference = if !data_names.is_empty() && rng.chance(50) {
                rng.pick(&data_names).clone()
            } else {
                format!("D{}", rng.below(4))
            };
            line.push_str(&format!(" [[{reference}]]"));
        }
        if rng.chance(35) {
            line.push(' ');
            line.push_str(&inline(rng));
        }
        if rng.chance(4) {
            line.push_str(rng.pick(&[" accDescr: described", " accTitle: titled", " title Late", " %% trailing"]));
        }
        out.push_str(&line);
        out.push('\n');
        ids.push(id);
        if rng.chance(12) {
            let data = format!("D{}", rng.below(4));
            let typed = if rng.chance(30) { format!(" `{}`", rng.pick(&DATA_TYPES)) } else { String::new() };
            out.push_str(&format!("data {data}{typed} {}", block(rng)));
            data_names.push(data);
        }
    }
    if rng.chance(15) {
        out.push_str(&format!("entity {}\n", name(rng)));
    }
    for _ in 0..rng.below(3) {
        if !rng.chance(40) {
            continue;
        }
        let frame = if !ids.is_empty() && rng.chance(85) { rng.pick(&ids).clone() } else { "99".to_owned() };
        let typed = if rng.chance(30) { format!(" `{}`", rng.pick(&DATA_TYPES)) } else { String::new() };
        out.push_str(&format!("note {frame}{typed} {}", block(rng)));
    }
    for _ in 0..rng.below(3) {
        if !rng.chance(35) {
            continue;
        }
        let frame = if !ids.is_empty() && rng.chance(85) { rng.pick(&ids).clone() } else { "98".to_owned() };
        let clause = |rng: &mut Rng| {
            (0..1 + rng.below(3)).map(|_| format!("{} {}", rng.pick(&KINDS), rng.pick(&NAMES))).collect::<Vec<_>>()
        };
        let given = clause(rng).join(" ");
        let when = if rng.chance(50) { format!(" when {}", clause(rng).join(" ")) } else { String::new() };
        let then = clause(rng).join(if rng.chance(50) { "\n  " } else { " " });
        out.push_str(&format!("gwt {frame} given {given}{when} then {then}\n"));
    }
    if rng.chance(6) {
        let at = rng.below(out.len() + 1);
        let at = (0..=at).rev().find(|&i| out.is_char_boundary(i)).unwrap_or(0);
        out.insert_str(at, rng.pick(&FRAGMENTS));
    }
    out
}
