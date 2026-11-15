//! Which icon a tree entry shows, ported from Pele's `treeview/icons.ts`. Only `file` and
//! `folder` are built in; every other icon name is offered to the host.

use super::model::{NodeType, TreeNode};
use crate::input::{JsObject, JsText, JsValue, Units, to_lowercase};
use crate::scene::IconGlyph;

/// Mermaid's name for its built-in pack.
pub(super) const BUILTIN_PACK: &str = "mermaid-treeview";

/// Pele's `GLYPHS`, by name.
pub(super) fn glyph(name: &[u16]) -> Option<IconGlyph> {
    if name.is_str("folder") {
        Some(IconGlyph::Folder)
    } else if name.is_str("file") {
        Some(IconGlyph::File)
    } else {
        None
    }
}

/// Pele's `IconConfig`, from the `treeView` section of the config.
pub(super) struct IconConfig<'a> {
    show_icons: bool,
    default_pack: JsText,
    filename_icons: Option<&'a JsObject>,
    extension_icons: Option<&'a JsObject>,
}

impl<'a> IconConfig<'a> {
    /// Pele's `iconConfig`.
    pub(super) fn of(section: Option<&'a JsObject>) -> Self {
        let get = |key: &str| section.and_then(|section| section.get(key));
        let map = |key: &str| get(key).and_then(JsValue::as_record);
        IconConfig {
            show_icons: get("showIcons").is_some_and(JsValue::truthy),
            default_pack: match get("defaultIconPack") {
                Some(JsValue::String(pack)) => pack.clone(),
                _ => JsText::default(),
            },
            filename_icons: map("filenameIcons"),
            extension_icons: map("extensionIcons"),
        }
    }

    /// Pele's `detectIcon`: a file name wins over an extension, and extensions match in lower
    /// case with or without the leading dot.
    fn detect(&self, name: &[u16]) -> Option<JsText> {
        let own = |map: Option<&JsObject>, key: &[u16]| match map?.iter().find(|(name, _)| name[..] == *key) {
            Some((_, JsValue::String(value))) => Some(value.clone()),
            _ => None,
        };
        if let Some(found) = own(self.filename_icons, name).filter(|found| !found.is_empty()) {
            return Some(found);
        }
        let dot = name.iter().rposition(|&c| c == u16::from(b'.')).filter(|&dot| dot > 0)?;
        let extension = to_lowercase(&name[dot..]);
        own(self.extension_icons, &extension).or_else(|| own(self.extension_icons, &extension[1..]))
    }

    fn qualify(&self, icon: &[u16]) -> JsText {
        if icon.contains(&u16::from(b':')) {
            return icon.into();
        }
        let mut out = if glyph(icon).is_some() || self.default_pack.is_empty() {
            JsText::from(BUILTIN_PACK)
        } else {
            self.default_pack.clone()
        };
        out.push_str(":");
        out.push_units(icon);
        out
    }

    /// Pele's `getNodeIcon`: the qualified icon name for a node, or `None` when it shows none.
    /// An `icon()` annotation always wins; otherwise icons appear only with `showIcons`.
    pub(super) fn icon(&self, node: &TreeNode) -> Option<JsText> {
        match node.icon.as_deref() {
            Some(icon) if icon.is_str("none") => return None,
            Some(icon) if !icon.is_empty() => return Some(self.qualify(icon)),
            _ => {}
        }
        if !self.show_icons {
            return None;
        }
        if node.node_type == NodeType::File
            && let Some(detected) = self.detect(&node.name)
        {
            if detected.is_str("none") {
                return None;
            }
            if !detected.is_empty() {
                return Some(self.qualify(&detected));
            }
        }
        let builtin = if node.node_type == NodeType::Directory { "folder" } else { "file" };
        Some(format!("{BUILTIN_PACK}:{builtin}").as_str().into())
    }
}
