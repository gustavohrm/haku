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
    /// Folders the engine would not install, by name, so a broken or
    /// unsupported extension is not silently missing.
    pub failed: Vec<String>,
    /// The first other failure while installing the folder, if any.
    pub error: Option<HakuError>,
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
