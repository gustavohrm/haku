//! The stores extensions are added from and updated from: where each serves
//! an extension, how it is asked for updates, and what it answers.
//! [Extensions § From the stores](../../../docs/specs/extensions.md#from-the-stores)
//! and [§ Updates from the stores](../../../docs/specs/extensions.md#updates-from-the-stores).

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::HakuError;
use crate::model::AddedExtension;

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
    pub const ALL: [Self; 2] = [Self::Chrome, Self::Edge];

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

    /// The store whose update service `manifest`'s `update_url` is, exactly.
    pub fn of_manifest(manifest: &str) -> Option<Self> {
        let manifest: serde_json::Value = serde_json::from_str(manifest.trim_start_matches('\u{feff}')).ok()?;
        let url = manifest.get("update_url")?.as_str()?;
        Self::ALL.into_iter().find(|store| store.update_service() == url)
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

    /// Where the store is asked whether `installed`, each an id and the
    /// version installed, have newer versions, for an engine of
    /// `engine_version`. A version is only digits and dots, and an id only
    /// letters, so neither needs escaping beyond the `x` parameter's own.
    pub fn update_check_url<'a>(
        self,
        installed: impl IntoIterator<Item = (&'a str, &'a str)>,
        engine_version: &str,
    ) -> String {
        let mut url = self.query("updatecheck", engine_version);
        for (id, version) in installed {
            url.push_str(&format!("&x=id%3D{id}%26v%3D{version}%26uc"));
        }
        url
    }
}

/// A newer version a store offers of an installed extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub id: String,
    pub version: String,
    /// Where to download it.
    pub url: String,
    /// Its SHA-256 in lower-case hex, when the store gives one.
    pub sha256: Option<String>,
}

/// The updates an update service's answer offers: for each extension, its
/// `updatecheck` when its status is `ok`, with where to download it and a
/// version. Extensions with no update, or one the store does not know, are
/// left out, as is anything that is not the answer expected.
pub fn parse_update_check(answer: &str) -> Vec<AvailableUpdate> {
    answer
        .split("<app ")
        .skip(1)
        .filter_map(|app| {
            let id = attribute(tag(app)?, "appid")?;
            let check = tag(&app[app.find("<updatecheck")? + "<updatecheck".len()..])?;
            if attribute(check, "status")? != "ok" {
                return None;
            }
            let url = attribute(check, "codebase")?;
            let version = attribute(check, "version")?;
            if !is_version(&version) || !(url.starts_with("https://") || url.starts_with("http://")) {
                return None;
            }
            let sha256 = attribute(check, "hash_sha256")
                .map(|hash| hash.to_ascii_lowercase())
                .filter(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
            Some(AvailableUpdate {
                id,
                version,
                url,
                sha256,
            })
        })
        .collect()
}

/// A tag's attributes: what follows its name, up to its end.
fn tag(from: &str) -> Option<&str> {
    from.find('>').map(|end| from[..end].trim_end_matches('/'))
}

/// The value of attribute `name` in `attributes`, with the entities XML
/// escapes in it written out.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let mut rest = attributes;
    loop {
        let at = rest.find(&format!("{name}=\""))?;
        let before = rest[..at].chars().next_back();
        rest = &rest[at + name.len() + 2..];
        // `name` must be a whole attribute name, not the end of another.
        if before.is_none_or(char::is_whitespace) {
            let value = &rest[..rest.find('"')?];
            return Some(
                value
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&"),
            );
        }
    }
}

/// Whether `version` is an extension version: one to four numbers,
/// separated by dots.
pub fn is_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() <= 4
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// Compares two extension versions number by number, a missing number
/// counting as 0, as Chrome does.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let numbers = |version: &str| -> Vec<u64> { version.split('.').map(|part| part.parse().unwrap_or(0)).collect() };
    let (a, b) = (numbers(a), numbers(b));
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
        .find(|order| order.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// A newer version of an installed extension that asks for access the
/// installed version did not, waiting for the user to review it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionUpdate {
    pub id: String,
    /// The folder of the installed extension it updates.
    pub folder: String,
    /// The version waiting.
    pub version: String,
    /// The store it comes from, which **Review update** downloads it from.
    pub store: ExtensionStore,
}

/// What a check of the stores did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionUpdateCheck {
    /// The extensions updated, each with the version it replaced.
    pub updated: Vec<AddedExtension>,
    /// The updates that ask for new access, waiting for review.
    pub waiting: Vec<ExtensionUpdate>,
    /// The first failure, if any; the rest of the check went on.
    pub error: Option<HakuError>,
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

    #[test]
    fn an_update_check_names_each_extension_and_its_version() {
        let url = ExtensionStore::Chrome.update_check_url([(ID, "1.2"), ("a".repeat(32).as_str(), "3")], "155.0");
        assert_eq!(
            url,
            format!(
                "https://clients2.google.com/service/update2/crx?response=updatecheck&acceptformat=crx3\
                 &prodversion=155.0&x=id%3D{ID}%26v%3D1.2%26uc&x=id%3D{}%26v%3D3%26uc",
                "a".repeat(32)
            )
        );
    }

    #[test]
    fn the_store_is_read_from_the_update_url_exactly() {
        let with = |url: &str| format!(r#"{{"update_url": "{url}"}}"#);
        assert_eq!(
            ExtensionStore::of_manifest(&with("https://clients2.google.com/service/update2/crx")),
            Some(ExtensionStore::Chrome)
        );
        assert_eq!(
            ExtensionStore::of_manifest(&with("https://edge.microsoft.com/extensionwebstorebase/v1/crx")),
            Some(ExtensionStore::Edge)
        );
        assert_eq!(
            ExtensionStore::of_manifest(&with("https://clients2.google.com.evil.example/service/update2/crx")),
            None
        );
        assert_eq!(ExtensionStore::of_manifest("{}"), None);
    }

    /// Answers as the stores gave them on 2026-11-15, shortened.
    #[test]
    fn an_update_check_answer_lists_the_updates_offered() {
        let chrome = r#"<?xml version="1.0" encoding="UTF-8"?><gupdate xmlns="http://www.google.com/update2/response" protocol="2.0" server="prod"><daystart elapsed_days="7258" elapsed_seconds="25128"/><app appid="ddkjiahejlhfcafbddmgiahcphecmpfh" cohort="1::" cohortname="" status="ok"><updatecheck _esbAllowlist="true" codebase="https://clients2.googleusercontent.com/crx/blobs/AR/X_2026_1110_1816_0.crx" fp="1.f830" hash_sha256="F830f9b2ac6c40dce7f193717fd286a4c966e8999018d5a3e75998d81751bb8a" protected="0" size="9079498" status="ok" version="2026.1110.1816"/></app><app appid="nngceckbapebfimnlniiiahkandclblb" cohort="1::" cohortname="" status="ok"><updatecheck _esbAllowlist="true" status="noupdate"/></app><app appid="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" status="error-unknownApplication"/></gupdate>"#;
        assert_eq!(
            parse_update_check(chrome),
            vec![AvailableUpdate {
                id: ID.into(),
                version: "2026.1110.1816".into(),
                url: "https://clients2.googleusercontent.com/crx/blobs/AR/X_2026_1110_1816_0.crx".into(),
                sha256: Some("f830f9b2ac6c40dce7f193717fd286a4c966e8999018d5a3e75998d81751bb8a".into()),
            }]
        );
        let edge = r#"<?xml version="1.0" encoding="utf-8"?><gupdate server="prod" protocol="2.0" xmlns="http://www.google.com/update2/response"><app appid="odfafepnkmbhccpbejgmiehpchacaeak" status="ok"><updatecheck status="ok" codebase="http://msedgeextensions.b.tlu.dl.delivery.mp.microsoft.com/filestreamingservice/files/b5f2?P1=1795342650&amp;P2=404" version="2026.1113.76.0" /></app><app appid="ifoakfbpdcdoeenechcleahebpibofpc" status="ok"><updatecheck status="noupdate" /></app></gupdate>"#;
        let offered = parse_update_check(edge);
        assert_eq!(offered.len(), 1);
        assert_eq!(
            offered[0].url,
            "http://msedgeextensions.b.tlu.dl.delivery.mp.microsoft.com/filestreamingservice/files/b5f2?P1=1795342650&P2=404"
        );
        assert_eq!(offered[0].sha256, None);
    }

    #[test]
    fn an_answer_that_is_not_one_offers_nothing() {
        assert!(parse_update_check("").is_empty());
        assert!(parse_update_check("<html>not found</html>").is_empty());
        // Neither a version nor a download address can be anything.
        assert!(parse_update_check(
            r#"<app appid="x"><updatecheck status="ok" codebase="file:///c:/x.crx" version="1.0"/></app>"#
        )
        .is_empty());
        assert!(parse_update_check(
            r#"<app appid="x"><updatecheck status="ok" codebase="https://x/y.crx" version="1.0-beta"/></app>"#
        )
        .is_empty());
    }

    #[test]
    fn an_attribute_is_matched_by_its_whole_name() {
        assert_eq!(
            attribute(r#" fakeversion="9" version="1""#, "version").as_deref(),
            Some("1")
        );
        assert_eq!(attribute(r#" fakeversion="9""#, "version"), None);
    }

    #[test]
    fn versions_compare_number_by_number() {
        assert_eq!(compare_versions("1.10", "1.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0", "1"), Ordering::Equal);
        assert_eq!(compare_versions("2026.1110.1816", "2026.1113.76.0"), Ordering::Less);
        assert!(is_version("1.2.3.4"));
        assert!(!is_version("1.2.3.4.5"));
        assert!(!is_version("1..2"));
        assert!(!is_version(""));
    }
}
