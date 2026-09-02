//! Native window operations Tauri does not expose.
//!
//! Tauri v2 has no z-order API, and each child webview is a native view that
//! composites above the window's own content. Haku needs the opposite: the
//! chrome must sit above every page so the viewport can have rounded corners
//! and menus can overlap page content. That requires per-platform code, which
//! lives here behind one interface.
//!
//! Windows is implemented. The other targets fail with
//! [`HakuError::Unsupported`] rather than silently doing nothing, so a missing
//! platform surfaces as a visible error instead of a subtly broken window.

use crate::error::Result;

#[cfg(windows)]
mod windows_impl;

#[cfg(not(windows))]
mod unsupported;

#[cfg(windows)]
use windows_impl as backend;

#[cfg(not(windows))]
use unsupported as backend;

/// A rectangle in physical (device) pixels, relative to the window client area.
///
/// Physical rather than logical because the native region APIs this feeds are
/// defined in device pixels; converting once at the boundary keeps the scale
/// factor out of the platform code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl PhysicalRect {
    pub fn right(self) -> i32 {
        self.x + self.width
    }

    pub fn bottom(self) -> i32 {
        self.y + self.height
    }
}

/// Raises the chrome webview above every content webview in its window.
///
/// Must be re-applied after any content webview is created: the platform places
/// a newly created child view on top.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation,
/// and [`HakuError::WindowMissing`] when the native handle cannot be resolved.
pub fn raise_chrome<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    backend::raise_chrome(webview)
}

/// Restricts which parts of the chrome webview receive input.
///
/// The chrome covers the whole window, so without this it would swallow every
/// click meant for the page. The mask removes `viewport` from the chrome's
/// input area, letting clicks, scrolling and hover reach the content webview
/// underneath, then adds `overlays` back for anything currently drawn above the
/// page, such as an open menu.
///
/// Passing no viewport makes the chrome solid again, which is what an internal
/// page or a fully covering overlay wants.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn set_input_mask<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    viewport: Option<PhysicalRect>,
    radius: i32,
    overlays: &[PhysicalRect],
) -> Result<()> {
    backend::set_input_mask(webview, viewport, radius, overlays)
}
