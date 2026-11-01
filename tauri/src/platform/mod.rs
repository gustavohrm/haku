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

use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use crate::error::{HakuError, Result};
use crate::model::{
    Commit, ContextMenuRequestId, CrashCause, DialogAnswer, DialogId, DownloadId, FindDirection, MemoryStatus,
    PageDialog, PageMenu, PermissionRequest, PermissionRequestId, Placement, Settled, Shortcut, SlotId,
    WindowRequestId,
};

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
///
/// Kept in exact device pixels rather than whole ones: where a fractional edge
/// falls decides which pixels its curve covers, and rounding the bounds first
/// moves the curve by up to half a pixel, a different way on each side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedRect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub radius: f64,
}

/// Which pixels along its curve and edges a shape takes.
///
/// A region's edge is hard, while CSS antialiases the one it draws: the pixels
/// it only partly covers are blended with what the chrome paints behind them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Pixels whose centre lies inside, so the hard edge follows the middle of
    /// the antialiased one. Right for a hole, where the chrome paints outside
    /// the curve and blends its corner with its own colour.
    Centre,
    /// Only pixels lying wholly inside. Right for an overlay, which the chrome
    /// paints over its own background, not the page: a pixel it partly covers
    /// blends with that background, which shows as a dark fringe over a light
    /// page. Leaving those to the page gives a clean, if hard, edge.
    Whole,
}

impl RoundedRect {
    /// The pixels it covers within `within`, as rows of rectangles.
    ///
    /// Only rows inside `within` are worked out, so a shape far larger than
    /// the window costs no more than one that fills it.
    ///
    /// Worked out from the curve CSS draws, on every side alike. GDI's own
    /// rounded region steps unevenly along the curve and cuts up to two pixels
    /// inside it, which shows as a jagged corner.
    pub fn spans(&self, edge: Edge, within: PhysicalRect) -> Vec<PhysicalRect> {
        // CSS reduces a radius too large for the box to half its shorter side.
        let radius = self
            .radius
            .max(0.0)
            .min((self.right - self.left) / 2.0)
            .min((self.bottom - self.top) / 2.0);
        // How far in the curve is at a height, and where a row of pixels meets it.
        let inset = |y: f64| {
            let rise = (self.top + radius - y).max(y - (self.bottom - radius)).max(0.0);
            radius - (radius * radius - rise * rise).max(0.0).sqrt()
        };
        let rows = match edge {
            Edge::Centre => pixel_at(self.top)..pixel_at(self.bottom),
            Edge::Whole => self.top.ceil() as i32..self.bottom.floor() as i32,
        };
        let rows = rows.start.max(within.y)..rows.end.min(within.bottom());
        let mut spans: Vec<PhysicalRect> = Vec::new();
        for y in rows {
            let (x, right) = match edge {
                Edge::Centre => {
                    let inset = inset(f64::from(y) + 0.5);
                    (pixel_at(self.left + inset), pixel_at(self.right - inset))
                }
                Edge::Whole => {
                    // Of the row's top and bottom, the one nearer the curve's
                    // end is cut deeper.
                    let inset = inset(f64::from(y)).max(inset(f64::from(y) + 1.0));
                    ((self.left + inset).ceil() as i32, (self.right - inset).floor() as i32)
                }
            };
            let (x, right) = (x.max(within.x), right.min(within.right()));
            if right <= x {
                continue;
            }
            // Rows that match the one above join it, so the straight part of
            // the shape is a single rectangle.
            match spans.last_mut() {
                Some(last) if last.x == x && last.right() == right && last.bottom() == y => last.height += 1,
                _ => spans.push(PhysicalRect {
                    x,
                    y,
                    width: right - x,
                    height: 1,
                }),
            }
        }
        spans
    }
}

/// The first pixel whose centre lies at or past `edge`.
fn pixel_at(edge: f64) -> i32 {
    (edge - 0.5).ceil() as i32
}

/// What a content webview reported about its own navigation.
#[derive(Clone, Debug, PartialEq)]
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
    /// The page asked for a permission Haku answers, and waits until
    /// [`answer_permission`] is called with its id. The page keeps running.
    PermissionRequested(PermissionRequest),
    /// The page started or stopped playing audio.
    AudioChanged { playing: bool },
    /// The page gained or lost a fullscreen element.
    FullscreenChanged { fullscreen: bool },
    /// A navigation of the top frame started. `form` is set when it carries a
    /// form submission, which makes the page it loads the user's work.
    NavigationStarted { form: bool },
    /// The document reached `DOMContentLoaded`, on `url`.
    Loaded { url: String },
    /// A browser shortcut was pressed while the page had focus. The page never
    /// saw the key.
    Shortcut(Shortcut),
    /// The native content webview gained keyboard focus.
    Focused,
    /// The navigation finished, after the document's `load`, on `url`. Not
    /// sent for a navigation cancelled by the next one, which says nothing
    /// about the page that replaced it.
    Completed { url: String },
    /// The page asked for a new window, by a link or by script. It waits until
    /// [`adopt_window`] hands it a webview or [`refuse_window`] answers it, and
    /// the engine opens no window of its own either way.
    WindowRequested {
        request: WindowRequestId,
        url: String,
        /// Asked for with a size or a position, as a popup is.
        popup: bool,
        /// The size and position asked for, for a popup.
        placement: Placement,
        /// Asked for with Ctrl held, as a link opened for later is.
        background: bool,
        /// Asked for with Shift held and not Ctrl, as a link opened in a new
        /// browser window is.
        window: bool,
        /// Asked for in answer to the user, such as a click, rather than by
        /// the page on its own.
        gesture: bool,
    },
    /// A page opened by script asked to close its window.
    CloseRequested,
    /// The page was right-clicked, and waits until [`answer_context_menu`]
    /// is called with the request's id. The engine shows no menu of its own.
    /// A WebView2 runtime too old for the event never sends this, and keeps
    /// showing the engine's menu.
    ContextMenuRequested(PageMenu),
    /// A process behind the webview exited.
    ProcessFailed(ProcessFailure),
    /// The page's find moved or recounted. `active` counts from 1, and is
    /// nothing while nothing matches.
    FindChanged { matches: u32, active: Option<u32> },
}

/// A process behind a webview exited, of the kinds Haku answers.
///
/// The engine restarts the others itself, or, for a frame's process, draws
/// the failure in the frame. [Crash recovery](../../../docs/specs/crash-recovery.md#what-fails).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessFailure {
    /// The webview's own page process: its page is gone.
    Page(CrashCause),
    /// The engine's browser process: every webview is gone.
    Engine,
}

/// Receives a webview's [`ProcessFailure`]s. Called on the UI thread, so it
/// must not block on anything that dispatches back to it.
pub type FailureSink = Arc<dyn Fn(ProcessFailure) + Send + Sync>;

/// Receives a failure no caller waits on, such as a dialog's that closed
/// after the call that opened it returned. Called on the UI thread.
pub type ReportFailure = Box<dyn Fn(HakuError) + Send + Sync>;

/// Receives [`PageSignal`]s. Called on the UI thread, so it must not block on
/// anything that dispatches back to it.
pub type PageSink = Arc<dyn Fn(PageSignal) + Send + Sync>;

/// A download a content webview started, as the engine first describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadStart {
    pub url: String,
    /// Where the engine writes the file: the profile's download folder,
    /// under a name it made unique.
    pub path: String,
    pub total: Option<u64>,
}

/// What the engine reports of a download after it started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadSignal {
    Progress {
        received: u64,
        total: Option<u64>,
    },
    Settled(Settled),
    /// A failed download Haku resumed runs again into a new file, which the
    /// engine names as it did the first.
    Restarted {
        path: String,
        total: Option<u64>,
    },
}

/// Lists a download a webview started, in the slot it ran in, and returns
/// the id it is listed under. Called on the UI thread, so it must not block
/// on anything that dispatches back to it.
pub type DownloadStartSink = Arc<dyn Fn(Option<SlotId>, DownloadStart) -> DownloadId + Send + Sync>;

/// Receives what the engine reports of a listed download. Called on the UI
/// thread, like [`DownloadStartSink`].
pub type DownloadSink = Arc<dyn Fn(DownloadId, DownloadSignal) + Send + Sync>;

/// What can be done to a running download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadAction {
    Pause,
    Resume,
    Cancel,
}

/// Receives the shortcuts pressed in a webview. Called on the UI thread, so it
/// must not block on anything that dispatches back to it.
pub type KeySink = Arc<dyn Fn(Shortcut) + Send + Sync>;

/// Reports the shortcuts pressed in the chrome to `sink`.
///
/// A key bound in [`crate::model::keymap`] is consumed natively, before the
/// webview acts on it, so it never reaches the interface or the engine's own
/// handling of the key. Content webviews report theirs as
/// [`PageSignal::Shortcut`] instead, attached by [`observe_page`], so a
/// shortcut works wherever focus is without the page taking part.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn intercept_keys<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: KeySink) -> Result<()> {
    backend::intercept_keys(webview, sink)
}

/// Reports a chrome webview's process failures to `sink`. Content webviews
/// report theirs as [`PageSignal::ProcessFailed`], attached by
/// [`observe_page`].
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn watch_failures<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: FailureSink) -> Result<()> {
    backend::watch_failures(webview, sink)
}

/// Lets the chrome read the clipboard without the engine asking the user,
/// so Paste in the interface's text fields works. Only the chrome is given
/// this: it runs Haku's own code, and pages keep the engine's prompt.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn allow_clipboard_read<R: tauri::Runtime>(chrome: &tauri::Webview<R>) -> Result<()> {
    backend::allow_clipboard_read(chrome)
}

/// Tells the user, in a native message box, about a failure Haku cannot show
/// in a window, and waits until it is dismissed.
pub fn show_fatal(message: &str) {
    backend::show_fatal(message);
}

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

/// Takes over every download content webviews start from here on, so the
/// engine shows none of its own download interface and Haku lists them
/// instead. Set once, before any content webview exists.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation,
/// and when downloads are already observed.
pub fn observe_downloads(started: DownloadStartSink, changed: DownloadSink) -> Result<()> {
    backend::observe_downloads(started, changed)
}

/// Pauses, resumes or cancels a download Haku follows.
///
/// Must not be called on the UI thread, where the download is reached.
///
/// @returns Whether the engine still had the download to act on. It does not
///   once the webview that ran it is gone.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation
/// or when the engine refuses the action on a download it still has, and
/// [`HakuError::WindowMissing`] when the UI thread does not answer.
pub fn control_download<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    id: DownloadId,
    action: DownloadAction,
) -> Result<bool> {
    backend::control_download(app, id, action)
}

/// Opens a downloaded file with the program the system associates with it.
///
/// Only for a path Haku recorded for a completed download, at the user's
/// request: a page never names what is opened.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when it cannot be opened.
pub fn open_file(path: &Path) -> Result<()> {
    backend::open_file(path)
}

/// Shows a file selected in its folder, in the system's file manager.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when it cannot be shown.
pub fn show_file(path: &Path) -> Result<()> {
    backend::show_file(path)
}

/// Shows or hides a content surface without activating it or moving focus.
/// Explicit focus is a separate operation.
///
/// # Errors
/// Propagates native failures; returns [`HakuError::Unsupported`] off Windows.
pub fn set_visible<R: tauri::Runtime>(webview: &tauri::Webview<R>, visible: bool) -> Result<()> {
    backend::set_visible(webview, visible)
}

/// Starts reporting an extension's toolbar popup asking for a new window or
/// to close itself, the only [`PageSignal`]s it sends: it is no tab, so
/// nothing else it does is followed, and its dialogs stay the engine's own.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn observe_popup<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: PageSink) -> Result<()> {
    backend::observe_popup(webview, sink)
}

/// The id the engine's extension APIs know a content webview's window by.
///
/// The engine treats every webview as a window of its own holding one tab,
/// so this is what an extension's popup must be told "the current window"
/// is, for the tab the user was on. Blocks for at most `timeout`, so it must
/// not be called on the UI thread.
///
/// @returns The id, or nothing when the engine did not say in time.
pub fn engine_window<R: tauri::Runtime>(webview: &tauri::Webview<R>, timeout: std::time::Duration) -> Option<i64> {
    parse_window_id(&backend::engine_window(webview, timeout)?)
}

/// The `windowId` of a DevTools `Browser.getWindowForTarget` answer.
fn parse_window_id(json: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()?
        .get("windowId")?
        .as_i64()
}

/// Finds `query` in a webview's page, highlighting every match, or moves to
/// the next or previous one. The count arrives as [`PageSignal::FindChanged`].
/// [Find, print and save](../../../docs/specs/find-print-save.md).
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the engine cannot find.
pub fn find<R: tauri::Runtime>(webview: &tauri::Webview<R>, query: &str, direction: FindDirection) -> Result<()> {
    backend::find(webview, query, direction)
}

/// Ends a webview's find and clears its highlights. Harmless with none.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the engine cannot find.
pub fn stop_find<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    backend::stop_find(webview)
}

/// Opens the browser's print preview for a webview's page, without waiting for
/// it. `failed` hears of a failure; cancelling is not one.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when the webview cannot be reached.
pub fn print<R: tauri::Runtime>(webview: &tauri::Webview<R>, failed: ReportFailure) -> Result<()> {
    backend::print(webview, failed)
}

/// Opens the engine's Save As dialog for a webview's page, without waiting
/// for it. `failed` hears of a failure; cancelling is not one.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when the webview cannot be reached.
pub fn save_page<R: tauri::Runtime>(webview: &tauri::Webview<R>, failed: ReportFailure) -> Result<()> {
    backend::save_page(webview, failed)
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

/// Grants or denies a permission a page asked for.
///
/// Does nothing for a request already answered or whose webview is gone.
/// Safe to call from any thread, like [`answer_dialog`].
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn answer_permission<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    id: PermissionRequestId,
    allow: bool,
) -> Result<()> {
    backend::answer_permission(app, id, allow)
}

/// Answers a page waiting on its context menu: runs the native item
/// `choice`, or nothing. A choice the page did not offer is taken as
/// nothing, and an answer to a request already answered does nothing. Safe
/// to call from any thread, like [`answer_dialog`].
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn answer_context_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: ContextMenuRequestId,
    choice: Option<i32>,
) -> Result<()> {
    backend::answer_context_menu(app, request, choice)
}

/// Hands a webview that has loaded nothing to the page waiting on `request`,
/// which loads the new window's page into it and stays connected to it
/// through `window.opener`.
///
/// Must not be called on the UI thread, where the handing over runs.
///
/// @returns Whether the webview was handed over. It is not when the request
///   was already answered or the engine refused the webview, and is then the
///   caller's to load.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation,
/// and [`HakuError::WindowMissing`] when the UI thread does not answer.
pub fn adopt_window<R: tauri::Runtime>(webview: &tauri::Webview<R>, request: WindowRequestId) -> Result<bool> {
    backend::adopt_window(webview, request)
}

/// Answers a page waiting on `request` without a webview, so the engine opens
/// nothing. Does nothing for a request already answered. Safe to call from any
/// thread, like [`answer_dialog`].
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn refuse_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>, request: WindowRequestId) -> Result<()> {
    backend::refuse_window(app, request)
}

/// Installs the unpacked Chrome extension in `folder` into the profile every
/// webview shares, or reinstalls it from there if it is installed already.
///
/// Blocks until the engine answers, so it must not be called on the UI thread.
///
/// @returns The extension's id, which follows from the folder's path, and
///   its display name.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the engine rejects the extension or
/// the platform has no implementation.
pub fn install_extension<R: tauri::Runtime>(chrome: &tauri::Webview<R>, folder: &Path) -> Result<(String, String)> {
    backend::install_extension(chrome, folder)
}

/// Switches an extension this process installed on or off, at once and in
/// every page. Does nothing for any other extension.
///
/// Blocks until the engine answers, so it must not be called on the UI thread.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the engine refuses, and on
/// platforms without an implementation.
pub fn set_extension_enabled<R: tauri::Runtime>(chrome: &tauri::Webview<R>, id: String, enabled: bool) -> Result<()> {
    backend::set_extension_enabled(chrome, id, enabled)
}

/// Uninstalls an extension from the profile every webview shares, which
/// deletes what it stored. Does nothing for an id that is not installed.
///
/// Blocks until the engine answers, so it must not be called on the UI thread.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the engine refuses, as it does for
/// its own built-in extensions, and on platforms without an implementation.
pub fn uninstall_extension<R: tauri::Runtime>(chrome: &tauri::Webview<R>, id: String) -> Result<()> {
    backend::uninstall_extension(chrome, id)
}

/// Asks the user for an extension with the system's own picker, owned by
/// `window`: a `.crx` or `.zip` file, or a folder. Answers nothing when the
/// picker is cancelled.
///
/// Blocks until the user answers, so it must not be called on the UI thread.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the picker cannot be shown, and on
/// platforms without an implementation.
pub fn pick_extension<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    source: crate::model::ExtensionSource,
) -> Result<Option<std::path::PathBuf>> {
    backend::pick_extension(window, source)
}

/// Opens a folder in the system's file manager.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when it cannot be opened.
pub fn reveal_folder(folder: &Path) -> Result<()> {
    backend::reveal_folder(folder)
}

/// Releases native requests and watcher handles before destroying a page.
///
/// # Errors
/// Returns a native dispatch failure.
pub fn forget_page<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    backend::forget_page(webview)
}

/// Checks the native profile before a private slot may navigate.
///
/// # Errors
/// Returns the platform failure when private mode cannot be verified.
pub fn is_private<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<bool> {
    backend::is_private(webview)
}

/// Moves the private worker watcher to a surviving private slot, or releases it.
///
/// # Errors
/// Returns a native dispatch or DevTools failure.
pub fn watch_private_workers<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    webview: Option<&tauri::Webview<R>>,
) -> Result<()> {
    backend::watch_private_workers(app, webview)
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
/// click meant for the page. The mask removes each of `viewports` from the chrome's
/// input area, letting clicks, scrolling and hover reach the content webview
/// underneath, then adds `overlays` back for anything currently drawn above the
/// page, such as an open menu.
///
/// Passing an empty viewport slice makes the chrome solid again, which is what an internal
/// page or a fully covering overlay wants.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] on platforms without an implementation.
pub fn set_input_mask<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    viewports: &[RoundedRect],
    overlays: &[RoundedRect],
) -> Result<()> {
    backend::set_input_mask(webview, viewports, overlays)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_id_is_read_from_the_engine_answer() {
        let answer = r#"{"windowId":506172816,"bounds":{"left":4,"top":65,"width":1912,"height":1011}}"#;
        assert_eq!(parse_window_id(answer), Some(506_172_816));
    }

    #[test]
    fn an_answer_without_a_window_id_gives_none() {
        assert_eq!(parse_window_id(r#"{"error":"no target"}"#), None);
        assert_eq!(parse_window_id("not json"), None);
    }

    const WINDOW: PhysicalRect = PhysicalRect {
        x: 0,
        y: 0,
        width: 1000,
        height: 1000,
    };

    fn shape(left: f64, top: f64, right: f64, bottom: f64, radius: f64) -> RoundedRect {
        RoundedRect {
            left,
            top,
            right,
            bottom,
            radius,
        }
    }

    /// Whether the point lies inside the shape as CSS draws it, worked out
    /// independently of `spans`.
    fn inside(shape: RoundedRect, x: f64, y: f64) -> bool {
        let radius = shape.radius;
        let cx = x.clamp(shape.left + radius, shape.right - radius);
        let cy = y.clamp(shape.top + radius, shape.bottom - radius);
        x >= shape.left
            && x < shape.right
            && y >= shape.top
            && y < shape.bottom
            && (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
    }

    /// Like `inside`, but counting the shape's own outline as inside.
    fn inside_or_on(shape: RoundedRect, x: f64, y: f64) -> bool {
        let radius = shape.radius;
        let cx = x.clamp(shape.left + radius, shape.right - radius);
        let cy = y.clamp(shape.top + radius, shape.bottom - radius);
        x >= shape.left
            && x <= shape.right
            && y >= shape.top
            && y <= shape.bottom
            && (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius + 1e-9
    }

    fn covered(spans: &[PhysicalRect], x: i32, y: i32) -> bool {
        spans
            .iter()
            .any(|span| x >= span.x && x < span.right() && y >= span.y && y < span.bottom())
    }

    #[test]
    fn a_square_shape_is_one_rectangle_whose_edges_round_where_they_lie() {
        // 10.4 + 10.4 ends at 20.8, so the far edges take pixel 20 as well.
        assert_eq!(
            shape(10.4, 10.4, 20.8, 20.8, 0.0).spans(Edge::Centre, WINDOW),
            vec![PhysicalRect {
                x: 10,
                y: 10,
                width: 11,
                height: 11
            }]
        );
    }

    #[test]
    fn a_rounded_shape_covers_exactly_the_pixels_whose_centres_lie_inside_its_curve() {
        // An overlay at a fractional position on a 125% display: GDI's own
        // rounded region cut this one deeper on the left than on the right.
        let shape = shape(12.875, 3.3, 263.625, 53.7, 10.0);
        let spans = shape.spans(Edge::Centre, WINDOW);

        for y in 0..60 {
            for x in 0..280 {
                let centre = inside(shape, f64::from(x) + 0.5, f64::from(y) + 0.5);
                assert_eq!(covered(&spans, x, y), centre, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn a_whole_edge_covers_exactly_the_pixels_lying_wholly_inside_the_curve() {
        // A toast over a light page: a pixel it only partly covers is painted
        // blended with the chrome behind it, which shows as a dark fringe.
        let shape = shape(12.875, 3.3, 263.625, 53.7, 10.0);
        let spans = shape.spans(Edge::Whole, WINDOW);

        for y in 0..60 {
            for x in 0..280 {
                let (left, top) = (f64::from(x), f64::from(y));
                // The shape is convex, so a pixel is inside when its corners are.
                let whole = [
                    (left, top),
                    (left + 1.0, top),
                    (left, top + 1.0),
                    (left + 1.0, top + 1.0),
                ]
                .iter()
                .all(|&(cx, cy)| inside_or_on(shape, cx, cy));
                assert_eq!(covered(&spans, x, y), whole, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn a_shape_far_larger_than_the_window_gives_only_the_rows_inside_it() {
        let window = PhysicalRect {
            x: 0,
            y: 0,
            width: 100,
            height: 80,
        };
        for edge in [Edge::Centre, Edge::Whole] {
            let spans = shape(-1e12, -1e12, 1e12, 1e12, 8.0).spans(edge, window);

            assert_eq!(spans, vec![window]);
        }
    }

    #[test]
    fn a_radius_beyond_half_the_shorter_side_is_reduced_as_css_does() {
        let shape = shape(0.0, 0.0, 40.0, 10.0, 20.0);
        let spans = shape.spans(Edge::Centre, WINDOW);

        assert!(covered(&spans, 20, 5));
        assert!(covered(&spans, 5, 5));
        assert!(!covered(&spans, 0, 0));
    }
}
