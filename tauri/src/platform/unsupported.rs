//! Placeholder backend for platforms whose native layer is not written yet.
//!
//! macOS would reorder with `addSubview:positioned:relativeTo:` on the
//! `WKWebView`, and mask with a `CAShapeLayer` on the chrome view. Linux would
//! restack the GTK child and shape it with `gdk_window_shape_combine_region`.
//! Both reach their handles through the same `Webview::with_webview` hook the
//! Windows backend uses.

use super::{PageSink, PhysicalRect, RoundedRect};
use crate::error::{HakuError, Result};
use crate::model::{DialogAnswer, DialogId};

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

pub fn stop_idle_workers<R: tauri::Runtime>(_chrome: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("stopping idle service workers"))
}

pub fn freeze<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("freezing background pages"))
}

pub fn resume<R: tauri::Runtime>(_webview: &tauri::Webview<R>) -> Result<()> {
    Err(unsupported("resuming frozen pages"))
}

pub fn total_memory() -> Option<u64> {
    None
}

pub fn memory_is_low() -> bool {
    false
}

pub fn answer_dialog<R: tauri::Runtime>(_app: &tauri::AppHandle<R>, _id: DialogId, _answer: DialogAnswer) -> Result<()> {
    Err(unsupported("answering page dialogs"))
}
