use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::HakuError;

/// The toolbar draws an extension's icon at 16 CSS pixels; this is that size
/// doubled, for high-density screens. The icon chosen is the smallest at
/// least this large.
const ICON_SIZE: u64 = 32;

/// An installed extension, as the interface lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub id: String,
    pub name: String,
    /// The manifest's `version`, or empty when it declares none.
    pub version: String,
    /// The subfolder of the extensions folder it was installed from, which
    /// names it to `remove_extension`.
    pub folder: String,
    /// Its options page, as a `chrome-extension://` URL, when its manifest
    /// declares one.
    pub options: Option<String>,
    /// The page its toolbar button opens, as a `chrome-extension://` URL.
    pub popup: Option<String>,
    /// Its toolbar icon as a `data:` URL, read from the manifest. The engine
    /// does not report the icon an extension sets while it runs, so this is
    /// the one it declares.
    pub icon: Option<String>,
}

/// What the extensions folder holds, as last installed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
    pub installed: Vec<Extension>,
    /// Folders the engine would not install, so a broken or unsupported
    /// extension is not silently missing.
    pub failed: Vec<FailedExtension>,
    /// The first other failure while installing the folder, if any.
    pub error: Option<HakuError>,
}

/// A subfolder of the extensions folder that the engine would not install.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FailedExtension {
    pub folder: String,
    pub error: HakuError,
}

/// What `add_extension` asks the user to pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionSource {
    /// A packed extension: a `.crx` or `.zip` file.
    File,
    /// An unpacked extension: a folder holding its `manifest.json`.
    Folder,
}

/// An extension added from settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AddedExtension {
    pub extension: Extension,
    /// The version it replaced, when it updated one already installed.
    pub replaced: Option<String>,
}

/// The manifest's `version`, or empty when it declares none.
pub fn version(manifest: &str) -> String {
    parse(manifest)
        .and_then(|manifest| manifest.get("version")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The `chrome-extension://` URL of an extension's options page, from
/// `options_ui.page` or else `options_page`, or nothing when it declares none.
pub fn options_url(id: &str, manifest: &str) -> Option<String> {
    let manifest = parse(manifest)?;
    let page = manifest
        .get("options_ui")
        .and_then(|options| options.get("page"))
        .or_else(|| manifest.get("options_page"))?
        .as_str()?
        .trim_start_matches('/');
    (!page.is_empty()).then(|| format!("chrome-extension://{id}/{page}"))
}

/// The manifest's `key`, when it declares one.
pub fn key(manifest: &str) -> Option<String> {
    parse(manifest)?.get("key")?.as_str().map(str::to_string)
}

/// The locale whose messages give the extension's name, when it has one.
pub fn default_locale(manifest: &str) -> Option<String> {
    parse(manifest)?.get("default_locale")?.as_str().map(str::to_string)
}

/// The extension's name as a person reads it: the manifest's `name`, or, for
/// a `__MSG_<name>__` name, that message from `messages`, the default
/// locale's `messages.json`. Message names are matched without regard to
/// case, as Chrome matches them.
pub fn display_name(manifest: &str, messages: Option<&str>) -> Option<String> {
    let name = parse(manifest)?.get("name")?.as_str()?.to_string();
    let Some(message) = name.strip_prefix("__MSG_").and_then(|rest| rest.strip_suffix("__")) else {
        return Some(name);
    };
    parse(messages?)?
        .as_object()?
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(message))
        .and_then(|(_, entry)| entry.get("message")?.as_str().map(str::to_string))
}

/// The folder an extension named `name` is kept in: the name in lower case,
/// with every run of characters other than letters and digits made one `-`.
/// A name with no letter or digit gives `extension`.
pub fn folder_name(name: &str) -> String {
    let mut folder = String::new();
    for part in name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
    {
        if !folder.is_empty() {
            folder.push('-');
        }
        folder.extend(part.chars().flat_map(char::to_lowercase));
    }
    if folder.is_empty() {
        folder.push_str("extension");
    }
    folder
}

/// The id Chrome gives an extension whose manifest `key` decodes to
/// `public_key`: the first 16 bytes of its SHA-256, each half-byte written as
/// a letter from `a` to `p`.
pub fn id_from_key(public_key: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(public_key)[..16]
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0xf])
        .map(|half| char::from(b'a' + half))
        .collect()
}

fn parse(manifest: &str) -> Option<serde_json::Value> {
    serde_json::from_str(manifest.trim_start_matches('\u{feff}')).ok()
}

fn action(manifest: &serde_json::Value) -> Option<&serde_json::Value> {
    manifest.get("action").or_else(|| manifest.get("browser_action"))
}

/// The `chrome-extension://` URL of an extension's toolbar popup, read from
/// its manifest, or nothing when it declares none.
pub fn popup_url(id: &str, manifest: &str) -> Option<String> {
    let manifest = parse(manifest)?;
    let page = action(&manifest)?
        .get("default_popup")?
        .as_str()?
        .trim_start_matches('/');
    (!page.is_empty()).then(|| format!("chrome-extension://{id}/{page}"))
}

/// The file of an extension's toolbar icon, relative to its folder, and its
/// media type: the action's own icon, or the extension's when the action
/// declares none.
///
/// Of the sizes declared, the smallest that is at least [`ICON_SIZE`] is
/// chosen, or the largest when none is. A path that leaves the folder, or a
/// format an `<img>` cannot show, gives nothing.
pub fn icon_file(manifest: &str) -> Option<(String, &'static str)> {
    let manifest = parse(manifest)?;
    let declared = action(&manifest)
        .and_then(|action| action.get("default_icon"))
        .or_else(|| manifest.get("icons"))?;
    let path = match declared {
        serde_json::Value::String(path) => path.clone(),
        serde_json::Value::Object(sizes) => {
            let mut sizes: Vec<(u64, &str)> = sizes
                .iter()
                .filter_map(|(size, path)| Some((size.parse().ok()?, path.as_str()?)))
                .collect();
            sizes.sort_unstable();
            let (_, path) = sizes
                .iter()
                .find(|(size, _)| *size >= ICON_SIZE)
                .or_else(|| sizes.last())?;
            (*path).to_string()
        }
        _ => return None,
    };
    // Either separator: a path rooted at `\` would otherwise be read from the
    // drive's root on Windows.
    let path = path.trim_start_matches(['/', '\\']).to_string();
    if path.split(['/', '\\']).any(|part| part == "..") || path.contains(':') {
        return None;
    }
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    Some((path, mime))
}

/// The smallest an extension's toolbar popup is made, in logical pixels, as
/// in Chrome.
pub const POPUP_MIN_SIZE: (f64, f64) = (25.0, 25.0);

/// The largest an extension's toolbar popup is made, in logical pixels, as in
/// Chrome. A page larger than this scrolls.
pub const POPUP_MAX_SIZE: (f64, f64) = (800.0, 600.0);

/// A rectangle on screen, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The size a toolbar popup is made for a page that lays out at `width` by
/// `height`, kept within [`POPUP_MIN_SIZE`] and [`POPUP_MAX_SIZE`].
pub fn popup_size(width: f64, height: f64) -> (f64, f64) {
    (
        width.ceil().clamp(POPUP_MIN_SIZE.0, POPUP_MAX_SIZE.0),
        height.ceil().clamp(POPUP_MIN_SIZE.1, POPUP_MAX_SIZE.1),
    )
}

/// Where a toolbar popup of `size` goes: just below the button it was opened
/// from, its right edge on the button's, and moved as little as it takes to
/// stay inside the screen's work area.
pub fn popup_origin(anchor: ScreenRect, size: (f64, f64), area: ScreenRect) -> (f64, f64) {
    let (width, height) = size;
    let x = (anchor.x + anchor.width - width)
        .min(area.x + area.width - width)
        .max(area.x);
    let y = (anchor.y + anchor.height)
        .min(area.y + area.height - height)
        .max(area.y);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_popup_comes_from_the_manifest_action() {
        let manifest = r#"{"manifest_version": 3, "action": {"default_popup": "popup/index.html"}}"#;
        assert_eq!(
            popup_url("abc", manifest).as_deref(),
            Some("chrome-extension://abc/popup/index.html")
        );
    }

    #[test]
    fn the_version_and_options_come_from_the_manifest() {
        let manifest = r#"{"version": "1.2.3", "options_ui": {"page": "/options.html"}, "options_page": "old.html"}"#;
        assert_eq!(version(manifest), "1.2.3");
        assert_eq!(
            options_url("abc", manifest).as_deref(),
            Some("chrome-extension://abc/options.html")
        );
        assert_eq!(
            options_url("abc", r#"{"options_page": "old.html"}"#).as_deref(),
            Some("chrome-extension://abc/old.html")
        );
        assert_eq!(options_url("abc", "{}"), None);
        assert_eq!(version("{}"), "");
    }

    #[test]
    fn a_localized_name_is_read_from_the_default_locale() {
        let manifest = r#"{"name": "__MSG_extName__", "default_locale": "en"}"#;
        let messages = r#"{"extname": {"message": "Bitwarden Password Manager"}}"#;
        assert_eq!(
            display_name(manifest, Some(messages)).as_deref(),
            Some("Bitwarden Password Manager")
        );
        assert_eq!(display_name(manifest, None), None);
        assert_eq!(
            display_name(r#"{"name": "uBlock Origin Lite"}"#, None).as_deref(),
            Some("uBlock Origin Lite")
        );
    }

    #[test]
    fn a_folder_is_named_after_the_extension() {
        assert_eq!(folder_name("uBlock Origin Lite"), "ublock-origin-lite");
        assert_eq!(
            folder_name("  Bitwarden: Password Manager! "),
            "bitwarden-password-manager"
        );
        assert_eq!(folder_name("../.."), "extension");
    }

    #[test]
    fn an_id_follows_from_its_key_as_in_chrome() {
        // SHA-256 of nothing starts e3 b0 c4 42 98 fc 1c 14 9a fb f4 c8 99 6f b9 24.
        assert_eq!(id_from_key(b""), "odlameecjipmbmbejkplpemijjgpljce");
    }

    #[test]
    fn an_extension_without_a_popup_has_none() {
        assert_eq!(popup_url("abc", r#"{"action": {}}"#), None);
        assert_eq!(popup_url("abc", "not json"), None);
    }

    const AREA: ScreenRect = ScreenRect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1040.0,
    };

    #[test]
    fn a_popup_is_sized_to_its_page_within_the_limits() {
        assert_eq!(popup_size(379.2, 600.0), (380.0, 600.0));
        assert_eq!(popup_size(4.0, 2000.0), (25.0, 600.0));
        assert_eq!(popup_size(1200.0, 10.0), (800.0, 25.0));
    }

    #[test]
    fn a_popup_hangs_below_its_button_with_their_right_edges_lined_up() {
        let anchor = ScreenRect {
            x: 1700.0,
            y: 40.0,
            width: 28.0,
            height: 28.0,
        };
        assert_eq!(popup_origin(anchor, (380.0, 600.0), AREA), (1348.0, 68.0));
    }

    #[test]
    fn a_popup_stays_inside_the_work_area() {
        let near_left = ScreenRect {
            x: 10.0,
            y: 1000.0,
            width: 28.0,
            height: 28.0,
        };
        assert_eq!(popup_origin(near_left, (380.0, 600.0), AREA), (0.0, 440.0));
    }

    #[test]
    fn the_icon_is_the_smallest_action_icon_large_enough() {
        let manifest = r#"{"action": {"default_icon": {"16": "a.png", "64": "c.png", "32": "b.png"}}}"#;
        assert_eq!(icon_file(manifest), Some(("b.png".into(), "image/png")));
    }

    #[test]
    fn the_icon_is_the_largest_when_none_is_large_enough() {
        let manifest = r#"{"action": {"default_icon": {"19": "images/icon19.png", "16": "x.png"}}}"#;
        assert_eq!(icon_file(manifest), Some(("images/icon19.png".into(), "image/png")));
    }

    #[test]
    fn the_icon_falls_back_to_the_extension_icons() {
        let manifest = r#"{"action": {"default_popup": "p.html"}, "icons": {"128": "big.png", "48": "mid.png"}}"#;
        assert_eq!(icon_file(manifest), Some(("mid.png".into(), "image/png")));
    }

    #[test]
    fn a_single_icon_path_is_used_as_it_is() {
        let manifest = r#"{"browser_action": {"default_icon": "/icon.svg"}}"#;
        assert_eq!(icon_file(manifest), Some(("icon.svg".into(), "image/svg+xml")));
    }

    #[test]
    fn an_icon_path_rooted_at_either_separator_stays_in_the_folder() {
        assert_eq!(
            icon_file(r#"{"icons": {"32": "\\Users\\me\\photo.png"}}"#),
            Some(("Users\\me\\photo.png".into(), "image/png"))
        );
        assert_eq!(
            icon_file(r#"{"icons": {"32": "\\\\server\\share\\a.png"}}"#),
            Some(("server\\share\\a.png".into(), "image/png"))
        );
    }

    #[test]
    fn an_icon_outside_the_folder_or_in_another_format_gives_nothing() {
        assert_eq!(icon_file(r#"{"icons": {"32": "../secret.png"}}"#), None);
        assert_eq!(icon_file(r#"{"icons": {"32": "C:/secret.png"}}"#), None);
        assert_eq!(icon_file(r#"{"icons": {"32": "icon.exe"}}"#), None);
        assert_eq!(icon_file(r#"{"name": "no icons"}"#), None);
    }
}
