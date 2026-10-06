use serde::{Deserialize, Serialize};
use specta::Type;

/// One entry in a tab's back/forward stack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Visit {
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
}

impl Visit {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            title: url.clone(),
            url,
            favicon: None,
        }
    }
}

/// How a page moved to a new URL, as the page itself classifies it.
///
/// The distinction decides whether a tab's history grows: a link or a pushed
/// route adds an entry, a redirect or a replaced route does not, and a page's
/// own back or forward moves through entries that already exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationKind {
    Push,
    Replace,
    Traverse,
    Reload,
}

impl NavigationKind {
    /// Reads a Navigation API `navigationType`.
    ///
    /// Anything unrecognised is treated as a replacement: it keeps the URL
    /// accurate without inventing a history entry.
    pub fn parse(value: &str) -> Self {
        match value {
            "push" => Self::Push,
            "traverse" => Self::Traverse,
            "reload" => Self::Reload,
            _ => Self::Replace,
        }
    }
}

/// One URL change a webview committed, in the order it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub url: String,
    pub kind: NavigationKind,
}

/// A tab's navigation stack.
///
/// The stack behaves like a browser's: navigating from anywhere other than the
/// end truncates the forward entries, exactly as pressing back then following a
/// link does in any browser.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct History {
    entries: Vec<Visit>,
    /// Exported as a JavaScript number; a navigation stack never approaches the
    /// range where a double stops counting exactly.
    #[specta(type = specta_typescript::Number)]
    index: usize,
}

impl History {
    pub fn new(initial: Visit) -> Self {
        Self {
            entries: vec![initial],
            index: 0,
        }
    }

    pub fn current(&self) -> &Visit {
        // `entries` is never empty and `index` is only ever set to a valid
        // position, so this cannot panic.
        &self.entries[self.index]
    }

    pub fn entries(&self) -> &[Visit] {
        &self.entries
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn can_go_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    /// Pushes a new entry, discarding any forward history.
    ///
    /// Navigating to the URL already displayed is treated as a refresh and does
    /// not grow the stack, so reload loops cannot inflate history.
    pub fn push(&mut self, visit: Visit) {
        if self.current().url == visit.url {
            self.entries[self.index] = visit;
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(visit);
        self.index = self.entries.len() - 1;
    }

    pub fn go_back(&mut self) -> Option<&Visit> {
        if !self.can_go_back() {
            return None;
        }
        self.index -= 1;
        Some(self.current())
    }

    pub fn go_forward(&mut self) -> Option<&Visit> {
        if !self.can_go_forward() {
            return None;
        }
        self.index += 1;
        Some(self.current())
    }

    /// Moves to the neighbouring entry showing `url`, as a page's own back or
    /// forward does.
    ///
    /// Only the adjacent entries are considered: a page steps one entry at a
    /// time, and matching further away could jump to an unrelated visit that
    /// happens to share the URL.
    ///
    /// @returns Whether an adjacent entry matched.
    pub fn step_to(&mut self, url: &str) -> bool {
        if self.can_go_back() && self.entries[self.index - 1].url == url {
            self.index -= 1;
            return true;
        }
        if self.can_go_forward() && self.entries[self.index + 1].url == url {
            self.index += 1;
            return true;
        }
        false
    }

    /// Updates the metadata of the entry currently displayed.
    ///
    /// The page reports its own title and favicon after loading, and a redirect
    /// can change the URL of the entry that is already on the stack.
    pub fn update_current(&mut self, url: Option<String>, title: Option<String>, favicon: Option<String>) {
        let entry = &mut self.entries[self.index];
        if let Some(url) = url {
            entry.url = url;
        }
        if let Some(title) = title {
            entry.title = title;
        }
        if favicon.is_some() {
            entry.favicon = favicon;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_at(urls: &[&str]) -> History {
        let mut history = History::new(Visit::new(urls[0]));
        for url in &urls[1..] {
            history.push(Visit::new(*url));
        }
        history
    }

    #[test]
    fn starts_at_the_initial_entry_with_no_movement_available() {
        let history = History::new(Visit::new("https://a.test"));
        assert_eq!(history.current().url, "https://a.test");
        assert!(!history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn going_back_then_navigating_discards_forward_entries() {
        let mut history = history_at(&["https://a.test", "https://b.test", "https://c.test"]);
        history.go_back();
        history.push(Visit::new("https://d.test"));

        assert_eq!(history.current().url, "https://d.test");
        assert!(!history.can_go_forward());
        assert_eq!(history.entries().len(), 3);
    }

    #[test]
    fn navigating_to_the_current_url_refreshes_instead_of_growing_the_stack() {
        let mut history = history_at(&["https://a.test", "https://b.test"]);
        history.push(Visit::new("https://b.test"));

        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.index(), 1);
    }

    #[test]
    fn back_and_forward_stop_at_the_ends_without_moving() {
        let mut history = history_at(&["https://a.test", "https://b.test"]);

        assert!(history.go_back().is_some());
        assert!(history.go_back().is_none());
        assert_eq!(history.index(), 0);

        assert!(history.go_forward().is_some());
        assert!(history.go_forward().is_none());
        assert_eq!(history.index(), 1);
    }

    #[test]
    fn stepping_to_a_neighbour_moves_in_whichever_direction_holds_it() {
        let mut history = history_at(&["https://a.test", "https://b.test", "https://c.test"]);

        assert!(history.step_to("https://b.test"));
        assert_eq!(history.index(), 1);
        assert!(history.step_to("https://c.test"));
        assert_eq!(history.index(), 2);
    }

    #[test]
    fn stepping_to_a_url_that_is_not_adjacent_moves_nowhere() {
        let mut history = history_at(&["https://a.test", "https://b.test", "https://c.test"]);

        assert!(!history.step_to("https://a.test"));
        assert_eq!(history.index(), 2);
    }

    #[test]
    fn unknown_navigation_types_are_treated_as_replacements() {
        assert_eq!(NavigationKind::parse("push"), NavigationKind::Push);
        assert_eq!(NavigationKind::parse("traverse"), NavigationKind::Traverse);
        assert_eq!(NavigationKind::parse("something-new"), NavigationKind::Replace);
    }

    #[test]
    fn updating_metadata_leaves_unspecified_fields_untouched() {
        let mut history = History::new(Visit::new("https://a.test"));
        history.update_current(None, Some("Title".into()), Some("icon.png".into()));
        history.update_current(Some("https://a.test/redirected".into()), None, None);

        let current = history.current();
        assert_eq!(current.url, "https://a.test/redirected");
        assert_eq!(current.title, "Title");
        assert_eq!(current.favicon.as_deref(), Some("icon.png"));
    }
}
