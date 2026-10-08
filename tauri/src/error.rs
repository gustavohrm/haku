use serde::{Deserialize, Serialize};
use specta::Type;

/// Every failure Haku exposes across the IPC boundary.
///
/// The variants are deliberately coarse: the frontend decides what to show the
/// user, and a stable, small set keeps the generated TypeScript union usable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum HakuError {
    #[error("tab not found: {0}")]
    TabNotFound(String),

    #[error("no webview slot is available")]
    NoSlotAvailable,

    #[error("invalid url: {0}")]
    InvalidUrl(String),

    #[error("the platform does not support this operation: {0}")]
    Unsupported(String),

    #[error("window or webview is missing: {0}")]
    WindowMissing(String),

    #[error("storage failure: {0}")]
    Storage(String),

    #[error("tauri failure: {0}")]
    Tauri(String),
}

impl From<tauri::Error> for HakuError {
    fn from(error: tauri::Error) -> Self {
        Self::Tauri(error.to_string())
    }
}

impl From<rusqlite::Error> for HakuError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<std::io::Error> for HakuError {
    fn from(error: std::io::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, HakuError>;
