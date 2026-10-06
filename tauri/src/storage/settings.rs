use serde::{Deserialize, Serialize};
use specta::Type;

use crate::model::{Policy, Preset, DEFAULT_CAPACITY};

const DEFAULT_SEARCH_URL: &str = "https://duckduckgo.com/?q=";
const DEFAULT_HOME_URL: &str = "haku://new-tab";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// How many webviews may exist at once. One means only the visible tab is
    /// loaded; raising it keeps that many background tabs in memory. Ignored
    /// while every background tab is discarded.
    #[specta(type = specta_typescript::Number)]
    pub webview_capacity: usize,
    /// Whether background tabs still holding a webview are paused.
    pub freeze_tabs: Policy,
    /// Whether Haku discards background tabs before it has to.
    pub discard_tabs: Policy,
    /// `system`, `light` or `dark`.
    pub theme: String,
    /// BCP-47 language tag the interface is shown in.
    pub locale: String,
    pub search_url: String,
    pub home_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            webview_capacity: DEFAULT_CAPACITY,
            freeze_tabs: Policy::Smart,
            discard_tabs: Policy::Smart,
            theme: "system".into(),
            locale: "en".into(),
            search_url: DEFAULT_SEARCH_URL.into(),
            home_url: DEFAULT_HOME_URL.into(),
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

    /// The pool size these settings call for. Discarding every background tab
    /// leaves nothing for extra slots to hold, so the configured count is set
    /// aside rather than kept as idle webviews.
    pub fn pool_capacity(&self) -> usize {
        if self.discard_tabs == Policy::Always {
            1
        } else {
            self.webview_capacity
        }
    }

    /// Takes on a preset's values.
    ///
    /// @param total_memory - Installed memory in bytes, which sizes the pool.
    pub fn with_preset(mut self, preset: Preset, total_memory: Option<u64>) -> Self {
        let values = preset.values(total_memory);
        if let Some(slots) = values.slots {
            self.webview_capacity = slots;
        }
        if let Some(freeze) = values.freeze {
            self.freeze_tabs = freeze;
        }
        self.discard_tabs = values.discard;
        self
    }

    /// The preset these settings match, or nothing when they were customised.
    ///
    /// Derived rather than stored, so it can never name values that are not in
    /// effect.
    pub fn preset(&self, total_memory: Option<u64>) -> Option<Preset> {
        Preset::ALL
            .into_iter()
            .find(|preset| self.clone().with_preset(*preset, total_memory) == *self)
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
        let settings = Settings {
            webview_capacity: 0,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.webview_capacity, 1);
    }

    #[test]
    fn a_blank_search_url_falls_back_to_the_default() {
        let settings = Settings {
            search_url: "  ".into(),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.search_url, Settings::default().search_url);
    }

    #[test]
    fn a_blank_home_url_falls_back_to_the_default() {
        let settings = Settings {
            home_url: String::new(),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.home_url, Settings::default().home_url);
    }

    #[test]
    fn every_field_is_required_on_the_wire_so_the_interface_never_sees_a_gap() {
        let json = serde_json::to_value(Settings::default()).unwrap();

        assert!(json.get("webviewCapacity").is_some());
        assert!(json.get("homeUrl").is_some());
    }

    #[test]
    fn applying_a_preset_is_recognised_as_that_preset() {
        for preset in Preset::ALL {
            let settings = Settings::default().with_preset(preset, Some(16 << 30));
            assert_eq!(settings.preset(Some(16 << 30)), Some(preset));
        }
    }

    #[test]
    fn changing_a_value_a_preset_set_leaves_no_preset_matching() {
        let mut settings = Settings::default().with_preset(Preset::Balanced, Some(16 << 30));
        settings.freeze_tabs = Policy::Always;
        assert_eq!(settings.preset(Some(16 << 30)), None);
    }

    #[test]
    fn saving_memory_matches_whatever_slots_and_freezing_were_left_at() {
        let mut settings = Settings::default().with_preset(Preset::SaveMemory, None);
        settings.webview_capacity = 5;
        settings.freeze_tabs = Policy::Never;
        assert_eq!(settings.preset(None), Some(Preset::SaveMemory));
    }

    #[test]
    fn a_valid_configuration_passes_through_sanitizing_unchanged() {
        let settings = Settings {
            webview_capacity: 4,
            ..Settings::default()
        };
        assert_eq!(settings.clone().sanitized(), settings);
    }
}
