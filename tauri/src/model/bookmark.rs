//! What a bookmark is, as the interface sees it.
//!
//! The tree lives in [`crate::storage::BookmarksDb`], which every change goes
//! through; this holds the shapes it reads out and the rules that need no
//! database. [Bookmarks](../../../docs/specs/bookmarks.md).

use serde::{Deserialize, Serialize};
use specta::Type;

/// Identifies one bookmark or folder for as long as it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
pub struct BookmarkId(#[specta(type = specta_typescript::Number)] pub i64);

/// A link or a folder, with what it holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkNode {
    pub id: BookmarkId,
    pub title: String,
    /// Milliseconds since the Unix epoch.
    #[specta(type = specta_typescript::Number)]
    pub added_at: i64,
    /// Milliseconds since the Unix epoch: when its title or URL last changed,
    /// or, for a folder, when what it holds last did.
    #[specta(type = specta_typescript::Number)]
    pub modified_at: i64,
    pub kind: BookmarkKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BookmarkKind {
    Link { url: String },
    Folder { children: Vec<BookmarkNode> },
}

/// The whole tree, under its two fixed roots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Bookmarks {
    /// What the bookmarks bar shows.
    pub bar: BookmarkNode,
    pub other: BookmarkNode,
}

/// What bookmarking a tab's page did.
/// [Bookmarks § Contract](../../../docs/specs/bookmarks.md#contract).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkedTab {
    /// The link added, or the page's most recently added bookmark.
    pub id: BookmarkId,
    /// Whether the link was added now.
    pub added: bool,
}

/// Schemes a bookmark may link to. A bookmark is followed by a click, so a
/// `javascript:` or `data:` one would run script or show content from
/// wherever the bookmark came from.
const SCHEMES: &[&str] = &["http://", "https://", "file://"];

/// Whether `url` may be the target of a bookmark: `http`, `https` or `file`,
/// with something after the scheme.
pub fn is_bookmarkable(url: &str) -> bool {
    SCHEMES.iter().any(|scheme| {
        url.get(..scheme.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
            && url.len() > scheme.len()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_and_file_addresses_are_bookmarkable() {
        assert!(is_bookmarkable("https://example.com/"));
        assert!(is_bookmarkable("http://example.com"));
        assert!(is_bookmarkable("HTTPS://EXAMPLE.COM"));
        assert!(is_bookmarkable("file:///C:/notes.html"));
    }

    #[test]
    fn script_data_internal_and_empty_addresses_are_not() {
        assert!(!is_bookmarkable("javascript:alert(1)"));
        assert!(!is_bookmarkable("data:text/html,hi"));
        assert!(!is_bookmarkable("haku://settings"));
        assert!(!is_bookmarkable("https://"));
        assert!(!is_bookmarkable("example.com"));
        assert!(!is_bookmarkable(""));
    }

    #[test]
    fn a_link_is_tagged_by_its_kind() {
        let link = BookmarkNode {
            id: BookmarkId(3),
            title: "A".into(),
            added_at: 1,
            modified_at: 2,
            kind: BookmarkKind::Link {
                url: "https://a.test/".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(&link).unwrap(),
            serde_json::json!({
                "id": 3, "title": "A", "addedAt": 1, "modifiedAt": 2,
                "kind": { "link": { "url": "https://a.test/" } }
            })
        );
    }
}
