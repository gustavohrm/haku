//! What the user said about recording history: rules for URLs, and how long
//! history is kept.
//! [History § Rules](../../../docs/specs/history.md#rules).

use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::error::Result;
use crate::model::url_pattern::{CompiledPattern, UrlPattern};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HistoryRuleAction {
    /// The visit is not recorded.
    Never,
    /// The visit is recorded with an empty title, and never retitled.
    AddressOnly,
    /// The visit is recorded, whatever later rules say.
    Always,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRule {
    pub pattern: UrlPattern,
    pub action: HistoryRuleAction,
}

/// The rules in the order they are checked; the first that matches decides.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(transparent)]
#[specta(inline)]
pub struct HistoryRules(pub Vec<HistoryRule>);

/// Drops any rule that cannot be read or compiled, and reads a value that is
/// not a list as none, rather than failing the whole settings file over a
/// hand-edited pattern. Written by hand rather than with `deserialize_with`,
/// which splits `Settings` into a read and a written type in the bindings.
impl<'de> Deserialize<'de> for HistoryRules {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let serde_json::Value::Array(entries) = serde_json::Value::deserialize(deserializer)? else {
            return Ok(Self::default());
        };
        Ok(Self(
            entries
                .into_iter()
                .filter_map(|entry| serde_json::from_value::<HistoryRule>(entry).ok())
                .filter(|rule| rule.pattern.compile().is_ok())
                .collect(),
        ))
    }
}

impl Deref for HistoryRules {
    type Target = Vec<HistoryRule>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for HistoryRules {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl HistoryRules {
    /// Every rule compiled, in order.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::InvalidPattern`] for the first rule
    /// whose pattern cannot be used.
    pub fn compile(&self) -> Result<CompiledRules> {
        Ok(CompiledRules(
            self.iter()
                .map(|rule| Ok((rule.pattern.compile()?, rule.action)))
                .collect::<Result<_>>()?,
        ))
    }

    /// Writes each site as its bare host, drops rules that cannot be used,
    /// and keeps the first of rules with the same pattern, since a later one
    /// could never decide anything.
    pub fn normalized(self) -> Self {
        let mut kept: Vec<HistoryRule> = Vec::new();
        for rule in self.0 {
            let Some(pattern) = rule.pattern.normalized().filter(|pattern| pattern.compile().is_ok()) else {
                continue;
            };
            if kept.iter().all(|other| other.pattern != pattern) {
                kept.push(HistoryRule { pattern, ..rule });
            }
        }
        Self(kept)
    }

    /// Whether `next` holds a rule these do not. Removing or reordering is not
    /// adding, so the settings page may still do either from a private window.
    pub fn adds_rule(&self, next: &Self) -> bool {
        next.iter().any(|rule| !self.contains(rule))
    }
}

/// What recording does with a visit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recording {
    Full,
    AddressOnly,
    Skip,
}

/// The rules, compiled once for every visit checked against them.
#[derive(Clone, Debug, Default)]
pub struct CompiledRules(Vec<(CompiledPattern, HistoryRuleAction)>);

impl CompiledRules {
    /// What the first matching rule says about `url`, whose host is `host`; a
    /// URL no rule matches is recorded.
    pub fn decide(&self, url: &str, host: &str) -> Recording {
        match self.0.iter().find(|(pattern, _)| pattern.matches(url, host)) {
            Some((_, HistoryRuleAction::Never)) => Recording::Skip,
            Some((_, HistoryRuleAction::AddressOnly)) => Recording::AddressOnly,
            Some((_, HistoryRuleAction::Always)) | None => Recording::Full,
        }
    }
}

/// How long visits are kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HistoryRetention {
    #[default]
    Forever,
    Days30,
    Days90,
    Year,
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

impl HistoryRetention {
    /// The time before which visits are deleted, or nothing when they are
    /// kept forever.
    ///
    /// @param now - Milliseconds since the epoch.
    pub fn cutoff(self, now: i64) -> Option<i64> {
        let days = match self {
            Self::Forever => return None,
            Self::Days30 => 30,
            Self::Days90 => 90,
            Self::Year => 365,
        };
        Some(now - days * DAY_MS)
    }
}

/// An unknown value, such as a hand edit or one written by a later Haku,
/// reads as keeping history forever: deleting on a misread would lose data
/// that cannot be recovered.
impl<'de> Deserialize<'de> for HistoryRetention {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok([Self::Forever, Self::Days30, Self::Days90, Self::Year]
            .into_iter()
            .find(|choice| serde_json::to_value(choice).is_ok_and(|spelled| spelled == value))
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::url_pattern::UrlPatternKind;

    fn rule(kind: UrlPatternKind, value: &str, action: HistoryRuleAction) -> HistoryRule {
        HistoryRule {
            pattern: UrlPattern::new(kind, value),
            action,
        }
    }

    fn decide(rules: Vec<HistoryRule>, url: &str) -> Recording {
        let host = crate::model::host_of(url).unwrap_or_default();
        HistoryRules(rules).compile().unwrap().decide(url, &host)
    }

    #[test]
    fn a_url_no_rule_matches_is_recorded() {
        let rules = vec![rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Never)];
        assert_eq!(decide(rules, "https://b.test/"), Recording::Full);
    }

    #[test]
    fn each_action_decides_a_matching_url() {
        let url = "https://a.test/";
        let of = |action| decide(vec![rule(UrlPatternKind::Site, "a.test", action)], url);
        assert_eq!(of(HistoryRuleAction::Never), Recording::Skip);
        assert_eq!(of(HistoryRuleAction::AddressOnly), Recording::AddressOnly);
        assert_eq!(of(HistoryRuleAction::Always), Recording::Full);
    }

    #[test]
    fn the_first_matching_rule_decides() {
        let exception_first = vec![
            rule(UrlPatternKind::Wildcard, "*a.test/public*", HistoryRuleAction::Always),
            rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Never),
        ];
        assert_eq!(
            decide(exception_first.clone(), "https://a.test/public/x"),
            Recording::Full
        );
        assert_eq!(decide(exception_first, "https://a.test/private"), Recording::Skip);

        let broad_first = vec![
            rule(UrlPatternKind::Wildcard, "*a.test/private*", HistoryRuleAction::Never),
            rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Always),
        ];
        assert_eq!(decide(broad_first, "https://a.test/private"), Recording::Skip);
    }

    #[test]
    fn a_stored_rule_that_cannot_be_read_or_compiled_is_dropped() {
        let json = serde_json::json!([
            { "pattern": { "kind": "site", "value": "a.test" }, "action": "never" },
            { "pattern": { "kind": "regex", "value": "(" }, "action": "never" },
            { "pattern": { "kind": "glob", "value": "x" }, "action": "never" },
            "nonsense",
        ]);
        let read = HistoryRules::deserialize(json).unwrap();
        assert_eq!(
            read.0,
            vec![rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Never)]
        );
    }

    #[test]
    fn a_stored_value_that_is_not_a_list_reads_as_no_rules() {
        assert_eq!(
            HistoryRules::deserialize(serde_json::json!({})).unwrap(),
            HistoryRules::default()
        );
    }

    #[test]
    fn normalizing_writes_sites_as_hosts_and_keeps_the_first_of_a_repeated_pattern() {
        let rules = HistoryRules(vec![
            rule(UrlPatternKind::Site, "https://A.test/x", HistoryRuleAction::Never),
            rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Always),
            rule(UrlPatternKind::Page, " ", HistoryRuleAction::Never),
        ]);
        assert_eq!(
            rules.normalized().0,
            vec![rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Never)]
        );
    }

    #[test]
    fn removing_or_reordering_rules_adds_none() {
        let a = rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Never);
        let b = rule(UrlPatternKind::Site, "b.test", HistoryRuleAction::Never);
        let current = HistoryRules(vec![a.clone(), b.clone()]);
        assert!(!current.adds_rule(&HistoryRules(vec![b.clone(), a.clone()])));
        assert!(!current.adds_rule(&HistoryRules(vec![a.clone()])));
        let changed = HistoryRules(vec![rule(UrlPatternKind::Site, "a.test", HistoryRuleAction::Always), b]);
        assert!(current.adds_rule(&changed));
    }

    #[test]
    fn retention_deletes_before_its_age_and_never_when_forever() {
        let now = 400 * DAY_MS;
        assert_eq!(HistoryRetention::Forever.cutoff(now), None);
        assert_eq!(HistoryRetention::Days30.cutoff(now), Some(370 * DAY_MS));
        assert_eq!(HistoryRetention::Year.cutoff(now), Some(35 * DAY_MS));
    }

    #[test]
    fn an_unknown_stored_retention_keeps_history_forever() {
        let read = HistoryRetention::deserialize(serde_json::json!("days7")).unwrap();
        assert_eq!(read, HistoryRetention::Forever);
        let read = HistoryRetention::deserialize(serde_json::json!("days90")).unwrap();
        assert_eq!(read, HistoryRetention::Days90);
    }
}
