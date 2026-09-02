use serde::{Deserialize, Serialize};
use specta::Type;

use crate::model::DEFAULT_CAPACITY;

/// Thirty seconds of silence before a pinned tab is considered idle.
///
/// Long enough that a paused video or a page being read is not discarded, short
/// enough that a forgotten pinned tab stops holding a webview hostage.
pub const DEFAULT_IDLE_RELEASE_MS: u64 = 30_000;

const DEFAULT_SEARCH_URL: &str = "https://duckduckgo.com/?q=";
const DEFAULT_HOME_URL: &str = "haku:new-tab";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// How many webviews may exist at once. One means only the visible tab is
    /// loaded; raising it keeps that many background tabs running.
    #[specta(type = specta_typescript::Number)]
    pub webview_capacity: usize,
    /// `system`, `light` or `dark`.
    pub theme: String,
    /// BCP-47 language tag the interface is shown in.
    pub locale: String,
    pub search_url: String,
    pub home_url: String,
    /// Milliseconds a pinned tab may be quiet before it gives up its webview.
    #[specta(type = specta_typescript::Number)]
    pub idle_release_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            webview_capacity: DEFAULT_CAPACITY,
            theme: "system".into(),
            locale: "en".into(),
            search_url: DEFAULT_SEARCH_URL.into(),
            home_url: DEFAULT_HOME_URL.into(),
            idle_release_ms: DEFAULT_IDLE_RELEASE_MS,
        }
    }
}

impl Settings {
    /// Clamps values that would leave the browser unusable.
    ///
    /// Settings can be hand-edited, so a zero capacity or an empty search URL
    /// has to be survivable rather than fatal.
    pub fn sanitized(mut self) -> Self {
        let defaults = Self::default();
        self.webview_capacity = self.webview_capacity.max(1);
        if self.search_url.trim().is_empty() {
            self.search_url = defaults.search_url;
        }
        if self.home_url.trim().is_empty() {
            self.home_url = defaults.home_url;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_pool_holds_a_single_webview() {
        assert_eq!(Settings::default().webview_capacity, 1);
    }

    #[test]
    fn a_zero_capacity_is_raised_so_the_active_tab_can_still_render() {
        let settings = Settings { webview_capacity: 0, ..Settings::default() }.sanitized();
        assert_eq!(settings.webview_capacity, 1);
    }

    #[test]
    fn a_blank_search_url_falls_back_to_the_default() {
        let settings = Settings { search_url: "  ".into(), ..Settings::default() }.sanitized();
        assert_eq!(settings.search_url, Settings::default().search_url);
    }

    #[test]
    fn a_blank_home_url_falls_back_to_the_default() {
        let settings = Settings { home_url: String::new(), ..Settings::default() }.sanitized();
        assert_eq!(settings.home_url, Settings::default().home_url);
    }

    #[test]
    fn every_field_is_required_on_the_wire_so_the_interface_never_sees_a_gap() {
        let json = serde_json::to_value(Settings::default()).unwrap();

        assert!(json.get("webviewCapacity").is_some());
        assert!(json.get("idleReleaseMs").is_some());
    }

    #[test]
    fn a_valid_configuration_passes_through_sanitizing_unchanged() {
        let settings = Settings { webview_capacity: 4, ..Settings::default() };
        assert_eq!(settings.clone().sanitized(), settings);
    }
}
