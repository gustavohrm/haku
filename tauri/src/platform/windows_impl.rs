//! Windows implementation of the chrome compositing operations.
//!
//! wry gives every webview its own container HWND parented to the window, and
//! places each new one at the top of the z-order. Tauri exposes no way to
//! reorder them, but it does hand out the underlying `ICoreWebView2Controller`,
//! whose `ParentWindow` is exactly that container. From there both operations
//! are ordinary Win32.

use std::sync::mpsc;
use std::time::Duration;

use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HRGN, RGN_DIFF, RGN_OR,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
};

use super::PhysicalRect;
use crate::error::{HakuError, Result};

/// How long to wait for the main thread to run a native operation.
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs `action` against the container HWND of `webview`, on the UI thread.
///
/// `with_webview` dispatches to the event loop, so the result comes back over a
/// channel. Tauri runs commands off the main thread, which is what makes
/// blocking here safe.
fn with_hwnd<R, F>(webview: &tauri::Webview<R>, action: F) -> Result<()>
where
    R: tauri::Runtime,
    F: FnOnce(HWND) -> Result<()> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();

    webview
        .with_webview(move |platform| {
            let outcome = unsafe {
                let controller: ICoreWebView2Controller = platform.controller();
                let mut hwnd = HWND::default();
                match controller.ParentWindow(&mut hwnd) {
                    Ok(()) => action(hwnd),
                    Err(error) => Err(HakuError::WindowMissing(error.to_string())),
                }
            };
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

pub fn raise_chrome<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    with_hwnd(webview, |hwnd| unsafe {
        SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
            .map_err(|error| HakuError::WindowMissing(error.to_string()))
    })
}

pub fn set_input_mask<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    viewport: Option<PhysicalRect>,
    radius: i32,
    overlays: &[PhysicalRect],
) -> Result<()> {
    let overlays = overlays.to_vec();

    with_hwnd(webview, move |hwnd| unsafe {
        // No viewport means nothing should show through: drop the region so the
        // chrome is a whole window again.
        let Some(viewport) = viewport else {
            SetWindowRgn(hwnd, None, true);
            return Ok(());
        };

        let mut client = Default::default();
        GetClientRect(hwnd, &mut client).map_err(|error| HakuError::WindowMissing(error.to_string()))?;

        let region = CreateRectRgn(0, 0, client.right, client.bottom);
        // A rounded hole is what gives the page rounded corners: the chrome
        // keeps painting the corner, and the page shows through the curve.
        // CreateRoundRectRgn takes the full width and height of the ellipse.
        let hole = if radius > 0 {
            CreateRoundRectRgn(
                viewport.x,
                viewport.y,
                viewport.right(),
                viewport.bottom(),
                radius * 2,
                radius * 2,
            )
        } else {
            CreateRectRgn(viewport.x, viewport.y, viewport.right(), viewport.bottom())
        };
        CombineRgn(Some(region), Some(region), Some(hole), RGN_DIFF);
        let _ = DeleteObject(hole.into());

        for overlay in &overlays {
            let patch = CreateRectRgn(overlay.x, overlay.y, overlay.right(), overlay.bottom());
            CombineRgn(Some(region), Some(region), Some(patch), RGN_OR);
            let _ = DeleteObject(patch.into());
        }

        // The window owns the region after this call; it must not be deleted.
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(HRGN::from(region).into());
            return Err(HakuError::WindowMissing("SetWindowRgn failed".into()));
        }
        Ok(())
    })
}
