//! Seeded random tree views for `treeview_front.rs` and `treeview_parity.rs`: entries indented and
//! drawn with box characters, folders and files, quoted names, every annotation, icons from the
//! config, the layout settings in and out of their ranges, and now and then a fragment that the
//! grammar or the box drawing rejects.

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

const FILES: [&str; 18] = [
    "index.ts",
    "README.md",
    "package.json",
    "Cargo.TOML",
    "main.rs",
    ".gitignore",
    "my file.txt",
    "a",
    "title.md",
    "a_very_long_file_name_that_goes_on_and_on_past_any_narrow_width.config.js",
    "notes with several words that can wrap at a narrow width.txt",
    "#35;hash.md",
    "x &amp; y.txt",
    "é.txt",
    "中文名字.md",
    "😀.png",
    "Makefile",
    "lib.d.ts",
];
const DIRS: [&str; 8] =
    ["src/", "lib/", "tests/", "docs/", "node_modules/", "a b/", "deeply_named_directory_for_wrapping/", "/"];
const DESCRIPTIONS: [&str; 6] = [
    "entry point",
    "the main module",
    "a long description that keeps going until it has to wrap on a narrow host",
    "#35; with codes &amp; more",
    "done",
    "中文",
];
const FRAGMENTS: [&str; 16] = [
    ":::",
    " :::",
    " :::1",
    " icon(",
    " icon(a:)",
    " ##",
    "\"",
    "'",
    "├── ",
    "└── ",
    "│   ",
    "title",
    "accDescr {",
    "\t",
    "  ",
    "\n",
];

fn name(rng: &mut Rng, directory: bool) -> String {
    let base = if directory { rng.pick(&DIRS).to_string() } else { rng.pick(&FILES).to_string() };
    match rng.below(12) {
        0 => format!("\"{base}\""),
        1 => format!("'{}'", base.replace('\'', "")),
        _ => base,
    }
}

fn annotations(rng: &mut Rng) -> String {
    let mut out = String::new();
    for _ in 0..rng.below(3) {
        match rng.below(9) {
            0 | 1 => out.push_str(" :::highlight"),
            2 => out.push_str(rng.pick(&[" :::other", " ::: muted-row", " :::a :::b"])),
            3 => out.push_str(rng.pick(&[
                " icon(x)",
                " icon(p:x)",
                " icon()",
                " icon(none)",
                " icon(folder)",
                " icon(file)",
            ])),
            4 => out.push_str(rng.pick(&[" icon(mermaid-treeview:folder)", " icon(mdi:home)", " icon(-)"])),
            _ => out.push_str(&format!(" ## {}", rng.pick(&DESCRIPTIONS))),
        }
    }
    out
}

fn config(rng: &mut Rng) -> String {
    let mut lines = Vec::new();
    if rng.chance(50) {
        lines.push(format!("    showIcons: {}", rng.pick(&["true", "false", "1", "0"])));
    }
    if rng.chance(30) {
        lines.push(format!("    defaultIconPack: {}", rng.pick(&["p", "''", "mdi"])));
    }
    if rng.chance(25) {
        lines
            .push("    filenameIcons:\n      README.md: book\n      Makefile: none\n      package.json: ''".to_owned());
    }
    if rng.chance(25) {
        lines.push("    extensionIcons:\n      .ts: ts\n      md: p:md\n      .png: none".to_owned());
    }
    for (key, values) in [
        ("paddingX", &["0", "12", "101", "-1"][..]),
        ("paddingY", &["0", "6", "100", "200"][..]),
        ("rowIndent", &["0", "30", "400", "500", "'9'"][..]),
        ("lineThickness", &["0.25", "3", "12", "0.1", "13"][..]),
    ] {
        if rng.chance(15) {
            lines.push(format!("    {key}: {}", rng.pick(values)));
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("---\nconfig:\n  treeView:\n{}\n---\n", lines.join("\n"))
}

/// A random tree view, mostly one Pele draws.
pub fn source(rng: &mut Rng) -> String {
    let mut out = if rng.chance(45) { config(rng) } else { String::new() };
    out.push_str("treeView-beta\n");
    if rng.chance(15) {
        out.push_str(&format!(
            "title {}\n",
            rng.pick(&["Project", "A <b>bold</b> tree", "A title long enough to wrap at a narrow width"])
        ));
    }
    if rng.chance(10) {
        out.push_str("accTitle: Files\naccDescr {\n  the files\n   of a project\n}\n");
    }
    let boxed = rng.chance(30);
    let unit = *rng.pick(&[2usize, 4, 4, 3, 1]);
    let entries = rng.below(14);
    let mut depth = 0usize;
    for index in 0..entries {
        if rng.chance(5) {
            out.push_str(rng.pick(&["%% a comment\n", "\n", "   \n"]));
        }
        let directory = rng.chance(35);
        let text = format!("{}{}", name(rng, directory), annotations(rng));
        if boxed {
            if index == 0 || depth == 0 && rng.chance(30) {
                out.push_str(&format!("{text}\n"));
                depth = 1;
                continue;
            }
            let mut prefix = String::new();
            for _ in 1..depth {
                prefix.push_str(rng.pick(&["│   ", "│   ", "┃   ", "    ", "\t", "│ ", "│  ", "   "]));
            }
            let branch = rng.pick(&["├── ", "└── ", "├─ ", "┣━━ ", "┗━━ ", "├──", "└"]);
            out.push_str(&format!("{prefix}{branch}{text}\n"));
            if rng.chance(8) {
                out.push_str("│   │\n");
            }
        } else {
            out.push_str(&format!("{}{text}\n", " ".repeat(depth * unit)));
        }
        if directory || rng.chance(15) {
            depth += 1;
        } else if depth > 0 && rng.chance(35) {
            depth -= 1 + rng.below(depth);
        }
    }
    if rng.chance(6) {
        let at = rng.below(out.len() + 1);
        let at = (0..=at).rev().find(|&i| out.is_char_boundary(i)).unwrap_or(0);
        out.insert_str(at, rng.pick(&FRAGMENTS));
    }
    out
}
