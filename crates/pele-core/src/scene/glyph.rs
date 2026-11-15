//! The drawings Pele puts in an icon's box itself where no host draws the icon: the icons
//! Mermaid's architecture diagrams know without an icon pack (`architecture/icons.ts`).

/// One built-in icon. It is drawn on the icon box's 24 by 24 grid, stroked in the foreground
/// colour 1.5 wide, with the box's round caps and joins and no fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconGlyph {
    Database,
    Server,
    Disk,
    Internet,
    Cloud,
}

impl IconGlyph {
    /// Pele's `builtinIcon`: the glyph an icon name stands for, where it stands for one.
    pub(crate) fn named(name: &[u16]) -> Option<IconGlyph> {
        let glyph = match String::from_utf16_lossy(name).as_str() {
            "database" => IconGlyph::Database,
            "server" => IconGlyph::Server,
            "disk" => IconGlyph::Disk,
            "internet" => IconGlyph::Internet,
            "cloud" => IconGlyph::Cloud,
            _ => return None,
        };
        Some(glyph)
    }

    /// The outline as Pele writes it, in the grid's units.
    pub(crate) fn path_data(self) -> &'static str {
        match self {
            IconGlyph::Database => "M4 6a8 3 0 1 0 16 0a8 3 0 1 0-16 0M4 6v12a8 3 0 0 0 16 0V6M4 12a8 3 0 0 0 16 0",
            IconGlyph::Server => "M3 4h18v7H3zM3 13h18v7H3zM7 7.5h.01M7 16.5h.01M11 7.5h6M11 16.5h6",
            IconGlyph::Disk => "M5 3h14v18H5zM12 6.5a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 10.5h.01M8 18h3",
            IconGlyph::Internet => {
                "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M3 12h18M12 3a13 13 0 0 0 0 18M12 3a13 13 0 0 1 0 18"
            }
            IconGlyph::Cloud => "M7 18a4 4 0 0 1-.5-7.97A6 6 0 0 1 18 9.5a4.25 4.25 0 0 1-.5 8.5z",
        }
    }
}
