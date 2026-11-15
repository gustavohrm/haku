//! The stores extensions are added from, and where each serves an extension.
//! [Extensions § From the stores](../../../docs/specs/extensions.md#from-the-stores).

use serde::{Deserialize, Serialize};
use specta::Type;

/// A store Haku adds extensions from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionStore {
    /// The Chrome Web Store.
    Chrome,
    /// Microsoft Edge Add-ons.
    Edge,
}

impl ExtensionStore {
    /// The store's update service, as the store writes it into the
    /// `update_url` of the extensions it serves.
    pub const fn update_service(self) -> &'static str {
        match self {
            Self::Chrome => "https://clients2.google.com/service/update2/crx",
            Self::Edge => "https://edge.microsoft.com/extensionwebstorebase/v1/crx",
        }
    }

    /// The store's name, as an error tells the user.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Chrome => "the Chrome Web Store",
            Self::Edge => "Microsoft Edge Add-ons",
        }
    }

    /// The query every request to the update service starts with: the CRX3
    /// format, for an engine of `engine_version`, which the service needs to
    /// choose a build.
    fn query(self, response: &str, engine_version: &str) -> String {
        let product = match self {
            Self::Chrome => "",
            Self::Edge => "&prod=chromiumcrx",
        };
        format!(
            "{}?response={response}&acceptformat=crx3{product}&prodversion={engine_version}",
            self.update_service()
        )
    }

    /// Where the store serves extension `id` as a CRX3 file, for an engine of
    /// `engine_version`: the newest version that engine runs.
    pub fn download_url(self, id: &str, engine_version: &str) -> String {
        format!(
            "{}&x=id%3D{id}%26installsource%3Dondemand%26uc",
            self.query("redirect", engine_version)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ddkjiahejlhfcafbddmgiahcphecmpfh";

    #[test]
    fn each_store_serves_an_extension_for_the_engine_version() {
        assert_eq!(
            ExtensionStore::Chrome.download_url(ID, "155.0.4283.62"),
            "https://clients2.google.com/service/update2/crx?response=redirect&acceptformat=crx3\
             &prodversion=155.0.4283.62&x=id%3Dddkjiahejlhfcafbddmgiahcphecmpfh%26installsource%3Dondemand%26uc"
        );
        assert_eq!(
            ExtensionStore::Edge.download_url(ID, "155.0.4283.62"),
            "https://edge.microsoft.com/extensionwebstorebase/v1/crx?response=redirect&acceptformat=crx3\
             &prod=chromiumcrx&prodversion=155.0.4283.62\
             &x=id%3Dddkjiahejlhfcafbddmgiahcphecmpfh%26installsource%3Dondemand%26uc"
        );
    }
}
