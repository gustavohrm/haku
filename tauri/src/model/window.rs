use serde::{Deserialize, Serialize};
use specta::Type;

/// Identifies one request a page made to open a new window. The engine holds
/// the page's request open until it is answered, with a webview or a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowRequestId(pub u64);

/// Prefix of a browser window's label, which its chrome webview shares.
///
/// The capability that gives the chrome its permissions names webviews by this
/// prefix, so content webviews must never be labelled with it.
pub const WINDOW_LABEL_PREFIX: &str = "window-";

/// Identifies a browser window for as long as it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
pub struct WindowId(#[specta(type = specta_typescript::Number)] pub u64);

impl WindowId {
    /// Label of the native window, and of the chrome webview that fills it.
    pub fn label(self) -> String {
        format!("{WINDOW_LABEL_PREFIX}{}", self.0)
    }

    /// Recovers a window from its label, or from its chrome's.
    ///
    /// A command is attributed to the window whose chrome called it by this,
    /// so anything that is not a chrome label must not parse.
    pub fn from_label(label: &str) -> Option<Self> {
        label.strip_prefix(WINDOW_LABEL_PREFIX)?.parse().ok().map(Self)
    }
}

/// What a window is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WindowKind {
    /// A browser window, with a tab strip and an address field.
    Normal,
    /// A window a page opened at a size or position of its choosing, such as
    /// a sign-in popup. It shows one page, with a read-only address, and is
    /// never given another tab.
    Popup,
}

/// Where a page asked for the window it opened to be, in CSS pixels on the
/// screen. Whatever the page left out is Haku's to choose.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    /// The window's top-left corner.
    pub position: Option<(f64, f64)>,
    /// The page's width and height, not counting the window's own bar.
    pub size: Option<(f64, f64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_label_round_trips() {
        assert_eq!(WindowId::from_label(&WindowId(3).label()), Some(WindowId(3)));
    }

    #[test]
    fn a_label_that_is_not_a_window_is_rejected() {
        assert_eq!(WindowId::from_label("content-3"), None);
        assert_eq!(WindowId::from_label("main"), None);
        assert_eq!(WindowId::from_label("window-"), None);
        assert_eq!(WindowId::from_label("window-x"), None);
    }
}
