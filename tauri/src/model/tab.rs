use serde::{Deserialize, Serialize};
use specta::Type;

use super::dialog::PageDialog;
use super::history::{History, Visit};
use super::pool::SlotId;

/// Scheme used by pages Haku renders itself, inside the chrome webview.
///
/// A tab on one of these never occupies a webview slot, which is why the pool
/// only ever competes over real web content.
pub const INTERNAL_SCHEME: &str = "haku://";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
pub struct TabId(#[specta(type = specta_typescript::Number)] pub u64);

/// Where a page was scrolled to.
///
/// Kept current for live tabs by the injected reporter so that discarding a tab
/// never has to ask a webview that may already be gone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct Scroll {
    #[specta(type = specta_typescript::Number)]
    pub x: f64,
    #[specta(type = specta_typescript::Number)]
    pub y: f64,
}

/// Whether a tab currently holds a webview.
///
/// A discarded tab is not a paused page: its webview is gone and reactivating
/// reloads the URL, then restores [`Tab::scroll`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum TabPresence {
    /// Bound to a pool slot and backed by a live webview.
    Live { slot: SlotId },
    /// No webview. Reactivating reloads the page.
    Discarded,
    /// Rendered by the chrome itself; never consumes a slot.
    Internal,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: TabId,
    pub history: History,
    pub presence: TabPresence,
    pub scroll: Scroll,
    /// The user asked this tab to stay resident. Fixed tabs raise the pool's
    /// effective capacity, so pinning can never starve the active tab.
    pub fixed: bool,
    /// When the tab was last shown, in milliseconds since the Unix epoch.
    #[specta(type = specta_typescript::Number)]
    pub active_at: u64,
    /// A dialog the page opened and is waiting on. Shown while the tab is
    /// active; a background tab's dialog waits until the tab is selected.
    pub dialog: Option<PageDialog>,
}

impl Tab {
    pub fn new(id: TabId, url: impl Into<String>) -> Self {
        let visit = Visit::new(url);
        let presence = if is_internal(&visit.url) { TabPresence::Internal } else { TabPresence::Discarded };

        Self {
            id,
            history: History::new(visit),
            presence,
            scroll: Scroll::default(),
            fixed: false,
            active_at: 0,
            dialog: None,
        }
    }

    pub fn url(&self) -> &str {
        &self.history.current().url
    }

    pub fn is_internal(&self) -> bool {
        matches!(self.presence, TabPresence::Internal)
    }

    pub fn slot(&self) -> Option<SlotId> {
        match self.presence {
            TabPresence::Live { slot } => Some(slot),
            _ => None,
        }
    }

    /// Reclassifies the tab after its URL changed, so navigating between an
    /// internal page and the web moves it in and out of the pool correctly.
    ///
    /// Leaving an internal page clears the scroll, because the position belonged
    /// to a page the tab is no longer showing.
    pub fn reclassify(&mut self) {
        let internal = is_internal(self.url());
        match (self.presence, internal) {
            (TabPresence::Internal, false) => {
                self.presence = TabPresence::Discarded;
                self.scroll = Scroll::default();
            }
            (_, true) => self.presence = TabPresence::Internal,
            _ => {}
        }
    }
}

pub fn is_internal(url: &str) -> bool {
    url.starts_with(INTERNAL_SCHEME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_web_tab_starts_discarded_because_it_holds_no_webview_yet() {
        let tab = Tab::new(TabId(1), "https://a.test");
        assert_eq!(tab.presence, TabPresence::Discarded);
        assert!(tab.slot().is_none());
    }

    #[test]
    fn a_tab_on_an_internal_page_never_needs_a_slot() {
        let tab = Tab::new(TabId(1), "haku://settings");
        assert!(tab.is_internal());
        assert!(tab.slot().is_none());
    }

    #[test]
    fn navigating_from_an_internal_page_to_the_web_makes_the_tab_poolable() {
        let mut tab = Tab::new(TabId(1), "haku://new-tab");
        tab.history.push(Visit::new("https://a.test"));
        tab.reclassify();

        assert_eq!(tab.presence, TabPresence::Discarded);
    }

    #[test]
    fn navigating_from_the_web_to_an_internal_page_releases_the_tab_from_the_pool() {
        let mut tab = Tab::new(TabId(1), "https://a.test");
        tab.presence = TabPresence::Live { slot: SlotId(0) };
        tab.history.push(Visit::new("haku://settings"));
        tab.reclassify();

        assert_eq!(tab.presence, TabPresence::Internal);
    }

    #[test]
    fn leaving_an_internal_page_discards_scroll_from_the_page_being_left() {
        let mut tab = Tab::new(TabId(1), "haku://history");
        tab.scroll = Scroll { x: 0.0, y: 900.0 };
        tab.history.push(Visit::new("https://a.test"));
        tab.reclassify();

        assert_eq!(tab.scroll, Scroll::default());
    }

    #[test]
    fn a_live_tab_stays_live_when_navigating_between_web_pages() {
        let mut tab = Tab::new(TabId(1), "https://a.test");
        tab.presence = TabPresence::Live { slot: SlotId(2) };
        tab.history.push(Visit::new("https://b.test"));
        tab.reclassify();

        assert_eq!(tab.presence, TabPresence::Live { slot: SlotId(2) });
    }
}
