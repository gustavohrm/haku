use serde::{Deserialize, Serialize};

/// The extensions Haku installed into the profile, as last installed.
///
/// The engine lists its own built-in extensions alongside the user's, by id
/// and name only, and refuses to remove them. Remembering which ids came from
/// the extensions folder, and from which subfolder, is what lets an extension
/// whose folder was deleted be uninstalled without touching the engine's, and
/// without uninstalling one whose folder is still there but failed to install
/// this time.
///
/// Internal state rather than a setting: it is rewritten after every install
/// and is not meant to be edited.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExtensions {
    pub installed: Vec<InstalledExtension>,
}

/// One extension Haku installed, and the subfolder of the extensions folder
/// it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExtension {
    pub id: String,
    pub folder: String,
}

impl InstalledExtensions {
    /// The extensions installed before whose folder no longer exists, judged
    /// by `exists`.
    pub fn removed(&self, exists: impl Fn(&str) -> bool) -> Vec<InstalledExtension> {
        self.installed
            .iter()
            .filter(|extension| !exists(&extension.folder))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(id: &str, folder: &str) -> InstalledExtension {
        InstalledExtension {
            id: id.into(),
            folder: folder.into(),
        }
    }

    #[test]
    fn an_extension_whose_folder_is_gone_is_removed() {
        let before = InstalledExtensions {
            installed: vec![installed("kept", "ubol"), installed("gone", "bitwarden")],
        };
        assert_eq!(
            before.removed(|folder| folder == "ubol"),
            vec![installed("gone", "bitwarden")]
        );
    }

    #[test]
    fn an_extension_whose_folder_is_still_there_is_kept_even_if_it_failed_to_install() {
        let before = InstalledExtensions {
            installed: vec![installed("broken", "ubol")],
        };
        assert!(before.removed(|_| true).is_empty());
    }

    #[test]
    fn nothing_is_removed_before_anything_was_recorded() {
        assert!(InstalledExtensions::default().removed(|_| false).is_empty());
    }
}
