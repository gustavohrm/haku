//! What the address bar suggests while the user types: matching and ranking
//! open tabs, history and bookmarks.
//! [Address bar § Suggestions](../../../docs/specs/address-bar.md#suggestions).

use std::collections::HashMap;

use serde::Serialize;
use specta::Type;

use crate::model::{host_of, SearchEngine, TabId};

/// The most rows the list shows.
pub const SUGGESTIONS_MAX: usize = 10;
/// The most open tabs the list offers.
pub const TABS_MAX: usize = 3;
/// The most history and bookmark rows the list shows, together.
pub const PAGES_MAX: usize = 6;
/// The most completions from the search engine the list shows.
pub const ENGINE_SUGGESTIONS_MAX: usize = 4;
/// How many matching history pages are ranked, the most used first. A page
/// past them would rank low anyway: few visits, so it is far down the list.
pub const PAGE_CANDIDATES_MAX: u32 = 100;

/// How much more a typed visit counts than any other.
const TYPED_WEIGHT: f64 = 4.0;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How much a visit counts by its age in days, as Firefox weighs them.
const RECENCY: &[(i64, f64)] = &[(4, 100.0), (14, 70.0), (31, 50.0), (90, 30.0)];
const RECENCY_OLD: f64 = 10.0;

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Suggestion {
    /// What Enter does for input that searches: `engine` names where.
    Search { engine: String, query: String },
    /// What Enter does for input that is an address, or completes to one.
    Visit { url: String },
    /// An open tab, offered to switch to rather than open again.
    Tab { tab: TabId, title: String, url: String },
    /// A page from history, a bookmark, or both.
    Page {
        title: String,
        url: String,
        bookmarked: bool,
    },
    /// An engine, while its keyword is being typed. Choosing it puts the
    /// keyword in the field, to type a query after.
    Engine { name: String, keyword: String },
    /// A completion from the search engine, which searches `engine` for
    /// `query`. `input` is what the field takes to do that: the query, or the
    /// engine's keyword and the query where the query alone would go
    /// elsewhere, to another engine or to an address.
    Suggested {
        engine: String,
        query: String,
        input: String,
    },
}

/// The rows for one input.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Suggestions {
    /// The input these answer, so the interface can drop an answer that
    /// arrives after the input changed.
    pub input: String,
    /// The input completed to a host the user typed before, or nothing.
    pub completion: Option<String>,
    /// What Enter does first, then engines, open tabs, then pages.
    pub rows: Vec<Suggestion>,
}

/// The search engine's completions for one input.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineSuggestions {
    /// The input these answer, as for [`Suggestions`].
    pub input: String,
    /// [`Suggestion::Suggested`] rows, at most [`ENGINE_SUGGESTIONS_MAX`].
    pub rows: Vec<Suggestion>,
}

/// The completions in an OpenSearch suggestions answer, `[query, [completion, …], …]`,
/// which every preset's suggestion URL answers in. Nothing for any other shape.
pub fn parse_open_search(body: &str) -> Vec<String> {
    let Ok(serde_json::Value::Array(parts)) = serde_json::from_str(body) else {
        return Vec::new();
    };
    let Some(serde_json::Value::Array(completions)) = parts.get(1) else {
        return Vec::new();
    };
    completions
        .iter()
        .filter_map(|completion| completion.as_str())
        .map(|completion| completion.trim().to_string())
        .filter(|completion| !completion.is_empty())
        .collect()
}

/// A history page, or a bookmark ranked as one.
#[derive(Clone, Debug, PartialEq)]
pub struct PageCandidate {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub typed_count: i64,
    /// Milliseconds since the Unix epoch.
    pub last_visited_at: i64,
    pub bookmarked: bool,
}

/// A bookmarked link.
#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkCandidate {
    pub url: String,
    pub title: String,
    /// Milliseconds since the Unix epoch.
    pub added_at: i64,
}

/// An open tab the list may offer.
#[derive(Clone, Copy, Debug)]
pub struct TabCandidate<'a> {
    pub id: TabId,
    pub url: &'a str,
    pub title: &'a str,
    pub active_at: u64,
}

/// The input's words, lowercased, as matching compares them.
pub fn words(input: &str) -> Vec<String> {
    input.split_whitespace().map(str::to_lowercase).collect()
}

/// Whether every word appears in the address or the title.
pub fn matches(words: &[String], url: &str, title: &str) -> bool {
    let (url, title) = (url.to_lowercase(), title.to_lowercase());
    !words.is_empty() && words.iter().all(|word| url.contains(word) || title.contains(word))
}

/// How many words start the host, past any `www.`, or a word of the title.
/// Such a match says more than one inside a word: `git` is more likely
/// GitHub than a page about digits.
fn boundary_hits(words: &[String], url: &str, title: &str) -> usize {
    let host = host_of(url).unwrap_or_default();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let title = title.to_lowercase();
    let title_words: Vec<&str> = title.split(|c: char| !c.is_alphanumeric()).collect();
    words
        .iter()
        .filter(|word| {
            host.starts_with(word.as_str()) || title_words.iter().any(|each| each.starts_with(word.as_str()))
        })
        .count()
}

/// How much a page's visits count: typed ones more, recent ones more.
fn frecency(page: &PageCandidate, now: i64) -> f64 {
    let age_days = (now - page.last_visited_at).max(0) / DAY_MS;
    let recency = RECENCY
        .iter()
        .find(|(days, _)| age_days <= *days)
        .map_or(RECENCY_OLD, |(_, weight)| *weight);
    let typed = page.typed_count.max(0) as f64;
    let other = (page.visit_count - page.typed_count).max(0) as f64;
    (typed * TYPED_WEIGHT + other) * recency
}

fn score(page: &PageCandidate, words: &[String], now: i64) -> f64 {
    frecency(page, now) * (1 + boundary_hits(words, &page.url, &page.title)) as f64
}

/// History pages and bookmarks that match, together, one row per address,
/// best first. A bookmark ranks as one typed visit more, made when it was
/// added if the page was never visited, so a bookmark never opened appears.
pub fn rank_pages(
    history: Vec<PageCandidate>,
    bookmarks: Vec<BookmarkCandidate>,
    words: &[String],
    now: i64,
    limit: usize,
) -> Vec<Suggestion> {
    let mut pages: HashMap<String, PageCandidate> = history.into_iter().map(|page| (page.url.clone(), page)).collect();
    for bookmark in bookmarks {
        let page = pages.entry(bookmark.url.clone()).or_insert_with(|| PageCandidate {
            url: bookmark.url,
            title: bookmark.title.clone(),
            visit_count: 0,
            typed_count: 0,
            last_visited_at: bookmark.added_at,
            bookmarked: false,
        });
        if !page.bookmarked {
            page.bookmarked = true;
            page.visit_count += 1;
            page.typed_count += 1;
        }
        // The user named the bookmark; the page's own title may say less.
        if !bookmark.title.trim().is_empty() {
            page.title = bookmark.title;
        }
    }
    let mut ranked: Vec<(f64, PageCandidate)> = pages
        .into_values()
        .filter(|page| matches(words, &page.url, &page.title))
        .map(|page| (score(&page, words, now), page))
        .collect();
    ranked.sort_by(|(a, x), (b, y)| {
        b.total_cmp(a)
            .then(y.last_visited_at.cmp(&x.last_visited_at))
            .then(x.url.cmp(&y.url))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, page)| Suggestion::Page {
            title: page.title,
            url: page.url,
            bookmarked: page.bookmarked,
        })
        .collect()
}

/// Open tabs that match, those matching at a word's start first, then the
/// one shown most recently.
pub fn rank_tabs<'a>(
    tabs: impl IntoIterator<Item = TabCandidate<'a>>,
    words: &[String],
    limit: usize,
) -> Vec<Suggestion> {
    let mut matching: Vec<(usize, TabCandidate<'a>)> = tabs
        .into_iter()
        .filter(|tab| matches(words, tab.url, tab.title))
        .map(|tab| (boundary_hits(words, tab.url, tab.title), tab))
        .collect();
    matching.sort_by(|(a, x), (b, y)| b.cmp(a).then(y.active_at.cmp(&x.active_at)));
    matching
        .into_iter()
        .take(limit)
        .map(|(_, tab)| Suggestion::Tab {
            tab: tab.id,
            title: tab.title.to_string(),
            url: tab.url.to_string(),
        })
        .collect()
}

/// The start of a host the input could complete to: a single word that is
/// not a path, lowercased, or nothing.
pub fn host_prefix(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    let plain = !trimmed.is_empty()
        && trimmed == input
        && !trimmed.contains(char::is_whitespace)
        && !trimmed.contains(['/', '?', '#', ':', '@']);
    plain.then(|| trimmed.to_lowercase())
}

/// `host`, or `host` without its `www.`, when it continues `prefix` and is
/// longer than it.
pub fn complete_host(prefix: &str, host: &str) -> Option<String> {
    [host, host.strip_prefix("www.").unwrap_or(host)]
        .into_iter()
        .find(|candidate| candidate.len() > prefix.len() && candidate.starts_with(prefix))
        .map(str::to_string)
}

/// The engines whose keyword is being typed: every engine with one for `@`
/// alone, else those whose keyword starts with the input.
pub fn keyword_rows(input: &str, engines: &[SearchEngine]) -> Vec<Suggestion> {
    let typed = input.trim();
    if !typed.starts_with('@') || typed.contains(char::is_whitespace) {
        return Vec::new();
    }
    let typed = typed.to_lowercase();
    engines
        .iter()
        .filter_map(|engine| Some((engine, engine.keyword.as_deref()?)))
        .filter(|(_, keyword)| keyword.to_lowercase().starts_with(&typed))
        .map(|(engine, keyword)| Suggestion::Engine {
            name: engine.name.clone(),
            keyword: keyword.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000 * DAY_MS;

    fn page(url: &str, title: &str, visits: i64, typed: i64, days_ago: i64) -> PageCandidate {
        PageCandidate {
            url: url.into(),
            title: title.into(),
            visit_count: visits,
            typed_count: typed,
            last_visited_at: NOW - days_ago * DAY_MS,
            bookmarked: false,
        }
    }

    fn urls(rows: &[Suggestion]) -> Vec<&str> {
        rows.iter()
            .map(|row| match row {
                Suggestion::Page { url, .. } | Suggestion::Tab { url, .. } | Suggestion::Visit { url } => url.as_str(),
                Suggestion::Search { query, .. } | Suggestion::Suggested { query, .. } => query.as_str(),
                Suggestion::Engine { keyword, .. } => keyword.as_str(),
            })
            .collect()
    }

    #[test]
    fn every_word_must_appear_in_the_address_or_the_title() {
        let words = words("Rust  BOOK");
        assert!(matches(&words, "https://doc.rust-lang.org/book/", ""));
        assert!(matches(&words, "https://example.test/", "The Rust Programming Book"));
        assert!(!matches(&words, "https://rust-lang.org/", "Rust"));
        assert!(!matches(&[], "https://rust-lang.org/", "Rust"));
    }

    #[test]
    fn typed_visits_outrank_more_clicked_ones() {
        let ranked = rank_pages(
            vec![
                page("https://clicked.test/", "x", 3, 0, 1),
                page("https://typed.test/", "x", 1, 1, 1),
            ],
            vec![],
            &words("x"),
            NOW,
            PAGES_MAX,
        );
        assert_eq!(urls(&ranked), ["https://typed.test/", "https://clicked.test/"]);
    }

    #[test]
    fn recent_visits_outrank_old_ones() {
        let ranked = rank_pages(
            vec![
                page("https://old.test/", "x", 5, 0, 200),
                page("https://recent.test/", "x", 2, 0, 2),
            ],
            vec![],
            &words("x"),
            NOW,
            PAGES_MAX,
        );
        assert_eq!(urls(&ranked), ["https://recent.test/", "https://old.test/"]);
    }

    #[test]
    fn a_match_at_the_start_of_the_host_or_a_title_word_ranks_first() {
        let ranked = rank_pages(
            vec![
                page("https://digits.test/", "Digits", 1, 0, 1),
                page("https://www.github.com/", "Home", 1, 0, 1),
                page("https://example.test/", "Learning Git", 1, 0, 1),
            ],
            vec![],
            &words("git"),
            NOW,
            PAGES_MAX,
        );
        assert_eq!(
            urls(&ranked),
            [
                "https://example.test/",
                "https://www.github.com/",
                "https://digits.test/"
            ]
        );
    }

    #[test]
    fn a_bookmark_never_visited_appears_and_one_visited_is_one_row_with_its_name() {
        let ranked = rank_pages(
            vec![page("https://both.test/", "Page title", 2, 0, 1)],
            vec![
                BookmarkCandidate {
                    url: "https://both.test/".into(),
                    title: "My name".into(),
                    added_at: NOW - 400 * DAY_MS,
                },
                BookmarkCandidate {
                    url: "https://never.test/".into(),
                    title: "Never".into(),
                    added_at: NOW - 400 * DAY_MS,
                },
            ],
            &words("e"),
            NOW,
            PAGES_MAX,
        );
        assert_eq!(
            ranked,
            [
                Suggestion::Page {
                    title: "My name".into(),
                    url: "https://both.test/".into(),
                    bookmarked: true
                },
                Suggestion::Page {
                    title: "Never".into(),
                    url: "https://never.test/".into(),
                    bookmarked: true
                },
            ]
        );
    }

    #[test]
    fn pages_stop_at_the_limit() {
        let history = (0..10)
            .map(|n| page(&format!("https://{n}.test/"), "x", 1, 0, 1))
            .collect();
        assert_eq!(
            rank_pages(history, vec![], &words("x"), NOW, PAGES_MAX).len(),
            PAGES_MAX
        );
    }

    #[test]
    fn tabs_match_at_word_starts_first_then_by_recency() {
        let tab = |id, url, title, active_at| TabCandidate {
            id: TabId(id),
            url,
            title,
            active_at,
        };
        let ranked = rank_tabs(
            [
                tab(1, "https://a.test/", "Inbox mail", 30),
                tab(2, "https://mail.test/", "x", 10),
                tab(3, "https://b.test/", "Gmail", 50),
                tab(4, "https://c.test/", "Other", 99),
            ],
            &words("mail"),
            TABS_MAX,
        );
        assert_eq!(
            urls(&ranked),
            ["https://a.test/", "https://mail.test/", "https://b.test/"]
        );
    }

    #[test]
    fn only_a_plain_word_completes_a_host() {
        assert_eq!(host_prefix("YouT").as_deref(), Some("yout"));
        assert_eq!(host_prefix("example.com/pa"), None);
        assert_eq!(host_prefix("two words"), None);
        assert_eq!(host_prefix("trailing "), None);
        assert_eq!(host_prefix("@g"), None);
        assert_eq!(host_prefix(""), None);
    }

    #[test]
    fn a_host_completes_past_its_www_and_never_to_itself() {
        assert_eq!(complete_host("you", "www.youtube.com").as_deref(), Some("youtube.com"));
        assert_eq!(
            complete_host("www.y", "www.youtube.com").as_deref(),
            Some("www.youtube.com")
        );
        assert_eq!(complete_host("youtube.com", "youtube.com"), None);
        assert_eq!(complete_host("tube", "youtube.com"), None);
    }

    #[test]
    fn typing_a_keyword_lists_the_engines_it_could_be() {
        let engine = |name: &str, keyword: Option<&str>| SearchEngine {
            id: name.into(),
            name: name.into(),
            url: "https://x.test/?q=%s".into(),
            suggest_url: None,
            keyword: keyword.map(Into::into),
        };
        let engines = [
            engine("Google", Some("@g")),
            engine("Brave", Some("@br")),
            engine("Bing", Some("@b")),
            engine("None", None),
            engine("Wiki", Some("w")),
        ];
        assert_eq!(urls(&keyword_rows("@", &engines)), ["@g", "@br", "@b"]);
        assert_eq!(urls(&keyword_rows("@B", &engines)), ["@br", "@b"]);
        assert!(keyword_rows("@g cats", &engines).is_empty());
        assert!(keyword_rows("w", &engines).is_empty());
    }

    #[test]
    fn completions_are_read_from_an_open_search_answer_and_nothing_else() {
        assert_eq!(
            parse_open_search(r#"["rust",[" rust book ","","rust lang",7],[],{"extra":1}]"#),
            vec!["rust book".to_string(), "rust lang".to_string()]
        );
        assert!(parse_open_search("[\"rust\"]").is_empty());
        assert!(parse_open_search(r#"{"results":["rust"]}"#).is_empty());
        assert!(parse_open_search("<html>").is_empty());
    }
}
