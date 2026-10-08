use serde::{Deserialize, Serialize};
use specta::Type;

/// An installed extension, as the interface lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub id: String,
    pub name: String,
    /// The page its toolbar button would open, as a `chrome-extension://`
    /// URL. The engine draws no toolbar, so it is opened as a tab instead.
    pub popup: Option<String>,
}

/// What the extensions folder holds, as last installed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
    pub installed: Vec<Extension>,
    /// Folders the engine would not install, by name, so a broken or
    /// unsupported extension is not silently missing.
    pub failed: Vec<String>,
}

/// A change that brings the profile in line with the extensions folder and
/// the settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionChange {
    /// Its folder is gone, so it is uninstalled.
    Remove(String),
    Enable(String, bool),
}

/// What to change so the profile holds exactly the extensions whose folders
/// are present, each enabled unless the settings disable it.
///
/// @param installed - The ids the profile held before the folders were added.
/// @param present - The ids the folders installed as.
/// @param disabled - The ids the settings disable.
pub fn reconcile(installed: &[String], present: &[String], disabled: &[String]) -> Vec<ExtensionChange> {
    let removed = installed
        .iter()
        .filter(|id| !present.contains(id))
        .map(|id| ExtensionChange::Remove(id.clone()));
    let toggled = present
        .iter()
        .map(|id| ExtensionChange::Enable(id.clone(), !disabled.contains(id)));
    removed.chain(toggled).collect()
}

/// The `chrome-extension://` URL of an extension's toolbar popup, read from
/// its manifest, or nothing when it declares none.
pub fn popup_url(id: &str, manifest: &str) -> Option<String> {
    let manifest: serde_json::Value = serde_json::from_str(manifest.trim_start_matches('\u{feff}')).ok()?;
    let action = manifest.get("action").or_else(|| manifest.get("browser_action"))?;
    let page = action.get("default_popup")?.as_str()?.trim_start_matches('/');
    (!page.is_empty()).then(|| format!("chrome-extension://{id}/{page}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn an_extension_whose_folder_is_gone_is_removed() {
        let changes = reconcile(&ids(&["a", "b"]), &ids(&["a"]), &[]);

        assert!(changes.contains(&ExtensionChange::Remove("b".into())));
        assert!(!changes.contains(&ExtensionChange::Remove("a".into())));
    }

    #[test]
    fn present_extensions_are_enabled_unless_the_settings_disable_them() {
        let changes = reconcile(&[], &ids(&["a", "b"]), &ids(&["b"]));

        assert_eq!(
            changes,
            [
                ExtensionChange::Enable("a".into(), true),
                ExtensionChange::Enable("b".into(), false)
            ]
        );
    }

    #[test]
    fn the_popup_comes_from_the_manifest_action() {
        let manifest = r#"{"manifest_version": 3, "action": {"default_popup": "popup/index.html"}}"#;
        assert_eq!(
            popup_url("abc", manifest).as_deref(),
            Some("chrome-extension://abc/popup/index.html")
        );
    }

    #[test]
    fn an_extension_without_a_popup_has_none() {
        assert_eq!(popup_url("abc", r#"{"action": {}}"#), None);
        assert_eq!(popup_url("abc", "not json"), None);
    }
}
