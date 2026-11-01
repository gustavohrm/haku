use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::model::keymap::{defaults_of, DEFAULTS};
use crate::model::permission::{normalized, SitePermissions};
use crate::model::{Chord, Keymap, Policy, Preset, Shortcut, DEFAULT_CAPACITY};

const DEFAULT_SEARCH_URL: &str = "https://duckduckgo.com/?q=";
const DEFAULT_HOME_URL: &str = "haku://new-tab";
const DEFAULT_KEPT_MEMORY_MB: u32 = 512;
/// Schemes Haku opens from a stored address, as a home page or a restored tab.
/// A home page typed with anything else is read as a bare host.
pub const OPENABLE_SCHEMES: &[&str] = &["http", "https", "file", "about", "haku", "chrome-extension"];
/// Schemes a search URL may use.
const SEARCH_SCHEMES: &[&str] = &["http", "https"];
const HTTPS_PREFIX: &str = "https://";

/// Where the tabs are drawn.
/// [Appearance § Tabs](../../../docs/specs/appearance.md#tabs).
///
/// Read leniently, as [`known_or_default`] explains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TabLayout {
    /// A strip across the top of the window.
    #[default]
    Horizontal,
    /// A column down the left side of the window.
    Vertical,
}

/// Whether the bars take space from the page or sit over it.
/// [Appearance § Bars](../../../docs/specs/appearance.md#bars).
///
/// Read leniently, as [`known_or_default`] explains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BarLayout {
    /// Beside the page, which is sized to the space they leave.
    #[default]
    Docked,
    /// Over the page, shown while the pointer is at the window's edge or
    /// something in them is needed.
    Floating,
}

/// Which side of the window the tab column is on.
/// [Appearance § Sidebar](../../../docs/specs/appearance.md#sidebar).
///
/// Read leniently, as [`known_or_default`] explains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SidebarSide {
    /// Down the left side of the window.
    #[default]
    Left,
    /// Down the right side of the window.
    Right,
}

/// Where the toolbar goes when the tabs are in a column.
/// [Appearance § Sidebar](../../../docs/specs/appearance.md#sidebar).
///
/// Read leniently, as [`known_or_default`] explains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ToolbarPlacement {
    /// A row across the top of the window.
    #[default]
    Top,
    /// At the head of the tab column, which then runs the window's full height.
    Sidebar,
}

/// The tab column's width, in CSS pixels, before the user resizes it.
pub const DEFAULT_SIDEBAR_WIDTH: u32 = 240;
/// The narrowest the tab column may be, in CSS pixels.
pub const MIN_SIDEBAR_WIDTH: u32 = 224;
/// The widest the tab column may be, in CSS pixels.
pub const MAX_SIDEBAR_WIDTH: u32 = 480;

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
    pub tab_layout: TabLayout,
    pub bar_layout: BarLayout,
    pub sidebar_side: SidebarSide,
    pub toolbar_placement: ToolbarPlacement,
    /// The tab column's width in CSS pixels, between [`MIN_SIDEBAR_WIDTH`]
    /// and [`MAX_SIDEBAR_WIDTH`].
    pub sidebar_width: u32,
    /// BCP-47 language tag the interface is shown in.
    pub locale: String,
    pub search_url: String,
    pub home_url: String,
    /// Ids of installed extensions the user switched off. Extensions are
    /// installed by placing them in the extensions folder, so this holds only
    /// the exceptions, and every new extension starts enabled.
    pub disabled_extensions: Vec<String>,
    /// Ids of extensions with a button of their own in the toolbar, in the
    /// order they appear there. The rest are reached from the extensions
    /// menu.
    pub pinned_extensions: Vec<String>,
    /// What the user allowed or blocked sites to use, by origin and kind.
    /// [Permissions](../../../docs/specs/permissions.md).
    pub site_permissions: SitePermissions,
    /// The chords of the shortcuts the user changed, by action.
    /// [Shortcuts § Changing bindings](../../../docs/specs/shortcuts.md#changing-bindings).
    pub shortcuts: Shortcuts,
}

/// The chords of the shortcuts the user changed, written as
/// [`Chord`]'s `Display` writes them. An action left out keeps its defaults;
/// an empty list leaves it unbound.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(transparent)]
#[specta(inline)]
pub struct Shortcuts(pub BTreeMap<Shortcut, Vec<String>>);

/// Drops an action since removed and a value that is not a list of text, as
/// [`SitePermissions`] does, rather than failing the whole settings file.
impl<'de> Deserialize<'de> for Shortcuts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let serde_json::Value::Object(entries) = serde_json::Value::deserialize(deserializer)? else {
            return Ok(Self::default());
        };
        Ok(Self(
            entries
                .into_iter()
                .filter_map(|(action, chords)| {
                    let action = serde_json::from_value(serde_json::Value::String(action)).ok()?;
                    Some((action, serde_json::from_value(chords).ok()?))
                })
                .collect(),
        ))
    }
}

impl Shortcuts {
    /// Rewrites every chord in one form, and drops those that do not read,
    /// are refused, or are already listed, under this action or one before it
    /// in table order. An action left with its defaults is dropped.
    fn normalized(self) -> Self {
        let mut taken = HashSet::new();
        let mut kept = BTreeMap::new();
        for (shortcut, defaults) in DEFAULTS {
            let Some(chords) = self.0.get(shortcut) else {
                continue;
            };
            let chords: Vec<Chord> = chords
                .iter()
                .filter_map(|chord| Chord::parse(chord))
                .filter(|chord| chord.refusal().is_none() && taken.insert(*chord))
                .collect();
            if chords != *defaults {
                kept.insert(*shortcut, chords.iter().map(Chord::to_string).collect());
            }
        }
        Self(kept)
    }

    /// The keymap these changes make of the defaults.
    pub fn keymap(&self) -> Keymap {
        let changed = self
            .0
            .iter()
            .map(|(shortcut, chords)| {
                (
                    *shortcut,
                    chords.iter().filter_map(|chord| Chord::parse(chord)).collect(),
                )
            })
            .collect();
        Keymap::new(&changed)
    }
}

/// An action's chords as the interface lists them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyBinding {
    pub shortcut: Shortcut,
    /// What is bound now, in the order the user gave them.
    pub chords: Vec<String>,
    pub defaults: Vec<String>,
}

/// Every action's chords, in table order.
pub fn key_bindings(keymap: &Keymap) -> Vec<KeyBinding> {
    let written = |chords: &[Chord]| chords.iter().map(Chord::to_string).collect();
    keymap
        .bindings()
        .map(|(shortcut, chords)| KeyBinding {
            shortcut,
            chords: written(chords),
            defaults: written(defaults_of(shortcut)),
        })
        .collect()
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
            tab_layout: TabLayout::default(),
            bar_layout: BarLayout::default(),
            sidebar_side: SidebarSide::default(),
            toolbar_placement: ToolbarPlacement::default(),
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            locale: "en".into(),
            search_url: DEFAULT_SEARCH_URL.into(),
            home_url: DEFAULT_HOME_URL.into(),
            disabled_extensions: Vec::new(),
            pinned_extensions: Vec::new(),
            site_permissions: SitePermissions::default(),
            shortcuts: Shortcuts::default(),
        }
    }
}

impl Settings {
    /// Clamps values that would leave the browser unusable.
    ///
    /// Settings can be hand-edited, so a zero capacity or a search URL that is
    /// not a web address has to be survivable rather than fatal.
    pub fn sanitized(mut self) -> Self {
        self.webview_capacity = self.webview_capacity.max(1);
        self.sidebar_width = self.sidebar_width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        self.search_url = normalized_search_url(&self.search_url);
        self.home_url = normalized_home_url(&self.home_url);
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
        self.site_permissions = SitePermissions(normalized(std::mem::take(&mut self.site_permissions.0)));
        self.shortcuts = std::mem::take(&mut self.shortcuts).normalized();
        self
    }

    /// Whether `next` keeps a site these settings do not. Removing one is not
    /// keeping one, so the settings page may still do it from a private window.
    pub fn keeps_new_site(&self, next: &Self) -> bool {
        next.kept_sites.iter().any(|site| !self.kept_sites.contains(site))
    }

    /// Whether `next` stores a permission decision these settings do not.
    /// Removing one is not storing one, as for kept sites.
    pub fn adds_site_permission(&self, next: &Self) -> bool {
        next.site_permissions
            .iter()
            .any(|permission| !self.site_permissions.contains(permission))
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

/// Reads one of `choices`, or the default when the value is none of them.
///
/// A stored document that fails to deserialize is set aside whole by
/// `read_json`, so a strict read would trade every setting for one hand-edited
/// or newer value. The command boundary reads leniently too, which is
/// harmless: the interface only sends values the bindings allow. Written by
/// hand rather than with `deserialize_with`, which splits `Settings` into a
/// read and a written type in the bindings.
fn known_or_default<'de, D, T>(deserializer: D, choices: &[T]) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Serialize + Default + Copy,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(choices
        .iter()
        .copied()
        .find(|choice| serde_json::to_value(choice).is_ok_and(|spelled| spelled == value))
        .unwrap_or_default())
}

impl<'de> Deserialize<'de> for TabLayout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        known_or_default(deserializer, &[Self::Horizontal, Self::Vertical])
    }
}

impl<'de> Deserialize<'de> for BarLayout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        known_or_default(deserializer, &[Self::Docked, Self::Floating])
    }
}

impl<'de> Deserialize<'de> for SidebarSide {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        known_or_default(deserializer, &[Self::Left, Self::Right])
    }
}

impl<'de> Deserialize<'de> for ToolbarPlacement {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        known_or_default(deserializer, &[Self::Top, Self::Sidebar])
    }
}

/// The home page as it is stored. A URL with a scheme Haku opens is kept as
/// typed; a bare host such as `example.com` gets `https://`; anything else is
/// the default, since every new tab would otherwise fail to open.
fn normalized_home_url(typed: &str) -> String {
    normalized_url(typed, OPENABLE_SCHEMES, DEFAULT_HOME_URL)
}

/// The search URL as it is stored, by the home page's rules but for web
/// addresses only: a search is sent to a site, and anything else would turn
/// every search typed in the address field into something that is not one.
fn normalized_search_url(typed: &str) -> String {
    normalized_url(typed, SEARCH_SCHEMES, DEFAULT_SEARCH_URL)
}

fn normalized_url(typed: &str, schemes: &[&str], default: &str) -> String {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        return default.into();
    }
    if let Ok(url) = tauri::Url::parse(trimmed) {
        if schemes.contains(&url.scheme()) {
            return trimmed.into();
        }
    }
    // Something that names a scheme already is not a bare host: `ftp://x`
    // would otherwise become `https://ftp://x`.
    if trimmed.contains("://") || trimmed.chars().any(char::is_whitespace) {
        return default.into();
    }
    let with_scheme = format!("{HTTPS_PREFIX}{trimmed}");
    match tauri::Url::parse(&with_scheme) {
        Ok(url) if url.host().is_some() => with_scheme,
        _ => default.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_shortcuts(raw: serde_json::Value) -> BTreeMap<Shortcut, Vec<String>> {
        let shortcuts: Shortcuts = serde_json::from_value(raw).unwrap();
        Settings {
            shortcuts,
            ..Settings::default()
        }
        .sanitized()
        .shortcuts
        .0
    }

    #[test]
    fn stored_shortcuts_are_rewritten_in_one_form_and_bad_ones_dropped() {
        let stored = stored_shortcuts(serde_json::json!({
            "reload": ["shift+ctrl+r", "Ctrl+C", "T", "Ctrl+Meta+R", "Ctrl+Shift+R"],
            "print": [],
            "noSuchAction": ["Ctrl+Q"],
            "find": "Ctrl+F",
        }));
        assert_eq!(
            stored,
            BTreeMap::from([
                (Shortcut::Reload, vec!["Ctrl+Shift+R".to_string()]),
                (Shortcut::Print, vec![])
            ])
        );
    }

    #[test]
    fn a_chord_listed_twice_stays_with_the_first_action_and_defaults_are_not_stored() {
        let stored = stored_shortcuts(serde_json::json!({
            "newTab": ["Ctrl+T"],
            "reload": ["Ctrl+Q"],
            "find": ["Ctrl+Q", "Ctrl+G"],
        }));
        assert_eq!(
            stored,
            BTreeMap::from([
                (Shortcut::Reload, vec!["Ctrl+Q".to_string()]),
                (Shortcut::Find, vec!["Ctrl+G".to_string()]),
            ])
        );
    }

    #[test]
    fn shortcuts_that_are_not_a_map_read_as_none_changed() {
        assert_eq!(stored_shortcuts(serde_json::json!(["Ctrl+T"])), BTreeMap::new());
    }

    #[test]
    fn the_keymap_lists_every_action_with_its_chords_and_defaults() {
        let shortcuts = Shortcuts(BTreeMap::from([(Shortcut::Reload, vec!["Ctrl+T".to_string()])]));
        let bindings = key_bindings(&shortcuts.keymap());
        assert_eq!(bindings.len(), DEFAULTS.len());
        let of = |shortcut| bindings.iter().find(|binding| binding.shortcut == shortcut).unwrap();
        assert_eq!(of(Shortcut::Reload).chords, ["Ctrl+T"]);
        assert_eq!(of(Shortcut::Reload).defaults, ["Ctrl+R", "F5"]);
        assert!(of(Shortcut::NewTab).chords.is_empty());
        assert_eq!(of(Shortcut::NewTab).defaults, ["Ctrl+T"]);
    }

    fn sanitized_home_url(home_url: &str) -> String {
        Settings {
            home_url: home_url.into(),
            ..Settings::default()
        }
        .sanitized()
        .home_url
    }

    #[test]
    fn only_a_site_not_kept_before_counts_as_keeping_one() {
        let kept = |sites: &[&str]| Settings {
            kept_sites: sites.iter().map(|site| (*site).to_string()).collect(),
            ..Settings::default()
        };
        let current = kept(&["a.test", "b.test"]);
        assert!(!current.keeps_new_site(&kept(&["a.test", "b.test"])));
        assert!(
            !current.keeps_new_site(&kept(&["b.test"])),
            "removing one is not keeping one"
        );
        assert!(!current.keeps_new_site(&kept(&[])));
        assert!(current.keeps_new_site(&kept(&["a.test", "c.test"])));
    }

    #[test]
    fn only_a_decision_not_stored_before_counts_as_adding_one() {
        use crate::model::{PermissionKind, PermissionState, SitePermission};
        let decided = |states: &[PermissionState]| Settings {
            site_permissions: SitePermissions(
                states
                    .iter()
                    .map(|state| SitePermission {
                        origin: "https://a.test".into(),
                        kind: PermissionKind::Camera,
                        state: *state,
                    })
                    .collect(),
            ),
            ..Settings::default()
        };
        let current = decided(&[PermissionState::Allow]);
        assert!(!current.adds_site_permission(&decided(&[PermissionState::Allow])));
        assert!(
            !current.adds_site_permission(&decided(&[])),
            "removing one is not adding one"
        );
        assert!(
            current.adds_site_permission(&decided(&[PermissionState::Block])),
            "changing one is adding one"
        );
    }

    #[test]
    fn a_home_url_without_a_scheme_gets_https() {
        assert_eq!(sanitized_home_url("example.com"), "https://example.com");
    }

    #[test]
    fn a_home_url_with_a_scheme_is_kept_as_typed() {
        assert_eq!(sanitized_home_url("https://example.com/a"), "https://example.com/a");
    }

    #[test]
    fn a_home_url_is_trimmed_before_it_is_kept() {
        assert_eq!(sanitized_home_url("  https://x.org  "), "https://x.org");
    }

    #[test]
    fn a_home_url_that_is_not_a_url_falls_back_to_the_default() {
        assert_eq!(sanitized_home_url("not a url"), Settings::default().home_url);
    }

    #[test]
    fn a_home_url_with_a_scheme_haku_does_not_open_falls_back_to_the_default() {
        assert_eq!(sanitized_home_url("ftp://example.com"), Settings::default().home_url);
        assert_eq!(sanitized_home_url("https://"), Settings::default().home_url);
    }

    #[test]
    fn a_host_with_a_port_gets_https_rather_than_being_read_as_a_scheme() {
        assert_eq!(sanitized_home_url("localhost:3000"), "https://localhost:3000");
    }

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

    fn sanitized_search_url(search_url: &str) -> String {
        Settings {
            search_url: search_url.into(),
            ..Settings::default()
        }
        .sanitized()
        .search_url
    }

    #[test]
    fn a_search_url_without_a_scheme_gets_https() {
        assert_eq!(
            sanitized_search_url("google.com/search?q="),
            "https://google.com/search?q="
        );
    }

    #[test]
    fn a_search_url_marking_where_the_search_goes_is_kept_as_typed() {
        assert_eq!(
            sanitized_search_url(" https://search.test/?q=%s "),
            "https://search.test/?q=%s"
        );
    }

    #[test]
    fn a_search_url_that_is_not_a_web_address_falls_back_to_the_default() {
        let default = Settings::default().search_url;
        assert_eq!(sanitized_search_url("not a url"), default);
        assert_eq!(sanitized_search_url("haku://settings?q="), default);
        assert_eq!(sanitized_search_url("javascript:alert(1)//"), default);
        assert_eq!(sanitized_search_url("https://"), default);
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
    fn layouts_default_to_horizontal_tabs_and_docked_bars() {
        let settings = Settings::default();
        assert_eq!(settings.tab_layout, TabLayout::Horizontal);
        assert_eq!(settings.bar_layout, BarLayout::Docked);
    }

    #[test]
    fn layouts_are_stored_under_their_names_and_values() {
        let json = serde_json::to_value(Settings {
            tab_layout: TabLayout::Vertical,
            bar_layout: BarLayout::Floating,
            ..Settings::default()
        })
        .unwrap();
        assert_eq!(json["tabLayout"], "vertical");
        assert_eq!(json["barLayout"], "floating");
    }

    fn stored(document: serde_json::Value) -> Settings {
        serde_json::from_value(crate::storage::merge::merge(
            serde_json::to_value(Settings::default()).unwrap(),
            document,
        ))
        .unwrap()
    }

    #[test]
    fn a_settings_file_from_before_layouts_gets_the_default_layouts() {
        let settings = stored(serde_json::json!({ "theme": "dark" }));
        assert_eq!(settings.tab_layout, TabLayout::Horizontal);
        assert_eq!(settings.bar_layout, BarLayout::Docked);
        assert_eq!(settings.theme, "dark");
    }

    #[test]
    fn a_stored_layout_is_read_back() {
        let settings = stored(serde_json::json!({ "tabLayout": "vertical", "barLayout": "floating" }));
        assert_eq!(settings.tab_layout, TabLayout::Vertical);
        assert_eq!(settings.bar_layout, BarLayout::Floating);
    }

    #[test]
    fn an_unknown_layout_falls_back_to_its_default_and_keeps_the_other_settings() {
        let settings = stored(serde_json::json!({
            "tabLayout": "diagonal",
            "barLayout": 3,
            "homeUrl": "https://example.com",
        }));
        assert_eq!(settings.tab_layout, TabLayout::Horizontal);
        assert_eq!(settings.bar_layout, BarLayout::Docked);
        assert_eq!(settings.home_url, "https://example.com");
    }

    #[test]
    fn an_unreadable_layout_in_a_settings_file_does_not_set_the_file_aside() {
        let directory = std::env::temp_dir().join(format!("haku-layout-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");
        std::fs::write(
            &path,
            r#"{ "tabLayout": "Vertical", "searchUrl": "https://search.test/?q=" }"#,
        )
        .unwrap();

        let settings: Settings = crate::storage::read_json(&path);

        assert_eq!(settings.tab_layout, TabLayout::Horizontal);
        assert_eq!(settings.search_url, "https://search.test/?q=");
        assert!(!crate::storage::corrupt_path(&path).exists());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn the_sidebar_defaults_to_the_left_with_the_toolbar_on_top() {
        let settings = stored(serde_json::json!({ "theme": "dark" }));
        assert_eq!(settings.sidebar_side, SidebarSide::Left);
        assert_eq!(settings.toolbar_placement, ToolbarPlacement::Top);
        assert_eq!(settings.sidebar_width, DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn the_sidebar_settings_are_stored_and_read_back() {
        let json = serde_json::to_value(Settings {
            sidebar_side: SidebarSide::Right,
            toolbar_placement: ToolbarPlacement::Sidebar,
            sidebar_width: 300,
            ..Settings::default()
        })
        .unwrap();
        assert_eq!(json["sidebarSide"], "right");
        assert_eq!(json["toolbarPlacement"], "sidebar");
        assert_eq!(json["sidebarWidth"], 300);

        let settings = stored(json);
        assert_eq!(settings.sidebar_side, SidebarSide::Right);
        assert_eq!(settings.toolbar_placement, ToolbarPlacement::Sidebar);
        assert_eq!(settings.sidebar_width, 300);
    }

    #[test]
    fn an_unknown_sidebar_value_falls_back_to_its_default() {
        let settings = stored(serde_json::json!({ "sidebarSide": "top", "toolbarPlacement": "Sidebar" }));
        assert_eq!(settings.sidebar_side, SidebarSide::Left);
        assert_eq!(settings.toolbar_placement, ToolbarPlacement::Top);
    }

    #[test]
    fn the_sidebar_width_is_clamped_when_sanitized() {
        let narrow = Settings {
            sidebar_width: 10,
            ..Settings::default()
        };
        let wide = Settings {
            sidebar_width: 5000,
            ..Settings::default()
        };
        assert_eq!(narrow.sanitized().sidebar_width, MIN_SIDEBAR_WIDTH);
        assert_eq!(wide.sanitized().sidebar_width, MAX_SIDEBAR_WIDTH);
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
