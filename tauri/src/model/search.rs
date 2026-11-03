//! Search engines: the presets Haku ships, the engines a user adds, and the
//! keywords that search one of them once.
//! [Address bar § Search engines](../../../docs/specs/address-bar.md#search-engines).

use serde::{Deserialize, Serialize};
use specta::Type;

/// The engine searches go to until the user picks another.
pub const DEFAULT_SEARCH_ENGINE: &str = "duckduckgo";

/// Marks where the search goes in an engine's URL, each time it appears, as in
/// other browsers' search URLs. A URL without it has the search added to its end.
pub const SEARCH_MARK: &str = "%s";

/// A search engine as the interface and the address field use it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchEngine {
    /// A preset's fixed id, or one the interface made for a custom engine.
    pub id: String,
    pub name: String,
    /// Where a search goes, with [`SEARCH_MARK`] where the query goes.
    pub url: String,
    /// Where completions for a query are asked, in the OpenSearch suggestions
    /// format, or nothing when the engine has none Haku can use.
    pub suggest_url: Option<String>,
    /// Typed before a query, searches this engine once.
    pub keyword: Option<String>,
}

/// An engine as the settings page lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListedSearchEngine {
    pub engine: SearchEngine,
    /// Shipped with Haku: its URL is fixed, and it is hidden, not removed.
    pub preset: bool,
    pub hidden: bool,
}

/// A preset, as written in code.
pub struct PresetEngine {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub suggest_url: Option<&'static str>,
    pub keyword: &'static str,
}

impl PresetEngine {
    /// This preset with `keyword` in place of its own; an empty one is none.
    pub fn engine(&self, keyword: Option<&str>) -> SearchEngine {
        let keyword = keyword.unwrap_or(self.keyword);
        SearchEngine {
            id: self.id.into(),
            name: self.name.into(),
            url: self.url.into(),
            suggest_url: self.suggest_url.map(Into::into),
            keyword: (!keyword.is_empty()).then(|| keyword.into()),
        }
    }
}

/// The presets, in the order the settings page lists them. Presets get `@`
/// keywords, so a search that happens to start with a short word is not
/// taken for a keyword.
pub const PRESET_ENGINES: &[PresetEngine] = &[
    PresetEngine {
        id: "duckduckgo",
        name: "DuckDuckGo",
        url: "https://duckduckgo.com/?q=%s",
        suggest_url: Some("https://duckduckgo.com/ac/?q=%s&type=list"),
        keyword: "@d",
    },
    PresetEngine {
        id: "google",
        name: "Google",
        url: "https://www.google.com/search?q=%s",
        suggest_url: Some("https://suggestqueries.google.com/complete/search?client=firefox&q=%s"),
        keyword: "@g",
    },
    PresetEngine {
        id: "bing",
        name: "Bing",
        url: "https://www.bing.com/search?q=%s",
        suggest_url: Some("https://api.bing.com/osjson.aspx?query=%s"),
        keyword: "@b",
    },
    PresetEngine {
        id: "brave",
        name: "Brave",
        url: "https://search.brave.com/search?q=%s",
        suggest_url: Some("https://search.brave.com/api/suggest?q=%s"),
        keyword: "@br",
    },
    PresetEngine {
        id: "startpage",
        name: "Startpage",
        url: "https://www.startpage.com/sp/search?query=%s",
        suggest_url: Some("https://www.startpage.com/osuggestions?q=%s"),
        keyword: "@s",
    },
    PresetEngine {
        id: "ecosia",
        name: "Ecosia",
        url: "https://www.ecosia.org/search?q=%s",
        suggest_url: Some("https://ac.ecosia.org/autocomplete?q=%s&type=list"),
        keyword: "@e",
    },
    // Kagi answers suggestions only for a signed-in session.
    PresetEngine {
        id: "kagi",
        name: "Kagi",
        url: "https://kagi.com/search?q=%s",
        suggest_url: None,
        keyword: "@k",
    },
];

/// The preset with this id.
pub fn preset_engine(id: &str) -> Option<&'static PresetEngine> {
    PRESET_ENGINES.iter().find(|preset| preset.id == id)
}

/// What the address field searches with: the default engine, and every
/// visible engine by keyword.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Search {
    pub default: SearchEngine,
    pub engines: Vec<SearchEngine>,
}

impl Search {
    /// The visible engine whose keyword this is, ignoring case.
    pub fn by_keyword(&self, word: &str) -> Option<&SearchEngine> {
        self.engines.iter().find(|engine| {
            engine
                .keyword
                .as_deref()
                .is_some_and(|keyword| keyword.eq_ignore_ascii_case(word))
        })
    }
}

/// `template` with `query`, already encoded, where the search goes.
pub fn search_url(template: &str, query: &str) -> String {
    if template.contains(SEARCH_MARK) {
        template.replace(SEARCH_MARK, query)
    } else {
        format!("{template}{query}")
    }
}

/// Whether two search URLs send a search to the same place, whether or not
/// they mark where it goes.
pub fn same_search_url(a: &str, b: &str) -> bool {
    const PROBE: &str = "haku-probe";
    search_url(a, PROBE) == search_url(b, PROBE)
}
