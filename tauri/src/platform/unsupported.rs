//! Placeholder backend for platforms whose native layer is not written yet.
//!
//! macOS would reorder with `addSubview:positioned:relativeTo:` on the
//! `WKWebView`, and mask with a `CAShapeLayer` on the chrome view. Linux would
//! restack the GTK child and shape it with `gdk_window_shape_combine_region`.
//! Both reach their handles through the same `Webview::with_webview` hook the
//! Windows backend uses.

use super::PhysicalRect;
use crate::error::{HakuError, Result};

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
    _overlays: &[PhysicalRect],
) -> Result<()> {
    Err(unsupported("masking chrome input"))
}
