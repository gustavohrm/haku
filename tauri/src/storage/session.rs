use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::browser::{Browser, SPLIT_RATIOS};
use crate::model::{WindowId, WindowKind};
use crate::storage::settings::OPENABLE_SCHEMES;

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

/// An ordered tab pair, indexed into its window's stored tabs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct SessionSplit {
    pub tabs: [usize; 2],
    #[specta(type = specta_typescript::Number)]
    pub ratio: f64,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "read_splits")]
    pub splits: Vec<SessionSplit>,
}

fn read_splits<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<SessionSplit>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Array(entries) => entries
            .into_iter()
            .filter_map(|entry| serde_json::from_value(entry).ok())
            .collect(),
        _ => Vec::new(),
    })
}

impl SessionWindow {
    /// The window without the tabs Haku does not open, its active tab still
    /// active, or the one after it when it was left out. An index past the
    /// tabs is dropped, so restoring falls back to the first tab, as before.
    fn openable(self) -> Self {
        let mut next = 0;
        let indices: Vec<Option<usize>> = self
            .tabs
            .iter()
            .map(|tab| {
                opens(&tab.url).then(|| {
                    let index = next;
                    next += 1;
                    index
                })
            })
            .collect();
        let mut claimed = vec![false; self.tabs.len()];
        let splits = self
            .splits
            .into_iter()
            .filter_map(|pair| {
                let [left, right] = pair.tabs;
                if left.checked_add(1) != Some(right)
                    || right >= indices.len()
                    || !pair.ratio.is_finite()
                    || !SPLIT_RATIOS.contains(&pair.ratio)
                {
                    return None;
                }
                let tabs = [indices[left]?, indices[right]?];
                if claimed[left] || claimed[right] {
                    return None;
                }
                claimed[left] = true;
                claimed[right] = true;
                Some(SessionSplit {
                    tabs,
                    ratio: pair.ratio,
                })
            })
            .collect();
        let active = self
            .active
            .filter(|&active| active < self.tabs.len())
            .map(|active| self.tabs.iter().take(active).filter(|tab| opens(&tab.url)).count());
        let tabs: Vec<SessionTab> = self.tabs.into_iter().filter(|tab| opens(&tab.url)).collect();
        Self {
            active: active.map(|active| active.min(tabs.len().saturating_sub(1))),
            tabs,
            splits,
            ..self
        }
    }
}

fn opens(url: &str) -> bool {
    tauri::Url::parse(url).is_ok_and(|url| OPENABLE_SCHEMES.contains(&url.scheme()))
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
            .filter(|&window| browser.kind(window) == Some(WindowKind::Normal) && !browser.is_private(window))
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
                    splits: state
                        .splits
                        .iter()
                        .map(|pair| SessionSplit {
                            tabs: pair
                                .tabs
                                .map(|id| state.tabs.iter().position(|tab| tab.id == id).expect("pair member")),
                            ratio: pair.ratio,
                        })
                        .collect(),
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
    ///
    /// The file may have been edited by hand, so a tab whose address has no
    /// scheme Haku opens is left out, as a stored home page would be.
    pub fn windows_or_home(&self, home_url: &str) -> Vec<SessionWindow> {
        let windows: Vec<SessionWindow> = if self.windows.is_empty() {
            vec![SessionWindow {
                tabs: self.tabs.clone(),
                active: self.active,
                bounds: None,
                splits: Vec::new(),
            }]
        } else {
            self.windows.clone()
        };
        let windows: Vec<SessionWindow> = windows
            .into_iter()
            .map(SessionWindow::openable)
            .filter(|window| !window.tabs.is_empty())
            .collect();
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
            splits: Vec::new(),
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
                let splits = window.splits.iter().map(|pair| (pair.tabs, pair.ratio)).collect();
                (tabs, window.active, splits)
            }),
        );
        (browser, windows)
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
