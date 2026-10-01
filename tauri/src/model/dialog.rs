use serde::{Deserialize, Serialize};
use specta::Type;

/// Identifies one dialog a page opened, so a late answer cannot reach a newer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct DialogId(#[specta(type = specta_typescript::Number)] pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DialogKind {
    Alert,
    Confirm,
    Prompt,
    /// "Leave site?", raised by a page's `beforeunload` handler.
    BeforeUnload,
}

/// A dialog a page opened with `alert`, `confirm`, `prompt` or `beforeunload`.
///
/// The page is paused until it is answered. Haku draws it itself instead of
/// letting the webview show its native one, so it matches the interface and
/// always names the site that asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PageDialog {
    pub id: DialogId,
    pub kind: DialogKind,
    pub message: String,
    /// Initial text of a prompt's field.
    pub default_text: String,
    /// The page that asked, so the interface can say who is asking.
    pub url: String,
}

impl PageDialog {
    /// The answer given when the page goes away before anyone answers.
    ///
    /// Leaving is what was happening anyway, so a "Leave site?" is accepted;
    /// anything else is dismissed, as closing the dialog would.
    pub fn abandoned(&self) -> DialogAnswer {
        match self.kind {
            DialogKind::BeforeUnload => DialogAnswer::Accept { text: None },
            _ => DialogAnswer::Dismiss,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum DialogAnswer {
    /// OK, or Leave. `text` is what a prompt's field held.
    Accept { text: Option<String> },
    /// Cancel, Stay, or closing the dialog.
    Dismiss,
}
