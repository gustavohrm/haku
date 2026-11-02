use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::model::keymap::{defaults_of, DEFAULTS};
use crate::model::permission::{normalized, SitePermissions};
use crate::model::search::same_search_url;
use crate::model::{preset_engine, ListedSearchEngine, Search, SearchEngine, DEFAULT_SEARCH_ENGINE, PRESET_ENGINES};
use crate::model::{Chord, Keymap, Policy, Preset, Shortcut, DEFAULT_CAPACITY};
use crate::model::{HistoryRetention, HistoryRules, SiteZooms};

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

/// When the bookmarks bar shows.
/// [Bookmarks § The bookmarks bar](../../../docs/specs/bookmarks.md#the-bookmarks-bar).
///
/// Read leniently, as [`known_or_default`] explains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BookmarksBar {
    /// In every tab.
    Always,
    /// Only while the focused tab is on the new-tab page, as in Chrome.
    #[default]
    NewTab,
    /// Never.
    Never,
}

/// Where bookmarks go while the toolbar is in the tab column.
/// [Bookmarks § The bookmarks bar](../../../docs/specs/bookmarks.md#the-bookmarks-bar).
///
/// Read leniently, as [`known_or_default`] explains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SidebarBookmarks {
    /// The bookmarks bar, as a row of its own across the top of the page area.
    Row,
    /// A button in the column's foot row, opening a menu of the bar.
    #[default]
    Button,
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
    pub bookmarks_bar: BookmarksBar,
    pub sidebar_bookmarks: SidebarBookmarks,
    /// The tab column's width in CSS pixels, between [`MIN_SIDEBAR_WIDTH`]
    /// and [`MAX_SIDEBAR_WIDTH`].
    pub sidebar_width: u32,
    /// BCP-47 language tag the interface is shown in.
    pub locale: String,
    /// The id of the engine searches go to.
    /// [Address bar § Search engines](../../../docs/specs/address-bar.md#search-engines).
    pub search_engine: String,
    /// Engines the user added, in the order they were added.
    pub custom_search_engines: Vec<SearchEngine>,
    /// Keywords the user gave presets in place of their own, by preset id. An
    /// empty one leaves the preset without a keyword.
    pub preset_keywords: BTreeMap<String, String>,
    /// Ids of presets the user hid. The default engine is never hidden.
    pub hidden_search_engines: Vec<String>,
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
    /// The zoom chosen for sites, by host. 100% is never stored.
    /// [Zoom](../../../docs/specs/zoom.md).
    pub site_zoom: SiteZooms,
    /// The chords of the shortcuts the user changed, by action.
    /// [Shortcuts § Changing bindings](../../../docs/specs/shortcuts.md#changing-bindings).
    pub shortcuts: Shortcuts,
    /// Whether browsing history is recorded at all.
    /// [History § Recording on, off and paused](../../../docs/specs/history.md#recording-on-off-and-paused).
    pub history_enabled: bool,
    /// How long visits are kept.
    pub history_retention: HistoryRetention,
    /// What is recorded for which URLs, checked in order.
    /// [History § Rules](../../../docs/specs/history.md#rules).
    pub history_rules: HistoryRules,
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
    /// in table order. An action left with its defaults is dropped, and so is
    /// one whose every chord was dropped: only an empty list unbinds.
    fn normalized(self) -> Self {
        let mut taken = HashSet::new();
        let mut kept = BTreeMap::new();
        for (shortcut, defaults) in DEFAULTS {
            let Some(stored) = self.0.get(shortcut) else {
                continue;
            };
            let chords: Vec<Chord> = stored
                .iter()
                .filter_map(|chord| Chord::parse(chord))
                .filter(|chord| chord.refusal().is_none() && taken.insert(*chord))
                .collect();
            if chords.is_empty() && !stored.is_empty() {
                continue;
            }
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
            bookmarks_bar: BookmarksBar::default(),
            sidebar_bookmarks: SidebarBookmarks::default(),
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            locale: "en".into(),
            search_engine: DEFAULT_SEARCH_ENGINE.into(),
            custom_search_engines: Vec::new(),
            preset_keywords: BTreeMap::new(),
            hidden_search_engines: Vec::new(),
            home_url: DEFAULT_HOME_URL.into(),
            disabled_extensions: Vec::new(),
            pinned_extensions: Vec::new(),
            site_permissions: SitePermissions::default(),
            site_zoom: SiteZooms::default(),
            shortcuts: Shortcuts::default(),
            history_enabled: true,
            history_retention: HistoryRetention::default(),
            history_rules: HistoryRules::default(),
        }
    }
}

impl Settings {
    /// Clamps values that would leave the browser unusable.
    ///
    /// Settings can be hand-edited, so a zero capacity or a search engine that
    /// does not exist has to be survivable rather than fatal.
    pub fn sanitized(mut self) -> Self {
        self.webview_capacity = self.webview_capacity.max(1);
        self.sidebar_width = self.sidebar_width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        self.sanitize_search_engines();
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
        self.site_zoom = std::mem::take(&mut self.site_zoom).normalized();
        self.shortcuts = std::mem::take(&mut self.shortcuts).normalized();
        self.history_rules = std::mem::take(&mut self.history_rules).normalized();
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

    /// Whether `next` stores a zoom level these settings do not. Removing one
    /// is not storing one, as for kept sites.
    pub fn adds_site_zoom(&self, next: &Self) -> bool {
        next.site_zoom.iter().any(|level| !self.site_zoom.contains(level))
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

impl<'de> Deserialize<'de> for BookmarksBar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        known_or_default(deserializer, &[Self::Always, Self::NewTab, Self::Never])
    }
}

impl<'de> Deserialize<'de> for SidebarBookmarks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        known_or_default(deserializer, &[Self::Row, Self::Button])
    }
}

impl Settings {
    /// Whether bookmarks are a button in the tab column rather than a bar,
    /// which needs the tabs in a column, the toolbar in it, and `button`.
    pub fn bookmarks_in_button(&self) -> bool {
        self.tab_layout == TabLayout::Vertical
            && self.toolbar_placement == ToolbarPlacement::Sidebar
            && self.sidebar_bookmarks == SidebarBookmarks::Button
    }

    /// What Ctrl+Shift+B leaves: `always` and `never` swap, and `newTab`
    /// becomes `always`.
    pub fn with_bookmarks_bar_toggled(mut self) -> Self {
        self.bookmarks_bar = match self.bookmarks_bar {
            BookmarksBar::Always => BookmarksBar::Never,
            BookmarksBar::NewTab | BookmarksBar::Never => BookmarksBar::Always,
        };
        self
    }
}

/// The home page as it is stored. A URL with a scheme Haku opens is kept as
/// typed; a bare host such as `example.com` gets `https://`; anything else is
/// the default, since every new tab would otherwise fail to open.
fn normalized_home_url(typed: &str) -> String {
    normalized_url(typed, OPENABLE_SCHEMES, DEFAULT_HOME_URL)
}

/// A search URL as it is stored, by the home page's rules but for web
/// addresses only: a search is sent to a site, and anything else would turn
/// every search typed in the address field into something that is not one.
/// Nothing when it is not one.
fn normalized_search_url(typed: &str) -> Option<String> {
    web_or_typed_url(typed, SEARCH_SCHEMES)
}

fn normalized_url(typed: &str, schemes: &[&str], default: &str) -> String {
    web_or_typed_url(typed, schemes).unwrap_or_else(|| default.into())
}

/// `typed` when it has one of `schemes`, `https://` and `typed` when it is a
/// bare host, and nothing otherwise.
fn web_or_typed_url(typed: &str, schemes: &[&str]) -> Option<String> {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = tauri::Url::parse(trimmed) {
        if schemes.contains(&url.scheme()) {
            return Some(trimmed.into());
        }
    }
    // Something that names a scheme already is not a bare host: `ftp://x`
    // would otherwise become `https://ftp://x`.
    if trimmed.contains("://") || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let with_scheme = format!("{HTTPS_PREFIX}{trimmed}");
    match tauri::Url::parse(&with_scheme) {
        Ok(url) if url.host().is_some() => Some(with_scheme),
        _ => None,
    }
}

/// The id a search URL from before search engines is kept under, when it is
/// no preset's.
const MIGRATED_ENGINE_ID: &str = "migrated";

impl Settings {
    /// Takes on a search URL stored before search engines existed, when the
    /// stored document names no engine: a preset's URL selects that preset,
    /// and any other becomes a custom engine named after its host.
    /// [Address bar § Approved decisions](../../../docs/specs/address-bar.md#approved-decisions), D.
    ///
    /// The URL is read from the document as stored, since `Settings` no longer
    /// has the field. It is not written back, so this runs at each launch until
    /// anything in settings is saved, with the same result each time.
    pub fn migrated(mut self, stored: &serde_json::Value) -> Self {
        if stored.get("searchEngine").is_some() {
            return self;
        }
        let Some(url) = stored
            .get("searchUrl")
            .and_then(serde_json::Value::as_str)
            .and_then(normalized_search_url)
        else {
            return self;
        };
        if let Some(preset) = PRESET_ENGINES.iter().find(|preset| same_search_url(preset.url, &url)) {
            self.search_engine = preset.id.into();
            return self;
        }
        let name = tauri::Url::parse(&url)
            .ok()
            .and_then(|parsed| {
                parsed
                    .host_str()
                    .map(|host| host.trim_start_matches("www.").to_string())
            })
            .unwrap_or_else(|| url.clone());
        self.custom_search_engines
            .retain(|engine| engine.id != MIGRATED_ENGINE_ID);
        self.custom_search_engines.push(SearchEngine {
            id: MIGRATED_ENGINE_ID.into(),
            name,
            url,
            suggest_url: None,
            keyword: None,
        });
        self.search_engine = MIGRATED_ENGINE_ID.into();
        self
    }

    /// Every engine, presets first, as the settings page lists them.
    pub fn listed_search_engines(&self) -> Vec<ListedSearchEngine> {
        let presets = PRESET_ENGINES.iter().map(|preset| ListedSearchEngine {
            engine: preset.engine(self.preset_keywords.get(preset.id).map(String::as_str)),
            preset: true,
            hidden: self.hidden_search_engines.iter().any(|id| id == preset.id),
        });
        let custom = self.custom_search_engines.iter().map(|engine| ListedSearchEngine {
            engine: engine.clone(),
            preset: false,
            hidden: false,
        });
        presets.chain(custom).collect()
    }

    /// What the address field searches with. Sanitized settings always name
    /// a visible engine; anything else searches with the default preset.
    pub fn search(&self) -> Search {
        let engines: Vec<SearchEngine> = self
            .listed_search_engines()
            .into_iter()
            .filter(|listed| !listed.hidden)
            .map(|listed| listed.engine)
            .collect();
        let default = engines
            .iter()
            .find(|engine| engine.id == self.search_engine)
            .cloned()
            .unwrap_or_else(|| PRESET_ENGINES[0].engine(None));
        Search { default, engines }
    }

    /// Drops custom engines that cannot search, unknown presets and keywords
    /// that clash, and keeps the default engine one that exists and shows.
    fn sanitize_search_engines(&mut self) {
        let mut ids: HashSet<String> = PRESET_ENGINES.iter().map(|preset| preset.id.to_string()).collect();
        let mut custom = Vec::new();
        for mut engine in self.custom_search_engines.drain(..) {
            engine.id = engine.id.trim().to_string();
            let Some(url) = normalized_search_url(&engine.url) else {
                continue;
            };
            if engine.id.is_empty() || !ids.insert(engine.id.clone()) {
                continue;
            }
            engine.url = url;
            engine.suggest_url = engine.suggest_url.as_deref().and_then(normalized_search_url);
            engine.name = engine.name.trim().to_string();
            if engine.name.is_empty() {
                engine.name.clone_from(&engine.url);
            }
            custom.push(engine);
        }

        // Kept first come, first served, presets before custom engines, so a
        // keyword that clashes never sends a search somewhere unexpected.
        let mut keywords = HashSet::new();
        let mut take = |keyword: &str| -> Option<String> {
            let keyword = keyword.trim();
            let usable = !keyword.is_empty()
                && !keyword.chars().any(char::is_whitespace)
                && keywords.insert(keyword.to_lowercase());
            usable.then(|| keyword.to_string())
        };
        let mut preset_keywords = BTreeMap::new();
        for preset in PRESET_ENGINES {
            let stored = self.preset_keywords.get(preset.id).map(String::as_str);
            let kept = take(stored.unwrap_or(preset.keyword)).unwrap_or_default();
            if kept != preset.keyword {
                preset_keywords.insert(preset.id.to_string(), kept);
            }
        }
        self.preset_keywords = preset_keywords;
        for engine in &mut custom {
            engine.keyword = engine.keyword.as_deref().and_then(&mut take);
        }
        self.custom_search_engines = custom;

        if !ids.contains(&self.search_engine) {
            DEFAULT_SEARCH_ENGINE.clone_into(&mut self.search_engine);
        }
        let mut hidden: Vec<String> = Vec::new();
        for id in self.hidden_search_engines.drain(..) {
            if preset_engine(&id).is_some() && id != self.search_engine && !hidden.contains(&id) {
                hidden.push(id);
            }
        }
        self.hidden_search_engines = hidden;
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
    fn history_is_recorded_and_kept_forever_with_no_rules_by_default() {
        let settings = Settings::default();
        assert!(settings.history_enabled);
        assert_eq!(settings.history_retention, HistoryRetention::Forever);
        assert!(settings.history_rules.is_empty());
    }

    #[test]
    fn stored_history_rules_are_normalized() {
        let history_rules: HistoryRules = serde_json::from_value(serde_json::json!([
            { "pattern": { "kind": "site", "value": "https://Bank.test/login" }, "action": "never" },
            { "pattern": { "kind": "site", "value": "bank.test" }, "action": "always" },
        ]))
        .unwrap();
        let settings = Settings {
            history_rules,
            ..Settings::default()
        }
        .sanitized();
        let rules: Vec<_> = settings
            .history_rules
            .iter()
            .map(|rule| (rule.pattern.value.as_str(), rule.action))
            .collect();
        assert_eq!(rules, vec![("bank.test", crate::model::HistoryRuleAction::Never)]);
    }

    #[test]
    fn an_action_whose_every_chord_is_dropped_keeps_its_defaults() {
        let stored = stored_shortcuts(serde_json::json!({
            "home": ["Ctrl+C", "Q", "Ctrl+Meta+H"],
            "print": [],
        }));
        assert_eq!(stored, BTreeMap::from([(Shortcut::Print, vec![])]));
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

    fn custom(id: &str, url: &str, keyword: Option<&str>) -> SearchEngine {
        SearchEngine {
            id: id.into(),
            name: id.into(),
            url: url.into(),
            suggest_url: None,
            keyword: keyword.map(Into::into),
        }
    }

    fn with_custom(engines: Vec<SearchEngine>) -> Settings {
        Settings {
            custom_search_engines: engines,
            ..Settings::default()
        }
        .sanitized()
    }

    fn sanitized_search_url(search_url: &str) -> Option<String> {
        with_custom(vec![custom("mine", search_url, None)])
            .custom_search_engines
            .first()
            .map(|engine| engine.url.clone())
    }

    #[test]
    fn a_custom_search_url_without_a_scheme_gets_https() {
        assert_eq!(
            sanitized_search_url("google.com/search?q=").as_deref(),
            Some("https://google.com/search?q=")
        );
    }

    #[test]
    fn a_custom_search_url_marking_where_the_search_goes_is_kept_as_typed() {
        assert_eq!(
            sanitized_search_url(" https://search.test/?q=%s ").as_deref(),
            Some("https://search.test/?q=%s")
        );
    }

    #[test]
    fn a_custom_engine_whose_url_is_not_a_web_address_is_dropped() {
        for url in [
            "not a url",
            "haku://settings?q=",
            "javascript:alert(1)//",
            "https://",
            "  ",
        ] {
            assert_eq!(sanitized_search_url(url), None, "{url}");
        }
    }

    #[test]
    fn searches_go_to_duckduckgo_by_default_with_every_preset_shown() {
        let settings = Settings::default();
        assert_eq!(settings.search_engine, "duckduckgo");
        let search = settings.search();
        assert_eq!(search.default.url, "https://duckduckgo.com/?q=%s");
        assert_eq!(search.engines.len(), PRESET_ENGINES.len());
    }

    #[test]
    fn an_engine_that_does_not_exist_falls_back_to_the_default() {
        let settings = Settings {
            search_engine: "gone".into(),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.search_engine, "duckduckgo");
    }

    #[test]
    fn the_default_engine_is_never_hidden_and_unknown_ids_are_not_kept() {
        let settings = Settings {
            search_engine: "google".into(),
            hidden_search_engines: vec!["google".into(), "bing".into(), "nope".into(), "bing".into()],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.hidden_search_engines, vec!["bing".to_string()]);
        assert!(settings.search().engines.iter().all(|engine| engine.id != "bing"));
    }

    #[test]
    fn a_custom_engine_reusing_a_taken_id_is_dropped() {
        let settings = with_custom(vec![
            custom("google", "https://a.test/?q=", None),
            custom("mine", "https://b.test/?q=", None),
            custom("mine", "https://c.test/?q=", None),
        ]);
        let urls: Vec<_> = settings
            .custom_search_engines
            .iter()
            .map(|engine| engine.url.as_str())
            .collect();
        assert_eq!(urls, vec!["https://b.test/?q="]);
    }

    #[test]
    fn a_keyword_already_taken_is_dropped_from_the_later_engine() {
        let settings = with_custom(vec![
            custom("one", "https://a.test/?q=", Some("@G")),
            custom("two", "https://b.test/?q=", Some("wk")),
            custom("three", "https://c.test/?q=", Some("WK")),
            custom("four", "https://d.test/?q=", Some("two words")),
        ]);
        let keywords: Vec<_> = settings
            .custom_search_engines
            .iter()
            .map(|engine| engine.keyword.as_deref())
            .collect();
        assert_eq!(keywords, vec![None, Some("wk"), None, None]);
    }

    #[test]
    fn a_preset_keyword_is_stored_only_when_it_differs_and_an_empty_one_removes_it() {
        let settings = Settings {
            preset_keywords: BTreeMap::from([
                ("google".to_string(), "gg".to_string()),
                ("bing".to_string(), "@b".to_string()),
                ("brave".to_string(), String::new()),
                ("nope".to_string(), "x".to_string()),
            ]),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(
            settings.preset_keywords,
            BTreeMap::from([
                ("brave".to_string(), String::new()),
                ("google".to_string(), "gg".to_string())
            ])
        );
        let search = settings.search();
        assert_eq!(search.by_keyword("GG").map(|engine| engine.id.as_str()), Some("google"));
        assert!(search.by_keyword("@g").is_none());
        assert!(search
            .engines
            .iter()
            .any(|engine| engine.id == "brave" && engine.keyword.is_none()));
    }

    #[test]
    fn a_stored_search_url_of_a_preset_selects_that_preset() {
        for (url, id) in [
            ("https://duckduckgo.com/?q=", "duckduckgo"),
            ("https://www.google.com/search?q=%s", "google"),
            ("www.bing.com/search?q=", "bing"),
        ] {
            let settings = Settings::default().migrated(&serde_json::json!({ "searchUrl": url }));
            assert_eq!(settings.search_engine, id, "{url}");
            assert!(settings.custom_search_engines.is_empty());
        }
    }

    #[test]
    fn any_other_stored_search_url_becomes_a_custom_engine_named_after_its_host() {
        let stored = serde_json::json!({ "searchUrl": "https://www.search.test/find?q=%s&x=1" });
        let settings = Settings::default().migrated(&stored).migrated(&stored).sanitized();
        assert_eq!(settings.search_engine, "migrated");
        assert_eq!(
            settings.custom_search_engines,
            vec![SearchEngine {
                id: "migrated".into(),
                name: "search.test".into(),
                url: "https://www.search.test/find?q=%s&x=1".into(),
                suggest_url: None,
                keyword: None,
            }]
        );
    }

    #[test]
    fn a_stored_engine_or_an_unusable_search_url_migrates_nothing() {
        let named = serde_json::json!({ "searchEngine": "bing", "searchUrl": "https://a.test/?q=" });
        assert_eq!(Settings::default().migrated(&named), Settings::default());
        let unusable = serde_json::json!({ "searchUrl": "javascript:alert(1)" });
        assert_eq!(Settings::default().migrated(&unusable), Settings::default());
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
        std::fs::write(&path, r#"{ "tabLayout": "Vertical", "theme": "dark" }"#).unwrap();

        let settings: Settings = crate::storage::read_json(&path);

        assert_eq!(settings.tab_layout, TabLayout::Horizontal);
        assert_eq!(settings.theme, "dark");
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
    fn the_bookmarks_bar_shows_on_new_tabs_and_is_a_button_in_the_column_by_default() {
        let settings = stored(serde_json::json!({ "theme": "dark" }));
        assert_eq!(settings.bookmarks_bar, BookmarksBar::NewTab);
        assert_eq!(settings.sidebar_bookmarks, SidebarBookmarks::Button);
    }

    #[test]
    fn the_bookmarks_settings_are_stored_and_read_back() {
        let json = serde_json::to_value(Settings {
            bookmarks_bar: BookmarksBar::Never,
            sidebar_bookmarks: SidebarBookmarks::Row,
            ..Settings::default()
        })
        .unwrap();
        assert_eq!(json["bookmarksBar"], "never");
        assert_eq!(json["sidebarBookmarks"], "row");
        assert_eq!(serde_json::to_value(BookmarksBar::NewTab).unwrap(), "newTab");

        let settings = stored(json);
        assert_eq!(settings.bookmarks_bar, BookmarksBar::Never);
        assert_eq!(settings.sidebar_bookmarks, SidebarBookmarks::Row);
    }

    #[test]
    fn an_unknown_bookmarks_value_falls_back_to_its_default() {
        let settings = stored(serde_json::json!({ "bookmarksBar": "sometimes", "sidebarBookmarks": "Row" }));
        assert_eq!(settings.bookmarks_bar, BookmarksBar::NewTab);
        assert_eq!(settings.sidebar_bookmarks, SidebarBookmarks::Button);
    }

    #[test]
    fn toggling_the_bookmarks_bar_swaps_always_and_never_and_shows_it_from_new_tab() {
        let toggled = |bookmarks_bar| {
            Settings {
                bookmarks_bar,
                ..Settings::default()
            }
            .with_bookmarks_bar_toggled()
            .bookmarks_bar
        };
        assert_eq!(toggled(BookmarksBar::Always), BookmarksBar::Never);
        assert_eq!(toggled(BookmarksBar::Never), BookmarksBar::Always);
        assert_eq!(toggled(BookmarksBar::NewTab), BookmarksBar::Always);
    }

    #[test]
    fn bookmarks_are_a_button_only_with_the_toolbar_in_the_column() {
        let column = Settings {
            tab_layout: TabLayout::Vertical,
            toolbar_placement: ToolbarPlacement::Sidebar,
            ..Settings::default()
        };
        assert!(column.bookmarks_in_button());
        assert!(!Settings {
            sidebar_bookmarks: SidebarBookmarks::Row,
            ..column.clone()
        }
        .bookmarks_in_button());
        // The toolbar placement applies only while the tabs are in a column.
        assert!(!Settings {
            tab_layout: TabLayout::Horizontal,
            ..column.clone()
        }
        .bookmarks_in_button());
        assert!(!Settings {
            toolbar_placement: ToolbarPlacement::Top,
            ..column
        }
        .bookmarks_in_button());
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
