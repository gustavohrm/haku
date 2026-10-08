use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::browser::Browser;
use crate::model::{WindowId, WindowKind};

/// One tab as it is remembered across restarts.
///
/// Only what survives a reload is stored. A discarded tab and a live tab are
/// indistinguishable here, because on the next launch every tab starts
/// discarded anyway.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    pub fixed: bool,
}

/// Where a window was on screen, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    #[specta(type = specta_typescript::Number)]
    pub x: f64,
    #[specta(type = specta_typescript::Number)]
    pub y: f64,
    #[specta(type = specta_typescript::Number)]
    pub width: f64,
    #[specta(type = specta_typescript::Number)]
    pub height: f64,
    /// The window filled its screen. The rectangle is where it returns to
    /// when it stops.
    pub maximized: bool,
}

/// One browser window as it is remembered across restarts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionWindow {
    pub tabs: Vec<SessionTab>,
    /// Index into `tabs`. An index rather than an id, because ids are assigned
    /// fresh on every launch.
    pub active: Option<usize>,
    /// Nothing when the window was never placed, which reopens it maximized.
    #[serde(default)]
    pub bounds: Option<WindowBounds>,
}

/// The open windows, the one used most recently last.
///
/// Popups are not remembered: the page that opened one is reloaded on the next
/// launch, and the connection between them cannot come back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub windows: Vec<SessionWindow>,
    /// The one window's tabs, as a session was stored before Haku had
    /// windows. Read so that such a session is not lost, and never written.
    #[serde(default, skip_serializing)]
    pub tabs: Vec<SessionTab>,
    #[serde(default, skip_serializing)]
    pub active: Option<usize>,
}

impl Session {
    /// @param bounds - Where each window was last seen on screen.
    pub fn capture(browser: &Browser, bounds: &HashMap<WindowId, WindowBounds>) -> Self {
        let windows = browser
            .window_ids()
            .into_iter()
            .filter(|&window| browser.kind(window) == Some(WindowKind::Normal))
            .filter_map(|window| {
                let state = browser.state_in(window)?;
                let active = state
                    .active
                    .and_then(|active| state.tabs.iter().position(|tab| tab.id == active));
                let tabs = state
                    .tabs
                    .iter()
                    .map(|tab| SessionTab {
                        url: tab.url().to_string(),
                        title: tab.history.current().title.clone(),
                        fixed: tab.fixed,
                    })
                    .collect();
                Some(SessionWindow {
                    tabs,
                    active,
                    bounds: bounds.get(&window).copied(),
                })
            })
            .collect();

        Self {
            windows,
            ..Self::default()
        }
    }

    /// The windows to reopen, a session stored before windows existed read as
    /// one, and an empty one as a single window on the home page, so Haku never
    /// opens with nothing in it.
    pub fn windows_or_home(&self, home_url: &str) -> Vec<SessionWindow> {
        let windows: Vec<SessionWindow> = if self.windows.is_empty() {
            vec![SessionWindow {
                tabs: self.tabs.clone(),
                active: self.active,
                bounds: None,
            }]
        } else {
            self.windows.clone()
        };
        let windows: Vec<SessionWindow> = windows.into_iter().filter(|window| !window.tabs.is_empty()).collect();
        if !windows.is_empty() {
            return windows;
        }
        vec![SessionWindow {
            tabs: vec![SessionTab {
                url: home_url.to_string(),
                title: String::new(),
                fixed: false,
            }],
            active: Some(0),
            bounds: None,
        }]
    }

    /// Rebuilds a browser from a stored session.
    ///
    /// @returns The browser, and the windows it was rebuilt from, in the order
    ///   of [`Browser::window_ids`], for where each is put on screen.
    pub fn restore(&self, capacity: usize, home_url: &str) -> (Browser, Vec<SessionWindow>) {
        let windows = self.windows_or_home(home_url);
        let browser = Browser::restored(
            capacity,
            windows.iter().map(|window| {
                let tabs = window.tabs.iter().map(|tab| (tab.url.clone(), tab.fixed)).collect();
                (tabs, window.active)
            }),
        );
        (browser, windows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window_of(urls: &[&str], active: Option<usize>) -> SessionWindow {
        SessionWindow {
            tabs: urls
                .iter()
                .map(|url| SessionTab {
                    url: (*url).to_string(),
                    title: (*url).to_string(),
                    fixed: false,
                })
                .collect(),
            active,
            bounds: None,
        }
    }

    fn session_of(urls: &[&str], active: Option<usize>) -> Session {
        Session {
            windows: vec![window_of(urls, active)],
            ..Session::default()
        }
    }

    fn restore(session: &Session, capacity: usize) -> Browser {
        session.restore(capacity, "haku://new-tab").0
    }

    #[test]
    fn an_empty_session_restores_to_the_home_page() {
        let browser = restore(&Session::default(), 1);

        assert_eq!(browser.tabs().len(), 1);
        assert_eq!(browser.tabs()[0].url(), "haku://new-tab");
    }

    #[test]
    fn restoring_reopens_every_tab_in_order() {
        let browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(0)), 1);

        let urls: Vec<&str> = browser.tabs().iter().map(|tab| tab.url()).collect();
        assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
    }

    #[test]
    fn restoring_reactivates_the_tab_that_was_active() {
        let browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(1)), 1);

        assert_eq!(browser.active(), browser.tabs().get(1).map(|tab| tab.id));
    }

    #[test]
    fn an_out_of_range_active_index_falls_back_to_the_first_tab() {
        let browser = restore(&session_of(&["https://a.test"], Some(9)), 1);

        assert_eq!(browser.active(), browser.tabs().first().map(|tab| tab.id));
    }

    #[test]
    fn pinned_tabs_are_still_pinned_after_a_restart() {
        let mut session = session_of(&["https://a.test", "https://b.test"], Some(1));
        session.windows[0].tabs[0].fixed = true;

        let browser = restore(&session, 1);

        assert!(browser.tabs()[0].fixed);
    }

    #[test]
    fn a_restored_browser_holds_no_webviews_until_it_is_reconciled() {
        // Restoring through the ordinary open and select path would mark tabs
        // live and record which slot holds which URL, while producing effects
        // that nobody applies. Reconciling against the first reported layout
        // would then find nothing to do and the window would stay blank.
        let browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(0)), 2);

        assert_eq!(browser.state().live_count, 0);
        assert!(browser.tabs().iter().all(|tab| tab.slot().is_none()));
    }

    #[test]
    fn reconciling_a_restored_session_loads_the_active_tab() {
        let mut browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(1)), 1);

        let effects = browser.reconcile();

        assert!(effects.iter().any(|effect| matches!(
            effect,
            crate::browser::Effect::EnsureSlot { url, .. } if url == "https://b.test"
        )));
    }

    #[test]
    fn a_captured_session_restores_to_the_same_tabs() {
        let original = session_of(&["https://a.test", "https://b.test"], Some(1));
        let browser = restore(&original, 2);

        let captured = Session::capture(&browser, &HashMap::new());

        let urls: Vec<&str> = captured.windows[0].tabs.iter().map(|tab| tab.url.as_str()).collect();
        assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
        assert_eq!(captured.windows[0].active, Some(1));
    }

    #[test]
    fn every_window_is_restored_with_its_own_tabs_and_active_tab() {
        let session = Session {
            windows: vec![
                window_of(&["https://a.test", "https://b.test"], Some(1)),
                window_of(&["https://c.test"], Some(0)),
            ],
            ..Session::default()
        };

        let browser = restore(&session, 2);

        let windows = browser.window_ids();
        assert_eq!(windows.len(), 2);
        let first = browser.state_in(windows[0]).unwrap();
        let urls: Vec<&str> = first.tabs.iter().map(|tab| tab.url()).collect();
        assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
        assert_eq!(first.active, Some(first.tabs[1].id));
        assert_eq!(browser.state_in(windows[1]).unwrap().tabs[0].url(), "https://c.test");
    }

    #[test]
    fn the_window_used_last_is_the_one_in_use_after_a_restart() {
        let session = Session {
            windows: vec![
                window_of(&["https://a.test"], None),
                window_of(&["https://c.test"], None),
            ],
            ..Session::default()
        };

        let browser = restore(&session, 2);

        assert_eq!(browser.tab(browser.active().unwrap()).unwrap().url(), "https://c.test");
    }

    #[test]
    fn a_session_stored_before_windows_existed_restores_as_one_window() {
        let stored = r#"{ "tabs": [{ "url": "https://a.test", "title": "A", "fixed": false }], "active": 0 }"#;
        let session: Session = serde_json::from_str(stored).unwrap();

        let browser = restore(&session, 1);

        assert_eq!(browser.window_ids().len(), 1);
        assert_eq!(browser.tabs()[0].url(), "https://a.test");
    }

    #[test]
    fn the_old_shape_is_never_written_back() {
        let session = session_of(&["https://a.test"], Some(0));

        let written = serde_json::to_value(&session).unwrap();

        assert!(written.get("tabs").is_none());
        assert!(written.get("windows").is_some());
    }

    #[test]
    fn a_captured_session_keeps_where_each_window_was() {
        let browser = restore(&session_of(&["https://a.test"], Some(0)), 1);
        let bounds = WindowBounds {
            x: 10.0,
            y: 20.0,
            width: 800.0,
            height: 600.0,
            maximized: false,
        };

        let captured = Session::capture(&browser, &HashMap::from([(browser.main_window(), bounds)]));

        assert_eq!(captured.windows[0].bounds, Some(bounds));
    }

    #[test]
    fn popups_are_not_remembered() {
        let mut browser = restore(&session_of(&["https://a.test"], Some(0)), 2);
        browser.reconcile();
        let opener = browser.tab(browser.active().unwrap()).unwrap().slot().unwrap();
        browser.open_from(
            opener,
            "https://login.test",
            crate::browser::Opening::Popup {
                request: crate::model::WindowRequestId(1),
                placement: crate::model::Placement::default(),
            },
        );

        let captured = Session::capture(&browser, &HashMap::new());

        assert_eq!(captured.windows.len(), 1);
        assert_eq!(captured.windows[0].tabs.len(), 1);
    }
}
