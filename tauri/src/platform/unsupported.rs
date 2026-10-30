//! Placeholder backend for platforms whose native layer is not written yet.
//!
//! macOS would reorder with `addSubview:positioned:relativeTo:` on the
//! `WKWebView`, and mask with a `CAShapeLayer` on the chrome view. Linux would
//! restack the GTK child and shape it with `gdk_window_shape_combine_region`.
//! Both reach their handles through the same `Webview::with_webview` hook the
//! Windows backend uses.

use std::path::Path;

use super::memory::EngineSnapshot;
use super::{FailureSink, KeySink, PageSink, RoundedRect};
use crate::error::{HakuError, Result};
use crate::model::{ContextMenuRequestId, DialogAnswer, DialogId, MemoryStatus, SlotId, WindowRequestId};

fn unsupported(operation: &str) -> HakuError {
    HakuError::Unsupported(format!("{operation} is only implemented on Windows"))
}

pub fn raise_chrome<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("raising the chrome above content webviews"))
}

pub fn set_visible<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _visible: bool) -> Result<()> {
    Err(unsupported("showing content without activation"))
}

pub fn set_input_mask<R: tauri::Runtime>(
    _webview: &tauri::Webview<R>,
    _viewports: &[RoundedRect],
    _overlays: &[RoundedRect],
) -> Result<()> {
    Err(unsupported("masking chrome input"))
}

pub fn observe_page<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _sink: PageSink) -> Result<()> {
    Err(unsupported("observing page navigation"))
}

/// Nowhere native to say it yet; the caller quits either way.
pub fn show_fatal(_message: &str) {}

pub fn watch_failures<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _sink: FailureSink) -> Result<()> {
    Err(unsupported("watching process failures"))
}

pub fn allow_clipboard_read<R: tauri::Runtime>(_chrome: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("allowing clipboard reads"))
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

pub fn answer_context_menu<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _request: ContextMenuRequestId,
    _choice: Option<i32>,
) -> Result<()> {
    Err(unsupported("answering context menus"))
}

pub fn adopt_window<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _request: WindowRequestId) -> Result<bool> {
    Err(unsupported("opening new windows"))
}

pub fn refuse_window<R: tauri::Runtime>(_app: &tauri::AppHandle<R>, _request: WindowRequestId) -> Result<()> {
    Err(unsupported("opening new windows"))
}

pub fn observe_popup<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _sink: PageSink) -> Result<()> {
    Err(unsupported("observing an extension popup"))
}

pub fn engine_window<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _timeout: std::time::Duration) -> Option<String> {
    None
}

pub fn uninstall_extension<R: tauri::Runtime>(_chrome: &tauri::Webview<R>, _id: String) -> Result<()> {
    Err(unsupported("uninstalling extensions"))
}

pub fn is_private<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<bool> {
    Err(unsupported("verifying private mode"))
}

pub fn watch_private_workers<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _webview: Option<&tauri::Webview<R>>,
) -> Result<()> {
    Err(unsupported("watching private service workers"))
}

pub fn forget_page<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("releasing page observers"))
}
