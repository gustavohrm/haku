use serde::{Deserialize, Serialize};
use specta::Type;

use crate::model::{Policy, Preset, DEFAULT_CAPACITY};

const DEFAULT_SEARCH_URL: &str = "https://duckduckgo.com/?q=";
const DEFAULT_HOME_URL: &str = "haku://new-tab";
const DEFAULT_KEPT_MEMORY_MB: u32 = 512;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// How many webviews may exist at once. One means only the visible tab is
    /// loaded; raising it keeps that many background tabs in memory. Ignored
    /// while every background tab is discarded.
    #[specta(type = specta_typescript::Number)]
    pub webview_capacity: usize,
    /// The most memory background tabs kept for what they would lose may
    /// hold together, in megabytes.
    pub kept_memory_mb: u32,
    /// Whether background tabs still holding a webview are paused.
    pub freeze_tabs: Policy,
    /// Whether Haku discards background tabs before it has to.
    pub discard_tabs: Policy,
    /// Hosts whose tabs are kept as if they held unsaved work, whatever the
    /// page reports.
    pub kept_sites: Vec<String>,
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
            kept_memory_mb: DEFAULT_KEPT_MEMORY_MB,
            freeze_tabs: Policy::Smart,
            discard_tabs: Policy::Smart,
            kept_sites: Vec::new(),
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
        // Hosts are compared as `host_of` gives them, so a hand-edited entry
        // in another case or with stray spaces still matches.
        let mut sites: Vec<String> = Vec::new();
        for site in self.kept_sites.drain(..) {
            let site = site.trim().to_ascii_lowercase();
            if !site.is_empty() && !sites.contains(&site) {
                sites.push(site);
            }
        }
        self.kept_sites = sites;
        self
    }

    /// The budget for kept tabs, in bytes.
    pub fn kept_memory_bytes(&self) -> u64 {
        u64::from(self.kept_memory_mb) * 1024 * 1024
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
    pub fn with_preset(mut self, preset: Preset) -> Self {
        let values = preset.values();
        if let Some(slots) = values.slots {
            self.webview_capacity = slots;
        }
        if let Some(kept) = values.kept_memory_mb {
            self.kept_memory_mb = kept;
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
    pub fn preset(&self) -> Option<Preset> {
        Preset::ALL
            .into_iter()
            .find(|preset| self.clone().with_preset(*preset) == *self)
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
            let settings = Settings::default().with_preset(preset);
            assert_eq!(settings.preset(), Some(preset));
        }
    }

    #[test]
    fn changing_a_value_a_preset_set_leaves_no_preset_matching() {
        let mut settings = Settings::default().with_preset(Preset::Balanced);
        settings.kept_memory_mb = 2048;
        assert_eq!(settings.preset(), None);
    }

    #[test]
    fn saving_memory_matches_whatever_slots_and_freezing_were_left_at() {
        let mut settings = Settings::default().with_preset(Preset::SaveMemory);
        settings.webview_capacity = 5;
        settings.kept_memory_mb = 64;
        settings.freeze_tabs = Policy::Never;
        assert_eq!(settings.preset(), Some(Preset::SaveMemory));
    }

    #[test]
    fn settings_saved_under_the_old_performance_preset_show_as_custom() {
        let settings = Settings {
            webview_capacity: 6,
            freeze_tabs: Policy::Smart,
            discard_tabs: Policy::Never,
            ..Settings::default()
        };
        assert_eq!(settings.preset(), None);
    }

    #[test]
    fn kept_sites_are_trimmed_lowercased_and_listed_once() {
        let settings = Settings {
            kept_sites: vec![" Mail.Example.com ".into(), "mail.example.com".into(), "  ".into()],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.kept_sites, vec!["mail.example.com".to_string()]);
    }

    #[test]
    fn a_settings_file_from_before_the_budget_gets_the_defaults() {
        let stored: Settings = serde_json::from_value(crate::storage::merge::merge(
            serde_json::to_value(Settings::default()).unwrap(),
            serde_json::json!({ "webviewCapacity": 3 }),
        ))
        .unwrap();
        assert_eq!(stored.kept_memory_mb, 512);
        assert!(stored.kept_sites.is_empty());
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
