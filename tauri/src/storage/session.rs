use serde::{Deserialize, Serialize};
use specta::Type;

use crate::browser::Browser;

/// One tab as it is remembered across restarts.
///
/// Only what survives a reload is stored. A suspended tab and a live tab are
/// indistinguishable here, because on the next launch every tab starts
/// suspended anyway.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    pub fixed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    /// Index into `tabs`. An index rather than an id, because ids are assigned
    /// fresh on every launch.
    pub active: Option<usize>,
}

impl Session {
    pub fn capture(browser: &Browser) -> Self {
        let tabs: Vec<SessionTab> = browser
            .tabs()
            .iter()
            .map(|tab| SessionTab {
                url: tab.url().to_string(),
                title: tab.history.current().title.clone(),
                fixed: tab.fixed,
            })
            .collect();

        let active = browser
            .active()
            .and_then(|active| browser.tabs().iter().position(|tab| tab.id == active));

        Self { tabs, active }
    }

    /// Rebuilds a browser from a stored session.
    ///
    /// An empty session opens the home page, so the window is never restored
    /// with nothing in it.
    pub fn restore(&self, capacity: usize, home_url: &str) -> Browser {
        if self.tabs.is_empty() {
            return Browser::restored(capacity, [(home_url.to_string(), false)], Some(0));
        }

        let tabs = self.tabs.iter().map(|tab| (tab.url.clone(), tab.fixed));
        Browser::restored(capacity, tabs, self.active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_of(urls: &[&str], active: Option<usize>) -> Session {
        Session {
            tabs: urls
                .iter()
                .map(|url| SessionTab { url: (*url).to_string(), title: (*url).to_string(), fixed: false })
                .collect(),
            active,
        }
    }

    #[test]
    fn an_empty_session_restores_to_the_home_page() {
        let browser = Session::default().restore(1, "haku:new-tab");

        assert_eq!(browser.tabs().len(), 1);
        assert_eq!(browser.tabs()[0].url(), "haku:new-tab");
    }

    #[test]
    fn restoring_reopens_every_tab_in_order() {
        let browser = session_of(&["https://a.test", "https://b.test"], Some(0)).restore(1, "haku:new-tab");

        let urls: Vec<&str> = browser.tabs().iter().map(|tab| tab.url()).collect();
        assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
    }

    #[test]
    fn restoring_reactivates_the_tab_that_was_active() {
        let browser = session_of(&["https://a.test", "https://b.test"], Some(1)).restore(1, "haku:new-tab");

        assert_eq!(browser.active(), browser.tabs().get(1).map(|tab| tab.id));
    }

    #[test]
    fn an_out_of_range_active_index_falls_back_to_the_first_tab() {
        let browser = session_of(&["https://a.test"], Some(9)).restore(1, "haku:new-tab");

        assert_eq!(browser.active(), browser.tabs().first().map(|tab| tab.id));
    }

    #[test]
    fn pinned_tabs_are_still_pinned_after_a_restart() {
        let mut session = session_of(&["https://a.test", "https://b.test"], Some(1));
        session.tabs[0].fixed = true;

        let browser = session.restore(1, "haku:new-tab");

        assert!(browser.tabs()[0].fixed);
    }

    #[test]
    fn a_restored_browser_holds_no_webviews_until_it_is_reconciled() {
        // Restoring through the ordinary open and select path would mark tabs
        // live and record which slot holds which URL, while producing effects
        // that nobody applies. Reconciling against the first reported layout
        // would then find nothing to do and the window would stay blank.
        let browser = session_of(&["https://a.test", "https://b.test"], Some(0)).restore(2, "haku:new-tab");

        assert_eq!(browser.state().live_count, 0);
        assert!(browser.tabs().iter().all(|tab| tab.slot().is_none()));
    }

    #[test]
    fn reconciling_a_restored_session_loads_the_active_tab() {
        let mut browser = session_of(&["https://a.test", "https://b.test"], Some(1)).restore(1, "haku:new-tab");

        let effects = browser.reconcile();

        assert!(effects.iter().any(|effect| matches!(
            effect,
            crate::browser::Effect::EnsureSlot { url, .. } if url == "https://b.test"
        )));
    }

    #[test]
    fn a_captured_session_restores_to_the_same_tabs() {
        let original = session_of(&["https://a.test", "https://b.test"], Some(1));
        let browser = original.restore(2, "haku:new-tab");

        let captured = Session::capture(&browser);

        let urls: Vec<&str> = captured.tabs.iter().map(|tab| tab.url.as_str()).collect();
        assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
        assert_eq!(captured.active, Some(1));
    }
}
