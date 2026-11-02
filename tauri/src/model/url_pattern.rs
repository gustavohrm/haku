//! Patterns that pick out URLs: one type for history rules, history filters
//! and deleting history, evaluated by one matcher, so what a rule stops, what
//! a filter shows and what a delete removes cannot disagree.
//! [History § Patterns](../../../docs/specs/history.md#patterns).

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{HakuError, Result};
use crate::model::tab::host_of;

/// The longest pattern accepted, in characters, so that no pattern makes
/// every visit slow to check.
pub const PATTERN_MAX_CHARS: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UrlPatternKind {
    /// That exact URL.
    Page,
    /// A host and every subdomain of it.
    Site,
    /// The whole URL, where `*` stands for any run of characters.
    Wildcard,
    /// A regular expression found anywhere in the URL.
    Regex,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UrlPattern {
    pub kind: UrlPatternKind,
    pub value: String,
}

impl UrlPattern {
    pub fn new(kind: UrlPatternKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
    }

    /// The pattern as it is stored and compared: a site as its bare host,
    /// anything else as written.
    ///
    /// @returns Nothing when a site names no host.
    pub fn normalized(&self) -> Option<Self> {
        match self.kind {
            UrlPatternKind::Site => site_host(&self.value).map(|host| Self::new(UrlPatternKind::Site, host)),
            _ => Some(self.clone()),
        }
    }

    /// Compiles the pattern for matching.
    ///
    /// # Errors
    /// Returns [`HakuError::InvalidPattern`] when the pattern is empty, longer
    /// than [`PATTERN_MAX_CHARS`], names no host as a site, or is not a valid
    /// regular expression within the crate's size limit.
    pub fn compile(&self) -> Result<CompiledPattern> {
        let value = self.value.trim();
        if value.is_empty() {
            return Err(HakuError::InvalidPattern("the pattern is empty".into()));
        }
        if value.chars().count() > PATTERN_MAX_CHARS {
            return Err(HakuError::InvalidPattern(format!(
                "the pattern is longer than {PATTERN_MAX_CHARS} characters"
            )));
        }
        Ok(match self.kind {
            UrlPatternKind::Page => CompiledPattern::Page(value.to_string()),
            UrlPatternKind::Site => CompiledPattern::Site(
                site_host(value).ok_or_else(|| HakuError::InvalidPattern(format!("`{value}` names no site")))?,
            ),
            UrlPatternKind::Wildcard => CompiledPattern::Expression(wildcard(value)?),
            UrlPatternKind::Regex => CompiledPattern::Expression(
                RegexBuilder::new(value)
                    .build()
                    .map_err(|error| HakuError::InvalidPattern(error.to_string()))?,
            ),
        })
    }
}

/// A pattern ready to match.
#[derive(Clone, Debug)]
pub enum CompiledPattern {
    Page(String),
    /// A lowercase host.
    Site(String),
    Expression(Regex),
}

impl CompiledPattern {
    /// Whether the pattern picks out `url`, whose host, as [`host_of`] gives
    /// it, is `host`. Taking the host apart lets a caller that stores it
    /// avoid parsing every URL again.
    pub fn matches(&self, url: &str, host: &str) -> bool {
        match self {
            Self::Page(page) => url == page,
            Self::Site(site) => {
                !host.is_empty()
                    && (host == site || host.strip_suffix(site.as_str()).is_some_and(|rest| rest.ends_with('.')))
            }
            Self::Expression(expression) => expression.is_match(url),
        }
    }
}

/// Whether any of `patterns` picks out `url`.
pub fn any_matches(patterns: &[CompiledPattern], url: &str, host: &str) -> bool {
    patterns.iter().any(|pattern| pattern.matches(url, host))
}

/// The host a site pattern names, written as a host or as any URL.
fn site_host(value: &str) -> Option<String> {
    let value = value.trim();
    // Only the host of a URL counts, whatever its scheme or case.
    let value = value.split_once("://").map_or(value, |(_, rest)| rest);
    // `*.example.com` and `.example.com` already mean what a site means.
    let bare = value.trim_start_matches("*.").trim_start_matches('.');
    if bare.contains(['*', ' ']) {
        return None;
    }
    host_of(&format!("https://{bare}")).filter(|host| !host.is_empty())
}

/// A wildcard as an anchored, case-insensitive expression.
fn wildcard(value: &str) -> Result<Regex> {
    let body: Vec<String> = value.split('*').map(regex::escape).collect();
    RegexBuilder::new(&format!("^{}$", body.join(".*")))
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .map_err(|error| HakuError::InvalidPattern(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(kind: UrlPatternKind, value: &str, url: &str) -> bool {
        let host = host_of(url).unwrap_or_default();
        UrlPattern::new(kind, value).compile().unwrap().matches(url, &host)
    }

    #[test]
    fn a_page_matches_only_its_exact_url() {
        assert!(matches(
            UrlPatternKind::Page,
            "https://a.test/x?y=1",
            "https://a.test/x?y=1"
        ));
        assert!(!matches(
            UrlPatternKind::Page,
            "https://a.test/x",
            "https://a.test/x?y=1"
        ));
    }

    #[test]
    fn a_site_matches_its_host_and_every_subdomain() {
        assert!(matches(UrlPatternKind::Site, "example.com", "https://example.com/a"));
        assert!(matches(UrlPatternKind::Site, "example.com", "http://mail.example.com/"));
        assert!(matches(
            UrlPatternKind::Site,
            "example.com",
            "https://a.b.example.com:8080/"
        ));
    }

    #[test]
    fn a_site_does_not_match_a_host_that_merely_ends_with_its_name() {
        assert!(!matches(UrlPatternKind::Site, "example.com", "https://notexample.com/"));
        assert!(!matches(
            UrlPatternKind::Site,
            "mail.example.com",
            "https://example.com/"
        ));
    }

    #[test]
    fn a_site_may_be_written_as_a_url_in_any_case() {
        assert!(matches(
            UrlPatternKind::Site,
            "HTTPS://Example.COM/path",
            "https://www.example.com/"
        ));
        assert!(matches(
            UrlPatternKind::Site,
            "*.example.com",
            "https://www.example.com/"
        ));
    }

    #[test]
    fn a_site_never_matches_a_url_without_a_host() {
        assert!(!matches(UrlPatternKind::Site, "example.com", "file:///C:/example.com"));
    }

    #[test]
    fn a_site_naming_no_host_is_refused() {
        assert!(UrlPattern::new(UrlPatternKind::Site, "https://").compile().is_err());
        assert!(UrlPattern::new(UrlPatternKind::Site, "a*b").compile().is_err());
    }

    #[test]
    fn a_wildcard_must_match_the_whole_url() {
        assert!(matches(
            UrlPatternKind::Wildcard,
            "*://*.bank.com/*",
            "https://my.bank.com/accounts"
        ));
        assert!(!matches(
            UrlPatternKind::Wildcard,
            "*://*.bank.com/*",
            "https://bank.com.evil.test/"
        ));
        assert!(!matches(UrlPatternKind::Wildcard, "https://a.test", "https://a.test/x"));
    }

    #[test]
    fn a_wildcard_star_matches_any_run_including_none() {
        assert!(matches(UrlPatternKind::Wildcard, "*token=*", "https://a.test/?token="));
        assert!(matches(UrlPatternKind::Wildcard, "https://a.test/*", "https://a.test/"));
    }

    #[test]
    fn a_wildcard_ignores_case() {
        assert!(matches(UrlPatternKind::Wildcard, "*SECRET*", "https://a.test/secret"));
    }

    #[test]
    fn a_wildcard_treats_every_other_character_literally() {
        assert!(matches(
            UrlPatternKind::Wildcard,
            "https://a.test/?q=(a+b)",
            "https://a.test/?q=(a+b)"
        ));
        assert!(!matches(
            UrlPatternKind::Wildcard,
            "https://a.test/?",
            "https://a.test/x"
        ));
        assert!(!matches(
            UrlPatternKind::Wildcard,
            "https://a.test/x.y",
            "https://a.test/xzy"
        ));
    }

    #[test]
    fn a_regex_matches_anywhere_unless_anchored() {
        assert!(matches(
            UrlPatternKind::Regex,
            r"/admin\b",
            "https://a.test/admin/users"
        ));
        assert!(!matches(UrlPatternKind::Regex, r"^/admin", "https://a.test/admin"));
    }

    #[test]
    fn a_regex_is_case_sensitive_unless_it_says_otherwise() {
        assert!(!matches(UrlPatternKind::Regex, "SECRET", "https://a.test/secret"));
        assert!(matches(UrlPatternKind::Regex, "(?i)SECRET", "https://a.test/secret"));
    }

    #[test]
    fn an_invalid_regex_is_refused_with_its_reason() {
        let refused = UrlPattern::new(UrlPatternKind::Regex, "(unclosed").compile();
        assert!(matches!(refused, Err(HakuError::InvalidPattern(reason)) if !reason.is_empty()));
    }

    #[test]
    fn a_regex_too_large_to_compile_is_refused() {
        let huge = UrlPattern::new(UrlPatternKind::Regex, r"\w{1000}\w{1000}");
        assert!(matches!(huge.compile(), Err(HakuError::InvalidPattern(_))));
    }

    #[test]
    fn an_empty_or_oversized_pattern_is_refused() {
        assert!(UrlPattern::new(UrlPatternKind::Page, "  ").compile().is_err());
        let long = "a".repeat(PATTERN_MAX_CHARS + 1);
        assert!(UrlPattern::new(UrlPatternKind::Wildcard, long).compile().is_err());
    }

    #[test]
    fn a_site_is_stored_as_its_bare_host() {
        let site = UrlPattern::new(UrlPatternKind::Site, " https://Mail.Example.com/inbox ");
        assert_eq!(site.normalized().unwrap().value, "mail.example.com");
        assert_eq!(UrlPattern::new(UrlPatternKind::Site, "*").normalized(), None);
    }
}
