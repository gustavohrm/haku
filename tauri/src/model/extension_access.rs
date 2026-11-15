//! What an extension's manifest says it may do, as the warnings shown before
//! it is installed. [Extensions § Before installing](../../../docs/specs/extensions.md#before-installing).
//!
//! Read from `permissions`, `host_permissions` and the pages its content
//! scripts run in. Optional permissions are asked for while it runs, so they
//! are not warned of here.

use serde::{Deserialize, Serialize};
use specta::Type;

/// One kind of access an extension asks for that the user is warned of.
/// Permissions that give no access worth a warning, such as `storage` or
/// `activeTab`, have none.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "hosts", rename_all = "camelCase")]
pub enum ExtensionAccess {
    /// Reading and changing every site, from `<all_urls>`, a pattern for any
    /// host, or `proxy`, which sees all traffic.
    AllSites,
    /// Reading and changing these hosts, as their patterns name them, such as
    /// `*.example.com`.
    Sites(Vec<String>),
    /// `history`.
    ChangeHistory,
    /// `tabs`, `webNavigation`, `sessions` or `declarativeNetRequestFeedback`,
    /// which see the addresses visited.
    ReadHistory,
    TopSites,
    Bookmarks,
    Downloads,
    /// `browsingData`: clearing history, cookies, cache and stored data.
    /// Chrome does not warn about it; Haku does, as it can wipe the profile.
    BrowsingData,
    ReadClipboard,
    WriteClipboard,
    /// `declarativeNetRequest`.
    BlockContent,
    /// `tabCapture`, `desktopCapture` or `pageCapture`.
    Capture,
    Location,
    Notifications,
    /// `identity.email`.
    Email,
    Privacy,
    ContentSettings,
    /// `management`: other extensions.
    Management,
    /// `nativeMessaging`: programs installed on the computer.
    NativeMessaging,
    Debugger,
}

/// An access warning as shown before an extension is added.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccessWarning {
    pub access: ExtensionAccess,
    /// For an update, whether the installed version did not ask for it, or,
    /// for sites, for one of them.
    pub new: bool,
}

/// The access `manifest` asks for, in the order it is shown: sites first.
pub fn access(manifest: &str) -> Vec<ExtensionAccess> {
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(manifest.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    let strings = |value: Option<&serde_json::Value>| -> Vec<String> {
        value
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let permissions = strings(manifest.get("permissions"));
    // Manifest V2 lists hosts among its permissions.
    let mut patterns = strings(manifest.get("host_permissions"));
    patterns.extend(
        permissions
            .iter()
            .filter(|permission| permission.contains("://") || *permission == "<all_urls>")
            .cloned(),
    );
    if let Some(scripts) = manifest.get("content_scripts").and_then(serde_json::Value::as_array) {
        for script in scripts {
            patterns.extend(strings(script.get("matches")));
        }
    }

    let mut all_sites = permissions.iter().any(|permission| permission == "proxy");
    let mut hosts = Vec::new();
    for pattern in &patterns {
        match host_of(pattern) {
            Some(Host::Any) => all_sites = true,
            Some(Host::Named(host)) => hosts.push(host),
            None => {}
        }
    }

    let mut found: Vec<ExtensionAccess> = permissions.iter().filter_map(|permission| warned(permission)).collect();
    if found.contains(&ExtensionAccess::ChangeHistory) {
        found.retain(|access| *access != ExtensionAccess::ReadHistory);
    }
    if all_sites {
        found.push(ExtensionAccess::AllSites);
    } else if !hosts.is_empty() {
        hosts.sort();
        hosts.dedup();
        found.push(ExtensionAccess::Sites(hosts));
    }
    found.sort();
    found.dedup();
    found
}

/// The warnings to show for `manifest`, each marked new when `installed`, the
/// manifest of the version it updates, did not ask for it.
pub fn warnings(manifest: &str, installed: Option<&str>) -> Vec<AccessWarning> {
    let before = installed.map(access);
    access(manifest)
        .into_iter()
        .map(|access| {
            let new = before.as_ref().is_some_and(|before| !covered(&access, before));
            AccessWarning { access, new }
        })
        .collect()
}

/// Whether `before` already gave all of `access`.
fn covered(access: &ExtensionAccess, before: &[ExtensionAccess]) -> bool {
    match access {
        ExtensionAccess::Sites(hosts) => {
            before.contains(&ExtensionAccess::AllSites)
                || before.iter().any(|old| match old {
                    ExtensionAccess::Sites(old) => {
                        hosts.iter().all(|host| old.iter().any(|old| host_covers(old, host)))
                    }
                    _ => false,
                })
        }
        ExtensionAccess::ReadHistory => {
            before.contains(&ExtensionAccess::ReadHistory) || before.contains(&ExtensionAccess::ChangeHistory)
        }
        access => before.contains(access),
    }
}

/// Whether the host pattern `old` reaches every host `new` does: the same
/// pattern, or `*.` a domain that `new` is, or is under. A bare host does not
/// reach `*.` itself, which adds its subdomains.
fn host_covers(old: &str, new: &str) -> bool {
    old == new
        || old.strip_prefix("*.").is_some_and(|domain| {
            let new = new.strip_prefix("*.").unwrap_or(new);
            new == domain || new.ends_with(&format!(".{domain}"))
        })
}

fn warned(permission: &str) -> Option<ExtensionAccess> {
    Some(match permission {
        "history" => ExtensionAccess::ChangeHistory,
        "tabs" | "webNavigation" | "sessions" | "declarativeNetRequestFeedback" => ExtensionAccess::ReadHistory,
        "topSites" => ExtensionAccess::TopSites,
        "bookmarks" => ExtensionAccess::Bookmarks,
        "downloads" => ExtensionAccess::Downloads,
        "browsingData" => ExtensionAccess::BrowsingData,
        "clipboardRead" => ExtensionAccess::ReadClipboard,
        "clipboardWrite" => ExtensionAccess::WriteClipboard,
        "declarativeNetRequest" => ExtensionAccess::BlockContent,
        "tabCapture" | "desktopCapture" | "pageCapture" => ExtensionAccess::Capture,
        "geolocation" => ExtensionAccess::Location,
        "notifications" => ExtensionAccess::Notifications,
        "identity.email" => ExtensionAccess::Email,
        "privacy" => ExtensionAccess::Privacy,
        "contentSettings" => ExtensionAccess::ContentSettings,
        "management" => ExtensionAccess::Management,
        "nativeMessaging" => ExtensionAccess::NativeMessaging,
        "debugger" => ExtensionAccess::Debugger,
        _ => return None,
    })
}

enum Host {
    Any,
    Named(String),
}

/// The host a match pattern gives access to, or nothing for a scheme that
/// reaches no site, such as `file` or `chrome`.
fn host_of(pattern: &str) -> Option<Host> {
    if pattern == "<all_urls>" {
        return Some(Host::Any);
    }
    let (scheme, rest) = pattern.split_once("://")?;
    if !matches!(scheme, "*" | "http" | "https" | "ws" | "wss") {
        return None;
    }
    let host = rest.split('/').next()?;
    // A port does not change which site it is.
    let host = host.rsplit_once(':').map_or(host, |(host, _)| host);
    match host {
        "" => None,
        "*" => Some(Host::Any),
        host => Some(Host::Named(host.to_ascii_lowercase())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_come_from_host_permissions_v2_permissions_and_content_scripts() {
        let manifest = r#"{
            "permissions": ["storage", "https://mail.example.com/*"],
            "host_permissions": ["*://*.example.org/*", "https://a.test:8080/"],
            "content_scripts": [{"matches": ["https://mail.example.com/inbox/*", "file:///*"]}]
        }"#;
        assert_eq!(
            access(manifest),
            [ExtensionAccess::Sites(vec![
                "*.example.org".into(),
                "a.test".into(),
                "mail.example.com".into(),
            ])]
        );
    }

    #[test]
    fn any_host_is_every_site_and_hides_the_named_ones() {
        for pattern in ["<all_urls>", "*://*/*", "https://*/*"] {
            let manifest = format!(r#"{{"host_permissions": ["https://a.test/*", "{pattern}"]}}"#);
            assert_eq!(access(&manifest), [ExtensionAccess::AllSites], "{pattern}");
        }
        assert_eq!(access(r#"{"permissions": ["proxy"]}"#), [ExtensionAccess::AllSites]);
    }

    #[test]
    fn permissions_without_a_warning_give_none() {
        let manifest = r#"{"permissions": ["storage", "activeTab", "scripting", "alarms", "contextMenus"]}"#;
        assert!(access(manifest).is_empty());
        assert!(access("not json").is_empty());
    }

    #[test]
    fn warnings_are_ordered_with_sites_first_and_history_said_once() {
        let manifest = r#"{"permissions": ["tabs", "history", "nativeMessaging", "<all_urls>", "tabs"]}"#;
        assert_eq!(
            access(manifest),
            [
                ExtensionAccess::AllSites,
                ExtensionAccess::ChangeHistory,
                ExtensionAccess::NativeMessaging,
            ]
        );
    }

    #[test]
    fn an_update_marks_only_what_the_installed_version_lacked() {
        let installed = r#"{"permissions": ["tabs"], "host_permissions": ["https://a.test/*"]}"#;
        let update = r#"{"permissions": ["tabs", "history", "downloads"], "host_permissions": ["https://a.test/*"]}"#;
        assert_eq!(
            warnings(update, Some(installed)),
            [
                AccessWarning {
                    access: ExtensionAccess::Sites(vec!["a.test".into()]),
                    new: false
                },
                AccessWarning {
                    access: ExtensionAccess::ChangeHistory,
                    new: true
                },
                AccessWarning {
                    access: ExtensionAccess::Downloads,
                    new: true
                },
            ]
        );

        let more_sites = r#"{"host_permissions": ["https://a.test/*", "https://b.test/*"]}"#;
        assert!(warnings(more_sites, Some(installed))[0].new);
        assert!(!warnings(more_sites, Some(r#"{"host_permissions": ["<all_urls>"]}"#))[0].new);
        assert!(!warnings(r#"{"permissions": ["tabs"]}"#, Some(r#"{"permissions": ["history"]}"#))[0].new);
    }

    #[test]
    fn a_host_under_a_domain_already_reached_is_not_new() {
        let before = Some(r#"{"host_permissions": ["*://*.example.com/*"]}"#);
        let new = |manifest| warnings(manifest, before)[0].new;
        assert!(!new(r#"{"host_permissions": ["https://mail.example.com/*"]}"#));
        assert!(!new(r#"{"host_permissions": ["https://example.com/*"]}"#));
        assert!(!new(r#"{"host_permissions": ["https://*.mail.example.com/*"]}"#));
        assert!(new(r#"{"host_permissions": ["https://notexample.com/*"]}"#));
        assert!(new(r#"{"host_permissions": ["https://example.org/*"]}"#));
        let bare = Some(r#"{"host_permissions": ["https://example.com/*"]}"#);
        assert!(warnings(r#"{"host_permissions": ["https://*.example.com/*"]}"#, bare)[0].new);
        let narrower = Some(r#"{"host_permissions": ["https://mail.example.com/*"]}"#);
        assert!(warnings(r#"{"host_permissions": ["*://*.example.com/*"]}"#, narrower)[0].new);
    }

    #[test]
    fn browsing_data_is_warned_about() {
        assert_eq!(
            access(r#"{"permissions": ["browsingData"]}"#),
            [ExtensionAccess::BrowsingData]
        );
    }

    #[test]
    fn a_new_extension_marks_nothing_new() {
        assert!(warnings(r#"{"permissions": ["history"]}"#, None)
            .iter()
            .all(|warning| !warning.new));
    }
}
