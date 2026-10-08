//! Placeholder backend for platforms whose native layer is not written yet.
//!
//! macOS would reorder with `addSubview:positioned:relativeTo:` on the
//! `WKWebView`, and mask with a `CAShapeLayer` on the chrome view. Linux would
//! restack the GTK child and shape it with `gdk_window_shape_combine_region`.
//! Both reach their handles through the same `Webview::with_webview` hook the
//! Windows backend uses.

use std::path::Path;

use super::memory::EngineSnapshot;
use super::{KeySink, PageSink, PhysicalRect, RoundedRect};
use crate::error::{HakuError, Result};
use crate::model::{DialogAnswer, DialogId, MemoryStatus, SlotId, WindowRequestId};

fn unsupported(operation: &str) -> HakuError {
    HakuError::Unsupported(format!("{operation} is only implemented on Windows"))
}

pub fn raise_chrome<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("raising the chrome above content webviews"))
}

pub fn set_input_mask<R: tauri::Runtime>(
    _webview: &tauri::Webview<R>,
    _viewport: Option<PhysicalRect>,
    _radius: i32,
    _overlays: &[RoundedRect],
) -> Result<()> {
    Err(unsupported("masking chrome input"))
}

pub fn observe_page<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _sink: PageSink) -> Result<()> {
    Err(unsupported("observing page navigation"))
}

pub fn intercept_keys<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _sink: KeySink) -> Result<()> {
    Err(unsupported("intercepting shortcut keys"))
}

pub fn install_extension<R: tauri::Runtime>(_chrome: &tauri::Webview<R>, _folder: &Path) -> Result<(String, String)> {
    Err(unsupported("installing extensions"))
}

pub fn set_extension_enabled<R: tauri::Runtime>(
    _chrome: &tauri::Webview<R>,
    _id: String,
    _enabled: bool,
) -> Result<()> {
    Err(unsupported("switching extensions"))
}

pub fn reveal_folder(_folder: &Path) -> Result<()> {
    Err(unsupported("opening a folder"))
}

pub fn stop_idle_workers<R: tauri::Runtime>(_chrome: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("stopping idle service workers"))
}

pub fn freeze<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("freezing background pages"))
}

pub fn resume<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("resuming frozen pages"))
}

pub fn evaluate<R: tauri::Runtime>(
    _webview: &tauri::Webview<R>,
    _expression: &str,
    _timeout: std::time::Duration,
) -> Option<String> {
    None
}

pub fn leave_page<R: tauri::Runtime>(
    _webview: &tauri::Webview<R>,
    _expression: &str,
    _capture: bool,
    _timeout: std::time::Duration,
) -> (Option<String>, Option<Vec<u8>>) {
    (None, None)
}

pub fn memory_status() -> Option<MemoryStatus> {
    None
}

pub fn engine_processes<R: tauri::Runtime>(
    _chrome: &tauri::Webview<R>,
    _slots: &[(SlotId, tauri::Webview<R>)],
) -> Result<EngineSnapshot> {
    Err(unsupported("measuring page memory"))
}

pub fn answer_dialog<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _id: DialogId,
    _answer: DialogAnswer,
) -> Result<()> {
    Err(unsupported("answering page dialogs"))
}

pub fn adopt_window<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _request: WindowRequestId) -> Result<bool> {
    Err(unsupported("opening new windows"))
}

pub fn refuse_window<R: tauri::Runtime>(_app: &tauri::AppHandle<R>, _request: WindowRequestId) -> Result<()> {
    Err(unsupported("opening new windows"))
}
