//! A page's context menu, as WebView2 offers it and as Haku draws it.
//! [Context menus § Page menus](../../../docs/specs/context-menus.md#page-menus).

use serde::{Deserialize, Serialize};
use specta::Type;

/// Identifies one context menu a page asked for, so a late answer cannot
/// reach a newer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct ContextMenuRequestId(#[specta(type = specta_typescript::Number)] pub u64);

/// What was right-clicked, by the engine's reckoning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ContextMenuTargetKind {
    Page,
    Image,
    Selection,
    Audio,
    Video,
}

/// What was right-clicked, and what the menu's items act on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextMenuTarget {
    pub kind: ContextMenuTargetKind,
    pub page_url: String,
    pub link_url: Option<String>,
    pub link_text: Option<String>,
    /// The image or media.
    pub source_url: Option<String>,
    pub selection: Option<String>,
    pub editable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NativeMenuItemKind {
    Command,
    Checkbox,
    Radio,
    Separator,
    Submenu,
}

/// One of the items the engine would have shown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NativeMenuItem {
    /// The engine's `CommandId`, which the answer hands back.
    pub id: i32,
    /// Stable and unlocalized, such as `"copy"` or `"spellCheck"`.
    pub name: String,
    /// In the engine's language, with no accelerator marks.
    pub label: String,
    pub kind: NativeMenuItemKind,
    pub enabled: bool,
    pub checked: bool,
    pub shortcut: Option<String>,
    pub children: Vec<NativeMenuItem>,
}

/// A page's request for a context menu, as the platform reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageMenu {
    pub request: ContextMenuRequestId,
    /// Where the pointer was, in physical pixels from the webview's top left.
    pub x: i32,
    pub y: i32,
    pub target: ContextMenuTarget,
    /// The allowed items only, as [`allowed_items`] leaves them.
    pub items: Vec<NativeMenuItem>,
}

/// The native items Haku keeps, by name. An allowlist, so an item a future
/// engine adds stays hidden until someone decides on it.
///
/// Names are as WebView2 reported them in the app; its documentation lists
/// only examples. `spellCheck` and `extension` come from that documentation
/// and are not yet seen in the app.
const KEPT: &[&str] = &[
    "undo",
    "redo",
    "cut",
    "copy",
    "paste",
    "pasteAndMatchStyle",
    "selectAll",
    "spellCheck",
    "emoji",
    "copyImage",
    "copyImageLocation",
    "copyLinkLocation",
    "extension",
];

/// The items of `items` Haku keeps, in the engine's order.
///
/// A separator stays; the interface tidies the ones that end up doubled or at
/// an edge. A submenu stays when it is kept itself, with its kept children,
/// and goes when none of them are.
pub fn allowed_items(items: Vec<NativeMenuItem>) -> Vec<NativeMenuItem> {
    items
        .into_iter()
        .filter_map(|mut item| match item.kind {
            NativeMenuItemKind::Separator => Some(item),
            _ if !KEPT.contains(&item.name.as_str()) => None,
            NativeMenuItemKind::Submenu => {
                item.children = allowed_items(item.children);
                item.children
                    .iter()
                    .any(|child| child.kind != NativeMenuItemKind::Separator)
                    .then_some(item)
            }
            _ => Some(item),
        })
        .collect()
}

/// Whether `id` is an item of `items` that can be chosen: one that was
/// offered and enabled, and is neither a separator nor a submenu. An item
/// inside a disabled submenu cannot be reached, so it is not offered.
pub fn offers(items: &[NativeMenuItem], id: i32) -> bool {
    items.iter().any(|item| match item.kind {
        NativeMenuItemKind::Separator => false,
        NativeMenuItemKind::Submenu => item.enabled && offers(&item.children, id),
        _ => item.enabled && item.id == id,
    })
}

/// `label` without its accelerator marks: a lone `&` goes, and `&&` stands
/// for a literal `&`.
pub fn plain_label(label: &str) -> String {
    let mut plain = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                plain.push('&');
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i32, name: &str, kind: NativeMenuItemKind) -> NativeMenuItem {
        NativeMenuItem {
            id,
            name: name.into(),
            label: name.into(),
            kind,
            enabled: true,
            checked: false,
            shortcut: None,
            children: Vec::new(),
        }
    }

    fn command(id: i32, name: &str) -> NativeMenuItem {
        item(id, name, NativeMenuItemKind::Command)
    }

    fn names(items: &[NativeMenuItem]) -> Vec<&str> {
        items.iter().map(|item| item.name.as_str()).collect()
    }

    #[test]
    fn keeps_allowed_items_in_order_and_drops_the_rest() {
        let items = vec![
            command(1, "back"),
            command(2, "cut"),
            command(3, "saveAs"),
            command(4, "copy"),
            command(5, "inspectElement"),
        ];
        assert_eq!(names(&allowed_items(items)), ["cut", "copy"]);
    }

    #[test]
    fn keeps_separators_for_the_interface_to_tidy() {
        let items = vec![
            command(1, "copy"),
            item(0, "separator", NativeMenuItemKind::Separator),
            command(2, "print"),
        ];
        assert_eq!(names(&allowed_items(items)), ["copy", "separator"]);
    }

    #[test]
    fn filters_inside_a_kept_submenu() {
        let mut submenu = item(10, "extension", NativeMenuItemKind::Submenu);
        submenu.children = vec![command(11, "extension"), command(12, "share")];
        let kept = allowed_items(vec![submenu]);
        assert_eq!(names(&kept[0].children), ["extension"]);
    }

    #[test]
    fn drops_a_submenu_left_with_nothing_to_choose() {
        let mut submenu = item(10, "spellCheck", NativeMenuItemKind::Submenu);
        submenu.children = vec![
            item(0, "separator", NativeMenuItemKind::Separator),
            command(12, "readAloud"),
        ];
        assert!(allowed_items(vec![submenu]).is_empty());
    }

    #[test]
    fn drops_a_submenu_that_is_not_kept_even_with_kept_children() {
        let mut submenu = item(10, "webCapture", NativeMenuItemKind::Submenu);
        submenu.children = vec![command(11, "copy")];
        assert!(allowed_items(vec![submenu]).is_empty());
    }

    #[test]
    fn offers_only_enabled_choosable_items_at_any_depth() {
        let mut disabled = command(2, "paste");
        disabled.enabled = false;
        let mut submenu = item(10, "extension", NativeMenuItemKind::Submenu);
        submenu.children = vec![command(11, "extension")];
        let items = vec![command(1, "copy"), disabled, submenu];
        assert!(offers(&items, 1));
        assert!(offers(&items, 11));
        assert!(!offers(&items, 2), "disabled");
        assert!(!offers(&items, 10), "a submenu is not a choice");
        assert!(!offers(&items, 99), "never offered");
    }

    #[test]
    fn offers_nothing_inside_a_disabled_submenu() {
        let mut submenu = item(10, "extension", NativeMenuItemKind::Submenu);
        submenu.enabled = false;
        submenu.children = vec![command(11, "extension")];
        assert!(!offers(&[submenu], 11));
    }

    #[test]
    fn strips_accelerator_marks() {
        assert_eq!(plain_label("&Copy"), "Copy");
        assert_eq!(plain_label("Save &as"), "Save as");
        assert_eq!(plain_label("Tom && Jerry"), "Tom & Jerry");
        assert_eq!(plain_label("Plain"), "Plain");
    }
}
