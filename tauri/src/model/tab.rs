use serde::{Deserialize, Serialize};
use specta::Type;

use super::dialog::PageDialog;
use super::history::{History, Visit};
use super::page_state::{Loss, LossSignal, PageRecord, PageState, DRAFT_LIMIT};
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

/// Whether a tab currently holds a webview, and whether it is running.
///
/// A discarded tab is not a paused page: its webview is gone and reactivating
/// reloads the URL, then restores [`Tab::scroll`]. A frozen tab is: it keeps
/// its page and resumes without a reload.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum TabPresence {
    /// Bound to a pool slot and backed by a live webview.
    Live { slot: SlotId },
    /// Bound to a pool slot, with its page paused in the background.
    Frozen { slot: SlotId },
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
    /// Recency is how Haku judges whether a tab is likely to be shown again.
    #[specta(type = specta_typescript::Number)]
    pub active_at: u64,
    /// A dialog the page opened and is waiting on. Shown while the tab is
    /// active; a background tab's dialog waits until the tab is selected.
    pub dialog: Option<PageDialog>,
    /// The page is playing audio, which a smart policy will not interrupt.
    pub audible: bool,
    /// Discarded to free memory while the system was short of it, and not
    /// loaded since.
    pub relieved: bool,
    /// The tab's page is loading into a slot after the tab held none, and its
    /// document has not loaded yet. The interface covers the page meanwhile,
    /// so whatever the slot showed before is never seen.
    pub restoring: bool,
    /// What the user typed into the page's forms, to put back when a
    /// discarded tab reloads. Never sent to the interface or written to disk.
    #[serde(skip)]
    pub draft: Option<String>,
    /// What the page would lose if discarded, as last read. Rust's alone.
    #[serde(skip)]
    pub page: PageRecord,
}

impl Tab {
    pub fn new(id: TabId, url: impl Into<String>) -> Self {
        let visit = Visit::new(url);
        let presence = if is_internal(&visit.url) {
            TabPresence::Internal
        } else {
            TabPresence::Discarded
        };

        Self {
            id,
            history: History::new(visit),
            presence,
            scroll: Scroll::default(),
            fixed: false,
            active_at: 0,
            dialog: None,
            audible: false,
            relieved: false,
            restoring: false,
            draft: None,
            page: PageRecord::default(),
        }
    }

    /// Forgets what belonged to the page in the current history entry, as
    /// the tab moves to another one.
    ///
    /// Whether the page is a form result is left alone: that is decided when
    /// a navigation starts, before the entry it lands on is committed.
    pub fn leave_entry(&mut self) {
        self.scroll = Scroll::default();
        self.draft = None;
        self.page.state = None;
        self.page.unreadable = false;
    }

    /// Records a reading of the page.
    ///
    /// A reading taken on another URL than the tab's current one arrived after
    /// the tab moved on, and only says the page could be read.
    ///
    /// @param state - The reading, or nothing when the page could not be read.
    pub fn record_state(&mut self, state: Option<PageState>) {
        let Some(mut state) = state else {
            self.page.unreadable = true;
            return;
        };
        self.page.unreadable = false;
        if state.url != self.url() {
            return;
        }
        self.scroll = state.scroll;
        self.draft = state.draft.take().filter(|draft| draft.len() <= DRAFT_LIMIT);
        self.page.state = Some(state);
    }

    pub fn url(&self) -> &str {
        &self.history.current().url
    }

    /// The reasons discarding the tab would cost something.
    ///
    /// @param kept_sites - Hosts the user asked not to unload, whose tabs are
    ///   treated as holding work whatever the page reports.
    pub fn loss_signals(&self, kept_sites: &[String]) -> Vec<LossSignal> {
        let kept = host_of(self.url()).is_some_and(|host| kept_sites.contains(&host));
        let mut signals: Vec<LossSignal> = kept.then_some(LossSignal::KeptSite).into_iter().collect();
        signals.extend(self.page.signals());
        signals
    }

    /// What discarding the tab would cost: the highest level of its signals.
    pub fn loss(&self, kept_sites: &[String]) -> Loss {
        self.loss_signals(kept_sites)
            .into_iter()
            .map(LossSignal::level)
            .max()
            .unwrap_or_default()
    }

    pub fn is_internal(&self) -> bool {
        matches!(self.presence, TabPresence::Internal)
    }

    pub fn slot(&self) -> Option<SlotId> {
        match self.presence {
            TabPresence::Live { slot } | TabPresence::Frozen { slot } => Some(slot),
            _ => None,
        }
    }

    /// Marks the tab discarded once its page is gone.
    ///
    /// Whatever the page was playing went with it. The engine reports the
    /// silence on the slot, which by then may belong to another tab.
    pub fn lose_page(&mut self) {
        self.presence = TabPresence::Discarded;
        self.audible = false;
        self.restoring = false;
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

/// A web page's host name, lowercased and without a port or credentials, as
/// kept sites are listed and as the interface's `URL.hostname` gives it, an
/// IPv6 address keeping its brackets. Nothing for anything but an `http` or
/// `https` URL.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    // A bracketed IPv6 address keeps its colons; anything else loses its port.
    let host = match host.find(']') {
        Some(end) if host.starts_with('[') => &host[..=end],
        _ => host.split(':').next()?,
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
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
    fn a_host_is_read_without_port_credentials_or_case() {
        assert_eq!(
            host_of("https://Mail.Example.com/inbox"),
            Some("mail.example.com".into())
        );
        assert_eq!(host_of("http://user:pw@a.test:8080/?q=1"), Some("a.test".into()));
        assert_eq!(host_of("http://[::1]:8765/"), Some("[::1]".into()));
        assert_eq!(host_of("https://a.test#top"), Some("a.test".into()));
        assert_eq!(host_of("haku://settings"), None);
        assert_eq!(host_of("about:blank"), None);
    }

    #[test]
    fn a_tab_on_a_kept_site_would_lose_work_whatever_the_page_reports() {
        let tab = Tab::new(TabId(1), "https://mail.example.com/inbox");
        let kept = vec!["mail.example.com".to_string()];

        assert_eq!(tab.loss(&kept), Loss::Work);
        assert_eq!(tab.loss_signals(&kept), vec![LossSignal::KeptSite]);
        assert_eq!(tab.loss(&[]), Loss::None);
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
