//! Native window operations Tauri does not expose.
//!
//! Tauri v2 has no z-order API, and each child webview is a native view that
//! composites above the window's own content. Haku needs the opposite: the
//! chrome must sit above every page so the viewport can have rounded corners
//! and menus can overlap page content. That requires per-platform code, which
//! lives here behind one interface, along with the other native operations
//! Haku needs: observing pages, answering their dialogs, stopping idle
//! service workers, and measuring memory.
//!
//! Windows is implemented. The other targets fail with
//! [`HakuError::Unsupported`] rather than silently doing nothing, so a missing
//! platform surfaces as a visible error instead of a subtly broken window.

use std::sync::{Arc, LazyLock, Mutex};

use crate::error::{HakuError, Result};
use crate::model::{Commit, DialogAnswer, DialogId, MemoryStatus, PageDialog, SlotId};

use memory::{Attribution, SlotMemoryTracker};

pub mod memory;
pub mod workers;

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

/// A region of the input mask, rounded when `radius` is above zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundedRect {
    pub rect: PhysicalRect,
    pub radius: i32,
}

/// What a content webview reported about its own navigation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageSignal {
    /// The page changed URL, title, or both. `commits` are in the order the
    /// page made them and `title` is the document title after all of them.
    Changed {
        commits: Vec<Commit>,
        title: Option<String>,
    },
    /// The webview's own back or forward was requested, and stopped.
    ///
    /// A webview's native history spans every tab that has used its slot, so
    /// following it could land on another tab's page. The request is handed to
    /// the tab's own history instead.
    TraverseRequested { url: String },
    /// The page opened `alert`, `confirm`, `prompt` or a "Leave site?"
    /// dialog and is paused until [`answer_dialog`] is called with its id.
    DialogRequested(PageDialog),
    /// The page started or stopped playing audio.
    AudioChanged { playing: bool },
    /// A navigation of the top frame started. `form` is set when it carries a
    /// form submission, which makes the page it loads the user's work.
    NavigationStarted { form: bool },
    /// The document reached `DOMContentLoaded`, on `url`.
    Loaded { url: String },
}

/// Receives [`PageSignal`]s. Called on the UI thread, so it must not block on
/// anything that dispatches back to it.
pub type PageSink = Arc<dyn Fn(PageSignal) + Send + Sync>;

/// Starts reporting a content webview's navigation to `sink`.
///
/// Must be attached before the webview loads anything it should report, which
/// is why content webviews are created on a blank page and navigated after.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn observe_page<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: PageSink) -> Result<()> {
    backend::observe_page(webview, sink)
}

/// Evaluates `expression` in a webview's page and returns the result as JSON.
///
/// Blocks for at most `timeout`, so it must not be called on the UI thread,
/// where the evaluation itself has to run.
///
/// @returns The result, or nothing when the evaluation failed or did not
///   finish in time, or the platform cannot evaluate.
pub fn evaluate<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    expression: &str,
    timeout: std::time::Duration,
) -> Option<String> {
    backend::evaluate(webview, expression, timeout)
}

/// Evaluates `expression` in a webview's page and, when `capture` is set,
/// captures what the webview shows as a JPEG, waiting for both together for
/// at most `timeout`.
///
/// A capture is only meaningful while the webview is visible. Blocks, so it
/// must not be called on the UI thread, where both have to run.
///
/// @returns The evaluation's JSON result and the image, each nothing when it
///   failed, did not finish in time, or was not asked for.
pub fn leave_page<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    expression: &str,
    capture: bool,
    timeout: std::time::Duration,
) -> (Option<String>, Option<Vec<u8>>) {
    backend::leave_page(webview, expression, capture, timeout)
}

/// Releases a page paused on a dialog, with the given answer.
///
/// Does nothing for a dialog that was already answered or whose webview is
/// gone. Safe to call from any thread: the answer is delivered on the UI thread,
/// which is the only one allowed to touch the paused dialog.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn answer_dialog<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: DialogId, answer: DialogAnswer) -> Result<()> {
    backend::answer_dialog(app, id, answer)
}

/// Stops service workers that keep running after every page using them is gone.
///
/// Attached once, to the chrome webview: it shares the browser profile with
/// every content webview, so it sees every page's workers. See [`workers`] for
/// why this is needed and how a worker is judged idle.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn stop_idle_workers<R: tauri::Runtime>(chrome: &tauri::Webview<R>) -> Result<()> {
    backend::stop_idle_workers(chrome)
}

/// Pauses a hidden content webview's page and lowers its memory target.
///
/// The engine may decline, for example while the webview is visible; the page
/// then keeps running, which costs memory but loses nothing.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn freeze<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    backend::freeze(webview)
}

/// Undoes [`freeze`]. Harmless on a webview that is not frozen.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn resume<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    backend::resume(webview)
}

/// The system's physical memory and commit figures, or nothing when they
/// cannot be read.
pub fn memory_status() -> Option<MemoryStatus> {
    backend::memory_status()
}

/// How much memory each slot's page holds, and what no slot accounts for.
///
/// Attribution remembers what it saw last time, so it is kept here for the
/// life of the process; see [`SlotMemoryTracker`].
///
/// @param chrome - The interface's webview, which shares the engine with every
/// slot and so can list its processes.
/// @param slots - Every slot's webview.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation,
/// and [`HakuError::WindowMissing`] when the engine cannot be reached.
pub fn slot_memory<R: tauri::Runtime>(
    chrome: &tauri::Webview<R>,
    slots: &[(SlotId, tauri::Webview<R>)],
) -> Result<Attribution> {
    static TRACKER: LazyLock<Mutex<SlotMemoryTracker>> = LazyLock::new(Mutex::default);

    let snapshot = backend::engine_processes(chrome, slots)?;
    let mut tracker = TRACKER
        .lock()
        .map_err(|_| HakuError::Storage("memory tracker lock poisoned".into()))?;
    Ok(tracker.attribute(&snapshot.frames, &snapshot.processes))
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
    overlays: &[RoundedRect],
) -> Result<()> {
    backend::set_input_mask(webview, viewport, radius, overlays)
}
