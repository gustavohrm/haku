use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

/// Identifies one permission request a page made, so a late answer cannot
/// reach a newer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct PermissionRequestId(#[specta(type = specta_typescript::Number)] pub u64);

/// What a page asked to use. [Permissions § Kinds](../../../docs/specs/permissions.md#kinds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    Camera,
    Microphone,
    Location,
    Notifications,
    MidiSysex,
    Sensors,
    WindowManagement,
    LocalFonts,
    FileReadWrite,
    ClipboardRead,
    MultipleDownloads,
}

/// A permission a page is waiting on. The page keeps running meanwhile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: PermissionRequestId,
    pub kind: PermissionKind,
    /// The origin the request is decided for, which the dialog is titled with.
    pub origin: String,
}

/// A decision kept for an origin and kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PermissionState {
    Allow,
    Block,
}

/// A decision stored in settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SitePermission {
    pub origin: String,
    pub kind: PermissionKind,
    pub state: PermissionState,
}

/// What the user answered a permission request with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PermissionDecision {
    Allow,
    Block,
    /// Escape, or the dialog closing. Denied without storing anything.
    Dismiss,
}

impl PermissionDecision {
    /// The decision to keep, if any.
    pub fn state(self) -> Option<PermissionState> {
        match self {
            Self::Allow => Some(PermissionState::Allow),
            Self::Block => Some(PermissionState::Block),
            Self::Dismiss => None,
        }
    }
}

/// A web page's origin, `scheme://host[:port]`, lowercased, as `URL.origin`
/// gives it for `http` and `https`. A default port is left out. Nothing for
/// any other URL.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest, default_port) = if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest, "443")
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest, "80")
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?.to_ascii_lowercase();
    let host_port = match host_port.rsplit_once(':') {
        Some((host, port)) if port == default_port && !host.is_empty() => host.to_string(),
        _ => host_port,
    };
    if host_port.is_empty() || host_port.starts_with(':') {
        return None;
    }
    Some(format!("{scheme}://{host_port}"))
}

/// The decisions stored in settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(transparent)]
#[specta(inline)]
pub struct SitePermissions(pub Vec<SitePermission>);

/// Drops any entry that cannot be read rather than failing the whole settings
/// file over a hand edit or a kind since removed. Written by hand rather than
/// with `deserialize_with`, which splits `Settings` into a read and a written
/// type in the bindings.
impl<'de> Deserialize<'de> for SitePermissions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries = Vec::<serde_json::Value>::deserialize(deserializer)?;
        Ok(Self(
            entries
                .into_iter()
                .filter_map(|entry| serde_json::from_value(entry).ok())
                .collect(),
        ))
    }
}

impl Deref for SitePermissions {
    type Target = Vec<SitePermission>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SitePermissions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Lowercases stored origins and keeps one decision per origin and kind, the
/// last one listed.
pub fn normalized(permissions: Vec<SitePermission>) -> Vec<SitePermission> {
    let mut kept: Vec<SitePermission> = Vec::new();
    for mut permission in permissions {
        permission.origin = permission.origin.trim().to_ascii_lowercase();
        if permission.origin.is_empty() {
            continue;
        }
        kept.retain(|other| other.origin != permission.origin || other.kind != permission.kind);
        kept.push(permission);
    }
    kept
}

/// The decision listed for an origin and kind.
pub fn decision_for(permissions: &[SitePermission], origin: &str, kind: PermissionKind) -> Option<PermissionState> {
    permissions
        .iter()
        .find(|permission| permission.origin == origin && permission.kind == kind)
        .map(|permission| permission.state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_origin_keeps_its_scheme_and_a_port_other_than_the_default() {
        assert_eq!(
            origin_of("https://Meet.Example/room?x=1").as_deref(),
            Some("https://meet.example")
        );
        assert_eq!(
            origin_of("http://meet.example/").as_deref(),
            Some("http://meet.example")
        );
        assert_eq!(
            origin_of("http://localhost:8080/a").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(origin_of("https://a.test:443/").as_deref(), Some("https://a.test"));
        assert_eq!(origin_of("https://user:pw@a.test/").as_deref(), Some("https://a.test"));
        assert_eq!(origin_of("https://[::1]:8443/").as_deref(), Some("https://[::1]:8443"));
    }

    #[test]
    fn only_web_pages_have_an_origin() {
        assert_eq!(origin_of("file:///C:/a.html"), None);
        assert_eq!(origin_of("about:blank"), None);
        assert_eq!(origin_of("https:///path"), None);
    }

    #[test]
    fn an_unreadable_stored_entry_is_dropped_and_the_rest_kept() {
        let json = serde_json::json!([
            { "origin": "https://a.test", "kind": "camera", "state": "allow" },
            { "origin": "https://b.test", "kind": "teleport", "state": "allow" },
            "nonsense",
        ]);
        let read = SitePermissions::deserialize(json).unwrap().0;
        assert_eq!(
            read,
            vec![SitePermission {
                origin: "https://a.test".into(),
                kind: PermissionKind::Camera,
                state: PermissionState::Allow,
            }]
        );
    }

    #[test]
    fn one_decision_is_kept_per_origin_and_kind_the_last_listed() {
        let entry = |origin: &str, state| SitePermission {
            origin: origin.into(),
            kind: PermissionKind::Camera,
            state,
        };
        let kept = normalized(vec![
            entry(" HTTPS://A.test ", PermissionState::Allow),
            entry("https://a.test", PermissionState::Block),
            entry("  ", PermissionState::Allow),
        ]);
        assert_eq!(kept, vec![entry("https://a.test", PermissionState::Block)]);
    }
}
